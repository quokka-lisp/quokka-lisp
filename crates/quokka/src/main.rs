//! `quokka`: the native program that runs a world, and makes one.
//!
//! This binary is the native *host*: it owns stdin, stdout and the filesystem,
//! and hands them to the VM through [`quokka_vm::host::Host`]. All language
//! behaviour lives in `quokka-vm` and, above the primitive table, in Lisp.
//!
//! Three modules, one concern each: [`host`] is what a world may ask of this
//! machine, [`world`] runs and saves a world, [`bootstrap`] is the
//! interpreter that makes the first one. What is left here is the command
//! line.

mod bootstrap;
mod host;
mod world;

use quokka_vm::{PHASE, VERSION};

/// What to type, printed when the arguments make no sense, or are missing.
const USAGE: &str = "\
usage:
  quokka WORLD.qi                       run a world: read, evaluated and printed
                                        by its own Lisp, compiled, tier 1
  quokka build OUT.qi [SRC...]          make a world from nothing (the cold build),
                                        with SRC files loaded into it, and save it
  quokka rebuild IN.qi OUT.qi           have a world rebuild itself from lisp/ on
                                        disk (run from the repository's root)
  quokka bootstrap [--compile | --show-code]
                                        the tier-0 interpreter's REPL, which makes
                                        the first world; not Quokka itself
  quokka bootstrap --run FILE           interpret FILE, in the differential format
  quokka bootstrap --compile-file FILE  compile FILE with the bootstrap compiler
  quokka --version                      which Quokka this is

A form typed at a world is given up after 5,000,000 steps, about a third
of a second, since nothing can interrupt one yet:
  --budget N          the steps a form may take; 0 for as many as it takes

A world may read files under the current directory and nowhere else:
  --root DIR          the directory it may read, in place of the current one
  --allow-read PATH   a further file or directory it may read; may be repeated
";

/// Say what went wrong and stop, with the status of a failure. One spelling
/// for every such message: it was "cannot read …" in four places and
/// "quokka: …" in the rest (#121).
fn die(message: &str) -> ! {
    eprintln!("quokka: {message}");
    std::process::exit(1);
}

/// The text of a file named on the command line, or stop.
fn source_of(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| die(&format!("cannot read {path}: {e}")))
}

/// Say what went wrong, then how to use the program, and stop.
fn usage_error(message: &str) -> ! {
    eprintln!("quokka: {message}\n\n{USAGE}");
    std::process::exit(2);
}

/// `quokka` runs a world (#51). Everything that makes or remakes one is a
/// subcommand, and the interpreter that makes the first one is behind
/// `bootstrap`, so the plain command is never the least capable mode.
fn main() {
    let args = host::grant(std::env::args().skip(1).collect());
    match args.first().map(String::as_str) {
        None => usage_error(
            "no world to run. Quokka runs from an image: make one with \
             `quokka build world.qi`, then run `quokka world.qi`.",
        ),
        Some("help" | "--help" | "-h") => print!("{USAGE}"),
        Some("--version" | "-V") => println!("quokka {VERSION} (phase {PHASE})"),
        Some("build") => world::build(&args[1..]),
        Some("rebuild") => world::rebuild(&args[1..]),
        Some("bootstrap") => bootstrap::bootstrap(&args[1..]),
        Some(flag) if flag.starts_with('-') => usage_error(&format!("unknown option {flag}")),
        Some(world) if args.len() == 1 => world::run_world(world),
        Some(_) => usage_error("a world is run on its own: `quokka WORLD.qi`"),
    }
}
