//! Quokka core VM.
//!
//! This crate is everything below the primitive table: tagged words, the heap,
//! the garbage collector, image save and load, the reader, the bootstrap
//! interpreter and (later) bytecode execution.
//!
//! It has **no host dependencies**. No `std::fs`, no threads, no wall clock, no
//! networking. Everything the VM needs from the outside world arrives through
//! [`host::Host`], which the native and WASM hosts implement separately.
//!
//! The VM never runs to completion on its own: it runs in slices and hands
//! control back to whoever called it, so the browser's event loop keeps
//! turning. See [`mod@slice`] for what that costs and why it is worth it.
//!
//! Every Lisp object is one tagged 64-bit word; see [`value`] for the encoding
//! and why it is shaped the way it is.
//!
//! See `docs/design.md` for the design and `CLAUDE.md` for the rules.

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

pub mod boot;
pub mod eval;
pub mod heap;
pub mod host;
pub mod primitives;
pub mod printer;
pub mod reader;
pub mod slice;
pub mod value;

/// The Quokka version string, as reported by the REPL banner and the dev console.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The phase this build implements, for the banner.
pub const PHASE: &str = "0 · Spike";

/// Evaluate every form in `source` and render the results in the differential
/// testing format: one line per top-level form.
///
/// ```text
/// => 42          the form's value, printed as the reader would want it back
/// !! error       the form signalled
/// ```
///
/// Errors are flattened to a single token deliberately. Quokka says "the
/// function NOPE is undefined" and SBCL says something else entirely, with a
/// condition class and a restart list; the text will never agree. Comparing
/// condition *classes* would be stronger and is a refinement for when the
/// condition system exists — for now this still catches the large case, which
/// is one implementation signalling where the other does not.
pub fn run_forms(source: &str, budget: u64) -> String {
    use crate::eval::eval_now;
    use crate::heap::Heap;
    use crate::printer::print;
    use crate::reader::Reader;

    let mut heap = Heap::new();
    // The prelude is part of the system under test: `defun` is one of its
    // macros now, and SBCL has it too.
    crate::boot::install(&mut heap).expect("the prelude boots");
    let chars = Reader::chars_of(source);
    let mut out = heap.mutate(|mc| {
        let mut reader = Reader::new(&chars);
        let mut out = String::new();
        loop {
            match reader.read(mc) {
                Ok(None) => break,
                Ok(Some(form)) => match eval_now(mc, form, budget) {
                    Ok(value) => {
                        out.push_str("=> ");
                        out.push_str(&print(mc, value));
                        out.push('\n');
                    }
                    Err(_) => out.push_str("!! error\n"),
                },
                // A malformed file is a fault in the test, not a result worth
                // comparing, so it is loud rather than recorded.
                Err(e) => panic!("could not read the test file: {e}"),
            }
        }
        out
    });
    if heap.is_exhausted() {
        out.push_str("!! heap exhausted\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert!(!VERSION.is_empty());
        assert_eq!(PHASE, "0 · Spike");
    }
}
