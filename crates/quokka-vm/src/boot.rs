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
#[path = "tests/boot.rs"]
mod tests;
