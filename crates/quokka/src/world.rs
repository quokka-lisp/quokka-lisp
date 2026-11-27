//! Running, building, rebuilding and saving a world.
//!
//! `quokka WORLD.qi` is a read-eval-print loop in which every form is read,
//! evaluated and printed by the world's own Lisp. This module is what the
//! terminal adds around that: gathering lines into forms, saving by atomic
//! replace, and keeping the world when the machine itself fails.

use std::io::{self, BufRead, IsTerminal, Read, Write};

use quokka_vm::heap::{Heap, Register};
use quokka_vm::host::Host;
use quokka_vm::value::Value;
use quokka_vm::vm::Outcome;
use quokka_vm::{PHASE, VERSION};

use crate::host::NativeHost;
use crate::{die, source_of, usage_error};

/// `quokka build OUT [SRC...]`: the cold build, then any SRC files loaded into
/// the new world, then saved.
///
/// The VM produces and consumes bytes; the file is the host's business
/// (rule 2), which is also what lets the browser put the same bytes in OPFS
/// or hand them to the user as a download.
pub(crate) fn build(args: &[String]) {
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
        let source = source_of(path);
        let mut failed = None;
        let ran = quokka_vm::vm::eval_source(
            &mut heap,
            &source,
            EVAL_BUDGET,
            2_000,
            &mut NativeHost,
            |outcome| match outcome {
                Outcome::Value(_) => true,
                Outcome::Error { message, .. } => {
                    failed = Some(message);
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
pub(crate) fn rebuild(args: &[String]) {
    let [from, to] = args else {
        usage_error("rebuild needs the world to start from and a file to save the result in");
    };
    let mut heap = load_image(from);
    // What the one form came to. Matched as an `Outcome`, where this used to
    // format it for debugging and test whether the text began "Value" (#121).
    let mut outcome = None;
    let ran = quokka_vm::vm::eval_source(
        &mut heap,
        "(quokka::%rebuild-world)",
        // As `differential.rs` gives it: a rebuild compiles every form of the
        // system. 20 ran out at floats (#17), 40 at step AB (#75, ~201M steps),
        // 60 at conditions' sub-step 2 (#125, just over 300M steps).
        EVAL_BUDGET * 100,
        2_000,
        &mut NativeHost,
        |came| {
            outcome = Some(came);
            false
        },
    );
    let failure = match (ran, outcome) {
        (Ok(()), Some(Outcome::Value(report))) => {
            println!("rebuilt: {report}");
            None
        }
        (Ok(()), Some(Outcome::Error { message, .. })) => Some(message),
        (Ok(()), Some(Outcome::Failed(e))) | (Err(e), _) => Some(e.to_string()),
        (Ok(()), None) => Some("the world read no form to rebuild with".to_string()),
    };
    if let Some(why) = failure {
        // The usual reason: the source is found by name, beneath the
        // directory the program was started in.
        let hint = if why.contains("not found") || why.contains("denied") {
            "\nquokka: a rebuild reads lisp/ beneath the current directory: \
             run it from the repository's root, or name that with --root"
        } else {
            ""
        };
        die(&format!("the rebuild failed: {why}{hint}"));
    }
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
pub(crate) fn save_world(heap: &mut Heap, path: &str) -> Result<(), String> {
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
pub(crate) fn run_world(path: &str) {
    let mut heap = load_image(path);
    let mut host = NativeHost;
    // The debugger talks to a person, so it is on when there is one: when
    // the input is a terminal, and not when a script is piped in, which
    // goes on past an error as it always has (#131).
    let person = io::stdin().is_terminal();
    heap.mutate(|mc| {
        quokka_vm::process::focus_main(mc);
        mc.set_register(Register::Value, if person { Value::T } else { Value::NIL })
    });
    if let Err(e) =
        quokka_vm::vm::call_global(&mut heap, "%HOST-DEBUGGER", EVAL_BUDGET, 2_000, &mut host)
    {
        host.write_stdout(&format!("; the debugger could not be set up: {e}\n"));
    }
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
pub(crate) fn world_command(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix(":save")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then_some(rest)
}

/// Run a REPL command: handled by the host between forms, so the world is
/// never saved in the middle of a computation (#51).
pub(crate) fn run_command(heap: &mut Heap, arguments: &str) -> String {
    let mut words = arguments.split_whitespace();
    match (words.next(), words.next()) {
        (Some(file), None) => match save_world(heap, file) {
            Ok(()) => String::new(),
            Err(e) => format!("; {e}\n"),
        },
        _ => "; :save takes one file name\n".to_string(),
    }
}

/// The read-eval-print loop of a terminal: prompt, read lines until
/// `complete` says they make whole forms, hand them to `evaluate`, print what
/// it says; until the input ends.
///
/// Lines are gathered because a form is not a line (#119): a `defun` pasted
/// from a file spans several, and `quokka world.qi < file.lisp` is how a
/// file is run before there are file hosts. Evaluating line by line gave
/// "a close parenthesis with no open one" for the second line of anything.
pub(crate) fn repl(
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
pub(crate) const MORE: &str = "    ... ";

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
pub(crate) fn contained<T>(heap: &mut Heap, otherwise: T, work: impl FnOnce(&mut Heap) -> T) -> T {
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
pub(crate) fn load_image(path: &str) -> Heap {
    let bytes = std::fs::read(path).unwrap_or_else(|e| die(&format!("cannot read {path}: {e}")));
    quokka_vm::image::load(&bytes, DEFAULT_IMAGE_LIMIT).unwrap_or_else(|e| {
        eprintln!("quokka: {path}: {e}");
        std::process::exit(1);
    })
}

/// Evaluate a line in a compiled image: read, evaluated and printed by the
/// image's own Lisp (#47), and the interpreter, the Rust reader and the Rust
/// printer not involved at all. There is no bootstrap rule here, because
/// there is nothing left to bootstrap.
pub(crate) fn image_print(heap: &mut Heap, line: &str, host: &mut NativeHost) -> String {
    let mut out = String::new();
    let ran = quokka_vm::vm::eval_source(heap, line, EVAL_BUDGET, 2_000, host, |outcome| {
        if !out.is_empty() {
            out.push('\n');
        }
        match outcome {
            Outcome::Value(text) => out.push_str(&text),
            Outcome::Error { message, .. } => out.push_str(&format!("; {message}")),
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
pub(crate) const EVAL_BUDGET: u64 = 5_000_000;

/// The soft limit a loaded image is given.
///
/// The host's policy, not the image's: see `Heap::from_image`. A native REPL
/// can afford a large heap; the browser will pass something smaller for the
/// same image.
pub(crate) const DEFAULT_IMAGE_LIMIT: usize = 1 << 22;
