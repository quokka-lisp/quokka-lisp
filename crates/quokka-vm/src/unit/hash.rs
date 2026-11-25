//! Tests for [`crate::hash`]: what the Lisp differential files cannot see.
//! They catch a wrong answer; they do not catch a table that gives the right
//! answer slowly, which is how float keys sat in one bucket unnoticed (#98).

use super::*;
use crate::heap::Heap;

fn fixnum<'h>(n: i64) -> Value<'h> {
    Value::fixnum(n).expect("small")
}

/// Keys whose low bits are all alike must still spread: a double such as
/// 2.0 has forty zero bits at the bottom, a character eight constant ones.
#[test]
fn keys_with_constant_low_bits_spread_over_the_buckets() {
    let buckets = 1024usize;
    let spread = |words: &mut dyn Iterator<Item = u64>| {
        let mut used = std::collections::HashSet::new();
        for word in words {
            used.insert(index_of(mix(OFFSET, word), buckets));
        }
        used.len()
    };
    let floats = spread(&mut (0..4096u64).map(|i| (i as f64).to_bits()));
    let shifted = spread(&mut (0..4096u64).map(|i| i << 40));
    let characters = spread(&mut (0..4096u64).map(|i| (i << 8) | 0b1101));
    // 4,096 keys into 1,024 buckets fill nearly all of them when they
    // spread; before the finaliser each of these used a handful.
    for (what, used) in [
        ("floats", floats),
        ("shifted", shifted),
        ("characters", characters),
    ] {
        assert!(
            used > 900,
            "{what} reached only {used} of {buckets} buckets"
        );
    }
}

#[test]
fn a_table_stores_finds_replaces_and_removes() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let test = mc.intern("EQL");
        let table = make(mc, test).expect("a table");
        let missing = mc.intern("MISSING");
        for i in 0..2000 {
            put(mc, table, fixnum(i), fixnum(i * 2)).expect("stores");
        }
        assert_eq!(count(mc, table).unwrap(), fixnum(2000));
        assert_eq!(get(mc, table, fixnum(1234), missing).unwrap(), fixnum(2468));
        put(mc, table, fixnum(1234), fixnum(-1)).expect("replaces");
        assert_eq!(
            count(mc, table).unwrap(),
            fixnum(2000),
            "a replacement adds nothing"
        );
        assert_eq!(get(mc, table, fixnum(1234), missing).unwrap(), fixnum(-1));
        assert!(remove(mc, table, fixnum(1234)).unwrap());
        assert!(!remove(mc, table, fixnum(1234)).unwrap(), "it is gone");
        assert_eq!(get(mc, table, fixnum(1234), missing).unwrap(), missing);
        assert_eq!(count(mc, table).unwrap(), fixnum(1999));
    });
}

/// A key hashed by its address is found again after the collector has moved
/// it: the table is marked stale when it is copied, and rehashed on its
/// next use (#64).
#[test]
fn an_address_keyed_table_survives_a_collection() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let test = mc.intern("EQ");
        let table = make(mc, test).expect("a table");
        let key = mc.cons(fixnum(1), fixnum(2));
        put(mc, table, key, fixnum(42)).expect("stores");
        // Padding, so that the survivors really move.
        for i in 0..1000 {
            mc.cons(fixnum(i), Value::NIL);
        }
        let both = mc.cons(table, key);
        mc.set_root(both);
    });
    heap.collect();
    heap.mutate(|mc| {
        let both = mc.root();
        let (table, key) = (mc.car(both), mc.cdr(both));
        assert_eq!(get(mc, table, key, Value::NIL).unwrap(), fixnum(42));
    });
}

/// A table refuses a test it does not know, with an error and not a panic.
#[test]
fn an_unknown_test_is_refused() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let test = mc.intern("STRING=");
        assert!(make(mc, test).is_err());
        assert!(
            get(mc, fixnum(5), fixnum(1), Value::NIL).is_err(),
            "5 is not a table"
        );
    });
}
