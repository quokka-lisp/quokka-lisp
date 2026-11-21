//! Quokka native terminal REPL.
//!
//! This binary is the native *host*: it owns stdin, stdout and the filesystem,
//! and hands them to the VM through [`quokka_vm::host::Host`]. All language
//! behaviour lives in `quokka-vm` and, above the primitive table, in Lisp.

use std::io::{self, BufRead, Write};

use quokka_vm::eval::{eval_now, Machine};
use quokka_vm::heap::Heap;
use quokka_vm::host::{Host, HostError};
use quokka_vm::printer::print;
use quokka_vm::reader::Reader;
use quokka_vm::slice::{run_slice, Status};
use quokka_vm::vm::run_now;
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

    // `quokka --compile` loads the compiler and evaluates by *compiling*:
    // each form becomes a function and the bytecode VM runs it. The ordinary
    // REPL interprets, and does not load the compiler at all.
    // `--show-code` also prints each form's instructions before running
    // them, which is how to look at what the compiler actually produced.
    let showing = args.iter().any(|a| a == "--show-code");
    let compiling = showing || args.iter().any(|a| a == "--compile");

    let mut host = NativeHost;
    host.write_stdout(&format!("Quokka {VERSION} — phase {PHASE}\n"));
    if compiling {
        host.write_stdout("Compiling: every form is compiled and run on the VM.\n");
        if showing {
            host.write_stdout("Showing the instructions for each form.\n");
        }
    }
    host.write_stdout("Ctrl-D to leave.\n");

    let mut heap = Heap::new();
    if let Err(e) = quokka_vm::boot::install(&mut heap) {
        eprintln!("quokka: {e}");
        std::process::exit(1);
    }
    if compiling {
        let loaded = heap.mutate(quokka_vm::boot::install_compiler);
        if let Err(e) = loaded {
            eprintln!("quokka: loading the compiler: {e}");
            std::process::exit(1);
        }
    }
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
                let output = if compiling {
                    compile_and_print(&mut heap, &line, showing)
                } else {
                    read_and_print(&mut heap, &line)
                };
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

/// Evaluate every form on the line by compiling it and running the result.
///
/// One form at a time, and each one driven a slice at a time with the heap
/// free to collect in between — which is not fussiness. Collection cannot
/// happen inside a `mutate`, and compiling allocates far more than evaluating
/// does: an instruction list, a constants list, a `gensym` per branch and a
/// fresh `append` for every concatenation, nearly all of it garbage at once.
/// The interpreting path above can afford one `mutate` per line and this
/// cannot.
///
/// The line is re-read for each form rather than its forms being carried
/// across mutations, because a `Value` cannot leave one and a REPL line is
/// short enough that reading it twice costs nothing.
fn compile_and_print(heap: &mut Heap, line: &str, showing: bool) -> String {
    let mut out = String::new();
    let mut index = 0usize;
    loop {
        match compile_one(heap, line, index, showing) {
            None => return out,
            Some(text) => {
                out.push_str(&text);
                out.push('\n');
                index += 1;
            }
        }
    }
}

/// Compile and run the `index`th form on the line, or `None` if there is none.
fn compile_one(heap: &mut Heap, line: &str, index: usize, showing: bool) -> Option<String> {
    let chars = Reader::chars_of(line);

    // Build `(compile-form 'FORM)` and start the interpreter on it.
    let started = heap.mutate(|mc| {
        let mut reader = Reader::new(&chars);
        let mut form = None;
        for _ in 0..=index {
            form = match reader.read(mc) {
                Ok(Some(f)) => Some(Ok(f)),
                Ok(None) => return None,
                Err(e) => Some(Err(format!("; {e}"))),
            };
            if matches!(form, Some(Err(_))) {
                break;
            }
        }
        match form {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok(f)) => {
                let quote = mc.intern("QUOTE");
                let quoted = {
                    let inner = mc.cons(f, quokka_vm::value::Value::NIL);
                    mc.cons(quote, inner)
                };
                let compile = mc.intern("COMPILE-FORM");
                let call = {
                    let args = mc.cons(quoted, quokka_vm::value::Value::NIL);
                    mc.cons(compile, args)
                };
                Machine::start(mc, call, quokka_vm::value::Value::NIL);
                Some(Ok(()))
            }
        }
    });

    match started {
        None => return None,
        Some(Err(e)) => return Some(e),
        Some(Ok(())) => {}
    }

    // Drive the compilation, collecting between slices.
    loop {
        let outcome = heap.mutate(|mc| {
            let (status, error, value) = {
                let mut machine = Machine::resume(mc);
                let status = run_slice(&mut machine, 2_000);
                (status, machine.error.take(), machine.result())
            };
            if let Some(e) = error {
                return Some(Err(format!("; compiling: {e}")));
            }
            if matches!(status, Status::Done { .. }) {
                mc.set_root(value);
                return Some(Ok(()));
            }
            None
        });
        if heap.wants_collection() || heap.is_exhausted() {
            heap.collect();
            heap.clear_exhausted();
        }
        match outcome {
            Some(Err(e)) => return Some(e),
            Some(Ok(())) => break,
            None => {}
        }
    }

    // Run it. The compiled function is the root, so it survives any
    // collection the run itself needs.
    Some(heap.mutate(|mc| {
        let function = mc.root();
        let mut out = String::new();
        if showing {
            out.push_str(&quokka_vm::code::disassemble_function(mc, function));
        }
        match run_now(mc, function, EVAL_BUDGET) {
            Ok(v) => out.push_str(&print(mc, v)),
            Err(e) => out.push_str(&format!("; {e}")),
        }
        out
    }))
}
