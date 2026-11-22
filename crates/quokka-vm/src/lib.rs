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

pub mod closure;
pub mod code;
pub mod condition;
pub mod env;
pub mod error;
pub mod hash;
pub mod heap;
pub mod host;
pub mod image;
pub mod lambda_list;
pub mod primitives;
pub mod printer;
pub mod slice;
pub mod special;
pub mod value;
pub mod vm;

/// The Quokka version string, as reported by the REPL banner and the dev console.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The phase this build implements, for the banner.
///
/// Phase 2: its gate, the fixpoint, passes (#50); its scope, all of tier 1,
/// is under way (#39).
pub const PHASE: &str = "2 · Self-hosting";

#[cfg(test)]
#[path = "unit/lib.rs"]
mod tests;
