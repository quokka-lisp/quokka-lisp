//! Tests for [`crate::reader`].
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

/// Read one form and print it back. The cheapest way to test a reader: if
/// text survives a round trip the structure was almost certainly right.
fn round_trip(source: &str) -> Result<String, ReadError> {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let chars = Reader::chars_of(source);
        let mut reader = Reader::new(&chars);
        match reader.read(mc)? {
            Some(v) => Ok(print(mc, v)),
            None => Ok(String::from("<nothing>")),
        }
    })
}

/// Backquote is a reader macro: the evaluator never sees one, so the test
/// is simply what code the reader produced.
#[test]
fn backquote_expands_to_cons_and_append() {
    reads_as("`a", "(QUOTE A)");
    reads_as("`,a", "A");
    reads_as("`(a)", "(CONS (QUOTE A) (QUOTE NIL))");
    reads_as("`(,a)", "(CONS A (QUOTE NIL))");
    reads_as("`(,@a)", "(APPEND A (QUOTE NIL))");
    reads_as("`(a . b)", "(CONS (QUOTE A) (QUOTE B))");
    reads_as("`(a ,b)", "(CONS (QUOTE A) (CONS B (QUOTE NIL)))");
}

/// The markers the reader uses internally carry a space in their names,
/// which no input can produce — this is that claim, written down.
#[test]
fn comma_markers_cannot_be_written_by_hand() {
    reads_as("(UNQUOTE MARKER)", "(UNQUOTE MARKER)");
}

/// The point of giving symbols a namespace: one name, two symbols.
#[test]
fn a_keyword_is_a_different_symbol_with_the_same_name() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let plain = mc.intern("FOO");
        let keyword = mc.intern_keyword("FOO");
        assert_ne!(plain, keyword, "a keyword is not the ordinary symbol");
        assert_eq!(mc.symbol_name(plain), "FOO");
        assert_eq!(
            mc.symbol_name(keyword),
            "FOO",
            "the colon is punctuation, not part of the name"
        );
        assert!(mc.is_keyword(keyword));
        assert!(!mc.is_keyword(plain));
        // Interning is still interning, within a namespace.
        assert_eq!(keyword, mc.intern_keyword("FOO"));
        assert_eq!(plain, mc.intern("FOO"));
    });
}

/// A keyword is a constant variable bound to itself, which is how `:foo`
/// evaluates without the evaluator knowing keywords exist.
#[test]
fn a_keyword_is_bound_to_itself() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let k = mc.intern_keyword("FOO");
        assert_eq!(mc.slot(k, crate::heap::symbol::VALUE), k);
    });
}

#[test]
fn keywords_read_and_print() {
    reads_as(":foo", ":FOO");
    reads_as("(:a :b 1)", "(:A :B 1)");
    reads_as("(:a . :b)", "(:A . :B)");
}

/// Everything else a colon could mean needs packages, so it is refused
/// rather than quietly read as an ordinary symbol with a colon in it.
#[test]
fn other_package_markers_are_refused() {
    for text in [":", "::foo", "foo:bar", "foo::bar"] {
        assert!(
            matches!(round_trip(text), Err(ReadError::PackageMarker { .. })),
            "reading {text:?} should have been refused"
        );
    }
}

#[test]
fn comma_outside_backquote_is_an_error() {
    assert!(matches!(
        round_trip("(a ,b)"),
        Err(ReadError::CommaOutsideBackquote { .. })
    ));
    assert!(matches!(
        round_trip(",a"),
        Err(ReadError::CommaOutsideBackquote { .. })
    ));
}

fn reads_as(source: &str, expected: &str) {
    assert_eq!(
        round_trip(source).as_deref(),
        Ok(expected),
        "reading {source:?}"
    );
}

#[test]
fn atoms() {
    reads_as("42", "42");
    reads_as("-7", "-7");
    reads_as("+7", "7");
    reads_as("0", "0");
    reads_as("foo", "FOO");
    reads_as("Foo", "FOO");
    reads_as("*special*", "*SPECIAL*");
    reads_as("nil", "NIL");
    reads_as("t", "T");
    reads_as("\"hello\"", "\"hello\"");
    reads_as("\"\"", "\"\"");
}

/// A lone `-` or `+` is a symbol, not a number, and anything with a
/// non-digit is a symbol. `1.5` is a symbol until floats arrive in tier 1.
#[test]
fn things_that_look_like_numbers_but_are_not() {
    reads_as("-", "-");
    reads_as("+", "+");
    reads_as("1+", "1+");
    reads_as("1.5", "1.5");
    reads_as("...", "...");
}

#[test]
fn lists() {
    reads_as("()", "NIL");
    reads_as("(1)", "(1)");
    reads_as("(1 2 3)", "(1 2 3)");
    reads_as("( 1   2 )", "(1 2)");
    reads_as("(a (b c) d)", "(A (B C) D)");
    // ((())) is a list holding a list holding (), and () is NIL.
    reads_as("((()))", "((NIL))");
}

#[test]
fn dotted_pairs() {
    reads_as("(1 . 2)", "(1 . 2)");
    reads_as("(1 2 . 3)", "(1 2 . 3)");
    // A dotted pair ending in nil is just a list, and prints as one.
    reads_as("(1 . (2 . nil))", "(1 2)");
}

#[test]
fn quote_is_just_a_list() {
    reads_as("'foo", "(QUOTE FOO)");
    reads_as("'(1 2)", "(QUOTE (1 2))");
    reads_as("''x", "(QUOTE (QUOTE X))");
}

#[test]
fn characters() {
    reads_as("#\\a", "#\\a");
    reads_as("#\\space", "#\\Space");
    reads_as("#\\Newline", "#\\Newline");
    reads_as("#\\(", "#\\(");
}

#[test]
fn comments_are_whitespace() {
    reads_as("; nothing\n42", "42");
    reads_as("#| block |# 42", "42");
    reads_as("#| outer #| inner |# still |# 42", "42");
    reads_as("(1 ; two\n 3)", "(1 3)");
}

#[test]
fn strings_escape_only_quote_and_backslash() {
    reads_as(r#""a\"b""#, r#""a\"b""#);
    reads_as(r#""a\\b""#, r#""a\\b""#);
    // No \n escape in Common Lisp: a newline in a string is a newline.
    assert_eq!(round_trip("\"a\nb\"").unwrap(), "\"a\nb\"");
}

#[test]
fn nothing_to_read() {
    reads_as("", "<nothing>");
    reads_as("   \n ; only a comment\n", "<nothing>");
}

// ---- errors are values, not panics -----------------------------------

fn fails(source: &str) -> ReadError {
    round_trip(source).expect_err("should not have read")
}

#[test]
fn malformed_input_is_an_error() {
    assert!(matches!(fails("(1 2"), ReadError::UnexpectedEnd { .. }));
    assert!(matches!(fails(")"), ReadError::UnbalancedClose { .. }));
    assert!(matches!(fails("(. 1)"), ReadError::MisplacedDot { .. }));
    assert!(matches!(fails("(1 . 2 3)"), ReadError::MisplacedDot { .. }));
    assert!(matches!(
        fails("\"unterminated"),
        ReadError::UnexpectedEnd { .. }
    ));
    assert!(matches!(
        fails("#\\nosuchname"),
        ReadError::UnknownCharacterName { .. }
    ));
    assert!(matches!(
        fails("#| unclosed"),
        ReadError::UnexpectedEnd { .. }
    ));
}

/// Bignums do not exist, so an integer that will not fit must say so rather
/// than wrap silently.
#[test]
fn an_integer_too_large_is_not_quietly_wrong() {
    // Larger than i64, so it is not a number at all and reads as a symbol.
    reads_as("99999999999999999999999999", "99999999999999999999999999");
}

/// The failure this limit exists for would otherwise be a dead process, not
/// an error — and on wasm32, where the stack is about a megabyte, it would
/// happen on input that was fine natively.
#[test]
fn nesting_too_deep_is_an_error_not_a_crash() {
    let deep = "(".repeat(MAX_DEPTH + 5) + &")".repeat(MAX_DEPTH + 5);
    assert!(matches!(fails(&deep), ReadError::TooDeep { .. }));

    // Just inside the limit still reads.
    let ok = "(".repeat(MAX_DEPTH - 1) + &")".repeat(MAX_DEPTH - 1);
    assert!(round_trip(&ok).is_ok());
}

// ---- symbols ---------------------------------------------------------

/// Two reads of the same name must give the same object, or `eq` on symbols
/// stops working and every lookup becomes a string comparison.
#[test]
fn symbols_are_interned() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let a = mc.intern("FOO");
        let b = mc.intern("FOO");
        let c = mc.intern("BAR");
        assert_eq!(a, b, "two reads of FOO gave different objects");
        assert_ne!(a, c);
        assert_eq!(mc.symbol_name(a), "FOO");
    });
}

#[test]
fn a_symbol_starts_unbound_in_both_cells() {
    use crate::heap::symbol;
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let s = mc.intern("UNUSED");
        assert_eq!(mc.slot(s, symbol::VALUE), Value::UNBOUND);
        assert_eq!(mc.slot(s, symbol::FUNCTION), Value::UNBOUND);
        assert!(mc.slot(s, symbol::PLIST).is_nil());
    });
}

/// The obarray is a root. If the collector forgets it, every symbol in the
/// system is silently reclaimed and the next read makes new ones that are
/// not `eq` to the old.
#[test]
fn symbols_survive_collection_and_stay_eq() {
    let mut heap = Heap::new();
    let before = heap.mutate(|mc| mc.intern("PERSISTENT").bits());
    heap.collect();
    heap.collect();
    heap.mutate(|mc| {
        let after = mc.intern("PERSISTENT");
        assert_ne!(after.bits(), before, "nothing moved, so nothing was tested");
        assert_eq!(mc.symbol_name(after), "PERSISTENT");
        assert_eq!(
            mc.intern("PERSISTENT"),
            after,
            "interning made a second PERSISTENT after collection"
        );
    });
    heap.verify().unwrap();
}

#[test]
fn a_read_structure_survives_collection() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let chars = Reader::chars_of("(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))");
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).unwrap().unwrap();
        mc.set_root(form);
    });
    heap.collect();
    let printed = heap.mutate(|mc| print(mc, mc.root()));
    assert_eq!(
        printed,
        "(DEFUN FACT (N) (IF (= N 0) 1 (* N (FACT (- N 1)))))"
    );
    heap.verify().unwrap();
}
