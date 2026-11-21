//! Tests for [`crate::lambda_list`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

use super::*;
use crate::heap::Heap;
use crate::printer::print;
use crate::reader::Reader;

/// Read `(params . body)` and show what the rewrite made of it.
fn rewritten(source: &str) -> String {
    let chars = Reader::chars_of(source);
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).unwrap().unwrap();
        let params = mc.car(form);
        let body = mc.cdr(form);
        let (params, body) = rewrite(mc, params, body, Kind::Ordinary).expect("rewrites");
        let whole = mc.cons(params, body);
        print(mc, whole)
    })
}

/// The common case must cost nothing: a list of required parameters comes
/// back untouched, body and all.
#[test]
fn a_simple_lambda_list_is_left_alone() {
    assert_eq!(rewritten("((a b) (+ a b))"), "((A B) (+ A B))");
    assert_eq!(rewritten("(() 1)"), "(NIL 1)");
    assert_eq!(rewritten("((a &rest r) r)"), "((A &REST R) R)");
}

/// A default becomes a `let*` binding, and the parameter list left behind
/// is one [`bind`] can match without evaluating anything.
#[test]
fn a_default_becomes_a_let_star() {
    assert_eq!(
        rewritten("((a &optional (b 10)) b)"),
        "((A &OPTIONAL B) (LET* ((B (IF (EQ B (QUOTE UNSUPPLIED MARKER)) 10 B))) B))"
    );
}

/// `&aux` is not a parameter at all; it leaves the lambda list entirely.
#[test]
fn aux_leaves_the_lambda_list() {
    assert_eq!(
        rewritten("((a &aux (d (* a a))) d)"),
        "((A) (LET* ((D (* A A))) D))"
    );
}

/// `&key b` means the keyword `:b`, and the pair is made explicit so the
/// binder never has to turn a name into a keyword at call time.
#[test]
fn key_parameters_carry_their_keyword() {
    assert_eq!(
        rewritten("((&key b) b)"),
        "((&KEY (:B B)) (LET* ((B (IF (EQ B (QUOTE UNSUPPLIED MARKER)) NIL B))) B))"
    );
}

fn rewritten_macro(source: &str) -> String {
    let chars = Reader::chars_of(source);
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).unwrap().unwrap();
        let params = mc.car(form);
        let body = mc.cdr(form);
        let (params, body) = rewrite(mc, params, body, Kind::Macro).expect("rewrites");
        let whole = mc.cons(params, body);
        print(mc, whole)
    })
}

/// Only macros destructure. A function receives values, and a value has
/// no shape to take apart.
#[test]
fn only_macros_destructure() {
    let chars = Reader::chars_of("(((a b)) a)");
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).unwrap().unwrap();
        let params = mc.car(form);
        let body = mc.cdr(form);
        assert!(rewrite(mc, params, body, Kind::Ordinary).is_err());
        assert!(rewrite(mc, params, body, Kind::Macro).is_ok());
    });
}

/// A nested pattern leaves a placeholder in the lambda list and becomes
/// `car`/`cdr` walking in the body. The gensym names are not pinned —
/// only the shape is, which is what matters.
#[test]
fn a_pattern_becomes_car_and_cdr() {
    let out = rewritten_macro("(((var name) &body body) var)");
    assert!(out.contains("LET*"), "{out}");
    assert!(out.contains("(CAR "), "{out}");
    assert!(out.contains("(CDR "), "{out}");
    assert!(out.contains("(VAR (IF (CONSP "), "{out}");
    assert!(out.contains("(NAME (IF (CONSP "), "{out}");
    // The arity check: a position that is missing signals rather than
    // binding nil, and anything left over signals too.
    assert!(out.contains("%DESTRUCTURING-TOO-SHORT"), "{out}");
    assert!(out.contains("%DESTRUCTURING-TOO-LONG"), "{out}");
    // &body is still a plain parameter, not part of the pattern.
    assert!(
        out.contains("&REST BODY") || out.contains("&BODY BODY"),
        "{out}"
    );
}

/// `(defmacro m (a . b) ...)` — the tail is `&rest` by another spelling,
/// and the list must be rebuilt as a proper one or the binder, which
/// walks conses, never reaches it.
#[test]
fn a_dotted_macro_lambda_list_becomes_rest() {
    assert_eq!(rewritten_macro("((a . b) a)"), "((A &REST B) A)");
}

/// Nothing to rewrite means nothing is rewritten, macro or not.
#[test]
fn a_macro_lambda_list_with_no_patterns_is_left_alone() {
    assert_eq!(rewritten_macro("((a b) a)"), "((A B) A)");
}

#[test]
fn a_malformed_lambda_list_is_an_error() {
    let chars = Reader::chars_of("((a . b) 1)");
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).unwrap().unwrap();
        let params = mc.car(form);
        let body = mc.cdr(form);
        assert!(rewrite(mc, params, body, Kind::Ordinary).is_err());
    });
}
