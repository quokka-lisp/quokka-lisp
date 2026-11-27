//! The host interface.
//!
//! CLAUDE.md rule 2: the core VM crate has no host dependencies. Everything the
//! VM needs from the outside world — console output, loading a Lisp source file,
//! and (later) the clock, storage and the display protocol — arrives from
//! whatever is hosting it. `quokka` is a host over `std`; `quokka-wasm` is
//! one over the browser.
//!
//! # Requests, not calls
//!
//! The obvious host interface is a trait the VM calls: `host.read_source(name)`
//! returns the text and the program carries on. It works natively and cannot
//! work in a browser, where reading storage or the network finishes *later*,
//! in a callback, and a VM that waited for it would freeze the tab — which is
//! precisely what rule 4 forbids.
//!
//! So the VM does not call its host. A program that wants something leaves a
//! **request** in the heap, in the [`Register::Request`] register, and the
//! machine stops with [`Status::Blocked`]. The host reads it with [`pending`],
//! does the work however suits it, writes the answer back with [`complete`],
//! and runs the next slice. The machine picks the answer up and carries on as
//! if the primitive had simply returned it.
//!
//! This is the **completion-queue** shape — io_uring's submission and
//! completion rings, Windows' I/O completion ports, the *proactor* pattern —
//! and in operating-system terms it is a **blocking system call**: a process
//! traps to the kernel, sleeps, and is woken with the result. Here the
//! computation is the process and the host is the kernel. Two alternatives were
//! turned down, and #35 has the table: Rust `async`, whose futures are state on
//! the Rust side that no image could save, and callbacks, which can no more be
//! saved and which call into the VM from outside a slice.
//!
//! What the shape buys:
//!
//! - **The VM never waits inside a step.** A native host answers before the
//!   next slice; a browser host answers when a Promise resolves. Same protocol.
//! - **A pending request is heap data**, so an image saved while waiting
//!   reloads still waiting, and the new host just sees the request.
//! - **Determinism survives.** The computation is suspended until the reply
//!   arrives, so it cannot observe how long it waited.
//! - **It generalises.** In phase 3, *blocked* becomes *parked*: the scheduler
//!   runs another process instead of returning to the host.
//!
//! # The trait is for hosts that can answer at once
//!
//! [`Host`] stays, for native hosts and the WASI tests, which can answer a
//! request synchronously: [`serve`] reads the pending request, asks the trait,
//! and completes it. A browser host does not implement it — it calls
//! [`pending`] and [`complete`] itself, around a Promise.
//!
//! [`Register::Request`]: crate::heap::Register::Request
//! [`Status::Blocked`]: crate::slice::Status::Blocked

use core::fmt;

use crate::heap::{Heap, Mutation, Register};
use crate::value::Value;

/// Why a host operation failed.
///
/// Deliberately coarse: the VM turns these into Lisp conditions, and the host
/// has no business describing its own errors in detail to the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The named resource does not exist on this host.
    NotFound(String),
    /// The host refused the operation (permissions, sandbox, quota).
    Denied(String),
    /// Anything else the host wants to report.
    Other(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::NotFound(what) => write!(f, "not found: {what}"),
            HostError::Denied(why) => write!(f, "denied: {why}"),
            HostError::Other(why) => write!(f, "{why}"),
        }
    }
}

/// A host that can answer requests at once.
///
/// Called by [`serve`] *between* slices, never from inside one — so an
/// implementation may take as long as it likes without breaking rule 4, though
/// the person at the keyboard will notice.
pub trait Host {
    /// Write a string to the host's console or dev-console transcript.
    ///
    /// The host's own output, and since #46 Lisp's too: a write to
    /// `*standard-output*` arrives here as a [`Request::WriteConsole`].
    fn write_stdout(&mut self, s: &str);

    /// Answer a [`Request::ReadSource`].
    ///
    /// Natively this is `std::fs`; under WASI it is the WASI filesystem.
    fn read_source(&mut self, name: &str) -> Result<String, HostError>;

    /// Answer a [`Request::ReadConsole`]: the next line from whoever is at
    /// the console, with its newline, after showing PROMPT; `None` at the
    /// end of the input (#131).
    ///
    /// By default there is nobody there, so the answer is the end of the
    /// input: a host with no console, a test or the browser page, needs to
    /// do nothing, and a world reading from it meets end of file.
    fn read_line(&mut self, prompt: &str) -> Option<String> {
        let _ = prompt;
        None
    }

    // Note: there is deliberately no `should_yield` or clock method here.
    //
    // Rule 4 says the VM must be able to yield, and something has to decide when
    // a slice ends. Three shapes were considered: the VM polling the host, the
    // host setting a wall-clock deadline the VM watches, or the caller passing a
    // step budget. The budget won — see the module docs on `crate::slice` for
    // the reasoning. It needs nothing from the host, which is why this trait
    // stays as small as it is.
}

/// A host that discards output and has no files. Useful in unit tests.
#[derive(Debug, Default)]
pub struct NullHost {
    /// Everything written to stdout, kept so tests can assert on it.
    pub output: String,
}

impl Host for NullHost {
    fn write_stdout(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        Err(HostError::NotFound(name.to_string()))
    }
}

/// The layout of a request record: what [`Register::Request`] holds.
///
/// An ordinary vector, because no Lisp code ever holds a reference to one —
/// it lives only in the register, so it cannot be mistaken for a vector the
/// program made. (Chapter 20 is the story of what happened when a type *was*
/// shared between two purposes that could meet.) Its kind and states are
/// symbols rather than numbers so that a request prints as something a person
/// can read, which matters in a debugger and will matter more to phase 3's
/// scheduler, written in Lisp.
///
/// [`Register::Request`]: crate::heap::Register::Request
pub mod request {
    /// Which primitive asked: its name, as a symbol. `%READ-SOURCE`.
    pub const KIND: usize = 0;
    /// What it asked about — the file's name.
    pub const ARGUMENT: usize = 1;
    /// `t` if the call was in tail position, so the answer must be *returned*
    /// rather than pushed. The machine would otherwise have forgotten by the
    /// time the answer arrives.
    pub const TAIL: usize = 2;
    /// One of [`WAITING`], [`REPLIED`], [`FAILED`].
    pub const STATE: usize = 3;
    /// The answer, or the condition to signal, once there is one.
    pub const REPLY: usize = 4;
    /// How many slots.
    pub const SLOTS: usize = 5;

    /// The host has not answered.
    pub const WAITING: &str = "%WAITING";
    /// The host answered; the reply is a value to hand to the program.
    pub const REPLIED: &str = "%REPLIED";
    /// The host could not; the reply is a condition to signal.
    pub const FAILED: &str = "%FAILED";
}

/// What a program can ask its host for.
///
/// A file host, the clock, the network and the display protocol will each
/// add some, and each will be a request rather than a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The text of a Lisp source file. Made by `%read-source`.
    ReadSource(String),
    /// Text for the console. Made by `%write-console`, which is what
    /// `*standard-output*` writes with (#46). Answered with an empty string.
    WriteConsole(String),
    /// A line from the console, the argument being the prompt to show. Made
    /// by `%read-console`, which is what the console input stream refills
    /// itself with (#131). Answered with [`complete_line`].
    ReadConsole(String),
}

/// Make a waiting request record. Called by the VM when it intercepts a
/// primitive that needs the host.
pub(crate) fn make_request<'h>(
    mc: &mut Mutation<'h>,
    kind: &str,
    argument: Value<'h>,
    tail: bool,
) -> Value<'h> {
    let record = mc.alloc(crate::heap::ObjectType::Vector, request::SLOTS);
    let kind = mc.intern(kind);
    let waiting = mc.intern(request::WAITING);
    mc.set_slot(record, request::KIND, kind);
    mc.set_slot(record, request::ARGUMENT, argument);
    mc.set_slot(
        record,
        request::TAIL,
        if tail { Value::T } else { Value::NIL },
    );
    mc.set_slot(record, request::STATE, waiting);
    mc.set_slot(record, request::REPLY, Value::NIL);
    record
}

/// The request the machine is waiting on, if any.
///
/// `None` when nothing is pending — including when the host has answered and
/// the machine simply has not run since to collect the answer.
pub fn pending(heap: &mut Heap) -> Option<Request> {
    heap.mutate(|mc| {
        let record = mc.register(Register::Request);
        if record == Value::NIL || !mc.symbol_is(mc.slot(record, request::STATE), request::WAITING)
        {
            return None;
        }
        let kind = mc.slot(record, request::KIND);
        let argument = mc.slot(record, request::ARGUMENT);
        let text = mc.string_to_rust(argument);
        if mc.symbol_is(kind, "%READ-SOURCE") {
            return Some(Request::ReadSource(text));
        }
        if mc.symbol_is(kind, "%WRITE-CONSOLE") {
            return Some(Request::WriteConsole(text));
        }
        if mc.symbol_is(kind, "%READ-CONSOLE") {
            return Some(Request::ReadConsole(text));
        }
        None
    })
}

/// Answer the pending request. Returns whether there was one.
///
/// A success becomes a Lisp value that the program receives as the
/// primitive's result. A failure becomes a condition the program receives as
/// a signal — `file-error` for a file, which is Common Lisp's type for it
/// (CLHS 5.3), so a `handler-case` written for SBCL catches it here too. The
/// condition's data is the file's name, which is what `file-error-pathname`
/// will read.
///
/// The work of building the answer happens here, outside any slice, in the
/// host's own time. That is incidentally kind to rule 9: a large file costs
/// one allocation between slices rather than a long step inside one.
pub fn complete(heap: &mut Heap, reply: Result<String, HostError>) -> bool {
    heap.mutate(|mc| {
        let record = mc.register(Register::Request);
        if record == Value::NIL || !mc.symbol_is(mc.slot(record, request::STATE), request::WAITING)
        {
            return false;
        }
        let (state, answer) = match reply {
            Ok(text) => (request::REPLIED, mc.alloc_string(&text)),
            Err(e) => {
                let argument = mc.slot(record, request::ARGUMENT);
                let kind = mc.intern("FILE-ERROR");
                let message = mc.alloc_string(&e.to_string());
                (
                    request::FAILED,
                    crate::condition::make(mc, kind, message, argument),
                )
            }
        };
        let state = mc.intern(state);
        mc.set_slot(record, request::STATE, state);
        mc.set_slot(record, request::REPLY, answer);
        true
    })
}

/// Answer a [`Request::ReadConsole`] with a line, or with `nil` for the end
/// of the input, which is not a failure: a stream meets end of file.
/// Returns whether a request was waiting.
pub fn complete_line(heap: &mut Heap, line: Option<String>) -> bool {
    match line {
        Some(text) => complete(heap, Ok(text)),
        None => heap.mutate(|mc| {
            let record = mc.register(Register::Request);
            if record == Value::NIL
                || !mc.symbol_is(mc.slot(record, request::STATE), request::WAITING)
            {
                return false;
            }
            let replied = mc.intern(request::REPLIED);
            mc.set_slot(record, request::STATE, replied);
            mc.set_slot(record, request::REPLY, Value::NIL);
            true
        }),
    }
}

/// Answer the pending request through a synchronous [`Host`], if there is one.
/// Returns whether there was.
///
/// The whole of what "asynchronous" costs a native host: one call between
/// slices. The protocol permits the answer to come later; this host simply
/// has it now.
pub fn serve(heap: &mut Heap, host: &mut dyn Host) -> bool {
    let Some(request) = pending(heap) else {
        return false;
    };
    let reply = match request {
        Request::ReadSource(name) => host.read_source(&name),
        Request::WriteConsole(text) => {
            host.write_stdout(&text);
            Ok(String::new())
        }
        Request::ReadConsole(prompt) => {
            let line = host.read_line(&prompt);
            return complete_line(heap, line);
        }
    };
    complete(heap, reply)
}
