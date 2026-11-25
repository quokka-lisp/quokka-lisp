//! Tests for [`crate::array`]: what a header's slots are trusted to hold.

use super::*;
use crate::heap::Heap;

fn fixnum<'h>(n: i64) -> Value<'h> {
    Value::fixnum(n).expect("small")
}

/// A vector of three elements behind a header with the given fill pointer.
fn with_fill<'h>(mc: &mut Mutation<'h>, fill: Value<'h>) -> Value<'h> {
    let data = mc.alloc(ObjectType::Vector, 3);
    for i in 0..3 {
        mc.set_slot(data, i, fixnum(i as i64 + 10));
    }
    let dimensions = mc.cons(fixnum(3), Value::NIL);
    make_header(
        mc,
        &[data, fixnum(0), fill, dimensions, Value::NIL, Value::NIL],
    )
}

/// The length is the fill pointer, and never outside the array: a negative
/// one, cast unchecked, was a length of 2^64 - 1 (#100).
#[test]
fn a_fill_pointer_is_the_length_and_stays_inside_the_array() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        for (fill, wanted) in [
            (Value::NIL, 3),
            (fixnum(2), 2),
            (fixnum(0), 0),
            (fixnum(-1), 0),
            (fixnum(99), 3),
        ] {
            let array = with_fill(mc, fill);
            assert!(is_header(mc, array) && is_vector(mc, array));
            assert_eq!(length(mc, array).unwrap(), wanted);
        }
        assert!(length(mc, fixnum(5)).is_err(), "a number has no length");
    });
}

#[test]
fn elements_are_read_and_written_through_a_header() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let array = with_fill(mc, Value::NIL);
        assert_eq!(aref(mc, &[array, fixnum(1)]).unwrap(), fixnum(11));
        set_aref(mc, &[array, fixnum(1), fixnum(-5)]).unwrap();
        assert_eq!(aref(mc, &[array, fixnum(1)]).unwrap(), fixnum(-5));
        assert!(aref(mc, &[array, fixnum(3)]).is_err(), "past the end");
        assert!(aref(mc, &[array, fixnum(-1)]).is_err(), "before the start");
        assert!(aref(mc, &[array]).is_err(), "too few subscripts");
        assert!(
            aref(mc, &[array, fixnum(0), fixnum(0)]).is_err(),
            "too many"
        );
    });
}

/// A bit vector's size is checked before anything is allocated, in both
/// directions (#91, #99).
#[test]
fn a_bit_vector_has_the_size_asked_for_or_is_refused() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let bits = make_bits(mc, fixnum(70)).expect("seventy bits");
        assert_eq!(length(mc, bits).unwrap(), 70);
        assert_eq!(aref(mc, &[bits, fixnum(69)]).unwrap(), fixnum(0));
        assert!(make_bits(mc, fixnum(-1)).is_err());
        assert!(make_bits(mc, fixnum(i64::MAX >> 4)).is_err());
        assert!(make_bits(mc, Value::NIL).is_err());
        let empty = make_bits(mc, fixnum(0)).expect("no bits");
        assert_eq!(length(mc, empty).unwrap(), 0);
    });
}
