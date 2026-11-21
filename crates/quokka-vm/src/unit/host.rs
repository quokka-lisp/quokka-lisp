//! Tests for [`crate::host`]: the request protocol, end to end.
//!
//! A child module of it, included with `#[path]`. Most of what is tested here
//! is public; the record layout in `request` is the part that is not.

use super::*;
use crate::boot;
use crate::eval::EvalError;
use crate::heap::Heap;
use crate::reader::Reader;
use crate::slice::{run_slice, Status};
use crate::vm::Vm;

#[test]
fn null_host_collects_output_and_has_no_files() {
    let mut host = NullHost::default();
    host.write_stdout("hello");
    assert_eq!(host.output, "hello");
    assert_eq!(
        host.read_source("boot.lisp"),
        Err(HostError::NotFound("boot.lisp".to_string()))
    );
}

/// A synchronous host with a fixed set of files.
struct Files(&'static [(&'static str, &'static str)]);

impl Host for Files {
    fn write_stdout(&mut self, _: &str) {}
    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        self.0
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, text)| text.to_string())
            .ok_or_else(|| HostError::NotFound(name.to_string()))
    }
}

/// A heap with the prelude and the compiler, which is what it takes to get a
/// compiled function to run until #33 builds an image of them.
fn compiler() -> Heap {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("the prelude boots");
    boot::install_compiler(&mut heap).expect("the compiler loads");
    heap
}

/// Compile one form, leaving the function in the `Value` register.
fn compile(heap: &mut Heap, source: &str) {
    heap.mutate(|mc| {
        let chars = Reader::chars_of(source);
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).expect("reads").expect("a form");
        let quote = mc.intern("QUOTE");
        let inner = mc.cons(form, Value::NIL);
        let quoted = mc.cons(quote, inner);
        let compile = mc.intern("COMPILE-FORM");
        let args = mc.cons(quoted, Value::NIL);
        let call = mc.cons(compile, args);
        mc.set_register(Register::Value, call);
    });
    crate::eval::eval_sliced(heap, 10_000_000, 2_000).expect("compiles");
}

/// Compile one form and start the VM on it, without running anything.
fn start(heap: &mut Heap, source: &str) {
    compile(heap, source);
    heap.mutate(|mc| {
        let f = mc.register(Register::Value);
        Vm::start(mc, f, Value::NIL);
    });
}

/// One slice, and whatever error it ended with.
fn slice(heap: &mut Heap) -> (Status, Option<EvalError>) {
    heap.mutate(|mc| {
        let mut vm = Vm::resume(mc);
        let status = run_slice(&mut vm, 100_000);
        (status, vm.error.take())
    })
}

/// The string in the `Value` register.
fn answer(heap: &mut Heap) -> String {
    heap.mutate(|mc| mc.string_to_rust(mc.register(Register::Value)))
}

/// The protocol, one step at a time: the machine blocks, the host reads the
/// request, nothing happens until it answers, and then the computation
/// carries on with the answer as the primitive's return value.
///
/// In tail position — the whole form is the call — so the answer is
/// *returned* from the function rather than pushed, which is what the
/// record's `TAIL` slot exists to remember.
#[test]
fn a_read_blocks_until_the_host_answers() {
    let mut heap = compiler();
    start(&mut heap, r#"(%read-source "hello.lisp")"#);

    assert!(matches!(slice(&mut heap), (Status::Blocked { .. }, None)));
    assert_eq!(
        pending(&mut heap),
        Some(Request::ReadSource("hello.lisp".to_string()))
    );

    // Running again without answering does nothing, and says so.
    assert!(matches!(
        slice(&mut heap),
        (Status::Blocked { steps: 1 }, None)
    ));

    assert!(complete(&mut heap, Ok("(+ 1 2)".to_string())));
    assert_eq!(pending(&mut heap), None, "answered, so no longer pending");

    assert!(matches!(slice(&mut heap), (Status::Done { .. }, None)));
    assert_eq!(answer(&mut heap), "(+ 1 2)");
}

/// Not in tail position: the answer is pushed, and the computation around
/// the call goes on to use it. Through the synchronous driver this time,
/// which is how the native REPL runs compiled code.
#[test]
fn the_answer_lands_where_a_return_value_would() {
    let mut heap = compiler();
    compile(&mut heap, r#"(cons (%read-source "a.lisp") 7)"#);
    let mut host = Files(&[("a.lisp", "A")]);
    crate::vm::run_sliced(&mut heap, 10_000_000, 2_000, &mut host).expect("runs");
    let (car, cdr) = heap.mutate(|mc| {
        let v = mc.register(Register::Value);
        let car = mc.car(v);
        (mc.string_to_rust(car), mc.cdr(v).as_fixnum())
    });
    assert_eq!((car.as_str(), cdr), ("A", Some(7)));
}

/// `funcall`, `apply` and `multiple-value-call` all reach a primitive by a
/// different road, and every road must lead through the machine's
/// interception. `multiple-value-call` did not: it called the primitive table
/// directly, and was fixed alongside this (#35).
#[test]
fn every_road_to_the_primitive_blocks() {
    for source in [
        r#"(funcall #'%read-source "a.lisp")"#,
        r#"(apply #'%read-source (list "a.lisp"))"#,
        r#"(multiple-value-call #'%read-source (values "a.lisp"))"#,
    ] {
        let mut heap = compiler();
        compile(&mut heap, source);
        let mut host = Files(&[("a.lisp", "A")]);
        crate::vm::run_sliced(&mut heap, 10_000_000, 2_000, &mut host)
            .unwrap_or_else(|e| panic!("{source}: {e}"));
        assert_eq!(answer(&mut heap), "A", "{source}");
    }
}

/// A host that cannot answer produces a condition of Common Lisp's type for
/// it, signalled through the machine's ordinary unwinding — here reaching the
/// top unhandled, because compiled code cannot yet call the interpreted
/// `handler-case` (#33).
#[test]
fn a_missing_file_is_a_file_error() {
    let mut heap = compiler();
    compile(&mut heap, r#"(%read-source "missing.lisp")"#);
    let mut host = Files(&[]);
    let e = crate::vm::run_sliced(&mut heap, 10_000_000, 2_000, &mut host)
        .expect_err("there is no such file");
    let text = format!("{e}");
    assert!(text.contains("FILE-ERROR"), "{text}");
    assert!(text.contains("missing.lisp"), "{text}");
}

#[test]
fn a_name_that_is_not_a_string_is_refused() {
    let mut heap = compiler();
    compile(&mut heap, "(%read-source 42)");
    let e = crate::vm::run_sliced(&mut heap, 10_000_000, 2_000, &mut Files(&[]))
        .expect_err("not a string");
    assert!(matches!(e, EvalError::WrongType { .. }), "{e:?}");
}

/// **The point of keeping the request in the heap.** Saved while waiting,
/// the image reloads still waiting: a new host, in a new process, sees the
/// request, answers it, and the computation finishes.
///
/// With a Rust future or a callback this could not work at all — the thing
/// that was waiting would have been in the process that exited.
#[test]
fn a_request_survives_being_saved_in_an_image() {
    let mut heap = compiler();
    start(&mut heap, r#"(cons (%read-source "hello.lisp") 7)"#);
    assert!(matches!(slice(&mut heap), (Status::Blocked { .. }, None)));

    let bytes = crate::image::save(&mut heap);
    drop(heap);
    let mut reloaded = crate::image::load(&bytes, 1 << 22).expect("loads");

    assert_eq!(
        pending(&mut reloaded),
        Some(Request::ReadSource("hello.lisp".to_string())),
        "still waiting, in a process that never made the request"
    );
    assert!(complete(&mut reloaded, Ok("hi".to_string())));
    assert!(matches!(slice(&mut reloaded), (Status::Done { .. }, None)));
    let car = reloaded.mutate(|mc| {
        let v = mc.register(Register::Value);
        let car = mc.car(v);
        mc.string_to_rust(car)
    });
    assert_eq!(car, "hi");
}

/// `run_now` runs inside one mutation, so it cannot step out to let a host
/// answer. It says so rather than spinning.
#[test]
fn run_now_cannot_answer_and_says_so() {
    let mut heap = compiler();
    compile(&mut heap, r#"(%read-source "a.lisp")"#);
    let outcome = heap.mutate(|mc| {
        let f = mc.register(Register::Value);
        crate::vm::run_now(mc, f, 100_000).map(|_| ())
    });
    assert_eq!(outcome, Err(EvalError::Blocked));
}

/// The interpreter has no I/O, by decision (#35), and refuses loudly rather
/// than quietly: a one-sided rule is enforced only where it was tested.
#[test]
fn the_interpreter_refuses_to_ask() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    let e = heap.mutate(|mc| {
        let chars = Reader::chars_of(r#"(%read-source "a.lisp")"#);
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).expect("reads").expect("a form");
        crate::eval::eval_now(mc, form, 100_000).map(|_| ())
    });
    let text = format!("{}", e.expect_err("refused"));
    assert!(text.contains("only compiled code"), "{text}");
}

#[test]
fn completing_when_nothing_waits_does_nothing() {
    let mut heap = Heap::new();
    assert_eq!(pending(&mut heap), None);
    assert!(!complete(&mut heap, Ok("unasked".to_string())));
}

/// A request from an abandoned computation must not be answered into the
/// next one.
#[test]
fn starting_afresh_forgets_a_stale_request() {
    let mut heap = compiler();
    start(&mut heap, r#"(%read-source "a.lisp")"#);
    assert!(matches!(slice(&mut heap), (Status::Blocked { .. }, None)));
    start(&mut heap, "(cons 1 2)");
    assert_eq!(pending(&mut heap), None);
    assert!(matches!(slice(&mut heap), (Status::Done { .. }, None)));
}

/// The two halves of this weekend meeting: a host failure (#35) caught by a
/// compiled `handler-case` (#36). Before #36 this halted the machine, because
/// compiled code could not catch anything at all.
#[test]
fn a_compiled_handler_case_catches_a_missing_file() {
    let mut heap = compiler();
    compile(
        &mut heap,
        r#"(handler-case (%read-source "missing.lisp") (file-error () 'missing))"#,
    );
    crate::vm::run_sliced(&mut heap, 10_000_000, 2_000, &mut Files(&[])).expect("caught");
    let out = heap.mutate(|mc| crate::printer::print(mc, mc.register(Register::Value)));
    assert_eq!(out, "MISSING");
}
