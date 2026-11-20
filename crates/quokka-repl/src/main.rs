//! Quokka native terminal REPL.
//!
//! This binary is the native *host*: it owns stdin, stdout and the filesystem,
//! and hands them to the VM through [`quokka_vm::host::Host`]. All language
//! behaviour lives in `quokka-vm` and, above the primitive table, in Lisp.

use std::io::{self, BufRead, Write};

use quokka_vm::heap::Heap;
use quokka_vm::host::{Host, HostError};
use quokka_vm::printer::print;
use quokka_vm::reader::Reader;
use quokka_vm::{PHASE, VERSION};

/// The native host: real files, real console.
struct NativeHost;

impl Host for NativeHost {
    fn write_stdout(&mut self, s: &str) {
        print!("{s}");
        let _ = io::stdout().flush();
    }

    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        std::fs::read_to_string(name).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => HostError::NotFound(name.to_string()),
            io::ErrorKind::PermissionDenied => HostError::Denied(name.to_string()),
            _ => HostError::Other(e.to_string()),
        })
    }
}

fn main() {
    let mut host = NativeHost;
    host.write_stdout(&format!("Quokka {VERSION} — phase {PHASE}\n"));
    host.write_stdout("Read and print only — there is no evaluator yet. Ctrl-D to leave.\n");

    let mut heap = Heap::new();
    let stdin = io::stdin();
    loop {
        host.write_stdout("quokka> ");
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                host.write_stdout("\n");
                return;
            }
            Ok(_) => {
                let output = read_and_print(&mut heap, &line);
                host.write_stdout(&output);

                // Between mutations is the only place collection may happen,
                // so this is where the heap's request gets answered.
                if heap.wants_collection() {
                    heap.collect();
                }
            }
            Err(e) => {
                host.write_stdout(&format!("; input error: {e}\n"));
                return;
            }
        }
    }
}

/// Read every form on the line and print each one back.
///
/// All of it happens inside one `mutate`, which is what makes holding the
/// values safe: no collection can happen part-way through. Only the finished
/// text leaves, because a `Value` cannot.
fn read_and_print(heap: &mut Heap, line: &str) -> String {
    let chars = Reader::chars_of(line);
    heap.mutate(|mc| {
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
    })
}
