//! Bignums: integers too large for a fixnum (#16, step L of #39).
//!
//! Decided in #16 (Carsten): **the representation and the fixnum boundary in
//! Rust, the algorithms in Lisp.** This module is the Rust half — making a
//! bignum, reading and writing its limbs, and the three operations that stay
//! here: normalising, comparing and hashing. Adding, multiplying, dividing
//! and printing are loops over limbs in `lisp/library/integers.lisp`, where
//! the evaluator slices them like any other Lisp (rule 4), so a long
//! multiplication cannot freeze the machine.
//!
//! **The representation** (decision 2): a `Bignum` heap object whose raw,
//! untraced slots are a sign word and then the magnitude in **limbs** of 30
//! bits, least significant first — *sign and magnitude*, base 2^30, as
//! CPython stores its `int` on 64-bit machines. Thirty bits because the
//! limb loops run in Lisp on fixnums: the product of two limbs is at most 60
//! bits and, with a carry, still a fixnum (62 bits of magnitude), so the
//! arithmetic on limbs never needs bignums itself.
//!
//! **Normal form.** Every bignum Lisp sees is normalised: no leading zero
//! limb, and a value outside fixnum range — anything that fits is a fixnum.
//! So a fixnum and a bignum are never equal, and `eq` on two fixnums is still
//! `=`. Lisp builds a result in a fresh bignum and hands it to
//! [`normalize`] before anyone else can see it.
//!
//! **Rule 9** (decision 4): [`normalize`], [`compare`] and [`hash`] take time
//! that grows with the number's length. Each is one pass at memory speed over
//! an object that already exists, and allocating that object was one step
//! that touched every limb just as much — `alloc` fills every slot. None
//! costs more than an allocation the machine already accepts as one step.

use crate::heap::{Mutation, ObjectType};
use crate::value::Value;
use std::cmp::Ordering;

/// Bits in a limb.
pub const LIMB_BITS: u32 = 30;
/// The largest limb, and the mask that takes one off a wider number.
pub const LIMB_MASK: u64 = (1 << LIMB_BITS) - 1;
/// Raw slot 0: 1 if negative, 0 otherwise. The limbs follow.
const SIGN: usize = 0;

/// Is `v` a bignum?
pub fn is<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::Bignum
}

/// A bignum of `limbs` zero limbs, for Lisp to fill and then normalise.
pub fn make<'h>(mc: &mut Mutation<'h>, negative: bool, limbs: usize) -> Value<'h> {
    let b = mc.alloc(ObjectType::Bignum, limbs + 1);
    mc.set_raw_slot(b, SIGN, negative as u64);
    for i in 1..=limbs {
        mc.set_raw_slot(b, i, 0);
    }
    b
}

/// How many limbs it has.
pub fn length<'h>(mc: &Mutation<'h>, b: Value<'h>) -> usize {
    mc.header(b).slots - 1
}

/// Is it negative?
pub fn negative<'h>(mc: &Mutation<'h>, b: Value<'h>) -> bool {
    mc.raw_slot(b, SIGN) != 0
}

/// Limb `i`, counting from the least significant.
pub fn limb<'h>(mc: &Mutation<'h>, b: Value<'h>, i: usize) -> u64 {
    mc.raw_slot(b, i + 1)
}

/// Store limb `i`.
pub fn set_limb<'h>(mc: &mut Mutation<'h>, b: Value<'h>, i: usize, limb: u64) {
    mc.set_raw_slot(b, i + 1, limb);
}

/// A fixnum's value as an unnormalised bignum — how Lisp gets at the limbs
/// of a fixnum, including the most negative one, whose magnitude no fixnum
/// can hold.
pub fn from_i64<'h>(mc: &mut Mutation<'h>, n: i64) -> Value<'h> {
    let mut magnitude = n.unsigned_abs();
    let b = make(mc, n < 0, 3);
    for i in 0..3 {
        set_limb(mc, b, i, magnitude & LIMB_MASK);
        magnitude >>= LIMB_BITS;
    }
    b
}

/// The sign and the magnitude's limbs of an integer, without leading zeros
/// (zero has none). `None` for anything that is not an integer.
fn sign_and_limbs<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<(bool, Vec<u64>)> {
    let (negative, mut limbs) = if let Some(n) = v.as_fixnum() {
        let mut magnitude = n.unsigned_abs();
        let mut limbs = Vec::new();
        while magnitude != 0 {
            limbs.push(magnitude & LIMB_MASK);
            magnitude >>= LIMB_BITS;
        }
        (n < 0, limbs)
    } else if is(mc, v) {
        let limbs = (0..length(mc, v)).map(|i| limb(mc, v, i)).collect();
        (negative(mc, v), limbs)
    } else {
        return None;
    };
    while limbs.last() == Some(&0) {
        limbs.pop();
    }
    Some((negative && !limbs.is_empty(), limbs))
}

/// The normal form of `b`: a fixnum if the value fits, otherwise a bignum
/// with no leading zero limb — `b` itself if it already has none.
pub fn normalize<'h>(mc: &mut Mutation<'h>, b: Value<'h>) -> Value<'h> {
    let (negative, limbs) = sign_and_limbs(mc, b).expect("normalising a bignum");
    // Three limbs are 90 bits, so any value that could be a fixnum fits in an
    // i128 on the way.
    if limbs.len() <= 3 {
        let magnitude = limbs
            .iter()
            .rev()
            .fold(0i128, |acc, &l| (acc << LIMB_BITS) | l as i128);
        let value = if negative { -magnitude } else { magnitude };
        if let Ok(n) = i64::try_from(value) {
            if let Some(fixnum) = Value::fixnum(n) {
                return fixnum;
            }
        }
    }
    if limbs.len() == length(mc, b) {
        return b;
    }
    let trimmed = make(mc, negative, limbs.len());
    for (i, &l) in limbs.iter().enumerate() {
        set_limb(mc, trimmed, i, l);
    }
    trimmed
}

/// Compare two integers, fixnum or bignum. `None` if either is not one.
pub fn compare<'h>(mc: &Mutation<'h>, a: Value<'h>, b: Value<'h>) -> Option<Ordering> {
    let (a_negative, a_limbs) = sign_and_limbs(mc, a)?;
    let (b_negative, b_limbs) = sign_and_limbs(mc, b)?;
    let magnitudes = a_limbs
        .len()
        .cmp(&b_limbs.len())
        .then_with(|| a_limbs.iter().rev().cmp(b_limbs.iter().rev()));
    Some(match (a_negative, b_negative) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        (false, false) => magnitudes,
        (true, true) => magnitudes.reverse(),
    })
}

/// A hash of a bignum's value, by `mix` (the hash table's own step), so two
/// equal bignums — different objects — land in the same bucket.
pub fn hash<'h>(mc: &Mutation<'h>, b: Value<'h>, start: u64, mix: fn(u64, u64) -> u64) -> u64 {
    let (negative, limbs) = sign_and_limbs(mc, b).expect("hashing a bignum");
    limbs
        .iter()
        .fold(mix(start, negative as u64), |h, &l| mix(h, l))
}
