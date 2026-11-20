//! The primitive table: the only door from Lisp into Rust.
//!
//! Rule 1 says Rust provides the VM and everything a user might want to change
//! lives in Lisp. These are the operations that cannot: arithmetic, allocation,
//! the accessors that define what a cons cell *is*. Everything else — `list`,
//! `mapcar`, `append` — is Lisp built on these, and belongs there.
//!
//! The table's size is part of the comprehensibility budget. Each entry is a
//! reason the system is less changeable by the people using it, so each one
//! should have to justify itself.
//!
//! # Rule 9: primitives are bounded
//!
//! A step budget only bounds time if one step is short, and nothing can
//! interrupt a primitive once control is inside it. Every one of these is O(1)
//! on its arguments. The first that is not — sorting, bignum multiplication,
//! hashing a long string — needs a plan for being resumable before it is added.
//!
//! # Indices are part of the image format
//!
//! A primitive is an immediate holding an index into this table. An image
//! carries those, so the VM that loads an image must have the same table in the
//! same order. Adding an entry at the end is a format addition; reordering is a
//! breaking change.

use crate::eval::EvalError;
use crate::heap::{symbol, Mutation};
use crate::value::Value;

/// One primitive: a name, how many arguments, and what it does.
struct Primitive {
    name: &'static str,
    arity: Arity,
}

enum Arity {
    Exactly(usize),
    AtLeast(usize),
}

/// The table. **Order is part of the image format** — append, never reorder.
const TABLE: &[Primitive] = &[
    Primitive {
        name: "+",
        arity: Arity::AtLeast(0),
    },
    Primitive {
        name: "-",
        arity: Arity::AtLeast(1),
    },
    Primitive {
        name: "*",
        arity: Arity::AtLeast(0),
    },
    Primitive {
        name: "=",
        arity: Arity::AtLeast(1),
    },
    Primitive {
        name: "<",
        arity: Arity::AtLeast(1),
    },
    Primitive {
        name: ">",
        arity: Arity::AtLeast(1),
    },
    Primitive {
        name: "CONS",
        arity: Arity::Exactly(2),
    },
    Primitive {
        name: "CAR",
        arity: Arity::Exactly(1),
    },
    Primitive {
        name: "CDR",
        arity: Arity::Exactly(1),
    },
    Primitive {
        name: "EQ",
        arity: Arity::Exactly(2),
    },
    Primitive {
        name: "NULL",
        arity: Arity::Exactly(1),
    },
    Primitive {
        name: "CONSP",
        arity: Arity::Exactly(1),
    },
    Primitive {
        name: "SYMBOLP",
        arity: Arity::Exactly(1),
    },
];

/// Put every primitive in its symbol's function cell, so `(+ 1 2)` can find it.
///
/// Called once when a heap is prepared. Interning the names is also what puts
/// them in the obarray, so they survive collection like any other symbol.
pub fn install(mc: &mut Mutation<'_>) {
    for (index, p) in TABLE.iter().enumerate() {
        let name = mc.intern(p.name);
        let value = Value::primitive(index as u32);
        mc.set_slot(name, symbol::FUNCTION, value);
    }
}

/// Call primitive `index` with `arguments`, a Lisp list.
pub fn call<'h>(
    mc: &mut Mutation<'h>,
    index: u32,
    arguments: Value<'h>,
) -> Result<Value<'h>, EvalError> {
    let Some(p) = TABLE.get(index as usize) else {
        return Err(EvalError::NotAFunction(String::from("#<bad primitive>")));
    };

    let args = to_vec(mc, arguments);
    let got = args.len();
    let ok = match p.arity {
        Arity::Exactly(n) => got == n,
        Arity::AtLeast(n) => got >= n,
    };
    if !ok {
        let wanted = match p.arity {
            Arity::Exactly(n) | Arity::AtLeast(n) => n,
        };
        return Err(EvalError::WrongArgumentCount {
            name: p.name,
            wanted,
            got,
        });
    }

    match p.name {
        "+" => fold(p.name, &args, 0, |a, b| a.checked_add(b)),
        "*" => fold(p.name, &args, 1, |a, b| a.checked_mul(b)),
        "-" => {
            let first = fixnum(p.name, args[0])?;
            if got == 1 {
                return make(first.checked_neg());
            }
            let mut acc = first;
            for v in &args[1..] {
                acc = acc
                    .checked_sub(fixnum(p.name, *v)?)
                    .ok_or(EvalError::IntegerOverflow)?;
            }
            make(Some(acc))
        }
        "=" => compare(p.name, &args, |a, b| a == b),
        "<" => compare(p.name, &args, |a, b| a < b),
        ">" => compare(p.name, &args, |a, b| a > b),
        "CONS" => Ok(mc.cons(args[0], args[1])),
        "CAR" => list_part(mc, p.name, args[0], true),
        "CDR" => list_part(mc, p.name, args[0], false),
        "EQ" => Ok(truth(args[0] == args[1])),
        "NULL" => Ok(truth(args[0].is_nil())),
        "CONSP" => Ok(truth(args[0].is_cons())),
        "SYMBOLP" => Ok(truth(mc.is_symbol(args[0]))),
        _ => Err(EvalError::NotAFunction(String::from(p.name))),
    }
}

/// `nil` is false and the only false thing; `t` is the canonical true.
fn truth<'h>(b: bool) -> Value<'h> {
    if b {
        Value::T
    } else {
        Value::NIL
    }
}

fn fixnum(name: &'static str, v: Value<'_>) -> Result<i64, EvalError> {
    v.as_fixnum().ok_or(EvalError::WrongType {
        name,
        wanted: "a number",
    })
}

fn make<'h>(n: Option<i64>) -> Result<Value<'h>, EvalError> {
    n.and_then(Value::fixnum).ok_or(EvalError::IntegerOverflow)
}

fn fold<'h>(
    name: &'static str,
    args: &[Value<'h>],
    start: i64,
    op: impl Fn(i64, i64) -> Option<i64>,
) -> Result<Value<'h>, EvalError> {
    let mut acc = start;
    for v in args {
        acc = op(acc, fixnum(name, *v)?).ok_or(EvalError::IntegerOverflow)?;
    }
    make(Some(acc))
}

/// Comparisons chain, as Common Lisp's do: `(< 1 2 3)` is true.
fn compare<'h>(
    name: &'static str,
    args: &[Value<'h>],
    op: impl Fn(i64, i64) -> bool,
) -> Result<Value<'h>, EvalError> {
    for pair in args.windows(2) {
        if !op(fixnum(name, pair[0])?, fixnum(name, pair[1])?) {
            return Ok(Value::NIL);
        }
    }
    Ok(Value::T)
}

/// `(car nil)` and `(cdr nil)` are `nil`, which Common Lisp allows and a great
/// deal of list code relies on.
fn list_part<'h>(
    mc: &Mutation<'h>,
    name: &'static str,
    v: Value<'h>,
    want_car: bool,
) -> Result<Value<'h>, EvalError> {
    if v.is_nil() {
        return Ok(Value::NIL);
    }
    if !v.is_cons() {
        return Err(EvalError::WrongType {
            name,
            wanted: "a list",
        });
    }
    Ok(if want_car { mc.car(v) } else { mc.cdr(v) })
}

fn to_vec<'h>(mc: &Mutation<'h>, list: Value<'h>) -> Vec<Value<'h>> {
    let mut out = Vec::new();
    let mut cursor = list;
    while cursor.is_cons() {
        out.push(mc.car(cursor));
        cursor = mc.cdr(cursor);
    }
    out
}
