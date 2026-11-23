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
use crate::error::EvalError;
use crate::heap::{symbol, Mutation, ObjectType, Register};
use crate::primitives;
use crate::slice::{Steppable, Transition};
use crate::value::Value;
use crate::{closure, env};

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
    /// A special variable's binding (#40). The datum is the symbol, and the
    /// value it had before is kept in [`SAVED`]: like a pause, a binding has
    /// no frame to restore, so the frame slot holds what it does need.
    pub const SPECIAL: i64 = 7;

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
    /// Where a [`SPECIAL`] record keeps the value the symbol had before.
    pub const SAVED: usize = FP;
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
    ///
    /// Does **not** verify. [`run_now`] does, and so should anything that
    /// loads an image; this is the raw entry point, kept separate so that a
    /// test can run deliberately malformed bytecode and watch what happens.
    pub fn start(mc: &'a mut Mutation<'h>, function: Value<'h>, env: Value<'h>) -> Vm<'a, 'h> {
        // Whatever was in progress is being discarded, and if it had special
        // variables bound — a computation its driver gave up on — their old
        // values come back now, before its stack is thrown away (#40).
        Vm {
            mc: &mut *mc,
            error: None,
            halted: false,
        }
        .unbind_all();
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
        // A request left over from an abandoned computation must not be
        // answered into this one.
        mc.set_register(Register::Request, Value::NIL);
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
    /// [`crate::code::verify`] is that pass, and `run_now` runs it — so for
    /// anything that got that far this assertion cannot fire. It stays as a
    /// belt, because `Vm::start` deliberately does not verify.
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
        // A compiled function on its own is a closure over nothing: what
        // `compile-form` returns, and so what `eval` calls (#47). CL's
        // `compile` returns a function too, not something to wrap first.
        if callee.is_object() && self.mc.header(callee).object_type == ObjectType::Function {
            return Some(callee);
        }
        // A macro as well as a closure: `macroexpand-1` is prelude Lisp that
        // calls an expander with `apply`, and the interpreter has always let
        // it (`closure::is_callable`). Once macros are compiled (#33), the VM
        // must agree, or a compiled compiler could not expand anything.
        if !closure::is_callable(self.mc, callee) {
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
            if closure::is_callable(self.mc, callee) {
                // An interpreted closure in a compiled program. Not a user
                // error and not a missing feature: something has mixed the
                // two evaluators, which #25 says must not happen.
                // Two quite different situations produce this, and the
                // message has to serve both: during the bootstrap the prelude
                // is still interpreted and compiled code simply cannot reach
                // it, and in a built image reaching one means the bootstrap
                // let an expander be compiled while its caller was not.
                return self.fail(EvalError::BadSyntax {
                    operator: "CALL",
                    detail: "reached an interpreted closure — compiled code \
                             can only call compiled code, and the prelude is \
                             not compiled yet",
                });
            }
            // Say *what* was called. "is not a function" without the value is
            // a hint; with it, it is usually the whole diagnosis.
            let what = crate::printer::print(self.mc, callee);
            return self.fail(EvalError::NotAFunction(what));
        };
        // A bare compiled form runs in no scope at all, as `Vm::start` runs
        // one. Binding its empty parameter list would make a scope object,
        // and every `defun` inside would capture it: three words a function,
        // and closures unlike the cold build's (#48).
        let scope = if callee == code_object && args.is_empty() {
            Value::NIL
        } else {
            let params = self.mc.slot(code_object, function::PARAMS);
            let captured = if callee == code_object {
                Value::NIL
            } else {
                self.mc.slot(callee, closure::ENV)
            };
            match self.bind_parameters(params, args, captured) {
                Ok(s) => s,
                Err(e) => return self.fail(e),
            }
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
        self.call_value(callee, &args, tail)
    }

    /// Call `callee` with `args`, whatever kind of thing it is.
    ///
    /// Separate from [`Vm::call`] because `funcall` and `apply` need it too:
    /// each is a primitive whose whole job is to call something else, which a
    /// primitive cannot do, so the machine intercepts them and arrives back
    /// here with a different callee.
    fn call_value(&mut self, callee: Value<'h>, args: &[Value<'h>], tail: bool) -> Transition {
        if let Some(index) = callee.as_primitive() {
            if let Some(transition) = self.intercept(index, args, tail) {
                return transition;
            }
            return match primitives::call_slice(self.mc, index, args) {
                Ok(v) => {
                    if tail {
                        self.mc.set_register(Register::Values, Value::UNBOUND);
                        self.do_return(v)
                    } else {
                        self.push_fresh(v);
                        Transition::Continue
                    }
                }
                Err(e) => match self.arithmetic_fallback(index, args, &e) {
                    Some(generic) => self.call_value(generic, args, tail),
                    None => self.fail(e),
                },
            };
        }
        self.enter(callee, args, tail)
    }

    /// The Lisp function to call when an arithmetic primitive's fixnum fast
    /// path has failed — on overflow, or because an argument is a bignum —
    /// or `None` to report the error as it is (#16, decision 1).
    ///
    /// The symbol is found by name once and then kept in the heap's symbol
    /// cache. Before the library defines the function, the error stands.
    fn arithmetic_fallback(
        &mut self,
        index: u32,
        args: &[Value<'h>],
        error: &EvalError,
    ) -> Option<Value<'h>> {
        let (slot, name) = primitives::fallback(index)?;
        let wanted = matches!(error, EvalError::IntegerOverflow)
            || args.iter().any(|a| crate::bignum::is(self.mc, *a));
        if !wanted {
            return None;
        }
        let symbol = self.mc.cached_symbol(slot, name);
        let function = self.mc.slot(symbol, crate::heap::symbol::FUNCTION);
        if function == Value::UNBOUND {
            None
        } else {
            Some(function)
        }
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
        // `funcall` and `apply` call a function, which is the machine's job
        // and not a primitive's — the same reason `error` and `values` are
        // here. The interpreter intercepts the identical five.
        if index == primitives::index(primitives::Prim::Funcall) {
            let Some((callee, rest)) = args.split_first() else {
                return Some(self.fail(EvalError::WrongArgumentCount {
                    name: "FUNCALL",
                    wanted: 1,
                    got: 0,
                }));
            };
            let (callee, rest) = (*callee, rest.to_vec());
            return Some(self.call_value(callee, &rest, tail));
        }
        if index == primitives::index(primitives::Prim::Apply) {
            // The last argument is a list and is spread; everything between
            // the function and it is passed as written.
            let Some((callee, rest)) = args.split_first() else {
                return Some(self.fail(EvalError::WrongArgumentCount {
                    name: "APPLY",
                    wanted: 2,
                    got: 0,
                }));
            };
            let callee = *callee;
            let Some((last, fixed)) = rest.split_last() else {
                return Some(self.fail(EvalError::WrongArgumentCount {
                    name: "APPLY",
                    wanted: 2,
                    got: 1,
                }));
            };
            let mut spread = fixed.to_vec();
            let mut cursor = *last;
            while cursor.is_cons() {
                spread.push(self.mc.car(cursor));
                cursor = self.mc.cdr(cursor);
            }
            return Some(self.call_value(callee, &spread, tail));
        }
        let asks_host = if index == primitives::index(primitives::Prim::ReadSource) {
            Some("%READ-SOURCE")
        } else if index == primitives::index(primitives::Prim::WriteConsole) {
            Some("%WRITE-CONSOLE")
        } else {
            None
        };
        if let Some(kind) = asks_host {
            // The primitives that do not return. Each leaves a request in the
            // heap and the machine stops; the host answers between slices and
            // the next step picks the answer up (`resume_request`). See
            // `crate::host` for why this is a request and not a call.
            if args.len() != 1 {
                return Some(self.fail(EvalError::WrongArgumentCount {
                    name: kind,
                    wanted: 1,
                    got: args.len(),
                }));
            }
            let name = args[0];
            if !(name.is_object() && self.mc.header(name).object_type.is_string()) {
                return Some(self.fail(EvalError::WrongType {
                    name: kind,
                    wanted: "a string",
                }));
            }
            let request = crate::host::make_request(self.mc, kind, name, tail);
            self.mc.set_register(Register::Request, request);
            return Some(Transition::Block);
        }
        if index == primitives::index(primitives::Prim::Error) {
            let Some((datum, rest)) = args.split_first() else {
                return Some(self.fail(EvalError::WrongArgumentCount {
                    name: "ERROR",
                    wanted: 1,
                    got: 0,
                }));
            };
            let (datum, rest) = (*datum, rest.to_vec());
            return Some(match self.make_condition(datum, &rest) {
                Ok(condition) => self.start_unwind(condition, Value::NIL),
                Err(e) => self.fail(e),
            });
        }
        if index == primitives::index(primitives::Prim::Values)
            || index == primitives::index(primitives::Prim::ValuesList)
        {
            let values: Vec<Value<'h>> = if index == primitives::index(primitives::Prim::Values) {
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

    /// `(error "text")`, `(error 'some-type)` or `(error condition)`, as a
    /// condition object.
    ///
    /// Must agree with the interpreter's `build_condition`, and until #36 did
    /// not, twice over. Given a condition, CLHS says `error` signals *that*
    /// condition — which is what re-signalling one a clause did not want
    /// relies on — and this wrapped it in a fresh `simple-error`, so its type
    /// was lost and no outer clause for it could match. And any other datum
    /// became a `simple-error` where the interpreter signals a `type-error`.
    fn make_condition(
        &mut self,
        datum: Value<'h>,
        rest: &[Value<'h>],
    ) -> Result<Value<'h>, EvalError> {
        if crate::condition::is_condition(self.mc, datum) {
            return Ok(datum);
        }
        let mut data = Value::NIL;
        for v in rest.iter().rev() {
            data = self.mc.cons(*v, data);
        }
        if self.mc.is_symbol(datum) {
            return Ok(crate::condition::make(self.mc, datum, Value::NIL, data));
        }
        if datum.is_object() && self.mc.header(datum).object_type.is_string() {
            let simple = self.mc.intern("SIMPLE-ERROR");
            return Ok(crate::condition::make(self.mc, simple, datum, data));
        }
        Err(EvalError::WrongType {
            name: "ERROR",
            wanted: "a condition, a symbol naming one, or a string",
        })
    }

    // ---- unwind records ----------------------------------------------------

    /// Push a record, remembering where to carry on if an unwind finds it.
    /// Bind SYMBOL dynamically to VALUE: a record holding its old value,
    /// which `UNMARK` and the unwinder put back. `DBIND` and `DBIND-SYMBOL`.
    fn bind_special(&mut self, symbol: Value<'h>, value: Value<'h>) {
        let old = self.mc.slot(symbol, crate::heap::symbol::VALUE);
        self.push_mark(mark::SPECIAL, symbol, 0);
        let at = self.mc.mp();
        let stack = self.mc.register(Register::Operands);
        self.mc.set_slot(stack, at + mark::SAVED, old);
        self.mc.set_slot(symbol, crate::heap::symbol::VALUE, value);
    }

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
        self.restore_binding(at);
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

        if self.mark_matches(at, target) && kind == mark::TAG {
            // A `go` has arrived, and unlike every other landing **the record
            // stays**: the tagbody is still running, and the next `go` — the
            // next trip round the loop — needs it, with the same marker, so a
            // closure made on an earlier trip still finds it. Before #37 this
            // shared the code below, popped the record, and so a tagbody could
            // not loop.
            //
            // Everything above the record goes; the record's PC is the start
            // of a jump table, one `JMP` per tag, and the index the `GO`
            // carried picks the entry. A computed goto, done the classic way.
            let index = self
                .mc
                .register(Register::Value)
                .as_fixnum()
                .unwrap_or(0)
                .max(0) as usize;
            let fp = self.read_index(at, mark::FP);
            let table = self.read_index(at, mark::PC);
            let function = self.mark_slot(at, mark::FUNCTION);
            let env = self.mark_slot(at, mark::ENV);
            self.drop_to(at + mark::SIZE);
            self.mc.set_mp(at);
            self.mc.set_fp(fp);
            self.mc.set_pc(table + index * Op::Jmp.width());
            self.mc.set_register(Register::Function, function);
            self.mc.set_register(Register::Env, env);
            self.mc.set_register(Register::Unwind, Value::UNBOUND);
            self.mc.set_register(Register::Values, Value::UNBOUND);
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
            if kind == mark::HANDLER {
                // A handler receives the **condition**: its compiled clause
                // dispatch has to look at it to decide whether it was wanted.
                // Not the `Value` register, which is what a block or catch
                // receives and which `error` leaves as `nil` — so until #36 a
                // compiled handler was handed `nil` and could not dispatch at
                // all. One value, so `push_fresh`.
                self.push_fresh(target);
            } else {
                // `push`, not `push_fresh`: a `(return-from b (values 1 2))`
                // carries both values out with it, exactly as chapter 23 says
                // a block is transparent to them.
                self.push(value);
            }
            return Transition::Continue;
        }

        // Not this one. Drop it and look further out, one step at a time —
        // putting back a special variable's old value if it is a binding,
        // which is all an unwind passing a binding has to do.
        self.restore_binding(at);
        self.drop_to(at);
        self.mc.set_mp(prev);
        Transition::Continue
    }

    // ---- stepping ----------------------------------------------------------

    /// Stop, or signal — whichever the program can do something about.
    ///
    /// The interpreter's rule, exactly (`eval::Machine::fail`): if the error is
    /// a fault in the program *and* a `handler-case` is open, it becomes a
    /// condition and unwinds to it; otherwise the machine halts carrying it.
    /// Checking for a handler first means no condition is allocated for an
    /// error nobody can catch, including running out of heap.
    ///
    /// Until #36 this always halted, so an error raised inside a primitive —
    /// `(car 5)` — could not be caught by compiled code at all.
    ///
    /// One distinction the interpreter does not need. In compiled code a
    /// `BadSyntax` always means the **bytecode** is wrong: the compiler checked
    /// the program's syntax before any of it ran, so what is left for the VM
    /// to find is an opcode that is not one, a jump before the start, a
    /// missing constant, a compiled caller meeting an interpreted closure.
    /// Those are never the program's to catch, for the reason `Malformed` is
    /// not — a `handler-case` would hide a compiler bug behind whatever the
    /// program does about errors. Decided here by variant, in one place,
    /// rather than at each of the eleven sites that raise one.
    fn fail(&mut self, e: EvalError) -> Transition {
        let the_programs = !matches!(e, EvalError::BadSyntax { .. });
        if the_programs && self.has_handler() {
            if let Some(condition) = crate::condition::from_eval_error(self.mc, &e) {
                return self.start_unwind(condition, Value::NIL);
            }
        }
        self.unbind_all();
        self.error = Some(e);
        self.halted = true;
        Transition::Halt
    }

    /// Is a `handler-case` record open anywhere on the stack?
    ///
    /// A walk of the record chain, as the interpreter walks its frames. Linear
    /// in the number of open records — `block`s and `catch`es as well as
    /// handlers — which stays small in any program a person writes.
    fn has_handler(&self) -> bool {
        let mut at = self.mc.mp();
        while at != usize::MAX {
            if self.mark_slot(at, mark::KIND).as_fixnum() == Some(mark::HANDLER) {
                return true;
            }
            at = self.read_index(at, mark::PREV);
        }
        false
    }

    /// Pick up the host's answer, or go on waiting for it.
    ///
    /// The answer goes exactly where the primitive's return value would have
    /// gone had it returned at once: pushed, or returned from the function if
    /// the call was in tail position. A failure is signalled as a condition,
    /// through the same unwinding `error` uses, so a handler can catch it.
    fn resume_request(&mut self, record: Value<'h>) -> Transition {
        use crate::host::request;
        let state = self.mc.slot(record, request::STATE);
        if self.mc.symbol_is(state, request::WAITING) {
            return Transition::Block;
        }
        let reply = self.mc.slot(record, request::REPLY);
        let tail = self.mc.slot(record, request::TAIL) != Value::NIL;
        self.mc.set_register(Register::Request, Value::NIL);
        if self.mc.symbol_is(state, request::FAILED) {
            return self.start_unwind(reply, Value::NIL);
        }
        if tail {
            self.mc.set_register(Register::Values, Value::UNBOUND);
            self.do_return(reply)
        } else {
            self.push_fresh(reply);
            Transition::Continue
        }
    }

    /// If the record at `at` is a special variable's binding, put the
    /// symbol's old value back. Anything else is left alone.
    fn restore_binding(&mut self, at: usize) {
        if self.mark_slot(at, mark::KIND).as_fixnum() == Some(mark::SPECIAL) {
            let symbol = self.mark_slot(at, mark::DATUM);
            let old = self.mark_slot(at, mark::SAVED);
            self.mc.set_slot(symbol, crate::heap::symbol::VALUE, old);
        }
    }

    /// Undo every special binding still on the stack, innermost first.
    ///
    /// For a computation that will not finish: one halted by an error nothing
    /// caught, or abandoned by its driver. Without this, a special variable
    /// bound when the machine stopped would stay rebound for ever, because the
    /// only things that restore it are the ways out the machine never took.
    fn unbind_all(&mut self) {
        let mut at = self.mc.mp();
        while at != usize::MAX {
            self.restore_binding(at);
            at = self.read_index(at, mark::PREV);
        }
        self.mc.set_mp(usize::MAX);
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
        // Before unwinding, though the two cannot coincide: a request is made
        // only from ordinary execution, and while one is waiting nothing else
        // happens. Checked on every step, which costs one register read.
        let request = self.mc.register(Register::Request);
        if request != Value::NIL {
            return self.resume_request(request);
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
            Op::Lset | Op::Fset => {
                let namespace = if decoded.op == Op::Lset {
                    env::VARIABLE
                } else {
                    env::FUNCTION
                };
                match self.slot_of(decoded.a, decoded.b, namespace) {
                    Ok((scope, at)) => {
                        let v = self.pop();
                        self.mc.set_slot(scope, at, v);
                        self.push_fresh(v);
                    }
                    Err(e) => return self.fail(e),
                }
            }
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
            Op::Block => {
                // A fresh, unforgeable identity. `ObjectType::Instance` with
                // no slots, which is what chapter 20 made conditions stop
                // sharing — a marker must never be mistaken for one.
                let marker = self.mc.alloc(ObjectType::Instance, 0);
                let name = match self.constant(decoded.a) {
                    Ok(v) => v,
                    Err(e) => return self.fail(e),
                };
                // The record first, so that it saves the environment *outside*
                // the block — which is what both ways out must restore. It
                // used to be pushed after the block's scope was bound, so
                // `UNMARK` and the unwinder both put that scope back, and
                // every compiled `block` left its name bound behind it:
                // `(let ((x 5)) (block b 1) x)` failed with the chain check.
                // Nothing noticed until a test read a variable after a block
                // (#37).
                let resume = (decoded.next as i64 + decoded.b as i64) as usize;
                self.push_mark(mark::BLOCK, marker, resume);
                let parent = self.mc.register(Register::Env);
                let scope = env::make(self.mc, parent, env::BLOCK, &[name], &[marker]);
                self.mc.set_register(Register::Env, scope);
            }
            Op::Tag => {
                // One marker for **every** tag of the tagbody, bound under each
                // tag's name, so a `go` to any of them finds this tagbody and
                // its slot index says which tag. The interpreter's
                // `eval_tagbody` does the same, and that settled the shape.
                let marker = self.mc.alloc(ObjectType::Instance, 0);
                let names = match self.constant(decoded.a) {
                    Ok(v) => v,
                    Err(e) => return self.fail(e),
                };
                if !names.is_object() || self.mc.header(names).object_type != ObjectType::Vector {
                    return self.fail(EvalError::BadSyntax {
                        operator: "TAG",
                        detail: "names a constant that is not a vector of tags",
                    });
                }
                let count = self.mc.header(names).slots;
                let tags: Vec<_> = (0..count).map(|i| self.mc.slot(names, i)).collect();
                let markers = vec![marker; count];
                let parent = self.mc.register(Register::Env);
                let scope = env::make(self.mc, parent, env::TAG, &tags, &markers);
                self.mc.set_register(Register::Env, scope);
                // The record *after* the scope — the opposite of `BLOCK`, and
                // for the opposite reason. A `return-from` resumes outside its
                // block; a `go` resumes inside its tagbody, among the forms
                // that can see the tags. So the record saves the inside, and
                // the normal way out is `UNMARK` and then `UNBIND 1`.
                let table = (decoded.next as i64 + decoded.b as i64) as usize;
                self.push_mark(mark::TAG, marker, table);
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
                // `return-from` carries a value. `go` carries **which tag**:
                // its slot index in the tagbody's scope, which the landing
                // turns into an entry of the jump table.
                let v = if decoded.op == Op::Retfrom {
                    self.pop()
                } else {
                    Value::fixnum(decoded.b as i64).expect("a u8 operand is a fixnum")
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
            Op::Dbind => {
                let symbol = match self.constant(decoded.a) {
                    Ok(s) if self.mc.is_symbol(s) => s,
                    Ok(_) => {
                        return self.fail(EvalError::BadSyntax {
                            operator: "DBIND",
                            detail: "names a constant that is not a symbol",
                        })
                    }
                    Err(e) => return self.fail(e),
                };
                let value = self.pop();
                self.bind_special(symbol, value);
            }
            Op::DbindSymbol => {
                let value = self.pop();
                let symbol = self.pop();
                if !self.mc.is_symbol(symbol) {
                    return self.fail(EvalError::WrongType {
                        name: "PROGV",
                        wanted: "a symbol",
                    });
                }
                self.bind_special(symbol, value);
            }
            Op::JmpTag => {
                // The fast path of `go`: the innermost record is the tagbody's
                // own, and it holds the stack base and the environment to
                // return to. See `Op::JmpTag` for why the compiler need count
                // nothing. Checked here as well as by the verifier, because a
                // wrong record would leave the machine in a state nothing else
                // would notice.
                let at = self.mc.mp();
                if at == usize::MAX || self.mark_slot(at, mark::KIND).as_fixnum() != Some(mark::TAG)
                {
                    return self.fail(EvalError::BadSyntax {
                        operator: "JMP-TAG",
                        detail: "the innermost record is not a tagbody's",
                    });
                }
                let env = self.mark_slot(at, mark::ENV);
                self.drop_to(at + mark::SIZE);
                self.mc.set_register(Register::Env, env);
                self.mc.set_register(Register::Values, Value::UNBOUND);
                return self.jump(decoded.next, decoded.a);
            }
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
                // Through `call_value`, not straight to the primitive table,
                // so that the primitives the machine must do itself are
                // intercepted here as on every other path. This arm used to
                // call the table directly, which meant
                // `(multiple-value-call #'funcall ...)` handed `funcall` to a
                // dispatcher that cannot call anything — found when
                // `%read-source` would have fallen through the same gap (#35).
                return self.call_value(callee, &args, false);
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
    // Once, before anything runs, exactly as the JVM verifies a class when it
    // is loaded rather than checking on every pop. A function that gets past
    // this cannot underflow the stack, cannot jump into the middle of an
    // instruction, and cannot need a deeper stack than it reserved.
    if let Err(e) = code::verify::function(mc, function) {
        return Err(EvalError::Malformed(format!("{e:?}")));
    }
    let mut vm = Vm::start(mc, function, Value::NIL);
    match run_slice(&mut vm, budget) {
        Status::Done { .. } => match vm.error.take() {
            Some(e) => Err(e),
            None => Ok(vm.result()),
        },
        Status::Yielded => Err(EvalError::BudgetExhausted),
        // There is no host here to answer it, and inside one mutation there
        // could not be: answering needs the heap between slices.
        Status::Blocked { .. } => Err(EvalError::Blocked),
    }
}

/// Run the compiled function in the `Value` register a slice at a time,
/// answering its requests through `host`, and leave the answer in `Value`.
///
/// The bytecode VM's `eval::eval_sliced`: each slice its own mutation, so the
/// collector runs during a computation and not only after it (#32). And the
/// place a synchronous host meets the request protocol — when a slice ends
/// [`crate::slice::Status::Blocked`], [`crate::host::serve`] answers it and the next slice
/// carries on. A browser host would not use this; it runs the same loop in
/// JavaScript, with a Promise where `serve` is.
///
/// Verifies first, as [`run_now`] does.
pub fn run_sliced(
    heap: &mut crate::heap::Heap,
    budget: u64,
    slice: u64,
    host: &mut dyn crate::host::Host,
) -> Result<(), EvalError> {
    use crate::slice::{run_slice, Status};
    let refused = heap.mutate(|mc| {
        let function = mc.register(Register::Value);
        if let Err(e) = code::verify::function(mc, function) {
            return Some(EvalError::Malformed(format!("{e:?}")));
        }
        Vm::start(mc, function, Value::NIL);
        None
    });
    if let Some(e) = refused {
        return Err(e);
    }
    let mut spent = 0u64;
    while spent < budget {
        spent += slice;
        let (status, error) = heap.mutate(|mc| {
            let mut vm = Vm::resume(mc);
            let status = run_slice(&mut vm, slice);
            (status, vm.error.take())
        });
        if let Some(e) = error {
            return Err(e);
        }
        match status {
            Status::Done { .. } => return Ok(()),
            Status::Yielded => {}
            Status::Blocked { .. } => {
                if !crate::host::serve(heap, host) {
                    return Err(EvalError::Blocked);
                }
            }
        }
    }
    Err(EvalError::BudgetExhausted)
}

/// Call the global function `name` with one argument — whatever is in the
/// `Value` register — on the bytecode VM, and leave its result there.
///
/// How a host calls Lisp without an interpreter. The REPL used to compile a
/// form by having the *interpreter* evaluate `(compile-form 'FORM)`, which
/// cannot work once `compile-form` is compiled: the interpreter refuses to
/// call compiled code, by design. So this assembles a four-instruction
/// function by hand — a **trampoline**, the usual name for a small piece of
/// code whose only job is to jump into something else —
///
/// ```text
///     GFREF  0      the function cell of `name`
///     CONST  1      the argument
///     CALL   1
///     RET
/// ```
///
/// and runs it with [`run_sliced`]. The trampoline is verified like any other
/// function before it runs.
pub fn call_global(
    heap: &mut crate::heap::Heap,
    name: &str,
    budget: u64,
    slice: u64,
    host: &mut dyn crate::host::Host,
) -> Result<(), EvalError> {
    heap.mutate(|mc| {
        let argument = mc.register(Register::Value);
        let symbol = mc.intern(name);
        let constants = mc.alloc(ObjectType::Vector, 2);
        mc.set_slot(constants, 0, symbol);
        mc.set_slot(constants, 1, argument);
        let mut bytes = Vec::new();
        code::encode(&mut bytes, Op::Gfref, 0, 0);
        code::encode(&mut bytes, Op::Const, 1, 0);
        code::encode(&mut bytes, Op::Call, 1, 0);
        code::encode(&mut bytes, Op::Ret, 0, 0);
        let label = mc.intern("CALL-FROM-HOST");
        let trampoline = code::make_function(mc, &bytes, constants, Value::NIL, 2, label);
        mc.set_register(Register::Value, trampoline);
    });
    run_sliced(heap, budget, slice, host)
}

/// What one form came to when a host ran it through [`eval_source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The form's value, as `prin1` wrote it.
    Value(String),
    /// A Lisp error, caught by the loop: its message.
    Error(String),
    /// What the loop could not catch, such as the step budget running out.
    Failed(EvalError),
}

/// Run every form of `source` through a built image, as its REPL does (#47):
/// read by the Lisp reader in `*package*`, evaluated by `eval`, printed by
/// `prin1`. The loop is `%host-step` in `lisp/library/eval.lisp`; this only
/// calls it until the input runs out, handing each [`Outcome`] to `each`,
/// which says whether to go on. Unlike `quokka_bootstrap::boot::compile_and_run_forms`,
/// the Rust reader and printer play no part.
pub fn eval_source(
    heap: &mut crate::heap::Heap,
    source: &str,
    budget: u64,
    slice: u64,
    host: &mut dyn crate::host::Host,
    mut each: impl FnMut(Outcome) -> bool,
) -> Result<(), EvalError> {
    heap.mutate(|mc| {
        let text = mc.alloc_string(source);
        mc.set_register(Register::Value, text);
    });
    call_global(heap, "%HOST-SOURCE", budget, slice, host)?;
    loop {
        heap.mutate(|mc| mc.set_register(Register::Value, Value::NIL));
        let outcome = match call_global(heap, "%HOST-STEP", budget, slice, host) {
            Err(e) => Outcome::Failed(e),
            Ok(()) => {
                let step = heap.mutate(|mc| {
                    let result = mc.register(Register::Value);
                    let kind = mc.car(result);
                    if mc.symbol_is(kind, "EOF") {
                        return None;
                    }
                    let text = mc.string_to_rust(mc.car(mc.cdr(result)));
                    Some(if mc.symbol_is(kind, "VALUE") {
                        Outcome::Value(text)
                    } else {
                        Outcome::Error(text)
                    })
                });
                match step {
                    Some(outcome) => outcome,
                    None => return Ok(()),
                }
            }
        };
        if !each(outcome) {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "unit/vm.rs"]
mod tests;
