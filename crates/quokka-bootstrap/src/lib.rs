//! Quokka's cold build: everything needed to make the *first* world, and
//! nothing a world needs once it exists (#29).
//!
//! A built world reads, compiles, evaluates and prints in its own Lisp, on
//! the bytecode VM in `quokka-vm`. Something has to make the first one, and it
//! cannot be a world: this crate is that something.
//!
//! - [`reader`] — the Rust reader, which reads the prelude, the compiler and
//!   the library before any Lisp can run.
//! - [`eval`] — the tree-walking interpreter, which runs the compiler the
//!   first time, so that it can compile itself.
//! - [`boot`] — the cold build itself, `build_image`: interpret, compile,
//!   install, load the library, prove nothing interpreted survived.
//!
//! The split is #25's *one evaluator at a time* made structural: `quokka-vm`
//! does not depend on this crate, so the VM cannot reach the interpreter even
//! by accident.

pub mod boot;
pub mod eval;
pub mod reader;

/// Evaluate every form in `source` and render the results in the differential
/// testing format: one line per top-level form.
///
/// ```text
/// => 42          the form's value
/// !! type-error  the form signalled, and the condition's type if CLHS names it
/// ```
pub fn run_forms(source: &str, budget: u64) -> String {
    use quokka_vm::heap::Heap;
    use quokka_vm::printer::print;

    let mut heap = Heap::new();
    // The prelude is part of the system under test: `defun` is one of its
    // macros now, and SBCL has it too.
    crate::boot::install(&mut heap).expect("the prelude boots");
    let mut out = String::new();
    // Sliced, so the collector runs between slices. A test file that built a
    // large structure in one form would otherwise have to fit in the heap
    // with all of its garbage (#32).
    let read = crate::eval::eval_forms(&mut heap, source, budget, 2_000, |mc, result| {
        match result {
            Ok(value) => {
                out.push_str("=> ");
                out.push_str(&print(mc, value));
                out.push('\n');
            }
            Err(e) => out.push_str(&quokka_vm::condition::report_error(&e)),
        }
        true
    });
    // A malformed file is a fault in the test, not a result worth comparing.
    if read.is_err() {
        out.push_str("!! read error\n");
    }
    out
}
