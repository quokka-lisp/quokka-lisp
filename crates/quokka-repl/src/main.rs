//! Quokka native terminal REPL.
//!
//! This binary is the native *host*: it owns stdin, stdout and the filesystem,
//! and hands them to the VM through [`quokka_vm::host::Host`]. All language
//! behaviour lives in `quokka-vm` and, above the primitive table, in Lisp.

use std::io::{self, BufRead, Write};

use quokka_vm::eval::eval_now;
use quokka_vm::heap::Heap;
use quokka_vm::host::{Host, HostError};
use quokka_vm::primitives;
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
    // `quokka --run FILE` evaluates a file and prints the differential testing
    // format, which is how baselines are produced. Everything else is a REPL.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--run" {
        match std::fs::read_to_string(&args[2]) {
            Ok(source) => print!("{}", quokka_vm::run_forms(&source, EVAL_BUDGET)),
            Err(e) => {
                eprintln!("cannot read {}: {e}", args[2]);
                std::process::exit(1);
            }
        }
        return;
    }

    let mut host = NativeHost;
    host.write_stdout(&format!("Quokka {VERSION} — phase {PHASE}\n"));
    host.write_stdout("Ctrl-D to leave.\n");

    let mut heap = Heap::new();
    heap.mutate(primitives::install);
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
                // so this is where the heap's request gets answered — and
                // where a heap that hit its ceiling gets a chance to recover,
                // since whatever was filling it is now unreachable.
                if heap.wants_collection() || heap.is_exhausted() {
                    heap.collect();
                    heap.clear_exhausted();
                }
            }
            Err(e) => {
                host.write_stdout(&format!("; input error: {e}\n"));
                return;
            }
        }
    }
}

/// How many evaluator steps one line may take before giving up.
///
/// Without an interrupt there is nothing else to stop a program that never
/// finishes, and a REPL that hangs is the failure the slice machinery exists to
/// prevent. Phase 3 replaces this with Ctrl-C landing in the debugger.
///
/// Five million rather than fifty: the old figure let `(fact -1)` run for
/// nineteen seconds before saying anything. A line that needs more than this is
/// rare enough to be worth noticing, and the heap ceiling usually arrives first
/// anyway.
const EVAL_BUDGET: u64 = 5_000_000;

/// Read every form on the line, evaluate it, and print the result.
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
                Ok(Some(form)) => match eval_now(mc, form, EVAL_BUDGET) {
                    Ok(value) => {
                        out.push_str(&print(mc, value));
                        out.push('\n');
                    }
                    Err(e) => {
                        out.push_str(&format!("; {e}\n"));
                    }
                },
                Err(e) => {
                    out.push_str(&format!("; {e}\n"));
                    break;
                }
            }
        }
        out
    })
}
