//! `quokka bootstrap`: the tier-0 interpreter and the bootstrap compiler.
//!
//! How the first world gets made, and where the interpreter and the
//! compiler are worked on. Not Quokka itself: no packages, no Lisp reader,
//! printer or `eval`.

use quokka_bootstrap::reader::Reader;
use quokka_vm::heap::{Heap, Register};
use quokka_vm::host::Host;
use quokka_vm::printer::print;
use quokka_vm::VERSION;

use crate::host::NativeHost;
use crate::world::{repl, EVAL_BUDGET};
use crate::{source_of, usage_error};

/// `quokka bootstrap ...`: the tier-0 interpreter, and the bootstrap
/// compiler running on it. This is how the first world gets made, and where
/// the interpreter and the compiler are worked on; it is not Quokka.
pub(crate) fn bootstrap(args: &[String]) {
    match args {
        // Interpret a file and print the differential format, which is how
        // tier-0 baselines are produced.
        [flag, file] if flag == "--run" => {
            let source = source_of(file);
            print!("{}", quokka_bootstrap::run_forms(&source, EVAL_BUDGET));
        }
        // Compile every form of a file and report what happened, installing
        // none of it — see `boot::compile_source` for why installing into a
        // running interpreter cannot work.
        [flag, file] if flag == "--compile-file" => compile_file(file),
        [] => bootstrap_repl(false, false),
        [flag] if flag == "--compile" => bootstrap_repl(true, false),
        [flag] if flag == "--show-code" => bootstrap_repl(true, true),
        _ => {
            usage_error("bootstrap takes --compile, --show-code, --run FILE or --compile-file FILE")
        }
    }
}

pub(crate) fn compile_file(file: &str) {
    let source = source_of(file);
    let mut heap = Heap::new();
    let prepared = quokka_bootstrap::boot::install(&mut heap)
        .and_then(|()| quokka_bootstrap::boot::install_compiler(&mut heap));
    if let Err(e) = prepared {
        eprintln!("quokka: {e}");
        std::process::exit(1);
    }
    match quokka_bootstrap::boot::compile_source(&mut heap, &source) {
        Ok(tally) => println!(
            "{file}: {} forms compiled and verified, {} macro definitions evaluated",
            tally.compiled, tally.interpreted
        ),
        Err(e) => {
            eprintln!("{file}: {e}");
            std::process::exit(1);
        }
    }
}

/// The interpreter's REPL. `compiling` loads the compiler and compiles each
/// form for the VM instead of interpreting it; `showing` also prints each
/// form's instructions first, which is how to look at what the compiler made.
pub(crate) fn bootstrap_repl(compiling: bool, showing: bool) {
    let mut host = NativeHost;
    host.write_stdout(&format!(
        "Quokka {VERSION} — bootstrap · the tier-0 interpreter, which makes the first world\n\
         Not Quokka itself: no packages, no Lisp reader, printer or eval. \
         To run Quokka, give it a world: quokka WORLD.qi\n"
    ));
    if compiling {
        host.write_stdout("Compiling each form with the bootstrap compiler, for the VM.\n");
        if showing {
            host.write_stdout("Showing the instructions for each form.\n");
        }
    }
    host.write_stdout("Ctrl-D leaves.\n");
    let mut heap = Heap::new();
    if let Err(e) = quokka_bootstrap::boot::install(&mut heap) {
        eprintln!("quokka: {e}");
        std::process::exit(1);
    }
    if compiling {
        if let Err(e) = quokka_bootstrap::boot::install_compiler(&mut heap) {
            eprintln!("quokka: loading the compiler: {e}");
            std::process::exit(1);
        }
    }
    repl(
        &mut heap,
        "boot> ",
        // The Rust reader says whether the text is whole: it is the reader
        // this REPL evaluates with.
        |heap, text| {
            let chars = Reader::chars_of(text);
            heap.mutate(|mc| {
                let mut reader = Reader::new(&chars);
                loop {
                    match reader.read(mc) {
                        Ok(Some(_)) => {}
                        Err(quokka_bootstrap::reader::ReadError::UnexpectedEnd { .. }) => {
                            return false
                        }
                        Ok(None) | Err(_) => return true,
                    }
                }
            })
        },
        |heap, text| {
            if compiling {
                compile_and_print(heap, text, showing, &mut NativeHost)
            } else {
                read_and_print(heap, text)
            }
        },
    );
}

/// Read every form on the line, evaluate it, and print the result.
///
/// Sliced rather than one `mutate` for the whole line, so the collector can
/// run during a form as well as between them. A line that builds something
/// large would otherwise have to fit in the heap with all of its garbage,
/// which is issue #32 and cost this project three separate debugging
/// sessions before anyone looked at the mechanism.
pub(crate) fn read_and_print(heap: &mut Heap, line: &str) -> String {
    let mut out = String::new();
    let read = quokka_bootstrap::eval::eval_forms(heap, line, EVAL_BUDGET, 2_000, |mc, result| {
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
/// `boot::compile_each` reads, compiles and verifies each form, a slice at
/// a time so the collector can run while it does, and evaluates a
/// `defmacro` with the interpreter instead (the bootstrap rule: an expander
/// compiled now could not be called by the interpreted compiler). What this
/// adds is what a REPL wants of each function: its instructions, if asked,
/// and its value.
pub(crate) fn compile_and_print(
    heap: &mut Heap,
    line: &str,
    showing: bool,
    host: &mut NativeHost,
) -> String {
    use quokka_bootstrap::boot::Handled;
    let mut out = String::new();
    let read = quokka_bootstrap::boot::compile_each(heap, line, |heap, outcome| {
        match outcome {
            Err(e) => out.push_str(&format!("; compiling: {e}")),
            Ok(Handled::Evaluated) => {
                out.push_str(&heap.mutate(|mc| print(mc, mc.register(Register::Value))));
            }
            Ok(Handled::Compiled) => {
                if showing {
                    out.push_str(&heap.mutate(|mc| {
                        let function = mc.register(Register::Value);
                        quokka_vm::code::disassemble_function(mc, function)
                    }));
                }
                // A slice at a time, answering the program's host requests
                // between slices: this is where a compiled
                // `(%read-source "file.lisp")` reaches the real filesystem.
                match quokka_vm::vm::run_sliced(heap, EVAL_BUDGET, 2_000, host) {
                    Ok(()) => {
                        out.push_str(&heap.mutate(|mc| print(mc, mc.register(Register::Value))))
                    }
                    Err(e) => out.push_str(&format!("; {e}")),
                }
            }
        }
        out.push('\n');
        true
    });
    if let Err(e) = read {
        out.push_str(&format!("; {e}\n"));
    }
    out
}
