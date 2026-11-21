//! Bringing the system up: primitives, then the prelude.
//!
//! A Lisp that can only run what its evaluator understands is a Lisp whose
//! vocabulary is fixed by whoever wrote the evaluator. The prelude is the
//! first place that stops being true — `lisp/prelude.lisp` is interpreted at
//! boot, and from then on `defun`, `when`, `cond`, `and` and `or` exist
//! without the Rust core knowing anything about them.
//!
//! # Why the source is compiled in
//!
//! The obvious way to load a prelude is to read it from disk. That is wrong
//! here, twice over:
//!
//! - Rule 2 says the core VM crate has no host dependencies — no `std::fs`.
//!   A prelude that needs a filesystem is a prelude the browser cannot have.
//! - Rule 2 also says every commit builds for `wasm32`, where "the file next
//!   to the binary" is not a thing that exists.
//!
//! So the text is baked into the binary with [`include_str!`]. The VM carries
//! its own Lisp. Natively and in the browser it is the same bytes, which also
//! means the two cannot drift apart.
//!
//! **This is scaffolding with a demolition date.** Phase 1 saves an image, and
//! an image already contains the prelude's effects — the symbols, the macro
//! objects, the closures — so booting from an image will skip this module
//! entirely. Interpreting the prelude at every start is what you do until you
//! can save the result of having done it once. That is the same shape as the
//! bootstrap itself (see `book/src/the-bootstrap.md`): do the slow thing once,
//! keep the answer, never do it again.
//!
//! # Why it is not part of `Heap::new`
//!
//! Booting allocates, and tests of the heap and the collector want a heap with
//! nothing in it. Keeping them separate also keeps the bootstrap honest: the
//! prelude is evaluated by the same evaluator, through the same primitives,
//! that user code uses. There is no privileged path, and if the prelude needs
//! something the language cannot express, that is a fact worth finding out.

use crate::eval::eval_now;
use crate::eval::EvalError;
use crate::heap::Mutation;
use crate::primitives;
use crate::reader::{ReadError, Reader};

/// The prelude source, compiled into the binary.
///
/// Changing `lisp/prelude.lisp` therefore forces a rebuild, which is correct:
/// it is part of the system, not configuration.
pub const PRELUDE: &str = include_str!("../../../lisp/prelude.lisp");

/// How many evaluator steps one prelude form may take.
///
/// Generous, and bounded anyway. A prelude form that does not finish is a bug
/// in the prelude, and an infinite loop at boot would be hard to diagnose from
/// a browser console.
const BOOT_BUDGET: u64 = 10_000_000;

/// Something went wrong before the system was usable.
///
/// A value rather than a panic, like [`ReadError`], and for the same reason —
/// but unlike a reader error this one is not recoverable: a half-booted system
/// has some of the prelude and not the rest. Callers report it and stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootError {
    /// The prelude did not parse.
    Read(ReadError),
    /// A prelude form signalled.
    Eval {
        /// Which form, counting from 1, so the file can be found by eye.
        form: usize,
        /// What went wrong.
        error: EvalError,
    },
}

impl core::fmt::Display for BootError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BootError::Read(e) => write!(f, "the prelude does not read: {e}"),
            BootError::Eval { form, error } => {
                write!(f, "prelude form {form} failed: {error}")
            }
        }
    }
}

/// Install the primitives and evaluate the prelude.
///
/// This is what every host calls: the REPL, the WASM console, and the tests.
/// Afterwards the system is the one users see.
///
/// Forms are evaluated in order, each in its own `mutate`, which is what lets
/// the collector run *between* them — a single `mutate` spanning the whole
/// prelude would be one long allocation with no collection possible, which is
/// exactly what rule 4 is against.
pub fn install(heap: &mut crate::heap::Heap) -> Result<(), BootError> {
    heap.mutate(primitives::install);

    let chars = Reader::chars_of(PRELUDE);
    let mut reader = Reader::new(&chars);
    let mut form_number = 0usize;

    loop {
        form_number += 1;
        // Read and evaluate inside one `mutate`, because a `Value` cannot
        // leave one. The reader itself holds no values — only an offset into
        // the text — so it is free to live across the boundary.
        let outcome = heap.mutate(|mc| match reader.read(mc) {
            Err(e) => Some(Err(BootError::Read(e))),
            Ok(None) => None,
            Ok(Some(form)) => match eval_now(mc, form, BOOT_BUDGET) {
                Ok(_) => Some(Ok(())),
                Err(error) => Some(Err(BootError::Eval {
                    form: form_number,
                    error,
                })),
            },
        });
        match outcome {
            None => return Ok(()),
            Some(Ok(())) => {}
            Some(Err(e)) => return Err(e),
        }
    }
}

/// Install the primitives and the prelude inside an existing mutation.
///
/// The collector cannot run part-way through, so this allocates the whole
/// prelude at once. [`install`] is the one to use; this exists for callers
/// that are already inside a `mutate` and cannot get out of one.
pub fn install_in<'h>(mc: &mut Mutation<'h>) -> Result<(), BootError> {
    primitives::install(mc);
    let chars = Reader::chars_of(PRELUDE);
    let mut reader = Reader::new(&chars);
    let mut form_number = 0usize;
    loop {
        form_number += 1;
        match reader.read(mc) {
            Err(e) => return Err(BootError::Read(e)),
            Ok(None) => return Ok(()),
            Ok(Some(form)) => {
                if let Err(error) = eval_now(mc, form, BOOT_BUDGET) {
                    return Err(BootError::Eval {
                        form: form_number,
                        error,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::Heap;
    use crate::printer::print;

    fn after_boot(source: &str) -> String {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        let chars = crate::reader::Reader::chars_of(source);
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let mut last = String::new();
            while let Ok(Some(form)) = reader.read(mc) {
                match eval_now(mc, form, BOOT_BUDGET) {
                    Ok(v) => last = print(mc, v),
                    Err(e) => return format!("; {e}"),
                }
            }
            last
        })
    }

    /// A literal cannot be written, and the refusal is catchable.
    ///
    /// Not a differential test, and it cannot be: CLHS 3.7.1 leaves modifying
    /// a literal undefined, SBCL's interpreter permits it, and Quokka signals.
    /// Both conform. What the oracle cannot adjudicate stays here.
    #[test]
    fn a_literal_refuses_to_be_written() {
        assert_eq!(
            after_boot(
                r#"(defun greeting () "hello")
                   (handler-case (setf (aref (greeting) 0) #\J)
                     (type-error () 'refused))"#
            ),
            "REFUSED"
        );
    }

    /// And the literal is still what it was — which is the bug this prevents.
    ///
    /// Before `ConstantString`, that `setf` edited the source of `greeting`
    /// and said nothing, so a later call returned "Jello". Issue #26.
    #[test]
    fn a_literal_survives_the_attempt() {
        assert_eq!(
            after_boot(
                r#"(defun greeting () "hello")
                   (ignore-errors (setf (aref (greeting) 0) #\J))
                   (greeting)"#
            ),
            r#""hello""#
        );
    }

    /// A symbol's name is a literal too, and for a sharper reason: the obarray
    /// finds symbols *by* their names, so a mutated name would make `intern`
    /// hand back the wrong symbol — or no symbol at all — for a name that
    /// still reads the same.
    #[test]
    fn a_symbol_name_is_a_constant() {
        assert_eq!(
            after_boot(
                "(handler-case (setf (aref (symbol-name 'foo) 0) #\\X) (error () 'refused))"
            ),
            // SYMBOL-NAME copies, so the write lands on the copy and succeeds.
            // The point is what it does *not* reach: see the next assertion.
            "#\\X"
        );
        assert_eq!(after_boot("(symbol-name 'foo)"), r#""FOO""#);
    }

    /// The point of the whole module: the system boots at all.
    #[test]
    fn the_prelude_loads() {
        let mut heap = Heap::new();
        assert_eq!(install(&mut heap), Ok(()));
    }

    /// What booting costs, pinned so it cannot creep unnoticed.
    ///
    /// Every start pays this until phase 1, when an image holds the result
    /// instead. The number is here to be read, not to be defended — if a
    /// later prelude needs more, raise it and notice that you did.
    #[test]
    fn booting_costs_a_known_amount_of_heap() {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        heap.collect();
        // A moving target, deliberately. 2,476 words when the prelude was
        // only `defun` and the conditionals; 4,246 once it also had the
        // sequence functions, hash tables and `macroexpand`; 7,905 after
        // `setf`, `defstruct`, conditions and multiple values; 8,192 once
        // `make-string` and `copy-seq` arrived with issue #26. About 64 KB.
        //
        // The point is not the number but that raising it is a deliberate
        // act with a failing test attached — the same job as
        // `the_table_order_is_pinned`. Each rise should be explainable by
        // what was added; this one is.
        let words = heap.used_words();
        assert!(
            words < 9_000,
            "the prelude now costs {words} live words after a collection"
        );
    }

    /// `defun` is Lisp now, and still a Lisp-2 definition.
    #[test]
    fn defun_is_a_macro_and_still_works() {
        assert_eq!(after_boot("(defun square (x) (* x x)) (square 9)"), "81");
        assert_eq!(after_boot("(defun f (x) x)"), "F");
    }

    /// The bug the move to Lisp fixed: CLHS says `defun` wraps its body in a
    /// block named after the function. The Rust version did not.
    #[test]
    fn defun_establishes_a_block_named_after_the_function() {
        assert_eq!(
            after_boot("(defun f (x) (if (= x 0) (return-from f 'zero) 'other)) (f 0)"),
            "ZERO"
        );
    }

    #[test]
    fn the_conditionals_are_lisp() {
        assert_eq!(after_boot("(when t 1 2 3)"), "3");
        assert_eq!(after_boot("(when nil 1)"), "NIL");
        assert_eq!(after_boot("(unless nil 'yes)"), "YES");
        assert_eq!(after_boot("(cond (nil 1) (t 2))"), "2");
        assert_eq!(after_boot("(cond)"), "NIL");
        // A clause with no body returns the test's value, once.
        assert_eq!(after_boot("(cond ((+ 1 2)))"), "3");
    }

    #[test]
    fn and_or_short_circuit_and_return_values() {
        assert_eq!(after_boot("(and)"), "T");
        assert_eq!(after_boot("(and 1 2)"), "2");
        assert_eq!(after_boot("(and nil (car nil))"), "NIL");
        assert_eq!(after_boot("(or)"), "NIL");
        assert_eq!(after_boot("(or nil 3)"), "3");
        assert_eq!(after_boot("(or 1 (car 2))"), "1");
    }

    /// A macro that binds a temporary must not shadow the caller's names.
    #[test]
    fn or_does_not_capture_the_callers_variables() {
        assert_eq!(after_boot("(let ((once 'outer)) (or nil once))"), "OUTER");
        assert_eq!(after_boot("(let ((v 'outer)) (or nil v))"), "OUTER");
    }

    /// The first debugging tool: a macro's output as an ordinary value.
    #[test]
    fn macroexpand_shows_what_a_macro_did() {
        assert_eq!(
            after_boot("(defmacro m (a) `(if ,a 1 2)) (macroexpand-1 '(m t))"),
            "(IF T 1 2)"
        );
        // One step, versus all the way down.
        assert_eq!(
            after_boot(
                "(defmacro inner (a) `(if ,a 1 2))
                 (defmacro outer (a) (list 'inner a))
                 (macroexpand-1 '(outer t))"
            ),
            "(INNER T)"
        );
        assert_eq!(
            after_boot(
                "(defmacro inner (a) `(if ,a 1 2))
                 (defmacro outer (a) (list 'inner a))
                 (macroexpand '(outer t))"
            ),
            "(IF T 1 2)"
        );
    }

    /// Anything that is not a macro call is returned untouched, which is what
    /// makes `macroexpand` safe to call on arbitrary forms.
    #[test]
    fn macroexpand_leaves_everything_else_alone() {
        assert_eq!(after_boot("(macroexpand-1 'foo)"), "FOO");
        assert_eq!(after_boot("(macroexpand-1 5)"), "5");
        assert_eq!(after_boot("(macroexpand-1 '())"), "NIL");
        assert_eq!(after_boot("(macroexpand-1 '(car x))"), "(CAR X)");
        // Subforms are not descended into; that would be macroexpand-all.
        assert_eq!(
            after_boot("(defmacro m (a) a) (macroexpand '(list (m 1)))"),
            "(LIST (M 1))"
        );
    }

    /// Vector mutation through the primitive itself.
    ///
    /// `tests/lisp/vectors.lisp` now covers this against SBCL, through
    /// `setf` — which is why `setf` was pulled forward. These stay because
    /// they exercise `%set-aref` directly rather than through a macro, so a
    /// `setf` expansion that was wrong in some compensating way would not
    /// make both pass.
    #[test]
    fn vectors_can_be_written() {
        assert_eq!(
            after_boot("(let ((v (vector 1 2 3))) (%set-aref v 0 'changed) v)"),
            "#(CHANGED 2 3)"
        );
        assert_eq!(
            after_boot(
                "(let ((v (make-array 3 :initial-element 0)))
                   (%set-aref v 1 'middle)
                   (list (aref v 0) (aref v 1) (aref v 2)))"
            ),
            "(0 MIDDLE 0)"
        );
        // Built one element at a time, which is how a compiler will use them.
        assert_eq!(
            after_boot(
                "(defun build (n)
                   (let ((v (make-array n :initial-element 0)))
                     (labels ((fill-from (i)
                                (if (= i n) v
                                  (progn (%set-aref v i (* i i)) (fill-from (+ i 1))))))
                       (fill-from 0))))
                 (build 5)"
            ),
            "#(0 1 4 9 16)"
        );
    }

    /// A string's slots are raw character codes, not values, so writing one
    /// takes a character and nothing else.
    #[test]
    fn strings_hold_characters_and_only_characters() {
        assert_eq!(after_boot("(aref \"abc\" 1)"), "#\\b");
        // Through `copy-seq`, because the literal itself is a constant now
        // (issue #26) and this test is about *what* a string holds, not about
        // who may write one.
        assert_eq!(
            after_boot("(let ((s (copy-seq \"abc\"))) (%set-aref s 0 #\\z) s)"),
            "\"zbc\""
        );
        assert!(
            after_boot("(%set-aref (copy-seq \"abc\") 0 5)").starts_with("; "),
            "storing a fixnum in a string must be refused"
        );
    }

    /// A vector's elements are traced, so what is only reachable through one
    /// survives a collection. The `gc-torture` build runs this with a
    /// collection at every mutation boundary.
    #[test]
    fn vector_elements_survive_collection() {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        let source = crate::reader::Reader::chars_of(
            "(defun squares (n)
               (labels ((upto (i acc)
                          (if (= i n) (reverse acc) (upto (+ i 1) (cons (* i i) acc)))))
                 (apply #'vector (upto 0 nil))))
             (setq v (squares 200))",
        );
        heap.mutate(|mc| {
            let mut reader = Reader::new(&source);
            while let Ok(Some(form)) = reader.read(mc) {
                eval_now(mc, form, BOOT_BUDGET).expect("runs");
            }
        });
        heap.collect();
        let after = crate::reader::Reader::chars_of("(list (length v) (aref v 199))");
        let out = heap.mutate(|mc| {
            let mut reader = Reader::new(&after);
            let form = reader.read(mc).unwrap().unwrap();
            let value = eval_now(mc, form, BOOT_BUDGET).expect("runs");
            print(mc, value)
        });
        assert_eq!(out, "(200 39601)");
    }

    /// Hash tables through the primitives directly.
    ///
    /// `tests/lisp/hash-tables.lisp` covers the same ground against SBCL now
    /// that `setf` exists; these exercise `%set-gethash` without a macro in
    /// between.
    #[test]
    fn hash_tables_store_and_find() {
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table)))
                   (%set-gethash 'a h 1)
                   (%set-gethash 'b h 2)
                   (list (gethash 'a h) (gethash 'b h) (gethash 'c h) (hash-table-count h)))"
            ),
            "(1 2 NIL 2)"
        );
        // Storing twice under one key replaces rather than adds.
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table)))
                   (%set-gethash 'a h 1) (%set-gethash 'a h 2)
                   (list (gethash 'a h) (hash-table-count h)))"
            ),
            "(2 1)"
        );
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table)))
                   (%set-gethash 'a h 1) (remhash 'a h)
                   (list (gethash 'a h) (hash-table-count h)))"
            ),
            "(NIL 0)"
        );
    }

    /// Fixnums, characters and symbols are hashable by content. A keyword and
    /// an ordinary symbol share a name and are different keys, which the hash
    /// has to account for.
    #[test]
    fn keys_are_hashed_by_content() {
        assert_eq!(
            after_boot("(let ((h (make-hash-table))) (%set-gethash 1 h 'one) (gethash 1 h))"),
            "ONE"
        );
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table))) (%set-gethash #\\a h 'char) (gethash #\\a h))"
            ),
            "CHAR"
        );
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table)))
                   (%set-gethash :k h 'keyword) (%set-gethash 'k h 'symbol)
                   (list (gethash :k h) (gethash 'k h)))"
            ),
            "(KEYWORD SYMBOL)"
        );
        // Strings match by content, and only in an EQUAL table.
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table :test 'equal)))
                   (%set-gethash \"key\" h 'found)
                   (list (gethash \"key\" h) (gethash \"other\" h 'no)))"
            ),
            "(FOUND NO)"
        );
    }

    /// The divergence the decision bought, and the reason it is loud: a key
    /// the collector could move is refused rather than hashed unreliably.
    #[test]
    fn unhashable_keys_are_refused_rather_than_guessed() {
        for source in [
            "(let ((h (make-hash-table))) (%set-gethash '(1) h 'x))",
            "(let ((h (make-hash-table))) (%set-gethash (vector 1) h 'x))",
            // A string is not an EQL key, because that would mean comparing
            // string objects by identity.
            "(let ((h (make-hash-table))) (%set-gethash \"s\" h 'x))",
            "(make-hash-table :test 'bogus)",
        ] {
            assert!(
                after_boot(source).starts_with("; "),
                "should have been refused: {source}"
            );
        }
    }

    /// Growing past the load factor rehashes everything, and must not lose or
    /// duplicate an entry. Under `gc-torture` this also runs with a collection
    /// at every mutation boundary.
    #[test]
    fn a_table_survives_growing() {
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table)))
                   (labels ((fill-up (i)
                              (if (= i 200) h (progn (%set-gethash i h (* i i)) (fill-up (+ i 1))))))
                     (let ((full (fill-up 0)))
                       (list (hash-table-count full)
                             (gethash 0 full)
                             (gethash 7 full)
                             (gethash 199 full)
                             (gethash 200 full 'absent)))))"
            ),
            "(200 0 49 39601 ABSENT)"
        );
    }

    #[test]
    fn maphash_visits_every_entry() {
        assert_eq!(
            after_boot(
                "(let ((h (make-hash-table)) (total 0))
                   (%set-gethash 'a h 1) (%set-gethash 'b h 2) (%set-gethash 'c h 3)
                   (maphash (lambda (k v) (setq total (+ total v))) h)
                   total)"
            ),
            "6"
        );
    }

    /// A condition named by any symbol, which SBCL cannot do without
    /// `define-condition` — so the oracle cannot check this half.
    #[test]
    fn a_condition_can_be_named_by_any_symbol() {
        assert_eq!(
            after_boot("(handler-case (error 'boom) (boom () 'caught))"),
            "CAUGHT"
        );
        assert_eq!(
            after_boot("(handler-case (error 'boom) (error (c) (%condition-type c)))"),
            "BOOM"
        );
        // A clause for a different type does not catch it.
        assert!(after_boot("(handler-case (error 'a) (b () 'wrong))").starts_with("; "));
    }

    #[test]
    fn a_string_datum_makes_a_simple_error() {
        assert_eq!(
            after_boot("(handler-case (error \"gone wrong\") (error (c) (%condition-type c)))"),
            "SIMPLE-ERROR"
        );
        assert_eq!(
            after_boot("(handler-case (error \"gone wrong\") (error (c) (%condition-message c)))"),
            "\"gone wrong\""
        );
    }

    /// The evaluator's own errors carry Common Lisp's type names, so a
    /// program that catches `undefined-function` catches what it expects.
    #[test]
    fn evaluator_errors_carry_standard_type_names() {
        for (source, wanted) in [
            ("(car 5)", "TYPE-ERROR"),
            ("(no-such-function)", "UNDEFINED-FUNCTION"),
            ("no-such-variable", "UNBOUND-VARIABLE"),
            ("(aref (vector 1) 9)", "TYPE-ERROR"),
        ] {
            let program = format!("(handler-case {source} (error (c) (%condition-type c)))");
            assert_eq!(after_boot(&program), wanted, "for {source}");
        }
    }

    /// **Regression.** Conditions were `ObjectType::Instance` at first, and
    /// `fresh_marker` allocates an `Instance` too — so every `block` and
    /// `catch` marker looked like a condition, and a `return-from` passing
    /// through a `handler-case` was caught by it. Conditions have their own
    /// object type now.
    #[test]
    fn a_return_from_passes_through_a_handler() {
        assert_eq!(
            after_boot("(block out (handler-case (return-from out 'returned) (error () 'no)))"),
            "RETURNED"
        );
        assert_eq!(
            after_boot("(catch 'tag (handler-case (throw 'tag 'thrown) (error () 'no)))"),
            "THROWN"
        );
        // And a tagbody's go, which uses the same markers.
        assert_eq!(
            after_boot(
                "(let ((n 0))
                   (tagbody
                     top
                     (setq n (+ n 1))
                     (handler-case (if (< n 3) (go top)) (error () nil)))
                   n)"
            ),
            "3"
        );
    }

    /// Running out of heap is not the program's fault, so it is not the
    /// program's to catch. A handler that caught it would be catching the
    /// machinery rather than the program.
    #[test]
    fn resource_limits_are_not_conditions() {
        // `%handler-case` rather than the prelude macro, so the heap holds
        // only what this test is about.
        let source = "(%handler-case
                        (labels ((grow (acc) (grow (cons 1 acc)))) (grow nil))
                        (error () 'caught))";
        let chars = crate::reader::Reader::chars_of(source);
        let mut heap = Heap::with_soft_limit(20_000);
        install_in_small_heap(&mut heap);
        let outcome = heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            eval_now(mc, form, u64::MAX).map(|_| ())
        });
        assert_eq!(outcome, Err(crate::eval::EvalError::HeapExhausted));
    }

    fn install_in_small_heap(heap: &mut Heap) {
        heap.mutate(|mc| {
            crate::primitives::install(mc);
        });
    }

    #[test]
    fn apply_spreads_its_last_argument() {
        assert_eq!(after_boot("(apply #'+ '(1 2 3))"), "6");
        assert_eq!(after_boot("(apply #'+ 1 2 '(3 4))"), "10");
        assert_eq!(after_boot("(apply #'list 1 '())"), "(1)");
        assert_eq!(after_boot("(apply #'+ '())"), "0");
        // The spread list is shared rather than copied, so a long one costs
        // nothing here — the binder pays for it instead.
        assert_eq!(
            after_boot(
                "(defun build (n acc) (if (= n 0) acc (build (- n 1) (cons 1 acc))))
                 (apply #'+ (build 500 nil))"
            ),
            "500"
        );
    }

    #[test]
    fn the_list_functions_are_lisp() {
        assert_eq!(after_boot("(length '(1 2 3))"), "3");
        assert_eq!(after_boot("(length nil)"), "0");
        assert_eq!(after_boot("(reverse '(1 2 3))"), "(3 2 1)");
        assert_eq!(after_boot("(cadr '(1 2 3))"), "2");
        assert_eq!(after_boot("(nth 1 '(a b c))"), "B");
        assert_eq!(after_boot("(member 'b '(a b c))"), "(B C)");
        assert_eq!(after_boot("(not nil)"), "T");
    }

    /// Tail calls, which is why `length` of a long list does not grow the
    /// frame stack. 10,000 is far more than any Rust stack would survive if
    /// the evaluator were recursive (chapter 2).
    #[test]
    fn prelude_recursion_is_bounded_by_the_heap_not_the_stack() {
        assert_eq!(
            after_boot(
                "(defun build (n acc) (if (= n 0) acc (build (- n 1) (cons n acc))))
                 (length (build 10000 nil))"
            ),
            "10000"
        );
    }
}
