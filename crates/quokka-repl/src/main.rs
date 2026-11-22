//! Quokka native terminal REPL.
//!
//! This binary is the native *host*: it owns stdin, stdout and the filesystem,
//! and hands them to the VM through [`quokka_vm::host::Host`]. All language
//! behaviour lives in `quokka-vm` and, above the primitive table, in Lisp.

use std::io::{self, BufRead, Write};

use quokka_vm::heap::{Heap, Register};
use quokka_vm::host::{Host, HostError};
use quokka_vm::printer::print;
use quokka_vm::reader::Reader;
use quokka_vm::vm::Outcome;
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

    // `quokka --save-image FILE` writes the booted world to a file, and
    // `quokka --image FILE` starts from one instead of booting. Together
    // they are phase 1's gate, in the form a person can actually hold:
    // the second start does not interpret the prelude at all.
    //
    // The VM produces and consumes bytes; the file is the host's business
    // (rule 2), which is also what lets the browser put the same bytes in
    // OPFS or hand them to the user as a download.
    if args.len() >= 3 && args[1] == "--save-image" {
        // The compiled world: prelude, macros and compiler all bytecode, and
        // a heap scan to prove nothing interpreted survived (#33).
        let mut heap = Heap::new();
        if let Err(e) = quokka_vm::boot::build_image(&mut heap) {
            eprintln!("quokka: building the image: {e}");
            std::process::exit(1);
        }
        // Any further arguments are source files loaded into the world before
        // it is saved, which is how an image comes to hold work rather than
        // just a boot. Read by the image's own reader and compiled by its own
        // compiler (#47) — the interpreter has nothing left to run in here.
        let mut native = NativeHost;
        for path in &args[3..] {
            let source = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("cannot read {path}: {e}");
                    std::process::exit(1);
                }
            };
            let mut failed = None;
            let ran = quokka_vm::vm::eval_source(
                &mut heap,
                &source,
                EVAL_BUDGET,
                2_000,
                &mut native,
                |outcome| match outcome {
                    Outcome::Value(_) => true,
                    Outcome::Error(e) => {
                        failed = Some(e);
                        false
                    }
                    Outcome::Failed(e) => {
                        failed = Some(format!("{e}"));
                        false
                    }
                },
            );
            if let Err(e) = ran {
                failed = Some(format!("{e}"));
            }
            if let Some(e) = failed {
                eprintln!("{path}: {e}");
                std::process::exit(1);
            }
        }
        let bytes = quokka_vm::image::save(&mut heap);
        if let Err(e) = std::fs::write(&args[2], &bytes) {
            eprintln!("cannot write {}: {e}", args[2]);
            std::process::exit(1);
        }
        println!(
            "{}: {} bytes, {} words, sequence {}",
            args[2],
            bytes.len(),
            heap.used_words(),
            heap.sequence()
        );
        return;
    }

    // `quokka --rebuild-image IN OUT` starts from the image IN, has it rebuild
    // itself from the source files on disk — read, compiled and installed by
    // its own Lisp (#48) — and saves the result as OUT. Run from the
    // repository's root, which is where the file names it asks for are.
    if args.len() == 4 && args[1] == "--rebuild-image" {
        let mut heap = load_image(&args[2]);
        let mut said = String::new();
        let ran = quokka_vm::vm::eval_source(
            &mut heap,
            "(quokka::%rebuild-world)",
            EVAL_BUDGET * 20,
            2_000,
            &mut NativeHost,
            |outcome| {
                said = format!("{outcome:?}");
                true
            },
        );
        if ran.is_err() || !said.starts_with("Value") {
            eprintln!("quokka: the rebuild failed: {said} {ran:?}");
            std::process::exit(1);
        }
        let bytes = quokka_vm::image::save(&mut heap);
        if let Err(e) = std::fs::write(&args[3], &bytes) {
            eprintln!("cannot write {}: {e}", args[3]);
            std::process::exit(1);
        }
        println!("{}: rebuilt, {said}; {} bytes", args[3], bytes.len());
        return;
    }

    // `quokka --compile-file FILE` compiles every form of a file and reports
    // what happened, without installing any of it. That last part is the
    // point rather than a shortcut — see `boot::compile_source` for why
    // installing into a running interpreter cannot work, and why it has to
    // wait for the image.
    if args.len() == 3 && args[1] == "--compile-file" {
        let source = match std::fs::read_to_string(&args[2]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot read {}: {e}", args[2]);
                std::process::exit(1);
            }
        };
        let mut heap = Heap::new();
        let prepared = quokka_vm::boot::install(&mut heap)
            .and_then(|()| quokka_vm::boot::install_compiler(&mut heap));
        if let Err(e) = prepared {
            eprintln!("quokka: {e}");
            std::process::exit(1);
        }
        match quokka_vm::boot::compile_source(&mut heap, &source) {
            Ok(tally) => {
                println!(
                    "{}: {} forms compiled and verified, {} macro definitions \
                     evaluated",
                    args[2], tally.compiled, tally.interpreted
                );
            }
            Err(e) => {
                eprintln!("{}: {e}", args[2]);
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

    // Either load a world or build one. Loading is what a finished Quokka
    // does always; building from source is the fallback that keeps the
    // system reproducible from `cargo` alone.
    let from_image = args.iter().position(|a| a == "--image").map(|i| i + 1);
    let mut heap = match from_image.and_then(|i| args.get(i)) {
        Some(path) => {
            let h = load_image(path);
            host.write_stdout(&format!(
                "Loaded {path}: {} words, sequence {}.\n",
                h.used_words(),
                h.sequence()
            ));
            h
        }
        None => {
            let mut heap = Heap::new();
            if let Err(e) = quokka_vm::boot::install(&mut heap) {
                eprintln!("quokka: {e}");
                std::process::exit(1);
            }
            if compiling {
                if let Err(e) = quokka_vm::boot::install_compiler(&mut heap) {
                    eprintln!("quokka: loading the compiler: {e}");
                    std::process::exit(1);
                }
            }
            heap
        }
    };
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
                let output = if from_image.is_some() {
                    image_print(&mut heap, &line, &mut host)
                } else if compiling {
                    compile_and_print(&mut heap, &line, showing, &mut host)
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

/// Read an image file into a heap, or say why not and stop.
fn load_image(path: &str) -> Heap {
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(1);
    });
    quokka_vm::image::load(&bytes, DEFAULT_IMAGE_LIMIT).unwrap_or_else(|e| {
        eprintln!("quokka: {path}: {e}");
        std::process::exit(1);
    })
}

/// Evaluate a line in a compiled image: read, evaluated and printed by the
/// image's own Lisp (#47), and the interpreter, the Rust reader and the Rust
/// printer not involved at all. There is no bootstrap rule here, because
/// there is nothing left to bootstrap.
fn image_print(heap: &mut Heap, line: &str, host: &mut NativeHost) -> String {
    let mut out = String::new();
    let ran = quokka_vm::vm::eval_source(heap, line, EVAL_BUDGET, 2_000, host, |outcome| {
        if !out.is_empty() {
            out.push('\n');
        }
        match outcome {
            Outcome::Value(text) => out.push_str(&text),
            Outcome::Error(message) => out.push_str(&format!("; {message}")),
            Outcome::Failed(e) => out.push_str(&format!("; {e}")),
        }
        true
    });
    if let Err(e) = ran {
        out.push_str(&format!("; {e}"));
    }
    out.push('\n');
    out
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

/// The soft limit a loaded image is given.
///
/// The host's policy, not the image's: see `Heap::from_image`. A native REPL
/// can afford a large heap; the browser will pass something smaller for the
/// same image.
const DEFAULT_IMAGE_LIMIT: usize = 1 << 22;

/// Read every form on the line, evaluate it, and print the result.
///
/// Sliced rather than one `mutate` for the whole line, so the collector can
/// run during a form as well as between them. A line that builds something
/// large would otherwise have to fit in the heap with all of its garbage,
/// which is issue #32 and cost this project three separate debugging
/// sessions before anyone looked at the mechanism.
fn read_and_print(heap: &mut Heap, line: &str) -> String {
    let mut out = String::new();
    let read = quokka_vm::eval::eval_forms(heap, line, EVAL_BUDGET, 2_000, |mc, result| {
        match result {
            Ok(value) => {
                out.push_str(&print(mc, value));
                out.push('\n');
            }
            Err(e) => out.push_str(&format!("; {e}\n")),
        }
        true
    });
    if let Err(e) = read {
        out.push_str(&format!("; {e}\n"));
    }
    out
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
fn compile_and_print(heap: &mut Heap, line: &str, showing: bool, host: &mut NativeHost) -> String {
    let mut out = String::new();
    let mut index = 0usize;
    loop {
        match compile_one(heap, line, index, showing, host) {
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
/// What the reading step decided to do with a form.
enum Next {
    /// There was no form here; the line is finished.
    Nothing,
    /// Compile it and run the result.
    Compile,
    /// Evaluate it with the interpreter instead — see `defines_a_macro`.
    Interpret,
    /// It could not be read.
    Failed(String),
}

fn compile_one(
    heap: &mut Heap,
    line: &str,
    index: usize,
    showing: bool,
    host: &mut NativeHost,
) -> Option<String> {
    let chars = Reader::chars_of(line);

    // Read the `index`th form, and either leave `(compile-form 'FORM)` in the
    // Value register or say that this one is not for compiling.
    let next = heap.mutate(|mc| {
        let mut reader = Reader::new(&chars);
        let mut form = None;
        for _ in 0..=index {
            match reader.read(mc) {
                Ok(Some(f)) => form = Some(f),
                Ok(None) => return Next::Nothing,
                Err(e) => return Next::Failed(format!("; {e}")),
            }
        }
        let Some(f) = form else {
            return Next::Nothing;
        };
        // `defmacro` is evaluated, not compiled, while the compiler is itself
        // interpreted: its expander will be called *by the compiler*, and
        // compiled code cannot be called from interpreted code — so an
        // expander compiled now could never run. #25 calls this the bootstrap
        // rule. The file compiler will own it; until there is one, the host
        // does.
        if defines_a_macro(mc, f) {
            return Next::Interpret;
        }
        let quoted = {
            let quote = mc.intern("QUOTE");
            let inner = mc.cons(f, quokka_vm::value::Value::NIL);
            mc.cons(quote, inner)
        };
        let call = {
            let compile = mc.intern("COMPILE-FORM");
            let args = mc.cons(quoted, quokka_vm::value::Value::NIL);
            mc.cons(compile, args)
        };
        mc.set_register(Register::Value, call);
        Next::Compile
    });

    match next {
        Next::Nothing => return None,
        Next::Failed(e) => return Some(e),
        Next::Interpret => return Some(interpret_one(heap, line, index)),
        Next::Compile => {}
    }

    // Drive the compilation a slice at a time, so the collector can run
    // between them — compiling allocates far more than evaluating does.
    if let Err(e) = quokka_vm::eval::eval_sliced(heap, EVAL_BUDGET, 2_000) {
        return Some(format!("; compiling: {e}"));
    }

    // Run it. The compiled function is the root, so it survives any
    // collection the run itself needs.
    let mut out = String::new();
    if showing {
        out = heap.mutate(|mc| {
            let function = mc.register(Register::Value);
            quokka_vm::code::disassemble_function(mc, function)
        });
    }
    // A slice at a time, answering the program's host requests between
    // slices: this is where a compiled `(%read-source "file.lisp")` reaches
    // the real filesystem. The native host answers at once; the protocol
    // would let it answer later (#35).
    match quokka_vm::vm::run_sliced(heap, EVAL_BUDGET, 2_000, host) {
        Ok(()) => out.push_str(&heap.mutate(|mc| print(mc, mc.register(Register::Value)))),
        Err(e) => out.push_str(&format!("; {e}")),
    }
    Some(out)
}

/// Does this form define a macro?
///
/// `defmacro` is a special operator here, not a macro, so this is a look at
/// the head and nothing cleverer.
fn defines_a_macro<'h>(
    mc: &quokka_vm::heap::Mutation<'h>,
    form: quokka_vm::value::Value<'h>,
) -> bool {
    form.is_cons() && mc.symbol_is(mc.car(form), "DEFMACRO")
}

/// Evaluate the `index`th form on the line with the interpreter, for the
/// forms the compiling REPL must not compile.
fn interpret_one(heap: &mut Heap, line: &str, index: usize) -> String {
    let mut out = String::new();
    let mut seen = 0usize;
    let _ = quokka_vm::eval::eval_forms(heap, line, EVAL_BUDGET, 2_000, |mc, result| {
        if seen == index {
            match result {
                Ok(v) => out.push_str(&print(mc, v)),
                Err(e) => out.push_str(&format!("; {e}")),
            }
        }
        seen += 1;
        seen <= index
    });
    out
}
