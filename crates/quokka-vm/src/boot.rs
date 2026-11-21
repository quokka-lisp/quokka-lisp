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

/// The compiler, in the order its files must be read.
///
/// **Not loaded at boot.** The prelude is interpreted every time a heap is
/// prepared, because it defines `defun` and nothing works without it. The
/// compiler is interpreted *once*, to build an image, and after that it is
/// compiled code inside that image — so making every REPL start interpret it
/// would be paying for something that start is not going to use.
///
/// Listed rather than discovered, because there is no `load` yet and the
/// order matters. `scripts/check-compiler.sh` fails if a file in
/// `lisp/compiler/` is missing from here, for the same reason
/// `check-differential.sh` exists: a source file nobody loads is worse than
/// no source file.
pub const COMPILER: &[(&str, &str)] = &[(
    "compile.lisp",
    include_str!("../../../lisp/compiler/compile.lisp"),
)];

/// Load the compiler into a heap that already has the prelude.
///
/// Separate from [`install`] on purpose: see [`COMPILER`]. A REPL does not
/// call this unless asked to; the image builder and the tests do.
pub fn install_compiler(heap: &mut crate::heap::Heap) -> Result<(), BootError> {
    for (_file, source) in COMPILER {
        load(heap, source)?;
    }
    Ok(())
}

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

/// Read the prelude and evaluate every form, in order.
///
/// Sliced, so the collector runs during a form as well as between them —
/// see [`crate::eval::eval_forms`] and issue #32.
pub fn install(heap: &mut crate::heap::Heap) -> Result<(), BootError> {
    heap.mutate(primitives::install);
    load(heap, PRELUDE)
}

/// Load one source text, stopping at the first form that fails.
fn load(heap: &mut crate::heap::Heap, source: &str) -> Result<(), BootError> {
    let mut form_number = 0usize;
    let mut failure = None;
    let read = crate::eval::eval_forms(heap, source, BOOT_BUDGET, 2_000, |_mc, result| {
        form_number += 1;
        match result {
            Ok(_) => true,
            Err(error) => {
                failure = Some(BootError::Eval {
                    form: form_number,
                    error,
                });
                false
            }
        }
    });
    if let Err(e) = read {
        return Err(BootError::Read(e));
    }
    match failure {
        Some(e) => Err(e),
        None => Ok(()),
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
#[path = "unit/boot.rs"]
mod tests;
