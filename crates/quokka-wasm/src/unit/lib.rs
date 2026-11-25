//! Tests for the `quokka-wasm` crate root: the browser host, driven as a page
//! drives it. They run natively — nothing here needs a browser, which is the
//! point of keeping the DOM on the other side of `take_output`.

use super::*;
use std::sync::OnceLock;

/// A world, built once. Building is the cold build's job, which is why the
/// bootstrap crate is a dev-dependency here and not a dependency.
fn world() -> Quokka {
    static IMAGE: OnceLock<Vec<u8>> = OnceLock::new();
    let image = IMAGE.get_or_init(|| {
        let mut heap = Heap::new();
        quokka_bootstrap::boot::build_image(&mut heap).unwrap_or_else(|e| panic!("{e}"));
        image::save(&mut heap)
    });
    Quokka::new(image).expect("the image loads")
}

/// Submit a line and pump `run` as a page's timer would, `steps` at a time.
/// Returns the output and how many calls it took.
fn pump(q: &mut Quokka, line: &str, steps: u32) -> (String, usize) {
    q.submit(line).expect("idle");
    let mut out = String::new();
    let mut calls = 0;
    while q.run(steps) {
        out.push_str(&q.take_output());
        calls += 1;
        assert!(calls < 10_000_000, "never finished");
    }
    out.push_str(&q.take_output());
    (out, calls)
}

#[test]
fn banner_names_the_phase() {
    assert!(world().banner().contains(PHASE));
}

#[test]
fn what_is_not_an_image_is_refused_with_a_reason() {
    assert_eq!(
        Quokka::new(b"not an image").unwrap_err(),
        "12 bytes is too short to be an image"
    );
}

/// The browser runs a world: tier 1, the Lisp reader and printer, packages.
/// None of this existed in the interpreter the page used to boot.
#[test]
fn a_line_is_read_evaluated_and_printed_by_the_world() {
    let mut q = world();
    assert_eq!(pump(&mut q, "(+ 1 (* 2 3))", 10_000).0, "7\n");
    assert_eq!(
        pump(&mut q, "(format nil \"~D item~:P\" 2) (expt 2 100)", 10_000).0,
        "\"2 items\"\n1267650600228229401496703205376\n"
    );
    assert_eq!(pump(&mut q, "(car 5)", 10_000).0.chars().next(), Some(';'));
    // Console output arrives through a host request, in order with values.
    assert_eq!(pump(&mut q, "(progn (princ \"hi\") 1)", 10_000).0, "hi1\n");
    assert!(q.heap_words() > 0);
}

/// **Slicing is invisible** (rule 4): the same line gives the same output
/// whether the page runs it three steps at a time or all at once — and at
/// three steps a call, the page gets control back thousands of times.
#[test]
fn the_answer_does_not_depend_on_how_it_is_sliced() {
    let line = "(defun fib (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (fib 15)";
    let (whole, few) = pump(&mut world(), line, u32::MAX);
    let (sliced, many) = pump(&mut world(), line, 3);
    assert_eq!(whole, "FIB\n610\n");
    assert_eq!(sliced, whole);
    assert!(few <= 2 && many > 1_000, "{few} calls and {many}");
}

/// A line that never ends does not freeze anything: `run` keeps returning,
/// and the page can stop it. The world is still there afterwards.
#[test]
fn an_endless_line_can_be_interrupted() {
    let mut q = world();
    q.submit("(defvar *n* 0) (tagbody top (go top))").unwrap();
    for _ in 0..50 {
        assert!(q.run(1_000));
    }
    assert!(q.submit("1").is_err(), "busy");
    q.interrupt();
    assert!(!q.busy());
    assert!(q.take_output().ends_with("; interrupted\n"));
    assert_eq!(pump(&mut q, "*n*", 10_000).0, "0\n");
}

/// Running out of heap is reported and recovered from; the page does not
/// have to be reloaded, as it did before #84.
#[test]
fn out_of_heap_is_survived() {
    let mut q = world();
    let (out, _) = pump(&mut q, "(make-array 100000000000) (+ 1 2)", 100_000);
    assert!(out.starts_with("; out of heap"), "{out}");
    assert!(out.ends_with("3\n"), "{out}");
}

/// A world saved in the browser loads again, and keeps what was defined.
#[test]
fn a_world_can_be_saved_and_loaded_again() {
    let mut q = world();
    pump(&mut q, "(defun twice (x) (* 2 x))", 10_000);
    let mut again = Quokka::new(&q.save()).expect("loads");
    assert_eq!(pump(&mut again, "(twice 21)", 10_000).0, "42\n");
}
