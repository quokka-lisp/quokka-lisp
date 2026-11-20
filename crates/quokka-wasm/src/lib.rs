//! Quokka WASM bindings.
//!
//! This crate is the browser *host*. It implements
//! [`quokka_vm::host::Host`] over the page and exposes a small surface to
//! JavaScript for the dev console in `www/`.

use quokka_vm::heap::Heap;
use quokka_vm::host::{Host, HostError};
use quokka_vm::printer::print;
use quokka_vm::reader::Reader;
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
    heap: Heap,
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
            heap: Heap::new(),
        }
    }

    /// The banner the dev console shows on load.
    pub fn banner(&self) -> String {
        format!("Quokka {VERSION} — phase {PHASE} (wasm)")
    }

    /// Read every form on the line and print each one back.
    ///
    /// Not evaluation — there is no evaluator yet. The reader and printer are
    /// enough to prove the browser build does real work.
    pub fn eval(&mut self, input: &str) -> String {
        self.host.output.clear();
        let chars = Reader::chars_of(input);
        let text = self.heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let mut out = String::new();
            loop {
                match reader.read(mc) {
                    Ok(None) => break,
                    Ok(Some(form)) => {
                        out.push_str(&print(mc, form));
                        out.push('\n');
                    }
                    Err(e) => {
                        out.push_str(&format!("; {e}\n"));
                        break;
                    }
                }
            }
            out
        });
        if self.heap.wants_collection() {
            self.heap.collect();
        }
        self.host.write_stdout(&text);
        self.host.output.clone()
    }

    /// How many words the heap is using. The dev console shows it, so the
    /// collector is visible rather than theoretical.
    pub fn heap_words(&self) -> usize {
        self.heap.used_words()
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
    fn eval_reads_and_prints() {
        let mut q = Quokka::new();
        assert_eq!(q.eval("(fact 20)"), "(FACT 20)\n");
        assert_eq!(q.eval("'(1 2 . 3)"), "(QUOTE (1 2 . 3))\n");
        assert!(q.eval("(oops").starts_with("; input ended"));
        assert!(q.heap_words() > 0);
    }
}
