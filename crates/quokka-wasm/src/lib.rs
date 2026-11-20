//! Quokka WASM bindings.
//!
//! This crate is the browser *host*. It implements
//! [`quokka_vm::host::Host`] over the page and exposes a small surface to
//! JavaScript for the dev console in `www/`.

use quokka_vm::host::{Host, HostError};
use quokka_vm::{PHASE, VERSION};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// The browser host. Output is buffered and handed back to JavaScript rather
/// than printed, so the VM never touches the DOM directly — see the display
/// protocol section of `docs/design.md`.
#[derive(Default)]
pub struct BrowserHost {
    output: String,
}

impl Host for BrowserHost {
    fn write_stdout(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        // Phase 0: sources are bundled into the page. Wire that up here.
        Err(HostError::NotFound(name.to_string()))
    }
}

/// A Quokka VM instance owned by the page.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Quokka {
    host: BrowserHost,
}

impl Default for Quokka {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Quokka {
    /// Create a VM instance.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self {
            host: BrowserHost::default(),
        }
    }

    /// The banner the dev console shows on load.
    pub fn banner(&self) -> String {
        format!("Quokka {VERSION} — phase {PHASE} (wasm)")
    }

    /// Evaluate one line of input and return everything printed.
    pub fn eval(&mut self, input: &str) -> String {
        self.host.output.clear();
        // Phase 0: the reader and interpreter go here.
        self.host
            .write_stdout(&format!("; not implemented: {}\n", input.trim()));
        self.host.output.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_names_the_phase() {
        let q = Quokka::new();
        assert!(q.banner().contains("phase 0"));
    }

    #[test]
    fn eval_returns_host_output() {
        let mut q = Quokka::new();
        assert_eq!(q.eval("(fact 20)"), "; not implemented: (fact 20)\n");
    }
}
