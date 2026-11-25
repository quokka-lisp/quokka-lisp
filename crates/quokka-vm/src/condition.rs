//! Conditions: errors as objects a program can catch.
//!
//! Up to here, an error ended the computation. `EvalError` is a Rust enum, the
//! machine halts carrying one, and the REPL prints it. Nothing written in Lisp
//! could do anything about it, which makes a whole class of program —
//! including a compiler that wants to report a bad form and carry on —
//! impossible to write.
//!
//! # What a condition is here
//!
//! An object with a **type** (a symbol), a **message** (a string or nil) and
//! some **data** (a list). That is all.
//!
//! What it is *not* is a CLOS instance. In Common Lisp conditions are classes
//! in a hierarchy — `simple-error` is an `error` is a `serious-condition` is a
//! `condition` — and `handler-case` matches by subtype. Quokka has no classes
//! (that is phase 3), so type matching is deliberately flat:
//!
//! - a clause naming `error` or `t` catches everything;
//! - any other clause catches a condition whose type symbol is `eq` to it.
//!
//! That is enough for tier 0, whose purpose is a compiler that can report a
//! problem and a caller that can catch one. It is a real divergence from the
//! standard and it is written down in `docs/design.md` rather than discovered.
//!
//! # Where the evaluator's own errors go
//!
//! An `EvalError` becomes a condition when one is signalled inside a
//! `handler-case`, so `(handler-case (car 5) (error (c) 'caught))` works. The
//! type symbols are the standard's names — `type-error`, `unbound-variable`,
//! `undefined-function`, `program-error`, `control-error`,
//! `arithmetic-error` — so a program written against Common Lisp catches the
//! things it expects to catch.
//!
//! Two errors are deliberately **not** conditions: `HeapExhausted` and
//! `BudgetExhausted`. Neither is a fault in the program being run — one is the
//! system running out of room and the other is the caller declining to wait —
//! and a handler that caught them would be catching the machinery rather than
//! the program. `docs/design.md` has heap exhaustion becoming a
//! `storage-condition` in phase 3, when there is a hierarchy to put it in.

use crate::error::EvalError;
use crate::heap::{Mutation, ObjectType};
use crate::value::Value;

/// The condition's type, as a symbol.
pub const TYPE: usize = 0;
/// A message string, or nil.
pub const MESSAGE: usize = 1;
/// Any extra data, as a list.
pub const DATA: usize = 2;
/// How many slots a condition has.
pub const SLOTS: usize = 3;

/// Build a condition.
pub fn make<'h>(
    mc: &mut Mutation<'h>,
    type_name: Value<'h>,
    message: Value<'h>,
    data: Value<'h>,
) -> Value<'h> {
    let c = mc.alloc(ObjectType::Condition, SLOTS);
    mc.set_slot(c, TYPE, type_name);
    mc.set_slot(c, MESSAGE, message);
    mc.set_slot(c, DATA, data);
    c
}

/// Is this a condition?
pub fn is_condition<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::Condition
}

/// Does a `handler-case` clause naming `wanted` catch `condition`?
///
/// Flat matching, as the module documentation explains: `error` and `t` catch
/// everything, and anything else must name the condition's own type.
pub fn matches<'h>(mc: &Mutation<'h>, wanted: Value<'h>, condition: Value<'h>) -> bool {
    if wanted == Value::T || mc.symbol_is(wanted, "ERROR") || mc.symbol_is(wanted, "CONDITION") {
        return true;
    }
    wanted == mc.slot(condition, TYPE)
}

/// The condition an `EvalError` becomes, or `None` if it is not a fault in the
/// program and so should stop the machine instead.
///
/// The type symbols are Common Lisp's, so a program that catches
/// `undefined-function` catches what it expects to.
pub fn from_eval_error<'h>(mc: &mut Mutation<'h>, e: &EvalError) -> Option<Value<'h>> {
    let (type_name, message) = classify(e)?;
    let type_name = mc.intern(type_name);
    let message = mc.alloc_string(&message);
    Some(make(mc, type_name, message, Value::NIL))
}

/// The condition types CLHS names that a test may tell apart, most of them
/// leaves of the lattice. Anything else — a `simple-error`, a type a
/// program made up — is reported as plain `error`.
const REPORTED: &[&str] = &[
    "ARITHMETIC-ERROR",
    "CELL-ERROR",
    "CONTROL-ERROR",
    "DIVISION-BY-ZERO",
    "END-OF-FILE",
    "FILE-ERROR",
    "FLOATING-POINT-INEXACT",
    "FLOATING-POINT-INVALID-OPERATION",
    "FLOATING-POINT-OVERFLOW",
    "FLOATING-POINT-UNDERFLOW",
    "PACKAGE-ERROR",
    "PARSE-ERROR",
    "PRINT-NOT-READABLE",
    "PROGRAM-ERROR",
    "READER-ERROR",
    "STREAM-ERROR",
    "TYPE-ERROR",
    "UNBOUND-SLOT",
    "UNBOUND-VARIABLE",
    "UNDEFINED-FUNCTION",
];

/// The line a differential test prints for a form that signalled:
/// `!! type-error`, or `!! error` for a condition with no standard type of
/// its own. `scripts/oracle.lisp` prints the same line for SBCL.
///
/// The type is in the line because without it every error looked alike, and
/// Quokka failing for the wrong reason — an undefined function, say — agreed
/// with SBCL signalling the `type-error` the test was written for (#103).
pub fn report(condition_type: &str) -> String {
    if REPORTED.contains(&condition_type) {
        format!("!! {}\n", condition_type.to_lowercase())
    } else {
        String::from("!! error\n")
    }
}

/// [`report`] for an error that reached the top as an [`EvalError`].
pub fn report_error(e: &EvalError) -> String {
    match e {
        // "TYPE: message", as `describe` writes an uncaught condition.
        EvalError::Signalled(what) => report(what.split(':').next().unwrap_or("")),
        _ => report(classify(e).map_or("", |(type_name, _)| type_name)),
    }
}

/// The condition type and message an `EvalError` stands for, or `None` if
/// it is not the program's fault.
fn classify(e: &EvalError) -> Option<(&'static str, String)> {
    Some(match e {
        EvalError::UnboundVariable(name) => ("UNBOUND-VARIABLE", format!("{name} is unbound")),
        EvalError::UndefinedFunction(name) => {
            ("UNDEFINED-FUNCTION", format!("{name} is undefined"))
        }
        EvalError::NotAFunction(what) => ("TYPE-ERROR", format!("{what} is not a function")),
        EvalError::NoSuchExit(what) => ("CONTROL-ERROR", format!("no exit named {what}")),
        EvalError::BadSyntax { operator, detail } => {
            ("PROGRAM-ERROR", format!("{operator} {detail}"))
        }
        EvalError::WrongArgumentCount { name, wanted, got } => (
            "PROGRAM-ERROR",
            format!("{name} wants {wanted} arguments and got {got}"),
        ),
        EvalError::WrongArgumentsTo {
            function,
            wanted,
            got,
        } => (
            "PROGRAM-ERROR",
            format!("{function} wants {wanted} arguments and got {got}"),
        ),
        EvalError::WrongType { name, wanted } => ("TYPE-ERROR", format!("{name} wants {wanted}")),
        EvalError::BadCall(detail) => ("PROGRAM-ERROR", format!("bad call: {detail}")),
        EvalError::IndexOutOfBounds { index, length } => (
            "TYPE-ERROR",
            format!("index {index} is outside a sequence of length {length}"),
        ),
        EvalError::IntegerOverflow => (
            "ARITHMETIC-ERROR",
            String::from("integer overflow, and bignums do not exist yet"),
        ),
        EvalError::Arithmetic { condition, detail } => (*condition, String::from(*detail)),
        // Already a condition that nobody wanted; re-signalling it would
        // loop. It reaches here only after the stack has been exhausted.
        EvalError::Signalled(_) => return None,
        // Not the program's fault, so not the program's to catch.
        //
        // `Malformed` is here for a sharper reason than the other two: a
        // function the verifier refused was produced by the compiler, and a
        // `handler-case` quietly swallowing that would hide a compiler bug
        // behind whatever the program does about errors.
        EvalError::Malformed(_)
        | EvalError::Fault { .. }
        | EvalError::HeapExhausted
        | EvalError::BudgetExhausted
        | EvalError::Blocked => return None,
    })
}

/// How a condition prints when nothing caught it.
pub fn describe<'h>(mc: &Mutation<'h>, condition: Value<'h>) -> String {
    let type_name = mc.symbol_name(mc.slot(condition, TYPE));
    let message = mc.slot(condition, MESSAGE);
    if message.is_object() && mc.header(message).object_type.is_string() {
        format!("{type_name}: {}", mc.string_to_rust(message))
    } else {
        type_name
    }
}
