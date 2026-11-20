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

use crate::eval::EvalError;
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
    let (type_name, message) = match e {
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
        EvalError::WrongType { name, wanted } => ("TYPE-ERROR", format!("{name} wants {wanted}")),
        EvalError::IndexOutOfBounds { index, length } => (
            "TYPE-ERROR",
            format!("index {index} is outside a sequence of length {length}"),
        ),
        EvalError::IntegerOverflow => (
            "ARITHMETIC-ERROR",
            String::from("integer overflow, and bignums do not exist yet"),
        ),
        // Already a condition that nobody wanted; re-signalling it would
        // loop. It reaches here only after the stack has been exhausted.
        EvalError::Signalled(_) => return None,
        // Not the program's fault, so not the program's to catch.
        EvalError::HeapExhausted | EvalError::BudgetExhausted => return None,
    };
    let type_name = mc.intern(type_name);
    let message = mc.alloc_string(&message);
    Some(make(mc, type_name, message, Value::NIL))
}

/// How a condition prints when nothing caught it.
pub fn describe<'h>(mc: &Mutation<'h>, condition: Value<'h>) -> String {
    let type_name = mc.symbol_name(mc.slot(condition, TYPE));
    let message = mc.slot(condition, MESSAGE);
    if message.is_object() && mc.header(message).object_type == ObjectType::String {
        format!("{type_name}: {}", mc.string_to_rust(message))
    } else {
        type_name
    }
}
