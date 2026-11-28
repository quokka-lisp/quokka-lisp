//! Tests for the `quokka-wasm` crate root: the browser host, driven as a page
//! drives it. They run natively — nothing here needs a browser, which is the
//! point of keeping the DOM on the other side of `take_output`.

use super::*;
use std::sync::OnceLock;

/// A world, built once. Building is the cold build's job, which is why the
/// bootstrap crate is a dev-dependency here and not a dependency.
fn world() -> Quokka {
    static IMAGE: OnceLock<Vec<u8>> = OnceLock::new();
    let image = IMAGE.get_or_init(|| {
        let mut heap = Heap::new();
        quokka_bootstrap::boot::build_image(&mut heap).unwrap_or_else(|e| panic!("{e}"));
        image::save(&mut heap)
    });
    Quokka::new(image).expect("the image loads")
}

/// Submit a line and pump `run` as a page's timer would, `steps` at a time.
/// Returns the output and how many calls it took.
fn pump(q: &mut Quokka, line: &str, steps: u32) -> (String, usize) {
    q.submit(line).expect("idle");
    let mut out = String::new();
    let mut calls = 0;
    while q.run(steps) {
        out.push_str(&q.take_output());
        calls += 1;
        assert!(calls < 10_000_000, "never finished");
    }
    out.push_str(&q.take_output());
    (out, calls)
}

#[test]
fn banner_names_the_phase() {
    assert!(world().banner().contains(PHASE));
}

#[test]
fn what_is_not_an_image_is_refused_with_a_reason() {
    assert_eq!(
        Quokka::new(b"not an image, no").unwrap_err(),
        "16 bytes is too short to be an image"
    );
    assert_eq!(
        Quokka::new(b"not an image").unwrap_err(),
        "12 bytes is not a whole number of words: truncated, or not an image"
    );
}

/// The browser runs a world: tier 1, the Lisp reader and printer, packages.
/// None of this existed in the interpreter the page used to boot.
#[test]
fn a_line_is_read_evaluated_and_printed_by_the_world() {
    let mut q = world();
    assert_eq!(pump(&mut q, "(+ 1 (* 2 3))", 10_000).0, "7\n");
    assert_eq!(
        pump(&mut q, "(format nil \"~D item~:P\" 2) (expt 2 100)", 10_000).0,
        "\"2 items\"\n1267650600228229401496703205376\n"
    );
    assert_eq!(pump(&mut q, "(car 5)", 10_000).0.chars().next(), Some(';'));
    // Console output arrives through a host request, in order with values.
    assert_eq!(pump(&mut q, "(progn (princ \"hi\") 1)", 10_000).0, "hi1\n");
    assert!(q.heap_words() > 0);
}

/// **Slicing is invisible** (rule 4): the same line gives the same output
/// whether the page runs it three steps at a time or all at once — and at
/// three steps a call, the page gets control back thousands of times.
#[test]
fn the_answer_does_not_depend_on_how_it_is_sliced() {
    let line = "(defun fib (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (fib 15)";
    let (whole, few) = pump(&mut world(), line, u32::MAX);
    let (sliced, many) = pump(&mut world(), line, 3);
    assert_eq!(whole, "FIB\n610\n");
    assert_eq!(sliced, whole);
    assert!(few <= 2 && many > 1_000, "{few} calls and {many}");
}

/// A line that never ends does not freeze anything: `run` keeps returning,
/// and the page can stop it. The world is still there afterwards.
#[test]
fn an_endless_line_can_be_interrupted() {
    let mut q = world();
    q.submit("(defvar *n* 0) (tagbody top (go top))").unwrap();
    for _ in 0..50 {
        assert!(q.run(1_000));
    }
    assert!(q.submit("1").is_err(), "busy");
    q.interrupt();
    assert!(!q.busy());
    assert!(q.take_output().ends_with("; interrupted\n"));
    assert_eq!(pump(&mut q, "*n*", 10_000).0, "0\n");
}

/// Running out of heap is reported and recovered from; the page does not
/// have to be reloaded, as it did before #84.
#[test]
fn out_of_heap_is_survived() {
    let mut q = world();
    let (out, _) = pump(&mut q, "(make-array 100000000000) (+ 1 2)", 100_000);
    assert!(out.starts_with("; out of heap"), "{out}");
    assert!(out.ends_with("3\n"), "{out}");
}

/// A world saved in the browser loads again, and keeps what was defined.
#[test]
fn a_world_can_be_saved_and_loaded_again() {
    let mut q = world();
    pump(&mut q, "(defun twice (x) (* 2 x))", 10_000);
    let mut again = Quokka::new(&q.save()).expect("loads");
    assert_eq!(pump(&mut again, "(twice 21)", 10_000).0, "42\n");
}

// ---- the environment's host side (#137) --------------------------------

/// Run until the world is idle or finished; the messages it sent.
fn settle(q: &mut Quokka) -> String {
    let mut calls = 0;
    while q.run(10_000) {
        calls += 1;
        assert!(calls < 1_000_000, "never settled");
    }
    q.take_messages()
}

/// A program that shows each event it is given, upcased, until "stop".
fn echo() -> Quokka {
    let mut q = world();
    pump(
        &mut q,
        "(defun echo (ignored) \
           (do ((e (quokka::%await-event) (quokka::%await-event))) \
               ((equal e \"stop\") :done) \
             (quokka::%display (string-upcase e))))",
        100_000,
    );
    q.start("ECHO").expect("starts");
    q
}

/// A world waiting for an event with none to give it is idle: `run` says
/// there is nothing to do, at once, and the page stops calling it. The
/// program is still running.
#[test]
fn a_world_waiting_for_the_person_is_idle() {
    let mut q = echo();
    assert_eq!(settle(&mut q), "");
    assert!(q.busy());
    assert!(!q.run(10_000), "idle, and stays idle");
}

/// An event wakes it; what it displays is collected, in order; the end of
/// the program ends the run.
#[test]
fn events_wake_the_world_and_messages_come_back() {
    let mut q = echo();
    settle(&mut q);
    q.post_event("hello");
    assert_eq!(settle(&mut q), "HELLO");
    q.post_event("one");
    q.post_event("two");
    assert_eq!(settle(&mut q), "ONE\nTWO");
    q.post_event("stop");
    settle(&mut q);
    assert!(!q.busy(), "the program returned");
}

/// While the main process waits for the person, another process's display
/// is still answered (`host::waiting`): the event the page cannot answer
/// does not hold up the requests it can.
#[test]
fn a_background_display_is_answered_while_the_main_process_waits() {
    let mut q = world();
    pump(
        &mut q,
        "(defun watch (ignored) \
           (quokka-thread:make-thread \
             (lambda () (dotimes (i 5) (quokka-thread:thread-yield)) \
                        (quokka::%display \"from a thread\"))) \
           (quokka::%await-event))",
        100_000,
    );
    q.start("WATCH").unwrap();
    assert_eq!(settle(&mut q), "from a thread");
    assert!(q.busy());
}

/// A program that never ends is stopped by the page, started again, and
/// told why with the event `(:interrupted)`.
#[test]
fn an_interrupted_program_starts_again_and_is_told() {
    let mut q = world();
    pump(
        &mut q,
        "(defun spinner (ignored) \
           (do ((e (quokka::%await-event) (quokka::%await-event))) (nil) \
             (quokka::%display e) \
             (when (equal e \"spin\") (tagbody top (go top)))))",
        100_000,
    );
    q.start("SPINNER").unwrap();
    settle(&mut q);
    q.post_event("spin");
    for _ in 0..20 {
        assert!(q.run(1_000), "busy spinning");
    }
    q.interrupt();
    assert!(q.busy());
    assert_eq!(settle(&mut q), "spin\n(:interrupted)");
}

/// The read-eval-print loop has nobody to wait for: a form that asks for an
/// event gets nil, or one the page posted.
#[test]
fn the_repl_is_never_left_waiting_for_an_event() {
    let mut q = world();
    assert_eq!(pump(&mut q, "(quokka::%await-event)", 10_000).0, "NIL\n");
    q.post_event("(:key)");
    assert_eq!(
        pump(&mut q, "(quokka::%await-event)", 10_000).0,
        "\"(:key)\"\n"
    );
}

// ---- output records (#138) ---------------------------------------------

/// Evaluate LINE at the REPL and return what it displayed and printed.
fn shown(q: &mut Quokka, line: &str) -> (String, String) {
    let (out, _) = pump(q, line, 100_000);
    (q.take_messages(), out)
}

/// A recording stream turns what `format` writes into text records, one
/// per run of a style; the first flush shows the block with everything in
/// it, and later writes are sent as what changed.
#[test]
fn a_record_stream_records_runs_and_flushes_what_changed() {
    let mut q = world();
    pump(
        &mut q,
        "(in-package :quokka-ui) \
         (defvar *root* (make-instance 'viewer-record)) \
         (setf (quokka::%record-shown *root*) t) \
         (defvar *b* (make-block-record)) \
         (defvar *s* (make-record-stream *b*))",
        1_000_000,
    );
    let (messages, _) = shown(
        &mut q,
        "(progn (add-record *root* *b*) (format *s* \"~D cards\" 3) \
                (with-ink (*s* +dim+) (write-string \" (dim)\" *s*)) \
                (flush-display) :done)",
    );
    let mut seen = Vec::new();
    let first = q_ids(&mut seen, &messages);
    assert_eq!(
        first,
        "((:APPEND R1 (:BLOCK R2 NIL (:TEXT R3 NIL \"3 cards\") (:TEXT R4 (:INK :DIM) \" (dim)\"))))"
    );
    let (messages, _) = shown(
        &mut q,
        "(progn (with-ink (*s* +dim+) (write-string \" more\" *s*)) (flush-display) :done)",
    );
    assert_eq!(
        q_ids(&mut seen, &messages),
        "((:APPEND-TEXT R4 \" more\"))",
        "only what the page lacks is sent (#166)"
    );
    let (messages, _) = shown(&mut q, "(progn (flush-display) :done)");
    assert_eq!(messages, "", "nothing changed, nothing sent");
}

/// Record numbers differ from world to world; name them R1, R2... in the
/// order they first appear, across the calls that share SEEN.
fn q_ids(seen: &mut Vec<String>, messages: &str) -> String {
    let mut out = String::new();
    let mut rest = messages;
    // Every node and message names a record by the number after its kind.
    while let Some(at) = rest.find(|c: char| c.is_ascii_digit()) {
        let (before, after) = rest.split_at(at);
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        out.push_str(before);
        let is_id = before.ends_with(' ')
            && (before.trim_end().ends_with(":APPEND")
                || before.trim_end().ends_with(":APPEND-TEXT")
                || before.trim_end().ends_with(":TEXT")
                || before.trim_end().ends_with(":BLOCK")
                || before.trim_end().ends_with(":SET")
                || before.trim_end().ends_with(":EDITOR")
                || before.trim_end().ends_with(":VIEWER")
                || before.trim_end().ends_with(":INSERT")
                || before.trim_end().ends_with(":REPLACE")
                || before.trim_end().ends_with(":REMOVE")
                || before.trim_end().ends_with(":FOCUS"));
        if is_id {
            let n = match seen.iter().position(|s| *s == digits) {
                Some(i) => i + 1,
                None => {
                    seen.push(digits.clone());
                    seen.len()
                }
            };
            out.push_str(&format!("R{n}"));
        } else {
            out.push_str(&digits);
        }
        rest = &after[digits.len()..];
    }
    out.push_str(rest);
    out
}

// ---- viewers, redisplay and the command loop (#139) --------------------

/// A world whose surface is one counting viewer, started as the page starts
/// the environment, and the messages of its first redisplay.
fn counter() -> (Quokka, String) {
    let mut q = world();
    pump(
        &mut q,
        "(defpackage :counting (:use :common-lisp :quokka-ui)) (in-package :counting) \
         (defclass counter (viewer) ((n :initform 0 :accessor n))) \
         (defmethod display-viewer ((v counter) stream) \
           (format stream \"count ~D\" (n v)) \
           (when (>= (n v) 2) \
             (with-ink (stream +accent+) (format stream \"~%big\")))) \
         (defmethod viewer-input ((v counter) event) \
           (cond ((equal (event-key event) \"ArrowUp\") (incf (n v)) t) \
                 ((equal (event-key event) \"x\") (error \"no x here\")) \
                 (t nil))) \
         (setf *surface* (make-instance 'counter :title \"Counter\"))",
        1_000_000,
    );
    q.start("%RUN-ENVIRONMENT").expect("starts");
    let first = settle(&mut q);
    (q, first)
}

#[test]
fn the_environment_shows_its_surface_and_redraws_what_changed() {
    let mut seen = Vec::new();
    let (mut q, first) = counter();
    assert_eq!(
        q_ids(&mut seen, &first),
        "((:RESET (:VIEWER R1 (:TITLE \"Counter\") (:TEXT R2 NIL \"count 0\"))))"
    );
    assert!(q.busy(), "waiting for the person");
    q.post_event("(:key :key \"ArrowUp\" :code \"ArrowUp\")");
    assert_eq!(
        q_ids(&mut seen, &settle(&mut q)),
        "((:SET R2 :TEXT \"count 1\"))"
    );
    q.post_event("(:key :key \"ArrowUp\" :code \"ArrowUp\")");
    assert_eq!(
        q_ids(&mut seen, &settle(&mut q)),
        "((:SET R2 :TEXT \"count 2\") (:APPEND R1 (:TEXT R3 (:INK :ACCENT) \"\nbig\")))"
    );
    // A key nobody wants changes nothing, and nothing is sent.
    q.post_event("(:key :key \"q\" :code \"KeyQ\")");
    assert_eq!(settle(&mut q), "");
}

/// An error in a viewer is shown in it, and the loop goes on.
#[test]
fn an_error_in_a_viewer_does_not_end_the_loop() {
    let (mut q, _) = counter();
    q.post_event("(:key :key \"x\" :code \"KeyX\")");
    let messages = settle(&mut q);
    assert!(messages.contains("no x here"), "{messages}");
    assert!(messages.contains(":ERROR"), "{messages}");
    q.post_event("(:key :key \"ArrowUp\" :code \"ArrowUp\")");
    assert!(settle(&mut q).contains("count 1"));
    assert!(q.busy());
}

/// What the page sends is read as data and never evaluated; what cannot be
/// read is ignored.
#[test]
fn a_malformed_event_is_ignored() {
    let (mut q, _) = counter();
    for bad in [
        "(:key",
        "#.(error \"no\")",
        "42",
        "",
        "(:key :target 99999999)",
    ] {
        q.post_event(bad);
        settle(&mut q);
        assert!(q.busy(), "still running after {bad:?}");
    }
    q.post_event("(:key :key \"ArrowUp\")");
    assert!(settle(&mut q).contains("count 1"));
}

/// A program that fails before it ever waits is not started again for ever
/// (the review of #137): three times, and it is stopped; `run` returns.
#[test]
fn a_program_that_keeps_failing_is_stopped() {
    let mut q = world();
    pump(
        &mut q,
        "(defun broken-start (x) (quokka::%display \"up\") (car 5))",
        100_000,
    );
    q.start("BROKEN-START").unwrap();
    for _ in 0..20 {
        q.run(1_000_000);
    }
    assert!(!q.busy());
    assert!(q.take_output().contains("stopped"));
    assert_eq!(
        q.start("NO-SUCH-FUNCTION-AT-ALL").unwrap_err(),
        "the function NO-SUCH-FUNCTION-AT-ALL is undefined"
    );
}

/// The next event goes to the main process, whose command loop it is for,
/// even when another process waits for one too.
#[test]
fn events_go_to_the_main_process_first() {
    let mut q = world();
    pump(
        &mut q,
        "(defun two-waiters (x) \
           (quokka-thread:make-thread (lambda () (quokka::%display (format nil \"thread got ~A\" (quokka::%await-event))))) \
           (do () (nil) (quokka::%display (format nil \"main got ~A\" (quokka::%await-event)))))",
        100_000,
    );
    q.start("TWO-WAITERS").unwrap();
    settle(&mut q);
    q.post_event("one");
    assert_eq!(settle(&mut q), "main got one");
}

// ---- the listener (#141) -------------------------------------------------

/// The record the last `(:FOCUS n)` in MESSAGES names: the editor the
/// caret is in.
fn focused(messages: &str) -> u64 {
    let at = messages.rfind("(:FOCUS ").expect("something has the focus");
    messages[at + 8..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap()
}

/// Escape TEXT as a string the world's reader reads.
fn lisp_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Type TEXT into editor ID as the page reports it, press KEY, and return
/// what the world sent back.
fn type_in(q: &mut Quokka, id: u64, text: &str, key: &str) -> String {
    let n = text.chars().count();
    q.post_event(&format!(
        "(:edit :target {id} :text {} :start {n} :end {n})",
        lisp_string(text)
    ));
    let modifiers = if key == "Meta+Enter" { "(:meta)" } else { "()" };
    q.post_event(&format!(
        "(:key :target {id} :key \"Enter\" :code \"Enter\" :modifiers {modifiers} :start {n} :end {n})"
    ));
    settle(q)
}

/// The environment as the page starts it: one listener.
fn listener() -> (Quokka, String) {
    let mut q = world();
    q.start("%RUN-ENVIRONMENT").expect("starts");
    let first = settle(&mut q);
    (q, first)
}

#[test]
fn the_listener_shows_a_prompt_and_an_editor() {
    let (_, first) = listener();
    assert!(first.starts_with("((:RESET (:VIEWER "), "{first}");
    assert!(first.contains("(:TITLE \"Listener\")"), "{first}");
    assert!(first.contains("\"CL-USER> \""), "{first}");
    assert!(first.contains("(:EDITOR "), "{first}");
    assert!(
        first.contains(":KEYS (\"Enter\" \"Meta+Enter\" \"Control+Enter\")"),
        "{first}"
    );
    focused(&first);
}

/// **The gate, in the world's half**: a form typed into the listener and
/// entered is evaluated; its value is shown under it, and a new prompt
/// waits below.
#[test]
fn a_form_entered_in_the_listener_is_evaluated() {
    let (mut q, first) = listener();
    let editor = focused(&first);
    let after = type_in(&mut q, editor, "(+ 1 2)", "Enter");
    assert!(after.contains("\"3\n\""), "{after}");
    assert!(after.contains("\"CL-USER> \""), "a new prompt: {after}");
    assert_ne!(focused(&after), editor, "the caret moved to the new entry");
    // Output goes in a block of its own, set apart with a bar.
    let printed = type_in(
        &mut q,
        focused(&after),
        "(progn (format t \"hi~%\") :done)",
        "Enter",
    );
    assert!(printed.contains(":BAR T"), "{printed}");
    assert!(printed.contains("\"hi\n\""), "{printed}");
    assert!(printed.contains("\":DONE\n\""), "{printed}");
}

/// Enter in a form that is not finished makes a new line instead.
#[test]
fn enter_in_an_unfinished_form_makes_a_new_line() {
    let (mut q, first) = listener();
    let editor = focused(&first);
    let after = type_in(&mut q, editor, "(+ 1", "Enter");
    assert!(after.contains(":TEXT \"(+ 1\n\" :START 5"), "{after}");
}

/// An earlier form, edited and run with Cmd-Enter, runs as a new entry
/// below; the old results are struck through and both entries say so.
#[test]
fn an_edited_form_runs_again_below() {
    let (mut q, first) = listener();
    let editor = focused(&first);
    let after = type_in(&mut q, editor, "(* 2 3)", "Enter");
    assert!(after.contains("\"6\n\""), "{after}");
    let again = type_in(&mut q, editor, "(* 2 4)", "Meta+Enter");
    assert!(again.contains(":STRIKE T"), "{again}");
    assert!(again.contains("edited · ran again below"), "{again}");
    assert!(again.contains("from 1 form up"), "{again}");
    assert!(again.contains("\"8\n\""), "{again}");
    assert!(
        again.contains("(:INSERT "),
        "the new entry goes above the input: {again}"
    );
}

/// A form that reads gets an editor for the line, in its own entry, and
/// the line typed there is what it reads.
#[test]
fn a_form_that_reads_waits_for_a_line() {
    let (mut q, first) = listener();
    let asking = type_in(
        &mut q,
        focused(&first),
        "(list (read-line) :after)",
        "Enter",
    );
    assert!(q.busy());
    let line_editor = focused(&asking);
    let after = type_in(&mut q, line_editor, "hello there", "Enter");
    assert!(after.contains("(\\\"hello there\\\" :AFTER)"), "{after}");
}

/// An error opens the debugger in the listener; a restart chosen by number
/// at its prompt returns to the listener.
#[test]
fn an_error_opens_the_debugger_in_the_listener() {
    let (mut q, first) = listener();
    let debugging = type_in(&mut q, focused(&first), "(car 5)", "Enter");
    assert!(debugging.contains("debugger invoked"), "{debugging}");
    assert!(debugging.contains("\"0] \""), "{debugging}");
    let back = type_in(&mut q, focused(&debugging), "0", "Enter");
    assert!(back.contains("\"CL-USER> \""), "a new prompt: {back}");
    let next = type_in(&mut q, focused(&back), "(+ 40 2)", "Enter");
    assert!(next.contains("\"42\n\""), "{next}");
}

/// The prompt is drawn by a generic function a person can change, and its
/// records remember that it drew them (docs/display.md, decision 7).
#[test]
fn the_prompt_is_drawn_by_listener_prompt() {
    let (mut q, first) = listener();
    let after = type_in(
        &mut q,
        focused(&first),
        "(defmethod quokka-env:listener-prompt ((l quokka-env:listener) s) (format s \"quokka ~D › \" (quokka-env:listener-count l)))",
        "Enter",
    );
    assert!(after.contains("\"quokka 2 › \""), "{after}");
    let who = type_in(
        &mut q,
        focused(&after),
        "(quokka::%function-name (quokka-ui:record-drawn-by (first (quokka-ui:record-children (quokka::listener-entry-prompt (first (quokka::%listener-entries quokka-env:*listener*)))))))",
        "Enter",
    );
    assert!(
        who.contains("(:METHOD QUOKKA-ENV:LISTENER-PROMPT (QUOKKA-ENV:LISTENER T))"),
        "{who}"
    );
}

// ---- drawing (#143) --------------------------------------------------------

/// A picture drawn in the listener is a drawing record of shapes, sent as
/// nodes the renderer turns into SVG; numbers that are ratios arrive as
/// doubles the page can read.
#[test]
fn a_drawing_in_the_listener_is_sent_as_shapes() {
    let (mut q, first) = listener();
    let after = type_in(
        &mut q,
        focused(&first),
        "(progn (quokka-ui:with-drawing (t :width 100 :height 50) \
                  (quokka-ui:draw-circle* 25 25 20 :ink quokka-ui:+accent+) \
                  (quokka-ui:with-translation (50 0) \
                    (quokka-ui:draw-rectangle* 0 0 1/2 30 :filled nil)) \
                  (quokka-ui:draw-text* \"hi\" 60 45 :align :center)) \
                :drawn)",
        "Enter",
    );
    assert!(after.contains("(:DRAWING "), "{after}");
    assert!(
        after.contains(":WIDTH 100 :HEIGHT 50 :SCALE :FIT"),
        "{after}"
    );
    assert!(
        after.contains("(:TYPE :ELLIPSE :CX 25 :CY 25 :RX 20 :RY 20 :INK :ACCENT :FILLED T)"),
        "{after}"
    );
    assert!(after.contains("(:GROUP "), "{after}");
    assert!(after.contains(":TRANSFORM (1 0 0 1 50 0)"), "{after}");
    assert!(
        after.contains(":WIDTH 0.5 "),
        "a ratio arrives as a double: {after}"
    );
    assert!(after.contains(":TEXT \"hi\""), "{after}");
    assert!(after.contains("\":DRAWN\n\""), "{after}");
}

/// A viewer that draws again moves its shapes: the circle keeps its
/// identity and only its place is sent.
#[test]
fn a_redrawn_shape_keeps_its_identity() {
    let mut q = world();
    pump(
        &mut q,
        "(defpackage :moving (:use :common-lisp :quokka-ui)) (in-package :moving) \
         (defclass mover (viewer) ((x :initform 10 :accessor x))) \
         (defmethod display-viewer ((v mover) stream) \
           (with-drawing (stream :width 200 :height 100) \
             (draw-circle* (x v) 50 10 :ink +teal+))) \
         (defmethod viewer-input ((v mover) event) (incf (x v) 10) t) \
         (setf *surface* (make-instance 'mover))",
        1_000_000,
    );
    q.start("%RUN-ENVIRONMENT").unwrap();
    let mut seen = Vec::new();
    q_ids(&mut seen, &settle(&mut q));
    q.post_event("(:key :key \"ArrowRight\")");
    let moved = settle(&mut q);
    assert!(moved.starts_with("((:SET "), "{moved}");
    assert!(moved.contains(":CX 20"), "{moved}");
    assert!(!moved.contains("(:SHAPE"), "no new shape: {moved}");
}

// ---- what the review of #138-#143 found -----------------------------------

/// After a Stop, the listener runs forms again: the abandoned form's
/// cleanups never ran, and `viewer-restarted` puts its state right.
#[test]
fn the_listener_works_after_a_stop() {
    let (mut q, first) = listener();
    q.post_event(&format!(
        "(:edit :target {} :text \"(tagbody top (go top))\" :start 22 :end 22)",
        focused(&first)
    ));
    q.post_event(&format!(
        "(:key :target {} :key \"Enter\" :code \"Enter\" :modifiers ())",
        focused(&first)
    ));
    for _ in 0..20 {
        assert!(q.run(1_000), "spinning");
    }
    q.interrupt();
    let after = settle(&mut q);
    let next = type_in(&mut q, focused(&after), "(+ 40 2)", "Enter");
    assert!(next.contains("\"42\n\""), "{next}");
}

/// After a failure of the machine, out of heap, likewise.
#[test]
fn the_listener_works_after_running_out_of_heap() {
    let (mut q, first) = listener();
    let after = type_in(
        &mut q,
        focused(&first),
        "(make-array 100000000000)",
        "Enter",
    );
    let next = type_in(&mut q, focused(&after), "(+ 40 2)", "Enter");
    assert!(next.contains("\"42\n\""), "{next}");
}

/// "Return to the command loop" from the listener's debugger leaves a new
/// prompt waiting.
#[test]
fn leaving_the_debugger_for_the_command_loop_leaves_a_prompt() {
    let (mut q, first) = listener();
    let debugging = type_in(&mut q, focused(&first), "(car 5)", "Enter");
    let back = type_in(&mut q, focused(&debugging), "1", "Enter");
    assert!(back.contains("\"CL-USER> \""), "{back}");
    let next = type_in(&mut q, focused(&back), "(+ 40 2)", "Enter");
    assert!(next.contains("\"42\n\""), "{next}");
}

/// A property list that is not one is no event, not an error that
/// restarts the environment.
#[test]
fn an_event_with_a_broken_property_list_is_ignored() {
    let (mut q, first) = listener();
    q.post_event("(:key :key \"a\" . 5)");
    q.post_event("(:edit :target 1 :text)");
    let quiet = settle(&mut q);
    assert!(!quiet.contains(":RESET"), "{quiet}");
    let next = type_in(&mut q, focused(&first), "(+ 40 2)", "Enter");
    assert!(next.contains("\"42\n\""), "{next}");
}

/// A record replaced and the replacement removed before a flush: the page
/// is told to take the old one away.
#[test]
fn a_replacement_removed_before_the_flush_removes_the_original() {
    let mut q = world();
    pump(
        &mut q,
        "(in-package :quokka-ui) \
         (defvar *root* (make-instance 'viewer-record)) \
         (setf (quokka::%record-shown *root*) t) \
         (defvar *a* (make-block-record)) (defvar *b* (make-block-record)) \
         (add-record *root* *a*) (flush-display)",
        1_000_000,
    );
    q.take_messages();
    let (messages, _) = shown(
        &mut q,
        "(progn (replace-record *a* *b*) (remove-record *b*) (flush-display) :done)",
    );
    assert!(messages.starts_with("((:REMOVE "), "{messages}");
    // And a property that does not cross as it is arrives as text.
    let (messages, _) = shown(
        &mut q,
        "(progn (add-record *root* *b*) (flush-display) \
                (setf (record-property *b* :odd) (list 1/2 (make-hash-table))) \
                (flush-display) :done)",
    );
    assert!(messages.contains(":ODD (0.5 \"#<"), "{messages}");
}

/// An edit still waiting when the next for the same editor arrives is
/// replaced (#194): each says the whole text, so the older says nothing the
/// newer does not, and typing faster than the world reads queued a copy of
/// the text a character. An edit never moves past a key, nor replaces one
/// for another editor.
#[test]
fn a_waiting_edit_is_replaced_by_the_next_for_its_editor() {
    let mut q = world();
    q.post_event("(:edit :target 5 :text \"a\" :start 1 :end 1)");
    q.post_event("(:edit :target 5 :text \"ab\" :start 2 :end 2)");
    assert_eq!(q.events.len(), 1);
    assert!(q.events[0].contains("\"ab\""));
    q.post_event("(:key :target 5 :key \"Enter\")");
    q.post_event("(:edit :target 5 :text \"abc\" :start 3 :end 3)");
    q.post_event("(:edit :target 6 :text \"x\" :start 1 :end 1)");
    assert_eq!(q.events.len(), 4, "a key and another editor are kept apart");
}

/// Stop ends a thread that would run for ever (#195). In the environment
/// it used to give up the form and leave the thread, so `run` answered
/// "more to do" without end and the page worked at full speed with nothing
/// to press that would end it.
#[test]
fn stop_ends_a_thread_that_never_would() {
    let (mut q, first) = listener();
    let editor = focused(&first);
    let form = "(quokka-thread:make-thread (lambda () (do () (nil) (quokka-thread:thread-yield))))";
    let n = form.chars().count();
    q.post_event(&format!(
        "(:edit :target {editor} :text {form:?} :start {n} :end {n})"
    ));
    q.post_event(&format!(
        "(:key :target {editor} :key \"Enter\" :start {n} :end {n})"
    ));
    for _ in 0..300 {
        q.run(20_000);
    }
    assert!(q.run(20_000), "the thread keeps the world from being idle");
    q.interrupt();
    let mut calls = 0;
    while q.run(20_000) {
        calls += 1;
        assert!(calls < 2_000, "the thread went on after Stop");
    }
}

/// Stop lets the form run its cleanups before it is let go of (#173), as
/// the terminal does for a form that runs out of its budget. Dropped where
/// it stood, a form stopped inside `with-mutex` left the mutex held.
#[test]
fn a_stopped_form_runs_its_cleanups() {
    let mut q = world();
    pump(
        &mut q,
        "(defvar *m* (quokka-thread:make-mutex)) (defvar *log* nil)",
        1_000_000,
    );
    q.submit(
        "(quokka-thread:with-mutex (*m*) (unwind-protect (do () (nil)) (push :cleaned *log*)))",
    )
    .expect("idle");
    for _ in 0..50 {
        assert!(q.run(20_000), "the form never ends by itself");
    }
    q.interrupt();
    let (out, _) = pump(
        &mut q,
        "(list *log* (quokka-thread:mutex-owner *m*))",
        1_000_000,
    );
    assert!(out.contains("((:CLEANED) NIL)"), "{out}");
}
