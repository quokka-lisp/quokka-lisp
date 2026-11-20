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
//! See `docs/design.md` for the design and `CLAUDE.md` for the rules.

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

pub mod host;

/// The Quokka version string, as reported by the REPL banner and the dev console.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The phase this build implements, for the banner.
pub const PHASE: &str = "0 · Spike";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert!(!VERSION.is_empty());
        assert_eq!(PHASE, "0 · Spike");
    }
}
