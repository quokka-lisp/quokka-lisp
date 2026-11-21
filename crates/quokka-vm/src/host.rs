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

    // Note: there is deliberately no `should_yield` or clock method here.
    //
    // Rule 4 says the VM must be able to yield, and something has to decide when
    // a slice ends. Three shapes were considered: the VM polling the host, the
    // host setting a wall-clock deadline the VM watches, or the caller passing a
    // step budget. The budget won — see the module docs on `crate::slice` for
    // the reasoning. It needs nothing from the host, which is why this trait
    // stays as small as it is.
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
#[path = "unit/host.rs"]
mod tests;
