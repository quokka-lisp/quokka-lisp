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
        // 2,476 words when this was written — about 20 KB, which is the
        // macro objects, the closures, their bodies, and one symbol per name.
        let words = heap.used_words();
        assert!(
            words < 4_000,
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
