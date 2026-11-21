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
use crate::heap::ObjectType;
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

/// What [`compile_source`] found in a file.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Forms handed to `compile-form`, verified, and thrown away.
    pub compiled: usize,
    /// Forms evaluated by the interpreter instead — the macro definitions.
    pub interpreted: usize,
}

/// Compile every form of `source`, without installing any of the results.
///
/// This is the file compiler of #31, and the thing to understand about it is
/// what it deliberately does *not* do.
///
/// # Why nothing is installed
///
/// The obvious file compiler compiles a file and installs what it produces,
/// so that `(defun length ...)` leaves a *compiled* `length` behind. Doing
/// that to the prelude would break the system in the same breath, because the
/// compiler is itself interpreted Lisp and it calls `length`, `reverse`,
/// `second`, `cddr`, `member`, `append` and `apply` — all of them prelude
/// functions. Installing a compiled `length` would hand the interpreter a
/// compiled closure the first time the compiler asked how long a list was,
/// and [`crate::eval`] now refuses one, loudly, by design.
///
/// That is not a limitation to work around. It is #25's rule holding: one
/// evaluator at a time. The way out is not to install into a running
/// interpreter at all, but to compile everything, write the results into an
/// image, and start the *next* session from that image with the bytecode VM
/// already in charge. Installing therefore belongs to phase 1's step D, not
/// here.
///
/// So what this proves is narrower and still worth proving: that the compiler
/// can compile every form of a real file, and that the verifier accepts
/// everything it emits. A compiler that handles the prelude handles something
/// nobody wrote to flatter it.
///
/// # Why macros are evaluated
///
/// A `defmacro` is interpreted rather than compiled, which looks like a
/// special case and is really the same rule again. The compiler expands
/// macros while it compiles, which means it *calls* the expander — from
/// interpreted code. An expander compiled here could not be called, so the
/// next form that used the macro would fail. The expander has to exist, in
/// the interpreter, before the forms that use it are compiled.
///
/// This is the bootstrap rule, and it is the host's job only until the image
/// exists. The REPL was doing it by hand; now there is one copy.
pub fn compile_source(heap: &mut crate::heap::Heap, source: &str) -> Result<Tally, BootError> {
    use crate::heap::Register;
    use crate::value::Value;

    /// What the reader left behind, and what to do about it.
    enum Next {
        Done,
        Unreadable(ReadError),
        /// `(compile-form 'FORM)` is in the register.
        Compile,
        /// The form itself is in the register.
        Interpret,
    }

    let chars = Reader::chars_of(source);
    let mut reader = Reader::new(&chars);
    let mut tally = Tally::default();
    let mut form_number = 0usize;

    loop {
        form_number += 1;
        let next = heap.mutate(|mc| match reader.read(mc) {
            Err(e) => Next::Unreadable(e),
            Ok(None) => Next::Done,
            Ok(Some(form)) => {
                if form.is_cons() && mc.symbol_is(mc.car(form), "DEFMACRO") {
                    mc.set_register(Register::Value, form);
                    return Next::Interpret;
                }
                // `(compile-form 'FORM)`. The quote matters: `compile-form`
                // takes the form as data, and without it the form would be
                // evaluated on the way in.
                let quoted = {
                    let quote = mc.intern("QUOTE");
                    let inner = mc.cons(form, Value::NIL);
                    mc.cons(quote, inner)
                };
                let call = {
                    let compile = mc.intern("COMPILE-FORM");
                    let args = mc.cons(quoted, Value::NIL);
                    mc.cons(compile, args)
                };
                mc.set_register(Register::Value, call);
                Next::Compile
            }
        });

        let compiling = match next {
            Next::Done => return Ok(tally),
            Next::Unreadable(e) => return Err(BootError::Read(e)),
            Next::Compile => true,
            Next::Interpret => false,
        };

        // Sliced either way, so the collector runs *during* a form as well as
        // between them. Compiling allocates far more than evaluating does,
        // and #32 is what happens when it cannot — see [`crate::eval`].
        if let Err(error) = crate::eval::eval_sliced(heap, BOOT_BUDGET, 2_000) {
            return Err(BootError::Eval {
                form: form_number,
                error,
            });
        }

        if !compiling {
            tally.interpreted += 1;
            continue;
        }

        // The verifier is the point. "It compiled" only means the compiler
        // did not signal; "it verifies" means the bytes are a program the
        // machine can run — no jump into the middle of an instruction, no
        // path that underflows, no constant that is not there. Checking here
        // makes the whole prelude a test of the compiler's output and not
        // merely of its temper.
        if let Some(bad) = heap.mutate(|mc| {
            let f = mc.register(Register::Value);
            if !f.is_object() || mc.header(f).object_type != ObjectType::Function {
                return Some(format!(
                    "compile-form returned {}",
                    crate::printer::print(mc, f)
                ));
            }
            crate::code::verify::function(mc, f)
                .err()
                .map(|e| format!("{e:?}"))
        }) {
            return Err(BootError::Eval {
                form: form_number,
                error: EvalError::Malformed(bad),
            });
        }
        tally.compiled += 1;
    }
}

#[cfg(test)]
#[path = "unit/boot.rs"]
mod tests;
