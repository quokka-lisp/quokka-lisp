//! Tests for [`crate::value`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

use super::*;

/// A seeded xorshift, so the generative tests below are deterministic.
///
/// Deliberately not `proptest`: the differential-testing strategy wants the
/// same test to behave identically on native, under wasmtime and in a
/// browser, and a dependency-free generator with a fixed seed guarantees
/// that. It also keeps the dependency list empty (rule 7).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn fixnum(&mut self) -> i64 {
        let raw = self.next() as i64 >> 2;
        raw.clamp(MIN_FIXNUM, MAX_FIXNUM)
    }
}

fn interesting_fixnums() -> Vec<i64> {
    vec![
        0,
        1,
        -1,
        2,
        -2,
        42,
        -42,
        i32::MAX as i64,
        i32::MIN as i64,
        MAX_FIXNUM,
        MIN_FIXNUM,
        MAX_FIXNUM - 1,
        MIN_FIXNUM + 1,
    ]
}

// ---- representation --------------------------------------------------

#[test]
fn fixnums_round_trip() {
    for n in interesting_fixnums() {
        let v = Value::fixnum(n).expect("in range");
        assert!(v.is_fixnum(), "{n} should be a fixnum");
        assert_eq!(v.as_fixnum(), Some(n));
        assert_eq!(v.kind(), Kind::Fixnum);
    }

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..10_000 {
        let n = rng.fixnum();
        assert_eq!(Value::fixnum(n).and_then(Value::as_fixnum), Some(n));
    }
}

#[test]
fn integers_outside_the_range_are_not_fixnums() {
    assert_eq!(Value::fixnum(MAX_FIXNUM + 1), None);
    assert_eq!(Value::fixnum(MIN_FIXNUM - 1), None);
    assert_eq!(Value::fixnum(i64::MAX), None);
    assert_eq!(Value::fixnum(i64::MIN), None);
}

#[test]
fn every_value_has_exactly_one_kind() {
    let samples = vec![
        Value::fixnum(0).unwrap(),
        Value::fixnum(-1).unwrap(),
        Value::cons(0).unwrap(),
        Value::cons(999).unwrap(),
        Value::object(17).unwrap(),
        Value::forwarding(17).unwrap(),
        Value::NIL,
        Value::T,
        Value::UNBOUND,
        Value::character('q'),
    ];
    for v in samples {
        let flags = [
            v.is_fixnum(),
            v.is_cons(),
            v.is_object(),
            v.is_immediate(),
            v.is_forwarding(),
        ];
        assert_eq!(
            flags.iter().filter(|b| **b).count(),
            1,
            "{v:?} answered yes to more than one kind"
        );
    }
}

#[test]
fn heap_references_round_trip() {
    for index in [0usize, 1, 2, 8, 1_000, 1 << 20] {
        for make in [
            Value::cons as fn(usize) -> Option<Value<'static>>,
            Value::object,
            Value::forwarding,
        ] {
            let v = make(index).expect("in range");
            assert_eq!(v.word_index(), Some(index));
            assert!(v.is_heap_ref());
            assert!(!v.is_fixnum());
        }
    }
}

/// The index field is 61 bits wide. Only a 64-bit host can even name a
/// `usize` that overflows it, which is why this is target-gated rather than
/// written as an ordinary boundary check.
#[test]
#[cfg(target_pointer_width = "64")]
fn the_index_field_is_sixty_one_bits() {
    let last = MAX_WORD_INDEX as usize;
    assert_eq!(Value::cons(last).and_then(Value::word_index), Some(last));
    assert_eq!(Value::cons(last + 1), None);
    assert_eq!(Value::object(last + 1), None);
    assert_eq!(Value::forwarding(last + 1), None);
}

#[test]
fn the_three_heap_tags_are_distinct() {
    let (c, o, f) = (
        Value::cons(42).unwrap(),
        Value::object(42).unwrap(),
        Value::forwarding(42).unwrap(),
    );
    assert_ne!(c, o);
    assert_ne!(o, f);
    assert_ne!(c, f);
    assert_eq!(
        (c.word_index(), o.word_index(), f.word_index()),
        (Some(42), Some(42), Some(42))
    );
}

#[test]
fn nil_and_t_are_static_objects_and_unbound_is_not() {
    for v in [Value::NIL, Value::T] {
        assert!(v.is_object(), "{v:?} should be a heap object");
        assert!(!v.is_fixnum());
        assert!(!v.is_immediate());
    }
    assert!(Value::UNBOUND.is_immediate());
    assert_ne!(Value::NIL, Value::T);
    assert_ne!(Value::T, Value::UNBOUND);
    assert!(Value::NIL.is_nil());
    assert!(!Value::T.is_nil());
    assert!(Value::T.is_true());
    assert!(!Value::NIL.is_true());
}

/// Nothing reachable from Lisp may answer yes to `is_header`, or a linear
/// scan of the heap could mistake a cons cell's car for an object header.
#[test]
fn no_ordinary_value_looks_like_a_header() {
    let mut candidates = vec![
        Value::NIL,
        Value::T,
        Value::UNBOUND,
        Value::cons(0).unwrap(),
        Value::object(0).unwrap(),
        Value::forwarding(0).unwrap(),
    ];
    for n in interesting_fixnums() {
        candidates.push(Value::fixnum(n).unwrap());
    }
    for c in ['a', '\0', '\u{10FFFF}'] {
        candidates.push(Value::character(c));
    }
    let mut rng = Rng(4242);
    for _ in 0..10_000 {
        candidates.push(Value::fixnum(rng.fixnum()).unwrap());
    }
    for v in candidates {
        assert!(!v.is_header(), "{v:?} would be read as an object header");
    }
}

#[test]
fn characters_round_trip() {
    for c in ['a', 'Z', '0', ' ', '\n', 'é', 'あ', '\u{10FFFF}', '\u{0}'] {
        let v = Value::character(c);
        assert!(v.is_character());
        assert!(v.is_immediate());
        assert_eq!(v.as_character(), Some(c));
    }
    assert_eq!(Value::NIL.as_character(), None);
    assert_eq!(Value::fixnum(65).unwrap().as_character(), None);
}

/// The image format depends on these exact bit patterns: an image written by
/// one build is read by another, possibly on a different target. Pin them so
/// that changing the encoding is a deliberate act with a format version
/// bump, not an accident.
#[test]
fn the_encoding_is_stable() {
    // nil and t are object references into the static area now, not
    // immediates. This is an image format change, which is exactly the
    // kind of thing this test exists to make deliberate.
    //
    // `t` sits one header plus one symbol's slots past `nil`, so this
    // number moves whenever a symbol grows a slot. It went from 6 to 7
    // when symbols gained `HOME` for the keyword namespace — another
    // format change, caught here rather than in a mysteriously wrong
    // image six months from now.
    assert_eq!(Value::NIL.bits(), 0b011);
    assert_eq!(Value::T.bits(), (7 << 3) | 0b011);
    assert_eq!(Value::UNBOUND.bits(), (2 << 8) | 0b101);
    assert_eq!(Value::character('A').bits(), (65 << 8) | (1 << 3) | 0b101);
    assert_eq!(Value::fixnum(1).unwrap().bits(), 2);
    assert_eq!(Value::fixnum(-1).unwrap().bits(), u64::MAX - 1);
    assert_eq!(Value::cons(1).unwrap().bits(), 0b1_001);
    assert_eq!(Value::object(1).unwrap().bits(), 0b1_011);
    assert_eq!(Value::forwarding(1).unwrap().bits(), 0b1_111);
}

#[test]
fn bits_round_trip_through_storage() {
    let mut rng = Rng(12345);
    for _ in 0..1000 {
        let v = Value::fixnum(rng.fixnum()).unwrap();
        assert_eq!(Value::from_bits(v.bits()), v);
    }
}

// ---- arithmetic ------------------------------------------------------

#[test]
fn addition_and_subtraction_need_no_untagging() {
    let mut rng = Rng(0xDEAD_BEEF);
    for _ in 0..10_000 {
        // Quarter range, so sums and differences stay in range.
        let a = rng.fixnum() / 4;
        let b = rng.fixnum() / 4;
        let (va, vb) = (Value::fixnum(a).unwrap(), Value::fixnum(b).unwrap());
        assert_eq!(va.checked_add(vb).and_then(Value::as_fixnum), Some(a + b));
        assert_eq!(va.checked_sub(vb).and_then(Value::as_fixnum), Some(a - b));
    }
}

#[test]
fn multiplication_is_correct() {
    for a in [0i64, 1, -1, 2, -3, 7, 1_000, -1_000, i32::MAX as i64] {
        for b in [0i64, 1, -1, 2, -3, 7, 1_000, -1_000] {
            let (va, vb) = (Value::fixnum(a).unwrap(), Value::fixnum(b).unwrap());
            assert_eq!(
                va.checked_mul(vb).and_then(Value::as_fixnum),
                Some(a * b),
                "{a} * {b}"
            );
        }
    }
}

/// The property the fixnum tag was chosen for: `checked_add` on tagged words
/// overflows exactly when the true sum leaves fixnum range. Not one value
/// early, not one late.
#[test]
fn overflow_detection_is_exact() {
    let max = Value::fixnum(MAX_FIXNUM).unwrap();
    let min = Value::fixnum(MIN_FIXNUM).unwrap();
    let one = Value::fixnum(1).unwrap();
    let zero = Value::fixnum(0).unwrap();

    // The last additions that still fit.
    assert_eq!(
        max.checked_add(zero).and_then(Value::as_fixnum),
        Some(MAX_FIXNUM)
    );
    assert_eq!(
        Value::fixnum(MAX_FIXNUM - 1)
            .unwrap()
            .checked_add(one)
            .and_then(Value::as_fixnum),
        Some(MAX_FIXNUM)
    );
    assert_eq!(
        min.checked_add(zero).and_then(Value::as_fixnum),
        Some(MIN_FIXNUM)
    );

    // The first ones that do not.
    assert_eq!(max.checked_add(one), None);
    assert_eq!(min.checked_sub(one), None);
    assert_eq!(max.checked_add(max), None);
    assert_eq!(min.checked_add(min), None);

    // Negation is in range everywhere except the one asymmetric end.
    assert_eq!(
        max.checked_neg().and_then(Value::as_fixnum),
        Some(-MAX_FIXNUM)
    );
    assert_eq!(min.checked_neg(), None);
}

#[test]
fn arithmetic_rejects_non_fixnums() {
    let one = Value::fixnum(1).unwrap();
    for other in [
        Value::NIL,
        Value::T,
        Value::character('x'),
        Value::cons(4).unwrap(),
        Value::object(4).unwrap(),
    ] {
        assert_eq!(one.checked_add(other), None, "{other:?}");
        assert_eq!(other.checked_add(one), None, "{other:?}");
        assert_eq!(one.checked_sub(other), None, "{other:?}");
        assert_eq!(one.checked_mul(other), None, "{other:?}");
        assert_eq!(other.checked_neg(), None, "{other:?}");
    }
}

/// Doubling preserves order, so comparing raw words compares the numbers.
/// The compiler is allowed to use this; the test records that it is true.
#[test]
fn raw_word_order_matches_numeric_order() {
    let mut rng = Rng(777);
    for _ in 0..10_000 {
        let (a, b) = (rng.fixnum(), rng.fixnum());
        let (va, vb) = (Value::fixnum(a).unwrap(), Value::fixnum(b).unwrap());
        assert_eq!(
            (va.bits() as i64) < (vb.bits() as i64),
            a < b,
            "comparing {a} and {b}"
        );
    }
}

#[test]
fn debug_output_is_readable() {
    assert_eq!(format!("{:?}", Value::fixnum(-7).unwrap()), "-7");
    assert_eq!(format!("{:?}", Value::NIL), "nil");
    assert_eq!(format!("{:?}", Value::T), "t");
    assert_eq!(format!("{:?}", Value::character('q')), "#\\q");
    assert_eq!(format!("{:?}", Value::cons(3).unwrap()), "#<cons @3>");
}
