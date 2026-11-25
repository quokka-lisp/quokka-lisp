//! Arrays that are not simple, and bit vectors (#67, step T).
//!
//! # Simple arrays and headers
//!
//! Common Lisp has one array model (CLHS 15): an array has any number of
//! dimensions, and may have a fill pointer, be adjustable, or be *displaced*
//! onto another array's elements. Implementations build it in two layers, and
//! so does Quokka. A **simple** array — one dimension, none of the rest — is
//! the object it always was: a general [`ObjectType::Vector`], one of the
//! three string types, or a [`ObjectType::BitVector`]. Every other array is an
//! [`ObjectType::ArrayHeader`] in front of the array holding its elements:
//!
//! | slot | |
//! | --- | --- |
//! | [`DATA`] | the array whose elements, in row-major order, are this one's |
//! | [`OFFSET`] | where in `DATA`'s elements this array's first one is |
//! | [`FILL`] | the fill pointer, or nil |
//! | [`DIMENSIONS`] | a list of fixnums |
//! | [`DISPLACED`] | true if `DATA` is an array this one was displaced to |
//! | [`ADJUSTABLE`] | true if `adjust-array` may change this array in place |
//!
//! `adjust-array` keeps the header and swaps its `DATA`, so every reference to
//! the array sees the new size: an object cannot grow where it stands, which is
//! why the header exists at all. `DATA` is usually a fresh simple array, and
//! for a displaced array the array displaced to, which may itself be a header;
//! [`storage`] follows the chain to the simple array at its end.
//!
//! Rust knows the layout and does what is bounded by it: finding an element
//! (`aref`, setting one, `length`) and answering the type predicates. Making,
//! adjusting, filling and printing arrays are Lisp (`lisp/library/arrays.lisp`),
//! as the algorithms on bignums are (#16).
//!
//! # Bit vectors
//!
//! A simple bit vector is raw words, 64 bits to a word, the count in raw slot 0
//! as a packed string's is. An element is a fixnum, 0 or 1.
//!
//! # Arrays of element type nil
//!
//! `(make-array 5 :element-type nil)` is an array that can hold nothing,
//! since no object is of type `nil`: it has a length, and every attempt to
//! read or write an element is an error (#77). SBCL has the same object,
//! `(simple-array nil (*))`. Its count is raw slot 0, and nothing follows.
//!
//! CLHS 15.1.2.2 says `(vector nil)` is a string, since `nil` is a subtype of
//! `character`; SBCL, the oracle, says it is not (`stringp` is false), and so
//! does Quokka (rule 6). ansi-test marks the tests that assume otherwise with
//! the note `:nil-vectors-are-strings`, which the runner disables.

use crate::error::EvalError;
use crate::heap::{Mutation, ObjectType};
use crate::value::Value;

/// The array holding the elements.
pub const DATA: usize = 0;
/// The first element's index in `DATA`.
pub const OFFSET: usize = 1;
/// The fill pointer, or nil.
pub const FILL: usize = 2;
/// The dimensions, a list of fixnums.
pub const DIMENSIONS: usize = 3;
/// True if displaced.
pub const DISPLACED: usize = 4;
/// True if adjustable.
pub const ADJUSTABLE: usize = 5;
/// How many slots a header has.
pub const SLOTS: usize = 6;

/// A displacement chain longer than this is refused rather than followed: a
/// program can make a cycle with `adjust-array`, and a primitive must stay
/// short (rule 9).
const MAX_CHAIN: usize = 64;

/// What a simple array holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A general vector: any object.
    Vector,
    /// One of the string types: characters.
    String,
    /// A bit vector: 0 and 1.
    Bits,
    /// An array of element type nil: no element can be read or stored.
    Nothing,
}

/// Where an array's elements live: the simple array at the end of its chain,
/// and the index there of its first element.
pub struct Storage<'h> {
    /// The simple array.
    pub simple: Value<'h>,
    /// The index in it of the array's first element.
    pub start: usize,
    /// What it holds.
    pub kind: Kind,
    /// How many elements it really has.
    pub len: usize,
}

/// Where element INDEX of an array is in its simple array — checked against
/// that array's real length, not only against the header's dimensions. A
/// header's slots are Lisp's to write, and an array displaced onto one that
/// `adjust-array` has since made smaller points past its end; either would
/// otherwise reach a slot that is not there, and a Rust panic would end the
/// whole machine where a Lisp error should be signalled.
fn element<'h>(s: &Storage<'h>, index: usize) -> Result<usize, EvalError> {
    s.start
        .checked_add(index)
        .filter(|&at| at < s.len)
        .ok_or(EvalError::IndexOutOfBounds {
            index: index as i64,
            length: s.len.saturating_sub(s.start),
        })
}

/// A simple array's kind and length.
fn simple<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<(Kind, usize)> {
    if !v.is_object() {
        return None;
    }
    let header = mc.header(v);
    match header.object_type {
        ObjectType::Vector => Some((Kind::Vector, header.slots)),
        ObjectType::BitVector => Some((Kind::Bits, mc.raw_slot(v, 0) as usize)),
        ObjectType::NilVector => Some((Kind::Nothing, mc.raw_slot(v, 0) as usize)),
        t if t.is_string() => Some((Kind::String, mc.string_length(v))),
        _ => None,
    }
}

/// Is this an array header?
pub fn is_header<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::ArrayHeader
}

/// An array's dimensions, or `None` if it is not an array.
pub fn dimensions<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<Vec<usize>> {
    if let Some((_, length)) = simple(mc, v) {
        return Some(vec![length]);
    }
    if !is_header(mc, v) {
        return None;
    }
    // Lisp can store any list here, a circular one included (#92).
    let (items, _) = mc.list_elements(mc.slot(v, DIMENSIONS))?;
    items
        .iter()
        .map(|d| d.as_fixnum().and_then(|d| usize::try_from(d).ok()))
        .collect()
}

/// Follow an array to the simple array holding its elements.
pub fn storage<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<Storage<'h>> {
    let (mut v, mut start) = (v, 0);
    for _ in 0..MAX_CHAIN {
        if let Some((kind, len)) = simple(mc, v) {
            return Some(Storage {
                simple: v,
                start,
                kind,
                len,
            });
        }
        if !is_header(mc, v) {
            return None;
        }
        let offset = mc.slot(v, OFFSET).as_fixnum().filter(|&o| o >= 0)?;
        start = start.checked_add(offset as usize)?;
        v = mc.slot(v, DATA);
    }
    None
}

fn not_an_array(name: &'static str) -> EvalError {
    EvalError::WrongType {
        name,
        wanted: "an array",
    }
}

/// The row-major index of SUBSCRIPTS in an array of DIMS, each checked.
fn row_major(
    dims: &[usize],
    subscripts: &[Value<'_>],
    name: &'static str,
) -> Result<usize, EvalError> {
    if subscripts.len() != dims.len() {
        return Err(EvalError::WrongType {
            name,
            wanted: "one subscript for each of the array's dimensions",
        });
    }
    let mut index: usize = 0;
    for (subscript, &dim) in subscripts.iter().zip(dims) {
        let i = subscript.as_fixnum().ok_or(EvalError::WrongType {
            name,
            wanted: "a fixnum subscript",
        })?;
        if i < 0 || i as usize >= dim {
            return Err(EvalError::IndexOutOfBounds {
                index: i,
                length: dim,
            });
        }
        index = index
            .checked_mul(dim)
            .and_then(|x| x.checked_add(i as usize))
            .ok_or(EvalError::WrongType {
                name,
                wanted: "an array whose size fits in a fixnum",
            })?;
    }
    Ok(index)
}

/// The element at AT, an index already checked by [`element`].
fn read<'h>(mc: &Mutation<'h>, s: &Storage<'h>, at: usize) -> Result<Value<'h>, EvalError> {
    Ok(match s.kind {
        Kind::Vector => mc.slot(s.simple, at),
        Kind::String => Value::character(mc.string_char(s.simple, at)),
        Kind::Bits => Value::fixnum(((mc.raw_slot(s.simple, 1 + at / 64) >> (at % 64)) & 1) as i64)
            .expect("a bit"),
        Kind::Nothing => return Err(holds_nothing("AREF")),
    })
}

fn holds_nothing(name: &'static str) -> EvalError {
    EvalError::WrongType {
        name,
        wanted: "an array that can hold an element — one of element type nil holds none",
    }
}

/// `(aref array subscript...)`: any array, any rank. A fill pointer does not
/// limit it (CLHS `aref`).
pub fn aref<'h>(mc: &Mutation<'h>, args: &[Value<'h>]) -> Result<Value<'h>, EvalError> {
    let dims = dimensions(mc, args[0]).ok_or(not_an_array("AREF"))?;
    let index = row_major(&dims, &args[1..], "AREF")?;
    let s = storage(mc, args[0]).ok_or(not_an_array("AREF"))?;
    read(mc, &s, element(&s, index)?)
}

/// `(%set-aref array subscript... value)`.
pub fn set_aref<'h>(mc: &mut Mutation<'h>, args: &[Value<'h>]) -> Result<Value<'h>, EvalError> {
    let (value, subscripts) = (args[args.len() - 1], &args[1..args.len() - 1]);
    let dims = dimensions(mc, args[0]).ok_or(not_an_array("SET-AREF"))?;
    let index = row_major(&dims, subscripts, "SET-AREF")?;
    let s = storage(mc, args[0]).ok_or(not_an_array("SET-AREF"))?;
    let at = element(&s, index)?;
    match s.kind {
        Kind::Vector => mc.set_slot(s.simple, at, value),
        Kind::String => {
            // A literal is readable everywhere and writable nowhere. CLHS
            // 3.7.1 leaves modifying one undefined, which is permission to
            // share literals and no help when a program modifies one anyway
            // (#26). Only `String` is writable, and it is the one layout
            // where any character fits anywhere (#44).
            if mc.header(s.simple).object_type != ObjectType::String {
                return Err(EvalError::WrongType {
                    name: "SET-AREF",
                    wanted:
                        "a mutable string — this one is a literal, so copy it with COPY-SEQ first",
                });
            }
            let ch = value.as_character().ok_or(EvalError::WrongType {
                name: "SET-AREF",
                wanted: "a character, to store in a string",
            })?;
            mc.set_raw_slot(s.simple, at, ch as u64);
        }
        Kind::Bits => {
            let bit = match value.as_fixnum() {
                Some(b @ (0 | 1)) => b as u64,
                _ => {
                    return Err(EvalError::WrongType {
                        name: "SET-AREF",
                        wanted: "a bit, 0 or 1, to store in a bit vector",
                    })
                }
            };
            let word = mc.raw_slot(s.simple, 1 + at / 64);
            mc.set_raw_slot(
                s.simple,
                1 + at / 64,
                (word & !(1 << (at % 64))) | (bit << (at % 64)),
            );
        }
        Kind::Nothing => return Err(holds_nothing("SET-AREF")),
    }
    Ok(value)
}

/// A vector's length: its fill pointer if it has one.
pub fn length<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Result<usize, EvalError> {
    if let Some((_, length)) = simple(mc, v) {
        return Ok(length);
    }
    let dims = dimensions(mc, v).ok_or(not_an_array("LENGTH"))?;
    if dims.len() != 1 {
        return Err(EvalError::WrongType {
            name: "LENGTH",
            wanted: "a vector, not an array of several dimensions",
        });
    }
    // Never more than the array holds, and never negative: cast unchecked,
    // a fill pointer of -1 was a length of 2^64 - 1 (#100).
    Ok(match mc.slot(v, FILL).as_fixnum() {
        Some(fill) => usize::try_from(fill).unwrap_or(0).min(dims[0]),
        None => dims[0],
    })
}

/// A one-dimensional array, of any kind.
pub fn is_vector<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    simple(mc, v).is_some() || (is_header(mc, v) && dimensions(mc, v).is_some_and(|d| d.len() == 1))
}

/// A one-dimensional array of characters.
pub fn is_string<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    is_vector(mc, v) && storage(mc, v).is_some_and(|s| s.kind == Kind::String)
}

/// Element I of vector V, as `aref` reads it, or nil if there is none.
pub fn vector_element<'h>(mc: &Mutation<'h>, v: Value<'h>, i: usize) -> Value<'h> {
    storage(mc, v)
        .and_then(|s| element(&s, i).ok().and_then(|at| read(mc, &s, at).ok()))
        .unwrap_or(Value::NIL)
}

/// A string's or bit vector's active elements as text — the characters, or
/// `0`s and `1`s — and whether they are bits: what `equal` compares, so what
/// an `equal` hash table hashes. `None` for anything else.
pub fn equal_text<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<(bool, String)> {
    if !is_vector(mc, v) {
        return None;
    }
    let s = storage(mc, v)?;
    if matches!(s.kind, Kind::Vector | Kind::Nothing) {
        return None;
    }
    let n = length(mc, v).ok()?;
    let mut text = String::new();
    for i in 0..n {
        let at = element(&s, i).ok()?;
        text.push(match s.kind {
            Kind::String => mc.string_char(s.simple, at),
            _ if read(mc, &s, at).ok()? == Value::fixnum(1).expect("small") => '1',
            _ => '0',
        });
    }
    Some((s.kind == Kind::Bits, text))
}

/// What `%array-kind` answers: 0 not an array, 1 a simple vector, 2 a simple
/// string, 3 a simple bit vector, 4 a header, 5 a simple array of element
/// type nil. Lisp builds every array type predicate on it.
pub fn kind_code<'h>(mc: &Mutation<'h>, v: Value<'h>) -> i64 {
    match simple(mc, v) {
        Some((Kind::Vector, _)) => 1,
        Some((Kind::String, _)) => 2,
        Some((Kind::Bits, _)) => 3,
        Some((Kind::Nothing, _)) => 5,
        None if is_header(mc, v) => 4,
        None => 0,
    }
}

/// `(%make-array-header data offset fill dimensions displaced adjustable)`.
pub fn make_header<'h>(mc: &mut Mutation<'h>, args: &[Value<'h>]) -> Value<'h> {
    let h = mc.alloc(ObjectType::ArrayHeader, SLOTS);
    for (slot, value) in args.iter().enumerate().take(SLOTS) {
        mc.set_slot(h, slot, *value);
    }
    h
}

/// A header's slot, by number, for `%array-header-slot` and its setter.
pub fn header_slot<'h>(
    mc: &Mutation<'h>,
    h: Value<'h>,
    slot: Value<'h>,
) -> Result<usize, EvalError> {
    match slot.as_fixnum() {
        Some(i) if is_header(mc, h) && (0..SLOTS as i64).contains(&i) => Ok(i as usize),
        _ => Err(EvalError::WrongType {
            name: "%ARRAY-HEADER-SLOT",
            wanted: "an array header and a slot number",
        }),
    }
}

/// `(%make-bit-vector n)`: N zero bits.
pub fn make_bits<'h>(mc: &mut Mutation<'h>, n: Value<'h>) -> Result<Value<'h>, EvalError> {
    let Some(bits) = n.as_fixnum() else {
        return Err(EvalError::WrongType {
            name: "MAKE-ARRAY",
            wanted: "a non-negative fixnum size",
        });
    };
    // A negative size rounds up to no words at all and so passed the size
    // check, leaving a bit vector that claimed 2^64 - 1 bits in none: the
    // heap verifier found it, once it looked at counts (#99).
    if bits < 0 {
        return Err(EvalError::WrongType {
            name: "MAKE-ARRAY",
            wanted: "a non-negative fixnum size",
        });
    }
    // Sized in words, so the check is on the words (#91).
    let words = crate::primitives::size(mc, "MAKE-ARRAY", bits.saturating_add(63) / 64, 1)? - 1;
    let n = bits as usize;
    let v = mc.alloc(ObjectType::BitVector, 1 + words);
    mc.set_raw_slot(v, 0, n as u64);
    for w in 0..words {
        mc.set_raw_slot(v, 1 + w, 0);
    }
    Ok(v)
}

/// `(%make-nil-vector n)`: a simple array of N elements of type nil.
pub fn make_nil_vector<'h>(mc: &mut Mutation<'h>, n: Value<'h>) -> Result<Value<'h>, EvalError> {
    let n = match n.as_fixnum() {
        Some(n) if n >= 0 => n as u64,
        _ => {
            return Err(EvalError::WrongType {
                name: "MAKE-ARRAY",
                wanted: "a non-negative fixnum size",
            })
        }
    };
    let v = mc.alloc(ObjectType::NilVector, 1);
    mc.set_raw_slot(v, 0, n);
    Ok(v)
}

#[cfg(test)]
#[path = "unit/array.rs"]
mod tests;
