//! The host interface.
//!
//! CLAUDE.md rule 2: the core VM crate has no host dependencies. Everything the
//! VM needs from the outside world — console output, loading a Lisp source file,
//! and (later) the clock, storage and the display protocol — arrives through
//! this trait. `quokka-repl` implements it over `std`; `quokka-wasm` implements
//! it over the browser.
//!
//! Until file hosts exist (see "Files and storage" in `docs/design.md`), Lisp
//! sources are loaded through a single primitive: [`Host::read_source`].

use core::fmt;

/// Why a host operation failed.
///
/// Deliberately coarse: the VM turns these into Lisp conditions, and the host
/// has no business describing its own errors in detail to the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The named resource does not exist on this host.
    NotFound(String),
    /// The host refused the operation (permissions, sandbox, quota).
    Denied(String),
    /// Anything else the host wants to report.
    Other(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::NotFound(what) => write!(f, "not found: {what}"),
            HostError::Denied(why) => write!(f, "denied: {why}"),
            HostError::Other(why) => write!(f, "{why}"),
        }
    }
}

/// Services the VM asks of whatever is hosting it.
///
/// Implementations must not block for long: CLAUDE.md rule 4 says the
/// interpreter runs in slices and returns control to its caller.
pub trait Host {
    /// Write a string to the host's console or dev-console transcript.
    fn write_stdout(&mut self, s: &str);

    /// Read a Lisp source file by name.
    ///
    /// Natively this is `std::fs`; under WASI it is the WASI filesystem; in the
    /// browser it is a file bundled into the page.
    fn read_source(&mut self, name: &str) -> Result<String, HostError>;

    // ----------------------------------------------------------------------
    // TODO(carsten): yielding.
    //
    // Rule 4 says the VM must be able to yield to the browser event loop, and
    // the interpreter runs in slices. That means something has to decide *when*
    // a slice ends, and only the host knows: natively "never, just run", in the
    // browser "when this frame's budget is spent".
    //
    // Three shapes, with different costs:
    //
    //   a) `fn should_yield(&mut self) -> bool` — the interpreter polls it every
    //      N reductions. Simple, no host timer in the core, but the poll
    //      interval becomes a tuning knob and the host cannot pre-empt.
    //
    //   b) `fn deadline_passed(&self) -> bool` backed by a host-set deadline —
    //      more accurate, but puts a notion of time into the host trait, which
    //      the native host has to fake or ignore.
    //
    //   c) No host method at all: the *caller* passes a step budget into
    //      `run_slice(budget)` and the VM returns when it is spent. Keeps the
    //      trait smaller and makes tests deterministic, but the browser has to
    //      guess a budget rather than measure one.
    //
    // (c) is the most testable and (b) the most responsive. Pick one and add
    // the method (or deliberately leave the trait alone) here. This shapes the
    // interpreter's main loop, so it is worth deciding before phase 0 code
    // lands rather than after.
    // ----------------------------------------------------------------------
}

/// A host that discards output and has no files. Useful in unit tests.
#[derive(Debug, Default)]
pub struct NullHost {
    /// Everything written to stdout, kept so tests can assert on it.
    pub output: String,
}

impl Host for NullHost {
    fn write_stdout(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        Err(HostError::NotFound(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_host_collects_output_and_has_no_files() {
        let mut host = NullHost::default();
        host.write_stdout("hello");
        assert_eq!(host.output, "hello");
        assert_eq!(
            host.read_source("boot.lisp"),
            Err(HostError::NotFound("boot.lisp".to_string()))
        );
    }
}
