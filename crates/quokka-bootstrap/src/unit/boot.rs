//! Tests for [`crate::boot`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

use super::*;
use quokka_vm::heap::Heap;
use quokka_vm::printer::print;

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
        after_boot("(handler-case (setf (aref (symbol-name 'foo) 0) #\\X) (error () 'refused))"),
        // SYMBOL-NAME hands back the name itself, not a copy (#44), so the
        // write meets a constant string and is refused.
        "REFUSED"
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
    // 9,663 once an expander became a function of the form and the
    // environment (#76): every macro now takes its arguments apart in
    // generated code, where a plain lambda list used to be left alone.
    // 10,588 once the prelude learned setf functions and `%block-body`
    // (#78): about fifty lines of new functions. 11,623 with the condition
    // types' lattice and `multiple-value-bind`'s declarations (#80, #81).
    // 12,160 with `handler-case`'s `:no-error` clause (#106): the macro's
    // second expansion and the two functions that find the clause.
    // 13,924 with the obarray's index (#102): a vector of 1,024 buckets
    // and a cons for each symbol, which a collection keeps because the
    // index is a root. Not the prelude's cost, and an image does not
    // carry it; it is what finding a symbol without walking them all costs.
    //
    // The point is not the number but that raising it is a deliberate
    // act with a failing test attached — the same job as
    // `the_table_order_is_pinned`. Each rise should be explainable by
    // what was added; this one is.
    let words = heap.used_words();
    assert!(
        words < 14_300,
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
        after_boot("(let ((h (make-hash-table))) (%set-gethash #\\a h 'char) (gethash #\\a h))"),
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

/// What is still refused (#64): a cons in an EQUAL table, which `equal`
/// compares by contents an address cannot stand for, and an unknown test.
#[test]
fn unhashable_keys_are_refused_rather_than_guessed() {
    for source in [
        "(let ((h (make-hash-table :test 'equal))) (%set-gethash '(1) h 'x))",
        "(make-hash-table :test 'bogus)",
    ] {
        assert!(
            after_boot(source).starts_with("; "),
            "should have been refused: {source}"
        );
    }
}

/// Any other key is the same key only as itself (#64): a list, a vector, or
/// a string in an EQL table is found by the object and not by an equal one —
/// and still found after collections have moved it, the table having been
/// marked stale by the collector and rehashed.
#[test]
fn keys_without_contents_are_found_by_identity() {
    assert_eq!(
        after_boot(
            "(let ((h (make-hash-table)) (k (list 1)) (v (vector 1)) (s (make-string 1 :initial-element #\\a)))
               (%set-gethash k h 'list)
               (%set-gethash v h 'vector)
               (%set-gethash s h 'string)
               (labels ((churn (n) (if (> n 0) (progn (make-string 64 :initial-element #\\x) (churn (- n 1))))))
                 (churn 20000))
               (list (gethash k h) (gethash v h) (gethash s h)
                     (gethash (list 1) h 'other) (gethash \"a\" h 'other)))"
        ),
        "(LIST VECTOR STRING OTHER OTHER)"
    );
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
    assert_eq!(outcome, Err(quokka_vm::error::EvalError::HeapExhausted));
}

fn install_in_small_heap(heap: &mut Heap) {
    heap.mutate(|mc| {
        quokka_vm::primitives::install(mc);
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

/// The compiler, loaded and used: a Lisp function compiles a Lisp form into
/// bytecode, and the VM runs it.
mod compiler {
    use super::*;
    use quokka_vm::heap::Register;
    use quokka_vm::vm::run_now;

    /// Read `source` into the root and evaluate it there.
    ///
    /// A thin wrapper over [`crate::eval::eval_root`], which drives the
    /// machine a slice at a time so the collector can run between them.
    /// Compiling allocates far more than evaluating — an instruction list, a
    /// constants list, a `gensym` per branch, a fresh `append` per
    /// concatenation — and doing it inside one mutation exhausts the heap.
    fn drive(heap: &mut Heap, source: &str) -> Result<(), String> {
        let chars = Reader::chars_of(source);
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            match reader.read(mc) {
                Ok(Some(form)) => {
                    mc.set_register(Register::Value, form);
                    Ok(())
                }
                Ok(None) => Err(String::from("; nothing to read")),
                Err(e) => Err(format!("; {e:?}")),
            }
        })?;
        crate::eval::eval_sliced(heap, 10_000_000, 500).map_err(|e| format!("; {e}"))
    }

    /// Compile `source` with the Lisp compiler and run the result on the VM.
    fn compile_and_run(source: &str) -> String {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        install_compiler(&mut heap).expect("the compiler loads");
        if let Err(e) = drive(&mut heap, &format!("(compile-form '{source})")) {
            return format!("; compiling: {e}");
        }
        // The compiled function is the root, so it survives the collections
        // the run below may need.
        let outcome = heap.mutate(|mc| {
            let f = mc.register(Register::Value);
            match run_now(mc, f, 1_000_000) {
                Ok(v) => Ok(print(mc, v)),
                Err(e) => Err(format!("; running: {e}")),
            }
        });
        match outcome {
            Ok(s) => s,
            Err(e) => e,
        }
    }

    /// What `compile-form` hands back, printed. Book chapter 28 shows this
    /// transcript, so something had better check it.
    #[test]
    fn a_compiled_function_prints_its_name() {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        install_compiler(&mut heap).expect("the compiler loads");
        drive(&mut heap, "(compile-form '(+ 1 2))").expect("compiles");
        let printed = heap.mutate(|mc| {
            let f = mc.register(Register::Value);
            print(mc, f)
        });
        assert_eq!(printed, "#<compiled COMPILED-FORM>");
    }

    /// **The first Lisp-compiled Quokka program.**
    ///
    /// Nothing in Rust decided what these instructions should be. A Lisp
    /// function read the form, chose `GFREF`, `FIX`, `FIX`, `CALL`, `RET`,
    /// and handed them to `%assemble`.
    #[test]
    fn a_call_compiles_and_runs() {
        assert_eq!(compile_and_run("(+ 1 2)"), "3");
    }

    #[test]
    fn nested_calls_compile() {
        assert_eq!(compile_and_run("(+ (* 2 3) 4)"), "10");
        assert_eq!(compile_and_run("(- 10 4)"), "6");
    }

    #[test]
    fn constants_compile() {
        assert_eq!(compile_and_run("7"), "7");
        assert_eq!(compile_and_run("nil"), "NIL");
        assert_eq!(compile_and_run("t"), "T");
        assert_eq!(compile_and_run("(quote hello)"), "HELLO");
        assert_eq!(compile_and_run("\"text\""), "\"text\"");
    }

    /// Beyond a `FIX` operand, so it has to become a constant instead.
    #[test]
    fn a_large_integer_becomes_a_constant() {
        assert_eq!(compile_and_run("1000000"), "1000000");
        assert_eq!(compile_and_run("(+ 100000 1)"), "100001");
    }

    #[test]
    fn if_compiles_to_jumps() {
        assert_eq!(compile_and_run("(if t 1 2)"), "1");
        assert_eq!(compile_and_run("(if nil 1 2)"), "2");
        assert_eq!(compile_and_run("(if (= 1 1) (+ 1 1) 99)"), "2");
    }

    #[test]
    fn let_binds_and_reads_back() {
        assert_eq!(compile_and_run("(let ((a 1) (b 2)) (+ a b))"), "3");
        assert_eq!(compile_and_run("(let ((a 1)) (let ((b 2)) (- b a)))"), "1");
    }

    /// Every init is evaluated in the *outer* scope, which is what `let`
    /// means. Here the inner `a` must not be visible to its own init.
    #[test]
    fn a_lets_inits_run_in_the_outer_scope() {
        assert_eq!(
            compile_and_run("(let ((a 1)) (let ((a (+ a 10))) a))"),
            "11"
        );
    }

    #[test]
    fn setq_assigns_and_returns_the_value() {
        assert_eq!(compile_and_run("(let ((a 1)) (setq a 5) a)"), "5");
        assert_eq!(compile_and_run("(let ((a 1)) (setq a 5))"), "5");
        assert_eq!(compile_and_run("(progn (setq *g* 7) *g*)"), "7");
    }

    #[test]
    fn progn_keeps_only_the_last_value() {
        assert_eq!(compile_and_run("(progn 1 2 3)"), "3");
        assert_eq!(compile_and_run("(progn)"), "NIL");
    }

    #[test]
    fn a_lambda_compiles_and_is_callable() {
        assert_eq!(compile_and_run("((lambda (x) (+ x 1)) 41)"), "42");
        assert_eq!(compile_and_run("((lambda (x y) (- x y)) 10 4)"), "6");
        assert_eq!(compile_and_run("((lambda () 7))"), "7");
    }

    /// The `adder` from the README and chapter 26, compiled: the inner
    /// lambda reads `n` one scope out, which is `LREF 1 0`.
    #[test]
    fn a_closure_captures_the_scope_it_was_made_in() {
        assert_eq!(
            compile_and_run("((lambda (n) ((lambda (x) (+ x n)) 5)) 37)"),
            "42"
        );
    }

    /// A lambda is compiled into the enclosing function's constants, so the
    /// constants being accumulated have to be saved and put back. Two in a
    /// row is what catches getting that wrong.
    #[test]
    fn two_lambdas_do_not_disturb_each_others_constants() {
        assert_eq!(
            compile_and_run("(+ ((lambda (x) (+ x 1)) 1) ((lambda (y) (* y 2)) 20))"),
            "42"
        );
    }

    /// Macros are expanded before anything looks at the form as a call, so
    /// `when` never reaches the call path — and neither does anything a user
    /// defines with `defmacro`.
    #[test]
    fn macros_are_expanded_before_compiling() {
        assert_eq!(compile_and_run("(when t 1 2)"), "2");
        assert_eq!(compile_and_run("(when nil 1)"), "NIL");
        assert_eq!(compile_and_run("(unless nil 'yes)"), "YES");
        assert_eq!(
            compile_and_run("(cond ((= 1 2) 'no) ((= 1 1) 'yes))"),
            "YES"
        );
        assert_eq!(compile_and_run("(and 1 2 3)"), "3");
        assert_eq!(compile_and_run("(or nil 4)"), "4");
    }

    #[test]
    fn flet_binds_a_local_function() {
        assert_eq!(compile_and_run("(flet ((f (x) (+ x 1))) (f 41))"), "42");
    }

    /// The Lisp-2 made visible: a local *function* named `list` and nothing
    /// happening to the variable namespace.
    #[test]
    fn a_local_function_shadows_a_global_one() {
        assert_eq!(
            compile_and_run("(flet ((length (x) 99)) (length 'anything))"),
            "99"
        );
    }

    /// An `flet` function is made before its scope exists, so it cannot see
    /// itself — which is the whole difference from `labels`, and is why
    /// calling `f` from inside `f` here reaches the global one and fails.
    #[test]
    fn an_flet_function_cannot_see_itself() {
        let out = compile_and_run("(flet ((f (x) (f x))) (f 1))");
        assert!(out.contains("undefined") || out.contains("F"), "{out}");
    }

    /// `labels`, which needed the thirty-seventh opcode: the closures have to
    /// capture a scope that already exists, so the scope is made empty and
    /// `FSET` stores them into it.
    #[test]
    fn labels_functions_can_see_themselves() {
        assert_eq!(
            compile_and_run("(labels ((down (n) (if (= n 0) 'done (down (- n 1))))) (down 5))"),
            "DONE"
        );
    }

    #[test]
    fn labels_functions_can_see_each_other() {
        assert_eq!(
            compile_and_run(
                "(labels ((evenp (n) (if (= n 0) t (oddp (- n 1))))
                          (oddp (n) (if (= n 0) nil (evenp (- n 1)))))
                   (evenp 10))"
            ),
            "T"
        );
    }

    /// The counting that chapter 26 warns about, compiled: inside `f`, the
    /// variable `x` is two links out — past `f`'s own parameters and past the
    /// function scope `flet` pushed, which a variable lookup steps over but
    /// the compiler must still count.
    #[test]
    fn a_function_scope_is_a_link_the_compiler_counts() {
        assert_eq!(compile_and_run("(let ((x 7)) (flet ((f () x)) (f)))"), "7");
        assert_eq!(
            compile_and_run("(let ((x 7)) (labels ((f () x)) (f)))"),
            "7"
        );
    }

    /// Twenty thousand deep, which only terminates because the compiler put
    /// `TCALL` where a `CALL` would have been.
    ///
    /// The interpreter gets this by popping a frame before dispatching, and
    /// so *discovers* that a call was in tail position. The compiler
    /// **decides** it, from the shape of the source, and writes it down.
    #[test]
    fn a_tail_recursive_loop_runs_in_constant_space() {
        assert_eq!(
            compile_and_run("(labels ((down (n) (if (= n 0) 'done (down (- n 1))))) (down 20000))"),
            "DONE"
        );
    }

    /// The same shape with the call *not* in tail position — it is an
    /// argument to `+` — so each level keeps a frame. Twenty thousand would
    /// be a different test; this one only has to prove the two differ.
    #[test]
    fn a_non_tail_call_is_still_a_call() {
        assert_eq!(
            compile_and_run("(labels ((add (n) (if (= n 0) 0 (+ 1 (add (- n 1)))))) (add 100))"),
            "100"
        );
    }

    /// Tail position reaches through everything that passes a value along,
    /// which is the same list of forms chapter 23 found for multiple values.
    #[test]
    fn tail_position_reaches_through_if_and_let_and_progn() {
        let out = compile_instructions("(lambda (n) (if n (f n) (g n)))");
        assert_eq!(out.matches("TCALL").count(), 2, "{out}");
        let out = compile_instructions("(lambda (n) (let ((x n)) (f x)))");
        assert!(out.contains("TCALL"), "{out}");
        let out = compile_instructions("(lambda (n) (progn (f n) (g n)))");
        assert_eq!(out.matches("TCALL").count(), 1, "{out}");
        assert_eq!(out.matches("(CALL").count(), 1, "{out}");
    }

    // ---- #36: compiled code catching errors ------------------------------
    //
    // Four faults, each with a test that fails without its fix: the compiler
    // had no `%handler-case`, the unwinder handed a handler `nil` instead of
    // the condition, the VM's `fail` halted where the interpreter signals,
    // and `(error c)` did not re-signal `c`.

    /// The case that was reported: an error raised *inside a primitive*,
    /// caught by compiled code.
    #[test]
    fn compiled_code_catches_an_error_from_a_primitive() {
        assert_eq!(
            compile_and_run("(handler-case (car 5) (type-error () 'caught))"),
            "CAUGHT"
        );
    }

    /// The clause variable is the condition itself — which needs the
    /// unwinder to hand the handler the condition, not the `Value` register.
    #[test]
    fn a_clause_variable_holds_the_condition() {
        assert_eq!(
            compile_and_run("(handler-case (car 5) (type-error (e) (%condition-type e)))"),
            "TYPE-ERROR"
        );
    }

    /// A clause that does not want the condition passes it outwards, to a
    /// handler that does. The dispatch is compiled code ending in
    /// `(error c)`, and the inner record is already gone by then.
    #[test]
    fn an_unwanted_condition_goes_further_out() {
        assert_eq!(
            compile_and_run(
                "(handler-case (handler-case (car 5) (unbound-variable () 'inner)) \
                 (type-error () 'outer))"
            ),
            "OUTER"
        );
    }

    /// CLHS: given a condition, `error` signals *that* condition. The VM
    /// used to wrap it in a fresh `simple-error`, so the outer `foo` clause
    /// could never have matched.
    #[test]
    fn error_given_a_condition_signals_that_condition() {
        assert_eq!(
            compile_and_run(
                "(handler-case (handler-case (error 'foo) (error (c) (error c))) \
                 (foo () 'same))"
            ),
            "SAME"
        );
    }

    /// The interpreter signals a `type-error` for a datum that is neither a
    /// symbol nor a string, and now the VM does too.
    #[test]
    fn error_given_a_number_is_a_type_error() {
        assert_eq!(
            compile_and_run("(handler-case (error 42) (type-error () 'refused))"),
            "REFUSED"
        );
    }

    #[test]
    fn a_form_that_does_not_signal_keeps_its_value() {
        assert_eq!(compile_and_run("(handler-case 42 (error () 'never))"), "42");
    }

    /// `ignore-errors` is a prelude macro over `handler-case`, and the prelude
    /// is what #33 is about to compile.
    #[test]
    fn ignore_errors_compiles() {
        assert_eq!(compile_and_run("(ignore-errors (car 5))"), "NIL");
    }

    /// With nothing to catch it, a program error still stops the machine,
    /// carrying the original error rather than a condition allocated for
    /// nobody.
    #[test]
    fn with_no_handler_the_error_still_stops_the_machine() {
        let out = compile_and_run("(car 5)");
        assert!(out.starts_with("; running:"), "{out}");
        assert!(out.contains("CAR wants a list"), "{out}");
    }

    /// A fault in the bytecode or the bootstrap is **not** catchable, even
    /// with a catch-all clause right there. Calling the interpreted `length`
    /// from compiled code breaks #25's rule, and a `handler-case` quietly
    /// swallowing that would hide exactly the kind of mistake the rule is
    /// there to expose.
    #[test]
    fn a_fault_in_the_machine_is_not_the_programs_to_catch() {
        let out = compile_and_run("(handler-case (length '(1 2)) (error () 'swallowed))");
        assert!(out.contains("interpreted closure"), "{out}");
    }

    /// The protected form is not in tail position, because the record must
    /// outlive what it calls; a clause body is, because the record is gone by
    /// the time it runs. The clause takes everything, so there is no
    /// re-signal after it: `condition`, since `error` no longer means
    /// everything once a condition need not be an error (#125).
    #[test]
    fn only_a_clause_body_is_in_tail_position() {
        let out = compile_instructions("(lambda (n) (handler-case (f n) (condition () (g n))))");
        assert_eq!(out.matches("TCALL").count(), 1, "{out}");
        assert_eq!(out.matches("(CALL").count(), 1, "{out}");
        assert!(out.contains("HANDLER"), "{out}");
    }

    /// A clause that does not take everything is followed by the re-signal,
    /// which is a call to `error`, in tail position too — but never a tail
    /// call, so that the frame that signalled stays for the debugger (#130):
    /// one `TCALL`, the clause body's. After a catch-all there is no re-signal, because it
    /// could never run; the test above is the other half.
    #[test]
    fn a_selective_handler_ends_in_a_re_signal() {
        let out = compile_instructions("(lambda (n) (handler-case (f n) (type-error () (g n))))");
        assert_eq!(out.matches("TCALL").count(), 1, "{out}");
    }

    // ---- #37: the special operators the compiler did not have -----------

    #[test]
    fn a_throw_finds_its_catch() {
        assert_eq!(compile_and_run("(catch 'done (throw 'done 5) 99)"), "5");
    }

    #[test]
    fn a_throw_passes_an_inner_catch_by() {
        assert_eq!(
            compile_and_run(
                "(catch 'outer (catch 'inner (throw 'outer 'skipped-inner)) 'not-here)"
            ),
            "SKIPPED-INNER"
        );
    }

    /// The difference from `block`: the `throw` is in a function whose
    /// source has no `catch` in it. Found at run time, by the stack.
    #[test]
    fn a_throw_reaches_a_catch_in_its_caller() {
        assert_eq!(
            compile_and_run("(flet ((thrower () (throw 'k 3))) (catch 'k (thrower) 'not-here))"),
            "3"
        );
    }

    #[test]
    fn a_throw_carries_every_value() {
        assert_eq!(
            compile_and_run("(multiple-value-call #'list (catch 'v (throw 'v (values 1 2))))"),
            "(1 2)"
        );
    }

    #[test]
    fn a_throw_with_no_catch_says_so() {
        let out = compile_and_run("(throw 'nowhere 1)");
        assert!(out.contains("no exit named NOWHERE"), "{out}");
    }

    #[test]
    fn unwind_protect_cleans_up_on_a_normal_exit() {
        assert_eq!(
            compile_and_run("(let ((log nil)) (unwind-protect 1 (setq log 'ran)) log)"),
            "RAN"
        );
    }

    /// The inline copy of the cleanup runs between the protected form and
    /// the end, so without `MVSAVE`/`MVRESTORE` its value would replace the
    /// protected form's.
    #[test]
    fn unwind_protect_keeps_every_value_of_the_protected_form() {
        assert_eq!(
            compile_and_run("(multiple-value-call #'list (unwind-protect (values 1 2) 3))"),
            "(1 2)"
        );
    }

    /// The other copy: the fragment the unwinder jumps to.
    #[test]
    fn unwind_protect_cleans_up_when_a_throw_passes_through() {
        assert_eq!(
            compile_and_run(
                "(let ((log nil)) (catch 'x (unwind-protect (throw 'x 1) (setq log 'cleaned))) log)"
            ),
            "CLEANED"
        );
    }

    /// Chapter 20's test, compiled from source this time rather than
    /// assembled by hand.
    #[test]
    fn cleanups_run_innermost_first() {
        assert_eq!(
            compile_and_run(
                "(let ((order nil)) \
                   (catch 'x \
                     (unwind-protect \
                       (unwind-protect (throw 'x 0) (setq order (cons 'inner order))) \
                       (setq order (cons 'outer order)))) \
                   order)"
            ),
            "(OUTER INNER)"
        );
    }

    /// A `block` removes its own scope however it is left. It did not: the
    /// record saved the environment *inside* the block, so both ways out put
    /// the block's scope back, and the variable after it was looked up one
    /// link too deep. A bug from #28 that no test read a variable late
    /// enough to see.
    #[test]
    fn a_block_unbinds_its_name_however_it_is_left() {
        assert_eq!(compile_and_run("(let ((x 5)) (block b 1) x)"), "5");
        assert_eq!(
            compile_and_run("(let ((x 5)) (block b (return-from b 1)) x)"),
            "5"
        );
    }

    #[test]
    fn a_return_from_runs_a_cleanup_on_its_way_out() {
        assert_eq!(
            compile_and_run(
                "(let ((log nil)) (block b (unwind-protect (return-from b 1) (setq log 'ran))) log)"
            ),
            "RAN"
        );
    }

    /// #36 and #37 together: an error unwinds to a handler, and the cleanup
    /// in between runs first.
    #[test]
    fn an_error_runs_a_cleanup_on_its_way_to_a_handler() {
        assert_eq!(
            compile_and_run(
                "(let ((log nil)) \
                   (handler-case (unwind-protect (car 5) (setq log 'ran)) \
                     (type-error () log)))"
            ),
            "RAN"
        );
    }

    #[test]
    fn multiple_value_prog1_keeps_the_first_forms_values() {
        assert_eq!(
            compile_and_run(
                "(let ((x 0)) \
                   (multiple-value-call #'list \
                     (multiple-value-prog1 (values 'a 'b) (setq x 1)) \
                     x))"
            ),
            "(A B 1)"
        );
    }

    #[test]
    fn the_and_locally_are_their_bodies() {
        assert_eq!(compile_and_run("(the fixnum 42)"), "42");
        assert_eq!(compile_and_run("(locally 1 2 3)"), "3");
    }

    /// The interpreter's rule, including the three spellings it accepts.
    #[test]
    fn eval_when_runs_only_for_execute() {
        assert_eq!(compile_and_run("(eval-when (:execute) 'ran)"), "RAN");
        assert_eq!(compile_and_run("(eval-when (execute) 'ran)"), "RAN");
        assert_eq!(compile_and_run("(eval-when (eval) 'ran)"), "RAN");
        assert_eq!(
            compile_and_run("(eval-when (:compile-toplevel) 'never)"),
            "NIL"
        );
    }

    /// The guard, which can no longer be shown failing on a real operator:
    /// since #33 the compiler has a case for every special operator the
    /// interpreter has. So this checks that — each name in the interpreter's
    /// own table, `eval::special::ALL`, reaches a case in the compiler rather
    /// than the guard. A new special operator added to the interpreter and
    /// not the compiler fails here, by name.
    #[test]
    fn the_compiler_has_a_case_for_every_special_operator() {
        // One heap for all of them: booting the compiler per operator made
        // this the slowest test in the suite for no gain.
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        install_compiler(&mut heap).expect("the compiler loads");
        for (name, _) in quokka_vm::special::ALL {
            let out = drive(&mut heap, &format!("(compile-form '({name}))")).err();
            let out = out.unwrap_or_default();
            assert!(
                !out.contains("cannot compile the special operator"),
                "{name}: {out}"
            );
        }
    }

    /// `defmacro` compiles. Run by this *interpreted* compiler it defines a
    /// compiled macro — and using that macro from interpreted code is then
    /// refused, which is the one-evaluator rule doing its job; the compiled
    /// image is where it is used.
    #[test]
    fn defmacro_compiles() {
        assert_eq!(
            compile_and_run("(defmacro twice (x) (list 'progn x x))"),
            "TWICE"
        );
    }

    /// And `macrolet` waits for a compiled compiler: this one is interpreted,
    /// so it cannot call the expander it just compiled.
    #[test]
    fn macrolet_needs_a_compiled_compiler() {
        let out = compile_and_run("(macrolet ((m () 1)) (m))");
        assert!(out.contains("compiled closure"), "{out}");
    }

    /// The interpreter's own answer, read from the symbol.
    #[test]
    fn special_operator_p_reads_the_symbol() {
        assert_eq!(
            compile_and_run(
                "(list (%special-operator-p 'if) (%special-operator-p 'car) (%special-operator-p 42))"
            ),
            "(T NIL NIL)"
        );
    }

    /// Every form that leaves a record cancels tail position for what is
    /// inside it; `multiple-value-prog1` does too when it has forms to run
    /// after the first, because the first one's values must survive them.
    #[test]
    fn records_cancel_tail_position() {
        for source in [
            "(lambda (n) (catch 'k (f n)))",
            "(lambda (n) (unwind-protect (f n) (g n)))",
            "(lambda (n) (multiple-value-prog1 (f n) (g n)))",
        ] {
            let out = compile_instructions(source);
            assert_eq!(out.matches("TCALL").count(), 0, "{source}: {out}");
        }
        let out = compile_instructions("(lambda (n) (multiple-value-prog1 (f n)))");
        assert_eq!(out.matches("TCALL").count(), 1, "{out}");
    }

    // ---- #37: tagbody and go, by B (TAG/GO) and C (JMP-TAG) ------------

    #[test]
    fn a_tagbody_is_a_loop() {
        assert_eq!(
            compile_and_run(
                "(let ((i 0) (acc nil)) \
                   (tagbody top \
                     (if (= i 3) (go end)) \
                     (setq acc (cons i acc)) \
                     (setq i (+ i 1)) \
                     (go top) \
                    end) \
                   acc)"
            ),
            "(2 1 0)"
        );
    }

    /// A thousand trips: the record survives every `go`, which `TAG` as
    /// first built — sharing `BLOCK`'s pop-on-landing — could not do twice.
    #[test]
    fn a_tagbody_can_be_gone_to_again_and_again() {
        assert_eq!(
            compile_and_run(
                "(let ((n 0)) (tagbody again (setq n (+ n 1)) (if (< n 1000) (go again))) n)"
            ),
            "1000"
        );
    }

    /// A tagbody yields `nil`; one with no tags is just its statements.
    #[test]
    fn a_tagbody_yields_nil() {
        assert_eq!(compile_and_run("(tagbody 1 2 3)"), "NIL");
        assert_eq!(
            compile_and_run("(let ((x 0)) (list (tagbody (setq x 1) (setq x 2)) x))"),
            "(NIL 2)"
        );
    }

    /// Any atom is a tag — the interpreter's rule — integers included.
    #[test]
    fn an_integer_is_a_tag() {
        assert_eq!(
            compile_and_run("(let ((n 0)) (tagbody 10 (setq n (+ n 1)) (if (< n 3) (go 10))) n)"),
            "3"
        );
    }

    /// The case only B can do: a `go` from inside a closure, made on the
    /// first trip and called on later ones. The marker is the same object on
    /// every trip, so the closure's lookup still finds the record.
    #[test]
    fn a_closure_made_on_an_earlier_trip_can_still_go() {
        assert_eq!(
            compile_and_run(
                "(let ((f nil) (n 0)) \
                   (tagbody top \
                     (setq n (+ n 1)) \
                     (if (= n 1) (setq f (lambda () (go top)))) \
                     (if (< n 3) (funcall f))) \
                   n)"
            ),
            "3"
        );
    }

    #[test]
    fn a_go_through_an_unwind_protect_runs_the_cleanup() {
        assert_eq!(
            compile_and_run(
                "(let ((log nil)) \
                   (tagbody top \
                     (unwind-protect (if (null log) (go top)) (setq log (cons 'x log)))) \
                   log)"
            ),
            "(X X)"
        );
    }

    /// A `go` out of a `block` crosses its record and must use the unwinder;
    /// and the block's scope must be gone afterwards, which is the `block`
    /// bug of the section above seen from another side.
    #[test]
    fn a_go_out_of_a_block_leaves_no_scope_behind() {
        assert_eq!(
            compile_and_run(
                "(let ((n 0)) (tagbody top (setq n (+ n 1)) (block b (if (< n 3) (go top)))) n)"
            ),
            "3"
        );
    }

    /// The two things `JMP-TAG` undoes by itself: a scope bound since the
    /// tagbody began, and values left on the operand stack by a call that
    /// was in the middle of collecting its arguments.
    #[test]
    fn a_local_go_undoes_scopes_and_stack_by_itself() {
        assert_eq!(
            compile_and_run(
                "(let ((x 5)) (tagbody a (let ((y 1)) (if (= x 5) (progn (setq x 6) (go a))))) x)"
            ),
            "6"
        );
        assert_eq!(
            compile_and_run(
                "(let ((n 0)) (tagbody top (setq n (+ n 1)) (list 1 2 (if (< n 5) (go top) 3))) n)"
            ),
            "5"
        );
    }

    #[test]
    fn an_inner_tagbody_can_go_to_an_outer_one() {
        assert_eq!(
            compile_and_run(
                "(let ((n 0)) \
                   (tagbody outer (setq n (+ n 1)) (tagbody inner (if (< n 3) (go outer)))) \
                   n)"
            ),
            "3"
        );
    }

    /// Going to a tagbody that has finished is an error, not a jump into
    /// nothing: its record has gone, and the unwinder says so.
    #[test]
    fn going_to_a_finished_tagbody_is_an_error() {
        let out = compile_and_run(
            "(let ((f nil)) (tagbody (setq f (lambda () (go top))) top) (funcall f))",
        );
        assert!(out.contains("no exit named"), "{out}");
    }

    #[test]
    fn a_go_to_a_tag_that_is_not_there_is_a_compile_error() {
        let out = compile_and_run("(tagbody top (go elsewhere))");
        assert!(out.contains("go names a tag that is not here"), "{out}");
    }

    /// C's choice. Nothing between the `go` and its tagbody but a `let` and
    /// a call's arguments: one `JMP-TAG`, no `GO`.
    #[test]
    fn a_local_go_is_one_jmp_tag() {
        let out = compile_instructions("(lambda (n) (tagbody top (let ((x n)) (f x (go top)))))");
        assert!(out.contains("(JMP-TAG"), "{out}");
        assert!(!out.contains("(GO "), "{out}");
    }

    /// And where it must not: a record between — a `block`, an
    /// `unwind-protect`, an inner tagbody — means `GO` and the unwinder.
    #[test]
    fn a_go_across_a_record_is_an_unwind() {
        for source in [
            "(lambda (n) (tagbody top (block b (go top))))",
            "(lambda (n) (tagbody top (unwind-protect (go top) (f n))))",
            "(lambda (n) (tagbody outer (tagbody inner (go outer))))",
        ] {
            let out = compile_instructions(source);
            assert!(out.contains("(GO "), "{source}: {out}");
        }
        // The inner tagbody's own tag is local to it.
        let out = compile_instructions("(lambda (n) (tagbody outer (tagbody inner (go inner))))");
        assert!(out.contains("(JMP-TAG"), "{out}");
    }

    /// From inside a closure it is always `GO`: the closure may be running
    /// several frames deeper, where no jump can reach. The closure is a
    /// separate function, so this looks at the disassembly of both.
    #[test]
    fn a_go_from_a_closure_is_an_unwind() {
        let out =
            compile_and_run("(%disassemble (lambda (n) (tagbody top (f (lambda () (go top))))))");
        assert!(out.contains("GO 1 0"), "{out}");
        assert!(!out.contains("JMP-TAG"), "{out}");
    }

    // ---- #38: what the fourth leg found ---------------------------------

    /// Every pair, not just the first. Found by running `control.lisp`
    /// compiled for the first time: `(setq a 1 b 2)` returned 1 and left `b`
    /// alone.
    #[test]
    fn setq_assigns_every_pair() {
        assert_eq!(
            compile_and_run("(let ((a 0) (b 0)) (list (setq a 1 b 2) a b))"),
            "(2 1 2)"
        );
        assert_eq!(compile_and_run("(setq)"), "NIL");
    }

    /// What the interpreter refuses as malformed, the compiler refuses too —
    /// at compile time, which in a compiled world is when the error belongs.
    #[test]
    fn a_malformed_special_form_is_a_compile_error() {
        for source in [
            "(let)",
            "(let 1)",
            "(let ((1 2)) 3)",
            "(block)",
            "(catch)",
            "(flet ((f)) 1)",
            "(quote)",
            "(quote 1 2)",
            "(if 1)",
            "(setq a)",
            "(return-from)",
            "(function 1)",
        ] {
            let out = compile_and_run(source);
            assert!(out.starts_with("; compiling:"), "{source}: {out}");
            assert!(out.contains("bad "), "{source}: {out}");
        }
    }

    /// The instruction list for a form, as the compiler produces it.
    fn compile_instructions(source: &str) -> String {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        install_compiler(&mut heap).expect("the compiler loads");
        // The body of the lambda, not the CLOSURE that wraps it.
        let form = format!("(%compile-body (cddr '{source}) (list (list ':variable 'n)) t)");
        if let Err(e) = drive(&mut heap, &form) {
            return e;
        }
        heap.mutate(|mc| {
            let v = mc.register(Register::Value);
            print(mc, v)
        })
    }

    /// `block` and `return-from`, which the prelude needs because `defun`
    /// emits one whenever a body mentions `return-from`.
    #[test]
    fn a_block_can_be_left_early() {
        assert_eq!(compile_and_run("(block out (return-from out 42) 99)"), "42");
        assert_eq!(compile_and_run("(block out 7)"), "7");
        assert_eq!(
            compile_and_run(
                "((lambda (n) (block done (if (= n 0) (return-from done 'zero)) 'other)) 0)"
            ),
            "ZERO"
        );
    }

    /// A `return-from` from inside a closure that has escaped, which is why
    /// the marker goes in the environment rather than at a known stack depth.
    #[test]
    fn return_from_works_from_inside_a_closure() {
        assert_eq!(
            compile_and_run("(block out (funcall (lambda () (return-from out 'left))) 'stayed)"),
            "LEFT"
        );
    }

    /// Tail position is cancelled inside a block, because the record has to
    /// outlive the call. If it were not, the verifier would refuse the
    /// function — so this test passing at all is the rule being kept.
    #[test]
    fn a_call_inside_a_block_is_not_a_tail_call() {
        let out = compile_instructions("(lambda (n) (block b (f n)))");
        assert!(out.contains("(CALL"), "{out}");
        assert!(!out.contains("TCALL"), "{out}");
    }

    #[test]
    fn function_compiles() {
        assert_eq!(compile_and_run("(funcall #'+ 1 2)"), "3");
        assert_eq!(compile_and_run("(funcall (lambda (x) (* x 2)) 21)"), "42");
        assert_eq!(compile_and_run("(let ((f #'-)) (funcall f 10 4))"), "6");
    }

    /// `funcall` and `apply` are primitives whose job is to call something
    /// else, which a primitive cannot do — so the machine intercepts them,
    /// exactly as the interpreter does.
    #[test]
    fn apply_spreads_its_last_argument() {
        assert_eq!(compile_and_run("(apply #'+ '(1 2 3))"), "6");
        assert_eq!(compile_and_run("(apply #'list 1 2 '(3 4))"), "(1 2 3 4)");
    }

    #[test]
    fn values_and_multiple_value_call_compile() {
        assert_eq!(compile_and_run("(values 1 2)"), "1");
        assert_eq!(
            compile_and_run("(multiple-value-call #'list (values 1 2) (values 3 4))"),
            "(1 2 3 4)"
        );
        assert_eq!(
            compile_and_run("(multiple-value-bind (a b) (values 1 2) (list a b))"),
            "(1 2)"
        );
    }

    /// The interpreter refuses a compiled closure, which is the half of #25's
    /// rule that was missing. Accepting one would not have failed — a
    /// `Function` self-evaluates, so the call would have returned the
    /// function object instead of calling it.
    #[test]
    fn the_interpreter_refuses_a_compiled_closure() {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        install_compiler(&mut heap).expect("the compiler loads");
        // Compile a function and put it in a symbol's function cell, then
        // call it from the interpreter.
        drive(&mut heap, "(compile-form '(lambda (x) x))").expect("compiles");
        let out = heap.mutate(|mc| {
            let compiled = mc.register(Register::Value);
            // `compile-form` makes a function of no arguments whose body is
            // the lambda; running it would give the closure. For this test
            // the function object itself is enough: wrap it as a closure and
            // install it.
            let name = mc.intern("COMPILED-THING");
            let nil = quokka_vm::value::Value::NIL;
            let c = quokka_vm::closure::make(mc, nil, compiled, nil);
            mc.set_slot(name, quokka_vm::heap::symbol::FUNCTION, c);
            let chars = Reader::chars_of("(compiled-thing)");
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).expect("reads").expect("a form");
            match eval_now(mc, form, BOOT_BUDGET) {
                Ok(v) => format!("returned {}", print(mc, v)),
                Err(e) => format!("{e}"),
            }
        });
        assert!(out.contains("compiled closure"), "{out}");
    }

    /// The disassembly of a compiled function, as `--show-code` prints it.
    ///
    /// Pinned because it is a tool for reading the compiler's output, and a
    /// tool that quietly stops annotating constants is worse than none: the
    /// numbers would still look right.
    #[test]
    fn a_disassembly_names_what_the_operands_mean() {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        install_compiler(&mut heap).expect("the compiler loads");
        drive(&mut heap, "(compile-form '((lambda (x) (+ x 1)) 41))").expect("compiles");
        let text = heap.mutate(|mc| {
            let f = mc.register(Register::Value);
            quokka_vm::code::disassemble_function(mc, f)
        });
        assert_eq!(
            text,
            concat!(
                "COMPILED-FORM NIL  ; stack 2\n",
                "     0  CLOSURE 0  ; #<compiled LAMBDA>\n",
                "     3  FIX 41\n",
                "     6  TCALL 1\n",
                "     8  RET\n",
                "\n",
                "    LAMBDA (X)  ; stack 3\n",
                "         0  GFREF 0  ; +\n",
                "         3  LREF 0 0\n",
                "         6  FIX 1\n",
                "         9  TCALL 2\n",
                "        11  RET\n",
            ),
            "\n{text}"
        );
    }

    /// A jump prints where it lands, and a primitive prints its name.
    #[test]
    fn a_disassembly_resolves_jumps_and_primitives() {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        install_compiler(&mut heap).expect("the compiler loads");
        drive(&mut heap, "(compile-form '(if t 1 2))").expect("compiles");
        let text = heap.mutate(|mc| {
            let f = mc.register(Register::Value);
            quokka_vm::code::disassemble_function(mc, f)
        });
        assert!(text.contains("JMP-NIL"), "{text}");
        assert!(text.contains("; -> "), "{text}");
    }

    /// Lambda lists with `&optional`, `&key` and `&rest`, which the compiler
    /// does not implement — `%rewrite-lambda-list` hands it a simple list and
    /// a `let*`, the same rewrite the interpreter gets.
    #[test]
    fn optional_parameters_compile() {
        assert_eq!(
            compile_and_run("((lambda (a &optional b) (list a b)) 1 2)"),
            "(1 2)"
        );
        assert_eq!(
            compile_and_run("((lambda (a &optional b) (list a b)) 1)"),
            "(1 NIL)"
        );
        assert_eq!(
            compile_and_run("((lambda (a &optional (b 9)) (list a b)) 1)"),
            "(1 9)"
        );
    }

    #[test]
    fn keyword_parameters_compile() {
        assert_eq!(
            compile_and_run("((lambda (a &key b) (list a b)) 1 :b 2)"),
            "(1 2)"
        );
        assert_eq!(
            compile_and_run("((lambda (a &key b) (list a b)) 1)"),
            "(1 NIL)"
        );
        assert_eq!(compile_and_run("((lambda (&key (b 7)) b))"), "7");
    }

    #[test]
    fn rest_parameters_compile() {
        assert_eq!(compile_and_run("((lambda (a &rest r) r) 1 2 3)"), "(2 3)");
        assert_eq!(compile_and_run("((lambda (&rest r) r))"), "NIL");
    }

    /// A supplied-p variable, which is the whole reason an unsupplied
    /// argument gets a marker rather than nil (chapter 18).
    #[test]
    fn supplied_p_compiles() {
        assert_eq!(
            compile_and_run("((lambda (&optional (b nil given)) given) 1)"),
            "T"
        );
        assert_eq!(
            compile_and_run("((lambda (&optional (b nil given)) given))"),
            "NIL"
        );
    }

    /// `let*`, which the compiler needed before it could do `&optional` —
    /// the rewrite emits one.
    #[test]
    fn let_star_binds_one_at_a_time() {
        assert_eq!(
            compile_and_run("(let* ((a 1) (b (+ a 1))) (list a b))"),
            "(1 2)"
        );
        assert_eq!(compile_and_run("(let* () 7)"), "7");
    }

    /// A prelude function — Lisp, interpreted, called from compiled code.
    ///
    /// Which is a thing #25 said must not happen in a *built image*: there,
    /// everything is compiled and the interpreter is cold. During the
    /// bootstrap it is exactly what happens, because the prelude has not been
    /// compiled yet — and `CALL` refuses it, by design.
    #[test]
    fn calling_an_interpreted_closure_is_refused() {
        let out = compile_and_run("(length (quote (1 2 3)))");
        assert!(out.contains("interpreted closure"), "{out}");
    }

    /// The compiler's output is verified like anything else, so a compiler
    /// bug that produced a malformed function would be caught at assembly
    /// rather than becoming a wrong answer somewhere later.
    ///
    /// `(FIX 1) (FIX 2) (RET)` is *not* an example, which is worth saying
    /// because it was this test's first guess. Leaving a value under the one
    /// being returned is harmless: `RET` restores the stack pointer to the
    /// frame base, so the leftover is dropped. The JVM's verifier does not
    /// require an empty stack at return either. What is refused is a `RET`
    /// with nothing to return at all.
    #[test]
    fn the_compilers_output_is_verified() {
        let mut heap = Heap::new();
        install(&mut heap).expect("the prelude boots");
        let chars = Reader::chars_of("(%assemble '((POP) (RET)) #() nil 'bad)");
        let out = heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).expect("reads").expect("a form");
            match eval_now(mc, form, BOOT_BUDGET) {
                Ok(_) => String::from("assembled, which it should not have"),
                Err(e) => format!("{e}"),
            }
        });
        // And it says why, which until #97 it did not.
        assert!(
            out.contains("malformed") && out.contains("Underflow"),
            "{out}"
        );
    }
}

/// **The file compiler's gate.** Every form of the real prelude compiles,
/// and the verifier accepts every function that comes out.
///
/// Six hundred lines of Lisp nobody wrote to flatter the compiler: `defstruct`
/// with its generated accessors, `handler-case`, the sequence functions,
/// `setf` and its expanders. If the compiler can do this it can do the
/// compiler, which is what phase 1's image needs it to do.
#[test]
fn the_whole_prelude_compiles() {
    let mut heap = Heap::new();
    install(&mut heap).expect("the prelude boots");
    install_compiler(&mut heap).expect("the compiler loads");
    let tally = compile_source(&mut heap, PRELUDE).expect("the prelude compiles");
    // `Ok` is the gate: `compile_source` stops at the first form that fails
    // to compile or fails to verify, so reaching the end means all of them
    // did. The counts are evidence of scale rather than a second check, and
    // a floor rather than an equality so that adding a prelude function is
    // not a test failure.
    assert!(tally.compiled >= 40, "{tally:?}");
    assert!(
        tally.interpreted >= 10,
        "the prelude defines macros, and those are evaluated rather than \
         compiled — see compile_source: {tally:?}"
    );
}

/// **The one that matters for phase 1.** The compiler compiles its own
/// source.
///
/// Not self-hosting yet — the output is thrown away, so this is the compiler
/// *interpreted* compiling the compiler, not the compiler running itself.
/// But it is the precondition: an image that holds a compiled compiler can
/// only be built if the compiler can compile the text of the compiler, and
/// that is a harder test than the prelude because `compile.lisp` is the
/// densest tier-0 Lisp in the repository.
#[test]
fn the_compiler_compiles_itself() {
    let mut heap = Heap::new();
    install(&mut heap).expect("the prelude boots");
    install_compiler(&mut heap).expect("the compiler loads");
    for (file, source) in COMPILER {
        let tally = compile_source(&mut heap, source)
            .unwrap_or_else(|e| panic!("{file} does not compile: {e}"));
        assert!(tally.compiled > 0, "{file}: {tally:?}");
    }
}

/// **#33's build.** The prelude and the compiler, compiled and installed in
/// two passes, with nothing interpreted left in the heap afterwards.
#[test]
fn a_world_can_be_built_with_nothing_interpreted_in_it() {
    let mut heap = Heap::new();
    let built = build_image(&mut heap).unwrap_or_else(|e| panic!("{e}"));
    assert!(built.forms > 90, "{built:?}");
    assert_eq!(heap.interpreted_code().count, 0);
}

/// The compiled world, built once and kept as image bytes; each test loads
/// its own copy. Loading is cheap and building is not — and going through an
/// image every time means every test below also tests saving and loading.
fn compiled_image() -> &'static [u8] {
    static IMAGE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    IMAGE.get_or_init(|| {
        let mut heap = Heap::new();
        build_image(&mut heap).unwrap_or_else(|e| panic!("{e}"));
        quokka_vm::image::save(&mut heap)
    })
}

fn compiled_world() -> Heap {
    quokka_vm::image::load(compiled_image(), 1 << 22).expect("the compiled image loads")
}

/// Run `source` in a compiled world — compiled by the world's own compiler,
/// on the VM — and print what each form gave.
fn in_compiled_world(heap: &mut Heap, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut host = quokka_vm::host::NullHost::default();
    crate::boot::compile_and_run_forms(heap, source, 10_000_000, 2_000, &mut host, |mc, r| {
        out.push(match r {
            Ok(v) => print(mc, v),
            Err(e) => format!("; {e}"),
        });
        true
    })
    .expect("reads");
    out
}

/// **The phase 1 gate, in the compiled world.** An image built from compiled
/// code is saved, loaded into a fresh heap, and used — and nothing in that
/// heap is interpreted, nor does the interpreter run: every form goes through
/// the image's compiled `compile-form` and the VM.
#[test]
fn a_compiled_world_loads_and_runs() {
    let mut heap = compiled_world();
    assert_eq!(heap.interpreted_code().count, 0);
    assert_eq!(
        in_compiled_world(
            &mut heap,
            "(length '(1 2 3)) \
             (reverse '(a b c)) \
             (defun sq (x) (* x x)) (sq 12) \
             (when (> 3 2) 'yes) \
             (handler-case (car 5) (type-error () 'caught)) \
             (let ((n 0)) (tagbody top (setq n (+ n 1)) (if (< n 10) (go top))) n)"
        ),
        ["3", "(C B A)", "SQ", "144", "YES", "CAUGHT", "10"]
    );
}

/// What a compiled compiler can do that an interpreted one could not: call
/// expanders it compiled itself, for a user's `defmacro` and for `macrolet`.
#[test]
fn a_compiled_world_compiles_its_own_macros() {
    let mut heap = compiled_world();
    assert_eq!(
        in_compiled_world(
            &mut heap,
            "(defmacro twice (f) (list 'progn f f)) \
             (let ((n 0)) (twice (setq n (+ n 1))) n) \
             (macrolet ((cube (x) (list '* x x x))) (cube 3)) \
             (defstruct point x y) \
             (let ((p (make-point :x 1 :y 2))) (setf (point-x p) 42) (point-x p))"
        ),
        ["TWICE", "2", "27", "POINT", "42"]
    );
}

/// The world stays compiled as it is worked in: defining a function and a
/// macro leaves no interpreted code behind, because the image's compiler
/// compiled both.
#[test]
fn working_in_a_compiled_world_keeps_it_compiled() {
    let mut heap = compiled_world();
    in_compiled_world(
        &mut heap,
        "(defun add1 (x) (+ x 1)) (defmacro swap-args (f a b) (list f b a))",
    );
    heap.collect();
    let left = heap.interpreted_code();
    assert_eq!(left.count, 0, "{left:?}");
}

/// Chapter 28 ended on two things that did not work, and said they would
/// stop being errors once the prelude and the compiler were compiled. They
/// have: compiled code calls `length`, and compiled code calls the compiler.
#[test]
fn the_two_refusals_of_chapter_28_are_gone() {
    let mut heap = compiled_world();
    let out = in_compiled_world(&mut heap, "(length '(1 2 3)) (compile-form '(+ 1 2))");
    assert_eq!(out[0], "3");
    assert_eq!(out[1], "#<compiled COMPILED-FORM>");
}

/// And the other side of #25's rule: the interpreter, handed a compiled
/// world, cannot quietly run in it. It is refused at the first compiled
/// function it meets — which in this world is all of them.
#[test]
fn the_interpreter_cannot_run_in_a_compiled_world() {
    let mut heap = compiled_world();
    let e = heap.mutate(|mc| {
        let chars = Reader::chars_of("(length '(1 2))");
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).expect("reads").expect("a form");
        crate::eval::eval_now(mc, form, 100_000).map(|_| ())
    });
    let text = format!("{}", e.expect_err("refused"));
    assert!(text.contains("compiled closure"), "{text}");
}

/// "Resumes where it left off", in the compiled world: a compiled loop is
/// stopped part-way, saved in an image, and finished in another heap.
#[test]
fn a_compiled_computation_resumes_from_an_image() {
    use quokka_vm::heap::Register;
    use quokka_vm::slice::{run_slice, Status};

    let mut heap = compiled_world();
    let mut host = quokka_vm::host::NullHost::default();
    heap.mutate(|mc| {
        let chars = Reader::chars_of(
            "(let ((n 0)) (tagbody top (setq n (+ n 1)) (if (< n 500) (go top))) n)",
        );
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).expect("reads").expect("a form");
        mc.set_register(Register::Value, form);
    });
    quokka_vm::vm::call_global(&mut heap, "COMPILE-FORM", 10_000_000, 2_000, &mut host)
        .expect("compiles");
    heap.mutate(|mc| {
        let f = mc.register(Register::Value);
        quokka_vm::vm::Vm::start(mc, f, quokka_vm::value::Value::NIL);
    });
    let status = heap.mutate(|mc| run_slice(&mut quokka_vm::vm::Vm::resume(mc), 300));
    assert_eq!(
        status,
        Status::Yielded,
        "300 steps should not finish 500 trips"
    );

    let bytes = quokka_vm::image::save(&mut heap);
    drop(heap);
    let mut reloaded = quokka_vm::image::load(&bytes, 1 << 22).expect("loads");
    let status = reloaded.mutate(|mc| run_slice(&mut quokka_vm::vm::Vm::resume(mc), 1_000_000));
    assert!(matches!(status, Status::Done { .. }), "{status:?}");
    let n = reloaded.mutate(|mc| print(mc, mc.register(Register::Value)));
    assert_eq!(n, "500");
}

/// A computation abandoned with a special variable bound — here, by running
/// out of budget in a loop that never ends — must not leave the binding in
/// place. Starting the next computation undoes it (#40).
#[test]
fn an_abandoned_computation_leaves_no_binding_behind() {
    use quokka_vm::heap::Register;
    use quokka_vm::slice::{run_slice, Status};

    let mut heap = compiled_world();
    in_compiled_world(&mut heap, "(defvar *x* 0)");
    let mut host = quokka_vm::host::NullHost::default();
    heap.mutate(|mc| {
        let chars = Reader::chars_of("(let ((*x* 1)) (tagbody top (go top)))");
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).expect("reads").expect("a form");
        mc.set_register(Register::Value, form);
    });
    quokka_vm::vm::call_global(&mut heap, "COMPILE-FORM", 10_000_000, 2_000, &mut host)
        .expect("compiles");
    heap.mutate(|mc| {
        let f = mc.register(Register::Value);
        quokka_vm::vm::Vm::start(mc, f, quokka_vm::value::Value::NIL);
    });
    let status = heap.mutate(|mc| run_slice(&mut quokka_vm::vm::Vm::resume(mc), 500));
    assert_eq!(status, Status::Yielded, "the loop never ends");
    assert_eq!(in_compiled_world(&mut heap, "*x*"), ["0"]);
}

/// And one that halts on an error nothing caught undoes its bindings as it
/// stops, so the very next form sees the old value.
#[test]
fn an_uncaught_error_undoes_its_bindings() {
    let mut heap = compiled_world();
    let out = in_compiled_world(&mut heap, "(defvar *y* 0) (let ((*y* 9)) (car 1)) *y*");
    assert!(out[1].starts_with("; "), "{out:?}");
    assert_eq!(out[2], "0");
}

/// Adoption (#43) leaves no symbol behind: after a build, every symbol in the
/// Rust system table has a package or is a keyword — none has the home `0`
/// that means "not yet adopted". Checked on the heap directly, before
/// anything is read, because reading a form would itself make new symbols.
#[test]
fn after_a_build_every_system_symbol_has_a_package() {
    use quokka_vm::heap::{home, symbol};
    let heap = compiled_world();
    let mut heap = heap;
    let (unadopted, total) = heap.mutate(|mc| {
        let unadopted_home = quokka_vm::value::Value::fixnum(home::CURRENT).expect("small");
        let mut cursor = mc.system_symbols();
        let (mut unadopted, mut total) = (Vec::new(), 0usize);
        while cursor.is_cons() {
            let s = mc.car(cursor);
            total += 1;
            if mc.slot(s, symbol::HOME) == unadopted_home {
                unadopted.push(mc.string_to_rust(mc.slot(s, symbol::NAME)));
            }
            cursor = mc.cdr(cursor);
        }
        (unadopted, total)
    });
    assert!(
        total > 1_000,
        "{total} symbols, which is fewer than COMMON-LISP alone"
    );
    assert!(unadopted.is_empty(), "never adopted: {unadopted:?}");
}

/// **No input may stop the process** (#90, #91, #92). Each of these forms is
/// ordinary Common Lisp, and each once ended the program that ran it: the
/// first three overflowed the Rust stack in the printer that writes the error
/// message, the next two panicked or asked the operating system for 800 GB,
/// and the last three walked a circular list in one step, where the step
/// budget cannot interrupt.
///
/// What they give now matters less than that they give something.
#[test]
fn hostile_forms_end_in_an_error_and_not_the_process() {
    let mut heap = compiled_world();
    let said = in_compiled_world(
        &mut heap,
        "(let ((x (list 1))) (setf (car x) x) (funcall x)) \
         (let ((v (vector 1))) (setf (aref v 0) v) (funcall v)) \
         (let ((x nil)) (dotimes (i 200000) (setq x (list x))) (funcall x)) \
         (make-array most-positive-fixnum) \
         (make-string 100000000000) \
         (let ((x (list 1 2))) (setf (cddr x) x) (apply #'list x)) \
         (let ((x (list 1 2))) (setf (cddr x) x) (values-list x)) \
         (let ((x (list 1 2))) (setf (cddr x) x) (append x (list 3))) \
         (+ 1 2)",
    );
    assert_eq!(said.len(), 9);
    for (i, line) in said[..8].iter().enumerate() {
        assert!(line.starts_with("; "), "form {i} gave {line}");
    }
    // Nesting past the printer's depth is written as `#`, as `*print-level*`
    // writes it.
    assert!(said[0].contains("(#)"), "{}", said[0]);
    assert!(said[3].contains("out of heap"), "{}", said[3]);
    assert!(said[5].contains("circular"), "{}", said[5]);
    // And the world is still there.
    assert_eq!(said[8], "3");
}

/// The other half: the bounds refuse nothing a program may fairly ask for.
#[test]
fn the_bounds_leave_ordinary_programs_alone() {
    let mut heap = compiled_world();
    assert_eq!(
        in_compiled_world(
            &mut heap,
            "(apply #'+ 1 2 (list 3 4)) \
             (length (apply #'list (make-list 100000))) \
             (append (list 1 2) (list 3) 4) \
             (multiple-value-list (values-list (list 1 2 3))) \
             (length (make-array 100000)) \
             (length (make-array 1000 :element-type 'bit)) \
             (handler-case (let ((x (list 1))) (setf (cdr x) x) (apply #'list x)) \
               (type-error () 'caught))"
        ),
        [
            "10",
            "100000",
            "(1 2 3 . 4)",
            "(1 2 3)",
            "100000",
            "1000",
            "CAUGHT"
        ]
    );
}

/// **No input may panic the machine** (#88): every primitive, called with
/// every combination of a palette of arguments, gives a value or an error.
///
/// This is *fuzzing* in its plainest form — feed a program inputs nobody
/// would write and see whether it falls over — and exhaustive rather than
/// random, so a failure names itself and reproduces. The palette holds one of
/// each kind of object and the numbers at the edges; a primitive that trusts
/// an argument's type, sign or size meets the one that breaks it.
///
/// A panic is caught where the unwinding can be (not on wasm32, where the
/// test simply stops) so that one run lists every primitive that failed.
/// Each gets a fresh world: some of these calls unbind or redefine what the
/// next would need. And the heap must verify afterwards — a primitive that
/// returns having corrupted the heap is the worse fault.
///
/// Not run on wasm32: the primitives are the same Rust there, a panic
/// cannot be caught there, and with one thread it was a quarter of that
/// job's time in CI.
#[test]
#[cfg_attr(target_arch = "wasm32", ignore)]
fn no_primitive_panics_on_any_argument() {
    let mut failed = Vec::new();
    for name in quokka_vm::primitives::names() {
        let source = format!(
            "(let* ((s (find-symbol \"{name}\" \"QUOKKA\"))
                    (f (and s (fboundp s) (symbol-function s)))
                    (vals (list nil t 0 1 -1 7 most-positive-fixnum most-negative-fixnum
                                1.5d0 1/2 (expt 2 70) \"str\" #\\a (make-symbol \"S\") :key
                                (list 1 2) (cons 1 2) (vector 1 2) (make-hash-table) #'car))
                    (few (list nil 0 -1 \"s\" (list 1) most-positive-fixnum)))
               (when f
                 (handler-case (funcall f) (error () nil))
                 (dolist (a vals) (handler-case (funcall f a) (error () nil)))
                 (dolist (a vals)
                   (dolist (b vals) (handler-case (funcall f a b) (error () nil))))
                 (dolist (a few)
                   (dolist (b few)
                     (dolist (c few) (handler-case (funcall f a b c) (error () nil))))))
               :done)"
        );
        let outcome = std::panic::catch_unwind(|| {
            let mut heap = compiled_world();
            let mut host = quokka_vm::host::NullHost::default();
            let _ = quokka_vm::vm::eval_source(
                &mut heap,
                &source,
                3_000_000,
                200_000,
                &mut host,
                |_| true,
            );
            heap.verify().map_err(|e| e.to_string())
        });
        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(why)) => failed.push(format!("{name}: left the heap inconsistent: {why}")),
            Err(_) => failed.push(format!("{name}: panicked")),
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Run `source` through a world's own reader, `eval` and printer, as a host
/// does, and give what each form printed.
fn said_by_the_world(heap: &mut Heap, source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut host = quokka_vm::host::NullHost::default();
    quokka_vm::vm::eval_source(heap, source, 10_000_000, 2_000, &mut host, |outcome| {
        out.push(match outcome {
            quokka_vm::vm::Outcome::Value(text) => text,
            quokka_vm::vm::Outcome::Error { message, .. } => format!("; {message}"),
            quokka_vm::vm::Outcome::Failed(e) => format!("; failed: {e}"),
        });
        true
    })
    .expect("the world reads");
    out
}

/// **An unwind knows what kind it is** (#96). A tag may be any object, a
/// condition included, and with one register for "where this is going" a
/// handler took a `throw` and a `catch` took an error. Both forms are from
/// the review; SBCL gives 1 for the first and an unhandled error for the
/// second.
#[test]
fn a_throw_is_not_a_signal_and_a_signal_is_not_a_throw() {
    let mut heap = compiled_world();
    let said = said_by_the_world(
        &mut heap,
        "(let ((c (handler-case (error \"x\") (error (e) e))))
           (catch c (handler-case (throw c 1) (error (e) :wrong))))
         (let ((c (handler-case (error \"x\") (error (e) e))))
           (list (catch c (error c)) :survived))
         (catch 'a (unwind-protect (throw 'a 1) (+ 1 1)))",
    );
    assert_eq!(said[0], "1");
    assert!(said[1].starts_with("; "), "{}", said[1]);
    assert_eq!(said[2], "1", "a throw still passes through a cleanup");
}

/// **The program's mistakes are the program's to catch** (#97): a malformed
/// lambda list is a `program-error`, and leaving a block or tagbody that has
/// already been left is a `control-error` signalled where it is tried, with
/// the block's name. Both used to stop the machine as if it were at fault.
#[test]
fn a_programs_own_mistakes_are_conditions() {
    let mut heap = compiled_world();
    let said = said_by_the_world(
        &mut heap,
        "(handler-case (eval '(lambda (5) 1)) (program-error () :caught))
         (handler-case (eval '(defmacro m (&whole) 1)) (program-error () :caught))
         (handler-case (funcall (block b (lambda () (return-from b 1))))
           (control-error () :control-error))
         (let ((f nil))
           (tagbody (setq f (lambda () (go out))) out)
           (handler-case (funcall f) (control-error () :control-error)))
         (funcall (block gone (lambda () (return-from gone 1))))",
    );
    assert_eq!(
        said[..4],
        [":CAUGHT", ":CAUGHT", ":CONTROL-ERROR", ":CONTROL-ERROR"]
    );
    assert!(said[4].contains("GONE"), "{}", said[4]);
}

/// **No Rust recursion whose depth Lisp decides** (#93), and **what the
/// assembler vouches for cannot change afterwards** (#94). Each of these
/// overflowed the stack, panicked, or ran code the verifier had passed on
/// different constants; now each is an error and the world goes on.
#[test]
fn the_machine_refuses_what_it_cannot_do_safely() {
    let mut heap = compiled_world();
    let said = said_by_the_world(
        &mut heap,
        "(let ((x (list 1))) (setf (car x) x) (quokka::%rewrite-macro-lambda-list x nil))
         (progn (setf (symbol-function 'quokka::%generic-+) #'+) (+ most-positive-fixnum 1))
         (funcall (quokka::%assemble
                   '((quokka::block 0 l) (quokka::nil) (quokka::nil) (quokka::nil)
                     (quokka::unmark) (quokka::pop) (quokka::pop) (quokka::ret)
                     (:label l) (quokka::ret))
                   (vector 'b) nil 'x))
         (let* ((k (vector (vector 'a)))
                (f (quokka::%assemble
                    '((quokka::nil) (quokka::bind 0) (quokka::nil) (quokka::ret)) k nil 'x)))
           (setf (aref k 0) (make-array 100000))
           (funcall f))
         (quokka::%assemble '((quokka::nil) (quokka::ret)) \"constants\" nil 'x)
         (quokka::%assemble '((quokka::nil) (quokka::ret)) (vector) '(&key (:a . 5)) 'x)
         (quokka::%hash-delete-entry 5 (make-hash-table))
         (+ 1 2)",
    );
    assert_eq!(said.len(), 8, "{said:?}");
    for (i, line) in said[..7].iter().enumerate() {
        // Form 3 is the one that now simply works: the function kept the
        // constants it was verified with.
        if i != 3 {
            assert!(line.starts_with("; "), "form {i} gave {line}");
        }
    }
    assert!(said[0].contains("nested too deeply"), "{}", said[0]);
    assert!(said[2].contains("MarksNotBalanced"), "{}", said[2]);
    assert_eq!(said[3], "NIL");
    assert_eq!(said[7], "3", "and the world is still there");
}

/// **Keys spread over the buckets whatever their low bits are** (#98).
/// 40,000 float keys took thirteen seconds where fixnums took a quarter of
/// one, because a double such as 2.0 has forty zero bits at the bottom and
/// the bucket was chosen by exactly those: every key in one chain, walked
/// inside a single step, where no budget counts it. So the test looks at
/// the chain itself, through the primitive the library searches with.
#[test]
fn float_and_character_keys_do_not_share_a_bucket() {
    let mut heap = compiled_world();
    let said = said_by_the_world(
        &mut heap,
        "(let ((h (make-hash-table)))
           (dotimes (i 20000) (setf (gethash (float i) h) i))
           (list (hash-table-count h) (gethash 1234.0 h)
                 (< (length (quokka::%hash-bucket 1234.0 h)) 32)))
         (let ((h (make-hash-table)))
           (dotimes (i 200) (setf (gethash (code-char i) h) i))
           (list (gethash #\\a h) (< (length (quokka::%hash-bucket #\\a h)) 32)))",
    );
    assert_eq!(said, ["(20000 1234 T)", "(97 T)"]);
}

/// **A compile error says where, and a function knows its name** (#110).
/// "return-from names a block that is not here" was true of some function
/// in the file; an arity error was about "a compiled function"; a typed
/// error from the library printed as `NIL`; and a form past one of the
/// bytecode's ceilings was refused by the assembler in the assembler's
/// words.
#[test]
fn errors_say_where_and_functions_say_who() {
    let mut heap = compiled_world();
    let args = vec!["1"; 300].join(" ");
    let source = format!(
        "(defun lost (x) (return-from nowhere x))
         (defun two (a b) (list a b))
         (two 1)
         (flet ((inner (x) x)) (inner))
         (funcall (lambda (x) x))
         (prin1-to-string #'two)
         (tagbody a a)
         (list {args})
         (length (apply #'list (quote ({args}))))
         (evenp 1.5d0)
         (handler-case (eval '(let ((t 1)) t)) (program-error () :still-a-program-error))"
    );
    let said = said_by_the_world(&mut heap, &source);
    assert_eq!(
        said[0],
        "; in DEFUN LOST: return-from names a block that is not here: NOWHERE"
    );
    assert_eq!(said[2], "; TWO wants 2 arguments and got 1");
    // A local function is named as SBCL names it (#130).
    assert_eq!(said[3], "; (FLET INNER) wants 1 arguments and got 0");
    assert_eq!(said[4], "; this function wants 1 arguments and got 0");
    assert_eq!(said[5], "\"#<function TWO>\"");
    assert!(
        said[6].contains("in TAGBODY") && said[6].contains("same tag twice"),
        "{}",
        said[6]
    );
    assert!(said[7].contains("more than 255 arguments"), "{}", said[7]);
    assert_eq!(said[8], "300", "apply is not bound by the ceiling");
    assert_eq!(said[9], "; EVENP wants an integer");
    assert_eq!(said[10], ":STILL-A-PROGRAM-ERROR");
}
