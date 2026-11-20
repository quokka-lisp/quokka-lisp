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

use crate::heap::{symbol, Mutation, ObjectType, Register};
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
    /// Discard frames until one matches the `Unwind` register, running any
    /// cleanups passed on the way.
    ///
    /// This is the third half-step, and it is what every non-local exit is
    /// made of. `return-from`, `throw` and `go` differ only in how their
    /// target is found — once found, all three unwind identically.
    Unwind,
    /// Finished. `value` is the answer.
    Halt,
}

impl Mode {
    fn from_u8(byte: u8) -> Mode {
        match byte {
            1 => Mode::Return,
            2 => Mode::Unwind,
            3 => Mode::Halt,
            _ => Mode::Eval,
        }
    }
    fn as_u8(self) -> u8 {
        match self {
            Mode::Eval => 0,
            Mode::Return => 1,
            Mode::Unwind => 2,
            Mode::Halt => 3,
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
    /// A `return-from`, `go` or `throw` with nowhere to go.
    NoSuchExit(String),
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
    /// A condition reached the top without being handled.
    Signalled(String),
    /// An index outside a sequence.
    IndexOutOfBounds {
        /// What was asked for.
        index: i64,
        /// How long the sequence is.
        length: usize,
    },
    /// Arithmetic left fixnum range, and bignums do not exist.
    IntegerOverflow,
    /// The heap reached its hard ceiling. Common Lisp calls this a
    /// `storage-condition`; it becomes one in phase 3.
    HeapExhausted,
    /// The step budget ran out. Not an error in the program so much as a
    /// refusal to wait any longer — phase 3 replaces it with Ctrl-C landing in
    /// the debugger, which is what a person actually wants.
    BudgetExhausted,
}

impl core::fmt::Display for EvalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EvalError::UnboundVariable(n) => write!(f, "the variable {n} is unbound"),
            EvalError::UndefinedFunction(n) => write!(f, "the function {n} is undefined"),
            EvalError::NotAFunction(n) => write!(f, "{n} is not a function"),
            EvalError::NoSuchExit(n) => write!(f, "there is no exit named {n} here"),
            EvalError::BadSyntax { operator, detail } => write!(f, "bad {operator}: {detail}"),
            EvalError::WrongArgumentCount { name, wanted, got } => {
                write!(f, "{name} wants {wanted} argument(s), got {got}")
            }
            EvalError::WrongType { name, wanted } => write!(f, "{name} wants {wanted}"),
            EvalError::Signalled(what) => write!(f, "{what}"),
            EvalError::IndexOutOfBounds { index, length } => {
                write!(f, "index {index} is outside a sequence of length {length}")
            }
            EvalError::IntegerOverflow => {
                write!(f, "integer overflow, and bignums do not exist yet")
            }
            EvalError::HeapExhausted => write!(
                f,
                "out of heap — a computation that does not finish, perhaps?"
            ),
            EvalError::BudgetExhausted => write!(
                f,
                "gave up after the step budget — a computation that does not finish, perhaps?"
            ),
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
        mc.set_register(Register::Unwind, Value::NIL);
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

    /// How many frames are pending.
    ///
    /// For tests that need to observe the stack rather than infer it: a tail
    /// call is a claim about depth, and a claim about depth should be measured.
    pub fn stack_depth(&self) -> usize {
        let mut depth = 0;
        let mut cursor = self.mc.register(Register::Stack);
        while cursor.is_cons() {
            depth += 1;
            cursor = self.mc.cdr(cursor);
        }
        depth
    }

    /// Has it finished?
    pub fn is_halted(&self) -> bool {
        Mode::from_u8(self.mc.mode()) == Mode::Halt
    }

    /// Stop, or signal — whichever the program can do something about.
    ///
    /// If the error is a fault in the program *and* some `handler-case` is on
    /// the stack, it becomes a condition and unwinds to it. Otherwise the
    /// machine halts carrying it, exactly as before.
    ///
    /// The handler search happens here rather than unconditionally because
    /// turning every error into a condition would mean allocating one for
    /// errors nobody can catch, including the one for running out of heap.
    fn fail(&mut self, e: EvalError) -> Transition {
        if self.has_handler() {
            if let Some(condition) = crate::condition::from_eval_error(self.mc, &e) {
                return self.signal(condition);
            }
        }
        self.error = Some(e);
        self.mc.set_mode(Mode::Halt.as_u8());
        Transition::Halt
    }

    /// Is there a `handler-case` anywhere on the stack?
    fn has_handler(&mut self) -> bool {
        let mut cursor = self.mc.register(Register::Stack);
        while cursor.is_cons() {
            if frame::kind(self.mc, self.mc.car(cursor)) == frame::HANDLER {
                return true;
            }
            cursor = self.mc.cdr(cursor);
        }
        false
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
        //
        // Dispatch is a slot read rather than a name comparison: `if` is
        // special *because the symbol IF is*, so that is where the fact lives.
        let head = self.mc.car(control);
        if self.mc.is_symbol(head) {
            if let Some(op) = self.mc.slot(head, symbol::SPECIAL).as_fixnum() {
                return match op {
                    special::QUOTE => self.eval_quote(control),
                    special::IF => self.eval_if(control),
                    special::LAMBDA => self.eval_lambda(control),
                    special::PROGN => self.eval_progn(control),
                    special::SETQ => self.eval_setq(control),
                    special::LET => self.eval_let(control),
                    special::LET_STAR => self.eval_let_star(control),
                    special::FLET => self.eval_flet(control, false),
                    special::LABELS => self.eval_flet(control, true),
                    special::FUNCTION => self.eval_function(control),
                    // `the` declares a type the interpreter does not check, and
                    // `locally` carries declarations there are none of yet.
                    // Both reduce to their body.
                    special::THE => {
                        let rest = self.mc.cdr(control);
                        if !rest.is_cons() {
                            return self.fail(EvalError::BadSyntax {
                                operator: "the",
                                detail: "wants a type and a form",
                            });
                        }
                        let form = self.mc.cdr(rest);
                        let env = self.mc.register(Register::Env);
                        self.begin(form, env)
                    }
                    special::LOCALLY => {
                        let body = self.mc.cdr(control);
                        let env = self.mc.register(Register::Env);
                        self.begin(body, env)
                    }
                    special::EVAL_WHEN => self.eval_eval_when(control),
                    special::BLOCK => self.eval_block(control),
                    special::RETURN_FROM => self.eval_return_from(control),
                    special::CATCH => self.eval_catch(control),
                    special::THROW => self.eval_throw(control),
                    special::TAGBODY => self.eval_tagbody(control),
                    special::GO => self.eval_go(control),
                    special::UNWIND_PROTECT => self.eval_unwind_protect(control),
                    special::DEFMACRO => self.eval_defmacro(control),
                    special::HANDLER_CASE => self.eval_handler_case(control),
                    _ => self.fail(EvalError::BadSyntax {
                        operator: "a special operator",
                        detail: "is not implemented",
                    }),
                };
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

    fn eval_lambda(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "lambda",
                detail: "wants a parameter list",
            });
        }
        let params = self.mc.car(rest);
        let body = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);
        // The body is not evaluated. Capturing `env` is what lets the closure
        // still see its enclosing scope after that scope has been left.
        //
        // The lambda list is rewritten here, once, rather than at every call:
        // defaults and `&aux` become a `let*` around the body, so what the
        // closure stores is something `bind_lambda_list` can match without
        // evaluating anything.
        let (params, body) = match crate::lambda_list::rewrite(
            self.mc,
            params,
            body,
            crate::lambda_list::Kind::Ordinary,
        ) {
            Ok(pair) => pair,
            Err(e) => return self.fail(e),
        };
        let c = closure::make(self.mc, params, body, env);
        self.deliver(c)
    }

    /// `(%handler-case form clause...)`, where a clause is
    /// `(type (var) body...)`.
    ///
    /// The frame is pushed *before* the form is evaluated, which is the whole
    /// mechanism: a signal unwinds the stack looking for one, exactly as a
    /// `throw` unwinds looking for a `catch`. The difference is only what
    /// counts as a match — a tag by identity there, a condition type here.
    fn eval_handler_case(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "handler-case",
                detail: "wants a form to protect",
            });
        }
        let protected = self.mc.car(rest);
        let clauses = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);

        let f = self.mc.alloc(ObjectType::Frame, 3);
        let kind = Value::fixnum(frame::HANDLER).expect("small");
        self.mc.set_slot(f, frame::KIND, kind);
        self.mc.set_slot(f, frame::HANDLER_CLAUSES, clauses);
        self.mc.set_slot(f, frame::HANDLER_ENV, env);
        self.push(f);

        self.mc.set_register(Register::Control, protected);
        self.mc.set_register(Register::Env, env);
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    /// Turn `error`'s arguments into a condition.
    ///
    /// `(error 'my-error)` names a type. `(error "text")` is a `simple-error`
    /// carrying a message — Common Lisp would treat the string as a `format`
    /// control string and the rest as its arguments, which needs a `format`
    /// Quokka has not got; the arguments are kept as data instead. Re-signalling
    /// a condition that was caught is `(error c)`.
    fn build_condition(&mut self, arguments: Value<'h>) -> Result<Value<'h>, EvalError> {
        if !arguments.is_cons() {
            return Err(EvalError::WrongArgumentCount {
                name: "ERROR",
                wanted: 1,
                got: 0,
            });
        }
        let datum = self.mc.car(arguments);
        let rest = self.mc.cdr(arguments);
        if crate::condition::is_condition(self.mc, datum) {
            return Ok(datum);
        }
        if self.mc.is_symbol(datum) {
            return Ok(crate::condition::make(self.mc, datum, Value::NIL, rest));
        }
        if datum.is_object() && self.mc.header(datum).object_type == ObjectType::String {
            let simple = self.mc.intern("SIMPLE-ERROR");
            return Ok(crate::condition::make(self.mc, simple, datum, rest));
        }
        Err(EvalError::WrongType {
            name: "ERROR",
            wanted: "a condition, a symbol naming one, or a string",
        })
    }

    /// Signal `condition`: unwind to the nearest handler that wants it.
    ///
    /// The target register carries the condition itself rather than a marker.
    /// Nothing else can be mistaken for one, so `step_unwind` tells the two
    /// kinds of unwinding apart without needing a second mode.
    ///
    /// That claim was false when this was written: conditions were
    /// `ObjectType::Instance`, which is also what `fresh_marker` allocates, so
    /// every `block` marker looked like a condition and a `return-from`
    /// through a `handler-case` was caught by it. Conditions have their own
    /// object type now, and `a_return_from_passes_through_a_handler` is the
    /// test that says so.
    fn signal(&mut self, condition: Value<'h>) -> Transition {
        self.mc.set_register(Register::Value, condition);
        self.begin_unwind(condition)
    }

    fn eval_progn(&mut self, form: Value<'h>) -> Transition {
        let body = self.mc.cdr(form);
        let env = self.mc.register(Register::Env);
        self.begin(body, env)
    }

    fn eval_setq(&mut self, form: Value<'h>) -> Transition {
        let pairs = self.mc.cdr(form);
        self.setq_next(pairs)
    }

    fn setq_next(&mut self, pairs: Value<'h>) -> Transition {
        if !pairs.is_cons() {
            // `(setq)` and the end of `(setq a 1 b 2)` both yield the last
            // value, or nil when there was none.
            return self.deliver(self.mc.register(Register::Value));
        }
        let name = self.mc.car(pairs);
        let rest = self.mc.cdr(pairs);
        if !self.mc.is_symbol(name) || !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "setq",
                detail: "wants alternating symbols and values",
            });
        }
        let value_form = self.mc.car(rest);
        let env = self.mc.register(Register::Env);
        let f = frame::make_setq(self.mc, name, self.mc.cdr(rest), env);
        self.push(f);
        self.mc.set_register(Register::Control, value_form);
        // Reached from both halves of the cycle: from `eval_setq` already in
        // Eval, and from `return_to_setq` in Return. Saying so explicitly is
        // the fix for `(setq a 1 b 2)` assigning only the first pair.
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    /// Evaluate `forms` in order, yielding the last.
    ///
    /// **The last form is in tail position**, so it must be started with no
    /// frame outstanding — otherwise a recursive call in tail position would
    /// grow the stack and the tail-call property would be lost. That is the one
    /// rule every operator with a body has to respect.
    fn begin(&mut self, forms: Value<'h>, env: Value<'h>) -> Transition {
        if !forms.is_cons() {
            return self.deliver(Value::NIL);
        }
        let first = self.mc.car(forms);
        let rest = self.mc.cdr(forms);
        self.mc.set_register(Register::Env, env);
        self.mc.set_register(Register::Control, first);
        self.mc.set_mode(Mode::Eval.as_u8());
        if rest.is_cons() {
            // More to come, so remember it. Not pushed when `first` is last.
            let f = frame::make_progn(self.mc, rest, env);
            self.push(f);
        }
        Transition::Continue
    }

    /// `(let ((v init) ...) body...)`.
    ///
    /// Every init is evaluated in the *outer* scope, and only then are the
    /// bindings made. That is the whole difference from `let*`, and it is why
    /// `(let ((x 1)) (let ((x 2) (y x)) y))` gives 1 rather than 2.
    fn eval_let(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "let",
                detail: "wants a binding list",
            });
        }
        let bindings = self.mc.car(rest);
        let body = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);

        // A binding list is a list. `(let 1 ...)` is malformed, not empty —
        // treating a non-list as "no bindings" would quietly accept nonsense.
        if !bindings.is_cons() {
            if !bindings.is_nil() {
                return self.fail(EvalError::BadSyntax {
                    operator: "let",
                    detail: "wants a list of bindings",
                });
            }
            return self.begin(body, env);
        }
        let Some(first) = self.binding_init(bindings) else {
            return self.fail(EvalError::BadSyntax {
                operator: "let",
                detail: "wants (name init), (name) or name",
            });
        };
        let f = frame::make_let(self.mc, bindings, bindings, Value::NIL, body, env);
        self.push(f);
        self.mc.set_register(Register::Control, first);
        Transition::Continue
    }

    fn return_to_let(&mut self, f: Value<'h>) -> Transition {
        let bindings = self.mc.slot(f, frame::LET_BINDINGS);
        let remaining = self.mc.slot(f, frame::LET_REMAINING);
        let done = self.mc.slot(f, frame::LET_DONE);
        let body = self.mc.slot(f, frame::LET_BODY);
        let env = self.mc.slot(f, frame::LET_ENV);

        let done = self.mc.cons(self.mc.register(Register::Value), done);
        let remaining = self.mc.cdr(remaining);

        if remaining.is_cons() {
            let Some(next) = self.binding_init(remaining) else {
                return self.fail(EvalError::BadSyntax {
                    operator: "let",
                    detail: "wants (name init), (name) or name",
                });
            };
            let f = frame::make_let(self.mc, bindings, remaining, done, body, env);
            self.push(f);
            self.mc.set_register(Register::Env, env);
            self.mc.set_register(Register::Control, next);
            self.mc.set_mode(Mode::Eval.as_u8());
            return Transition::Continue;
        }

        let names = self.binding_names(bindings);
        let in_order = self.reverse(done);
        let values = self.list_to_vec(in_order);
        let scope = env::make(self.mc, env, env::VARIABLE, &names, &values);
        self.begin(body, scope)
    }

    /// `(let* ((v init) ...) body...)`.
    ///
    /// One scope per binding, each nested in the last, so an init sees every
    /// binding before it. More objects than `let` for the same code, which is
    /// the honest cost of the semantics rather than an oversight.
    fn eval_let_star(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "let*",
                detail: "wants a binding list",
            });
        }
        let bindings = self.mc.car(rest);
        let body = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);
        if !bindings.is_cons() && !bindings.is_nil() {
            return self.fail(EvalError::BadSyntax {
                operator: "let*",
                detail: "wants a list of bindings",
            });
        }
        self.let_star_next(bindings, body, env)
    }

    fn let_star_next(
        &mut self,
        remaining: Value<'h>,
        body: Value<'h>,
        env: Value<'h>,
    ) -> Transition {
        if !remaining.is_cons() {
            return self.begin(body, env);
        }
        let Some(init) = self.binding_init(remaining) else {
            return self.fail(EvalError::BadSyntax {
                operator: "let*",
                detail: "wants (name init), (name) or name",
            });
        };
        let f = frame::make_let_star(self.mc, remaining, body, env);
        self.push(f);
        self.mc.set_register(Register::Env, env);
        self.mc.set_register(Register::Control, init);
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    fn return_to_let_star(&mut self, f: Value<'h>) -> Transition {
        let remaining = self.mc.slot(f, frame::LETSTAR_REMAINING);
        let body = self.mc.slot(f, frame::LETSTAR_BODY);
        let env = self.mc.slot(f, frame::LETSTAR_ENV);
        let value = self.mc.register(Register::Value);

        let Some(name) = self.binding_name(self.mc.car(remaining)) else {
            return self.fail(EvalError::BadSyntax {
                operator: "let*",
                detail: "wants a symbol to bind",
            });
        };
        let scope = env::make(self.mc, env, env::VARIABLE, &[name], &[value]);
        self.let_star_next(self.mc.cdr(remaining), body, scope)
    }

    /// `(flet ((f (params) body...) ...) body...)` and `labels`.
    ///
    /// The only difference is which scope the closures capture. Under `flet`
    /// they capture the *outer* one, so they cannot see each other; under
    /// `labels` they capture the new one, so they can — which is what makes
    /// mutual recursion possible. That means the scope has to exist before the
    /// closures that go in it, so it is built empty and filled.
    fn eval_flet(&mut self, form: Value<'h>, recursive: bool) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "flet",
                detail: "wants a function binding list",
            });
        }
        let bindings = self.mc.car(rest);
        let body = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);
        if !bindings.is_cons() && !bindings.is_nil() {
            return self.fail(EvalError::BadSyntax {
                operator: "flet",
                detail: "wants a list of function bindings",
            });
        }

        let mut names = Vec::new();
        let mut shapes = Vec::new();
        let mut cursor = bindings;
        while cursor.is_cons() {
            let binding = self.mc.car(cursor);
            if !binding.is_cons() || !self.mc.cdr(binding).is_cons() {
                return self.fail(EvalError::BadSyntax {
                    operator: "flet",
                    detail: "wants (name (params) body...)",
                });
            }
            let name = self.mc.car(binding);
            if !self.mc.is_symbol(name) {
                return self.fail(EvalError::BadSyntax {
                    operator: "flet",
                    detail: "wants a symbol for a name",
                });
            }
            let after = self.mc.cdr(binding);
            names.push(name);
            shapes.push((self.mc.car(after), self.mc.cdr(after)));
            cursor = self.mc.cdr(cursor);
        }

        // Built with placeholders so the scope exists first; `labels` closures
        // then capture it and are written into it.
        let placeholders = vec![Value::NIL; names.len()];
        let scope = env::make(self.mc, env, env::FUNCTION, &names, &placeholders);
        let captured = if recursive { scope } else { env };
        for (i, (params, fn_body)) in shapes.into_iter().enumerate() {
            let (params, fn_body) = match crate::lambda_list::rewrite(
                self.mc,
                params,
                fn_body,
                crate::lambda_list::Kind::Ordinary,
            ) {
                Ok(pair) => pair,
                Err(e) => return self.fail(e),
            };
            let c = closure::make(self.mc, params, fn_body, captured);
            self.mc.set_slot(scope, env::FIRST_PAIR + 1 + i * 2, c);
        }
        self.begin(body, scope)
    }

    /// `(function f)`, which `#'f` reads as.
    fn eval_function(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() || !self.mc.cdr(rest).is_nil() {
            return self.fail(EvalError::BadSyntax {
                operator: "function",
                detail: "wants exactly one name or lambda",
            });
        }
        let what = self.mc.car(rest);
        let env = self.mc.register(Register::Env);

        if self.mc.is_symbol(what) {
            return match env::lookup_function(self.mc, env, what) {
                Some(f) => self.deliver(f),
                None => {
                    let name = self.mc.symbol_name(what);
                    self.fail(EvalError::UndefinedFunction(name))
                }
            };
        }
        // `#'(lambda ...)`, which is the same as the lambda itself.
        if what.is_cons() && self.mc.symbol_is(self.mc.car(what), "LAMBDA") {
            return self.eval_lambda(what);
        }
        self.fail(EvalError::BadSyntax {
            operator: "function",
            detail: "wants a symbol or a lambda",
        })
    }

    /// `(eval-when (situations...) body...)`.
    ///
    /// An interpreter only ever has one situation: it is executing. So the
    /// body runs if `:execute` or `eval` is listed, and otherwise yields nil.
    /// The compile-time situations matter in phase 1 and mean nothing here.
    fn eval_eval_when(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "eval-when",
                detail: "wants a situation list",
            });
        }
        let situations = self.mc.car(rest);
        let body = self.mc.cdr(rest);
        let mut cursor = situations;
        while cursor.is_cons() {
            let s = self.mc.car(cursor);
            // `:execute` is a keyword, which means a symbol in the KEYWORD
            // package — and there are no packages yet, so the reader leaves the
            // colon in the name. Both spellings are accepted until there are.
            if self.mc.symbol_is(s, "EXECUTE")
                || self.mc.symbol_is(s, ":EXECUTE")
                || self.mc.symbol_is(s, "EVAL")
            {
                let env = self.mc.register(Register::Env);
                return self.begin(body, env);
            }
            cursor = self.mc.cdr(cursor);
        }
        self.deliver(Value::NIL)
    }

    // ---- non-local exits -------------------------------------------------
    //
    // Three operators leave a computation early, and they differ only in how
    // the target is found. `return-from` and `go` look theirs up in the
    // environment, so they can only reach somewhere that encloses them in the
    // source. `throw` searches the frame stack at run time, so it can reach a
    // caller that the thrower has never heard of.
    //
    // Once a target is found all three do the same thing: put it in the Unwind
    // register, put the value in Value, and switch to Unwind mode.

    /// A fresh object, equal to nothing but itself.
    ///
    /// `block` and `tagbody` each need an identity that no other entry to the
    /// same form can collide with — two calls to a recursive function are two
    /// different blocks, even though the name is the same.
    fn fresh_marker(&mut self) -> Value<'h> {
        self.mc.alloc(ObjectType::Instance, 0)
    }

    fn eval_block(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() || !self.mc.is_symbol(self.mc.car(rest)) {
            return self.fail(EvalError::BadSyntax {
                operator: "block",
                detail: "wants a name and a body",
            });
        }
        let name = self.mc.car(rest);
        let body = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);

        let marker = self.fresh_marker();
        let scope = env::make(self.mc, env, env::BLOCK, &[name], &[marker]);
        let f = frame::make_marked(self.mc, frame::BLOCK, marker);
        self.push(f);
        self.begin(body, scope)
    }

    fn eval_return_from(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() || !self.mc.is_symbol(self.mc.car(rest)) {
            return self.fail(EvalError::BadSyntax {
                operator: "return-from",
                detail: "wants a block name",
            });
        }
        let name = self.mc.car(rest);
        let env = self.mc.register(Register::Env);
        let Some(marker) = env::lookup_marker(self.mc, env, env::BLOCK, name) else {
            let what = self.mc.symbol_name(name);
            return self.fail(EvalError::NoSuchExit(what));
        };

        // The value form is evaluated first, then the unwind happens — which is
        // why this needs a frame rather than being immediate.
        let value_form = self.mc.cdr(rest);
        let f = frame::make_marked(self.mc, frame::EXIT, marker);
        self.push(f);
        if value_form.is_cons() {
            self.mc
                .set_register(Register::Control, self.mc.car(value_form));
            self.mc.set_mode(Mode::Eval.as_u8());
            Transition::Continue
        } else {
            self.deliver(Value::NIL)
        }
    }

    fn eval_catch(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "catch",
                detail: "wants a tag and a body",
            });
        }
        // The tag is evaluated, unlike a block name — that is the whole
        // difference between lexical and dynamic.
        let f = frame::make_catch_pending(self.mc, self.mc.cdr(rest));
        self.push(f);
        self.mc.set_register(Register::Control, self.mc.car(rest));
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    fn eval_throw(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "throw",
                detail: "wants a tag and a value",
            });
        }
        let f = frame::make_throw_pending(self.mc, self.mc.cdr(rest));
        self.push(f);
        self.mc.set_register(Register::Control, self.mc.car(rest));
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    /// `(tagbody tag form ... tag form ...)`.
    ///
    /// Atoms in the body are labels; conses are forms. It yields nil, and
    /// exists so that `go` has somewhere to go — which is to say, so that every
    /// looping macro in the language has something to expand into.
    fn eval_tagbody(&mut self, form: Value<'h>) -> Transition {
        let body = self.mc.cdr(form);
        let env = self.mc.register(Register::Env);
        let marker = self.fresh_marker();

        // Every tag in this body is bound to the same marker, so `go` finds
        // *this* tagbody; which tag to resume at travels in the Value register.
        let mut tags = Vec::new();
        let mut cursor = body;
        while cursor.is_cons() {
            let item = self.mc.car(cursor);
            if !item.is_cons() {
                tags.push(item);
            }
            cursor = self.mc.cdr(cursor);
        }
        let markers = vec![marker; tags.len()];
        let scope = env::make(self.mc, env, env::TAG, &tags, &markers);

        let f = frame::make_tagbody(self.mc, marker, body, body, scope);
        self.push(f);
        self.tagbody_step(body, body, marker, scope)
    }

    /// Run the next form of a tagbody, skipping labels.
    fn tagbody_step(
        &mut self,
        body: Value<'h>,
        remaining: Value<'h>,
        marker: Value<'h>,
        env: Value<'h>,
    ) -> Transition {
        let mut cursor = remaining;
        while cursor.is_cons() && !self.mc.car(cursor).is_cons() {
            cursor = self.mc.cdr(cursor); // a label, not a form
        }
        if !cursor.is_cons() {
            // Fell off the end. A tagbody yields nil.
            self.pop();
            return self.deliver(Value::NIL);
        }
        let next = self.mc.car(cursor);
        let rest = self.mc.cdr(cursor);
        let f = frame::make_tagbody(self.mc, marker, body, rest, env);
        self.replace_top(f);
        self.mc.set_register(Register::Env, env);
        self.mc.set_register(Register::Control, next);
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    fn eval_go(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "go",
                detail: "wants a tag",
            });
        }
        let tag = self.mc.car(rest);
        let env = self.mc.register(Register::Env);
        let Some(marker) = env::lookup_marker(self.mc, env, env::TAG, tag) else {
            let what = crate::printer::print(self.mc, tag);
            return self.fail(EvalError::NoSuchExit(what));
        };
        // The tag travels as the value; the tagbody frame reads it on arrival.
        self.mc.set_register(Register::Value, tag);
        self.begin_unwind(marker)
    }

    /// `(defmacro name (params) body...)`.
    ///
    /// The same shape as `defun`, and a completely different thing. A function
    /// receives values and returns a value; a macro receives **unevaluated
    /// forms** and returns a form, which is then evaluated in its place.
    ///
    /// That is only coherent because code is data: a macro is an ordinary Lisp
    /// function whose arguments and result happen to be program text.
    fn eval_defmacro(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() || !self.mc.cdr(rest).is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "defmacro",
                detail: "wants a name, a parameter list and a body",
            });
        }
        let name = self.mc.car(rest);
        if !self.mc.is_symbol(name) {
            return self.fail(EvalError::BadSyntax {
                operator: "defmacro",
                detail: "wants a symbol for a name",
            });
        }
        let after = self.mc.cdr(rest);
        let params = self.mc.car(after);
        let body = self.mc.cdr(after);
        let env = self.mc.register(Register::Env);
        let m = {
            let (params, body) = match crate::lambda_list::rewrite(
                self.mc,
                params,
                body,
                crate::lambda_list::Kind::Macro,
            ) {
                Ok(pair) => pair,
                Err(e) => return self.fail(e),
            };
            closure::make_macro(self.mc, params, body, env)
        };
        self.mc.set_slot(name, symbol::FUNCTION, m);
        self.deliver(name)
    }

    /// `(unwind-protect body cleanup...)`.
    ///
    /// The cleanup runs however the body ends — normally, or because something
    /// unwound straight past. The second case is the hard one: the unwind has
    /// to *pause*, run ordinary code, and then carry on from where it was.
    fn eval_unwind_protect(&mut self, form: Value<'h>) -> Transition {
        let rest = self.mc.cdr(form);
        if !rest.is_cons() {
            return self.fail(EvalError::BadSyntax {
                operator: "unwind-protect",
                detail: "wants a body",
            });
        }
        let body = self.mc.car(rest);
        let cleanup = self.mc.cdr(rest);
        let env = self.mc.register(Register::Env);

        let f = frame::make_protect(self.mc, cleanup, env);
        self.push(f);
        self.mc.set_register(Register::Control, body);
        self.mc.set_mode(Mode::Eval.as_u8());
        Transition::Continue
    }

    // ---- the Unwind half -------------------------------------------------

    /// Start unwinding towards `target`. The value to deliver is already in the
    /// Value register.
    fn begin_unwind(&mut self, target: Value<'h>) -> Transition {
        self.mc.set_register(Register::Unwind, target);
        self.mc.set_mode(Mode::Unwind.as_u8());
        Transition::Continue
    }

    fn step_unwind(&mut self) -> Transition {
        let stack = self.mc.register(Register::Stack);
        if stack.is_nil() {
            let target = self.mc.register(Register::Unwind);
            // A condition nobody handled. The machine stops, which is what an
            // unhandled error means — phase 3 replaces this with the debugger.
            if crate::condition::is_condition(self.mc, target) {
                let what = crate::condition::describe(self.mc, target);
                self.mc.set_register(Register::Unwind, Value::NIL);
                return self.fail(EvalError::Signalled(what));
            }
            // Nothing matched: a `throw` with no catch, which is an error
            // rather than a silent halt.
            let what = crate::printer::print(self.mc, target);
            return self.fail(EvalError::NoSuchExit(what));
        }

        let f = self.mc.car(stack);
        let target = self.mc.register(Register::Unwind);
        let kind = frame::kind(self.mc, f);

        // A cleanup must run even though we are leaving. The unwind pauses: the
        // saved target goes into a CLEANUP frame, ordinary evaluation resumes
        // for the cleanup forms, and the CLEANUP frame restarts the unwind
        // afterwards. Without this, unwind-protect would be a comment.
        if kind == frame::PROTECT {
            self.pop();
            let cleanup = self.mc.slot(f, frame::PROTECT_CLEANUP);
            let env = self.mc.slot(f, frame::PROTECT_ENV);
            let value = self.mc.register(Register::Value);
            let resume = frame::make_cleanup(self.mc, value, target);
            self.push(resume);
            return self.begin(cleanup, env);
        }

        // A condition looking for a handler. The clauses are tried in order,
        // as CLHS says, and the first type that matches wins.
        if kind == frame::HANDLER && crate::condition::is_condition(self.mc, target) {
            let clauses = self.mc.slot(f, frame::HANDLER_CLAUSES);
            let env = self.mc.slot(f, frame::HANDLER_ENV);
            let mut cursor = clauses;
            while cursor.is_cons() {
                let clause = self.mc.car(cursor);
                cursor = self.mc.cdr(cursor);
                if !clause.is_cons() {
                    continue;
                }
                let wanted = self.mc.car(clause);
                if !crate::condition::matches(self.mc, wanted, target) {
                    continue;
                }
                let after = self.mc.cdr(clause);
                let (names, values) = if after.is_cons() && self.mc.car(after).is_cons() {
                    // `(type (c) body...)` — the condition is bound to c.
                    let var = self.mc.car(self.mc.car(after));
                    (vec![var], vec![target])
                } else {
                    // `(type () body...)` or `(type body...)`: no binding.
                    (Vec::new(), Vec::new())
                };
                let body = if after.is_cons() && self.mc.car(after).is_cons() {
                    self.mc.cdr(after)
                } else {
                    after
                };
                self.pop();
                self.mc.set_register(Register::Unwind, Value::NIL);
                let scope = env::make(self.mc, env, env::VARIABLE, &names, &values);
                return self.begin(body, scope);
            }
            // This handler-case does not want it; keep unwinding.
            self.pop();
            return Transition::Continue;
        }

        // A matching block, catch or tagbody ends the unwind.
        if frame::marker_of(self.mc, f) == Some(target) {
            if kind == frame::TAGBODY {
                let body = self.mc.slot(f, frame::TAGBODY_BODY);
                let env = self.mc.slot(f, frame::TAGBODY_ENV);
                let tag = self.mc.register(Register::Value);
                let resume = self.after_tag(body, tag);
                self.mc.set_register(Register::Unwind, Value::NIL);
                return self.tagbody_step(body, resume, target, env);
            }
            self.pop();
            self.mc.set_register(Register::Unwind, Value::NIL);
            let value = self.mc.register(Register::Value);
            return self.deliver(value);
        }

        self.pop();
        Transition::Continue
    }

    /// The part of a tagbody's body just after `tag`.
    fn after_tag(&mut self, body: Value<'h>, tag: Value<'h>) -> Value<'h> {
        let mut cursor = body;
        while cursor.is_cons() {
            if self.mc.car(cursor) == tag {
                return self.mc.cdr(cursor);
            }
            cursor = self.mc.cdr(cursor);
        }
        Value::NIL
    }

    fn pop(&mut self) {
        let stack = self.mc.register(Register::Stack);
        let rest = self.mc.cdr(stack);
        self.mc.set_register(Register::Stack, rest);
    }

    fn replace_top(&mut self, f: Value<'h>) {
        self.pop();
        self.push(f);
    }

    // ---- binding shapes --------------------------------------------------
    //
    // A binding may be written `(name init)`, `(name)` or bare `name`. The last
    // two bind nil.

    fn binding_init(&mut self, remaining: Value<'h>) -> Option<Value<'h>> {
        let binding = self.mc.car(remaining);
        if self.mc.is_symbol(binding) {
            return Some(Value::NIL);
        }
        if !binding.is_cons() || !self.mc.is_symbol(self.mc.car(binding)) {
            return None;
        }
        let after = self.mc.cdr(binding);
        Some(if after.is_cons() {
            self.mc.car(after)
        } else {
            Value::NIL
        })
    }

    fn binding_name(&mut self, binding: Value<'h>) -> Option<Value<'h>> {
        if self.mc.is_symbol(binding) {
            return Some(binding);
        }
        if binding.is_cons() && self.mc.is_symbol(self.mc.car(binding)) {
            return Some(self.mc.car(binding));
        }
        None
    }

    fn binding_names(&mut self, bindings: Value<'h>) -> Vec<Value<'h>> {
        let mut names = Vec::new();
        let mut cursor = bindings;
        while cursor.is_cons() {
            if let Some(name) = self.binding_name(self.mc.car(cursor)) {
                names.push(name);
            }
            cursor = self.mc.cdr(cursor);
        }
        names
    }

    fn list_to_vec(&mut self, list: Value<'h>) -> Vec<Value<'h>> {
        let mut out = Vec::new();
        let mut cursor = list;
        while cursor.is_cons() {
            out.push(self.mc.car(cursor));
            cursor = self.mc.cdr(cursor);
        }
        out
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
            // A lexical binding from `flet` or `labels` shadows the symbol's
            // own function cell, which is what makes them lexical.
            match env::lookup_function(self.mc, env, head) {
                Some(f) => f,
                None => {
                    let name = self.mc.symbol_name(head);
                    return self.fail(EvalError::UndefinedFunction(name));
                }
            }
        } else {
            // `((lambda (x) x) 1)`. The head is evaluated like any other form,
            // which is the one place the Lisp-2 rule bends — and it has to,
            // since there is no symbol here to carry a function cell.
            let env = self.mc.register(Register::Env);
            let f = frame::make_args(self.mc, Value::NIL, form, env);
            self.push(f);
            self.mc.set_register(Register::Control, head);
            return Transition::Continue;
        };

        // A macro takes the argument *forms*, unevaluated, and its result is
        // evaluated in place of the call. Nothing is evaluated on the way in,
        // so there is no argument-collecting frame — just the expansion.
        if closure::is_macro(self.mc, function) {
            // The caller's environment has to be saved, because running the
            // macro body replaces it: a macro is a closure, so applying it
            // sets Env to the macro's own scope. In a recursive evaluator the
            // Rust stack would hand the caller's environment back by itself;
            // here Env is a register, and a register that is clobbered must be
            // restored by whoever clobbered it. That is what this frame is
            // for, and all it is for.
            let f = frame::make_marked(self.mc, frame::EXPAND, env);
            self.push(f);
            return self.apply_closure(function, argument_forms);
        }

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
            frame::PROGN => {
                let rest = self.mc.slot(f, frame::PROGN_REST);
                let env = self.mc.slot(f, frame::PROGN_ENV);
                self.begin(rest, env)
            }
            frame::SETQ => self.return_to_setq(f),
            frame::LET => self.return_to_let(f),
            frame::LETSTAR => self.return_to_let_star(f),
            // A block or catch that was left normally: its value is the value.
            frame::BLOCK | frame::CATCH => {
                let value = self.mc.register(Register::Value);
                self.deliver(value)
            }
            // A return-from whose value has now been evaluated.
            frame::EXIT => {
                let marker = self.mc.slot(f, frame::MARKER);
                self.begin_unwind(marker)
            }
            // A macro returned a form. Evaluate that instead of the call —
            // which is the whole of what a macro is.
            // The protected form finished without signalling, so the
            // handlers were not needed. Drop them and pass the value on.
            frame::HANDLER => {
                let value = self.mc.register(Register::Value);
                self.deliver(value)
            }
            frame::EXPAND => {
                let expansion = self.mc.register(Register::Value);
                // Back to where the macro was *called*, not where it was
                // written. `(let ((v 10)) (twice v))` depends on exactly this.
                let caller_env = self.mc.slot(f, frame::EXPAND_ENV);
                self.mc.set_register(Register::Env, caller_env);
                self.mc.set_register(Register::Control, expansion);
                self.mc.set_mode(Mode::Eval.as_u8());
                Transition::Continue
            }
            frame::CATCH_PENDING => {
                let tag = self.mc.register(Register::Value);
                let body = self.mc.slot(f, frame::PENDING_REST);
                let env = self.mc.register(Register::Env);
                let c = frame::make_marked(self.mc, frame::CATCH, tag);
                self.push(c);
                self.begin(body, env)
            }
            frame::THROW_PENDING => {
                let tag = self.mc.register(Register::Value);
                let rest = self.mc.slot(f, frame::PENDING_REST);
                let e = frame::make_marked(self.mc, frame::EXIT, tag);
                self.push(e);
                if rest.is_cons() {
                    let value_form = self.mc.car(rest);
                    self.mc.set_register(Register::Control, value_form);
                    self.mc.set_mode(Mode::Eval.as_u8());
                    Transition::Continue
                } else {
                    self.deliver(Value::NIL)
                }
            }
            frame::TAGBODY => {
                // A form finished; run the next one. Its value is discarded —
                // a tagbody is for effect.
                let marker = self.mc.slot(f, frame::MARKER);
                let body = self.mc.slot(f, frame::TAGBODY_BODY);
                let rest = self.mc.slot(f, frame::TAGBODY_REST);
                let env = self.mc.slot(f, frame::TAGBODY_ENV);
                self.push(f);
                self.tagbody_step(body, rest, marker, env)
            }
            // The body returned normally, so the cleanup still runs — and the
            // body's value has to survive it.
            frame::PROTECT => {
                let cleanup = self.mc.slot(f, frame::PROTECT_CLEANUP);
                let env = self.mc.slot(f, frame::PROTECT_ENV);
                let value = self.mc.register(Register::Value);
                let resume = frame::make_cleanup(self.mc, value, Value::NIL);
                self.push(resume);
                self.begin(cleanup, env)
            }
            // A cleanup finished. Its own value is discarded; what was saved
            // before it ran is what matters.
            frame::CLEANUP => {
                let saved = self.mc.slot(f, frame::CLEANUP_VALUE);
                let target = self.mc.slot(f, frame::CLEANUP_TARGET);
                self.mc.set_register(Register::Value, saved);
                if target.is_nil() {
                    self.deliver(saved)
                } else {
                    self.begin_unwind(target)
                }
            }
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

    fn return_to_setq(&mut self, f: Value<'h>) -> Transition {
        let name = self.mc.slot(f, frame::SETQ_NAME);
        let rest = self.mc.slot(f, frame::SETQ_REST);
        let env = self.mc.slot(f, frame::SETQ_ENV);
        let value = self.mc.register(Register::Value);
        env::assign(self.mc, env, name, value);
        self.mc.set_register(Register::Env, env);
        self.setq_next(rest)
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
        // `(funcall f 1 2)` applies whatever `f` *holds* rather than whatever
        // its name denotes as a function. A Lisp-2 needs this: a closure in a
        // variable is otherwise uncallable, because the operator position only
        // ever looks at function cells.
        //
        // A loop rather than recursion, since `(funcall #'funcall ...)` nests
        // as deeply as the user likes and Rust's stack is not the place for it.
        let mut function = function;
        let mut arguments = arguments;
        loop {
            if function.as_primitive() == Some(primitives::FUNCALL) {
                if !arguments.is_cons() {
                    return self.fail(EvalError::WrongArgumentCount {
                        name: "FUNCALL",
                        wanted: 1,
                        got: 0,
                    });
                }
                function = self.mc.car(arguments);
                arguments = self.mc.cdr(arguments);
                continue;
            }
            // `(error datum ...)` signals rather than returning, so it
            // unwinds the stack — which a primitive cannot do, for the same
            // reason one cannot make a call.
            if function.as_primitive() == Some(primitives::ERROR) {
                let condition = match self.build_condition(arguments) {
                    Ok(c) => c,
                    Err(e) => return self.fail(e),
                };
                return self.signal(condition);
            }
            // `(apply f a b list)` calls f with a, b and the elements of list.
            // Intercepted here for the same reason as funcall: the call itself
            // is the machine's business, and a primitive that wanted to make
            // one would have to run the evaluator from inside Rust.
            if function.as_primitive() == Some(primitives::APPLY) {
                match self.spread(arguments) {
                    Ok((f, args)) => {
                        function = f;
                        arguments = args;
                        continue;
                    }
                    Err(e) => return self.fail(e),
                }
            }
            break;
        }

        if let Some(index) = function.as_primitive() {
            return match primitives::call(self.mc, index, arguments) {
                Ok(v) => self.deliver(v),
                Err(e) => self.fail(e),
            };
        }
        if closure::is_callable(self.mc, function) {
            return self.apply_closure(function, arguments);
        }
        let what = crate::printer::print(self.mc, function);
        self.fail(EvalError::NotAFunction(what))
    }

    /// Turn `apply`'s arguments into the argument list for the real call.
    ///
    /// `(f a b (c d))` becomes `(a b c d)`. The last argument is **shared**
    /// rather than copied, which is what keeps `apply` itself cheap however
    /// long the list is — only the fixed arguments before it are consed, and a
    /// written call has a bounded number of those.
    fn spread(&mut self, arguments: Value<'h>) -> Result<(Value<'h>, Value<'h>), EvalError> {
        if !arguments.is_cons() {
            return Err(EvalError::WrongArgumentCount {
                name: "APPLY",
                wanted: 2,
                got: 0,
            });
        }
        let function = self.mc.car(arguments);
        let rest = self.mc.cdr(arguments);
        if !rest.is_cons() {
            return Err(EvalError::WrongArgumentCount {
                name: "APPLY",
                wanted: 2,
                got: 1,
            });
        }

        let mut fixed = Vec::new();
        let mut cursor = rest;
        loop {
            let next = self.mc.cdr(cursor);
            if next.is_nil() {
                break;
            }
            if !next.is_cons() {
                return Err(EvalError::BadSyntax {
                    operator: "APPLY",
                    detail: "wants a proper argument list",
                });
            }
            fixed.push(self.mc.car(cursor));
            cursor = next;
        }
        let spread = self.mc.car(cursor);
        if !spread.is_nil() && !spread.is_cons() {
            return Err(EvalError::WrongType {
                name: "APPLY",
                wanted: "a list as the last argument",
            });
        }
        let mut out = spread;
        for v in fixed.into_iter().rev() {
            out = self.mc.cons(v, out);
        }
        Ok((function, out))
    }

    /// Bind the parameters and evaluate the body.
    ///
    /// **Pushes no frame.** That is what makes a call in tail position a tail
    /// call: by the time this runs, the caller's frames have already been
    /// popped, so a self-call in tail position leaves the stack exactly as deep
    /// as it was. The property falls out of the design rather than being added.
    fn apply_closure(&mut self, function: Value<'h>, arguments: Value<'h>) -> Transition {
        let params = self.mc.slot(function, closure::PARAMS);
        let body = self.mc.slot(function, closure::BODY);
        let captured = self.mc.slot(function, closure::ENV);

        let (names, values) = match self.bind_lambda_list(params, arguments) {
            Ok(pair) => pair,
            Err(e) => return self.fail(e),
        };
        let scope = env::make(self.mc, captured, env::VARIABLE, &names, &values);
        self.begin(body, scope)
    }

    /// Match a lambda list against arguments.
    ///
    /// Supports `&optional`, `&rest` and `&body`. The last two are the same
    /// thing with different names — `&body` exists so that a macro definition
    /// reads as what it is, and so an editor can indent it as a body.
    ///
    /// Not supported: default values for optionals, which need a form
    /// evaluated at call time and therefore a frame, and destructuring
    /// patterns, which macros may use and functions may not. Both are noted
    /// rather than silently accepted.
    /// Match arguments to a **simple** lambda list: bare symbols, `&optional`,
    /// `&rest`, and `&key` as `(keyword variable)` pairs.
    ///
    /// Nothing here evaluates anything, which is the point —
    /// [`crate::lambda_list::rewrite`] has already turned every default form
    /// into a `let*` around the body, so this only has to decide *which* value
    /// each name gets, never *compute* one.
    ///
    /// A parameter with no argument is bound to the unsupplied marker rather
    /// than to nil, because `(f)` and `(f nil)` have to be able to differ. The
    /// `let*` upstream turns the marker into the default.
    fn bind_lambda_list(
        &mut self,
        params: Value<'h>,
        arguments: Value<'h>,
    ) -> Result<(Vec<Value<'h>>, Vec<Value<'h>>), EvalError> {
        let mut names = Vec::new();
        let mut values = Vec::new();
        let mut param = params;
        let mut arg = arguments;
        let mut required = 0usize;
        let mut supplied = 0usize;
        let mut section = 0u8; // 0 required, 1 optional, 2 rest, 3 key
                               // `&rest` swallows everything left, so leftover arguments stop being
                               // an error once one has been bound. `&key` still scans the same tail,
                               // which is what CLHS says the two do together.
        let mut has_rest = false;
        let mut keys: Vec<(Value<'h>, Value<'h>)> = Vec::new();
        let mut allow_other_keys = false;
        let marker = self.mc.intern(crate::lambda_list::UNSUPPLIED);

        while param.is_cons() {
            let name = self.mc.car(param);
            param = self.mc.cdr(param);

            if self.mc.is_symbol(name) {
                if self.mc.symbol_is(name, "&OPTIONAL") {
                    section = 1;
                    continue;
                }
                if self.mc.symbol_is(name, "&REST") || self.mc.symbol_is(name, "&BODY") {
                    let Some(rest_name) = (param.is_cons()).then(|| self.mc.car(param)) else {
                        return Err(EvalError::BadSyntax {
                            operator: "a lambda list",
                            detail: "&rest wants a name after it",
                        });
                    };
                    param = self.mc.cdr(param);
                    names.push(rest_name);
                    values.push(arg);
                    has_rest = true;
                    section = 2;
                    continue;
                }
                if self.mc.symbol_is(name, "&KEY") {
                    section = 3;
                    continue;
                }
                if self.mc.symbol_is(name, "&ALLOW-OTHER-KEYS") {
                    allow_other_keys = true;
                    continue;
                }
            }

            if section == 3 {
                // `(keyword variable)`, which is what the rewrite emits.
                if !name.is_cons() {
                    return Err(EvalError::BadSyntax {
                        operator: "a lambda list",
                        detail: "&key wants (keyword variable) pairs here",
                    });
                }
                let keyword = self.mc.car(name);
                let variable = self.mc.car(self.mc.cdr(name));
                keys.push((keyword, variable));
                continue;
            }

            if !self.mc.is_symbol(name) {
                return Err(EvalError::BadSyntax {
                    operator: "a lambda list",
                    detail: "wants symbols for parameters",
                });
            }

            if section == 0 {
                required += 1;
            }
            if arg.is_cons() {
                names.push(name);
                values.push(self.mc.car(arg));
                arg = self.mc.cdr(arg);
                supplied += 1;
            } else if section == 1 {
                names.push(name);
                values.push(marker);
            } else {
                return Err(EvalError::WrongArgumentCount {
                    name: "this function",
                    wanted: required,
                    got: supplied,
                });
            }
        }

        if !keys.is_empty() || allow_other_keys {
            self.bind_keys(
                &keys,
                arg,
                marker,
                allow_other_keys,
                &mut names,
                &mut values,
            )?;
            return Ok((names, values));
        }

        if arg.is_cons() && !has_rest {
            let mut extra = 0;
            let mut cursor = arg;
            while cursor.is_cons() {
                extra += 1;
                cursor = self.mc.cdr(cursor);
            }
            return Err(EvalError::WrongArgumentCount {
                name: "this function",
                wanted: names.len(),
                got: names.len() + extra,
            });
        }
        Ok((names, values))
    }

    /// Scan what is left of the argument list as `:keyword value` pairs.
    ///
    /// CLHS says the *leftmost* occurrence of a keyword wins, which is why
    /// this takes the first match rather than the last, and that unknown
    /// keywords are an error unless `&allow-other-keys` was declared.
    fn bind_keys(
        &mut self,
        keys: &[(Value<'h>, Value<'h>)],
        mut arg: Value<'h>,
        marker: Value<'h>,
        allow_other_keys: bool,
        names: &mut Vec<Value<'h>>,
        values: &mut Vec<Value<'h>>,
    ) -> Result<(), EvalError> {
        let mut given: Vec<(Value<'h>, Value<'h>)> = Vec::new();
        while arg.is_cons() {
            let keyword = self.mc.car(arg);
            let rest = self.mc.cdr(arg);
            if !rest.is_cons() {
                return Err(EvalError::BadSyntax {
                    operator: "a function call",
                    detail: "odd number of keyword arguments",
                });
            }
            given.push((keyword, self.mc.car(rest)));
            arg = self.mc.cdr(rest);
        }

        if !allow_other_keys {
            for (keyword, _) in &given {
                if !keys.iter().any(|(k, _)| k == keyword) {
                    return Err(EvalError::BadSyntax {
                        operator: "a function call",
                        detail: "unknown keyword argument",
                    });
                }
            }
        }

        for (keyword, variable) in keys {
            let found = given
                .iter()
                .find(|(k, _)| k == keyword)
                .map(|(_, v)| *v)
                .unwrap_or(marker);
            names.push(*variable);
            values.push(found);
        }
        Ok(())
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
        env::lookup(self.mc, env, name)
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
        // Checked before anything else: past the hard limit the heap will not
        // grow further, so continuing would allocate into a ceiling. One check
        // per step bounds the overshoot to one step's worth of words.
        if self.mc.is_exhausted() {
            return self.fail(EvalError::HeapExhausted);
        }
        match Mode::from_u8(self.mc.mode()) {
            Mode::Eval => self.step_eval(),
            Mode::Return => self.step_return(),
            Mode::Unwind => self.step_unwind(),
            Mode::Halt => Transition::Halt,
        }
    }
}

/// Special operator codes, as stored in a symbol's `SPECIAL` slot.
///
/// Common Lisp has 25 special operators. These six are what the phase 0 gate
/// needs; the rest arrive once the shape of one is settled by worked examples
/// rather than written blind.
pub mod special {
    /// `(quote x)` — return `x` unevaluated.
    pub const QUOTE: i64 = 1;
    /// `(if test then else)` — evaluate one branch.
    pub const IF: i64 = 2;
    /// `(lambda (params) body...)` — make a closure.
    pub const LAMBDA: i64 = 3;
    /// `(progn forms...)` — evaluate in order, yield the last.
    pub const PROGN: i64 = 4;
    /// `(setq var value ...)` — assign.
    pub const SETQ: i64 = 5;
    // 6 was `defun`, which lives in `lisp/prelude.lisp` now — it is a macro
    // in Common Lisp and never should have been an operator here. The number
    // is left unused rather than reused, so an old image cannot silently mean
    // something new.
    //
    // `lambda` (3) stays, and is the interesting case. It is *also* a macro in
    // Common Lisp, expanding to `(function (lambda ...))` — but `function`
    // still has to turn a lambda expression into a closure, so the Rust code
    // below cannot leave. Making the symbol a macro as well would move no
    // work, only a dispatch. The line is in the right place already.
    /// `(let ((v init)...) body...)` — bind variables, all at once.
    pub const LET: i64 = 7;
    /// `(let* ((v init)...) body...)` — bind variables in order.
    pub const LET_STAR: i64 = 8;
    /// `(flet ((f (params) body)...) body...)` — bind functions, not visible
    /// to each other.
    pub const FLET: i64 = 9;
    /// `(labels ((f (params) body)...) body...)` — bind functions that can see
    /// themselves and each other.
    pub const LABELS: i64 = 10;
    /// `(function f)`, written `#'f` — the function a name denotes.
    pub const FUNCTION: i64 = 11;
    /// `(the type form)` — a declaration the interpreter may ignore.
    pub const THE: i64 = 12;
    /// `(locally decls... body...)` — a body with declarations.
    pub const LOCALLY: i64 = 13;
    /// `(eval-when (situations...) body...)` — when this is for.
    pub const EVAL_WHEN: i64 = 14;
    /// `(block name body...)` — a named region that can be left early.
    pub const BLOCK: i64 = 15;
    /// `(return-from name value)` — leave it.
    pub const RETURN_FROM: i64 = 16;
    /// `(catch tag body...)` — a region findable at run time.
    pub const CATCH: i64 = 17;
    /// `(throw tag value)` — leave the nearest matching catch.
    pub const THROW: i64 = 18;
    /// `(tagbody tag form... tag form...)` — a body with labels.
    pub const TAGBODY: i64 = 19;
    /// `(go tag)` — jump to one.
    pub const GO: i64 = 20;
    /// `(unwind-protect body cleanup...)` — cleanup however the body ends.
    pub const UNWIND_PROTECT: i64 = 21;
    /// `(defmacro name (params) body...)` — define a macro.
    pub const DEFMACRO: i64 = 22;
    /// `(%handler-case form (type (var) body...) ...)` — run `form`, and if a
    /// condition is signalled inside it, hand it to a matching clause.
    ///
    /// Named with a `%` because **Common Lisp's `handler-case` is a macro**,
    /// over `handler-bind` and `block`. Quokka has no `handler-bind`, so the
    /// machinery is an operator — but the CL-visible `handler-case` is a
    /// prelude macro over this, which keeps the special operator count honest.
    pub const HANDLER_CASE: i64 = 23;

    /// Every special operator, by name.
    pub const ALL: &[(&str, i64)] = &[
        ("QUOTE", QUOTE),
        ("IF", IF),
        ("LAMBDA", LAMBDA),
        ("PROGN", PROGN),
        ("SETQ", SETQ),
        ("LET", LET),
        ("LET*", LET_STAR),
        ("FLET", FLET),
        ("LABELS", LABELS),
        ("FUNCTION", FUNCTION),
        ("THE", THE),
        ("LOCALLY", LOCALLY),
        ("EVAL-WHEN", EVAL_WHEN),
        ("BLOCK", BLOCK),
        ("RETURN-FROM", RETURN_FROM),
        ("CATCH", CATCH),
        ("THROW", THROW),
        ("TAGBODY", TAGBODY),
        ("GO", GO),
        ("UNWIND-PROTECT", UNWIND_PROTECT),
        ("DEFMACRO", DEFMACRO),
        ("%HANDLER-CASE", HANDLER_CASE),
    ];
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
    /// More body forms to evaluate after this one.
    pub const PROGN: i64 = 3;
    /// Waiting on the value of a `setq`.
    pub const SETQ: i64 = 4;
    /// Evaluating a `let`'s init forms, all in the outer scope.
    pub const LET: i64 = 5;
    /// Evaluating a `let*`'s next init, in the scope built so far.
    pub const LETSTAR: i64 = 6;
    /// A `block`: an unwind target found lexically.
    pub const BLOCK: i64 = 7;
    /// A `catch`: an unwind target found by searching the stack.
    pub const CATCH: i64 = 8;
    /// A `tagbody`, holding where it is up to.
    pub const TAGBODY: i64 = 9;
    /// An `unwind-protect`, whose cleanup runs however the body ends.
    pub const PROTECT: i64 = 10;
    /// Waiting for a cleanup to finish, holding what to do next.
    pub const CLEANUP: i64 = 11;
    /// Waiting on the value of a `return-from`.
    pub const EXIT: i64 = 12;
    /// Waiting on a `catch`'s tag, before the body runs.
    pub const CATCH_PENDING: i64 = 13;
    /// Waiting on a `throw`'s tag, before its value is evaluated.
    pub const THROW_PENDING: i64 = 14;
    /// Waiting for a macro to produce a form, which is then evaluated in place
    /// of the call.
    pub const EXPAND: i64 = 15;
    /// A `handler-case` in progress: its clauses are waiting for a condition.
    pub const HANDLER: i64 = 16;

    /// Slot 0 of every frame: which kind.
    pub const KIND: usize = 0;
    /// A `handler-case`'s clauses, unevaluated.
    pub const HANDLER_CLAUSES: usize = 1;
    /// The environment the clause bodies run in.
    pub const HANDLER_ENV: usize = 2;
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
    /// Body forms still to evaluate.
    pub const PROGN_REST: usize = 1;
    /// The environment the body runs in.
    pub const PROGN_ENV: usize = 2;
    /// Which variable is being assigned.
    pub const SETQ_NAME: usize = 1;
    /// Remaining name/value pairs.
    pub const SETQ_REST: usize = 2;
    /// The environment the assignment happens in.
    pub const SETQ_ENV: usize = 3;
    /// All of a `let`'s bindings, kept for their names.
    pub const LET_BINDINGS: usize = 1;
    /// Bindings whose inits are still to evaluate.
    pub const LET_REMAINING: usize = 2;
    /// Values so far, in reverse.
    pub const LET_DONE: usize = 3;
    /// The body, once the bindings are made.
    pub const LET_BODY: usize = 4;
    /// The outer scope, which every init is evaluated in.
    pub const LET_ENV: usize = 5;
    /// A `let*`'s bindings still to make, the first in flight.
    pub const LETSTAR_REMAINING: usize = 1;
    /// The body.
    pub const LETSTAR_BODY: usize = 2;
    /// The scope built so far.
    pub const LETSTAR_ENV: usize = 3;
    /// Where a macro call was made, so its expansion is evaluated there.
    /// Shares the marker slot, which an `EXPAND` frame has no other use for.
    pub const EXPAND_ENV: usize = 1;
    /// The identity a `block`, `catch` or `tagbody` is unwound to.
    pub const MARKER: usize = 1;
    /// A tagbody's whole body, for resuming at a tag.
    pub const TAGBODY_BODY: usize = 2;
    /// What of it is still to run.
    pub const TAGBODY_REST: usize = 3;
    /// The scope its tags are bound in.
    pub const TAGBODY_ENV: usize = 4;
    /// An `unwind-protect`'s cleanup forms.
    pub const PROTECT_CLEANUP: usize = 1;
    /// The scope they run in.
    pub const PROTECT_ENV: usize = 2;
    /// The value to deliver once a cleanup finishes.
    pub const CLEANUP_VALUE: usize = 1;
    /// What the paused unwind was looking for, or nil if the body returned
    /// normally.
    pub const CLEANUP_TARGET: usize = 2;
    /// The remaining forms of a pending `catch` or `throw`.
    pub const PENDING_REST: usize = 2;

    /// What this frame can be unwound to, if anything.
    pub fn marker_of<'h>(mc: &Mutation<'h>, f: Value<'h>) -> Option<Value<'h>> {
        match kind(mc, f) {
            BLOCK | CATCH | TAGBODY => Some(mc.slot(f, MARKER)),
            _ => None,
        }
    }

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

    /// A frame carrying only an identity: `block`, `catch`, `return-from`.
    pub fn make_marked<'h>(mc: &mut Mutation<'h>, kind: i64, marker: Value<'h>) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 2);
        mc.set_slot(f, KIND, Value::fixnum(kind).expect("small"));
        mc.set_slot(f, MARKER, marker);
        f
    }

    /// A `tagbody`, holding its body and where it is up to.
    pub fn make_tagbody<'h>(
        mc: &mut Mutation<'h>,
        marker: Value<'h>,
        body: Value<'h>,
        rest: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 5);
        mc.set_slot(f, KIND, Value::fixnum(TAGBODY).expect("small"));
        mc.set_slot(f, MARKER, marker);
        mc.set_slot(f, TAGBODY_BODY, body);
        mc.set_slot(f, TAGBODY_REST, rest);
        mc.set_slot(f, TAGBODY_ENV, env);
        f
    }

    /// An `unwind-protect` waiting on its body.
    pub fn make_protect<'h>(
        mc: &mut Mutation<'h>,
        cleanup: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 3);
        mc.set_slot(f, KIND, Value::fixnum(PROTECT).expect("small"));
        mc.set_slot(f, PROTECT_CLEANUP, cleanup);
        mc.set_slot(f, PROTECT_ENV, env);
        f
    }

    /// A paused unwind, or a value waiting on a cleanup.
    pub fn make_cleanup<'h>(
        mc: &mut Mutation<'h>,
        value: Value<'h>,
        target: Value<'h>,
    ) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 3);
        mc.set_slot(f, KIND, Value::fixnum(CLEANUP).expect("small"));
        mc.set_slot(f, CLEANUP_VALUE, value);
        mc.set_slot(f, CLEANUP_TARGET, target);
        f
    }

    /// A `catch` whose tag is being evaluated.
    pub fn make_catch_pending<'h>(mc: &mut Mutation<'h>, rest: Value<'h>) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 3);
        mc.set_slot(f, KIND, Value::fixnum(CATCH_PENDING).expect("small"));
        mc.set_slot(f, PENDING_REST, rest);
        f
    }

    /// A `throw` whose tag is being evaluated.
    pub fn make_throw_pending<'h>(mc: &mut Mutation<'h>, rest: Value<'h>) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 3);
        mc.set_slot(f, KIND, Value::fixnum(THROW_PENDING).expect("small"));
        mc.set_slot(f, PENDING_REST, rest);
        f
    }

    /// A frame remembering the rest of a body.
    pub fn make_progn<'h>(mc: &mut Mutation<'h>, rest: Value<'h>, env: Value<'h>) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 3);
        mc.set_slot(f, KIND, Value::fixnum(PROGN).expect("small"));
        mc.set_slot(f, PROGN_REST, rest);
        mc.set_slot(f, PROGN_ENV, env);
        f
    }

    /// A frame waiting on the value of a `setq`.
    pub fn make_setq<'h>(
        mc: &mut Mutation<'h>,
        name: Value<'h>,
        rest: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 4);
        mc.set_slot(f, KIND, Value::fixnum(SETQ).expect("small"));
        mc.set_slot(f, SETQ_NAME, name);
        mc.set_slot(f, SETQ_REST, rest);
        mc.set_slot(f, SETQ_ENV, env);
        f
    }

    /// A frame part-way through a `let`'s init forms.
    #[allow(clippy::too_many_arguments)]
    pub fn make_let<'h>(
        mc: &mut Mutation<'h>,
        bindings: Value<'h>,
        remaining: Value<'h>,
        done: Value<'h>,
        body: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 6);
        mc.set_slot(f, KIND, Value::fixnum(LET).expect("small"));
        mc.set_slot(f, LET_BINDINGS, bindings);
        mc.set_slot(f, LET_REMAINING, remaining);
        mc.set_slot(f, LET_DONE, done);
        mc.set_slot(f, LET_BODY, body);
        mc.set_slot(f, LET_ENV, env);
        f
    }

    /// A frame waiting on a `let*`'s next init.
    pub fn make_let_star<'h>(
        mc: &mut Mutation<'h>,
        remaining: Value<'h>,
        body: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let f = mc.alloc(ObjectType::Frame, 4);
        mc.set_slot(f, KIND, Value::fixnum(LETSTAR).expect("small"));
        mc.set_slot(f, LETSTAR_REMAINING, remaining);
        mc.set_slot(f, LETSTAR_BODY, body);
        mc.set_slot(f, LETSTAR_ENV, env);
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
        Status::Yielded => Err(EvalError::BudgetExhausted),
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
            crate::boot::install_in(mc).expect("the prelude boots");
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

    // ---- closures, definitions, scope ------------------------------------

    /// Evaluate several forms in one heap, so definitions persist.
    fn eval_all(source: &str) -> Result<String, EvalError> {
        let mut heap = Heap::new();
        let chars = Reader::chars_of(source);
        heap.mutate(|mc| {
            crate::boot::install_in(mc).expect("the prelude boots");
            let mut reader = Reader::new(&chars);
            let mut last = String::new();
            while let Some(form) = reader.read(mc).expect("reads") {
                let value = eval_now(mc, form, PLENTY)?;
                last = print(mc, value);
            }
            Ok(last)
        })
    }

    /// Run a program, sampling the frame stack as it goes, and return the
    /// deepest it ever reached along with the final value.
    ///
    /// Claims about tail calls and loops are claims about *depth*, and a claim
    /// about depth should be measured rather than inferred from the fact that
    /// nothing crashed.
    fn deepest_stack(source: &str, slice: u64) -> (usize, String) {
        use crate::slice::{run_slice, Status};

        let chars = Reader::chars_of(source);
        let mut heap = Heap::new();
        let deepest = heap.mutate(|mc| {
            crate::boot::install_in(mc).expect("the prelude boots");
            let mut reader = Reader::new(&chars);
            let mut deepest = 0usize;
            while let Some(form) = reader.read(mc).expect("reads") {
                let mut machine = Machine::start(mc, form, Value::NIL);
                while run_slice(&mut machine, slice) == Status::Yielded {
                    deepest = deepest.max(machine.stack_depth());
                }
                assert!(machine.error.is_none(), "{:?}", machine.error);
            }
            deepest
        });
        let answer = heap.mutate(|mc| print(mc, mc.register(Register::Value)));
        (deepest, answer)
    }

    fn program_gives(source: &str, expected: &str) {
        assert_eq!(
            eval_all(source).as_deref(),
            Ok(expected),
            "running {source}"
        );
    }

    #[test]
    fn lambda_makes_something_callable() {
        program_gives("((lambda (x) (* x x)) 7)", "49");
        program_gives("((lambda () 42))", "42");
        program_gives("((lambda (a b) (- a b)) 10 3)", "7");
    }

    #[test]
    fn defun_defines_a_function() {
        program_gives("(defun square (x) (* x x)) (square 9)", "81");
        // defun returns the name.
        program_gives("(defun f (x) x)", "F");
    }

    /// A Lisp-2 keeps the variable and the function apart: defining one must
    /// leave the other alone.
    #[test]
    fn a_name_can_be_a_function_and_a_variable_at_once() {
        program_gives(
            "(defun thing (x) (* x 2)) (setq thing 5) (thing thing)",
            "10",
        );
    }

    #[test]
    fn setq_assigns() {
        program_gives("(setq x 1) x", "1");
        program_gives("(setq a 1 b 2) (+ a b)", "3");
        program_gives("(setq x 1) (setq x (+ x 1)) x", "2");
        // Assigning inside a function body touches the binding, not a global.
        program_gives(
            "(setq x 'outer) ((lambda (x) (setq x 'inner) x) 1)",
            "INNER",
        );
        program_gives(
            "(setq x 'outer) ((lambda (x) (setq x 'inner)) 1) x",
            "OUTER",
        );
    }

    #[test]
    fn progn_yields_its_last_form() {
        program_gives("(progn 1 2 3)", "3");
        program_gives("(progn)", "NIL");
        program_gives("(progn (setq x 1) (setq x (+ x 1)) x)", "2");
    }

    /// The thing a closure is for: the captured scope outlives the call that
    /// made it. `adder`'s `n` is gone by the time the returned lambda runs, and
    /// the lambda still knows it.
    #[test]
    fn a_closure_captures_its_scope() {
        program_gives(
            "(defun adder (n) (lambda (x) (+ x n)))
             (setq add5 (adder 5))
             (setq add9 (adder 9))
             (+ (funcall add5 1) (funcall add9 1))",
            "16",
        );
    }

    #[test]
    fn inner_scopes_shadow_outer_ones() {
        program_gives("(setq x 'global) ((lambda (x) x) 'local)", "LOCAL");
        program_gives("(setq x 'global) ((lambda (y) x) 'local)", "GLOBAL");
        program_gives("((lambda (x) ((lambda (x) x) 'inner)) 'outer)", "INNER");
        program_gives("((lambda (x) ((lambda (y) x) 'inner)) 'outer)", "OUTER");
    }

    #[test]
    fn wrong_argument_count_to_a_closure_is_an_error() {
        assert!(matches!(
            eval_all("(defun f (x) x) (f 1 2)"),
            Err(EvalError::WrongArgumentCount { .. })
        ));
    }

    // ---- the phase 0 gate ------------------------------------------------

    /// `(fact 20)` runs. The first half of the phase 0 gate.
    ///
    /// 20! is 2,432,902,008,176,640,000 and the largest fixnum is
    /// 4,611,686,018,427,387,903, so it fits — and 21! does not. The gate asks
    /// for 20 rather than a rounder number because that is the largest
    /// factorial needing no bignums.
    #[test]
    fn the_gate_fact_20() {
        program_gives(
            "(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 20)",
            "2432902008176640000",
        );
        program_gives(
            "(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 0)",
            "1",
        );
        program_gives(
            "(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 5)",
            "120",
        );
    }

    // ---- tail calls ------------------------------------------------------

    /// A tail call must not grow the stack.
    ///
    /// The threshold allows a few dozen frames because `deepest_stack`
    /// measures every form in the source, and expanding the `defun` is itself
    /// a few frames deep. What matters is that the number does not move with
    /// `n`: twenty thousand iterations cost the same as two.
    #[test]
    fn a_tail_call_does_not_grow_the_stack() {
        let (max_depth, answer) = deepest_stack(
            "(defun down (n) (if (= n 0) 'done (down (- n 1)))) (down 20000)",
            50,
        );
        assert_eq!(answer, "DONE");
        assert!(
            max_depth < 50,
            "the stack reached {max_depth} frames; a tail call grew it"
        );
    }

    /// Non-tail recursion must still work, and must still use the stack — a
    /// machine that silently dropped frames would pass the test above.
    #[test]
    fn non_tail_recursion_still_grows_the_stack() {
        let (max_depth, answer) = deepest_stack(
            "(defun sum (n) (if (= n 0) 0 (+ n (sum (- n 1))))) (sum 100)",
            5,
        );
        assert_eq!(answer, "5050");
        assert!(
            max_depth > 100,
            "only {max_depth} frames; the stack should grow here"
        );
    }

    /// **A known limitation, pinned so it cannot change unnoticed.**
    ///
    /// `defun` wraps its body in a block named after the function when the
    /// body mentions `return-from` (see `lisp/prelude.lisp`). A `block`
    /// pushes a frame, and a frame outstanding at a call is precisely what
    /// tail-call elimination must not have — so a function that *both* uses
    /// `return-from` and recurses in tail position costs one frame per call.
    ///
    /// The evaluator cannot simply pop the block frame on a tail call,
    /// because a closure can carry the block's marker out of the block and
    /// call `return-from` later:
    ///
    /// ```lisp
    /// (defun f () (register-callback (lambda () (return-from f 'later))))
    /// ```
    ///
    /// Proving that does not happen is flow analysis, which belongs to the
    /// compiler (phase 1) and not to a tree-walking interpreter. **SBCL's
    /// interpreter behaves the same way** — it stacks `SB-EVAL::EVAL-BLOCK`
    /// frames until the control stack is exhausted — so this is where the
    /// line between interpreter and compiler normally sits, rather than a
    /// fault peculiar to Quokka. Quokka at least fails with
    /// `EvalError::HeapExhausted` instead of a native stack overflow, because
    /// its frames are in its own heap.
    ///
    /// If this test starts failing because the number got *smaller*, someone
    /// has fixed it; delete the test and celebrate.
    #[test]
    fn a_block_in_the_body_still_defeats_the_tail_call() {
        let without = "(defun plain (n) (if (= n 0) 'done (plain (- n 1)))) (plain 5000)";
        let with = "(defun blocked (n)
                      (if (= n 0) (return-from blocked 'done) (blocked (- n 1))))
                    (blocked 5000)";

        let (shallow, answer) = deepest_stack(without, 50);
        assert_eq!(answer, "DONE");
        assert!(shallow < 50, "no block, so no growth: got {shallow}");

        let (deep, answer) = deepest_stack(with, 50);
        assert_eq!(
            answer, "DONE",
            "the answer is still right, only the cost is wrong"
        );
        assert!(
            deep > 4_000,
            "{deep} frames for 5,000 calls — if this dropped, the block frame \
             is being elided now and this test should go"
        );
    }

    // ---- group A: scopes and near-no-ops ---------------------------------

    #[test]
    fn let_binds_all_at_once() {
        program_gives("(let ((x 1) (y 2)) (+ x y))", "3");
        program_gives("(let () 1)", "1");
        program_gives("(let ((x)) x)", "NIL");
        program_gives("(let (x) x)", "NIL");
        // Every init is evaluated in the OUTER scope, which is the whole
        // difference from let*.
        program_gives("(let ((x 1)) (let ((x 2) (y x)) y))", "1");
    }

    #[test]
    fn let_star_binds_in_order() {
        program_gives("(let* ((x 1) (y (+ x 1))) (* x y))", "2");
        program_gives("(let* ((x 1) (x (+ x 1))) x)", "2");
        program_gives("(let ((x 1)) (let* ((x 2) (y x)) y))", "2");
    }

    #[test]
    fn a_binding_is_undone_on_the_way_out() {
        program_gives("(setq g 'global) (let ((g 'inner)) g) g", "GLOBAL");
        // setq inside touches the binding, not the global.
        program_gives(
            "(setq h 'outer) (let ((h 'in)) (setq h 'changed)) h",
            "OUTER",
        );
    }

    /// A Lisp-2 keeps the namespaces apart in *lexical* scopes too, not only on
    /// symbols. A local function named CAR must not disturb the variable, and a
    /// local variable named CAR must not disturb the function.
    #[test]
    fn flet_and_let_bind_in_different_namespaces() {
        // The same name as a variable and as a function at once.
        program_gives(
            "(let ((f 5)) (flet ((f (x) (* x 2))) (list f (f 10))))",
            "(5 20)",
        );
        program_gives(
            "(flet ((f (x) (* x 2))) (let ((f 5)) (list f (f 10))))",
            "(5 20)",
        );
        // And the sharpest demonstration: inside an flet, the name as a
        // *variable* is still unbound. The namespaces do not leak into each
        // other even when one of them has a binding.
        assert!(matches!(
            eval_all("(flet ((f (x) x)) f)"),
            Err(EvalError::UnboundVariable(_))
        ));
        // Deliberately not tested against the oracle: binding a COMMON-LISP
        // symbol is undefined behaviour and SBCL rejects it with a package
        // lock. Quokka has no packages, so this is Quokka's behaviour rather
        // than Common Lisp's.
        program_gives("(let ((car 5)) (car '(1 2)))", "1");
    }

    /// The only difference between them: which scope the closures capture.
    #[test]
    fn labels_sees_itself_and_flet_does_not() {
        program_gives(
            "(labels ((e (n) (if (= n 0) t (o (- n 1))))
                      (o (n) (if (= n 0) nil (e (- n 1)))))
               (list (e 10) (o 10)))",
            "(T NIL)",
        );
        // An flet binding referring to a sibling must not find it.
        assert!(matches!(
            eval_all("(flet ((a () (b)) (b () 1)) (a))"),
            Err(EvalError::UndefinedFunction(_))
        ));
    }

    /// Tail calls must still work through a lexical function binding.
    #[test]
    fn labels_recursion_is_still_tail_recursive() {
        program_gives(
            // Five thousand rather than fifty: PLENTY is a step budget, and
            // each iteration costs a dozen steps. Constant *space* is what is
            // being shown here, and `a_tail_call_does_not_grow_the_stack`
            // measures the depth directly.
            "(labels ((down (n) (if (= n 0) 'done (down (- n 1))))) (down 5000))",
            "DONE",
        );
    }

    #[test]
    fn function_finds_what_a_name_denotes() {
        program_gives("(funcall #'car '(1 2))", "1");
        program_gives("(funcall (function cdr) '(1 2))", "(2)");
        program_gives("(funcall #'(lambda (x) (* x 3)) 14)", "42");
        // A lexical binding shadows the symbol's own function cell.
        program_gives("(flet ((f (x) 'local)) (funcall #'f 1))", "LOCAL");
    }

    #[test]
    fn the_and_locally_reduce_to_their_body() {
        program_gives("(the integer 42)", "42");
        program_gives("(the t (+ 1 1))", "2");
        program_gives("(locally 1 2)", "2");
        program_gives("(locally)", "NIL");
    }

    /// An interpreter has one situation: it is executing.
    #[test]
    fn eval_when_runs_only_for_execute() {
        program_gives("(eval-when (:execute) 'ran)", "RAN");
        program_gives("(eval-when (eval) 'ran)", "RAN");
        program_gives("(eval-when (:compile-toplevel) 'no)", "NIL");
        program_gives("(eval-when () 'no)", "NIL");
    }

    /// A binding list is a list. Treating a non-list as "no bindings" would
    /// quietly accept nonsense, which a test found before SBCL did.
    #[test]
    fn malformed_binding_lists_signal() {
        for source in [
            "(let)",
            "(let 1 2)",
            "(let* 1)",
            "(flet 1)",
            "(flet ((f)) 1)",
        ] {
            assert!(
                matches!(eval_all(source), Err(EvalError::BadSyntax { .. })),
                "{source} should be malformed"
            );
        }
        assert!(matches!(
            eval_all("(function)"),
            Err(EvalError::BadSyntax { .. })
        ));
        assert!(matches!(
            eval_all("(function nosuchthing)"),
            Err(EvalError::UndefinedFunction(_))
        ));
    }

    // ---- nil is a symbol -------------------------------------------------

    /// The point of the static area, and the thing four special cases were
    /// paying for. `nil` and `t` are symbols, as Common Lisp requires.
    #[test]
    fn nil_and_t_are_symbols() {
        program_gives("(symbolp nil)", "T");
        program_gives("(symbolp t)", "T");
        program_gives("(symbolp '())", "T");
        program_gives("(symbolp 'anything)", "T");
        program_gives("(symbolp 1)", "NIL");
        program_gives("(symbolp \"text\")", "NIL");
        program_gives("(symbolp '(1))", "NIL");
    }

    /// And everything `nil` did before still works, which is the part a
    /// representation change could quietly break.
    #[test]
    fn nil_is_still_false_and_the_empty_list() {
        program_gives("(null nil)", "T");
        program_gives("(null '())", "T");
        program_gives("(eq nil '())", "T");
        program_gives("(if nil 'yes 'no)", "NO");
        program_gives("(cdr '(1))", "NIL");
        program_gives("(cons 1 nil)", "(1)");
        program_gives("nil", "NIL");
        program_gives("t", "T");
        program_gives("(eq 'nil nil)", "T");
    }

    /// `(block nil ...)` needed a special case when `nil` was an immediate.
    /// It needs none now, which is what "fixed" means here.
    #[test]
    fn block_nil_needs_no_special_case() {
        program_gives("(block nil (return-from nil 'works))", "WORKS");
        program_gives("(block nil 1 2 3)", "3");
    }

    /// Reading `nil` must find the static symbol rather than making a new one.
    #[test]
    fn nil_is_interned_like_any_symbol() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            assert_eq!(mc.intern("NIL"), Value::NIL);
            assert_eq!(mc.intern("T"), Value::T);
            assert_eq!(mc.symbol_name(Value::NIL), "NIL");
            assert_eq!(mc.symbol_name(Value::T), "T");
            assert!(mc.is_symbol(Value::NIL));
        });
    }

    /// The static area must survive collection without moving, or every
    /// `Value::NIL` in the system would name something else.
    #[test]
    fn nil_does_not_move_when_everything_else_does() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let mut list = Value::NIL;
            for n in 0..100 {
                list = mc.cons(Value::fixnum(n).unwrap(), list);
            }
            mc.set_root(list);
        });
        for _ in 0..5 {
            heap.collect();
            heap.verify().unwrap();
        }
        heap.mutate(|mc| {
            assert!(mc.is_symbol(Value::NIL), "nil stopped being a symbol");
            assert_eq!(mc.symbol_name(Value::NIL), "NIL");
            assert_eq!(mc.intern("NIL"), Value::NIL, "a second nil appeared");
            // And the list that was moved five times is still terminated by it.
            let mut cursor = mc.root();
            let mut count = 0;
            while !cursor.is_nil() {
                count += 1;
                cursor = mc.cdr(cursor);
            }
            assert_eq!(count, 100);
        });
    }

    // ---- group B: non-local exits ----------------------------------------

    #[test]
    fn block_and_return_from_are_lexical() {
        program_gives("(block nil 1 2 3)", "3");
        program_gives("(block nil 1 (return-from nil 2) 3)", "2");
        program_gives("(block done (return-from done 'early) 'late)", "EARLY");
        // The inner block does not catch a return aimed at the outer one.
        program_gives(
            "(block outer (block inner (return-from outer 'far)) 'near)",
            "FAR",
        );
        program_gives(
            "(block outer (block inner (return-from inner 'in)) 'after)",
            "AFTER",
        );
        program_gives("(block b (return-from b (+ 1 2)))", "3");
        program_gives("(block b (return-from b))", "NIL");
    }

    /// Two entries to the same block are two different blocks, which is why the
    /// target is a fresh object rather than the name.
    #[test]
    fn recursive_blocks_do_not_collide() {
        program_gives(
            "(defun f (n) (block b (if (= n 0) (return-from b 'bottom) (f (- n 1)))))
             (f 5)",
            "BOTTOM",
        );
    }

    /// catch is the dynamic version: the thrower need not know where the catch
    /// is, or even that there is one.
    #[test]
    fn catch_and_throw_are_dynamic() {
        program_gives("(catch 'tag 1 2 3)", "3");
        program_gives("(catch 'tag (throw 'tag 42) 'unreached)", "42");
        program_gives("(catch 'a (catch 'b (throw 'a 'through)))", "THROUGH");
        program_gives("(catch 'a (catch 'b (throw 'b 'inner)))", "INNER");
        // Through fifty frames of a caller that knows nothing about it.
        program_gives(
            "(defun deep (n) (if (= n 0) (throw 'out 'arrived) (deep (- n 1))))
             (catch 'out (deep 50))",
            "ARRIVED",
        );
    }

    #[test]
    fn tagbody_and_go_make_a_loop() {
        program_gives("(tagbody)", "NIL");
        program_gives("(tagbody a b c)", "NIL");
        program_gives(
            "(setq n 0) (tagbody top (setq n (+ n 1)) (if (< n 5) (go top))) n",
            "5",
        );
        program_gives(
            "(setq out nil) (tagbody (go skip) (setq out 'wrong) skip (setq out 'right)) out",
            "RIGHT",
        );
    }

    /// A tagbody loop must run in **constant space**, or every looping macro
    /// in the language inherits a limit from the construct it expands into.
    ///
    /// Measured rather than inferred: the frame is replaced on each iteration
    /// rather than pushed, and the way to show that is to watch the depth.
    #[test]
    fn a_tagbody_loop_does_not_grow_the_stack() {
        let (deepest, answer) = deepest_stack(
            "(setq n 0) (tagbody top (setq n (+ n 1)) (if (< n 20000) (go top))) n",
            50,
        );
        assert_eq!(answer, "20000");
        assert!(
            deepest < 20,
            "twenty thousand iterations reached {deepest} frames"
        );
    }

    /// And a `catch` that is never thrown to must not accumulate either, since
    /// a loop wrapped in one is ordinary.
    #[test]
    fn repeated_catches_do_not_accumulate() {
        let (deepest, answer) = deepest_stack(
            "(defun f (n) (if (= n 0) 'done (catch 'tag (f (- n 1)))))
             (f 200)",
            50,
        );
        assert_eq!(answer, "DONE");
        // This one *should* grow — a catch is a frame that must be there to be
        // found. The point is that it grows linearly rather than faster.
        assert!(
            deepest > 200 && deepest < 1_000,
            "200 nested catches reached {deepest} frames"
        );
    }

    #[test]
    fn unwind_protect_cleans_up_on_a_normal_return() {
        program_gives(
            "(setq log nil) (unwind-protect 'body (setq log 'cleaned)) log",
            "CLEANED",
        );
        // The body's value survives; the cleanup's is discarded.
        program_gives("(unwind-protect 'kept 'discarded)", "KEPT");
    }

    /// The hard case: the unwind has to *pause*, run ordinary code, and then
    /// carry on from where it was.
    #[test]
    fn unwind_protect_cleans_up_while_being_unwound_through() {
        program_gives(
            "(setq log nil) (block b (unwind-protect (return-from b 'left) (setq log 'cleaned))) log",
            "CLEANED",
        );
        program_gives(
            "(setq log nil) (catch 'x (unwind-protect (throw 'x 'thrown) (setq log 'cleaned))) log",
            "CLEANED",
        );
        // And the unwind continues past it to a target further out.
        program_gives(
            "(catch 'outer (unwind-protect (catch 'inner (throw 'outer 'past)) nil))",
            "PAST",
        );
    }

    /// Nested cleanups run innermost first, which is the only order that makes
    /// sense and is worth pinning.
    #[test]
    fn nested_cleanups_run_inside_out() {
        program_gives(
            "(setq log nil)
             (catch 'x
               (unwind-protect
                   (unwind-protect (throw 'x 'done) (setq log (cons 'inner log)))
                 (setq log (cons 'outer log))))
             log",
            "(OUTER INNER)",
        );
    }

    #[test]
    fn an_exit_with_no_target_signals() {
        for source in [
            "(return-from nowhere 1)",
            "(throw 'nothing 1)",
            "(go nowhere)",
        ] {
            assert!(
                matches!(eval_all(source), Err(EvalError::NoSuchExit(_))),
                "{source} should have nowhere to go"
            );
        }
        assert!(matches!(
            eval_all("(block)"),
            Err(EvalError::BadSyntax { .. })
        ));
        assert!(matches!(
            eval_all("(catch)"),
            Err(EvalError::BadSyntax { .. })
        ));
    }

    // ---- not finishing ---------------------------------------------------

    /// A recursion with no base case must stop, and stop *cheaply*.
    ///
    /// `(fact -1)` never reaches `(= n 0)`, and because the `*` is still
    /// pending it is not a tail call, so every level keeps its frames.
    /// Collection cannot help: they are all reachable. Only a ceiling can.
    #[test]
    fn a_recursion_that_never_ends_hits_the_heap_ceiling() {
        // Defined with the primitive rather than `defun`, so the 4,000-word
        // heap holds only what this test is about. `defun` is a prelude macro
        // now, and the prelude does not fit in a heap this small — which is
        // the right answer for a test of the ceiling itself.
        let source = "(%set-symbol-function 'fact\n              (lambda (n) (if (= n 0) 1 (* n (fact (- n 1))))))\n             (fact -1)";
        let chars = Reader::chars_of(source);
        let mut heap = Heap::with_soft_limit(4_000);
        let limit = heap.hard_limit();

        let outcome = heap.mutate(|mc| {
            primitives::install(mc);
            let mut reader = Reader::new(&chars);
            let definition = reader.read(mc).unwrap().unwrap();
            eval_now(mc, definition, PLENTY).unwrap();
            let call = reader.read(mc).unwrap().unwrap();
            // The value cannot leave the mutation, and is not wanted: the
            // error is the subject. Mapping it away is the brand doing its job.
            eval_now(mc, call, u64::MAX).map(|_| ())
        });

        assert_eq!(outcome, Err(EvalError::HeapExhausted));
        // The overshoot past the ceiling is one step's worth of words, not a
        // multiple of the heap.
        assert!(
            heap.used_words() < limit + 64,
            "overshot the ceiling by {} words",
            heap.used_words() - limit
        );
    }

    /// And the heap must be usable afterwards. An error that leaves the system
    /// unusable is only marginally better than a crash.
    #[test]
    fn the_heap_recovers_after_exhaustion() {
        // Defined with the primitive rather than `defun`, so the 4,000-word
        // heap holds only what this test is about. `defun` is a prelude macro
        // now, and the prelude does not fit in a heap this small — which is
        // the right answer for a test of the ceiling itself.
        let source = "(%set-symbol-function 'fact\n              (lambda (n) (if (= n 0) 1 (* n (fact (- n 1))))))\n             (fact -1)";
        let chars = Reader::chars_of(source);
        let mut heap = Heap::with_soft_limit(4_000);

        heap.mutate(|mc| {
            primitives::install(mc);
            let mut reader = Reader::new(&chars);
            let definition = reader.read(mc).unwrap().unwrap();
            eval_now(mc, definition, PLENTY).unwrap();
            let call = reader.read(mc).unwrap().unwrap();
            assert_eq!(
                eval_now(mc, call, u64::MAX).map(|_| ()),
                Err(EvalError::HeapExhausted)
            );
        });

        // What filled the heap is unreachable now, so collecting frees it.
        assert!(heap.is_exhausted());
        heap.collect();
        heap.clear_exhausted();
        assert!(!heap.is_exhausted());
        heap.verify().unwrap();

        let answer = heap.mutate(|mc| {
            let chars = Reader::chars_of("(fact 20)");
            let mut reader = Reader::new(&chars);
            let call = reader.read(mc).unwrap().unwrap();
            let value = eval_now(mc, call, PLENTY).unwrap();
            print(mc, value)
        });
        assert_eq!(answer, "2432902008176640000");
    }

    /// A budget that runs out is reported as itself, not as a syntax error.
    #[test]
    fn running_out_of_budget_says_so() {
        let mut heap = Heap::new();
        let chars = Reader::chars_of("(+ 1 (* 2 (- 10 (+ 3 4))))");
        let outcome = heap.mutate(|mc| {
            crate::boot::install_in(mc).expect("the prelude boots");
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            eval_now(mc, form, 3).map(|_| ())
        });
        assert_eq!(outcome, Err(EvalError::BudgetExhausted));
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
                crate::boot::install_in(mc).expect("the prelude boots");
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
            crate::boot::install_in(mc).expect("the prelude boots");
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
            crate::boot::install_in(mc).expect("the prelude boots");
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

/// Lexical environments: a chain of scopes.
///
/// Each scope is one heap object: a parent link, then name/value pairs. Looking
/// a name up scans this scope's names, then follows the parent, then falls back
/// to the symbol's own value cell, which is where globals live.
///
/// # Why not an association list
///
/// The textbook tree-walker uses an alist — `((x . 1) (y . 2))`, extended by
/// consing onto the front. It is shorter, and this is not.
///
/// Two reasons. A two-argument call costs one object of six words here against
/// four cons cells of eight there, with better locality. And, decisively, this
/// is the layout *lexical addressing* uses: once the compiler of phase 1 can
/// resolve `x` to "scope 2, slot 0" at compile time, the name scan below
/// becomes an index and the data structure does not change. An alist would have
/// to be thrown away.
///
/// The genuinely fast option — lexical addressing — is not available yet,
/// because resolving a name to a slot number requires a compiler to walk the
/// body, and a tree-walking interpreter re-reads the source each time with
/// nowhere to attach the answer. This is the efficient option that exists.
pub mod env {
    use super::*;

    /// Slot 0: the enclosing scope, or `nil`.
    pub const PARENT: usize = 0;
    /// Slot 1: which namespace this scope binds in.
    pub const NAMESPACE: usize = 1;
    /// Name/value pairs begin here.
    pub const FIRST_PAIR: usize = 2;

    /// Variables — what `let` binds and a symbol evaluates to.
    pub const VARIABLE: i64 = 0;
    /// Block names — what `block` binds and `return-from` looks up.
    ///
    /// A third and fourth namespace, because `(block list ...)` must not
    /// disturb either the variable `list` or the function. Common Lisp has
    /// several namespaces, not two, and this is two of the others.
    pub const BLOCK: i64 = 2;
    /// `tagbody` tags — what `go` looks up.
    pub const TAG: i64 = 3;

    /// Functions — what `flet` and `labels` bind, and what the operator
    /// position of a call looks up.
    ///
    /// Two namespaces rather than one is what makes this a Lisp-2, and the
    /// separation has to reach into lexical scopes as well as symbols: `(flet
    /// ((list (x) x)) (list 1))` must call the local function while `list` the
    /// variable is untouched.
    pub const FUNCTION: i64 = 1;

    /// A scope binding `names` to `values` inside `parent`.
    pub fn make<'h>(
        mc: &mut Mutation<'h>,
        parent: Value<'h>,
        namespace: i64,
        names: &[Value<'h>],
        values: &[Value<'h>],
    ) -> Value<'h> {
        debug_assert_eq!(names.len(), values.len());
        let scope = mc.alloc(ObjectType::Environment, FIRST_PAIR + names.len() * 2);
        mc.set_slot(scope, PARENT, parent);
        mc.set_slot(scope, NAMESPACE, Value::fixnum(namespace).expect("small"));
        for (i, (name, value)) in names.iter().zip(values).enumerate() {
            mc.set_slot(scope, FIRST_PAIR + i * 2, *name);
            mc.set_slot(scope, FIRST_PAIR + 1 + i * 2, *value);
        }
        scope
    }

    /// Where `name` is bound in `namespace`: the scope and the slot holding it.
    ///
    /// Scopes of the other namespace are stepped over rather than searched,
    /// which is how a local function and a local variable of the same name stay
    /// apart.
    fn find<'h>(
        mc: &Mutation<'h>,
        env: Value<'h>,
        namespace: i64,
        name: Value<'h>,
    ) -> Option<(Value<'h>, usize)> {
        let mut scope = env;
        // Terminating on `is_object` was right until `nil` became one. The
        // chain ends at `nil`, which is a *symbol* in the static area — so the
        // loop asks what kind of object it is rather than whether it is one.
        // That is also total: anything unexpected ends the walk rather than
        // being read as a scope.
        while scope.is_object() && mc.header(scope).object_type == ObjectType::Environment {
            if mc.slot(scope, NAMESPACE).as_fixnum() == Some(namespace) {
                let slots = mc.header(scope).slots;
                let mut i = FIRST_PAIR;
                while i < slots {
                    if mc.slot(scope, i) == name {
                        return Some((scope, i + 1));
                    }
                    i += 2;
                }
            }
            scope = mc.slot(scope, PARENT);
        }
        None
    }

    /// The value of `name`, or `None` if it is unbound everywhere.
    pub fn lookup<'h>(mc: &Mutation<'h>, env: Value<'h>, name: Value<'h>) -> Option<Value<'h>> {
        if let Some((scope, slot)) = find(mc, env, VARIABLE, name) {
            return Some(mc.slot(scope, slot));
        }
        // Globals live in the symbol's own value cell, which #4 already gave us.
        let global = mc.slot(name, symbol::VALUE);
        if global == Value::UNBOUND {
            None
        } else {
            Some(global)
        }
    }

    /// The function `name` denotes: a lexical binding from `flet` or `labels`
    /// first, then the symbol's own function cell.
    pub fn lookup_function<'h>(
        mc: &Mutation<'h>,
        env: Value<'h>,
        name: Value<'h>,
    ) -> Option<Value<'h>> {
        if let Some((scope, slot)) = find(mc, env, FUNCTION, name) {
            return Some(mc.slot(scope, slot));
        }
        let global = mc.slot(name, symbol::FUNCTION);
        if global == Value::UNBOUND {
            None
        } else {
            Some(global)
        }
    }

    /// The marker a block name or a tag is bound to, if any.
    ///
    /// Lexical: `return-from` and `go` can only reach a block or tag that
    /// encloses them in the source, which is what makes them safe. `throw`
    /// is the dynamic alternative and does not come through here.
    pub fn lookup_marker<'h>(
        mc: &Mutation<'h>,
        env: Value<'h>,
        namespace: i64,
        name: Value<'h>,
    ) -> Option<Value<'h>> {
        find(mc, env, namespace, name).map(|(scope, slot)| mc.slot(scope, slot))
    }

    /// Assign to `name`, creating a global if it is bound nowhere.
    pub fn assign<'h>(mc: &mut Mutation<'h>, env: Value<'h>, name: Value<'h>, value: Value<'h>) {
        if let Some((scope, slot)) = find(mc, env, VARIABLE, name) {
            mc.set_slot(scope, slot, value);
        } else {
            mc.set_slot(name, symbol::VALUE, value);
        }
    }
}

/// Closures: a function plus the scope it was written in.
///
/// Capturing the environment is what makes `(let ((x 10)) (lambda () x))` still
/// know that `x` is 10 after the `let` has finished. Without it a lambda could
/// only see globals.
pub mod closure {
    use super::*;

    /// The parameter list.
    pub const PARAMS: usize = 0;
    /// The body forms, an implicit `progn`.
    pub const BODY: usize = 1;
    /// The scope the lambda was evaluated in.
    pub const ENV: usize = 2;

    /// Make a closure.
    pub fn make<'h>(
        mc: &mut Mutation<'h>,
        params: Value<'h>,
        body: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let c = mc.alloc(ObjectType::Closure, 3);
        mc.set_slot(c, PARAMS, params);
        mc.set_slot(c, BODY, body);
        mc.set_slot(c, ENV, env);
        c
    }

    /// Is this a closure?
    pub fn is<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
        v.is_object() && mc.header(v).object_type == ObjectType::Closure
    }

    /// Make a macro: the same three slots, a different type.
    pub fn make_macro<'h>(
        mc: &mut Mutation<'h>,
        params: Value<'h>,
        body: Value<'h>,
        env: Value<'h>,
    ) -> Value<'h> {
        let m = mc.alloc(ObjectType::Macro, 3);
        mc.set_slot(m, PARAMS, params);
        mc.set_slot(m, BODY, body);
        mc.set_slot(m, ENV, env);
        m
    }

    /// Is this a macro?
    pub fn is_macro<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
        v.is_object() && mc.header(v).object_type == ObjectType::Macro
    }

    /// Is this either?
    pub fn is_callable<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
        is(mc, v) || is_macro(mc, v)
    }
}
