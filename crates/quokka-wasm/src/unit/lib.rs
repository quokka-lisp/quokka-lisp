//! Tests for the `quokka-wasm` crate root.

use super::*;

#[test]
fn banner_names_the_phase() {
    // Against the constant rather than a literal: the phase moves, and a
    // test that has to be edited every time is a test that gets edited
    // wrongly. What is worth pinning is that the banner says the phase at
    // all, not which one it is today.
    let q = Quokka::new();
    assert!(q.banner().contains(PHASE));
}

#[test]
fn eval_reads_and_prints() {
    let mut q = Quokka::new();
    assert_eq!(q.eval("(+ 1 (* 2 3))"), "7\n");
    assert_eq!(q.eval("'(1 2 . 3)"), "(1 2 . 3)\n");
    assert!(q.eval("(oops").starts_with("; input ended"));
    assert!(q.heap_words() > 0);
}
