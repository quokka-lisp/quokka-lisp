//! The printer: heap objects out, text in.
//!
//! The counterpart to the Rust reader (`quokka_bootstrap::reader`), and the P
//! in the bootstrap's REPL. Minimal by design —
//! enough to see what was read and to compare against another Lisp, which is
//! what the differential tests need.
//!
//! # What it does not do yet
//!
//! - **Cycles.** A structure that points back at itself would print forever.
//!   Common Lisp's answer is `*print-circle*`, which labels shared structure as
//!   `#1=(a . #1#)`, and the Lisp printer has it (#64). This one has three
//!   blunt limits instead — [`MAX_LENGTH`], [`MAX_DEPTH`], [`MAX_CHARS`] — so
//!   that it stops the printing rather than the process (#90). They are
//!   `*print-length*` and `*print-level*` with fixed values, and `#` for a
//!   level too deep is CLHS 22.1.3.5's notation.
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

/// How deep to follow cars and elements before printing `#`.
///
/// The printer recurses on nesting, in Rust, and rule 4 allows no Rust
/// recursion whose depth data decides: a list that is its own car, or merely
/// 200,000 levels deep, overflowed the stack — about a megabyte on wasm32.
pub const MAX_DEPTH: usize = 64;

/// How many bytes to print before giving up with `...`.
///
/// Length and depth together still allow 10,000^64 elements, and a shared
/// structure — `(x x)` forty times over — is small in the heap and 2^40 long
/// in print.
pub const MAX_CHARS: usize = 1 << 20;

/// Print a value the way the reader would want it back.
pub fn print<'h>(mc: &Mutation<'h>, v: Value<'h>) -> String {
    let mut out = String::new();
    write_value(&mut out, mc, v, 0);
    out
}

/// Has the output reached [`MAX_CHARS`]? Says so, once, when it has.
fn full(out: &mut String) -> bool {
    if out.len() >= MAX_CHARS && !out.ends_with("...") {
        out.push_str("...");
    }
    out.len() >= MAX_CHARS
}

fn write_value<'h>(out: &mut String, mc: &Mutation<'h>, v: Value<'h>, depth: usize) {
    // Only what has parts can nest: a cons or a vector.
    let nests = v.is_cons() || v.is_object() && mc.header(v).object_type == ObjectType::Vector;
    if depth > MAX_DEPTH && nests {
        out.push('#');
        return;
    }
    match v.kind() {
        Kind::Fixnum => {
            let _ = write!(out, "{}", v.as_fixnum().expect("fixnum"));
        }
        Kind::Immediate => write_immediate(out, v),
        Kind::Cons => write_list(out, mc, v, depth),
        Kind::Object => write_object(out, mc, v, depth),
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
            // SBCL's way, which the Lisp printer follows too (#46).
            ' ' => " ".to_string(),
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

fn write_list<'h>(out: &mut String, mc: &Mutation<'h>, v: Value<'h>, depth: usize) {
    out.push('(');
    let mut cursor = v;
    let mut printed = 0;
    loop {
        write_value(out, mc, mc.car(cursor), depth + 1);
        cursor = mc.cdr(cursor);
        printed += 1;

        if cursor.is_nil() || full(out) {
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
            write_value(out, mc, cursor, depth + 1);
            break;
        }
        out.push(' ');
    }
    out.push(')');
}

fn write_object<'h>(out: &mut String, mc: &Mutation<'h>, v: Value<'h>, depth: usize) {
    let header = mc.header(v);
    match header.object_type {
        ObjectType::Symbol => {
            // The colon is punctuation, not part of the name, so the printer
            // puts it back on rather than the symbol carrying it around.
            if mc.is_keyword(v) {
                out.push(':');
            }
            out.push_str(&mc.symbol_name(v));
        }
        t if t.is_string() => {
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
            //
            // The name is worth printing when there is one: a backtrace of
            // `#<function>` six times tells you nothing, and a compiled
            // closure knows what it was called.
            let body = mc.slot(v, crate::closure::BODY);
            if body.is_object() && mc.header(body).object_type == ObjectType::Function {
                let _ = write!(out, "#<function {}>", name_of_function(mc, body));
            } else {
                out.push_str("#<function>");
            }
        }
        ObjectType::Function => {
            let _ = write!(out, "#<compiled {}>", name_of_function(mc, v));
        }
        // A macro is a closure that is called at a different time, and like
        // any closure there is no text that reads back as this one.
        ObjectType::Macro => out.push_str("#<macro>"),
        ObjectType::Condition => {
            let _ = write!(out, "#<{}>", crate::condition::describe(mc, v));
        }
        ObjectType::HashTable => out.push_str("#<hash-table>"),
        ObjectType::Instance => out.push_str("#<structure>"),
        // Rust's shortest round-trip digits; the Lisp printer lays them out
        // as SBCL does, and is the one a world uses.
        ObjectType::Ratio => {
            write_value(out, mc, mc.slot(v, crate::ratio::NUMERATOR), depth + 1);
            out.push('/');
            write_value(out, mc, mc.slot(v, crate::ratio::DENOMINATOR), depth + 1);
        }
        ObjectType::DoubleFloat => {
            let _ = write!(out, "{:?}", crate::float::get(mc, v));
        }
        ObjectType::Vector => {
            out.push_str("#(");
            for i in 0..header.slots {
                if i > 0 {
                    out.push(' ');
                }
                // The same length cap as a list, for the same reason: a
                // vector can be as long as the heap allows.
                if i >= MAX_LENGTH {
                    out.push_str("...");
                    break;
                }
                write_value(out, mc, mc.slot(v, i), depth + 1);
                if full(out) {
                    break;
                }
            }
            out.push(')');
        }
        other => {
            let _ = write!(out, "#<{other:?} {} slots>", header.slots);
        }
    }
}

/// A compiled function's name, or a placeholder.
fn name_of_function<'h>(mc: &Mutation<'h>, f: Value<'h>) -> String {
    let name = mc.slot(f, crate::code::function::NAME);
    if mc.is_symbol(name) {
        mc.symbol_name(name)
    } else {
        String::from("(anonymous)")
    }
}
