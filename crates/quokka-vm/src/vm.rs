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
        mc.set_register(Register::Unwind, Value::NIL);
        mc.set_pc(0);
        mc.set_sp(0);
        mc.set_fp(0);
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
            let what = if closure::is(self.mc, callee) {
                // An interpreted closure in a compiled program. Not a user
                // error and not a missing feature: something has mixed the
                // two evaluators, which #25 says must not happen.
                "an interpreted closure reached compiled code, which means the \
                 bootstrap let an expander be compiled while its caller was not"
            } else {
                "is not a compiled function"
            };
            return self.fail(EvalError::BadSyntax {
                operator: "CALL",
                detail: what,
            });
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
                match primitives::call_slice(self.mc, decoded.a as u32, &args) {
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
            other => {
                return self.fail(EvalError::BadSyntax {
                    operator: other.name(),
                    detail: "is specified but not implemented in the VM yet",
                })
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
    use crate::code::encode;
    use crate::heap::Heap;
    use crate::slice::{run_slice, Status};

    /// Primitive indices, pinned where they are used so that a reordering of
    /// the table shows up here rather than as arithmetic that is quietly
    /// wrong. `primitives::the_table_order_is_pinned` is the other half.
    const ADD: i32 = 0;
    const SUB: i32 = 1;
    const MUL: i32 = 2;

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

    /// Everything specified but not yet built says so by name, rather than
    /// doing nothing and leaving a wrong answer to be explained later.
    #[test]
    fn an_unimplemented_opcode_names_itself() {
        let message = run(&[(Op::Mvcall, 1, 0)]);
        assert!(message.contains("MVCALL"), "{message}");
        assert!(message.contains("not implemented"), "{message}");
    }
}
