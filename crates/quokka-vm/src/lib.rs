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
pub mod code;
pub mod condition;
pub mod eval;
pub mod hash;
pub mod heap;
pub mod host;
pub mod lambda_list;
pub mod primitives;
pub mod printer;
pub mod reader;
pub mod slice;
pub mod value;
pub mod vm;

/// The Quokka version string, as reported by the REPL banner and the dev console.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The phase this build implements, for the banner.
///
/// Phase 0 ended when the gate passed; tier 0 — the language the compiler is
/// written in — is finished, so this build is the one phase 1 bootstraps from.
pub const PHASE: &str = "1 · Bytecode and image";

/// Evaluate every form in `source` and render the results in the differential
/// testing format: one line per top-level form.
///
/// ```text
/// => 42          the form's value
/// !! error       the form signalled
/// ```
pub fn run_forms(source: &str, budget: u64) -> String {
    use crate::heap::Heap;
    use crate::printer::print;

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
            Err(_) => out.push_str("!! error\n"),
        }
        true
    });
    // A malformed file is a fault in the test, not a result worth comparing.
    if read.is_err() {
        out.push_str("!! read error\n");
    }
    out
}

#[cfg(test)]
#[path = "unit/lib.rs"]
mod tests;
