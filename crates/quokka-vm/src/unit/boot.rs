//! Tests for [`crate::boot`].
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

/// The compiler, loaded and used: a Lisp function compiles a Lisp form into
/// bytecode, and the VM runs it.
mod compiler {
    use super::*;
    use crate::heap::Register;
    use crate::vm::run_now;

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
    /// the time it runs.
    #[test]
    fn only_a_clause_body_is_in_tail_position() {
        let out = compile_instructions("(lambda (n) (handler-case (f n) (error () (g n))))");
        assert_eq!(out.matches("TCALL").count(), 1, "{out}");
        assert_eq!(out.matches("(CALL").count(), 1, "{out}");
        assert!(out.contains("HANDLER"), "{out}");
    }

    /// A clause that does not take everything is followed by the re-signal,
    /// which is a call to `error` — and in tail position too, so there are
    /// two. After a catch-all there is no re-signal, because it could never
    /// run; the test above is the other half.
    #[test]
    fn a_selective_handler_ends_in_a_re_signal() {
        let out = compile_instructions("(lambda (n) (handler-case (f n) (type-error () (g n))))");
        assert_eq!(out.matches("TCALL").count(), 2, "{out}");
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

    /// The guard. A special operator the compiler has no case for is a
    /// compile error that names it — not, as before, a call to a function
    /// named after it that compiled and failed only when run.
    #[test]
    fn an_operator_the_compiler_lacks_is_a_compile_error() {
        for (source, name) in [
            ("(tagbody top)", "TAGBODY"),
            ("(macrolet ((m () 1)) (m))", "MACROLET"),
        ] {
            let out = compile_and_run(source);
            assert!(out.starts_with("; compiling:"), "{out}");
            assert!(out.contains(&format!("special operator {name}")), "{out}");
        }
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
            let nil = crate::value::Value::NIL;
            let c = crate::eval::closure::make(mc, nil, compiled, nil);
            mc.set_slot(name, crate::heap::symbol::FUNCTION, c);
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
            crate::code::disassemble_function(mc, f)
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
            crate::code::disassemble_function(mc, f)
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
        assert!(out.contains("verifier refused"), "{out}");
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
