//! Tests for [`crate::ratio`].

use super::*;
use crate::heap::Heap;

fn ratio<'h>(mc: &mut Mutation<'h>, n: i64, d: i64) -> Value<'h> {
    make(mc, Value::fixnum(n).unwrap(), Value::fixnum(d).unwrap())
}

/// A ratio equals a double exactly when its denominator is a power of two
/// and its numerator fits the mantissa — which is what an `equalp` table
/// needs to know, since `equalp` compares numbers with `=`.
#[test]
fn only_a_power_of_two_denominator_is_an_exact_double() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let cases = [
            (1, 2, Some(0.5)),
            (-3, 4, Some(-0.75)),
            (5, 1024, Some(5.0 / 1024.0)),
            (1, 3, None),
            (1, 10, None),
            (7, 6, None),
        ];
        for (n, d, wanted) in cases {
            let r = ratio(mc, n, d);
            assert!(is(mc, r));
            assert_eq!(exact_double(mc, r), wanted, "{n}/{d}");
        }
        // A numerator past 2^53 has more bits than a double keeps.
        let wide = ratio(mc, (1 << 53) + 1, 2);
        assert_eq!(exact_double(mc, wide), None);
        assert!(!is(mc, Value::fixnum(3).unwrap()));
    });
}
