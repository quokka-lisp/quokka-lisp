//! `quokka`: the native program that runs a world, and makes one.
//!
//! This binary is the native *host*: it owns stdin, stdout and the filesystem,
//! and hands them to the VM through [`quokka_vm::host::Host`]. All language
//! behaviour lives in `quokka-vm` and, above the primitive table, in Lisp.

use std::io::{self, BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use quokka_bootstrap::reader::Reader;
use quokka_vm::heap::{Heap, Register};
use quokka_vm::host::{Host, HostError};
use quokka_vm::printer::print;
use quokka_vm::vm::Outcome;
use quokka_vm::{PHASE, VERSION};

/// The native host: real files, real console.
struct NativeHost;

impl Host for NativeHost {
    fn write_stdout(&mut self, s: &str) {
        print!("{s}");
        let _ = io::stdout().flush();
    }

    /// A source file, for a world that asks by name.
    ///
    /// Only one under a directory this world was granted ([`permitted`]),
    /// only a regular file, and only one of a size a source file has (#87).
    /// A world's request is answered between slices, where no step budget
    /// applies: a FIFO would hold the host for ever, and a 100 MB file
    /// became a string of 1.7 GB.
    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        let grants = GRANTS.get().map(Vec::as_slice).unwrap_or(&[]);
        // A relative name is relative to the root, which is the first
        // grant: the world's idea of "here" is the directory it was given.
        let Some(base) = grants.first() else {
            return Err(HostError::Denied(format!(
                "{name}: this world may read nothing"
            )));
        };
        // What is opened is the path that was checked, with every link
        // already followed — not the name, which could be made to mean
        // something else between the check and the read.
        let file = permitted(grants, base, name)?;
        let name = file.to_str().unwrap_or(name);
        match std::fs::metadata(name) {
            Ok(file) if !file.is_file() => {
                return Err(HostError::Denied(format!("{name}, which is not a file")))
            }
            Ok(file) if file.len() > SOURCE_LIMIT => {
                return Err(HostError::Denied(format!(
                    "{name}, which at {} bytes is too large to be source",
                    file.len()
                )))
            }
            // Missing or unreadable: the read below says which.
            _ => {}
        }
        std::fs::read_to_string(name).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => HostError::NotFound(name.to_string()),
            io::ErrorKind::PermissionDenied => HostError::Denied(name.to_string()),
            _ => HostError::Other(e.to_string()),
        })
    }
}

/// The directories this process's worlds may read from: the current one
/// unless `--root` named another, and whatever `--allow-read` added. Set
/// once, in `main`, before any world runs; a host answers for the process.
static GRANTS: OnceLock<Vec<PathBuf>> = OnceLock::new();

/// The file NAME names, if a world granted GRANTS may read it.
///
/// **A world reaches the outside only through its host, and a host grants a
/// directory, never the user's whole authority** (#87). Images are meant to
/// be shared, and opening one runs its code; `%read-source` used to read
/// any file the user could, so a downloaded world could read `~/.ssh`. This
/// is the *capability* shape — authority is what was handed over, not what
/// the user could do — that WASI's preopened directories and Deno's
/// `--allow-read` have, and that a `local:/` file host will stand on.
///
/// The check is on where the name **leads**, not on how it is spelt. `..`
/// and an absolute path are the obvious ways out and a symbolic link is the
/// quiet one: `root/notes` is inside the root as text and may be a link to
/// `/etc`. So the name is resolved first — `canonicalize` follows every
/// link and every `..`, as the kernel would — and it is the resolved path
/// that must lie under a grant, and the resolved path that is then opened.
/// The grants were resolved the same way when they were given.
///
/// A name that leads nowhere still must not say more than "not found"
/// about a place the world may not look. So for a missing file the nearest
/// directory that does exist is resolved and checked: outside the grants
/// the answer is the refusal, the same as for a file that is there.
fn permitted(grants: &[PathBuf], base: &Path, name: &str) -> Result<PathBuf, HostError> {
    let inside = |real: &Path| grants.iter().any(|grant| real.starts_with(grant));
    let refused = || {
        HostError::Denied(format!(
            "{name}, which is outside what this world may read \
             (--root DIR or --allow-read PATH grants more)"
        ))
    };
    let asked = base.join(name);
    if let Ok(real) = asked.canonicalize() {
        return if inside(&real) {
            Ok(real)
        } else {
            Err(refused())
        };
    }
    // It does not exist. Take `.` and `..` out as text, then find the
    // deepest ancestor that does exist and see where *that* really is.
    let mut plain = PathBuf::new();
    for part in asked.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                plain.pop();
            }
            other => plain.push(other),
        }
    }
    let exists = plain.ancestors().find_map(|a| a.canonicalize().ok());
    match exists {
        Some(real) if inside(&real) => Err(HostError::NotFound(name.to_string())),
        _ => Err(refused()),
    }
}

/// Take `--root DIR` and `--allow-read PATH` out of the arguments and make
/// them the process's grants; what is left is the command.
fn grant(arguments: Vec<String>) -> Vec<String> {
    let resolve = |flag: &str, path: Option<String>| -> PathBuf {
        let Some(path) = path else {
            usage_error(&format!("{flag} wants a path after it"));
        };
        // Resolved now, links and all, so that a grant is a real place and
        // the comparison in `permitted` is between two real places.
        std::fs::canonicalize(&path).unwrap_or_else(|e| usage_error(&format!("{flag} {path}: {e}")))
    };
    let (mut root, mut more, mut rest) = (None, Vec::new(), Vec::new());
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => root = Some(resolve("--root", arguments.next())),
            "--allow-read" => more.push(resolve("--allow-read", arguments.next())),
            _ => rest.push(argument),
        }
    }
    let root = root.unwrap_or_else(|| resolve("the current directory", Some(".".to_string())));
    more.insert(0, root);
    GRANTS.set(more).expect("the grants are set once");
    rest
}

/// The largest file a world may read as source: sixteen megabytes, about
/// forty times all of Quokka's own Lisp.
const SOURCE_LIMIT: u64 = 16 << 20;

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

A world may read files under the current directory and nowhere else:
  --root DIR          the directory it may read, in place of the current one
  --allow-read PATH   a further file or directory it may read; may be repeated
";

/// Say what went wrong, then how to use the program, and stop.
fn usage_error(message: &str) -> ! {
    eprintln!("quokka: {message}\n\n{USAGE}");
    std::process::exit(2);
}

/// `quokka` runs a world (#51). Everything that makes or remakes one is a
/// subcommand, and the interpreter that makes the first one is behind
/// `bootstrap`, so the plain command is never the least capable mode.
fn main() {
    let args = grant(std::env::args().skip(1).collect());
    match args.first().map(String::as_str) {
        None => usage_error(
            "no world to run. Quokka runs from an image: make one with \
             `quokka build world.qi`, then run `quokka world.qi`.",
        ),
        Some("help" | "--help" | "-h") => print!("{USAGE}"),
        Some("--version" | "-V") => println!("quokka {VERSION} (phase {PHASE})"),
        Some("build") => build(&args[1..]),
        Some("rebuild") => rebuild(&args[1..]),
        Some("bootstrap") => bootstrap(&args[1..]),
        Some(flag) if flag.starts_with('-') => usage_error(&format!("unknown option {flag}")),
        Some(world) if args.len() == 1 => run_world(world),
        Some(_) => usage_error("a world is run on its own: `quokka WORLD.qi`"),
    }
}

/// `quokka build OUT [SRC...]`: the cold build, then any SRC files loaded into
/// the new world, then saved.
///
/// The VM produces and consumes bytes; the file is the host's business
/// (rule 2), which is also what lets the browser put the same bytes in OPFS
/// or hand them to the user as a download.
fn build(args: &[String]) {
    let Some((out, sources)) = args.split_first() else {
        usage_error("build needs a file to save the world in");
    };
    // The compiled world: prelude, macros and compiler all bytecode, and a
    // heap scan to prove nothing interpreted survived (#33).
    let mut heap = Heap::new();
    if let Err(e) = quokka_bootstrap::boot::build_image(&mut heap) {
        eprintln!("quokka: building the world: {e}");
        std::process::exit(1);
    }
    // Further files are loaded into the world before it is saved, which is
    // how a world comes to hold work rather than just a boot. Read by the
    // world's own reader and compiled by its own compiler (#47).
    for path in sources {
        let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(1);
        });
        let mut failed = None;
        let ran = quokka_vm::vm::eval_source(
            &mut heap,
            &source,
            EVAL_BUDGET,
            2_000,
            &mut NativeHost,
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
    save_world(&mut heap, out).unwrap_or_else(|e| {
        eprintln!("quokka: {e}");
        std::process::exit(1);
    });
}

/// `quokka rebuild IN OUT`: the world in IN rebuilds itself from the source
/// files on disk — read, compiled and installed by its own Lisp (#48) — and
/// is saved as OUT.
fn rebuild(args: &[String]) {
    let [from, to] = args else {
        usage_error("rebuild needs the world to start from and a file to save the result in");
    };
    let mut heap = load_image(from);
    let mut said = String::new();
    let ran = quokka_vm::vm::eval_source(
        &mut heap,
        "(quokka::%rebuild-world)",
        // As `differential.rs` gives it: a rebuild compiles every form of the
        // system. 20 ran out at floats (#17), 40 at step AB (#75, ~201M steps).
        EVAL_BUDGET * 60,
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
    println!("rebuilt: {said}");
    save_world(&mut heap, to).unwrap_or_else(|e| {
        eprintln!("quokka: {e}");
        std::process::exit(1);
    });
}

/// Save a world to a file, and say so.
///
/// By **atomic replace** (#120): the bytes go to a new file beside the
/// target, are forced to the disk, and only then take the target's name.
/// A rename within one directory either happens or does not, so whatever
/// interrupts a save — Ctrl-C, a full disk, the power — leaves the old world
/// or the new one and never half of either. Written in place, as this used
/// to be, an interrupted `:save` over the file the session came from
/// destroyed the only copy. Until something records work between saves
/// (#24), the save is all the durability there is.
///
/// And it replaces only a world. `:save` takes whatever name is typed, so a
/// slip of the fingers used to overwrite any file at all.
fn save_world(heap: &mut Heap, path: &str) -> Result<(), String> {
    if let Ok(mut existing) = std::fs::File::open(path) {
        let mut start = [0u8; 8];
        let is_world = existing.read_exact(&mut start).is_ok()
            && u64::from_le_bytes(start) == quokka_vm::image::MAGIC;
        if !is_world {
            return Err(format!(
                "{path} exists and is not a world, so it is left alone; \
                 save under another name, or remove it first"
            ));
        }
    }
    let bytes = quokka_vm::image::save(heap);
    let temporary = format!("{path}.saving");
    let written = std::fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(&bytes)?;
        // Without this the rename can reach the disk before the bytes do,
        // and a crash then leaves the new name on an empty file.
        file.sync_all()
    });
    if let Err(e) = written.and_then(|()| std::fs::rename(&temporary, path)) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("cannot write {path}: {e}"));
    }
    println!(
        "{path}: {} bytes, {} words, sequence {}",
        bytes.len(),
        heap.used_words(),
        heap.sequence()
    );
    Ok(())
}

/// `quokka WORLD`: a REPL in a world. Every form is read, evaluated and
/// printed by the world's own Lisp; the interpreter, the Rust reader and the
/// Rust printer play no part. `:save FILE` saves the world between forms.
fn run_world(path: &str) {
    let mut heap = load_image(path);
    let mut host = NativeHost;
    host.write_stdout(&format!(
        "Quokka {VERSION} — phase {PHASE}\n\
         {path}: {} words, sequence {} · compiled · tier 1\n\
         :save FILE saves this world · Ctrl-D leaves\n",
        heap.used_words(),
        heap.sequence()
    ));
    repl(
        &mut heap,
        "quokka> ",
        |heap, text| {
            // A command is whole on its line. Anything else is whole when
            // the world's own reader says so; if the reader cannot be asked,
            // evaluate what there is and let that report the trouble.
            world_command(text).is_some()
                || quokka_vm::vm::source_complete(heap, text, EVAL_BUDGET, 2_000, &mut NativeHost)
                    .unwrap_or(true)
        },
        |heap, text| match world_command(text) {
            Some(command) => run_command(heap, command),
            None => image_print(heap, text, &mut NativeHost),
        },
    );
}

/// The arguments of a REPL command, if `text` is one. There is one command,
/// `:save`, and only a line that begins with it is a command: every other
/// `:word` is a keyword, a form like any other, whose value is itself. The
/// REPL used to take any line beginning with a colon, so `:foo` answered
/// "no command :foo" where Lisp answers `:FOO` (#119).
fn world_command(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix(":save")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then_some(rest)
}

/// Run a REPL command: handled by the host between forms, so the world is
/// never saved in the middle of a computation (#51).
fn run_command(heap: &mut Heap, arguments: &str) -> String {
    let mut words = arguments.split_whitespace();
    match (words.next(), words.next()) {
        (Some(file), None) => match save_world(heap, file) {
            Ok(()) => String::new(),
            Err(e) => format!("; {e}\n"),
        },
        _ => "; :save takes one file name\n".to_string(),
    }
}

/// `quokka bootstrap ...`: the tier-0 interpreter, and the bootstrap
/// compiler running on it. This is how the first world gets made, and where
/// the interpreter and the compiler are worked on; it is not Quokka.
fn bootstrap(args: &[String]) {
    match args {
        // Interpret a file and print the differential format, which is how
        // tier-0 baselines are produced.
        [flag, file] if flag == "--run" => {
            let source = std::fs::read_to_string(file).unwrap_or_else(|e| {
                eprintln!("cannot read {file}: {e}");
                std::process::exit(1);
            });
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

fn compile_file(file: &str) {
    let source = std::fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("cannot read {file}: {e}");
        std::process::exit(1);
    });
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
fn bootstrap_repl(compiling: bool, showing: bool) {
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

/// The read-eval-print loop of a terminal: prompt, read lines until
/// `complete` says they make whole forms, hand them to `evaluate`, print what
/// it says; until the input ends.
///
/// Lines are gathered because a form is not a line (#119): a `defun` pasted
/// from a file spans several, and `quokka world.qi < file.lisp` is how a
/// file is run before there are file hosts. Evaluating line by line gave
/// "a close parenthesis with no open one" for the second line of anything.
fn repl(
    heap: &mut Heap,
    prompt: &str,
    mut complete: impl FnMut(&mut Heap, &str) -> bool,
    mut evaluate: impl FnMut(&mut Heap, &str) -> String,
) {
    let mut host = NativeHost;
    let stdin = io::stdin();
    let mut pending = String::new();
    loop {
        host.write_stdout(if pending.is_empty() { prompt } else { MORE });
        let mut line = String::new();
        let ended = match stdin.lock().read_line(&mut line) {
            Ok(read) => read == 0,
            // Bytes that are not UTF-8, or a terminal that went away. Not an
            // ordinary end: a script that reads from this must see it fail.
            Err(e) => {
                eprintln!("quokka: reading the input: {e}");
                std::process::exit(1);
            }
        };
        pending.push_str(&line);
        if pending.trim().is_empty() {
            pending.clear();
        } else if ended || contained(heap, true, |heap| complete(heap, &pending)) {
            // At the end of the input an unfinished form is evaluated all
            // the same: that is what says it was unfinished.
            let text = std::mem::take(&mut pending);
            let output = contained(heap, String::new(), |heap| evaluate(heap, &text));
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
        if ended {
            host.write_stdout("\n");
            return;
        }
    }
}

/// The prompt for the second and later lines of a form.
const MORE: &str = "    ... ";

/// Run `work` on the world, and if the machine panics in it, keep the world.
///
/// A panic in the VM is a bug in the VM: no input, Lisp or image, should be
/// able to cause one. But bugs exist, the review behind #88 found several
/// that plain Lisp could reach, and the world is the user's work — principle
/// 5 of the design is that an error stops and asks and never throws state
/// away. So the unwinding stops here (`catch_unwind`; the panic's own message
/// has already gone to stderr) and the heap is checked.
///
/// A heap is plain data, and a panic *between* mutations leaves it exactly as
/// it was. A panic *inside* one may have left an object half written, which
/// is what [`Heap::verify`] is for: if it passes, the dead computation is
/// dropped and the REPL goes on, and the user is told to save under a new
/// name; if it does not, nothing is saved over anything and the process ends,
/// because every later answer from that heap would be suspect.
fn contained<T>(heap: &mut Heap, otherwise: T, work: impl FnOnce(&mut Heap) -> T) -> T {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    // `AssertUnwindSafe` is the promise that state seen after a panic is not
    // trusted blindly, and the check below is how it is kept.
    if let Ok(done) = catch_unwind(AssertUnwindSafe(|| work(&mut *heap))) {
        return done;
    }
    let sound = catch_unwind(AssertUnwindSafe(|| {
        heap.verify()?;
        quokka_vm::vm::recover(heap);
        heap.verify()
    }));
    match sound {
        Ok(Ok(())) => eprintln!(
            "quokka: that was a bug in Quokka's machine, not in your program — please \
             report it, with the message above.\n\
             quokka: the world passed its consistency check and is still here. \
             Save it under a new name (:save FILE) before going on."
        ),
        Ok(Err(why)) => {
            eprintln!("quokka: a bug in Quokka's machine left the world inconsistent ({why}).");
            eprintln!("quokka: nothing was saved; the last saved world is untouched.");
            std::process::exit(3);
        }
        Err(_) => {
            eprintln!("quokka: a bug in Quokka's machine left the world unreadable.");
            eprintln!("quokka: nothing was saved; the last saved world is untouched.");
            std::process::exit(3);
        }
    }
    otherwise
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
    if let Err(e) = quokka_bootstrap::eval::eval_sliced(heap, EVAL_BUDGET, 2_000) {
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
    let _ = quokka_bootstrap::eval::eval_forms(heap, line, EVAL_BUDGET, 2_000, |mc, result| {
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

#[cfg(test)]
#[path = "unit/main.rs"]
mod tests;
