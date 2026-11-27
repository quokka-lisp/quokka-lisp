//! Phase 3's gate (#133): **fix a function from inside an error and retry
//! the frame**, as a person does it, through the real `quokka` program.
//!
//! The debugger's other tests go through the test harness's own host. This
//! one builds a world with the `quokka` binary and runs it with a session on
//! standard input, so it goes through the terminal host: its console reads,
//! its REPL around them, and its prompts. With input piped the terminal host
//! leaves the debugger off, as it does for any script (#131), so each
//! session turns it on first.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// A world built once for every test here, by the program under test.
fn world() -> &'static PathBuf {
    static WORLD: OnceLock<PathBuf> = OnceLock::new();
    WORLD.get_or_init(|| {
        // Cargo's directory for integration tests' files, which `cargo clean`
        // empties; the process id keeps two test runs apart.
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("quokka-gate-{}.qi", std::process::id()));
        let built = Command::new(env!("CARGO_BIN_EXE_quokka"))
            .arg("build")
            .arg(&path)
            .output()
            .expect("quokka runs");
        assert!(
            built.status.success(),
            "quokka build failed: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        path
    })
}

/// Run a REPL session in the world: INPUT is what the person types, one
/// line after another; the answer is everything the program wrote.
fn session(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_quokka"))
        .arg(world())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("quokka runs");
    child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(input.as_bytes())
        .expect("the session is written");
    let out = child.wait_with_output().expect("quokka ends");
    assert!(
        out.status.success(),
        "quokka failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("output is text")
}

/// Does OUT contain each of PIECES, in this order?
fn in_order(out: &str, pieces: &[&str]) {
    let mut rest = out;
    for piece in pieces {
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => panic!("expected {piece:?} next, in:\n{out}"),
        }
    }
}

/// The gate in the main thread: an error opens the debugger, which shows
/// the frame that signalled; the person looks at the stack and the
/// definition, types the fix, and retries the frame; the caller gets the
/// fixed function's answer, and the world carries on with the fix in place.
/// `caller` counts its calls: it ran once, so the retry ran `broken`'s
/// frame and not the whole form again.
#[test]
fn a_function_is_fixed_from_inside_its_error_and_its_frame_retried() {
    let out = session(
        "(quokka-debug:enable-debugger)\n\
         (defvar *calls* 0)\n\
         (defun broken (x) (error \"broken ~a\" x))\n\
         (defun caller (y) (incf *calls*) (list :got (broken (* y 2))))\n\
         (caller 5)\n\
         backtrace\n\
         source\n\
         (defun broken (x) (+ x 1))\n\
         restart-frame\n\
         *calls*\n\
         (caller 6)\n",
    );
    in_order(
        &out,
        &[
            "debugger invoked on a SIMPLE-ERROR: broken 10",
            "0: [ABORT] Return to the top level.",
            "0: (BROKEN 10)",
            "0] 0: (BROKEN 10)\n1: (CALLER 5)",
            "0] (defun broken (x) (error \"broken ~a\" x))",
            "0] BROKEN",
            "0] (:GOT 11)",
            "quokka> 1",
            "quokka> (:GOT 13)",
        ],
    );
}

/// The gate in a thread, conditions and processes together: the error is in
/// a thread the main thread waits for, the debugger opens in that thread and
/// names it, and the fix and the retry happen there; the main thread's
/// `join-thread` gets the fixed answer.
#[test]
fn a_function_is_fixed_from_inside_an_error_in_a_thread() {
    let out = session(
        "(quokka-debug:enable-debugger)\n\
         (defvar *calls* 0)\n\
         (defun broken (x) (error \"broken ~a\" x))\n\
         (defun caller (y) (incf *calls*) (list :got (broken (* y 2))))\n\
         (quokka-thread:join-thread (quokka-thread:make-thread #'caller :name \"worker\" :arguments '(5)))\n\
         backtrace\n\
         (defun broken (x) (+ x 1))\n\
         restart-frame\n\
         *calls*\n",
    );
    in_order(
        &out,
        &[
            "debugger invoked on a SIMPLE-ERROR in thread \"worker\": broken 10",
            "0: [ABORT] Terminate this thread.",
            "0] 0: (BROKEN 10)\n1: (CALLER 5)",
            "0] BROKEN",
            "0] (:GOT 11)",
            "quokka> 1",
        ],
    );
}
