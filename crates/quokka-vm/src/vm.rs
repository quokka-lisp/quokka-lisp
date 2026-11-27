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

/// How many words a call frame's header takes: the argument count, the
/// call's token, the callee, and the saved program counter, frame pointer,
/// function and environment. The frame pointer points just above it.
///
/// The arguments themselves sit below the header, as the call passed them
/// (#130). The callee does not read them there: they have become an
/// `Environment` by the time the frame exists, because variables live on the
/// chain rather than in frame slots — one allocation per call, which
/// `docs/design.md` records as the price of keeping one environment
/// representation. They are kept for the debugger, which shows what a frame
/// was called with and can call it again: kept on the stack, which a call
/// writes anyway, rather than in a list, which would be an allocation per
/// call. `frame` names the header's slots.
const FRAME: usize = frame::SIZE;

/// A call frame, from below:
///
/// ```text
///    argument 0 .. argument n-1   as the call passed them
///    n                            ARGUMENT_COUNT
///    nil, or the call's token     TOKEN: made when the frame is first read
///    the callee                   CALLEE: what was called, before a
///                                 funcallable instance is opened
///    saved pc                     PC
///    saved frame pointer          FP
///    saved function               FUNCTION: the caller's
///    saved environment            ENV: the caller's
///    ...the callee's operands     <- frame pointer
/// ```
///
/// Offsets are from the header's base, `fp - SIZE`.
pub mod frame {
    /// How many arguments lie below the header.
    pub const ARGUMENT_COUNT: usize = 0;
    /// `nil` until something reads the frame: then a fresh cons that names
    /// this call and no other (`vm::frames`). A place on the stack is
    /// reused by every call made there, and a later call of the same
    /// function at the same depth has the same place and callee, so the
    /// token is what tells a debugger's old frame from a new one. Made only
    /// when asked for, so that a call pays one store for it.
    pub const TOKEN: usize = 1;
    /// The function called, as the caller had it.
    pub const CALLEE: usize = 2;
    /// Where the caller carries on.
    pub const PC: usize = 3;
    /// The caller's frame pointer.
    pub const FP: usize = 4;
    /// The caller's function.
    pub const FUNCTION: usize = 5;
    /// The caller's environment.
    pub const ENV: usize = 6;
    /// How many slots the header takes.
    pub const SIZE: usize = 7;
}

/// The frame whose pointer is FP: the base of its header and its argument
/// count, or `None` if FP is not a frame's pointer that the stack can hold.
///
/// Every read of a frame goes through here, so that a damaged stack — one
/// in an image made by hand — is an error and never an index out of range
/// (rule 10).
fn frame_bounds(mc: &Mutation<'_>, fp: usize) -> Option<(usize, usize)> {
    let stack = mc.register(Register::Operands);
    if !stack.is_object() || mc.header(stack).object_type != ObjectType::Vector {
        return None;
    }
    if fp > mc.header(stack).slots {
        return None;
    }
    open_frame(mc, fp)
}

/// `frame_bounds` for the machine itself, which always has a stack vector
/// with the stack pointer inside it (`Heap::verify` checks that much of an
/// image, and nothing moves either out of it): the checks a call and a
/// return need, and no more, since they are the commonest things it does.
fn open_frame(mc: &Mutation<'_>, fp: usize) -> Option<(usize, usize)> {
    if fp < FRAME || fp > mc.sp() {
        return None;
    }
    let stack = mc.register(Register::Operands);
    let base = fp - FRAME;
    let n = mc.slot(stack, base + frame::ARGUMENT_COUNT).as_fixnum()?;
    let n = usize::try_from(n).ok()?;
    (n <= base).then_some((base, n))
}

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
    /// Set while the machine is calling the library's
    /// `%signal-machine-error` for an error of its own, so that a failure
    /// in making that call halts instead of trying the call again
    /// (`Vm::fail_with`).
    signalling: bool,
    /// Set when the running process has unwound to its base because it was
    /// told to end (`terminate_process`), so its halt is that and not an
    /// error.
    terminating: bool,
}

/// What a process told to end unwinds towards: a symbol, which no record
/// matches — a `block` or `tag` wants its marker, a `catch` a cons, a
/// `handler-case` a condition or a two-slot vector — so the unwind runs
/// every cleanup and reaches the bottom of the process's stack (#34).
const TERMINATE: &str = "%TERMINATE-PROCESS";

/// The library's special that marks a region in which a process is not to
/// be ended yet (`%without-interrupts` in `threads.lisp`).
const DEFERRED: &str = "%INTERRUPTS-DEFERRED";

/// What the machine knows about the error it is raising, beyond the error
/// itself: the Lisp values a condition's initargs are made from (#125,
/// sub-step 3), which an `EvalError` cannot hold since it outlives the heap.
#[derive(Clone)]
enum Culprit<'h> {
    /// Nothing more.
    Nothing,
    /// The name of a variable or function: a `cell-error`'s.
    Name(Value<'h>),
    /// A value that was not a function, called.
    NotAFunction(Value<'h>),
    /// A primitive, by its index, and the arguments it failed on.
    Call(u32, Vec<Value<'h>>),
}

/// Whether the machine has stopped and why, and how full the heap is. Its
/// stack, its program counter and everything else it is doing are in the
/// heap's registers, where a debugger written in Lisp will read them.
impl std::fmt::Debug for Vm<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vm")
            .field("halted", &self.halted)
            .field("error", &self.error)
            .field("heap", &self.mc)
            .finish()
    }
}

impl<'a, 'h> Vm<'a, 'h> {
    /// Prepare to run `function` in `env`, discarding whatever was in
    /// progress.
    ///
    /// Does **not** verify. [`run_now`] does, and so should anything that
    /// loads an image; this is the raw entry point, kept separate so that a
    /// test can run deliberately malformed bytecode and watch what happens.
    pub fn start(mc: &'a mut Mutation<'h>, function: Value<'h>, env: Value<'h>) -> Vm<'a, 'h> {
        // A host's computation runs in the main process (#34): whichever
        // process was running when the last one ended is put away first,
        // with its registers, and the main process's are the ones replaced.
        crate::process::focus_main(mc);
        // Whatever was in progress is being discarded, and if it had special
        // variables bound — a computation its driver gave up on — their old
        // values come back now, before its stack is thrown away (#40).
        Vm {
            mc: &mut *mc,
            error: None,
            halted: false,
            signalling: false,
            terminating: false,
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
            signalling: false,
            terminating: false,
        }
    }

    /// Give up the computation in the registers, and let go of all it held.
    ///
    /// For a host whose computation failed in a way no handler could catch.
    /// [`Vm::start`] does as much when the next one begins — but a heap that
    /// ran out needs the dead computation's stack gone *before* anything
    /// else is tried, since that stack is what holds the heap full (#85).
    pub fn abandon(mc: &'a mut Mutation<'h>) {
        let empty = code::make_function(mc, &[], Value::NIL, Value::NIL, 0, Value::NIL);
        Vm::start(mc, empty, Value::NIL);
    }

    /// Continue a computation already in the registers.
    pub fn resume(mc: &'a mut Mutation<'h>) -> Vm<'a, 'h> {
        Vm {
            mc,
            error: None,
            halted: false,
            signalling: false,
            terminating: false,
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
    ///
    /// Rule 9: growing copies the whole stack inside one step. It doubles,
    /// so the copies add up to less than the pushes that caused them
    /// (*amortised* constant time, as a growing vector's), but one of them is
    /// as long as the stack is deep. That is left as it is, on purpose
    /// (#102): the stack is an object in the heap, and the collector already
    /// copies it, with everything else alive, in one pause. A copy here can
    /// never be longer than that pause, so the bound that decides when the
    /// collector must become incremental (#86: a pause measured over 16 ms)
    /// covers this too. A stack in segments would bound it separately, and
    /// would make every frame reference two numbers instead of one.
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
    ///
    /// The belt is one comparison, in every build, against the *frame*: a
    /// pop at `sp == fp` would take the caller's saved registers, which a
    /// test of `sp > 0` never noticed, and in a release build even that test
    /// was gone and `sp - 1` wrapped (#94). It stops the machine with a
    /// fault; the instruction that popped finishes on a `nil` and nothing
    /// runs after it.
    fn pop(&mut self) -> Value<'h> {
        let sp = self.mc.sp();
        if sp <= self.mc.fp() {
            self.error.get_or_insert(EvalError::Fault {
                operator: "a compiled function",
                detail: "popped below its own frame",
            });
            self.halted = true;
            return Value::NIL;
        }
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
        EvalError::Fault {
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
            return Err(EvalError::Fault {
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
            return Err(EvalError::Fault {
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
    ///
    /// CALLED is what the caller called, which the frame keeps: CALLEE
    /// itself, or the funcallable instance (a generic function) whose
    /// function CALLEE is.
    fn enter(
        &mut self,
        called: Value<'h>,
        callee: Value<'h>,
        args: &[Value<'h>],
        tail: bool,
    ) -> Transition {
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
                return self.fail(EvalError::Fault {
                    operator: "CALL",
                    detail: "reached an interpreted closure — compiled code \
                             can only call compiled code, and the prelude is \
                             not compiled yet",
                });
            }
            // Say *what* was called. "is not a function" without the value is
            // a hint; with it, it is usually the whole diagnosis.
            let what = crate::printer::print(self.mc, callee);
            return self.fail_with(EvalError::NotAFunction(what), Culprit::NotAFunction(callee));
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
                // Say which function, now that a function knows its name:
                // "this function wants 2 arguments" in a program of fifty
                // functions names none of them (#110).
                Err(EvalError::WrongArgumentCount { wanted, got, .. }) => {
                    let name = self.mc.slot(code_object, function::NAME);
                    // A local function's name is a list, `(flet f)` (#130).
                    let function = if self.mc.is_symbol(name) && !self.mc.symbol_is(name, "LAMBDA")
                    {
                        self.mc.symbol_name(name)
                    } else if name.is_cons() {
                        crate::printer::print(self.mc, name)
                    } else {
                        String::from("this function")
                    };
                    return self.fail(EvalError::WrongArgumentsTo {
                        function,
                        wanted,
                        got,
                    });
                }
                Err(e) => return self.fail(e),
            }
        };
        let wanted = self
            .mc
            .slot(code_object, function::MAX_STACK)
            .as_fixnum()
            .unwrap_or(0)
            .max(0) as usize;

        let fp = self.mc.fp();
        if tail && fp == 0 {
            // The function the machine was started on has no frame of its
            // own (`Vm::start`), so there is no call to replace: its tail
            // callee runs frameless too, and returning from it halts.
            self.drop_to(0);
        } else {
            // The caller's state: the registers for a call, or for a tail
            // call the header of the frame being reused, whose caller is the
            // one the tail callee returns to.
            let saved = if tail {
                let Some((base, n)) = open_frame(self.mc, fp) else {
                    return self.damaged_frame("TCALL");
                };
                let stack = self.mc.register(Register::Operands);
                let header = [
                    self.mc.slot(stack, base + frame::PC),
                    self.mc.slot(stack, base + frame::FP),
                    self.mc.slot(stack, base + frame::FUNCTION),
                    self.mc.slot(stack, base + frame::ENV),
                ];
                self.drop_to(base - n);
                header
            } else {
                let pc = self.mc.pc();
                [
                    Value::fixnum(pc as i64).expect("a code offset is a fixnum"),
                    Value::fixnum(fp as i64).expect("a stack offset is a fixnum"),
                    self.mc.register(Register::Function),
                    self.mc.register(Register::Env),
                ]
            };
            // Written in one go, after one check for room: a call is the
            // commonest thing the machine does, and a push checks for room
            // each time.
            self.ensure(args.len() + FRAME + wanted);
            let stack = self.mc.register(Register::Operands);
            let mut sp = self.mc.sp();
            let count = Value::fixnum(args.len() as i64).expect("an argument count is small");
            for word in args
                .iter()
                .copied()
                .chain([count, Value::NIL, called])
                .chain(saved)
            {
                self.mc.set_slot(stack, sp, word);
                sp += 1;
            }
            self.mc.set_sp(sp);
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
            // Its first character, read where the name is: copying the name
            // into a Rust `String` to look at one character was an
            // allocation for every parameter of every call (#102).
            let text = self.mc.slot(name, crate::heap::symbol::NAME);
            if self.mc.string_length(text) > 0 && self.mc.string_char(text, 0) == '&' {
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
        let Some((base, n)) = open_frame(self.mc, fp) else {
            return self.damaged_frame("RET");
        };
        let stack = self.mc.register(Register::Operands);
        let pc = self
            .mc
            .slot(stack, base + frame::PC)
            .as_fixnum()
            .unwrap_or(0) as usize;
        let caller_fp = self
            .mc
            .slot(stack, base + frame::FP)
            .as_fixnum()
            .unwrap_or(0) as usize;
        let function = self.mc.slot(stack, base + frame::FUNCTION);
        let env = self.mc.slot(stack, base + frame::ENV);
        self.drop_to(base - n);
        self.mc.set_pc(pc);
        self.mc.set_fp(caller_fp);
        self.mc.set_register(Register::Function, function);
        self.mc.set_register(Register::Env, env);
        self.push(v);
        Transition::Continue
    }

    /// A frame pointer that does not point above a frame's header: only an
    /// image made by hand can do it, and it is the machine's fault, not the
    /// program's.
    fn damaged_frame(&mut self, operator: &'static str) -> Transition {
        self.fail(EvalError::Fault {
            operator,
            detail: "the frame pointer does not point at a frame",
        })
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
        let callee = match closure::designated(self.mc, callee) {
            Ok(function) => function,
            Err(e) => return self.fail_with(e, Culprit::Name(callee)),
        };
        // A funcallable instance, such as a generic function, is called
        // through the function it holds, with the same arguments (#124). Its
        // function is never another funcallable instance (the setter refuses
        // one), so one replacement is enough.
        let called = callee;
        let callee = if crate::instance::is_funcallable(self.mc, callee) {
            let function = crate::instance::function(self.mc, callee);
            if !crate::instance::is_plain_function(self.mc, function) {
                return self.fail(EvalError::NotAFunction(String::from(
                    "#<funcallable-instance with no function>",
                )));
            }
            function
        } else {
            callee
        };
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
                Err(e) => match self.fallback(index, args, &e) {
                    Some(generic) => self.call_value(generic, args, tail),
                    None => self.fail_with(e, Culprit::Call(index, args.to_vec())),
                },
            };
        }
        self.enter(called, callee, args, tail)
    }

    /// The Lisp function to call when an arithmetic primitive's fixnum fast
    /// path has failed — on overflow, or because an argument is a bignum —
    /// or a hash table primitive met a key the library compares (#71); or
    /// `None` to report the error as it is (#16, decision 1).
    ///
    /// The symbol is found by name once and then kept in the heap's symbol
    /// cache. Before the library defines the function, the error stands.
    fn fallback(&mut self, index: u32, args: &[Value<'h>], error: &EvalError) -> Option<Value<'h>> {
        let (slot, name) = primitives::fallback(index)?;
        let wanted = matches!(error, EvalError::IntegerOverflow)
            || args
                .iter()
                .any(|a| crate::bignum::is(self.mc, *a) || crate::ratio::is(self.mc, *a))
            || (args.len() > 1 && crate::hash::needs_lisp(self.mc, args[1], args[0]));
        if !wanted {
            return None;
        }
        let symbol = self.mc.cached_symbol(slot, name);
        let function = self.mc.slot(symbol, crate::heap::symbol::FUNCTION);
        // A primitive is no fallback: it would fail as this one did and fall
        // back again, each time one Rust frame deeper, until the stack ran
        // out — `(setf (symbol-function 'quokka::%generic-+) #'+)` did it
        // (#93). The library's fallbacks are Lisp, which is entered through
        // a frame in the heap, under the step budget.
        if function == Value::UNBOUND || function.as_primitive().is_some() {
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
            let Some((mut elements, _)) = self.mc.list_elements(*last) else {
                return Some(self.fail(primitives::circular("APPLY")));
            };
            spread.append(&mut elements);
            return Some(self.call_value(callee, &spread, tail));
        }
        let asks_host = if index == primitives::index(primitives::Prim::ReadSource) {
            Some("%READ-SOURCE")
        } else if index == primitives::index(primitives::Prim::WriteConsole) {
            Some("%WRITE-CONSOLE")
        } else if index == primitives::index(primitives::Prim::ReadConsole) {
            Some("%READ-CONSOLE")
        } else if index == primitives::index(primitives::Prim::Display) {
            Some("%DISPLAY")
        } else if index == primitives::index(primitives::Prim::AwaitEvent) {
            Some("%AWAIT-EVENT")
        } else {
            None
        };
        if let Some(kind) = asks_host {
            // The primitives that do not return. Each leaves a request in the
            // heap and the machine stops; the host answers between slices and
            // the next step picks the answer up (`resume_request`). See
            // `crate::host` for why this is a request and not a call. Each
            // takes a string, but `%await-event`, which takes nothing.
            let wanted = usize::from(kind != "%AWAIT-EVENT");
            if args.len() != wanted {
                return Some(self.fail(EvalError::WrongArgumentCount {
                    name: kind,
                    wanted,
                    got: args.len(),
                }));
            }
            let name = args.first().copied().unwrap_or(Value::NIL);
            if wanted == 1 && !(name.is_object() && self.mc.header(name).object_type.is_string()) {
                return Some(self.fail(EvalError::WrongType {
                    name: kind,
                    wanted: "a string",
                }));
            }
            let request = crate::host::make_request(self.mc, kind, name, tail);
            self.mc.set_register(Register::Request, request);
            return Some(Transition::Block);
        }
        if index == primitives::index(primitives::Prim::UnwindToHandler) {
            return Some(self.unwind_to_handler(args));
        }
        if index == primitives::index(primitives::Prim::UnwindToFrame) {
            return Some(self.unwind_to_frame(args));
        }
        if index == primitives::index(primitives::Prim::ProcessYield) {
            return Some(self.yield_process(tail));
        }
        if index == primitives::index(primitives::Prim::ProcessTerminate) {
            return Some(self.terminate_process(args, tail));
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
                // Intercepted before the table's arity check, so it is made
                // here; and a dotted list is not a list of values (#76).
                if args.len() != 1 {
                    return Some(self.fail(EvalError::WrongArgumentCount {
                        name: "VALUES-LIST",
                        wanted: 1,
                        got: args.len(),
                    }));
                }
                let Some((out, cursor)) = self.mc.list_elements(args[0]) else {
                    return Some(self.fail(primitives::circular("VALUES-LIST")));
                };
                if !cursor.is_nil() {
                    return Some(self.fail(EvalError::WrongType {
                        name: "VALUES-LIST",
                        wanted: "a proper list",
                    }));
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
            // `(error 'type-error "why" datum)` is how the library signals a
            // typed condition, and the text is the first thing after the
            // type. It was dropped, so every such error read "; NIL" (#110).
            // The arguments stay whole in the data, for whoever reads them.
            let message = match rest.first() {
                Some(text) if text.is_object() && self.mc.header(*text).object_type.is_string() => {
                    *text
                }
                _ => Value::NIL,
            };
            return Ok(crate::condition::make(self.mc, datum, message, data));
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
            return self.fail(EvalError::Fault {
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
            // tag may usefully be is the same as identity. Only a *throw*
            // looks for one, and a throw's target is its tag in a cons —
            // see `Op::Throw` for why.
            mark::CATCH => target.is_cons() && datum == self.mc.car(target),
            // A handler record stops *any* condition, and the compiled
            // clause dispatch it resumes into decides whether this one is
            // wanted — re-signalling if not, which searches further out
            // because this record has already been popped.
            //
            // Which keeps the judgement in Lisp. Deciding here would mean
            // the VM knowing what a `handler-case` clause looks like, and
            // rule 1 says that belongs above the primitive table.
            //
            // Since step B (#125) a world with the library signals in Lisp,
            // and `signal` itself chooses the `handler-case` whose clauses
            // want the condition, then unwinds to that record alone: the
            // target is a vector of the record's place and the condition
            // (`Vm::unwind_to_handler`).
            mark::HANDLER => {
                let _ = datum;
                crate::condition::is_condition(self.mc, target)
                    || self.handler_target(target) == Some(at)
            }
            _ => false,
        }
    }

    /// One step of unwinding.
    fn step_unwind(&mut self) -> Transition {
        let target = self.mc.register(Register::Unwind);
        let at = self.mc.mp();
        // An unwind to a frame (#130) has arrived when no record is left
        // that the frame or anything it called pushed: those all lie above
        // its frame pointer. Until then it passes each record as a `throw`
        // passes a record that is not its catch, below.
        if let Some(place) = self.frame_target(target) {
            if at == usize::MAX || at < place {
                return self.arrive_at_frame(target, place);
            }
        }
        if at == usize::MAX {
            // A process told to end has run every cleanup and is at its
            // base: it ends here (#34).
            if self.mc.symbol_is(target, TERMINATE) {
                self.mc.set_register(Register::Unwind, Value::UNBOUND);
                self.terminating = true;
                return self.halt_with(Value::NIL);
            }
            // Nothing wanted it. A throw to a tag nobody is catching, or a
            // condition nobody is handling.
            let value = self.mc.register(Register::Value);
            self.mc.set_register(Register::Unwind, Value::UNBOUND);
            let _ = value;
            if crate::condition::is_condition(self.mc, target) {
                let what = crate::condition::describe(self.mc, target);
                return self.fail(EvalError::Signalled(what));
            }
            let what = if target.is_cons() {
                crate::printer::print(self.mc, self.mc.car(target))
            } else {
                "that block or tag, which has already been left".to_string()
            };
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
                let condition = if self.handler_target(target).is_some() {
                    self.mc.slot(target, 1)
                } else {
                    target
                };
                self.push_fresh(condition);
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
    /// One distinction: a [`Fault`](EvalError::Fault) means the **bytecode**
    /// or the machine is wrong — an opcode that is not one, a jump before
    /// the start, a missing constant, a compiled caller meeting an
    /// interpreted closure — and is never the program's to catch, for the
    /// reason `Malformed` is not: a `handler-case` would hide a compiler bug
    /// behind whatever the program does about errors. Decided here by
    /// variant, in one place. `BadSyntax` is the program's own syntax, such
    /// as a lambda list `(lambda (5) …)`, and is a `program-error` (#97).
    fn fail(&mut self, e: EvalError) -> Transition {
        self.fail_with(e, Culprit::Nothing)
    }

    /// [`Vm::fail`], knowing more about the error (#125, sub-step 3).
    ///
    /// Once the library defines `%signal-machine-error`, an error that is
    /// the program's becomes a call of it, **at the point of the error**:
    /// with the condition's type, the machine's message, and a property
    /// list of what the machine knows, the variable's name, the value
    /// called, or the primitive and its arguments. The library makes the
    /// condition and signals it like any other, so `handler-bind` sees
    /// `(car 5)` with the frames that failed still on the stack, and the
    /// floor of a running handler (CLHS 9.1.4.1) is respected. The call is
    /// the arithmetic fallback's technique (`Vm::fallback`); the function
    /// never returns, since `error` does not.
    ///
    /// Before the library is loaded, and in the interpreter, the old way
    /// stands: a condition made here, unwinding to the innermost
    /// `handler-case`.
    fn fail_with(&mut self, e: EvalError, culprit: Culprit<'h>) -> Transition {
        let the_programs = !matches!(e, EvalError::Fault { .. } | EvalError::Malformed(_));
        if the_programs && !self.signalling {
            if let Some(handler) = self.machine_error_handler() {
                if let Some((type_name, message)) = crate::condition::type_and_message(&e) {
                    let args = self.machine_error_arguments(type_name, &message, &e, culprit);
                    self.signalling = true;
                    let transition = self.call_value(handler, &args, false);
                    self.signalling = false;
                    return transition;
                }
            }
        }
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

    /// The library's `%signal-machine-error`, or `None` before it exists.
    fn machine_error_handler(&mut self) -> Option<Value<'h>> {
        let symbol = self.mc.cached_symbol(
            crate::heap::symbol_cache::MACHINE_ERROR,
            "%SIGNAL-MACHINE-ERROR",
        );
        let function = self.mc.slot(symbol, symbol::FUNCTION);
        if function == Value::UNBOUND || function.as_primitive().is_some() {
            None
        } else {
            Some(function)
        }
    }

    /// `(type message context)`: what `%signal-machine-error` is called
    /// with. CONTEXT is a property list of what the machine knows: `:name`,
    /// `:datum` and `:expected-type`, or `:operation` and `:operands`.
    fn machine_error_arguments(
        &mut self,
        type_name: &str,
        message: &str,
        e: &EvalError,
        culprit: Culprit<'h>,
    ) -> Vec<Value<'h>> {
        let type_symbol = self.mc.intern(type_name);
        let text = self.mc.alloc_string(message);
        let mut context: Vec<Value<'h>> = Vec::new();
        match culprit {
            Culprit::Nothing => {}
            Culprit::Name(name) => {
                context.push(self.mc.intern_keyword("NAME"));
                context.push(name);
            }
            Culprit::NotAFunction(value) => {
                // `(or function symbol)`, a function designator, as SBCL says.
                let or = self.mc.intern("OR");
                let function = self.mc.intern("FUNCTION");
                let symbol = self.mc.intern("SYMBOL");
                let expected = self.mc.cons(symbol, Value::NIL);
                let expected = self.mc.cons(function, expected);
                let expected = self.mc.cons(or, expected);
                context.push(self.mc.intern_keyword("DATUM"));
                context.push(value);
                context.push(self.mc.intern_keyword("EXPECTED-TYPE"));
                context.push(expected);
            }
            Culprit::Call(index, args) => {
                if let EvalError::IndexOutOfBounds { index: bad, length } = e {
                    // `(integer 0 (length))`, SBCL's expected type for an index.
                    let integer = self.mc.intern("INTEGER");
                    let zero = Value::fixnum(0).expect("small");
                    let below = Value::fixnum(*length as i64).unwrap_or(Value::NIL);
                    let bound = self.mc.cons(below, Value::NIL);
                    let rest = self.mc.cons(bound, Value::NIL);
                    let rest = self.mc.cons(zero, rest);
                    let expected = self.mc.cons(integer, rest);
                    context.push(self.mc.intern_keyword("DATUM"));
                    context.push(Value::fixnum(*bad).unwrap_or(Value::NIL));
                    context.push(self.mc.intern_keyword("EXPECTED-TYPE"));
                    context.push(expected);
                }
                let operation = primitives::name_of(index).map(|name| self.mc.intern(name));
                let mut operands = Value::NIL;
                for v in args.iter().rev() {
                    operands = self.mc.cons(*v, operands);
                }
                context.push(self.mc.intern_keyword("OPERATION"));
                context.push(operation.unwrap_or(Value::NIL));
                context.push(self.mc.intern_keyword("OPERANDS"));
                context.push(operands);
            }
        }
        let mut list = Value::NIL;
        for v in context.into_iter().rev() {
            list = self.mc.cons(v, list);
        }
        vec![type_symbol, text, list]
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

    /// Is the block or tagbody that made MARKER still open? The same walk.
    fn has_exit(&self, marker: Value<'h>) -> bool {
        let mut at = self.mc.mp();
        while at != usize::MAX {
            let kind = self.mark_slot(at, mark::KIND).as_fixnum();
            if matches!(kind, Some(mark::BLOCK | mark::TAG))
                && self.mark_slot(at, mark::DATUM) == marker
            {
                return true;
            }
            at = self.read_index(at, mark::PREV);
        }
        false
    }

    /// Is a `catch` for TAG open anywhere on the stack? The same walk.
    fn has_catch(&self, tag: Value<'h>) -> bool {
        let mut at = self.mc.mp();
        while at != usize::MAX {
            if self.mark_slot(at, mark::KIND).as_fixnum() == Some(mark::CATCH)
                && self.mark_slot(at, mark::DATUM) == tag
            {
                return true;
            }
            at = self.read_index(at, mark::PREV);
        }
        false
    }

    /// `(%unwind-to-handler place condition)`: unwind to the `handler-case`
    /// record at PLACE, which `signal` has found wants CONDITION, running
    /// every cleanup on the way, as any unwind does. The record is checked
    /// to be open first: a place `%handler-marks` did not give is the
    /// program's mistake, and an unwind that found nothing would halt.
    fn unwind_to_handler(&mut self, args: &[Value<'h>]) -> Transition {
        if args.len() != 2 {
            return self.fail(EvalError::WrongArgumentCount {
                name: "%UNWIND-TO-HANDLER",
                wanted: 2,
                got: args.len(),
            });
        }
        let place = match args[0].as_fixnum() {
            Some(n) if n >= 0 => n as usize,
            _ => usize::MAX,
        };
        let mut at = self.mc.mp();
        while at != usize::MAX && at != place {
            at = self.read_index(at, mark::PREV);
        }
        if at == usize::MAX || self.mark_slot(at, mark::KIND).as_fixnum() != Some(mark::HANDLER) {
            return self.fail(EvalError::WrongType {
                name: "%UNWIND-TO-HANDLER",
                wanted: "the place of an open handler-case",
            });
        }
        let target = self.mc.alloc(ObjectType::Vector, 2);
        self.mc.set_slot(target, 0, args[0]);
        self.mc.set_slot(target, 1, args[1]);
        self.start_unwind(target, Value::NIL)
    }

    /// The place of the `handler-case` record TARGET is unwinding to, if it
    /// is the target `unwind_to_handler` makes: a two-slot vector holding a
    /// place and a condition. Nothing else unwinds towards a vector.
    fn handler_target(&self, target: Value<'h>) -> Option<usize> {
        if target.is_object() {
            let header = self.mc.header(target);
            if header.object_type == ObjectType::Vector && header.slots == 2 {
                return match self.mc.slot(target, 0).as_fixnum() {
                    Some(n) if n >= 0 => Some(n as usize),
                    _ => None,
                };
            }
        }
        None
    }

    /// `(%unwind-to-frame place token thunk)`: unwind to the frame at
    /// PLACE, running every cleanup and undoing every special binding made
    /// inside it, then call THUNK in the frame's place, so that its values
    /// are the frame's, returned to the frame's caller (#130). SBCL's
    /// `sb-debug:unwind-to-frame-and-call`, and how a frame is retried.
    ///
    /// The frame must still be open and still be the call `%frames` gave
    /// TOKEN for: a place is only a stack index, and a frame that has
    /// returned leaves its place to the next call made there, which has a
    /// token of its own or none (`frame::TOKEN`). Checked by walking the
    /// chain of frames, as `has_catch` walks the records for a `throw`,
    /// linear in the frames open.
    fn unwind_to_frame(&mut self, args: &[Value<'h>]) -> Transition {
        if args.len() != 3 {
            return self.fail(EvalError::WrongArgumentCount {
                name: "%UNWIND-TO-FRAME",
                wanted: 3,
                got: args.len(),
            });
        }
        let wanted = match args[0].as_fixnum() {
            Some(n) if n > 0 => n as usize,
            _ => usize::MAX,
        };
        let mut fp = self.mc.fp();
        let mut open = false;
        while let Some((base, _)) = frame_bounds(self.mc, fp) {
            let stack = self.mc.register(Register::Operands);
            if fp == wanted {
                open = args[1].is_cons() && self.mc.slot(stack, base + frame::TOKEN) == args[1];
                break;
            }
            match self.mc.slot(stack, base + frame::FP).as_fixnum() {
                Some(caller) if caller >= 0 && (caller as usize) < fp => fp = caller as usize,
                _ => break,
            }
        }
        if !open {
            return self.fail(EvalError::NoSuchExit(String::from(
                "that frame, which has already returned",
            )));
        }
        let target = self.mc.alloc(ObjectType::Vector, 3);
        for (i, v) in args.iter().enumerate() {
            self.mc.set_slot(target, i, *v);
        }
        self.start_unwind(target, Value::NIL)
    }

    /// The frame pointer an unwind towards TARGET is looking for, if it is
    /// the target `unwind_to_frame` makes: a three-slot vector of place,
    /// token and thunk. `handler_target`'s vectors have two slots, and
    /// nothing else unwinds towards a vector.
    fn frame_target(&self, target: Value<'h>) -> Option<usize> {
        if target.is_object() {
            let header = self.mc.header(target);
            if header.object_type == ObjectType::Vector && header.slots == 3 {
                return match self.mc.slot(target, 0).as_fixnum() {
                    Some(n) if n > 0 => Some(n as usize),
                    _ => None,
                };
            }
        }
        None
    }

    /// The unwind to the frame at PLACE has passed every record inside it:
    /// pop the frame as a return would, and call the thunk where it was
    /// called, so that the thunk returns to the frame's caller.
    fn arrive_at_frame(&mut self, target: Value<'h>, place: usize) -> Transition {
        let Some((base, n)) = frame_bounds(self.mc, place) else {
            return self.damaged_frame("%UNWIND-TO-FRAME");
        };
        let stack = self.mc.register(Register::Operands);
        let pc = self
            .mc
            .slot(stack, base + frame::PC)
            .as_fixnum()
            .unwrap_or(0) as usize;
        let caller_fp = self
            .mc
            .slot(stack, base + frame::FP)
            .as_fixnum()
            .unwrap_or(0) as usize;
        let function = self.mc.slot(stack, base + frame::FUNCTION);
        let env = self.mc.slot(stack, base + frame::ENV);
        let thunk = self.mc.slot(target, 2);
        self.drop_to(base - n);
        self.mc.set_pc(pc);
        self.mc.set_fp(caller_fp);
        self.mc.set_register(Register::Function, function);
        self.mc.set_register(Register::Env, env);
        self.mc.set_register(Register::Unwind, Value::UNBOUND);
        self.mc.set_register(Register::Values, Value::UNBOUND);
        self.call_value(thunk, &[], false)
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
            // The host made one of the machine's own conditions. With the
            // library loaded it is signalled in Lisp, as every error is.
            if crate::condition::is_condition(self.mc, reply) {
                if let Some(handler) = self.machine_error_handler() {
                    let type_name = self.mc.slot(reply, crate::condition::TYPE);
                    let message = self.mc.slot(reply, crate::condition::MESSAGE);
                    let message =
                        if message.is_object() && self.mc.header(message).object_type.is_string() {
                            message
                        } else {
                            self.mc.alloc_string("the host could not do it")
                        };
                    return self.call_value(handler, &[type_name, message, Value::NIL], false);
                }
            }
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
            return Err(EvalError::Fault {
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
            return self.fail(EvalError::Fault {
                operator: "a jump",
                detail: "lands before the start of the code",
            });
        }
        self.mc.set_pc(target as usize);
        Transition::Continue
    }
}

impl Vm<'_, '_> {
    /// One step of the running process, as if it were the only one.
    fn step_here(&mut self) -> Transition {
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
                return self.fail(EvalError::Fault {
                    operator: "bytecode",
                    detail: "is not an instruction",
                })
            }
            Err(CodeError::Truncated { .. }) => {
                // Running off the end means the function had no RET, which is
                // a bug in whatever produced it rather than in the program.
                return self.fail(EvalError::Fault {
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
                        return self.fail_with(EvalError::UnboundVariable(name), Culprit::Name(s));
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
                        return self
                            .fail_with(EvalError::UndefinedFunction(name), Culprit::Name(s));
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
                    return self.fail(EvalError::Fault {
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
                // A fresh, unforgeable identity: a `Marker`, which nothing
                // else is — not a condition (chapter 20), not a structure.
                let marker = self.mc.alloc(ObjectType::Marker, 0);
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
                let marker = self.mc.alloc(ObjectType::Marker, 0);
                let names = match self.constant(decoded.a) {
                    Ok(v) => v,
                    Err(e) => return self.fail(e),
                };
                if !names.is_object() || self.mc.header(names).object_type != ObjectType::Vector {
                    return self.fail(EvalError::Fault {
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
                let (marker, name) = match self.slot_of(decoded.a, decoded.b, namespace) {
                    Ok((scope, at)) => (self.mc.slot(scope, at), self.mc.slot(scope, at - 1)),
                    Err(e) => return self.fail(e),
                };
                // The block or tagbody may have been left already — a closure
                // that outlived it and then tried to return from it. That is
                // a `control-error` signalled *here*, before anything
                // unwinds, as `throw`'s is below: found only at the bottom of
                // the stack, every handler that could have caught it was
                // already gone, and the message could name only an internal
                // marker (#97).
                if !self.has_exit(marker) {
                    let what = crate::printer::print(self.mc, name);
                    return self.fail(EvalError::NoSuchExit(what));
                }
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
                // A throw to a tag nobody is catching is a `control-error`
                // signalled where the throw is, before anything unwinds, so
                // a handler around it still exists to catch it (CLHS
                // `throw`; ansi-test throw-error, #80). Found only after
                // unwinding everything, it could only stop the machine.
                if !self.has_catch(tag) {
                    let what = crate::printer::print(self.mc, tag);
                    return self.fail(EvalError::NoSuchExit(what));
                }
                // One register says where an unwind is going, and three
                // kinds of thing go: a `return-from` or `go` to a marker, a
                // signalled condition, and a throw to a tag. A tag may be any
                // object at all, a condition included, so a bare tag could
                // not be told from a condition being signalled: a handler
                // caught a `throw`, and a `catch` caught an error (#96). The
                // throw therefore travels as its tag in a cons, which is
                // neither a marker nor a condition, and each record matches
                // only its own kind. Phase 3's restarts and `signal` will be
                // further kinds, and need the same care.
                let target = self.mc.cons(tag, Value::NIL);
                return self.start_unwind(target, v);
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
                        return self.fail(EvalError::Fault {
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
                    return self.fail(EvalError::Fault {
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
                    return self.fail(EvalError::Fault {
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
                    let Some((mut elements, _)) = self.mc.list_elements(list) else {
                        return self.fail(primitives::circular("MULTIPLE-VALUE-CALL"));
                    };
                    args.append(&mut elements);
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

impl Steppable for Vm<'_, '_> {
    /// One step of the machine: of the running process, and then, when
    /// there are processes (#34), whatever the scheduler does about how it
    /// went. A process that finished, or blocked on the host while another
    /// can run, gives way to the next in the ring, and the slice goes on in
    /// that one; only the main process halts the machine, since its
    /// computation is the one the host asked for. With no processes the
    /// step is exactly what it was.
    fn step(&mut self) -> Transition {
        if self.mc.register(Register::Process) == Value::NIL {
            return self.step_here();
        }
        self.deliver_termination();
        let transition = self.step_here();
        match transition {
            Transition::Continue => Transition::Continue,
            Transition::Halt => {
                let running = self.mc.register(Register::Process);
                if self.mc.slot(running, crate::process::MAIN) != Value::NIL {
                    return Transition::Halt;
                }
                self.end_process();
                self.give_way(true)
            }
            Transition::Block => self.give_way(false),
        }
    }
}

impl<'h> Vm<'_, 'h> {
    /// The running process, not the main one, has halted: record how, and
    /// clear the halt, which was its own and not the machine's.
    fn end_process(&mut self) {
        let (state, result) = if self.terminating {
            (crate::process::TERMINATED, Value::NIL)
        } else if let Some(e) = self.error.take() {
            let message = self.mc.alloc_string(&e.to_string());
            (crate::process::FAILED, message)
        } else {
            let values = self.mc.register(Register::Values);
            let list = if values == Value::UNBOUND {
                let v = self.mc.register(Register::Value);
                self.mc.cons(v, Value::NIL)
            } else {
                values
            };
            (crate::process::FINISHED, list)
        };
        self.terminating = false;
        self.error = None;
        self.halted = false;
        crate::process::end(self.mc, state, result);
    }

    /// Switch to the next process that can run, and carry on in it. If none
    /// can, the running one ended (ENDED) or is waiting on the host: the
    /// machine blocks, in the main process if the running one has ended,
    /// so that the host answers whoever is waiting.
    fn give_way(&mut self, ended: bool) -> Transition {
        let leaving = self.mc.register(Register::Process);
        let transition = if let Some(next) = crate::process::next_runnable(self.mc) {
            crate::process::switch_to(self.mc, next);
            Transition::Continue
        } else {
            if ended {
                crate::process::focus_main(self.mc);
            }
            Transition::Block
        };
        if ended {
            crate::process::unlink(self.mc, leaving);
        }
        transition
    }

    /// Let the next process that can run have a turn, if one can: the end
    /// of a slice, and `%process-yield`.
    pub(crate) fn preempt(&mut self) {
        if self.mc.register(Register::Process) == Value::NIL {
            return;
        }
        if let Some(next) = crate::process::next_runnable(self.mc) {
            crate::process::switch_to(self.mc, next);
        }
    }

    /// `(%process-terminate process)`: make PROCESS unwind to its base,
    /// running its cleanups in its own context, and end (#34). Its request,
    /// if it was waiting on the host, is withdrawn. The main process is the
    /// host's and cannot be ended; an ended one is left as it is. Answers t
    /// if the process was told to end.
    fn terminate_process(&mut self, args: &[Value<'h>], tail: bool) -> Transition {
        let Some(&target) = args.first() else {
            return self.fail(EvalError::WrongArgumentCount {
                name: "%PROCESS-TERMINATE",
                wanted: 1,
                got: 0,
            });
        };
        if !crate::process::is(self.mc, target) {
            return self.fail(EvalError::WrongType {
                name: "%PROCESS-TERMINATE",
                wanted: "a process",
            });
        }
        let told = self.mc.slot(target, crate::process::MAIN) == Value::NIL
            && crate::process::alive(self.mc, target);
        if told {
            // Asked, not done: the machine ends it at its next step at which
            // that is safe (`Vm::step`). A process waiting on the host is
            // given up waiting, so that it runs again and can end.
            self.mc
                .set_slot(target, crate::process::TERMINATE, Value::T);
            if target != self.mc.register(Register::Process) {
                self.mc
                    .set_slot(target, crate::process::REQUEST, Value::NIL);
            }
        }
        let answer = if told { Value::T } else { Value::NIL };
        if tail {
            self.mc.set_register(Register::Values, Value::UNBOUND);
            self.do_return(answer)
        } else {
            self.push_fresh(answer);
            Transition::Continue
        }
    }

    /// Begin ending the running process if it has been told to and it is
    /// safe now (#34): not while a cleanup an unwind is running has not
    /// finished — the innermost record is the unwind's paused state — nor
    /// inside a region the library marks with `%without-interrupts` (its
    /// special `%interrupts-deferred` true), which it puts around taking and
    /// letting go of a lock and around a channel's bookkeeping. Without
    /// this, an end that fell in a cleanup skipped the rest of it, and a
    /// mutex being let go stayed held by a thread that no longer existed;
    /// the review of the channels found it. SBCL defers its interrupts in
    /// the same way (`without-interrupts`).
    fn deliver_termination(&mut self) {
        let running = self.mc.register(Register::Process);
        if self.mc.slot(running, crate::process::TERMINATE) == Value::NIL || self.is_unwinding() {
            return;
        }
        let at = self.mc.mp();
        if at != usize::MAX && self.mark_slot(at, mark::KIND).as_fixnum() == Some(mark::PAUSED) {
            return;
        }
        let deferred = self.mc.intern(DEFERRED);
        let value = self.mc.slot(deferred, crate::heap::symbol::VALUE);
        if value != Value::NIL && value != Value::UNBOUND {
            return;
        }
        self.mc
            .set_slot(running, crate::process::TERMINATE, Value::NIL);
        self.mc.set_register(Register::Request, Value::NIL);
        let end = self.mc.intern(TERMINATE);
        self.start_unwind(end, Value::NIL);
    }

    /// `(%process-yield)`: let the next process that can run have its turn
    /// now, rather than at the end of the slice. Answers nil.
    fn yield_process(&mut self, tail: bool) -> Transition {
        let transition = if tail {
            self.mc.set_register(Register::Values, Value::UNBOUND);
            self.do_return(Value::NIL)
        } else {
            self.push_fresh(Value::NIL);
            Transition::Continue
        };
        if matches!(transition, Transition::Continue) {
            self.preempt();
        }
        transition
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
    begin(heap)?;
    drive(heap, budget, slice, host)
}

/// Run the computation in the registers to its end, a slice at a time,
/// answering its requests from `host` as they come.
fn drive(
    heap: &mut crate::heap::Heap,
    budget: u64,
    slice: u64,
    host: &mut dyn crate::host::Host,
) -> Result<(), EvalError> {
    use crate::slice::Status;
    let mut spent = 0u64;
    while spent < budget {
        match one_slice(heap, slice)? {
            Status::Done { .. } => return Ok(()),
            // A slice that stopped to ask the host something is charged the
            // steps it ran, not the slice it was offered: charged the whole
            // slice, 2,500 console writes used up five million steps having
            // taken a few thousand (#95). At least one, so that a program
            // which only ever asks still reaches the budget.
            Status::Blocked { steps } => {
                spent += steps.max(1);
                if !answer(heap, host, &mut spent) {
                    return Err(EvalError::Blocked);
                }
            }
            // A process parked on the host while others ran (#34) is
            // answered between slices too, not only once all of them wait.
            Status::Yielded => {
                spent += slice;
                if crate::host::pending(heap).is_some() {
                    answer(heap, host, &mut spent);
                }
            }
        }
    }
    Err(EvalError::BudgetExhausted)
}

/// Answer the request a process is waiting on, through HOST; whether there
/// was one.
///
/// A line typed at the console begins the next piece of work, and gets a
/// budget of its own (#131): the budget stands in for an interrupt nobody
/// can press yet, and a debugging session runs entirely inside the line that
/// failed, so without this it would run out mid-session. Only a line: the
/// end of the input renews nothing, or a program reading it in a loop could
/// never be stopped.
fn answer(heap: &mut crate::heap::Heap, host: &mut dyn crate::host::Host, spent: &mut u64) -> bool {
    if let Some(crate::host::Request::ReadConsole(prompt)) = crate::host::pending(heap) {
        let mains = heap.mutate(|mc| crate::process::main_is_asking(mc));
        let line = host.read_line(&prompt);
        if line.is_some() && mains {
            *spent = 0;
        }
        crate::host::complete_line(heap, line)
    } else {
        crate::host::serve(heap, host)
    }
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
    begin_global(heap, name)?;
    drive(heap, budget, slice, host)
}

/// Verify the function in the `Value` register and make it the computation.
/// `(%handler-marks clusters floor)`: what `signal` must search, innermost
/// first. Each entry is `(0 place . clauses)` for a `handler-case` record and
/// `(1 place . cluster)` for a binding of the symbol CLUSTERS, which
/// `handler-bind` binds to one cluster of handlers (`conditions.lisp`).
///
/// Both kinds are records on the one mark chain, so one walk gives them in
/// their true dynamic order, which is what lets a `handler-case` compiled by
/// tier 0 and a `handler-bind` of the library nest as CLHS says (#125,
/// decision 2). The value a binding established is read from the binding
/// above it, which saved it, or from the symbol for the innermost.
///
/// While a handler runs, only the handlers outside its cluster are active
/// (CLHS 9.1.4.1). `signal` binds the symbol FLOOR to the place of the
/// cluster around each handler call; a binding of FLOOR hides every record
/// at or above that place, except those pushed after the binding itself,
/// which belong to the handler. A hidden binding still passes on its saved
/// value, so the walk knows what the visible bindings below it established.
///
/// A walk of the whole chain, as `Vm::has_handler` makes, linear in the
/// records open; it allocates only for the entries it returns.
pub fn handler_marks<'h>(
    mc: &mut Mutation<'h>,
    clusters: Value<'h>,
    floor: Value<'h>,
) -> Result<Value<'h>, EvalError> {
    if !mc.is_symbol(clusters) || !mc.is_symbol(floor) {
        return Err(EvalError::WrongType {
            name: "%HANDLER-MARKS",
            wanted: "two symbols",
        });
    }
    let stack = mc.register(Register::Operands);
    let read = |mc: &Mutation<'h>, at: usize, slot: usize| mc.slot(stack, at + slot);
    let place = |v: Value<'h>| match v.as_fixnum() {
        Some(n) if n >= 0 => n as usize,
        _ => usize::MAX,
    };
    let mut cluster = mc.slot(clusters, symbol::VALUE);
    let mut floor_value = mc.slot(floor, symbol::VALUE);
    let mut hidden_from = usize::MAX;
    let mut found: Vec<(i64, usize, Value<'h>)> = Vec::new();
    let mut at = mc.mp();
    while at != usize::MAX {
        let visible = at < hidden_from;
        match read(mc, at, mark::KIND).as_fixnum() {
            Some(mark::HANDLER) if visible => found.push((0, at, read(mc, at, mark::DATUM))),
            Some(mark::SPECIAL) => {
                let bound = read(mc, at, mark::DATUM);
                let saved = read(mc, at, mark::SAVED);
                if bound == clusters {
                    if visible && cluster != Value::UNBOUND {
                        found.push((1, at, cluster));
                    }
                    cluster = saved;
                } else if bound == floor {
                    if visible && place(floor_value) != usize::MAX {
                        hidden_from = place(floor_value);
                    }
                    floor_value = saved;
                }
            }
            _ => {}
        }
        at = place(read(mc, at, mark::PREV));
    }
    let mut list = Value::NIL;
    for (kind, at, datum) in found.into_iter().rev() {
        let at = Value::fixnum(at as i64).expect("a stack index is a fixnum");
        let rest = mc.cons(at, datum);
        let kind = Value::fixnum(kind).expect("small");
        let entry = mc.cons(kind, rest);
        list = mc.cons(entry, list);
    }
    Ok(list)
}

/// `(%frames)`: every open frame, innermost first, each as
/// `(place token callee environment . arguments)` — where its frame pointer
/// is, the token that names this call (made now if nothing has asked
/// before; see `frame::TOKEN`), what was called, the lexical environment it
/// was in when it called the frame above it (the one saved in that frame's
/// header; for the innermost, the current one), and the arguments as the
/// call passed them (#130, #131).
///
/// The innermost is the frame of the function that called `%frames`, since
/// a primitive has no frame. The function the machine was started on has
/// none either (`Vm::start`), so the walk ends above it.
///
/// A walk down the chain of saved frame pointers, linear in the frames open,
/// as `handler_marks` is in the records; it allocates only what it returns.
/// Each saved pointer must be below the one before, so a damaged stack ends
/// the walk rather than looping.
pub fn frames<'h>(mc: &mut Mutation<'h>) -> Result<Value<'h>, EvalError> {
    type Found<'h> = (usize, Value<'h>, Value<'h>, Value<'h>, Vec<Value<'h>>);
    let mut found: Vec<Found<'h>> = Vec::new();
    let mut fp = mc.fp();
    let mut environment = mc.register(Register::Env);
    while let Some((base, n)) = frame_bounds(mc, fp) {
        let mut token = mc.slot(mc.register(Register::Operands), base + frame::TOKEN);
        if !token.is_cons() {
            token = mc.cons(Value::NIL, Value::NIL);
            let stack = mc.register(Register::Operands);
            mc.set_slot(stack, base + frame::TOKEN, token);
        }
        let stack = mc.register(Register::Operands);
        let callee = mc.slot(stack, base + frame::CALLEE);
        let arguments = (base - n..base).map(|i| mc.slot(stack, i)).collect();
        found.push((fp, token, callee, environment, arguments));
        environment = mc.slot(stack, base + frame::ENV);
        match mc.slot(stack, base + frame::FP).as_fixnum() {
            Some(caller) if caller >= 0 && (caller as usize) < fp => fp = caller as usize,
            _ => break,
        }
    }
    let mut list = Value::NIL;
    for (place, token, callee, environment, arguments) in found.into_iter().rev() {
        let mut entry = Value::NIL;
        for v in arguments.into_iter().rev() {
            entry = mc.cons(v, entry);
        }
        entry = mc.cons(environment, entry);
        entry = mc.cons(callee, entry);
        entry = mc.cons(token, entry);
        let place = Value::fixnum(place as i64).expect("a stack index is a fixnum");
        entry = mc.cons(place, entry);
        list = mc.cons(entry, list);
    }
    Ok(list)
}

/// `(%frame-locals environment callee)`: the variables a frame can see that
/// are its own, as `(name . value)`, innermost first (#131): those of every
/// variable scope in ENVIRONMENT down to the scope CALLEE closed over,
/// where the variables of the code around the function begin. Scopes of
/// other namespaces (functions, blocks, tags) are passed over, and so is a
/// slot not yet given its value.
///
/// Linear in the scopes, which are as many as the function's nested
/// binding forms. Anything that is not a scope ends the walk, so a value
/// that is not an environment is simply one with no variables.
pub fn frame_locals<'h>(
    mc: &mut Mutation<'h>,
    environment: Value<'h>,
    callee: Value<'h>,
) -> Result<Value<'h>, EvalError> {
    let mut function = callee;
    if crate::instance::is_funcallable(mc, function) {
        function = crate::instance::function(mc, function);
    }
    let stop = if function.is_object() && mc.header(function).object_type == ObjectType::Closure {
        mc.slot(function, closure::ENV)
    } else {
        Value::NIL
    };
    let mut found = Vec::new();
    let mut scope = environment;
    // A scope is at least three words, so a chain longer than the heap
    // could hold is a cycle, which only a damaged image can make (rule 10).
    let mut left = mc.used_words() / 3 + 1;
    while scope != stop
        && left > 0
        && scope.is_object()
        && mc.header(scope).object_type == ObjectType::Environment
    {
        let slots = mc.header(scope).slots;
        if mc.slot(scope, env::NAMESPACE).as_fixnum() == Some(env::VARIABLE) {
            let mut i = env::FIRST_PAIR;
            while i + 1 < slots {
                let value = mc.slot(scope, i + 1);
                if value != Value::UNBOUND {
                    found.push((mc.slot(scope, i), value));
                }
                i += 2;
            }
        }
        scope = mc.slot(scope, env::PARENT);
        left -= 1;
    }
    let mut list = Value::NIL;
    for (name, value) in found.into_iter().rev() {
        let pair = mc.cons(name, value);
        list = mc.cons(pair, list);
    }
    Ok(list)
}

fn begin(heap: &mut crate::heap::Heap) -> Result<(), EvalError> {
    heap.mutate(|mc| {
        let function = mc.register(Register::Value);
        if let Err(e) = code::verify::function(mc, function) {
            return Err(EvalError::Malformed(format!("{e:?}")));
        }
        Vm::start(mc, function, Value::NIL);
        Ok(())
    })
}

/// Run the computation in the registers for at most `steps` steps: one
/// mutation, so the heap may collect when it returns.
///
/// With [`begin_global`] this is the whole of what a host needs to drive the
/// machine *itself* — which a browser must, because it has to give the page
/// back to the event loop between slices, and may only answer a
/// [`Status::Blocked`](crate::slice::Status::Blocked) when a Promise
/// resolves (#84). [`call_global`] is the same two calls in a loop, for a
/// host that can simply wait.
pub fn one_slice(
    heap: &mut crate::heap::Heap,
    steps: u64,
) -> Result<crate::slice::Status, EvalError> {
    let (status, error) = heap.mutate(|mc| {
        let mut vm = Vm::resume(mc);
        let status = crate::slice::run_slice(&mut vm, steps);
        // The slice is the time a process gets before the next one's turn
        // (#34): the step budget is the preemption, as Erlang counts
        // reductions.
        if status == crate::slice::Status::Yielded {
            vm.preempt();
        }
        (status, vm.error.take())
    });
    match error {
        Some(e) => Err(e),
        None => Ok(status),
    }
}

/// Begin a call of the global function `name` on the value in the `Value`
/// register, leaving it ready for [`one_slice`].
pub fn begin_global(heap: &mut crate::heap::Heap, name: &str) -> Result<(), EvalError> {
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
    begin(heap)
}

/// After a failure no handler caught: let go of the dead computation and,
/// if it filled the heap, make room again (#85).
pub fn recover(heap: &mut crate::heap::Heap) {
    heap.mutate(|mc| Vm::abandon(mc));
    if heap.is_exhausted() {
        heap.collect();
        heap.clear_exhausted();
    }
}

/// What `%host-step` left in the `Value` register: a form's outcome, or
/// `None` when the input has run out.
pub fn step_outcome(heap: &mut crate::heap::Heap) -> Option<Outcome> {
    heap.mutate(|mc| {
        // What `%host-step` returned is the program's to decide, not the
        // machine's to trust: a debugger can return anything from its frame
        // (`return-from-frame`, #130), and a malformed outcome is reported,
        // never read as a list it is not (rule 10).
        let malformed = || {
            Some(Outcome::Failed(EvalError::Fault {
                operator: "%HOST-STEP",
                detail: "returned something that is not a step's outcome",
            }))
        };
        let result = mc.register(Register::Value);
        if !result.is_cons() {
            return malformed();
        }
        let kind = mc.car(result);
        if mc.symbol_is(kind, "EOF") {
            return None;
        }
        let rest = mc.cdr(result);
        if !rest.is_cons() || !is_string(mc, mc.car(rest)) {
            return malformed();
        }
        let text = mc.string_to_rust(mc.car(rest));
        Some(if mc.symbol_is(kind, "VALUE") {
            Outcome::Value(text)
        } else {
            // `(:error message type-name)`.
            let named = mc.cdr(rest);
            let condition = if named.is_cons() && is_string(mc, mc.car(named)) {
                mc.string_to_rust(mc.car(named))
            } else {
                String::from("ERROR")
            };
            Outcome::Error {
                message: text,
                condition,
            }
        })
    })
}

/// Is V a string, of either layout?
fn is_string<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type.is_string()
}

/// Hand `source` to the world's read-eval-print loop: `%host-source`, ready
/// for [`one_slice`]. Then [`begin_step`] for each form.
pub fn begin_source(heap: &mut crate::heap::Heap, source: &str) -> Result<(), EvalError> {
    heap.mutate(|mc| {
        crate::process::focus_main(mc);
        let text = mc.alloc_string(source);
        mc.set_register(Register::Value, text);
    });
    begin_global(heap, "%HOST-SOURCE")
}

/// Begin reading, evaluating and printing the next form: `%host-step`.
pub fn begin_step(heap: &mut crate::heap::Heap) -> Result<(), EvalError> {
    heap.mutate(|mc| {
        crate::process::focus_main(mc);
        mc.set_register(Register::Value, Value::NIL)
    });
    begin_global(heap, "%HOST-STEP")
}

/// Begin a call of the global function NAME with the argument nil, in the
/// main process: how a host starts a program that is not the read-eval-print
/// loop, such as the environment's command loop (#137).
pub fn begin_call(heap: &mut crate::heap::Heap, name: &str) -> Result<(), EvalError> {
    heap.mutate(|mc| {
        crate::process::focus_main(mc);
        mc.set_register(Register::Value, Value::NIL)
    });
    begin_global(heap, name)
}

/// Could a slice do anything now: is some process able to run, rather than
/// every one waiting on the host? A host that leaves a request waiting asks
/// this to know whether the world is idle (#137, `docs/display.md`).
pub fn can_run(heap: &mut crate::heap::Heap) -> bool {
    heap.mutate(|mc| crate::process::can_run(mc))
}

/// Is `source` whole — does it end outside every form? `%source-complete-p`
/// in `lisp/library/eval.lisp` answers, with the world's own reader, so that
/// a REPL knows whether to evaluate what it has or to read another line
/// (#119). Counting parentheses in the host would be a second reader, wrong
/// about strings, comments and `#\(`.
pub fn source_complete(
    heap: &mut crate::heap::Heap,
    source: &str,
    budget: u64,
    slice: u64,
    host: &mut dyn crate::host::Host,
) -> Result<bool, EvalError> {
    heap.mutate(|mc| {
        crate::process::focus_main(mc);
        let text = mc.alloc_string(source);
        mc.set_register(Register::Value, text);
    });
    call_global(heap, "%SOURCE-COMPLETE-P", budget, slice, host)?;
    Ok(heap.mutate(|mc| !mc.register(Register::Value).is_nil()))
}

/// What one form came to when a host ran it through [`eval_source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The form's value, as `prin1` wrote it.
    Value(String),
    /// A Lisp error, caught by the loop.
    Error {
        /// What it said.
        message: String,
        /// The condition's type, as a symbol's name: `TYPE-ERROR`.
        condition: String,
    },
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
        crate::process::focus_main(mc);
        let text = mc.alloc_string(source);
        mc.set_register(Register::Value, text);
    });
    call_global(heap, "%HOST-SOURCE", budget, slice, host)?;
    loop {
        if heap.is_exhausted() {
            // The form that filled the heap has failed and been reported.
            // Nothing cleared the flag, so every later step failed at once
            // and this loop span for ever without reading another form (#85).
            recover(heap);
        }
        heap.mutate(|mc| {
            crate::process::focus_main(mc);
            mc.set_register(Register::Value, Value::NIL)
        });
        let outcome = match call_global(heap, "%HOST-STEP", budget, slice, host) {
            Err(e) => Outcome::Failed(e),
            Ok(()) => match step_outcome(heap) {
                Some(outcome) => outcome,
                None => return Ok(()),
            },
        };
        if !each(outcome) {
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "unit/vm.rs"]
mod tests;
