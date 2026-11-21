//! The bytecode machine: fetch, decode, execute.
//!
//! `crate::eval` walks a tree and this runs instructions, and after phase 1
//! only one of them is ever running. The interpreter bootstraps the compiler
//! and then goes cold; everything a built image evaluates is compiled first.
//! So these two never share a stack, a scope or a call, and neither has to
//! know the other exists. See #25 for why that question was harder than it
//! looks, and `docs/bytecode.md` for the instruction set.
//!
//! # What makes this sliceable
//!
//! [`Vm::step`] performs **one instruction** and returns, and every scrap of
//! what it was doing — the program counter, the stack pointer, the function —
//! lives in the heap rather than in a Rust local. So the loop can stop
//! between any two instructions (rule 4), and an image saved there resumes.
//! That is chapter 2's constraint collecting on a bill phase 0 paid.

use crate::code::{self, function, CodeError, Op};
use crate::eval::{closure, env, EvalError};
use crate::heap::{symbol, Mutation, ObjectType, Register};
use crate::primitives;
use crate::slice::{Steppable, Transition};
use crate::value::Value;

/// How many slots a new operand stack starts with.
///
/// Grown by doubling when a frame needs more than is left. Small enough that
/// a trivial program does not pay for a large vector, large enough that
/// nothing realistic grows more than a few times.
const INITIAL_STACK: usize = 64;

/// How many words a call frame's header takes: the saved program counter,
/// frame pointer, function and environment.
///
/// Arguments are not among them. They have become an `Environment` by the
/// time the frame exists, because variables live on the chain rather than in
/// frame slots — one allocation per call, which `docs/design.md` records as
/// the price of keeping one environment representation for now.
const FRAME: usize = 4;

/// Unwind records: how a compiled `block`, `catch`, `tagbody`,
/// `unwind-protect` or `handler-case` leaves something behind to be found.
///
/// The interpreter pushes a `Frame` object onto a cons list and walks it
/// (chapters 14 and 20). A contiguous stack has no cons list, so a record is
/// seven slots *on the operand stack*, and the records are chained to each
/// other by index through [`Mutation::mp`]. Pushing one allocates nothing.
///
/// Why a chain at all, when the records are already in order on the stack:
/// because ordinary values sit between them and nothing distinguishes a
/// record slot from a value slot by looking. The chain is what makes finding
/// the next one down a read rather than a search.
pub mod mark {
    /// `block` — a lexical exit. The datum is a fresh marker object.
    pub const BLOCK: i64 = 1;
    /// `catch` — a dynamic exit. The datum is the tag, an ordinary value.
    pub const CATCH: i64 = 2;
    /// A `tagbody` tag reachable from a closure that outlived it.
    pub const TAG: i64 = 3;
    /// `unwind-protect`. The datum is where the cleanup code starts.
    pub const PROTECT: i64 = 4;
    /// `handler-case`. The datum is the clause list.
    pub const HANDLER: i64 = 5;
    /// An unwind that has stopped to let a cleanup run, and will carry on
    /// when it reaches `UNPROTECT`.
    ///
    /// It reuses the frame slots for the three pieces of state an unwind is
    /// carrying, because a pause has no frame of its own to restore: `FP`
    /// holds the value, `PC` the full list of values, and `DATUM` the target
    /// still being looked for.
    pub const PAUSED: i64 = 6;

    /// The index of the next record down, or `usize::MAX` as a fixnum.
    pub const PREV: usize = 0;
    /// Which of the kinds above.
    pub const KIND: usize = 1;
    /// The marker, tag, clause list or cleanup address.
    pub const DATUM: usize = 2;
    /// The frame pointer to restore.
    pub const FP: usize = 3;
    /// Where to carry on.
    pub const PC: usize = 4;
    /// The function to carry on in.
    pub const FUNCTION: usize = 5;
    /// The environment to carry on in.
    pub const ENV: usize = 6;
    /// How many slots one record takes.
    pub const SIZE: usize = 7;
}

/// The bytecode machine.
pub struct Vm<'a, 'h> {
    mc: &'a mut Mutation<'h>,
    /// Set when an instruction fails. The machine halts and the driver reports
    /// it, exactly as `eval::Machine` does.
    pub error: Option<EvalError>,
    halted: bool,
}

impl<'a, 'h> Vm<'a, 'h> {
    /// Prepare to run `function` in `env`, discarding whatever was in
    /// progress.
    pub fn start(mc: &'a mut Mutation<'h>, function: Value<'h>, env: Value<'h>) -> Vm<'a, 'h> {
        let wanted = mc
            .slot(function, function::MAX_STACK)
            .as_fixnum()
            .unwrap_or(0)
            .max(0) as usize;
        let stack = mc.alloc(ObjectType::Vector, wanted.max(INITIAL_STACK));
        mc.set_register(Register::Function, function);
        mc.set_register(Register::Operands, stack);
        mc.set_register(Register::Env, env);
        mc.set_register(Register::Value, Value::NIL);
        mc.set_register(Register::Values, Value::UNBOUND);
        // UNBOUND, not nil: `(catch nil ...)` is legal, so nil cannot mean
        // "nothing is unwinding".
        mc.set_register(Register::Unwind, Value::UNBOUND);
        mc.set_pc(0);
        mc.set_sp(0);
        mc.set_fp(0);
        mc.set_mp(usize::MAX);
        Vm {
            mc,
            error: None,
            halted: false,
        }
    }

    /// Continue a computation already in the registers.
    pub fn resume(mc: &'a mut Mutation<'h>) -> Vm<'a, 'h> {
        Vm {
            mc,
            error: None,
            halted: false,
        }
    }

    /// The answer, once halted.
    pub fn result(&self) -> Value<'h> {
        self.mc.register(Register::Value)
    }

    // ---- the operand stack -------------------------------------------------

    fn capacity(&self) -> usize {
        let stack = self.mc.register(Register::Operands);
        self.mc.header(stack).slots
    }

    /// Make room for `extra` more slots, growing the stack if needed.
    ///
    /// Growing allocates, which is why it happens once per call — against
    /// `max-stack`, which the compiler worked out — rather than once per push.
    /// A JVM class file carries the same field for the same reason.
    fn ensure(&mut self, extra: usize) {
        let wanted = self.mc.sp() + extra;
        if wanted <= self.capacity() {
            return;
        }
        let mut bigger = self.capacity().max(INITIAL_STACK);
        while bigger < wanted {
            bigger *= 2;
        }
        let old = self.mc.register(Register::Operands);
        let new = self.mc.alloc(ObjectType::Vector, bigger);
        for i in 0..self.mc.sp() {
            let v = self.mc.slot(old, i);
            self.mc.set_slot(new, i, v);
        }
        self.mc.set_register(Register::Operands, new);
    }

    fn push(&mut self, v: Value<'h>) {
        self.ensure(1);
        let stack = self.mc.register(Register::Operands);
        let sp = self.mc.sp();
        self.mc.set_slot(stack, sp, v);
        self.mc.set_sp(sp + 1);
    }

    /// Push a value this instruction has just computed.
    ///
    /// **The multiple-values invariant lives here.** A form that produces one
    /// value must leave the `Values` register saying so, or a stale second
    /// value survives into the next form that looks — and surfaces a long way
    /// from whatever set it. The interpreter had one delivery point and
    /// chapter 23 paid for this in one line; a bytecode VM has an arm per
    /// opcode, so the rule is: anything computing a fresh value pushes it
    /// with *this*, and only `VALUES`, `VALUES-LIST` and a `RET` passing a
    /// callee's values through may use [`Vm::push`] directly.
    fn push_fresh(&mut self, v: Value<'h>) {
        self.mc.set_register(Register::Values, Value::UNBOUND);
        self.push(v);
    }

    /// Pop, clearing the slot behind.
    ///
    /// The clearing is not tidiness. Every slot of a traced object is traced,
    /// so a dead slot above the stack pointer would keep whatever it last
    /// held alive for as long as the stack lives — a leak with no symptom.
    /// One store is cheaper than teaching the collector about a boundary that
    /// moves.
    /// Underflow is a `debug_assert` rather than an error, because a program
    /// that pops an empty stack is not a program with a bug in it — it is a
    /// sequence of bytes that is not a program at all, and the compiler's
    /// `max-stack` accounting is what rules it out. The JVM draws the line in
    /// the same place and calls the thing that draws it a *verifier*: one
    /// pass over the code when it is loaded, rather than a test on every pop.
    /// Quokka has no verifier yet and should; #28 carries it.
    fn pop(&mut self) -> Value<'h> {
        let sp = self.mc.sp();
        debug_assert!(sp > 0, "popped an empty operand stack");
        let stack = self.mc.register(Register::Operands);
        let v = self.mc.slot(stack, sp - 1);
        self.mc.set_slot(stack, sp - 1, Value::NIL);
        self.mc.set_sp(sp - 1);
        v
    }

    /// Pop `n` values, keeping the order they were pushed in.
    ///
    /// Popping reverses, so this fills the vector backwards. Getting it the
    /// other way round would make `(- 10 4)` compute 4 − 10, which is the
    /// kind of mistake that passes every test using `+`.
    fn pop_n(&mut self, n: usize) -> Vec<Value<'h>> {
        let mut out = vec![Value::NIL; n];
        for i in (0..n).rev() {
            out[i] = self.pop();
        }
        out
    }

    // ---- scopes ------------------------------------------------------------

    /// The scope `depth` links out from the current environment.
    ///
    /// `depth` counts **every** link, whatever namespace it binds in — an
    /// `flet` scope is stepped over by a variable lookup but it is still a
    /// link, and the compiler has to have counted it. See book chapter 26.
    fn scope_at(&mut self, depth: i32) -> Result<Value<'h>, EvalError> {
        let mut scope = self.mc.register(Register::Env);
        for _ in 0..depth {
            if !self.is_scope(scope) {
                return Err(Self::chain_disagrees());
            }
            scope = self.mc.slot(scope, env::PARENT);
        }
        if !self.is_scope(scope) {
            return Err(Self::chain_disagrees());
        }
        Ok(scope)
    }

    fn is_scope(&self, v: Value<'h>) -> bool {
        v.is_object() && self.mc.header(v).object_type == ObjectType::Environment
    }

    /// The error for a depth that does not land where the compiler thought.
    ///
    /// This is the failure book chapter 26 warns about: a miscounted depth
    /// does not crash, it reads a *different variable*. The namespace check
    /// below turns one whole class of that bug — landing in a function scope
    /// when a variable was meant, or the reverse — into an error at the
    /// instruction that did it, rather than a wrong answer somewhere later.
    fn chain_disagrees() -> EvalError {
        EvalError::BadSyntax {
            operator: "a compiled variable reference",
            detail: "names a scope that is not there: the compiler's depth count \
                     and the machine's chain disagree",
        }
    }

    /// Where the value of slot `index` lives in `scope`, checking the
    /// namespace on the way.
    fn slot_of(
        &mut self,
        depth: i32,
        index: i32,
        namespace: i64,
    ) -> Result<(Value<'h>, usize), EvalError> {
        let scope = self.scope_at(depth)?;
        if self.mc.slot(scope, env::NAMESPACE).as_fixnum() != Some(namespace) {
            return Err(Self::chain_disagrees());
        }
        let at = env::FIRST_PAIR + (index as usize) * 2 + 1;
        if at >= self.mc.header(scope).slots {
            return Err(Self::chain_disagrees());
        }
        Ok((scope, at))
    }

    /// The symbol a global instruction names.
    fn symbol(&mut self, k: i32) -> Result<Value<'h>, EvalError> {
        let s = self.constant(k)?;
        if !self.mc.is_symbol(s) {
            return Err(EvalError::BadSyntax {
                operator: "a global reference",
                detail: "names a constant that is not a symbol",
            });
        }
        Ok(s)
    }

    /// Pop `n` values and push a scope binding `names` to them.
    fn bind_scope(&mut self, k: i32, namespace: i64) -> Result<(), EvalError> {
        let table = self.constant(k)?;
        if !table.is_object() || self.mc.header(table).object_type != ObjectType::Vector {
            return Err(EvalError::BadSyntax {
                operator: "BIND",
                detail: "names a constant that is not a vector of names",
            });
        }
        let n = self.mc.header(table).slots;
        let names: Vec<Value<'h>> = (0..n).map(|i| self.mc.slot(table, i)).collect();
        let values = self.pop_n(n);
        let parent = self.mc.register(Register::Env);
        let scope = env::make(self.mc, parent, namespace, &names, &values);
        self.mc.set_register(Register::Env, scope);
        Ok(())
    }

    // ---- frames ------------------------------------------------------------

    /// Clear every slot from `target` up to the stack pointer, and move the
    /// pointer down. See [`Vm::pop`] for why the clearing matters.
    fn drop_to(&mut self, target: usize) {
        let stack = self.mc.register(Register::Operands);
        for i in target..self.mc.sp() {
            self.mc.set_slot(stack, i, Value::NIL);
        }
        self.mc.set_sp(target);
    }

    /// Is this closure compiled, or is it a list of forms for the
    /// interpreter?
    ///
    /// Compiled code never calls an interpreted closure — that is the whole
    /// of #25's "one evaluator at a time", and the check below is where the
    /// decision stops being a plan and starts being enforced.
    fn compiled_body(&self, callee: Value<'h>) -> Option<Value<'h>> {
        if !closure::is(self.mc, callee) {
            return None;
        }
        let body = self.mc.slot(callee, closure::BODY);
        let compiled = body.is_object() && self.mc.header(body).object_type == ObjectType::Function;
        compiled.then_some(body)
    }

    /// Bind a callee's parameters and enter it.
    ///
    /// `tail` reuses the current frame instead of pushing one, which is what
    /// makes a tail call cost no stack — and, unlike the interpreter's
    /// version, is decided by the compiler rather than discovered by the
    /// frame still being there.
    fn enter(&mut self, callee: Value<'h>, args: &[Value<'h>], tail: bool) -> Transition {
        let Some(code_object) = self.compiled_body(callee) else {
            if closure::is(self.mc, callee) {
                // An interpreted closure in a compiled program. Not a user
                // error and not a missing feature: something has mixed the
                // two evaluators, which #25 says must not happen.
                return self.fail(EvalError::BadSyntax {
                    operator: "CALL",
                    detail: "reached an interpreted closure, which means the \
                             bootstrap let an expander be compiled while its \
                             caller was not",
                });
            }
            // Say *what* was called. "is not a function" without the value is
            // a hint; with it, it is usually the whole diagnosis.
            let what = crate::printer::print(self.mc, callee);
            return self.fail(EvalError::NotAFunction(what));
        };
        let params = self.mc.slot(code_object, function::PARAMS);
        let captured = self.mc.slot(callee, closure::ENV);
        let scope = match self.bind_parameters(params, args, captured) {
            Ok(s) => s,
            Err(e) => return self.fail(e),
        };
        let wanted = self
            .mc
            .slot(code_object, function::MAX_STACK)
            .as_fixnum()
            .unwrap_or(0)
            .max(0) as usize;

        if tail {
            let fp = self.mc.fp();
            self.drop_to(fp);
        } else {
            let pc = self.mc.pc();
            let fp = self.mc.fp();
            let current = self.mc.register(Register::Function);
            let env = self.mc.register(Register::Env);
            self.ensure(FRAME + wanted);
            self.push(Value::fixnum(pc as i64).expect("a code offset is a fixnum"));
            self.push(Value::fixnum(fp as i64).expect("a stack offset is a fixnum"));
            self.push(current);
            self.push(env);
            let sp = self.mc.sp();
            self.mc.set_fp(sp);
        }
        self.ensure(wanted);
        self.mc.set_register(Register::Function, code_object);
        self.mc.set_register(Register::Env, scope);
        self.mc.set_pc(0);
        Transition::Continue
    }

    /// Match arguments to parameters, without building a list when the
    /// parameters are all required.
    ///
    /// The general path is [`crate::lambda_list::bind`], which both machines
    /// share — but it takes a Lisp list, and a compiled call has its
    /// arguments in a row on the stack. Consing them up only for the binder
    /// to walk back down would put an allocation per argument into every
    /// call, which is one of the costs compiling was supposed to remove. So
    /// the common shape is bound directly and anything else falls through.
    fn bind_parameters(
        &mut self,
        params: Value<'h>,
        args: &[Value<'h>],
        parent: Value<'h>,
    ) -> Result<Value<'h>, EvalError> {
        if let Some(names) = self.all_required(params) {
            if names.len() != args.len() {
                return Err(EvalError::WrongArgumentCount {
                    name: "a compiled function",
                    wanted: names.len(),
                    got: args.len(),
                });
            }
            return Ok(env::make(self.mc, parent, env::VARIABLE, &names, args));
        }
        let mut list = Value::NIL;
        for v in args.iter().rev() {
            list = self.mc.cons(*v, list);
        }
        let (names, values) = crate::lambda_list::bind(self.mc, params, list)?;
        Ok(env::make(self.mc, parent, env::VARIABLE, &names, &values))
    }

    /// The parameter names, if the lambda list is nothing but required ones.
    fn all_required(&mut self, params: Value<'h>) -> Option<Vec<Value<'h>>> {
        let mut names = Vec::new();
        let mut cursor = params;
        while cursor.is_cons() {
            let name = self.mc.car(cursor);
            if !self.mc.is_symbol(name) {
                return None;
            }
            // `&optional`, `&rest`, `&key` — anything with a lambda-list
            // keyword needs the general binder.
            let text = self.mc.symbol_name(name);
            if text.starts_with('&') {
                return None;
            }
            names.push(name);
            cursor = self.mc.cdr(cursor);
        }
        cursor.is_nil().then_some(names)
    }

    /// Leave the current function, delivering `v`.
    ///
    /// Deliberately [`Vm::push`] and not [`Vm::push_fresh`]: a callee that
    /// returned several values has already set the `Values` register, and
    /// returning is one of the three places allowed to leave it alone.
    fn do_return(&mut self, v: Value<'h>) -> Transition {
        let fp = self.mc.fp();
        if fp == 0 {
            return self.halt_with(v);
        }
        let base = fp - FRAME;
        let stack = self.mc.register(Register::Operands);
        let pc = self.mc.slot(stack, base).as_fixnum().unwrap_or(0) as usize;
        let caller_fp = self.mc.slot(stack, base + 1).as_fixnum().unwrap_or(0) as usize;
        let function = self.mc.slot(stack, base + 2);
        let env = self.mc.slot(stack, base + 3);
        self.drop_to(base);
        self.mc.set_pc(pc);
        self.mc.set_fp(caller_fp);
        self.mc.set_register(Register::Function, function);
        self.mc.set_register(Register::Env, env);
        self.push(v);
        Transition::Continue
    }

    fn call(&mut self, n: usize, tail: bool) -> Transition {
        let args = self.pop_n(n);
        let callee = self.pop();
        if let Some(index) = callee.as_primitive() {
            if let Some(transition) = self.intercept(index, &args, tail) {
                return transition;
            }
            return match primitives::call_slice(self.mc, index, &args) {
                Ok(v) => {
                    if tail {
                        self.mc.set_register(Register::Values, Value::UNBOUND);
                        self.do_return(v)
                    } else {
                        self.push_fresh(v);
                        Transition::Continue
                    }
                }
                Err(e) => self.fail(e),
            };
        }
        self.enter(callee, &args, tail)
    }

    /// The primitives the machine has to do itself.
    ///
    /// `error` unwinds and `values` sets a register, and a primitive can do
    /// neither: it is handed arguments and returns one value. The interpreter
    /// intercepts the same three in its apply path, for the same reason.
    ///
    /// Compiled code normally meets `values` as the `VALUES` instruction and
    /// never gets here; this is the path for `(funcall #'values 1 2)`, where
    /// the function was a value rather than a name.
    fn intercept(&mut self, index: u32, args: &[Value<'h>], tail: bool) -> Option<Transition> {
        if index == primitives::ERROR {
            let datum = args.first().copied().unwrap_or(Value::NIL);
            let rest = args.get(1..).unwrap_or(&[]);
            let condition = self.make_condition(datum, rest);
            return Some(self.start_unwind(condition, Value::NIL));
        }
        if index == primitives::VALUES || index == primitives::VALUES_LIST {
            let values: Vec<Value<'h>> = if index == primitives::VALUES {
                args.to_vec()
            } else {
                let mut out = Vec::new();
                let mut cursor = args.first().copied().unwrap_or(Value::NIL);
                while cursor.is_cons() {
                    out.push(self.mc.car(cursor));
                    cursor = self.mc.cdr(cursor);
                }
                out
            };
            let primary = values.first().copied().unwrap_or(Value::NIL);
            if values.len() == 1 {
                self.push_fresh(primary);
            } else {
                let mut list = Value::NIL;
                for v in values.iter().rev() {
                    list = self.mc.cons(*v, list);
                }
                self.mc.set_register(Register::Values, list);
                self.push(primary);
            }
            if tail {
                let v = self.pop();
                return Some(self.do_return(v));
            }
            return Some(Transition::Continue);
        }
        None
    }

    /// `(error "text")` or `(error 'some-type)`, as a condition object.
    fn make_condition(&mut self, datum: Value<'h>, rest: &[Value<'h>]) -> Value<'h> {
        let mut data = Value::NIL;
        for v in rest.iter().rev() {
            data = self.mc.cons(*v, data);
        }
        if self.mc.is_symbol(datum) {
            return crate::condition::make(self.mc, datum, Value::NIL, data);
        }
        if datum.is_object() && self.mc.header(datum).object_type.is_string() {
            let simple = self.mc.intern("SIMPLE-ERROR");
            return crate::condition::make(self.mc, simple, datum, data);
        }
        let simple = self.mc.intern("SIMPLE-ERROR");
        crate::condition::make(self.mc, simple, Value::NIL, data)
    }

    // ---- unwind records ----------------------------------------------------

    /// Push a record, remembering where to carry on if an unwind finds it.
    fn push_mark(&mut self, kind: i64, datum: Value<'h>, resume: usize) {
        self.ensure(mark::SIZE);
        let at = self.mc.sp();
        let prev = self.mc.mp();
        let fp = self.mc.fp();
        let function = self.mc.register(Register::Function);
        let env = self.mc.register(Register::Env);
        self.push(Self::index(prev));
        self.push(Value::fixnum(kind).expect("a mark kind is small"));
        self.push(datum);
        self.push(Self::index(fp));
        self.push(Self::index(resume));
        self.push(function);
        self.push(env);
        self.mc.set_mp(at);
    }

    /// A stack or code index as a fixnum, with `usize::MAX` meaning "none".
    fn index(v: usize) -> Value<'h> {
        if v == usize::MAX {
            return Value::fixnum(-1).expect("small");
        }
        Value::fixnum(v as i64).expect("a stack index is a fixnum")
    }

    fn read_index(&self, at: usize, slot: usize) -> usize {
        let stack = self.mc.register(Register::Operands);
        match self.mc.slot(stack, at + slot).as_fixnum() {
            Some(n) if n >= 0 => n as usize,
            _ => usize::MAX,
        }
    }

    fn mark_slot(&self, at: usize, slot: usize) -> Value<'h> {
        let stack = self.mc.register(Register::Operands);
        self.mc.slot(stack, at + slot)
    }

    /// Drop the innermost record without acting on it: what a `block` or
    /// `catch` that was left normally does.
    fn pop_mark(&mut self) -> Transition {
        let at = self.mc.mp();
        if at == usize::MAX {
            return self.fail(EvalError::BadSyntax {
                operator: "UNMARK",
                detail: "there is no unwind record to drop",
            });
        }
        // The body's value is above the record and has to survive it.
        let v = self.pop();
        let prev = self.read_index(at, mark::PREV);
        let env = self.mark_slot(at, mark::ENV);
        self.drop_to(at);
        self.mc.set_mp(prev);
        self.mc.set_register(Register::Env, env);
        self.push(v);
        Transition::Continue
    }

    /// Begin unwinding towards `target`, carrying `value`.
    ///
    /// Unwinding is a mode, not a loop: one record per [`Vm::step`], because
    /// rule 4 does not stop applying because something went wrong. The
    /// `Unwind` register is `UNBOUND` when nothing is unwinding — not `nil`,
    /// which `(catch nil ...)` makes a perfectly good tag.
    fn start_unwind(&mut self, target: Value<'h>, value: Value<'h>) -> Transition {
        self.mc.set_register(Register::Unwind, target);
        self.mc.set_register(Register::Value, value);
        Transition::Continue
    }

    fn is_unwinding(&self) -> bool {
        self.mc.register(Register::Unwind) != Value::UNBOUND
    }

    /// Does this record want the unwind that is going past it?
    fn mark_matches(&mut self, at: usize, target: Value<'h>) -> bool {
        let kind = self.mark_slot(at, mark::KIND).as_fixnum().unwrap_or(0);
        let datum = self.mark_slot(at, mark::DATUM);
        match kind {
            // A block or tag is found by identity: the marker object the
            // BLOCK instruction made and bound in the environment.
            mark::BLOCK | mark::TAG => datum == target,
            // A catch is found by `eql` on the tag, which for everything a
            // tag may usefully be is the same as identity.
            mark::CATCH => datum == target,
            // A handler record stops *any* condition, and the compiled
            // clause dispatch it resumes into decides whether this one is
            // wanted — re-signalling if not, which searches further out
            // because this record has already been popped.
            //
            // Which keeps the judgement in Lisp. Deciding here would mean
            // the VM knowing what a `handler-case` clause looks like, and
            // rule 1 says that belongs above the primitive table.
            mark::HANDLER => {
                let _ = datum;
                crate::condition::is_condition(self.mc, target)
            }
            _ => false,
        }
    }

    /// One step of unwinding.
    fn step_unwind(&mut self) -> Transition {
        let target = self.mc.register(Register::Unwind);
        let at = self.mc.mp();
        if at == usize::MAX {
            // Nothing wanted it. A throw to a tag nobody is catching, or a
            // condition nobody is handling.
            let value = self.mc.register(Register::Value);
            self.mc.set_register(Register::Unwind, Value::UNBOUND);
            let _ = value;
            if crate::condition::is_condition(self.mc, target) {
                let what = crate::condition::describe(self.mc, target);
                return self.fail(EvalError::Signalled(what));
            }
            let what = crate::printer::print(self.mc, target);
            return self.fail(EvalError::NoSuchExit(what));
        }
        let kind = self.mark_slot(at, mark::KIND).as_fixnum().unwrap_or(0);
        let prev = self.read_index(at, mark::PREV);

        // A cleanup runs *while* the unwind is in flight, so the unwind has
        // to be put down and picked up again. Everything it was carrying goes
        // into a PAUSED record, and `UNPROTECT` at the end of the cleanup
        // takes it back out.
        if kind == mark::PROTECT {
            let cleanup = self.read_index(at, mark::DATUM);
            let fp = self.read_index(at, mark::FP);
            let function = self.mark_slot(at, mark::FUNCTION);
            let env = self.mark_slot(at, mark::ENV);
            let value = self.mc.register(Register::Value);
            let values = self.mc.register(Register::Values);
            self.drop_to(at);
            self.mc.set_mp(prev);
            self.mc.set_fp(fp);
            self.mc.set_register(Register::Function, function);
            self.mc.set_register(Register::Env, env);
            self.push_mark(mark::PAUSED, target, 0);
            let paused = self.mc.mp();
            let stack = self.mc.register(Register::Operands);
            self.mc.set_slot(stack, paused + mark::FP, value);
            self.mc.set_slot(stack, paused + mark::PC, values);
            self.mc.set_register(Register::Unwind, Value::UNBOUND);
            self.mc.set_register(Register::Values, Value::UNBOUND);
            self.mc.set_pc(cleanup);
            return Transition::Continue;
        }

        if self.mark_matches(at, target) {
            let fp = self.read_index(at, mark::FP);
            let pc = self.read_index(at, mark::PC);
            let function = self.mark_slot(at, mark::FUNCTION);
            let env = self.mark_slot(at, mark::ENV);
            let value = self.mc.register(Register::Value);
            self.drop_to(at);
            self.mc.set_mp(prev);
            self.mc.set_fp(fp);
            self.mc.set_pc(pc);
            self.mc.set_register(Register::Function, function);
            self.mc.set_register(Register::Env, env);
            self.mc.set_register(Register::Unwind, Value::UNBOUND);
            // `push`, not `push_fresh`: a `(return-from b (values 1 2))`
            // carries both values out with it, exactly as chapter 23 says a
            // block is transparent to them.
            self.push(value);
            return Transition::Continue;
        }

        // Not this one. Drop it and look further out, one step at a time.
        self.drop_to(at);
        self.mc.set_mp(prev);
        Transition::Continue
    }

    // ---- stepping ----------------------------------------------------------

    fn fail(&mut self, e: EvalError) -> Transition {
        self.error = Some(e);
        self.halted = true;
        Transition::Halt
    }

    fn halt_with(&mut self, v: Value<'h>) -> Transition {
        self.mc.set_register(Register::Value, v);
        self.halted = true;
        Transition::Halt
    }

    fn constant(&mut self, k: i32) -> Result<Value<'h>, EvalError> {
        let f = self.mc.register(Register::Function);
        let constants = self.mc.slot(f, function::CONSTANTS);
        let k = k as usize;
        if !constants.is_object() || k >= self.mc.header(constants).slots {
            return Err(EvalError::BadSyntax {
                operator: "a compiled function",
                detail: "names a constant it does not have",
            });
        }
        Ok(self.mc.slot(constants, k))
    }

    fn jump(&mut self, from: usize, offset: i32) -> Transition {
        // Offsets are relative to the end of the jump, which is what makes a
        // single backpatching pass possible: the assembler knows the end as
        // soon as it has emitted the instruction.
        let target = from as i64 + offset as i64;
        if target < 0 {
            return self.fail(EvalError::BadSyntax {
                operator: "a jump",
                detail: "lands before the start of the code",
            });
        }
        self.mc.set_pc(target as usize);
        Transition::Continue
    }
}

impl Steppable for Vm<'_, '_> {
    fn step(&mut self) -> Transition {
        if self.halted {
            return Transition::Halt;
        }
        // Checked first, exactly as the interpreter does: past the hard limit
        // the heap will not grow, so continuing allocates into a ceiling.
        if self.mc.is_exhausted() {
            return self.fail(EvalError::HeapExhausted);
        }
        if self.is_unwinding() {
            return self.step_unwind();
        }

        let f = self.mc.register(Register::Function);
        let code = self.mc.slot(f, function::CODE);
        let length = self
            .mc
            .slot(f, function::CODE_LENGTH)
            .as_fixnum()
            .unwrap_or(0) as usize;
        let at = self.mc.pc();
        let decoded = match code::decode_in_heap(self.mc, code, length, at) {
            Ok(d) => d,
            Err(CodeError::UnknownOpcode { .. }) => {
                return self.fail(EvalError::BadSyntax {
                    operator: "bytecode",
                    detail: "is not an instruction",
                })
            }
            Err(CodeError::Truncated { .. }) => {
                // Running off the end means the function had no RET, which is
                // a bug in whatever produced it rather than in the program.
                return self.fail(EvalError::BadSyntax {
                    operator: "a compiled function",
                    detail: "ran past the end of its code without returning",
                });
            }
        };
        self.mc.set_pc(decoded.next);

        match decoded.op {
            Op::Nil => self.push_fresh(Value::NIL),
            Op::T => self.push_fresh(Value::T),
            Op::Fix => {
                let v = Value::fixnum(decoded.a as i64).expect("a 16-bit operand is a fixnum");
                self.push_fresh(v);
            }
            Op::Const => match self.constant(decoded.a) {
                Ok(v) => self.push_fresh(v),
                Err(e) => return self.fail(e),
            },
            Op::Pop => {
                self.pop();
            }
            Op::Prim => {
                let args = self.pop_n(decoded.b as usize);
                let index = decoded.a as u32;
                // `error` and `values` have to be done by the machine, and
                // the compiler reaches them through PRIM like any other
                // primitive, so the interception lives on both paths.
                if let Some(transition) = self.intercept(index, &args, false) {
                    return transition;
                }
                match primitives::call_slice(self.mc, index, &args) {
                    Ok(v) => self.push_fresh(v),
                    Err(e) => return self.fail(e),
                }
            }
            // ---- variables ----
            Op::Lref | Op::Fref => {
                let namespace = if decoded.op == Op::Lref {
                    env::VARIABLE
                } else {
                    env::FUNCTION
                };
                match self.slot_of(decoded.a, decoded.b, namespace) {
                    Ok((scope, at)) => {
                        let v = self.mc.slot(scope, at);
                        self.push_fresh(v);
                    }
                    Err(e) => return self.fail(e),
                }
            }
            Op::Lset => match self.slot_of(decoded.a, decoded.b, env::VARIABLE) {
                Ok((scope, at)) => {
                    let v = self.pop();
                    self.mc.set_slot(scope, at, v);
                    self.push_fresh(v);
                }
                Err(e) => return self.fail(e),
            },
            Op::Gref => match self.symbol(decoded.a) {
                Ok(s) => {
                    let v = self.mc.slot(s, symbol::VALUE);
                    if v == Value::UNBOUND {
                        let name = self.mc.symbol_name(s);
                        return self.fail(EvalError::UnboundVariable(name));
                    }
                    self.push_fresh(v);
                }
                Err(e) => return self.fail(e),
            },
            Op::Gset => match self.symbol(decoded.a) {
                Ok(s) => {
                    let v = self.pop();
                    self.mc.set_slot(s, symbol::VALUE, v);
                    self.push_fresh(v);
                }
                Err(e) => return self.fail(e),
            },
            Op::Gfref => match self.symbol(decoded.a) {
                Ok(s) => {
                    let v = self.mc.slot(s, symbol::FUNCTION);
                    if v == Value::UNBOUND {
                        let name = self.mc.symbol_name(s);
                        return self.fail(EvalError::UndefinedFunction(name));
                    }
                    self.push_fresh(v);
                }
                Err(e) => return self.fail(e),
            },

            // ---- scopes ----
            Op::Bind | Op::Bindf => {
                let namespace = if decoded.op == Op::Bind {
                    env::VARIABLE
                } else {
                    env::FUNCTION
                };
                if let Err(e) = self.bind_scope(decoded.a, namespace) {
                    return self.fail(e);
                }
            }
            Op::Unbind => {
                let mut scope = self.mc.register(Register::Env);
                for _ in 0..decoded.a {
                    if !self.is_scope(scope) {
                        return self.fail(Self::chain_disagrees());
                    }
                    scope = self.mc.slot(scope, env::PARENT);
                }
                self.mc.set_register(Register::Env, scope);
            }

            // ---- calls ----
            Op::Closure => {
                let code_object = match self.constant(decoded.a) {
                    Ok(v) => v,
                    Err(e) => return self.fail(e),
                };
                if !code_object.is_object()
                    || self.mc.header(code_object).object_type != ObjectType::Function
                {
                    return self.fail(EvalError::BadSyntax {
                        operator: "CLOSURE",
                        detail: "names a constant that is not a compiled function",
                    });
                }
                let params = self.mc.slot(code_object, function::PARAMS);
                let env = self.mc.register(Register::Env);
                let c = closure::make(self.mc, params, code_object, env);
                self.push_fresh(c);
            }
            Op::Call => return self.call(decoded.a as usize, false),
            Op::Tcall => return self.call(decoded.a as usize, true),

            Op::Jmp => return self.jump(decoded.next, decoded.a),
            Op::JmpNil => {
                let v = self.pop();
                if v.is_nil() {
                    return self.jump(decoded.next, decoded.a);
                }
            }
            Op::JmpNotNil => {
                let v = self.pop();
                if !v.is_nil() {
                    return self.jump(decoded.next, decoded.a);
                }
            }
            Op::Ret => {
                let v = self.pop();
                return self.do_return(v);
            }
            // ---- leaving early ----
            Op::Block | Op::Tag => {
                // A fresh, unforgeable identity. `ObjectType::Instance` with
                // no slots, which is what chapter 20 made conditions stop
                // sharing — a marker must never be mistaken for one.
                let marker = self.mc.alloc(ObjectType::Instance, 0);
                let name = match self.constant(decoded.a) {
                    Ok(v) => v,
                    Err(e) => return self.fail(e),
                };
                let namespace = if decoded.op == Op::Block {
                    env::BLOCK
                } else {
                    env::TAG
                };
                let kind = if decoded.op == Op::Block {
                    mark::BLOCK
                } else {
                    mark::TAG
                };
                let parent = self.mc.register(Register::Env);
                let scope = env::make(self.mc, parent, namespace, &[name], &[marker]);
                self.mc.set_register(Register::Env, scope);
                let resume = (decoded.next as i64 + decoded.b as i64) as usize;
                self.push_mark(kind, marker, resume);
            }
            Op::Retfrom | Op::Go => {
                let namespace = if decoded.op == Op::Retfrom {
                    env::BLOCK
                } else {
                    env::TAG
                };
                let marker = match self.slot_of(decoded.a, decoded.b, namespace) {
                    Ok((scope, at)) => self.mc.slot(scope, at),
                    Err(e) => return self.fail(e),
                };
                // `go` carries no value; `return-from` carries one.
                let v = if decoded.op == Op::Retfrom {
                    self.pop()
                } else {
                    Value::NIL
                };
                return self.start_unwind(marker, v);
            }
            Op::Catch => {
                let tag = self.pop();
                let resume = (decoded.next as i64 + decoded.a as i64) as usize;
                self.push_mark(mark::CATCH, tag, resume);
            }
            Op::Throw => {
                let v = self.pop();
                let tag = self.pop();
                return self.start_unwind(tag, v);
            }
            Op::Handler => {
                let clauses = match self.constant(decoded.a) {
                    Ok(v) => v,
                    Err(e) => return self.fail(e),
                };
                let resume = (decoded.next as i64 + decoded.b as i64) as usize;
                self.push_mark(mark::HANDLER, clauses, resume);
            }
            Op::Protect => {
                // The datum is where the cleanup starts, so the unwinder can
                // get to it. The normal path runs the same cleanup code
                // inline, which the compiler emits twice — one copy for each
                // way out, rather than a mechanism for sharing one.
                let cleanup = (decoded.next as i64 + decoded.a as i64) as usize;
                let at = Self::index(cleanup);
                self.push_mark(mark::PROTECT, at, decoded.next);
            }
            Op::Unmark => return self.pop_mark(),
            Op::Unprotect => {
                // A cleanup has finished. Take the unwind back out of the
                // PAUSED record it was put down in and carry on from where
                // it had got to.
                let at = self.mc.mp();
                if at == usize::MAX
                    || self.mark_slot(at, mark::KIND).as_fixnum() != Some(mark::PAUSED)
                {
                    return self.fail(EvalError::BadSyntax {
                        operator: "UNPROTECT",
                        detail: "there is no paused unwind to resume",
                    });
                }
                let target = self.mark_slot(at, mark::DATUM);
                let value = self.mark_slot(at, mark::FP);
                let values = self.mark_slot(at, mark::PC);
                let prev = self.read_index(at, mark::PREV);
                self.drop_to(at);
                self.mc.set_mp(prev);
                self.mc.set_register(Register::Values, values);
                return self.start_unwind(target, value);
            }

            // ---- multiple values ----
            Op::Values => {
                let n = decoded.a as usize;
                let vs = self.pop_n(n);
                // One value is the ordinary case and must cost nothing, so
                // the register keeps saying UNBOUND for it. Chapter 23.
                if n == 1 {
                    self.push_fresh(vs[0]);
                } else {
                    let mut list = Value::NIL;
                    for v in vs.iter().rev() {
                        list = self.mc.cons(*v, list);
                    }
                    let primary = vs.first().copied().unwrap_or(Value::NIL);
                    self.mc.set_register(Register::Values, list);
                    self.push(primary);
                }
            }
            Op::Mvsave => {
                // Everything the last form returned, as a list, *replacing*
                // the single value it left on the stack.
                //
                // Where the two machines genuinely differ. The interpreter
                // keeps the primary value in a register, so collecting every
                // value reads two registers and touches nothing else. Here
                // the primary is on the *stack* and only the extras are in a
                // register, so this has to pop before it builds. Pushing the
                // list without popping leaves the primary buried under it,
                // and the next instruction that pops a fixed number of things
                // takes the wrong ones — which is how this was found:
                // `multiple-value-call` tried to call `(1 2)`.
                let primary = self.pop();
                let values = self.mc.register(Register::Values);
                let list = if values == Value::UNBOUND {
                    self.mc.cons(primary, Value::NIL)
                } else {
                    values
                };
                self.push_fresh(list);
            }
            Op::Mvrestore => {
                let list = self.pop();
                let primary = if list.is_cons() {
                    self.mc.car(list)
                } else {
                    Value::NIL
                };
                self.mc.set_register(Register::Values, list);
                self.push(primary);
            }
            Op::Mvcall => {
                let lists = self.pop_n(decoded.a as usize);
                let callee = self.pop();
                let mut args = Vec::new();
                for list in lists {
                    let mut cursor = list;
                    while cursor.is_cons() {
                        args.push(self.mc.car(cursor));
                        cursor = self.mc.cdr(cursor);
                    }
                }
                if let Some(index) = callee.as_primitive() {
                    return match primitives::call_slice(self.mc, index, &args) {
                        Ok(v) => {
                            self.push_fresh(v);
                            Transition::Continue
                        }
                        Err(e) => self.fail(e),
                    };
                }
                return self.enter(callee, &args, false);
            }
        }
        Transition::Continue
    }
}

/// Run `function` to completion, for tests and for the REPL once there is a
/// compiler. The interpreter's `eval_now` with a different machine.
pub fn run_now<'h>(
    mc: &mut Mutation<'h>,
    function: Value<'h>,
    budget: u64,
) -> Result<Value<'h>, EvalError> {
    use crate::slice::{run_slice, Status};
    let mut vm = Vm::start(mc, function, Value::NIL);
    match run_slice(&mut vm, budget) {
        Status::Done { .. } => match vm.error.take() {
            Some(e) => Err(e),
            None => Ok(vm.result()),
        },
        Status::Yielded => Err(EvalError::BudgetExhausted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{encode, OPCODE_COUNT};
    use crate::heap::Heap;
    use crate::slice::{run_slice, Status};

    /// Primitive indices, pinned where they are used so that a reordering of
    /// the table shows up here rather than as arithmetic that is quietly
    /// wrong. `primitives::the_table_order_is_pinned` is the other half.
    const ADD: i32 = 0;
    const SUB: i32 = 1;
    const MUL: i32 = 2;

    /// One item of a hand-written program.
    ///
    /// Counting jump offsets by hand is how the first version of
    /// `a_tail_call_does_not_grow_the_stack` came to jump four bytes over a
    /// two-byte branch, landing in the *middle* of a later instruction, where
    /// the machine decoded the high half of an operand as an opcode and ran
    /// on through nonsense. The error that eventually surfaced was three
    /// instructions from the cause.
    ///
    /// So: labels. This is the assembler the compiler will need anyway, in
    /// miniature, and it exists because of a bug rather than in anticipation
    /// of one.
    enum I {
        /// An ordinary instruction.
        Ins(Op, i32, i32),
        /// Somewhere a jump can aim at. Takes no space.
        Label(&'static str),
        /// A jump, by label.
        To(Op, &'static str),
        /// A `BLOCK`-shaped instruction: a constant index and a label.
        Mark(Op, i32, &'static str),
    }
    use I::*;

    /// Resolve labels and emit.
    ///
    /// One pass to place the labels and one to emit, which is all it takes
    /// because an instruction's size depends only on its opcode — the whole
    /// point of fixing operand widths, arriving as a convenience rather than
    /// as the theory it was argued as.
    fn assemble(items: &[I]) -> Vec<u8> {
        let mut at = 0usize;
        let mut labels = std::collections::HashMap::new();
        for item in items {
            match item {
                Label(name) => {
                    labels.insert(*name, at);
                }
                Ins(op, _, _) | To(op, _) | Mark(op, _, _) => at += op.width(),
            }
        }
        let mut bytes = Vec::new();
        for item in items {
            match item {
                Label(_) => {}
                Ins(op, a, b) => encode(&mut bytes, *op, *a, *b),
                To(op, name) => {
                    let end = bytes.len() + op.width();
                    let target = labels[name] as i32;
                    encode(&mut bytes, *op, target - end as i32, 0);
                }
                Mark(op, k, name) => {
                    let end = bytes.len() + op.width();
                    let target = labels[name] as i32;
                    encode(&mut bytes, *op, *k, target - end as i32);
                }
            }
        }
        bytes
    }

    /// Assemble, wrap in a function, run, print.
    fn run_asm<'h>(mc: &mut Mutation<'h>, items: &[I], constants: &[Value<'h>]) -> String {
        let bytes = assemble(items);
        let table = mc.alloc(ObjectType::Vector, constants.len());
        for (i, c) in constants.iter().enumerate() {
            mc.set_slot(table, i, *c);
        }
        let name = mc.intern("TEST");
        let f = code::make_function(mc, &bytes, table, Value::NIL, 16, name);
        match run_now(mc, f, 100_000) {
            Ok(v) => crate::printer::print(mc, v),
            Err(e) => format!("; {e}"),
        }
    }

    /// Assemble a function with no constants.
    fn asm<'h>(mc: &mut Mutation<'h>, body: &[(Op, i32, i32)]) -> Value<'h> {
        asm_with(mc, body, &[])
    }

    fn asm_with<'h>(
        mc: &mut Mutation<'h>,
        body: &[(Op, i32, i32)],
        constants: &[Value<'h>],
    ) -> Value<'h> {
        let mut bytes = Vec::new();
        for (op, a, b) in body {
            encode(&mut bytes, *op, *a, *b);
        }
        let table = mc.alloc(ObjectType::Vector, constants.len());
        for (i, c) in constants.iter().enumerate() {
            mc.set_slot(table, i, *c);
        }
        let name = mc.intern("TEST");
        code::make_function(mc, &bytes, table, Value::NIL, 8, name)
    }

    fn run(body: &[(Op, i32, i32)]) -> String {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let f = asm(mc, body);
            match run_now(mc, f, 10_000) {
                Ok(v) => crate::printer::print(mc, v),
                Err(e) => format!("; {e}"),
            }
        })
    }

    /// The expression from chapter 25, compiled by hand.
    ///
    /// `(+ (* 2 3) 4)` — and the point of the stack is that nothing had to
    /// say where the 6 goes.
    #[test]
    fn an_expression_evaluates_on_the_stack() {
        assert_eq!(
            run(&[
                (Op::Fix, 2, 0),
                (Op::Fix, 3, 0),
                (Op::Prim, MUL, 2),
                (Op::Fix, 4, 0),
                (Op::Prim, ADD, 2),
                (Op::Ret, 0, 0),
            ]),
            "10"
        );
    }

    /// Popping reverses the order the arguments were pushed in, so a
    /// non-commutative primitive is the only thing that catches getting it
    /// wrong. Every test built on `+` would have passed either way.
    #[test]
    fn arguments_reach_a_primitive_in_the_order_they_were_pushed() {
        assert_eq!(
            run(&[
                (Op::Fix, 10, 0),
                (Op::Fix, 4, 0),
                (Op::Prim, SUB, 2),
                (Op::Ret, 0, 0),
            ]),
            "6"
        );
    }

    #[test]
    fn nesting_leaves_the_left_hand_answer_waiting() {
        assert_eq!(
            run(&[
                (Op::Fix, 1, 0),
                (Op::Fix, 2, 0),
                (Op::Prim, ADD, 2),
                (Op::Fix, 10, 0),
                (Op::Fix, 4, 0),
                (Op::Prim, SUB, 2),
                (Op::Prim, MUL, 2),
                (Op::Ret, 0, 0),
            ]),
            "18"
        );
    }

    #[test]
    fn constants_come_from_the_functions_own_table() {
        let mut heap = Heap::new();
        let printed = heap.mutate(|mc| {
            let hello = mc.intern("HELLO");
            let f = asm_with(mc, &[(Op::Const, 0, 0), (Op::Ret, 0, 0)], &[hello]);
            let v = run_now(mc, f, 100).expect("runs");
            crate::printer::print(mc, v)
        });
        assert_eq!(printed, "HELLO");
    }

    /// A `JMP-NIL` over the then-branch: the shape every `if` compiles to.
    #[test]
    fn a_conditional_jump_skips_the_branch_not_taken() {
        // (if nil 1 2) — the jump is taken, so 2.
        let taken = run(&[
            (Op::Nil, 0, 0),
            (Op::JmpNil, 4, 0), // over FIX 1 (3 bytes) and RET (1)
            (Op::Fix, 1, 0),
            (Op::Ret, 0, 0),
            (Op::Fix, 2, 0),
            (Op::Ret, 0, 0),
        ]);
        assert_eq!(taken, "2");

        // (if t 1 2) — not taken, so 1.
        let not_taken = run(&[
            (Op::T, 0, 0),
            (Op::JmpNil, 4, 0),
            (Op::Fix, 1, 0),
            (Op::Ret, 0, 0),
            (Op::Fix, 2, 0),
            (Op::Ret, 0, 0),
        ]);
        assert_eq!(not_taken, "1");
    }

    /// Offsets are relative to the *end* of the jump, which is the convention
    /// that lets an assembler backpatch in one pass. A negative one goes
    /// backwards, which is what every loop is made of.
    ///
    /// Written so that measuring from the wrong place is visible rather than
    /// merely wrong: from the end, this lands on `FIX 11` and returns 11;
    /// from the start of the jump it would land back at offset 0 and loop
    /// until the budget ran out.
    #[test]
    fn a_backward_jump_measures_from_the_end_of_the_jump() {
        assert_eq!(
            run(&[
                (Op::Jmp, 4, 0),   //  0 -> 7
                (Op::Fix, 11, 0),  //  3
                (Op::Ret, 0, 0),   //  6
                (Op::Fix, 22, 0),  //  7
                (Op::Jmp, -10, 0), // 10 -> 3
            ]),
            "11"
        );
    }

    /// Rule 4, tested rather than asserted: the machine stops between two
    /// instructions and carries on where it left off.
    #[test]
    fn a_computation_survives_being_cut_in_half() {
        let mut heap = Heap::new();
        let printed = heap.mutate(|mc| {
            let f = asm(
                mc,
                &[
                    (Op::Fix, 2, 0),
                    (Op::Fix, 3, 0),
                    (Op::Prim, MUL, 2),
                    (Op::Fix, 4, 0),
                    (Op::Prim, ADD, 2),
                    (Op::Ret, 0, 0),
                ],
            );
            {
                let mut vm = Vm::start(mc, f, Value::NIL);
                assert_eq!(run_slice(&mut vm, 2), Status::Yielded);
            }
            // Nothing was carried across in Rust. Everything the machine knew
            // is in the heap, which is the whole claim.
            let answer = {
                let mut vm = Vm::resume(mc);
                assert!(matches!(run_slice(&mut vm, 100), Status::Done { .. }));
                vm.result()
            };
            crate::printer::print(mc, answer)
        });
        assert_eq!(printed, "10");
    }

    /// A popped slot must be cleared, or the stack keeps every value it has
    /// ever held alive — a leak with no symptom until the heap is large.
    #[test]
    fn a_popped_slot_stops_referring_to_what_was_there() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let marker = mc.intern("MARKER");
            let f = asm_with(
                mc,
                &[
                    (Op::Const, 0, 0),
                    (Op::Pop, 0, 0),
                    (Op::Fix, 1, 0),
                    (Op::Ret, 0, 0),
                ],
                &[marker],
            );
            run_now(mc, f, 100).expect("runs");
            let stack = mc.register(Register::Operands);
            assert_eq!(
                mc.slot(stack, 0),
                Value::NIL,
                "the slot still refers to what was popped from it"
            );
        });
    }

    /// The invariant from chapter 23, which the interpreter got for one line
    /// and this machine has to keep per opcode.
    #[test]
    fn computing_a_value_clears_the_values_register() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let f = asm(mc, &[(Op::Fix, 1, 0), (Op::Ret, 0, 0)]);
            let mut vm = Vm::start(mc, f, Value::NIL);
            // Pretend a previous form returned several values.
            vm.mc.set_register(Register::Values, Value::NIL);
            assert_eq!(vm.step(), Transition::Continue);
            assert_eq!(
                vm.mc.register(Register::Values),
                Value::UNBOUND,
                "FIX pushed one value and left the register saying otherwise"
            );
        });
    }

    /// `let` is instructions, not a frame: evaluate the inits, `BIND` them
    /// into a scope, run the body, `UNBIND`.
    #[test]
    fn a_scope_binds_and_reads_back() {
        let mut heap = Heap::new();
        let printed = heap.mutate(|mc| {
            let a = mc.intern("A");
            let names = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(names, 0, a);
            let f = asm_with(
                mc,
                &[
                    (Op::Fix, 41, 0),
                    (Op::Bind, 0, 0),
                    (Op::Lref, 0, 0),
                    (Op::Fix, 1, 0),
                    (Op::Prim, ADD, 2),
                    (Op::Unbind, 1, 0),
                    (Op::Ret, 0, 0),
                ],
                &[names],
            );
            let v = run_now(mc, f, 1_000).expect("runs");
            crate::printer::print(mc, v)
        });
        assert_eq!(printed, "42");
    }

    /// The trap book chapter 26 describes: a function scope is a link in the
    /// chain although a variable lookup steps over it, so the compiler has to
    /// count it. If it miscounts, the reference lands somewhere real and
    /// returns the wrong value — silently, were it not for the namespace
    /// check.
    #[test]
    fn a_depth_that_lands_in_the_wrong_namespace_is_refused() {
        let mut heap = Heap::new();
        let message = heap.mutate(|mc| {
            let a = mc.intern("A");
            let vars = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(vars, 0, a);
            let fns = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(fns, 0, a);
            // A variable scope, then a function scope on top of it. `A` the
            // variable is two links out, not one.
            let f = asm_with(
                mc,
                &[
                    (Op::Fix, 1, 0),
                    (Op::Bind, 0, 0), // variable scope
                    (Op::Nil, 0, 0),
                    (Op::Bindf, 1, 0), // function scope
                    (Op::Lref, 0, 0),  // wrong: depth 0 is the function scope
                    (Op::Ret, 0, 0),
                ],
                &[vars, fns],
            );
            match run_now(mc, f, 1_000) {
                Ok(v) => format!("returned {}", crate::printer::print(mc, v)),
                Err(e) => format!("{e}"),
            }
        });
        assert!(message.contains("disagree"), "{message}");
    }

    /// The same program with the depth the compiler should have worked out.
    #[test]
    fn counting_the_function_scope_finds_the_variable() {
        let mut heap = Heap::new();
        let printed = heap.mutate(|mc| {
            let a = mc.intern("A");
            let vars = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(vars, 0, a);
            let fns = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(fns, 0, a);
            let f = asm_with(
                mc,
                &[
                    (Op::Fix, 7, 0),
                    (Op::Bind, 0, 0),
                    (Op::Nil, 0, 0),
                    (Op::Bindf, 1, 0),
                    (Op::Lref, 1, 0), // one link out, past the function scope
                    (Op::Ret, 0, 0),
                ],
                &[vars, fns],
            );
            let v = run_now(mc, f, 1_000).expect("runs");
            crate::printer::print(mc, v)
        });
        assert_eq!(printed, "7");
    }

    /// A global is read through its symbol every time, which is what lets a
    /// redefinition take effect. Chapter 26's `*limit*`.
    #[test]
    fn a_global_is_read_through_its_symbol() {
        let mut heap = Heap::new();
        let printed = heap.mutate(|mc| {
            let limit = mc.intern("*LIMIT*");
            let f = asm_with(
                mc,
                &[
                    (Op::Fix, 10, 0),
                    (Op::Gset, 0, 0),
                    (Op::Pop, 0, 0),
                    (Op::Gref, 0, 0),
                    (Op::Ret, 0, 0),
                ],
                &[limit],
            );
            let v = run_now(mc, f, 1_000).expect("runs");
            crate::printer::print(mc, v)
        });
        assert_eq!(printed, "10");
    }

    #[test]
    fn an_unbound_global_says_which_one() {
        let mut heap = Heap::new();
        let message = heap.mutate(|mc| {
            let nope = mc.intern("*NOPE*");
            let f = asm_with(mc, &[(Op::Gref, 0, 0), (Op::Ret, 0, 0)], &[nope]);
            match run_now(mc, f, 100) {
                Ok(_) => String::from("ran"),
                Err(e) => format!("{e}"),
            }
        });
        assert!(message.contains("*NOPE*"), "{message}");
    }

    /// `fact`, hand-assembled exactly as `docs/bytecode.md` and book chapter
    /// 25 print it — the first compiled Quokka function to run.
    #[test]
    fn fact_runs() {
        let mut heap = Heap::new();
        let printed = heap.mutate(|mc| {
            let fact = mc.intern("FACT");
            let n = mc.intern("N");
            let params = mc.cons(n, Value::NIL);
            let constants = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(constants, 0, fact);
            let mut bytes = Vec::new();
            for (op, a, b) in [
                (Op::Lref, 0, 0),   //  0  n
                (Op::Fix, 0, 0),    //  3  0
                (Op::Prim, 3, 2),   //  6  =
                (Op::JmpNil, 4, 0), //  9  -> 16
                (Op::Fix, 1, 0),    // 12
                (Op::Ret, 0, 0),    // 15
                (Op::Lref, 0, 0),   // 16  n, the left argument of *
                (Op::Gfref, 0, 0),  // 19  FACT's cell, read now
                (Op::Lref, 0, 0),   // 22
                (Op::Fix, 1, 0),    // 25
                (Op::Prim, SUB, 2), // 28  (- n 1)
                (Op::Call, 1, 0),   // 31
                (Op::Prim, MUL, 2), // 33
                (Op::Ret, 0, 0),    // 36
            ] {
                encode(&mut bytes, op, a, b);
            }
            let name = mc.intern("FACT");
            let code_object = code::make_function(mc, &bytes, constants, params, 4, name);
            assert_eq!(bytes.len(), 37, "chapter 25 counted thirty-seven bytes");

            // (defun fact ...) — put the closure in the symbol's function cell
            // so that GFREF finds it, which is what the real DEFUN expands to.
            let c = closure::make(mc, params, code_object, Value::NIL);
            mc.set_slot(fact, symbol::FUNCTION, c);

            // (fact 10)
            let caller = {
                let mut b = Vec::new();
                encode(&mut b, Op::Gfref, 0, 0);
                encode(&mut b, Op::Fix, 10, 0);
                encode(&mut b, Op::Call, 1, 0);
                encode(&mut b, Op::Ret, 0, 0);
                code::make_function(mc, &b, constants, Value::NIL, 4, name)
            };
            let v = run_now(mc, caller, 100_000).expect("runs");
            crate::printer::print(mc, v)
        });
        assert_eq!(printed, "3628800");
    }

    /// `down` from the README, and the property that makes it run at all:
    /// `TCALL` reuses the frame, so a hundred thousand calls deep the stack
    /// is exactly as deep as it was at the first.
    #[test]
    fn a_tail_call_does_not_grow_the_stack() {
        let mut heap = Heap::new();
        let (printed, depth) = heap.mutate(|mc| {
            let down = mc.intern("DOWN");
            let n = mc.intern("N");
            let params = mc.cons(n, Value::NIL);
            let constants = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(constants, 0, down);
            let mut bytes = Vec::new();
            for (op, a, b) in [
                (Op::Lref, 0, 0),
                (Op::Fix, 0, 0),
                (Op::Prim, 3, 2), // =
                // Over T and RET, which are one byte each — not four, as in
                // `fact`, where the then-branch is `FIX 1` and `RET`. Getting
                // this wrong lands in the *middle* of a later instruction and
                // the machine runs happily on through nonsense, which is the
                // argument for a label-aware assembler rather than counting.
                (Op::JmpNil, 2, 0),
                (Op::T, 0, 0),
                (Op::Ret, 0, 0),
                (Op::Gfref, 0, 0),
                (Op::Lref, 0, 0),
                (Op::Fix, 1, 0),
                (Op::Prim, SUB, 2),
                (Op::Tcall, 1, 0),
            ] {
                encode(&mut bytes, op, a, b);
            }
            let name = mc.intern("DOWN");
            let code_object = code::make_function(mc, &bytes, constants, params, 4, name);
            let c = closure::make(mc, params, code_object, Value::NIL);
            mc.set_slot(down, symbol::FUNCTION, c);

            let caller = {
                let mut b = Vec::new();
                encode(&mut b, Op::Gfref, 0, 0);
                encode(&mut b, Op::Fix, 20_000, 0);
                encode(&mut b, Op::Call, 1, 0);
                encode(&mut b, Op::Ret, 0, 0);
                code::make_function(mc, &b, constants, Value::NIL, 4, name)
            };
            let v = run_now(mc, caller, 10_000_000).expect("runs");
            (
                crate::printer::print(mc, v),
                mc.header(mc.register(Register::Operands)).slots,
            )
        });
        assert_eq!(printed, "T");
        assert_eq!(
            depth, INITIAL_STACK,
            "twenty thousand tail calls grew the operand stack"
        );
    }

    /// The other half of the same claim: an ordinary call *does* use stack,
    /// so the test above is measuring something.
    #[test]
    fn an_ordinary_call_does_grow_the_stack() {
        let mut heap = Heap::new();
        let grown = heap.mutate(|mc| {
            let loop_ = mc.intern("LOOP-DOWN");
            let n = mc.intern("N");
            let params = mc.cons(n, Value::NIL);
            let constants = mc.alloc(ObjectType::Vector, 1);
            mc.set_slot(constants, 0, loop_);
            let mut bytes = Vec::new();
            for (op, a, b) in [
                (Op::Lref, 0, 0),
                (Op::Fix, 0, 0),
                (Op::Prim, 3, 2),
                (Op::JmpNil, 2, 0),
                (Op::T, 0, 0),
                (Op::Ret, 0, 0),
                (Op::Gfref, 0, 0),
                (Op::Lref, 0, 0),
                (Op::Fix, 1, 0),
                (Op::Prim, SUB, 2),
                (Op::Call, 1, 0), // not a tail call this time
                (Op::Ret, 0, 0),
            ] {
                encode(&mut bytes, op, a, b);
            }
            let name = mc.intern("LOOP-DOWN");
            let code_object = code::make_function(mc, &bytes, constants, params, 4, name);
            let c = closure::make(mc, params, code_object, Value::NIL);
            mc.set_slot(loop_, symbol::FUNCTION, c);
            let caller = {
                let mut b = Vec::new();
                encode(&mut b, Op::Gfref, 0, 0);
                encode(&mut b, Op::Fix, 500, 0);
                encode(&mut b, Op::Call, 1, 0);
                encode(&mut b, Op::Ret, 0, 0);
                code::make_function(mc, &b, constants, Value::NIL, 4, name)
            };
            run_now(mc, caller, 1_000_000).expect("runs");
            mc.header(mc.register(Register::Operands)).slots
        });
        assert!(
            grown > INITIAL_STACK,
            "five hundred nested calls should have grown the stack"
        );
    }

    /// The big-bang rule, enforced rather than trusted: compiled code calling
    /// an interpreted closure is a bootstrap that went wrong, not a feature.
    #[test]
    fn compiled_code_refuses_to_call_an_interpreted_closure() {
        let mut heap = Heap::new();
        let message = heap.mutate(|mc| {
            let body = mc.intern("SOMETHING");
            let interpreted = closure::make(mc, Value::NIL, body, Value::NIL);
            let f = asm_with(
                mc,
                &[(Op::Const, 0, 0), (Op::Call, 0, 0), (Op::Ret, 0, 0)],
                &[interpreted],
            );
            match run_now(mc, f, 100) {
                Ok(_) => String::from("ran, which it should not have"),
                Err(e) => format!("{e}"),
            }
        });
        assert!(message.contains("interpreted closure"), "{message}");
    }

    /// `(block out (return-from out 42) 99)` — the 99 is never reached.
    #[test]
    fn a_block_can_be_left_early() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let name = mc.intern("OUT");
            run_asm(
                mc,
                &[
                    Mark(Op::Block, 0, "after"),
                    Ins(Op::Fix, 42, 0),
                    Ins(Op::Retfrom, 0, 0),
                    Ins(Op::Fix, 99, 0), // unreachable
                    Ins(Op::Unmark, 0, 0),
                    Label("after"),
                    Ins(Op::Ret, 0, 0),
                ],
                &[name],
            )
        });
        assert_eq!(out, "42");
    }

    /// And a block left the ordinary way drops its record and keeps its
    /// value, which is the path that has nothing to do with unwinding.
    #[test]
    fn a_block_left_normally_keeps_its_value() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let name = mc.intern("OUT");
            run_asm(
                mc,
                &[
                    Mark(Op::Block, 0, "after"),
                    Ins(Op::Fix, 7, 0),
                    Ins(Op::Unmark, 0, 0),
                    Label("after"),
                    Ins(Op::Ret, 0, 0),
                ],
                &[name],
            )
        });
        assert_eq!(out, "7");
    }

    /// `catch` and `throw` find each other by tag at run time rather than
    /// lexically, which is the whole difference from `block`.
    #[test]
    fn a_throw_finds_its_catch() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let tag = mc.intern("DONE");
            run_asm(
                mc,
                &[
                    Ins(Op::Const, 0, 0),
                    To(Op::Catch, "after"),
                    Ins(Op::Const, 0, 0),
                    Ins(Op::Fix, 5, 0),
                    Ins(Op::Throw, 0, 0),
                    Ins(Op::Fix, 99, 0),
                    Ins(Op::Unmark, 0, 0),
                    Label("after"),
                    Ins(Op::Ret, 0, 0),
                ],
                &[tag],
            )
        });
        assert_eq!(out, "5");
    }

    #[test]
    fn a_throw_with_no_catch_says_so() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let tag = mc.intern("NOWHERE");
            run_asm(
                mc,
                &[
                    Ins(Op::Const, 0, 0),
                    Ins(Op::Fix, 1, 0),
                    Ins(Op::Throw, 0, 0),
                ],
                &[tag],
            )
        });
        assert!(out.contains("no exit named NOWHERE"), "{out}");
    }

    /// A cleanup runs while the unwind is in flight, and the unwind then
    /// carries on. The value thrown must survive the detour.
    #[test]
    fn a_cleanup_runs_during_a_throw_and_the_throw_continues() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let tag = mc.intern("DONE");
            let flag = mc.intern("*RAN*");
            run_asm(
                mc,
                &[
                    Ins(Op::Const, 0, 0),
                    To(Op::Catch, "caught"),
                    To(Op::Protect, "cleanup"),
                    Ins(Op::Const, 0, 0),
                    Ins(Op::Fix, 5, 0),
                    Ins(Op::Throw, 0, 0),
                    Label("cleanup"),
                    Ins(Op::T, 0, 0),
                    Ins(Op::Gset, 1, 0),
                    Ins(Op::Pop, 0, 0),
                    Ins(Op::Unprotect, 0, 0),
                    Label("caught"),
                    // The thrown value is on the stack; add the flag to show
                    // the cleanup ran before the catch was reached.
                    Ins(Op::Gref, 1, 0),
                    Ins(Op::Prim, 0, 0), // (+) => 0, just to use the value
                    Ins(Op::Pop, 0, 0),
                    Ins(Op::Gref, 1, 0),
                    Ins(Op::Ret, 0, 0),
                ],
                &[tag, flag],
            )
        });
        assert_eq!(out, "T", "the cleanup did not run");
    }

    /// Chapter 20's test, compiled: cleanups run innermost first.
    #[test]
    fn nested_cleanups_run_innermost_first() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let tag = mc.intern("DONE");
            let order = mc.intern("*ORDER*");
            let inner = mc.intern("INNER");
            let outer = mc.intern("OUTER");
            run_asm(
                mc,
                &[
                    Ins(Op::Nil, 0, 0),
                    Ins(Op::Gset, 1, 0),
                    Ins(Op::Pop, 0, 0),
                    Ins(Op::Const, 0, 0),
                    To(Op::Catch, "caught"),
                    To(Op::Protect, "outer-cleanup"),
                    To(Op::Protect, "inner-cleanup"),
                    Ins(Op::Const, 0, 0),
                    Ins(Op::Fix, 1, 0),
                    Ins(Op::Throw, 0, 0),
                    Label("inner-cleanup"),
                    Ins(Op::Const, 2, 0),
                    Ins(Op::Gref, 1, 0),
                    Ins(Op::Prim, 6, 2), // CONS
                    Ins(Op::Gset, 1, 0),
                    Ins(Op::Pop, 0, 0),
                    Ins(Op::Unprotect, 0, 0),
                    Label("outer-cleanup"),
                    Ins(Op::Const, 3, 0),
                    Ins(Op::Gref, 1, 0),
                    Ins(Op::Prim, 6, 2),
                    Ins(Op::Gset, 1, 0),
                    Ins(Op::Pop, 0, 0),
                    Ins(Op::Unprotect, 0, 0),
                    Label("caught"),
                    Ins(Op::Pop, 0, 0),
                    Ins(Op::Gref, 1, 0),
                    Ins(Op::Ret, 0, 0),
                ],
                &[tag, order, inner, outer],
            )
        });
        assert_eq!(out, "(OUTER INNER)");
    }

    /// `(handler-case (error "boom") (error () 'caught))`, with the clause
    /// dispatch as ordinary compiled code — the VM only knows *that* a
    /// handler is here, never which conditions it wants.
    #[test]
    fn a_handler_catches_a_signalled_error() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let clauses = mc.intern("CLAUSES");
            let caught = mc.intern("CAUGHT");
            let boom = mc.alloc_constant_string("boom");
            run_asm(
                mc,
                &[
                    Mark(Op::Handler, 0, "handle"),
                    Ins(Op::Const, 2, 0),
                    Ins(Op::Prim, 40, 1), // ERROR
                    Ins(Op::Unmark, 0, 0),
                    To(Op::Jmp, "done"),
                    Label("handle"),
                    // The condition arrives on the stack. A real dispatch
                    // would test its type; this one takes everything.
                    Ins(Op::Pop, 0, 0),
                    Ins(Op::Const, 1, 0),
                    Label("done"),
                    Ins(Op::Ret, 0, 0),
                ],
                &[clauses, caught, boom],
            )
        });
        assert_eq!(out, "CAUGHT");
    }

    #[test]
    fn an_unhandled_error_stops_the_machine() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let boom = mc.alloc_constant_string("boom");
            run_asm(
                mc,
                &[
                    Ins(Op::Const, 0, 0),
                    Ins(Op::Prim, 40, 1),
                    Ins(Op::Ret, 0, 0),
                ],
                &[boom],
            )
        });
        assert!(out.contains("boom"), "{out}");
    }

    /// `(multiple-value-list (values 1 2))` in instructions: VALUES sets the
    /// register, MVSAVE turns whatever the last form produced into a list.
    #[test]
    fn values_can_be_collected() {
        assert_eq!(
            run(&[
                (Op::Fix, 1, 0),
                (Op::Fix, 2, 0),
                (Op::Values, 2, 0),
                (Op::Mvsave, 0, 0),
                (Op::Ret, 0, 0),
            ]),
            "(1 2)"
        );
    }

    /// One value is the ordinary case and must cost nothing: the register
    /// keeps saying UNBOUND, and MVSAVE builds the list only when asked.
    #[test]
    fn one_value_leaves_the_register_alone() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let f = asm(mc, &[(Op::Fix, 9, 0), (Op::Values, 1, 0), (Op::Ret, 0, 0)]);
            let v = run_now(mc, f, 100).expect("runs");
            assert_eq!(crate::printer::print(mc, v), "9");
            assert_eq!(mc.register(Register::Values), Value::UNBOUND);
        });
    }

    /// Calling a primitive through its function cell, which is what
    /// `(funcall #'+ 1 2)` compiles to when the function is a value rather
    /// than a name.
    #[test]
    fn a_primitive_can_be_called_through_its_symbol() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            primitives::install(mc);
            let plus = mc.intern("+");
            run_asm(
                mc,
                &[
                    Ins(Op::Gfref, 0, 0),
                    Ins(Op::Fix, 1, 0),
                    Ins(Op::Fix, 2, 0),
                    Ins(Op::Call, 2, 0),
                    Ins(Op::Ret, 0, 0),
                ],
                &[plus],
            )
        });
        assert_eq!(out, "3");
    }

    /// `(multiple-value-call #'+ (values 1 2) (values 3 4))` => 10.
    #[test]
    fn multiple_value_call_spreads_every_value() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            primitives::install(mc);
            let plus = mc.intern("+");
            run_asm(
                mc,
                &[
                    Ins(Op::Gfref, 0, 0),
                    Ins(Op::Fix, 1, 0),
                    Ins(Op::Fix, 2, 0),
                    Ins(Op::Values, 2, 0),
                    Ins(Op::Mvsave, 0, 0),
                    Ins(Op::Fix, 3, 0),
                    Ins(Op::Fix, 4, 0),
                    Ins(Op::Values, 2, 0),
                    Ins(Op::Mvsave, 0, 0),
                    Ins(Op::Mvcall, 2, 0),
                    Ins(Op::Ret, 0, 0),
                ],
                &[plus],
            )
        });
        assert_eq!(out, "10");
    }

    /// `multiple-value-prog1`: hold the first form's values across the rest.
    #[test]
    fn prog1_holds_its_values_across_what_follows() {
        assert_eq!(
            run(&[
                (Op::Fix, 1, 0),
                (Op::Fix, 2, 0),
                (Op::Values, 2, 0),
                (Op::Mvsave, 0, 0),
                // something else happens, producing one value
                (Op::Fix, 99, 0),
                (Op::Pop, 0, 0),
                (Op::Mvrestore, 0, 0),
                (Op::Mvsave, 0, 0),
                (Op::Ret, 0, 0),
            ]),
            "(1 2)"
        );
    }

    /// Chapter 23's transparency, now a property of the unwinder: a block
    /// left early carries every value out, not just the first.
    #[test]
    fn a_block_is_transparent_to_multiple_values() {
        let mut heap = Heap::new();
        let out = heap.mutate(|mc| {
            let name = mc.intern("OUT");
            run_asm(
                mc,
                &[
                    Mark(Op::Block, 0, "after"),
                    Ins(Op::Fix, 1, 0),
                    Ins(Op::Fix, 2, 0),
                    Ins(Op::Values, 2, 0),
                    Ins(Op::Retfrom, 0, 0),
                    Ins(Op::Unmark, 0, 0),
                    Label("after"),
                    Ins(Op::Mvsave, 0, 0),
                    Ins(Op::Ret, 0, 0),
                ],
                &[name],
            )
        });
        assert_eq!(out, "(1 2)");
    }

    #[test]
    fn a_byte_that_is_not_an_instruction_halts_rather_than_running_on() {
        let mut heap = Heap::new();
        let message = heap.mutate(|mc| {
            let code = mc.alloc_bytes(ObjectType::Code, &[200]);
            let f = mc.alloc(ObjectType::Function, function::SLOTS);
            mc.set_slot(f, function::CODE, code);
            mc.set_slot(f, function::CONSTANTS, Value::NIL);
            mc.set_slot(f, function::MAX_STACK, Value::fixnum(1).expect("small"));
            mc.set_slot(f, function::CODE_LENGTH, Value::fixnum(1).expect("small"));
            match run_now(mc, f, 100) {
                Ok(_) => String::from("ran, which it should not have"),
                Err(e) => format!("{e}"),
            }
        });
        assert!(message.contains("is not an instruction"), "{message}");
    }

    /// A function with no `RET` is a bug in whoever produced it. Running off
    /// the end must say so rather than reading whatever is next in the heap.
    #[test]
    fn running_off_the_end_is_an_error_not_an_adventure() {
        let mut heap = Heap::new();
        let message = heap.mutate(|mc| {
            let f = asm(mc, &[(Op::Fix, 1, 0)]);
            match run_now(mc, f, 100) {
                Ok(_) => String::from("ran, which it should not have"),
                Err(e) => format!("{e}"),
            }
        });
        assert!(message.contains("without returning"), "{message}");
    }

    /// There is no "not implemented" arm left to test. The `match` on `Op`
    /// in `step` is exhaustive, so the compiler is what now guarantees every
    /// opcode in the table does something — which is a better test than this
    /// one was.
    #[test]
    fn every_opcode_is_implemented() {
        for byte in 0..OPCODE_COUNT {
            assert!(Op::from_u8(byte as u8).is_some());
        }
    }
}
