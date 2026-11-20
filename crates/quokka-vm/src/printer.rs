//! The printer: heap objects out, text in.
//!
//! The counterpart to [`crate::reader`], and the P in REPL. Minimal by design —
//! enough to see what was read and to compare against another Lisp, which is
//! what the differential tests need.
//!
//! # What it does not do yet
//!
//! - **Cycles.** A list whose tail points back at itself prints forever.
//!   Common Lisp's answer is `*print-circle*`, which labels shared structure as
//!   `#1=(a . #1#)`; `docs/design.md` defers it. Until then [`MAX_LENGTH`]
//!   stops the printer rather than the process, which is a crude stand-in and
//!   is honest about it.
//! - **`princ` versus `prin1`.** Common Lisp distinguishes printing *for a
//!   program to read back* (strings quoted, characters as `#\a`) from printing
//!   *for a person* (contents only). Only the first exists here, because that is
//!   what a REPL and a test oracle want.
//! - **Pretty-printing.** Everything is one line.

use crate::heap::{Mutation, ObjectType};
use crate::value::{Kind, Value};
use core::fmt::Write;

/// How many list elements to print before giving up.
///
/// A blunt guard against a circular list, until `*print-circle*` exists.
pub const MAX_LENGTH: usize = 10_000;

/// Print a value the way the reader would want it back.
pub fn print<'h>(mc: &Mutation<'h>, v: Value<'h>) -> String {
    let mut out = String::new();
    write_value(&mut out, mc, v);
    out
}

fn write_value<'h>(out: &mut String, mc: &Mutation<'h>, v: Value<'h>) {
    match v.kind() {
        Kind::Fixnum => {
            let _ = write!(out, "{}", v.as_fixnum().expect("fixnum"));
        }
        Kind::Immediate => write_immediate(out, v),
        Kind::Cons => write_list(out, mc, v),
        Kind::Object => write_object(out, mc, v),
        // A forwarding pointer in a live heap is a collector bug; say so rather
        // than printing something plausible.
        Kind::Forwarding => out.push_str("#<forwarded>"),
    }
}

fn write_immediate(out: &mut String, v: Value<'_>) {
    // nil and t are not here any more: they are symbols, printed by name like
    // any other. Only the unbound marker is left, and it is not a Lisp object.
    if v == Value::UNBOUND {
        out.push_str("#<unbound>");
    } else if let Some(index) = v.as_primitive() {
        // Common Lisp prints a function unreadably, since no text reads back as
        // one. The name is for a person reading a REPL, and is why nothing
        // containing `#<` may appear in a differential test.
        match crate::primitives::name_of(index) {
            Some(name) => {
                let _ = write!(out, "#<function {name}>");
            }
            None => out.push_str("#<function>"),
        }
    } else if let Some(ch) = v.as_character() {
        out.push_str("#\\");
        out.push_str(&match ch {
            ' ' => "Space".to_string(),
            '\n' => "Newline".to_string(),
            '\t' => "Tab".to_string(),
            '\r' => "Return".to_string(),
            '\0' => "Nul".to_string(),
            other => other.to_string(),
        });
    } else {
        out.push_str("#<immediate>");
    }
}

fn write_list<'h>(out: &mut String, mc: &Mutation<'h>, v: Value<'h>) {
    out.push('(');
    let mut cursor = v;
    let mut printed = 0;
    loop {
        write_value(out, mc, mc.car(cursor));
        cursor = mc.cdr(cursor);
        printed += 1;

        if cursor.is_nil() {
            break;
        }
        if printed >= MAX_LENGTH {
            out.push_str(" ...");
            break;
        }
        if !cursor.is_cons() {
            // An improper list: (a . b). The dot is why cons cells are pairs
            // rather than list nodes — a list is just the common case.
            out.push_str(" . ");
            write_value(out, mc, cursor);
            break;
        }
        out.push(' ');
    }
    out.push(')');
}

fn write_object<'h>(out: &mut String, mc: &Mutation<'h>, v: Value<'h>) {
    let header = mc.header(v);
    match header.object_type {
        ObjectType::Symbol => out.push_str(&mc.symbol_name(v)),
        ObjectType::String => {
            out.push('"');
            for ch in mc.string_to_rust(v).chars() {
                if ch == '"' || ch == '\\' {
                    out.push('\\');
                }
                out.push(ch);
            }
            out.push('"');
        }
        ObjectType::Closure => {
            // Common Lisp prints functions unreadably, since there is no text
            // that reads back as this closure with its captured scope.
            out.push_str("#<function>");
        }
        other => {
            let _ = write!(out, "#<{other:?} {} slots>", header.slots);
        }
    }
}
