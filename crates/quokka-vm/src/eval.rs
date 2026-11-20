//! The evaluator: a machine, not a function.
//!
//! The reader produces lists. This works out what they mean.
//!
//! # The rules
//!
//! The whole of Lisp evaluation, which is startlingly little:
//!
//! | Form | Result |
//! |---|---|
//! | a number, character, string | itself |
//! | a symbol | its value, looked up in the environment |
//! | `(f a b)` | evaluate `f`, `a`, `b`; apply the first to the rest |
//!
//! Plus the exception where the interesting cases live: a list whose head is a
//! **special operator** follows that operator's own rule instead. `if` must not
//! evaluate both branches; `quote` must not evaluate its argument at all. These
//! are exactly the forms that need to control whether their arguments are
//! evaluated, which is why they cannot be ordinary functions — a function's
//! arguments are already evaluated by the time it is called.
//!
//! # Why this is a machine
//!
//! The obvious implementation is recursive: evaluating `(+ 1 (* 2 3))` calls
//! itself for `(* 2 3)`. Thirty lines and you have a working Lisp.
//!
//! Quokka cannot use it, for two reasons.
//!
//! **It could not stop.** Rule 4: the VM must hand control back to the browser
//! mid-computation, and state on the Rust call stack is destroyed by returning.
//! This is the reason `crate::slice` exists and is argued there in full.
//!
//! **Lisp recursion would become Rust recursion.** A Lisp function walking a
//! hundred-thousand-element list recursively would need a hundred thousand Rust
//! frames and overflow an 8 MB native stack, never mind WebAssembly's 1 MB. The
//! language's recursion limit would be set by an implementation detail nobody
//! chose. Here, Lisp recursion consumes *heap*.
//!
//! So the state goes into registers and an explicit stack, and evaluation
//! becomes a sequence of small transitions. This is SICP §5.4's
//! *explicit-control evaluator*.
//!
//! # The cycle
//!
//! Two half-steps, alternating:
//!
//! - **Eval** — look at `control` and either produce a value directly or push a
//!   frame and start on a sub-form.
//! - **Return** — a value is in `value`; hand it to the top frame, which
//!   decides what happens next.
//!
//! A frame answers "I was in the middle of something; here is how to continue
//! once you give me a value". That is a *continuation*, and here it is an
//! ordinary heap object — which is what will make the debugger possible.
//!
//! # A worked example
//!
//! `(+ 1 (* 2 3))`:
//!
//! | # | `control` | `value` | `stack`, top first |
//! |---|---|---|---|
//! | 0 | `(+ 1 (* 2 3))` | — | — |
//! | 1 | `+` | — | `Args{done:(), left:(+ 1 (* 2 3))}` |
//! | 2 | `1` | `#<+>` | `Args{done:(+), left:(1 (* 2 3))}` |
//! | 3 | `(* 2 3)` | `1` | `Args{done:(1 +), left:((* 2 3))}` |
//! | 4 | `*` | — | `Args{done:(), left:(* 2 3)}`, `Args{…}` |
//! | 5 | … | … | … |
//! | n | halt | `7` | — |
//!
//! No Rust recursion anywhere. Stop between any two rows and the entire state
//! is four heap values.

use crate::heap::{Mutation, ObjectType, Register};
use crate::primitives;
use crate::slice::{Steppable, Transition};
use crate::value::Value;

/// Which half of the cycle the machine is in.
///
/// Stored in the heap's `mode` byte rather than in a Rust local, so a slice can
/// end between any two steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Evaluate `control`.
    Eval,
    /// Deliver `value` to the top frame.
    Return,
    /// Finished. `value` is the answer.
    Halt,
}

impl Mode {
    fn from_u8(byte: u8) -> Mode {
        match byte {
            1 => Mode::Return,
            2 => Mode::Halt,
            _ => Mode::Eval,
        }
    }
    fn as_u8(self) -> u8 {
        match self {
            Mode::Eval => 0,
            Mode::Return => 1,
            Mode::Halt => 2,
        }
    }
}

/// What went wrong.
///
/// These become Lisp conditions in phase 3. Until then they stop the machine
/// and are reported, rather than panicking — a REPL that dies on a typo is not
/// a REPL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// A symbol with no value.
    UnboundVariable(String),
    /// A symbol with no function.
    UndefinedFunction(String),
    /// Something that is not a function in head position.
    NotAFunction(String),
    /// A special operator written wrongly.
    BadSyntax {
        /// Which operator.
        operator: &'static str,
        /// What is wrong.
        detail: &'static str,
    },
    /// A primitive given the wrong number of arguments.
    WrongArgumentCount {
        /// Which primitive.
        name: &'static str,
        /// How many it wanted.
        wanted: usize,
        /// How many it got.
        got: usize,
    },
    /// A primitive given the wrong kind of thing.
    WrongType {
        /// Which primitive.
        name: &'static str,
        /// What it wanted.
        wanted: &'static str,
    },
    /// Arithmetic left fixnum range, and bignums do not exist.
    IntegerOverflow,
}

impl core::fmt::Display for EvalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EvalError::UnboundVariable(n) => write!(f, "the variable {n} is unbound"),
            EvalError::UndefinedFunction(n) => write!(f, "the function {n} is undefined"),
            EvalError::NotAFunction(n) => write!(f, "{n} is not a function"),
            EvalError::BadSyntax { operator, detail } => write!(f, "bad {operator}: {detail}"),
            EvalError::WrongArgumentCount { name, wanted, got } => {
                write!(f, "{name} wants {wanted} argument(s), got {got}")
            }
            EvalError::WrongType { name, wanted } => write!(f, "{name} wants {wanted}"),
            EvalError::IntegerOverflow => {
                write!(f, "integer overflow, and bignums do not exist yet")
            }
        }
    }
}

/// The evaluator, borrowing the heap for one slice.
///
/// It holds no state of its own: the registers live in the heap so that a slice
/// can end at any step and the collector can forward them. This struct is only
/// the code.
pub struct Machine<'a, 'h> {
    mc: &'a mut Mutation<'h>,
    /// Set when a step fails. The machine halts and the driver reports it.
    pub error: Option<EvalError>,
}

impl<'a, 'h> Machine<'a, 'h> {
    /// Prepare to evaluate `form`, discarding whatever was in progress.
    pub fn start(mc: &'a mut Mutation<'h>, form: Value<'h>, env: Value<'h>) -> Machine<'a, 'h> {
        mc.set_register(Register::Control, form);
        mc.set_register(Register::Env, env);
        mc.set_register(Register::Value, Value::NIL);
        mc.set_register(Register::Stack, Value::NIL);
        mc.set_mode(Mode::Eval.as_u8());
        Machine { mc, error: None }
    }

    /// Continue a computation already in the registers.
    pub fn resume(mc: &'a mut Mutation<'h>) -> Machine<'a, 'h> {
        Machine { mc, error: None }
    }

    /// The answer, once halted.
    pub fn result(&self) -> Value<'h> {
        self.mc.register(Register::Value)
    }

    /// Has it finished?
    pub fn is_halted(&self) -> bool {
        Mode::from_u8(self.mc.mode()) == Mode::Halt
    }

    fn fail(&mut self, e: EvalError) -> Transition {
        self.error = Some(e);
        self.mc.set_mode(Mode::Halt.as_u8());
        Transition::Halt
    }

    // ---- the Eval half ---------------------------------------------------

    fn step_eval(&mut self) -> Transition {
        let control = self.mc.register(Register::Control);

        // Self-evaluating: numbers, characters, strings, nil, t.
        if !control.is_cons() && !self.mc.is_symbol(control) {
            return self.deliver(control);
        }

        // A symbol evaluates to its value.
        if self.mc.is_symbol(control) {
            let env = self.mc.register(Register::Env);
            return match self.lookup(control, env) {
                Some(v) => self.deliver(v),
                None => {
                    let name = self.mc.symbol_name(control);
                    self.fail(EvalError::UnboundVariable(name))
                }
            };
        }

        // A list. Special operator, or a call?
        let head = self.mc.car(control);
        if self.mc.is_symbol(head) {
            match self.mc.symbol_name(head).as_str() {
                "QUOTE" => return self.eval_quote(control),
                "IF" => return self.eval_if(control),
                _ => {}
            }
        }
        self.eval_call(control)
    }

    fn eval_quote(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() || !self.mc.cdr(rest).is_nil() {
            return self.fail(EvalError::BadSyntax {
                operator: "quote",
                detail: "wants exactly one argument",
            });
        }
        // The point of quote: the argument is returned, not evaluated.
        let quoted = self.mc.car(rest);
        self.deliver(quoted)
    }

    fn eval_if(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() || !self.mc.cdr(rest).is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "if",
                detail: "wants a test and a consequent",
            });
        }
        let test = self.mc.car(rest);
        let branches = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);

        // The branches are not evaluated now — that is the whole reason `if`
        // cannot be a function. They go in a frame, and the one that survives
        // the test is evaluated later.
        let f = frame::make_if(self.mc, branches, env);
        self.push(f);
        self.mc.set_register(Register::Control, test);
        Transition::Continue
    }

    fn eval_call(&mut self, form: Value<'h>) -> Transition {
        let head = self.mc.car(form);
        let argument_forms = self.mc.cdr(form);
        let env = self.mc.register(Register::Env);

        // **Common Lisp is a Lisp-2.** A symbol has a value cell and a function
        // cell, separately, and the operator position names the *function*.
        // So `(list list)` is legal: the first names a function, the second a
        // variable, and they need not be related.
        //
        // Evaluating the head like an ordinary form would be a Lisp-1 — which
        // is what Scheme does, and what this code did until a test asked for
        // `(cons 1 2)` and got "the variable CONS is unbound".
        let function = if self.mc.is_symbol(head) {
            let f = self.mc.slot(head, crate::heap::symbol::FUNCTION);
            if f == Value::UNBOUND {
                let name = self.mc.symbol_name(head);
                return self.fail(EvalError::UndefinedFunction(name));
            }
            f
        } else {
            // `((lambda (x) x) 1)` belongs here and arrives with lambda.
            let what = crate::printer::print(self.mc, head);
            return self.fail(EvalError::NotAFunction(what));
        };

        if !argument_forms.is_cons() {
            return self.apply(function, Value::NIL);
        }

        // The function is the first thing "done", so the reversal at the end
        // puts it back at the head of the argument list.
        let done = self.mc.cons(function, Value::NIL);
        let f = frame::make_args(self.mc, done, argument_forms, env);
        self.push(f);
        let first = self.mc.car(argument_forms);
        self.mc.set_register(Register::Control, first);
        Transition::Continue
    }

    // ---- the Return half -------------------------------------------------

    fn step_return(&mut self) -> Transition {
        let stack = self.mc.register(Register::Stack);
        if stack.is_nil() {
            self.mc.set_mode(Mode::Halt.as_u8());
            return Transition::Halt;
        }

        let f = self.mc.car(stack);
        let rest = self.mc.cdr(stack);
        self.mc.set_register(Register::Stack, rest);

        match frame::kind(self.mc, f) {
            frame::IF => self.return_to_if(f),
            frame::ARGS => self.return_to_args(f),
            _ => self.fail(EvalError::BadSyntax {
                operator: "a frame",
                detail: "has an unknown kind",
            }),
        }
    }

    fn return_to_if(&mut self, f: Value<'h>) -> Transition {
        let branches = self.mc.slot(f, frame::IF_BRANCHES);
        let env = self.mc.slot(f, frame::IF_ENV);
        let test = self.mc.register(Register::Value);

        // Anything but nil is true. An absent else-branch yields nil.
        let chosen = if test.is_true() {
            self.mc.car(branches)
        } else {
            let otherwise = self.mc.cdr(branches);
            if otherwise.is_cons() {
                self.mc.car(otherwise)
            } else {
                return self.deliver(Value::NIL);
            }
        };
        self.mc.set_register(Register::Env, env);
        self.mc.set_register(Register::Control, chosen);
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    fn return_to_args(&mut self, f: Value<'h>) -> Transition {
        let done = self.mc.slot(f, frame::ARGS_DONE);
        let left = self.mc.slot(f, frame::ARGS_LEFT);
        let env = self.mc.slot(f, frame::ARGS_ENV);
        let just_evaluated = self.mc.register(Register::Value);

        // Collected in reverse, because pushing onto the front of a list is the
        // cheap direction. Reversed once at the end rather than walked each time.
        let done = self.mc.cons(just_evaluated, done);
        let left = self.mc.cdr(left);

        if left.is_cons() {
            let next = self.mc.car(left);
            let f = frame::make_args(self.mc, done, left, env);
            self.push(f);
            self.mc.set_register(Register::Env, env);
            self.mc.set_register(Register::Control, next);
            self.mc.set_mode(Mode::Eval.as_u8());
            return Transition::Continue;
        }

        let arguments = self.reverse(done);
        let function = self.mc.car(arguments);
        let rest = self.mc.cdr(arguments);
        self.apply(function, rest)
    }

    // ---- applying --------------------------------------------------------

    fn apply(&mut self, function: Value<'h>, arguments: Value<'h>) -> Transition {
        if let Some(index) = function.as_primitive() {
            return match primitives::call(self.mc, index, arguments) {
                Ok(v) => self.deliver(v),
                Err(e) => self.fail(e),
            };
        }
        let what = crate::printer::print(self.mc, function);
        self.fail(EvalError::NotAFunction(what))
    }

    // ---- helpers ---------------------------------------------------------

    /// Hand a value to whatever is waiting for one.
    fn deliver(&mut self, v: Value<'h>) -> Transition {
        self.mc.set_register(Register::Value, v);
        self.mc.set_mode(Mode::Return.as_u8());
        Transition::Continue
    }

    fn push(&mut self, f: Value<'h>) {
        let stack = self.mc.register(Register::Stack);
        let pushed = self.mc.cons(f, stack);
        self.mc.set_register(Register::Stack, pushed);
    }

    /// Look a name up: the environment first, then the symbol's own value cell,
    /// which is where globals live.
    fn lookup(&mut self, name: Value<'h>, env: Value<'h>) -> Option<Value<'h>> {
        let mut cursor = env;
        while cursor.is_cons() {
            let binding = self.mc.car(cursor);
            if binding.is_cons() && self.mc.car(binding) == name {
                return Some(self.mc.cdr(binding));
            }
            cursor = self.mc.cdr(cursor);
        }
        let global = self.mc.slot(name, crate::heap::symbol::VALUE);
        if global == Value::UNBOUND {
            None
        } else {
            Some(global)
        }
    }

    fn reverse(&mut self, list: Value<'h>) -> Value<'h> {
        let mut out = Value::NIL;
        let mut cursor = list;
        while cursor.is_cons() {
            let item = self.mc.car(cursor);
            out = self.mc.cons(item, out);
            cursor = self.mc.cdr(cursor);
        }
        out
    }
}

impl Steppable for Machine<'_, '_> {
    fn step(&mut self) -> Transition {
        match Mode::from_u8(self.mc.mode()) {
            Mode::Eval => self.step_eval(),
            Mode::Return => self.step_return(),
            Mode::Halt => Transition::Halt,
        }
    }
}

/// Frames: one pending step of the evaluator.
///
/// Slot 0 is the kind. The rest depend on it. Deliberately plain objects rather
/// than a Rust enum, because the debugger will show them to a user and the
/// image will save them.
pub mod frame {
    use super::*;

    /// Waiting on the test of an `if`.
    pub const IF: i64 = 1;
    /// Evaluating the parts of a call, left to right.
    pub const ARGS: i64 = 2;

    /// Slot 0 of every frame: which kind.
    pub const KIND: usize = 0;
    /// `(then else...)`, unevaluated.
    pub const IF_BRANCHES: usize = 1;
    /// The environment to evaluate the chosen branch in.
    pub const IF_ENV: usize = 2;
    /// Values so far, in reverse.
    pub const ARGS_DONE: usize = 1;
    /// Forms still to evaluate, the first of which is in flight.
    pub const ARGS_LEFT: usize = 2;
    /// The environment the call is being made in.
    pub const ARGS_ENV: usize = 3;

    /// Which kind of frame this is.
    pub fn kind<'h>(mc: &Mutation<'h>, f: Value<'h>) -> i64 {
        mc.slot(f, KIND).as_fixnum().unwrap_or(0)
    }

    /// A frame waiting on an `if`'s test.
    pub fn make_if<'h>(mc: &mut Mutation<'h>, branches: Value<'h>, env: Value<'h>) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 3);
        mc.set_slot(f, KIND, Value::fixnum(IF).expect("small"));
        mc.set_slot(f, IF_BRANCHES, branches);
        mc.set_slot(f, IF_ENV, env);
        f
    }

    /// A frame part-way through evaluating a call's parts.
    pub fn make_args<'h>(
        mc: &mut Mutation<'h>,
        done: Value<'h>,
        left: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 4);
        mc.set_slot(f, KIND, Value::fixnum(ARGS).expect("small"));
        mc.set_slot(f, ARGS_DONE, done);
        mc.set_slot(f, ARGS_LEFT, left);
        mc.set_slot(f, ARGS_ENV, env);
        f
    }
}

/// Evaluate a form to completion inside one mutation.
///
/// Convenience for the REPL and tests. A browser would instead call
/// [`crate::slice::run_slice`] with a budget and return between slices — which
/// is the whole point of the machine above, and is what phase 4 will do.
///
/// The budget is a guard against a program that never finishes: without an
/// interrupt there is nothing else to stop one, and a frozen REPL is the
/// failure `crate::slice` exists to prevent.
pub fn eval_now<'h>(
    mc: &mut Mutation<'h>,
    form: Value<'h>,
    budget: u64,
) -> Result<Value<'h>, EvalError> {
    use crate::slice::{run_slice, Status};
    let mut machine = Machine::start(mc, form, Value::NIL);
    match run_slice(&mut machine, budget) {
        Status::Done { .. } => match machine.error.take() {
            Some(e) => Err(e),
            None => Ok(machine.result()),
        },
        Status::Yielded => Err(EvalError::BadSyntax {
            operator: "the evaluator",
            detail: "ran out of budget",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::Heap;
    use crate::primitives;
    use crate::printer::print;
    use crate::reader::Reader;

    const PLENTY: u64 = 100_000;

    fn eval_str(source: &str) -> Result<String, EvalError> {
        let mut heap = Heap::new();
        let chars = Reader::chars_of(source);
        heap.mutate(|mc| {
            primitives::install(mc);
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).expect("reads").expect("a form");
            eval_now(mc, form, PLENTY).map(|v| print(mc, v))
        })
    }

    fn evals_to(source: &str, expected: &str) {
        assert_eq!(
            eval_str(source).as_deref(),
            Ok(expected),
            "evaluating {source}"
        );
    }

    #[test]
    fn things_evaluate_to_themselves() {
        evals_to("42", "42");
        evals_to("-7", "-7");
        evals_to("\"text\"", "\"text\"");
        evals_to("#\\a", "#\\a");
        evals_to("nil", "NIL");
        evals_to("t", "T");
    }

    #[test]
    fn arithmetic() {
        evals_to("(+ 1 2)", "3");
        evals_to("(+)", "0");
        evals_to("(* 6 7)", "42");
        evals_to("(*)", "1");
        evals_to("(- 10 3)", "7");
        evals_to("(- 5)", "-5");
        evals_to("(+ 1 2 3 4 5)", "15");
    }

    /// The worked example from the module documentation.
    #[test]
    fn nested_calls() {
        evals_to("(+ 1 (* 2 3))", "7");
        evals_to("(* (+ 1 1) (+ 2 2))", "8");
        evals_to("(+ (+ (+ 1 1) 1) 1)", "4");
    }

    #[test]
    fn comparisons_chain() {
        evals_to("(= 1 1)", "T");
        evals_to("(= 1 2)", "NIL");
        evals_to("(< 1 2 3)", "T");
        evals_to("(< 1 3 2)", "NIL");
        evals_to("(> 3 2 1)", "T");
    }

    /// Quote's entire purpose is that its argument is *not* evaluated.
    #[test]
    fn quote_does_not_evaluate() {
        evals_to("'foo", "FOO");
        evals_to("(quote foo)", "FOO");
        evals_to("'(1 2 3)", "(1 2 3)");
        // Without quote this would be an undefined function.
        evals_to("'(nosuchfunction 1)", "(NOSUCHFUNCTION 1)");
    }

    /// `if` cannot be a function: a function's arguments are already evaluated
    /// by the time it is called, so both branches would run. The untaken branch
    /// here would error if it were evaluated, which is the test.
    #[test]
    fn if_evaluates_only_one_branch() {
        evals_to("(if t 1 2)", "1");
        evals_to("(if nil 1 2)", "2");
        evals_to("(if t 1 (undefined-function))", "1");
        evals_to("(if nil (undefined-function) 2)", "2");
        // Anything but nil is true.
        evals_to("(if 0 'yes 'no)", "YES");
        evals_to("(if '() 'yes 'no)", "NO");
        // A missing else-branch yields nil.
        evals_to("(if nil 1)", "NIL");
    }

    #[test]
    fn list_operations() {
        evals_to("(cons 1 2)", "(1 . 2)");
        evals_to("(cons 1 nil)", "(1)");
        evals_to("(car '(1 2 3))", "1");
        evals_to("(cdr '(1 2 3))", "(2 3)");
        evals_to("(car nil)", "NIL");
        evals_to("(cdr nil)", "NIL");
        evals_to("(null nil)", "T");
        evals_to("(null '(1))", "NIL");
        evals_to("(consp '(1))", "T");
        evals_to("(symbolp 'x)", "T");
    }

    /// Interning is what makes this work: two reads of X give one object, so
    /// `eq` can compare words rather than names.
    #[test]
    fn eq_on_symbols_is_identity() {
        evals_to("(eq 'x 'x)", "T");
        evals_to("(eq 'x 'y)", "NIL");
        evals_to("(eq 1 1)", "T");
    }

    #[test]
    fn errors_are_values() {
        assert!(matches!(
            eval_str("undefined-variable"),
            Err(EvalError::UnboundVariable(_))
        ));
        assert!(matches!(eval_str("(1 2)"), Err(EvalError::NotAFunction(_))));
        assert!(matches!(
            eval_str("(car 1)"),
            Err(EvalError::WrongType { .. })
        ));
        assert!(matches!(
            eval_str("(cons 1)"),
            Err(EvalError::WrongArgumentCount { .. })
        ));
        assert!(matches!(
            eval_str("(quote)"),
            Err(EvalError::BadSyntax { .. })
        ));
        assert!(matches!(eval_str("(if)"), Err(EvalError::BadSyntax { .. })));
    }

    /// 20! is the largest factorial that fits in a fixnum, which is almost
    /// certainly why the phase 0 gate asks for it: no bignums required.
    #[test]
    fn arithmetic_overflow_is_reported_not_wrapped() {
        evals_to("(* 2432902008176640000 1)", "2432902008176640000");
        assert_eq!(
            eval_str("(* 2432902008176640000 21)"),
            Err(EvalError::IntegerOverflow)
        );
    }

    // ---- the machine, rather than the language ---------------------------

    /// The property `crate::slice` was designed for, now on something real:
    /// how a computation is cut up must not change what it computes.
    #[test]
    fn slicing_does_not_change_the_answer() {
        use crate::slice::{run_slice, Status};

        let source = "(+ 1 (* 2 (- 10 (+ 3 4))))";
        let chars = Reader::chars_of(source);

        for budget in [1u64, 2, 3, 5, 8, 13, 100, 10_000] {
            let mut heap = Heap::new();
            let answer = heap.mutate(|mc| {
                primitives::install(mc);
                let mut reader = Reader::new(&chars);
                let form = reader.read(mc).unwrap().unwrap();
                {
                    let mut machine = Machine::start(mc, form, Value::NIL);
                    let mut slices = 0;
                    while run_slice(&mut machine, budget) == Status::Yielded {
                        slices += 1;
                        assert!(slices < 10_000, "budget {budget} made no progress");
                    }
                }
                // The answer is in the Value register, which outlives the
                // machine — the machine holds no state of its own.
                print(mc, mc.register(Register::Value))
            });
            assert_eq!(answer, "7", "budget {budget} gave a different answer");
        }
    }

    /// Deep nesting must consume heap, not the Rust stack. A recursive
    /// evaluator would die here, and on wasm32 it would die far sooner.
    ///
    /// The form is built directly rather than read, because the *reader* has a
    /// depth limit of 200 and would refuse it. The two limits are unrelated:
    /// the reader's exists because the reader recurses in Rust, and the
    /// evaluator has none because it does not. Reading `(+ 1 (+ 1 ...))` five
    /// thousand deep is a different problem from evaluating it.
    #[test]
    fn deep_nesting_does_not_touch_the_rust_stack() {
        let depth = 5_000;
        let mut heap = Heap::new();
        let answer = heap.mutate(|mc| {
            primitives::install(mc);
            let plus = mc.intern("+");
            let one = Value::fixnum(1).unwrap();

            let mut form = Value::fixnum(0).unwrap();
            for _ in 0..depth {
                let inner = mc.cons(form, Value::NIL);
                let args = mc.cons(one, inner);
                form = mc.cons(plus, args);
            }

            eval_now(mc, form, 10_000_000).map(|v| print(mc, v))
        });
        assert_eq!(answer.as_deref(), Ok(depth.to_string().as_str()));
    }

    /// Everything the machine is holding lives in registers the collector
    /// forwards. If any one of them were forgotten, a collection part-way
    /// through would reclaim the computation.
    #[test]
    fn a_computation_survives_collection_midway() {
        use crate::slice::{run_slice, Status};

        let chars = Reader::chars_of("(+ 1 (* 2 (+ 3 4)))");
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            primitives::install(mc);
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            Machine::start(mc, form, Value::NIL);
        });

        // Step a little, collect, step a little, collect...
        let mut finished = false;
        for _ in 0..200 {
            let done = heap.mutate(|mc| {
                let mut machine = Machine::resume(mc);
                matches!(run_slice(&mut machine, 2), Status::Done { .. }) || machine.is_halted()
            });
            heap.collect();
            heap.verify().unwrap();
            if done {
                finished = true;
                break;
            }
        }
        assert!(finished, "the machine never halted");
        let answer = heap.mutate(|mc| print(mc, mc.register(Register::Value)));
        assert_eq!(answer, "15");
    }
}
