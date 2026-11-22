//! Tests for [`quokka_vm::lambda_list`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

use quokka_bootstrap::reader::Reader;
use quokka_vm::heap::Heap;
use quokka_vm::lambda_list::*;
use quokka_vm::printer::print;

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

/// `names` and `bind` must agree about which names a lambda list binds and in
/// what order — the compiler builds its scope from the first and the machine
/// fills it with the second, so a disagreement is one variable reading
/// another's value.
///
/// Each case carries arguments that supply everything, so `bind` takes every
/// branch that pushes a name. The first draft used one argument list for all
/// of them and `(a b c)` refused it, which is the test being stricter than
/// its author.
#[test]
fn the_names_match_what_bind_binds() {
    let cases = [
        ("(a b c)", "(1 2 3)"),
        ("(a &optional b)", "(1 2)"),
        ("(a &optional b)", "(1)"),
        ("(a &rest r)", "(1 2 3)"),
        ("(&rest r)", "()"),
        ("(a &key (:k k))", "(1 :k 2)"),
        ("(a &key (:k k))", "(1)"),
        (
            "(a &optional b &rest r &key (:k k) (:j j))",
            "(1 2 :k 3 :j 4)",
        ),
        ("(&key (:k k) &allow-other-keys)", "(:k 1 :other 2)"),
        ("()", "()"),
    ];
    let mut heap = Heap::new();
    for (params_source, args_source) in cases {
        let chars = Reader::chars_of(&format!("{params_source} {args_source}"));
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let params = reader.read(mc).expect("reads").expect("a lambda list");
            let args = reader.read(mc).expect("reads").expect("arguments");
            let expected = names(mc, params).expect("names");
            let (bound, _) = bind(mc, params, args).expect("binds");
            let mut a = String::new();
            for v in &expected {
                a.push_str(&quokka_vm::printer::print(mc, *v));
                a.push(' ');
            }
            let mut b = String::new();
            for v in &bound {
                b.push_str(&quokka_vm::printer::print(mc, *v));
                b.push(' ');
            }
            assert_eq!(a, b, "for {params_source} with {args_source}");
        });
    }
}
