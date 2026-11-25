//! Tests for [`crate::float`]: the edges a differential file cannot print.

use super::*;
use crate::heap::Heap;

/// A double that is not a number never becomes a Lisp object: the result
/// that would be infinite or NaN is an error, as in SBCL with traps on.
#[test]
fn infinities_and_nan_are_never_made() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        assert!(make(mc, f64::INFINITY).is_err());
        assert!(make(mc, f64::NEG_INFINITY).is_err());
        assert!(make(mc, f64::NAN).is_err());
        let x = make(mc, 1.5).expect("a double");
        assert!(is(mc, x));
        assert_eq!(get(mc, x), 1.5);
        // Negative zero is a number of its own, and keeps its sign.
        let zero = make(mc, -0.0).expect("a double");
        assert!(get(mc, zero).is_sign_negative());
    });
}

/// `integer` is for a double already rounded to a whole number: a fixnum
/// while it fits one, and `None` past that, where Lisp builds the bignum.
#[test]
fn a_whole_double_is_a_fixnum_while_it_fits_one() {
    assert_eq!(integer(3.0), Value::fixnum(3));
    assert_eq!(integer(-0.0), Value::fixnum(0));
    assert_eq!(integer(-4e18), Value::fixnum(-4_000_000_000_000_000_000));
    assert_eq!(integer(1e300), None, "past the fixnum range");
    assert_eq!(integer(-5e18), None);
    // And the rounding that comes before it, by mode: 2 is the ceiling,
    // anything unlisted rounds to even.
    assert_eq!(round(2.5, 2), 3.0);
    assert_eq!(round(2.5, 99), 2.0);
    assert_eq!(round(3.5, 99), 4.0);
}

/// `decode` and `scale` are `integer-decode-float` and `scale-float`: taking
/// a double apart and putting it together gives the double back.
#[test]
fn decoding_and_scaling_are_inverses() {
    for x in [1.0, 0.1, -2.5, 1e-300, 123456.789e20, f64::MIN_POSITIVE] {
        let (mantissa, exponent, sign) = decode(x);
        assert_eq!(scale(mantissa as f64, exponent) * sign as f64, x, "{x}");
    }
}

/// The shortest digits that read back as the same double, which is what
/// makes printing and reading a round trip.
#[test]
fn the_shortest_digits_read_back_as_the_same_double() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        for x in [0.1, 1.0 / 3.0, 2.5e-10, 6.02214076e23, 5e-324] {
            let text = format!("{x:e}");
            let read = parse(mc, &text).expect("parses");
            assert_eq!(get(mc, read), x, "{text}");
            assert!(!digits(x).is_empty());
        }
        assert!(parse(mc, "not a number").is_err());
        assert!(parse(mc, "1e999").is_err(), "it would be infinite");
    });
}
