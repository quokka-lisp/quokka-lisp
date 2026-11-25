//! Tests for [`crate::bignum`]: the representation, which the arithmetic in
//! `lisp/library/integers.lisp` stands on and never checks.

use super::*;
use crate::heap::Heap;

/// A value that fits a fixnum is one, however it was made: there is one
/// representation of each integer, which is what lets `eq` compare small
/// ones and `hash` treat equal big ones alike.
#[test]
fn normalising_gives_each_integer_one_representation() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        // A bignum of three limbs holding 7, with two leading zeros.
        let padded = make(mc, false, 3);
        set_limb(mc, padded, 0, 7);
        set_limb(mc, padded, 1, 0);
        set_limb(mc, padded, 2, 0);
        assert_eq!(normalize(mc, padded), Value::fixnum(7).unwrap());

        // 2^90 does not fit, and keeps its four limbs.
        let big = make(mc, false, 4);
        for i in 0..3 {
            set_limb(mc, big, i, 0);
        }
        set_limb(mc, big, 3, 1);
        let normal = normalize(mc, big);
        assert!(is(mc, normal));
        assert_eq!(length(mc, normal), 4);

        // The same with a zero limb on top is trimmed back to it.
        let padded = make(mc, false, 5);
        for i in 0..5 {
            set_limb(mc, padded, i, u64::from(i == 3));
        }
        let trimmed = normalize(mc, padded);
        assert_eq!(length(mc, trimmed), 4);
        assert_eq!(compare(mc, trimmed, normal), Some(Ordering::Equal));
    });
}

#[test]
fn comparison_orders_by_sign_then_size_then_limbs() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let big = |mc: &mut Mutation<'_>, negative: bool, top: u64| {
            let b = make(mc, negative, 4);
            for i in 0..3 {
                set_limb(mc, b, i, 5);
            }
            set_limb(mc, b, 3, top);
            b.bits()
        };
        let value = |bits: u64| Value::from_bits(bits);
        let (plus_one, plus_two) = (value(big(mc, false, 1)), value(big(mc, false, 2)));
        let (minus_one, minus_two) = (value(big(mc, true, 1)), value(big(mc, true, 2)));
        let small = Value::fixnum(1_000_000).unwrap();
        assert_eq!(compare(mc, plus_one, plus_two), Some(Ordering::Less));
        assert_eq!(compare(mc, minus_one, minus_two), Some(Ordering::Greater));
        assert_eq!(compare(mc, minus_two, plus_one), Some(Ordering::Less));
        assert_eq!(compare(mc, small, plus_one), Some(Ordering::Less));
        assert_eq!(compare(mc, small, minus_one), Some(Ordering::Greater));
        assert_eq!(compare(mc, small, small), Some(Ordering::Equal));
        assert_eq!(
            compare(mc, small, Value::NIL),
            None,
            "nil is not an integer"
        );
    });
}

/// Two equal bignums are different objects and must hash alike, or an `eql`
/// table would hold the same number twice.
#[test]
fn equal_bignums_hash_alike() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let mix = |h: u64, w: u64| (h ^ w).wrapping_mul(0x0000_0100_0000_01b3);
        let make_one = |mc: &mut Mutation<'_>, top: u64| {
            let b = make(mc, false, 4);
            for i in 0..3 {
                set_limb(mc, b, i, 9);
            }
            set_limb(mc, b, 3, top);
            b.bits()
        };
        let (a, b, c) = (make_one(mc, 3), make_one(mc, 3), make_one(mc, 4));
        let hash_of = |bits: u64| hash(mc, Value::from_bits(bits), 17, mix);
        assert_ne!(a, b, "two objects");
        assert_eq!(hash_of(a), hash_of(b));
        assert_ne!(hash_of(a), hash_of(c));
    });
}

/// `from_i64` covers the integers a fixnum cannot hold and an `i64` can.
#[test]
fn an_i64_past_the_fixnum_range_becomes_a_bignum() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let most = from_i64(mc, i64::MAX);
        let least = from_i64(mc, i64::MIN);
        assert!(is(mc, most) && is(mc, least));
        assert!(!negative(mc, most) && negative(mc, least));
        assert_eq!(compare(mc, least, most), Some(Ordering::Less));
    });
}
