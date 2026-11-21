//! Tests for [`crate::eval`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

use super::*;
use crate::heap::Heap;
use crate::primitives;
use crate::printer::print;
use crate::reader::Reader;

const PLENTY: u64 = 100_000;

fn eval_str(source: &str) -> Result<String, EvalError> {
    let mut heap = Heap::new();
    let chars = Reader::chars_of(source);
    heap.mutate(|mc| {
        crate::boot::install_in(mc).expect("the prelude boots");
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).expect("reads").expect("a form");
        eval_now(mc, form, PLENTY).map(|v| print(mc, v))
    })
}

fn evals_to(source: &str, expected: &str) {
    assert_eq!(
        eval_str(source).as_deref(),
        Ok(expected),
        "evaluating {source}"
    );
}

#[test]
fn things_evaluate_to_themselves() {
    evals_to("42", "42");
    evals_to("-7", "-7");
    evals_to("\"text\"", "\"text\"");
    evals_to("#\\a", "#\\a");
    evals_to("nil", "NIL");
    evals_to("t", "T");
}

#[test]
fn arithmetic() {
    evals_to("(+ 1 2)", "3");
    evals_to("(+)", "0");
    evals_to("(* 6 7)", "42");
    evals_to("(*)", "1");
    evals_to("(- 10 3)", "7");
    evals_to("(- 5)", "-5");
    evals_to("(+ 1 2 3 4 5)", "15");
}

/// The worked example from the module documentation.
#[test]
fn nested_calls() {
    evals_to("(+ 1 (* 2 3))", "7");
    evals_to("(* (+ 1 1) (+ 2 2))", "8");
    evals_to("(+ (+ (+ 1 1) 1) 1)", "4");
}

#[test]
fn comparisons_chain() {
    evals_to("(= 1 1)", "T");
    evals_to("(= 1 2)", "NIL");
    evals_to("(< 1 2 3)", "T");
    evals_to("(< 1 3 2)", "NIL");
    evals_to("(> 3 2 1)", "T");
}

/// Quote's entire purpose is that its argument is *not* evaluated.
#[test]
fn quote_does_not_evaluate() {
    evals_to("'foo", "FOO");
    evals_to("(quote foo)", "FOO");
    evals_to("'(1 2 3)", "(1 2 3)");
    // Without quote this would be an undefined function.
    evals_to("'(nosuchfunction 1)", "(NOSUCHFUNCTION 1)");
}

/// `if` cannot be a function: a function's arguments are already evaluated
/// by the time it is called, so both branches would run. The untaken branch
/// here would error if it were evaluated, which is the test.
#[test]
fn if_evaluates_only_one_branch() {
    evals_to("(if t 1 2)", "1");
    evals_to("(if nil 1 2)", "2");
    evals_to("(if t 1 (undefined-function))", "1");
    evals_to("(if nil (undefined-function) 2)", "2");
    // Anything but nil is true.
    evals_to("(if 0 'yes 'no)", "YES");
    evals_to("(if '() 'yes 'no)", "NO");
    // A missing else-branch yields nil.
    evals_to("(if nil 1)", "NIL");
}

#[test]
fn list_operations() {
    evals_to("(cons 1 2)", "(1 . 2)");
    evals_to("(cons 1 nil)", "(1)");
    evals_to("(car '(1 2 3))", "1");
    evals_to("(cdr '(1 2 3))", "(2 3)");
    evals_to("(car nil)", "NIL");
    evals_to("(cdr nil)", "NIL");
    evals_to("(null nil)", "T");
    evals_to("(null '(1))", "NIL");
    evals_to("(consp '(1))", "T");
    evals_to("(symbolp 'x)", "T");
}

/// Interning is what makes this work: two reads of X give one object, so
/// `eq` can compare words rather than names.
#[test]
fn eq_on_symbols_is_identity() {
    evals_to("(eq 'x 'x)", "T");
    evals_to("(eq 'x 'y)", "NIL");
    evals_to("(eq 1 1)", "T");
}

#[test]
fn errors_are_values() {
    assert!(matches!(
        eval_str("undefined-variable"),
        Err(EvalError::UnboundVariable(_))
    ));
    assert!(matches!(eval_str("(1 2)"), Err(EvalError::NotAFunction(_))));
    assert!(matches!(
        eval_str("(car 1)"),
        Err(EvalError::WrongType { .. })
    ));
    assert!(matches!(
        eval_str("(cons 1)"),
        Err(EvalError::WrongArgumentCount { .. })
    ));
    assert!(matches!(
        eval_str("(quote)"),
        Err(EvalError::BadSyntax { .. })
    ));
    assert!(matches!(eval_str("(if)"), Err(EvalError::BadSyntax { .. })));
}

/// 20! is the largest factorial that fits in a fixnum, which is almost
/// certainly why the phase 0 gate asks for it: no bignums required.
#[test]
fn arithmetic_overflow_is_reported_not_wrapped() {
    evals_to("(* 2432902008176640000 1)", "2432902008176640000");
    assert_eq!(
        eval_str("(* 2432902008176640000 21)"),
        Err(EvalError::IntegerOverflow)
    );
}

// ---- closures, definitions, scope ------------------------------------

/// Evaluate several forms in one heap, so definitions persist.
fn eval_all(source: &str) -> Result<String, EvalError> {
    let mut heap = Heap::new();
    let chars = Reader::chars_of(source);
    heap.mutate(|mc| {
        crate::boot::install_in(mc).expect("the prelude boots");
        let mut reader = Reader::new(&chars);
        let mut last = String::new();
        while let Some(form) = reader.read(mc).expect("reads") {
            let value = eval_now(mc, form, PLENTY)?;
            last = print(mc, value);
        }
        Ok(last)
    })
}

/// Run a program, sampling the frame stack as it goes, and return the
/// deepest it ever reached along with the final value.
///
/// Claims about tail calls and loops are claims about *depth*, and a claim
/// about depth should be measured rather than inferred from the fact that
/// nothing crashed.
fn deepest_stack(source: &str, slice: u64) -> (usize, String) {
    use crate::slice::{run_slice, Status};

    let chars = Reader::chars_of(source);
    let mut heap = Heap::new();
    let deepest = heap.mutate(|mc| {
        crate::boot::install_in(mc).expect("the prelude boots");
        let mut reader = Reader::new(&chars);
        let mut deepest = 0usize;
        while let Some(form) = reader.read(mc).expect("reads") {
            let mut machine = Machine::start(mc, form, Value::NIL);
            while run_slice(&mut machine, slice) == Status::Yielded {
                deepest = deepest.max(machine.stack_depth());
            }
            assert!(machine.error.is_none(), "{:?}", machine.error);
        }
        deepest
    });
    let answer = heap.mutate(|mc| print(mc, mc.register(Register::Value)));
    (deepest, answer)
}

fn program_gives(source: &str, expected: &str) {
    assert_eq!(
        eval_all(source).as_deref(),
        Ok(expected),
        "running {source}"
    );
}

#[test]
fn lambda_makes_something_callable() {
    program_gives("((lambda (x) (* x x)) 7)", "49");
    program_gives("((lambda () 42))", "42");
    program_gives("((lambda (a b) (- a b)) 10 3)", "7");
}

#[test]
fn defun_defines_a_function() {
    program_gives("(defun square (x) (* x x)) (square 9)", "81");
    // defun returns the name.
    program_gives("(defun f (x) x)", "F");
}

/// A Lisp-2 keeps the variable and the function apart: defining one must
/// leave the other alone.
#[test]
fn a_name_can_be_a_function_and_a_variable_at_once() {
    program_gives(
        "(defun thing (x) (* x 2)) (setq thing 5) (thing thing)",
        "10",
    );
}

#[test]
fn setq_assigns() {
    program_gives("(setq x 1) x", "1");
    program_gives("(setq a 1 b 2) (+ a b)", "3");
    program_gives("(setq x 1) (setq x (+ x 1)) x", "2");
    // Assigning inside a function body touches the binding, not a global.
    program_gives(
        "(setq x 'outer) ((lambda (x) (setq x 'inner) x) 1)",
        "INNER",
    );
    program_gives(
        "(setq x 'outer) ((lambda (x) (setq x 'inner)) 1) x",
        "OUTER",
    );
}

#[test]
fn progn_yields_its_last_form() {
    program_gives("(progn 1 2 3)", "3");
    program_gives("(progn)", "NIL");
    program_gives("(progn (setq x 1) (setq x (+ x 1)) x)", "2");
}

/// The thing a closure is for: the captured scope outlives the call that
/// made it. `adder`'s `n` is gone by the time the returned lambda runs, and
/// the lambda still knows it.
#[test]
fn a_closure_captures_its_scope() {
    program_gives(
        "(defun adder (n) (lambda (x) (+ x n)))
         (setq add5 (adder 5))
         (setq add9 (adder 9))
         (+ (funcall add5 1) (funcall add9 1))",
        "16",
    );
}

#[test]
fn inner_scopes_shadow_outer_ones() {
    program_gives("(setq x 'global) ((lambda (x) x) 'local)", "LOCAL");
    program_gives("(setq x 'global) ((lambda (y) x) 'local)", "GLOBAL");
    program_gives("((lambda (x) ((lambda (x) x) 'inner)) 'outer)", "INNER");
    program_gives("((lambda (x) ((lambda (y) x) 'inner)) 'outer)", "OUTER");
}

#[test]
fn wrong_argument_count_to_a_closure_is_an_error() {
    assert!(matches!(
        eval_all("(defun f (x) x) (f 1 2)"),
        Err(EvalError::WrongArgumentCount { .. })
    ));
}

// ---- the phase 0 gate ------------------------------------------------

/// `(fact 20)` runs. The first half of the phase 0 gate.
///
/// 20! is 2,432,902,008,176,640,000 and the largest fixnum is
/// 4,611,686,018,427,387,903, so it fits — and 21! does not. The gate asks
/// for 20 rather than a rounder number because that is the largest
/// factorial needing no bignums.
#[test]
fn the_gate_fact_20() {
    program_gives(
        "(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 20)",
        "2432902008176640000",
    );
    program_gives(
        "(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 0)",
        "1",
    );
    program_gives(
        "(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1))))) (fact 5)",
        "120",
    );
}

// ---- tail calls ------------------------------------------------------

/// A tail call must not grow the stack.
///
/// The threshold allows a few dozen frames because `deepest_stack`
/// measures every form in the source, and expanding the `defun` is itself
/// a few frames deep. What matters is that the number does not move with
/// `n`: twenty thousand iterations cost the same as two.
#[test]
fn a_tail_call_does_not_grow_the_stack() {
    let (max_depth, answer) = deepest_stack(
        "(defun down (n) (if (= n 0) 'done (down (- n 1)))) (down 20000)",
        50,
    );
    assert_eq!(answer, "DONE");
    assert!(
        max_depth < 50,
        "the stack reached {max_depth} frames; a tail call grew it"
    );
}

/// Non-tail recursion must still work, and must still use the stack — a
/// machine that silently dropped frames would pass the test above.
#[test]
fn non_tail_recursion_still_grows_the_stack() {
    let (max_depth, answer) = deepest_stack(
        "(defun sum (n) (if (= n 0) 0 (+ n (sum (- n 1))))) (sum 100)",
        5,
    );
    assert_eq!(answer, "5050");
    assert!(
        max_depth > 100,
        "only {max_depth} frames; the stack should grow here"
    );
}

/// **A known limitation, pinned so it cannot change unnoticed.**
///
/// `defun` wraps its body in a block named after the function when the
/// body mentions `return-from` (see `lisp/prelude.lisp`). A `block`
/// pushes a frame, and a frame outstanding at a call is precisely what
/// tail-call elimination must not have — so a function that *both* uses
/// `return-from` and recurses in tail position costs one frame per call.
///
/// The evaluator cannot simply pop the block frame on a tail call,
/// because a closure can carry the block's marker out of the block and
/// call `return-from` later:
///
/// ```lisp
/// (defun f () (register-callback (lambda () (return-from f 'later))))
/// ```
///
/// Proving that does not happen is flow analysis, which belongs to the
/// compiler (phase 1) and not to a tree-walking interpreter. **SBCL's
/// interpreter behaves the same way** — it stacks `SB-EVAL::EVAL-BLOCK`
/// frames until the control stack is exhausted — so this is where the
/// line between interpreter and compiler normally sits, rather than a
/// fault peculiar to Quokka. Quokka at least fails with
/// `EvalError::HeapExhausted` instead of a native stack overflow, because
/// its frames are in its own heap.
///
/// If this test starts failing because the number got *smaller*, someone
/// has fixed it; delete the test and celebrate.
#[test]
fn a_block_in_the_body_still_defeats_the_tail_call() {
    let without = "(defun plain (n) (if (= n 0) 'done (plain (- n 1)))) (plain 5000)";
    let with = "(defun blocked (n)
                  (if (= n 0) (return-from blocked 'done) (blocked (- n 1))))
                (blocked 5000)";

    let (shallow, answer) = deepest_stack(without, 50);
    assert_eq!(answer, "DONE");
    assert!(shallow < 50, "no block, so no growth: got {shallow}");

    let (deep, answer) = deepest_stack(with, 50);
    assert_eq!(
        answer, "DONE",
        "the answer is still right, only the cost is wrong"
    );
    assert!(
        deep > 4_000,
        "{deep} frames for 5,000 calls — if this dropped, the block frame \
         is being elided now and this test should go"
    );
}

// ---- group A: scopes and near-no-ops ---------------------------------

#[test]
fn let_binds_all_at_once() {
    program_gives("(let ((x 1) (y 2)) (+ x y))", "3");
    program_gives("(let () 1)", "1");
    program_gives("(let ((x)) x)", "NIL");
    program_gives("(let (x) x)", "NIL");
    // Every init is evaluated in the OUTER scope, which is the whole
    // difference from let*.
    program_gives("(let ((x 1)) (let ((x 2) (y x)) y))", "1");
}

#[test]
fn let_star_binds_in_order() {
    program_gives("(let* ((x 1) (y (+ x 1))) (* x y))", "2");
    program_gives("(let* ((x 1) (x (+ x 1))) x)", "2");
    program_gives("(let ((x 1)) (let* ((x 2) (y x)) y))", "2");
}

#[test]
fn a_binding_is_undone_on_the_way_out() {
    program_gives("(setq g 'global) (let ((g 'inner)) g) g", "GLOBAL");
    // setq inside touches the binding, not the global.
    program_gives(
        "(setq h 'outer) (let ((h 'in)) (setq h 'changed)) h",
        "OUTER",
    );
}

/// A Lisp-2 keeps the namespaces apart in *lexical* scopes too, not only on
/// symbols. A local function named CAR must not disturb the variable, and a
/// local variable named CAR must not disturb the function.
#[test]
fn flet_and_let_bind_in_different_namespaces() {
    // The same name as a variable and as a function at once.
    program_gives(
        "(let ((f 5)) (flet ((f (x) (* x 2))) (list f (f 10))))",
        "(5 20)",
    );
    program_gives(
        "(flet ((f (x) (* x 2))) (let ((f 5)) (list f (f 10))))",
        "(5 20)",
    );
    // And the sharpest demonstration: inside an flet, the name as a
    // *variable* is still unbound. The namespaces do not leak into each
    // other even when one of them has a binding.
    assert!(matches!(
        eval_all("(flet ((f (x) x)) f)"),
        Err(EvalError::UnboundVariable(_))
    ));
    // Deliberately not tested against the oracle: binding a COMMON-LISP
    // symbol is undefined behaviour and SBCL rejects it with a package
    // lock. Quokka has no packages, so this is Quokka's behaviour rather
    // than Common Lisp's.
    program_gives("(let ((car 5)) (car '(1 2)))", "1");
}

/// The only difference between them: which scope the closures capture.
#[test]
fn labels_sees_itself_and_flet_does_not() {
    program_gives(
        "(labels ((e (n) (if (= n 0) t (o (- n 1))))
                  (o (n) (if (= n 0) nil (e (- n 1)))))
           (list (e 10) (o 10)))",
        "(T NIL)",
    );
    // An flet binding referring to a sibling must not find it.
    assert!(matches!(
        eval_all("(flet ((a () (b)) (b () 1)) (a))"),
        Err(EvalError::UndefinedFunction(_))
    ));
}

/// Tail calls must still work through a lexical function binding.
#[test]
fn labels_recursion_is_still_tail_recursive() {
    program_gives(
        // Five thousand rather than fifty: PLENTY is a step budget, and
        // each iteration costs a dozen steps. Constant *space* is what is
        // being shown here, and `a_tail_call_does_not_grow_the_stack`
        // measures the depth directly.
        "(labels ((down (n) (if (= n 0) 'done (down (- n 1))))) (down 5000))",
        "DONE",
    );
}

#[test]
fn function_finds_what_a_name_denotes() {
    program_gives("(funcall #'car '(1 2))", "1");
    program_gives("(funcall (function cdr) '(1 2))", "(2)");
    program_gives("(funcall #'(lambda (x) (* x 3)) 14)", "42");
    // A lexical binding shadows the symbol's own function cell.
    program_gives("(flet ((f (x) 'local)) (funcall #'f 1))", "LOCAL");
}

#[test]
fn the_and_locally_reduce_to_their_body() {
    program_gives("(the integer 42)", "42");
    program_gives("(the t (+ 1 1))", "2");
    program_gives("(locally 1 2)", "2");
    program_gives("(locally)", "NIL");
}

/// An interpreter has one situation: it is executing.
#[test]
fn eval_when_runs_only_for_execute() {
    program_gives("(eval-when (:execute) 'ran)", "RAN");
    program_gives("(eval-when (eval) 'ran)", "RAN");
    program_gives("(eval-when (:compile-toplevel) 'no)", "NIL");
    program_gives("(eval-when () 'no)", "NIL");
}

/// A binding list is a list. Treating a non-list as "no bindings" would
/// quietly accept nonsense, which a test found before SBCL did.
#[test]
fn malformed_binding_lists_signal() {
    for source in [
        "(let)",
        "(let 1 2)",
        "(let* 1)",
        "(flet 1)",
        "(flet ((f)) 1)",
    ] {
        assert!(
            matches!(eval_all(source), Err(EvalError::BadSyntax { .. })),
            "{source} should be malformed"
        );
    }
    assert!(matches!(
        eval_all("(function)"),
        Err(EvalError::BadSyntax { .. })
    ));
    assert!(matches!(
        eval_all("(function nosuchthing)"),
        Err(EvalError::UndefinedFunction(_))
    ));
}

// ---- nil is a symbol -------------------------------------------------

/// The point of the static area, and the thing four special cases were
/// paying for. `nil` and `t` are symbols, as Common Lisp requires.
#[test]
fn nil_and_t_are_symbols() {
    program_gives("(symbolp nil)", "T");
    program_gives("(symbolp t)", "T");
    program_gives("(symbolp '())", "T");
    program_gives("(symbolp 'anything)", "T");
    program_gives("(symbolp 1)", "NIL");
    program_gives("(symbolp \"text\")", "NIL");
    program_gives("(symbolp '(1))", "NIL");
}

/// And everything `nil` did before still works, which is the part a
/// representation change could quietly break.
#[test]
fn nil_is_still_false_and_the_empty_list() {
    program_gives("(null nil)", "T");
    program_gives("(null '())", "T");
    program_gives("(eq nil '())", "T");
    program_gives("(if nil 'yes 'no)", "NO");
    program_gives("(cdr '(1))", "NIL");
    program_gives("(cons 1 nil)", "(1)");
    program_gives("nil", "NIL");
    program_gives("t", "T");
    program_gives("(eq 'nil nil)", "T");
}

/// `(block nil ...)` needed a special case when `nil` was an immediate.
/// It needs none now, which is what "fixed" means here.
#[test]
fn block_nil_needs_no_special_case() {
    program_gives("(block nil (return-from nil 'works))", "WORKS");
    program_gives("(block nil 1 2 3)", "3");
}

/// Reading `nil` must find the static symbol rather than making a new one.
#[test]
fn nil_is_interned_like_any_symbol() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        assert_eq!(mc.intern("NIL"), Value::NIL);
        assert_eq!(mc.intern("T"), Value::T);
        assert_eq!(mc.symbol_name(Value::NIL), "NIL");
        assert_eq!(mc.symbol_name(Value::T), "T");
        assert!(mc.is_symbol(Value::NIL));
    });
}

/// The static area must survive collection without moving, or every
/// `Value::NIL` in the system would name something else.
#[test]
fn nil_does_not_move_when_everything_else_does() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let mut list = Value::NIL;
        for n in 0..100 {
            list = mc.cons(Value::fixnum(n).unwrap(), list);
        }
        mc.set_root(list);
    });
    for _ in 0..5 {
        heap.collect();
        heap.verify().unwrap();
    }
    heap.mutate(|mc| {
        assert!(mc.is_symbol(Value::NIL), "nil stopped being a symbol");
        assert_eq!(mc.symbol_name(Value::NIL), "NIL");
        assert_eq!(mc.intern("NIL"), Value::NIL, "a second nil appeared");
        // And the list that was moved five times is still terminated by it.
        let mut cursor = mc.root();
        let mut count = 0;
        while !cursor.is_nil() {
            count += 1;
            cursor = mc.cdr(cursor);
        }
        assert_eq!(count, 100);
    });
}

// ---- group B: non-local exits ----------------------------------------

#[test]
fn block_and_return_from_are_lexical() {
    program_gives("(block nil 1 2 3)", "3");
    program_gives("(block nil 1 (return-from nil 2) 3)", "2");
    program_gives("(block done (return-from done 'early) 'late)", "EARLY");
    // The inner block does not catch a return aimed at the outer one.
    program_gives(
        "(block outer (block inner (return-from outer 'far)) 'near)",
        "FAR",
    );
    program_gives(
        "(block outer (block inner (return-from inner 'in)) 'after)",
        "AFTER",
    );
    program_gives("(block b (return-from b (+ 1 2)))", "3");
    program_gives("(block b (return-from b))", "NIL");
}

/// Two entries to the same block are two different blocks, which is why the
/// target is a fresh object rather than the name.
#[test]
fn recursive_blocks_do_not_collide() {
    program_gives(
        "(defun f (n) (block b (if (= n 0) (return-from b 'bottom) (f (- n 1)))))
         (f 5)",
        "BOTTOM",
    );
}

/// catch is the dynamic version: the thrower need not know where the catch
/// is, or even that there is one.
#[test]
fn catch_and_throw_are_dynamic() {
    program_gives("(catch 'tag 1 2 3)", "3");
    program_gives("(catch 'tag (throw 'tag 42) 'unreached)", "42");
    program_gives("(catch 'a (catch 'b (throw 'a 'through)))", "THROUGH");
    program_gives("(catch 'a (catch 'b (throw 'b 'inner)))", "INNER");
    // Through fifty frames of a caller that knows nothing about it.
    program_gives(
        "(defun deep (n) (if (= n 0) (throw 'out 'arrived) (deep (- n 1))))
         (catch 'out (deep 50))",
        "ARRIVED",
    );
}

#[test]
fn tagbody_and_go_make_a_loop() {
    program_gives("(tagbody)", "NIL");
    program_gives("(tagbody a b c)", "NIL");
    program_gives(
        "(setq n 0) (tagbody top (setq n (+ n 1)) (if (< n 5) (go top))) n",
        "5",
    );
    program_gives(
        "(setq out nil) (tagbody (go skip) (setq out 'wrong) skip (setq out 'right)) out",
        "RIGHT",
    );
}

/// A tagbody loop must run in **constant space**, or every looping macro
/// in the language inherits a limit from the construct it expands into.
///
/// Measured rather than inferred: the frame is replaced on each iteration
/// rather than pushed, and the way to show that is to watch the depth.
#[test]
fn a_tagbody_loop_does_not_grow_the_stack() {
    let (deepest, answer) = deepest_stack(
        "(setq n 0) (tagbody top (setq n (+ n 1)) (if (< n 20000) (go top))) n",
        50,
    );
    assert_eq!(answer, "20000");
    assert!(
        deepest < 20,
        "twenty thousand iterations reached {deepest} frames"
    );
}

/// And a `catch` that is never thrown to must not accumulate either, since
/// a loop wrapped in one is ordinary.
#[test]
fn repeated_catches_do_not_accumulate() {
    let (deepest, answer) = deepest_stack(
        "(defun f (n) (if (= n 0) 'done (catch 'tag (f (- n 1)))))
         (f 200)",
        50,
    );
    assert_eq!(answer, "DONE");
    // This one *should* grow — a catch is a frame that must be there to be
    // found. The point is that it grows linearly rather than faster.
    assert!(
        deepest > 200 && deepest < 1_000,
        "200 nested catches reached {deepest} frames"
    );
}

#[test]
fn unwind_protect_cleans_up_on_a_normal_return() {
    program_gives(
        "(setq log nil) (unwind-protect 'body (setq log 'cleaned)) log",
        "CLEANED",
    );
    // The body's value survives; the cleanup's is discarded.
    program_gives("(unwind-protect 'kept 'discarded)", "KEPT");
}

/// The hard case: the unwind has to *pause*, run ordinary code, and then
/// carry on from where it was.
#[test]
fn unwind_protect_cleans_up_while_being_unwound_through() {
    program_gives(
        "(setq log nil) (block b (unwind-protect (return-from b 'left) (setq log 'cleaned))) log",
        "CLEANED",
    );
    program_gives(
        "(setq log nil) (catch 'x (unwind-protect (throw 'x 'thrown) (setq log 'cleaned))) log",
        "CLEANED",
    );
    // And the unwind continues past it to a target further out.
    program_gives(
        "(catch 'outer (unwind-protect (catch 'inner (throw 'outer 'past)) nil))",
        "PAST",
    );
}

/// Nested cleanups run innermost first, which is the only order that makes
/// sense and is worth pinning.
#[test]
fn nested_cleanups_run_inside_out() {
    program_gives(
        "(setq log nil)
         (catch 'x
           (unwind-protect
               (unwind-protect (throw 'x 'done) (setq log (cons 'inner log)))
             (setq log (cons 'outer log))))
         log",
        "(OUTER INNER)",
    );
}

#[test]
fn an_exit_with_no_target_signals() {
    for source in [
        "(return-from nowhere 1)",
        "(throw 'nothing 1)",
        "(go nowhere)",
    ] {
        assert!(
            matches!(eval_all(source), Err(EvalError::NoSuchExit(_))),
            "{source} should have nowhere to go"
        );
    }
    assert!(matches!(
        eval_all("(block)"),
        Err(EvalError::BadSyntax { .. })
    ));
    assert!(matches!(
        eval_all("(catch)"),
        Err(EvalError::BadSyntax { .. })
    ));
}

// ---- not finishing ---------------------------------------------------

/// A recursion with no base case must stop, and stop *cheaply*.
///
/// `(fact -1)` never reaches `(= n 0)`, and because the `*` is still
/// pending it is not a tail call, so every level keeps its frames.
/// Collection cannot help: they are all reachable. Only a ceiling can.
#[test]
fn a_recursion_that_never_ends_hits_the_heap_ceiling() {
    // Defined with the primitive rather than `defun`, so the 4,000-word
    // heap holds only what this test is about. `defun` is a prelude macro
    // now, and the prelude does not fit in a heap this small — which is
    // the right answer for a test of the ceiling itself.
    let source = "(%set-symbol-function 'fact\n              (lambda (n) (if (= n 0) 1 (* n (fact (- n 1))))))\n             (fact -1)";
    let chars = Reader::chars_of(source);
    let mut heap = Heap::with_soft_limit(4_000);
    let limit = heap.hard_limit();

    let outcome = heap.mutate(|mc| {
        primitives::install(mc);
        let mut reader = Reader::new(&chars);
        let definition = reader.read(mc).unwrap().unwrap();
        eval_now(mc, definition, PLENTY).unwrap();
        let call = reader.read(mc).unwrap().unwrap();
        // The value cannot leave the mutation, and is not wanted: the
        // error is the subject. Mapping it away is the brand doing its job.
        eval_now(mc, call, u64::MAX).map(|_| ())
    });

    assert_eq!(outcome, Err(EvalError::HeapExhausted));
    // The overshoot past the ceiling is one step's worth of words, not a
    // multiple of the heap.
    assert!(
        heap.used_words() < limit + 64,
        "overshot the ceiling by {} words",
        heap.used_words() - limit
    );
}

/// And the heap must be usable afterwards. An error that leaves the system
/// unusable is only marginally better than a crash.
#[test]
fn the_heap_recovers_after_exhaustion() {
    // Defined with the primitive rather than `defun`, so the 4,000-word
    // heap holds only what this test is about. `defun` is a prelude macro
    // now, and the prelude does not fit in a heap this small — which is
    // the right answer for a test of the ceiling itself.
    let source = "(%set-symbol-function 'fact\n              (lambda (n) (if (= n 0) 1 (* n (fact (- n 1))))))\n             (fact -1)";
    let chars = Reader::chars_of(source);
    let mut heap = Heap::with_soft_limit(4_000);

    heap.mutate(|mc| {
        primitives::install(mc);
        let mut reader = Reader::new(&chars);
        let definition = reader.read(mc).unwrap().unwrap();
        eval_now(mc, definition, PLENTY).unwrap();
        let call = reader.read(mc).unwrap().unwrap();
        assert_eq!(
            eval_now(mc, call, u64::MAX).map(|_| ()),
            Err(EvalError::HeapExhausted)
        );
    });

    // What filled the heap is unreachable now, so collecting frees it.
    assert!(heap.is_exhausted());
    heap.collect();
    heap.clear_exhausted();
    assert!(!heap.is_exhausted());
    heap.verify().unwrap();

    let answer = heap.mutate(|mc| {
        let chars = Reader::chars_of("(fact 20)");
        let mut reader = Reader::new(&chars);
        let call = reader.read(mc).unwrap().unwrap();
        let value = eval_now(mc, call, PLENTY).unwrap();
        print(mc, value)
    });
    assert_eq!(answer, "2432902008176640000");
}

/// A budget that runs out is reported as itself, not as a syntax error.
#[test]
fn running_out_of_budget_says_so() {
    let mut heap = Heap::new();
    let chars = Reader::chars_of("(+ 1 (* 2 (- 10 (+ 3 4))))");
    let outcome = heap.mutate(|mc| {
        crate::boot::install_in(mc).expect("the prelude boots");
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).unwrap().unwrap();
        eval_now(mc, form, 3).map(|_| ())
    });
    assert_eq!(outcome, Err(EvalError::BudgetExhausted));
}

// ---- the machine, rather than the language ---------------------------

/// The property `crate::slice` was designed for, now on something real:
/// how a computation is cut up must not change what it computes.
#[test]
fn slicing_does_not_change_the_answer() {
    use crate::slice::{run_slice, Status};

    let source = "(+ 1 (* 2 (- 10 (+ 3 4))))";
    let chars = Reader::chars_of(source);

    for budget in [1u64, 2, 3, 5, 8, 13, 100, 10_000] {
        let mut heap = Heap::new();
        let answer = heap.mutate(|mc| {
            crate::boot::install_in(mc).expect("the prelude boots");
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            {
                let mut machine = Machine::start(mc, form, Value::NIL);
                let mut slices = 0;
                while run_slice(&mut machine, budget) == Status::Yielded {
                    slices += 1;
                    assert!(slices < 10_000, "budget {budget} made no progress");
                }
            }
            // The answer is in the Value register, which outlives the
            // machine — the machine holds no state of its own.
            print(mc, mc.register(Register::Value))
        });
        assert_eq!(answer, "7", "budget {budget} gave a different answer");
    }
}

/// Deep nesting must consume heap, not the Rust stack. A recursive
/// evaluator would die here, and on wasm32 it would die far sooner.
///
/// The form is built directly rather than read, because the *reader* has a
/// depth limit of 200 and would refuse it. The two limits are unrelated:
/// the reader's exists because the reader recurses in Rust, and the
/// evaluator has none because it does not. Reading `(+ 1 (+ 1 ...))` five
/// thousand deep is a different problem from evaluating it.
#[test]
fn deep_nesting_does_not_touch_the_rust_stack() {
    let depth = 5_000;
    let mut heap = Heap::new();
    let answer = heap.mutate(|mc| {
        crate::boot::install_in(mc).expect("the prelude boots");
        let plus = mc.intern("+");
        let one = Value::fixnum(1).unwrap();

        let mut form = Value::fixnum(0).unwrap();
        for _ in 0..depth {
            let inner = mc.cons(form, Value::NIL);
            let args = mc.cons(one, inner);
            form = mc.cons(plus, args);
        }

        eval_now(mc, form, 10_000_000).map(|v| print(mc, v))
    });
    assert_eq!(answer.as_deref(), Ok(depth.to_string().as_str()));
}

/// Everything the machine is holding lives in registers the collector
/// forwards. If any one of them were forgotten, a collection part-way
/// through would reclaim the computation.
#[test]
fn a_computation_survives_collection_midway() {
    use crate::slice::{run_slice, Status};

    let chars = Reader::chars_of("(+ 1 (* 2 (+ 3 4)))");
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        crate::boot::install_in(mc).expect("the prelude boots");
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).unwrap().unwrap();
        Machine::start(mc, form, Value::NIL);
    });

    // Step a little, collect, step a little, collect...
    let mut finished = false;
    for _ in 0..200 {
        let done = heap.mutate(|mc| {
            let mut machine = Machine::resume(mc);
            matches!(run_slice(&mut machine, 2), Status::Done { .. }) || machine.is_halted()
        });
        heap.collect();
        heap.verify().unwrap();
        if done {
            finished = true;
            break;
        }
    }
    assert!(finished, "the machine never halted");
    let answer = heap.mutate(|mc| print(mc, mc.register(Register::Value)));
    assert_eq!(answer, "15");
}
