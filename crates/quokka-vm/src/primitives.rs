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
    // Handled by the evaluator, not by `call` below: applying a function is
    // the machine's job, not a primitive's. Present here so it has a name and
    // a function cell like everything else.
    Primitive {
        name: "FUNCALL",
        arity: Arity::AtLeast(1),
    },
    // Allocates one cons per argument, so its cost grows with the *call*
    // rather than with the data. Rule 9 is satisfied because a written call has
    // a bounded number of arguments — but `apply` would break that, and when
    // `apply` arrives this needs looking at again.
    Primitive {
        name: "LIST",
        arity: Arity::AtLeast(0),
    },
    // Needed by backquote: `(,@xs . rest) expands into an APPEND.
    //
    // Copies all but its last argument, so its cost grows with the *data*
    // rather than the call — the first primitive here for which rule 9 is a
    // live question. It is bounded by what is already in the heap, which is
    // bounded by the heap, so a single call cannot run away; but a long append
    // is a long step, and when slices start mattering this is the one to chunk.
    Primitive {
        name: "APPEND",
        arity: Arity::AtLeast(0),
    },
    // Macros need names nothing else can have, or an expansion that binds a
    // temporary captures whatever the caller happened to call it.
    Primitive {
        name: "GENSYM",
        arity: Arity::AtLeast(0),
    },
    // The one primitive the prelude could not do without: it is how `defun`,
    // written in Lisp, puts a function where the evaluator will find it.
    //
    // Common Lisp spells this `(setf (symbol-function 'name) f)`. Quokka has
    // no `setf` yet, and `setf` is itself a macro that needs a writer to
    // expand into — so the writer comes first and the sugar comes later. The
    // `%` marks it as Quokka's own rather than Common Lisp's, which is what
    // rule 6 asks for in the absence of packages.
    Primitive {
        name: "%SET-SYMBOL-FUNCTION",
        arity: Arity::Exactly(2),
    },
    // Reading the cell back, which is standard and which `defun` does not
    // need — but a prelude you cannot inspect is a prelude you debug blind.
    Primitive {
        name: "SYMBOL-FUNCTION",
        arity: Arity::Exactly(1),
    },
    Primitive {
        name: "KEYWORDP",
        arity: Arity::Exactly(1),
    },
    // Here to make a claim checkable. A keyword's name has no colon in it —
    // `(symbol-name :foo)` is "FOO" — and that is exactly what interning
    // `:foo` under the name ":FOO" would have got wrong, silently, until
    // somebody exposed this function. So it is exposed.
    Primitive {
        name: "SYMBOL-NAME",
        arity: Arity::Exactly(1),
    },
];

/// Where `FUNCALL` sits in the table, which `crate::eval` intercepts.
///
/// A constant rather than a search, with a test that it is still right —
/// table order is part of the image format, and a silent drift here would make
/// every `funcall` call some other primitive.
pub const FUNCALL: u32 = 13;

/// What primitive `index` is called, for printing.
pub fn name_of(index: u32) -> Option<&'static str> {
    TABLE.get(index as usize).map(|p| p.name)
}

/// Put every primitive in its symbol's function cell, so `(+ 1 2)` can find it.
///
/// Called once when a heap is prepared. Interning the names is also what puts
/// them in the obarray, so they survive collection like any other symbol.
pub fn install(mc: &mut Mutation<'_>) {
    // Special operators are marked on their own symbols; see `crate::eval`.
    for (name, code) in crate::eval::special::ALL {
        let symbol = mc.intern(name);
        let value = Value::fixnum(*code).expect("small");
        mc.set_slot(symbol, crate::heap::symbol::SPECIAL, value);
    }
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
        "APPEND" => {
            if args.is_empty() {
                return Ok(Value::NIL);
            }
            // The last argument is shared, not copied — which is what makes
            // append cheap enough to use in a macro expansion.
            let mut out = args[args.len() - 1];
            for list in args[..args.len() - 1].iter().rev() {
                let mut items = Vec::new();
                let mut cursor = *list;
                while cursor.is_cons() {
                    items.push(mc.car(cursor));
                    cursor = mc.cdr(cursor);
                }
                if !cursor.is_nil() {
                    return Err(EvalError::WrongType {
                        name: "APPEND",
                        wanted: "proper lists",
                    });
                }
                for item in items.into_iter().rev() {
                    out = mc.cons(item, out);
                }
            }
            Ok(out)
        }
        "GENSYM" => Ok(mc.gensym()),
        "%SET-SYMBOL-FUNCTION" => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name: "%SET-SYMBOL-FUNCTION",
                    wanted: "a symbol",
                });
            }
            mc.set_slot(args[0], symbol::FUNCTION, args[1]);
            Ok(args[1])
        }
        "KEYWORDP" => Ok(if mc.is_keyword(args[0]) {
            Value::T
        } else {
            Value::NIL
        }),
        "SYMBOL-NAME" => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name: "SYMBOL-NAME",
                    wanted: "a symbol",
                });
            }
            let name = mc.symbol_name(args[0]);
            Ok(mc.alloc_string(&name))
        }
        "SYMBOL-FUNCTION" => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name: "SYMBOL-FUNCTION",
                    wanted: "a symbol",
                });
            }
            let f = mc.slot(args[0], symbol::FUNCTION);
            if f == Value::UNBOUND {
                return Err(EvalError::UndefinedFunction(mc.symbol_name(args[0])));
            }
            Ok(f)
        }
        "LIST" => {
            let mut out = Value::NIL;
            for v in args.iter().rev() {
                out = mc.cons(*v, out);
            }
            Ok(out)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_funcall_index_is_still_right() {
        assert_eq!(TABLE[FUNCALL as usize].name, "FUNCALL");
    }

    /// Table order is part of the image format. This does not stop a reorder,
    /// but it makes one a deliberate act with a failing test attached.
    #[test]
    fn the_table_order_is_pinned() {
        let names: Vec<&str> = TABLE.iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            [
                "+",
                "-",
                "*",
                "=",
                "<",
                ">",
                "CONS",
                "CAR",
                "CDR",
                "EQ",
                "NULL",
                "CONSP",
                "SYMBOLP",
                "FUNCALL",
                "LIST",
                "APPEND",
                "GENSYM",
                "%SET-SYMBOL-FUNCTION",
                "SYMBOL-FUNCTION",
                "KEYWORDP",
                "SYMBOL-NAME"
            ]
        );
    }
}
