# Asking the outside world

> **Where this fits.** Chapter 30 made the whole system a file that can be
> saved and resumed. Everything in it so far happens *inside* the heap: no
> Lisp program has been able to read a file, ask the time or draw on a
> screen. This chapter gives Lisp its first way to ask the outside world for
> something, and it makes that asking **asynchronous** from the very first
> request, before anything could be built on a shape that would have to be
> torn out later. It is also where Quokka starts to look like what it really
> is: an operating system, with programs that make requests and a layer
> beneath them that answers.

## The question that started it

This chapter began with a question asked late at night, about the window
system that is still four phases away. Many Lisp programs running side by
side, windows to redraw, an editor updating as you type, files read and
written: *aren't we building an operating system?*

Yes. Genera, the operating system of the Symbolics Lisp machines, was one.
Smalltalk-80 was one. Oberon, one of Quokka's three ancestors, was one. And
the design already said so: Quokka will have *processes*, lightweight threads
that the VM switches between. That is phase 3, and none of it was built here.

What *was* worth doing straight away was checking which decisions in the
current code would be expensive to undo once phase 3 arrived. Issue #34 lists
four. The most urgent was **the host interface**.

The **host** is whatever is running Quokka: the native terminal program on
your computer, or the browser tab. The VM itself is not allowed to touch files,
clocks or screens (rule 2); everything it needs from the outside world, it has
to get from the host. The *host interface* is how it asks.

## What reading the code turned up

The host interface was a Rust trait, roughly:

```rust
pub trait Host {
    fn write_stdout(&mut self, s: &str);
    fn read_source(&mut self, name: &str) -> Result<String, HostError>;
}
```

`read_source` takes a name and returns the file's text, there and then. That
is a **synchronous** call: the caller waits until the answer comes back.

It turned out that *nothing in Lisp could call it.* No primitive used the
host at all. The only callers were the terminal program and the browser
console writing their own banners. The trait had the wrong shape, but no Lisp
program was waiting on it yet.

And a file in the repository had seen this coming. `slice.rs`, written in
phase 0, said:

> There will be a third case, `Blocked`, when the browser needs to load a
> source file asynchronously … It is deliberately not here yet — … the shape
> of the request should be decided when there is a real caller for it rather
> than guessed now.

So the job was not "make a trait asynchronous". It was **give Lisp its first
way to reach the outside world, and make it asynchronous from the start**,
with a real caller so that the design was made for a real use rather than
guessed at. The design already named the first caller: *"Before Quokka has
file hosts, Lisp source files are loaded through a single host primitive."*

## Why a synchronous call cannot work in a browser

A browser tab runs your page on one thread. That thread is driven by an
**event loop**: the browser keeps a queue of things to do — a key was pressed,
a timer fired, some data arrived — and takes them off one at a time, running a
little JavaScript for each. While one piece of JavaScript is running, nothing
else happens. The page does not repaint. Clicks do not register. That is what
a "frozen tab" is.

Reading from the browser's storage or the network takes time, so the browser
never makes JavaScript wait for it. You ask, you get back a **Promise** — an
object that stands for "the answer, when it arrives" — and your code finishes.
Later, once the answer is there, the browser puts a new item on the event
loop's queue, and a callback you supplied runs with it.

A `read_source` that returns the text immediately cannot be written on top of
that. The only way to get the answer would be to sit in a loop until it
arrives, and while you sit there, the event loop stops, and the answer — which
is delivered *by* the event loop — never arrives. Rule 4 exists for exactly
this reason: the VM runs in slices and hands control back so that the browser
keeps turning.

## Three ways to be asynchronous

| | how | why not |
| --- | --- | --- |
| **Rust `async`** | the host interface returns a `Future`, and the VM `await`s it | a `Future` is state on the Rust side: rustc compiles the waiting into a hidden state machine. No image could save it, and it needs an *executor* to drive it, which the core crate cannot depend on (rule 2). Rule 4 says execution state lives in the heap. |
| **Callbacks** | the VM hands the host a Rust closure to call with the answer | a closure cannot be saved in an image either, and when the host calls it, it calls *into* the VM from outside any slice |
| **Requests in the heap** | the program leaves a request in the heap and the machine stops; the host reads it, does the work however suits it, writes the answer back into the heap and runs the next slice | — |

The third won. Notice that the reason the first two lost is the same reason in
both cases, and it is the reason chapter 30 worked at all: **anything that is
not in the heap is lost when you save the world.** A request that is heap data
survives being written to a file. A future or a closure does not.

## A system call, in miniature

If you have used an operating system, you have seen this shape already, from
the other side.

When a program on Linux wants to read a file, it does not read the disk. It
makes a **system call**: it puts the request in a known place and hands
control to the kernel, the part of the operating system that is allowed to
touch hardware. If the data is not ready, the kernel puts the program to
sleep, runs something else, and wakes the program up when the answer arrives.
To the program it looks as though `read` simply took a while and returned.

Quokka's version has the same three parts. The running computation is the
program. The host is the kernel. And the request sits in the heap, in a
register:

```text
   Lisp program                     VM                          host
   ────────────                     ──                          ────
   (%read-source "a.lisp")  ──▶  intercepts it,
                                 builds a request record,
                                 puts it in Register::Request,
                                 slice ends: Blocked     ──▶  pending(heap)
                                                              → ReadSource("a.lisp")
                                                              …reads the file,
                                                              now or later…
                                                         ◀──  complete(heap, Ok(text))
                                 next slice: picks up
   gets the text back     ◀──    the reply, pushes it,
   as if it had returned         carries on
```

There is a name for this arrangement in systems programming: a
**completion queue**. Linux's io_uring has a submission ring, where programs
put requests, and a completion ring, where the kernel puts answers. Windows
has *I/O completion ports*. In design-pattern books it is called the
**proactor** pattern. Quokka's version holds one request at a time, because
until phase 3 there is only one computation to make requests. The shape is the
same.

## How it is built

The primitive is `%read-source`, with a `%` because it is not Common Lisp
(chapter 22). It is the first primitive that **does not return**. When the VM
meets it, the machine does the work itself rather than calling the primitive
table, exactly as it already did for `error`, `funcall` and `values`:

```rust
if index == primitives::index(primitives::Prim::ReadSource) {
    // ... check there is one argument and it is a string ...
    let request = crate::host::make_request(self.mc, "%READ-SOURCE", name, tail);
    self.mc.set_register(Register::Request, request);
    return Some(Transition::Block);
}
```

`Transition::Block` is new. It ends the slice with `Status::Blocked`, the case
`slice.rs` promised in phase 0.

The request record is an ordinary five-slot vector:

| slot | what |
| --- | --- |
| `KIND` | which primitive asked, as a symbol: `%READ-SOURCE` |
| `ARGUMENT` | the file's name |
| `TAIL` | whether the call was the last thing its function did |
| `STATE` | `%WAITING`, `%REPLIED` or `%FAILED` |
| `REPLY` | the answer, or the condition to signal |

The `TAIL` slot is there because of chapter 28's tail calls. If
`(%read-source …)` is the last thing a function does, its answer has to be
*returned from that function* rather than left on the stack for the next
instruction. By the time the answer arrives, the machine has forgotten which
kind of call it was, so the record remembers.

The host's side is two functions. `pending` reads the request out of the heap
as a Rust value:

```rust
pub enum Request {
    ReadSource(String),
}
```

and `complete` writes the answer back. A host that can answer straight away —
the native terminal, or the tests — uses a third function that does both:

```rust
pub fn serve(heap: &mut Heap, host: &mut dyn Host) -> bool {
    let Some(request) = pending(heap) else {
        return false;
    };
    let reply = match request {
        Request::ReadSource(name) => host.read_source(&name),
    };
    complete(heap, reply)
}
```

That is the whole cost of "asynchronous" for a native host: one call between
slices. The protocol allows the answer to come later. This host just happens
to have it already. The old `Host` trait survives, demoted to what it really
was: a way for a host that can answer at once to answer.

On the next slice the machine sees the register is not empty and picks up the
answer:

```rust
fn resume_request(&mut self, record: Value<'h>) -> Transition {
    let state = self.mc.slot(record, request::STATE);
    if self.mc.symbol_is(state, request::WAITING) {
        return Transition::Block;
    }
    let reply = self.mc.slot(record, request::REPLY);
    let tail = self.mc.slot(record, request::TAIL) != Value::NIL;
    self.mc.set_register(Register::Request, Value::NIL);
    if self.mc.symbol_is(state, request::FAILED) {
        return self.start_unwind(reply, Value::NIL);
    }
    if tail {
        self.mc.set_register(Register::Values, Value::UNBOUND);
        self.do_return(reply)
    } else {
        self.push_fresh(reply);
        Transition::Continue
    }
}
```

A failure becomes a **condition** (chapter 20) of type `file-error`, which is
Common Lisp's own type for it, signalled through the same unwinding `error`
uses. A `handler-case` written for SBCL will catch it here too.

## What it buys

**The VM never waits inside a step.** The native host answers before the next
slice. A browser host will answer when a Promise resolves. It is the same
protocol in both cases.

**Determinism survives.** The computation is frozen until the answer arrives,
so it cannot tell whether it waited a microsecond or a minute. Rule 4 says
slicing must be invisible to the program, and this keeps it invisible across
I/O too.

**A pending request can be saved in an image.** This test is the point of the
whole design:

```rust
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
```

The program asked for a file. The world was saved while it waited, and the
process that saved it exited. A new process loaded the image, found the
request still sitting in the register, answered it, and the computation
finished with `("hi" . 7)`. With a Rust future or a callback this test could
not even be written, because the thing that was waiting would have been in
the process that exited.

**It generalises.** In phase 3, *blocked* becomes *parked*. Instead of ending
the slice, the scheduler moves on to another process and comes back once the
answer is in. Each process will hold its own request, and the `Request`
register will become the running process's one, kept close to hand. The
protocol does not change.

## The decision: the interpreter has no I/O

`%read-source` works only in compiled code. Called from the interpreter, it is
refused:

```
; bad %READ-SOURCE: asks the host for something, which only compiled code
  can do: the interpreter is the bootstrap evaluator and has no I/O
```

The interpreter exists to build an image. It reads its own sources from
`include_str!`, builds the image, and is never run again (chapter 24, #25).
Giving it I/O would have meant writing the whole blocking mechanism twice, for
an evaluator that nothing will run once #33 lands. It also makes #29's idea
stronger: a browser build without the interpreter loses nothing it could have
done with the outside world.

The refusal is loud on purpose. Chapter 29 found an interpreter that would
have quietly returned the wrong value when handed compiled code, and drew the
lesson: a rule enforced on one side only is enforced only where somebody
happened to test it.

The cost is real, and it is temporary. Until #33 compiles the prelude, the
only places `%read-source` can run are the compiling REPL and the tests.

## Found on the way

**A primitive reached by `multiple-value-call` skipped the machine.** The
instruction behind `multiple-value-call` called the primitive table directly,
while every other way of calling a function went through the machine's own
check for the primitives it must handle itself. So
`(multiple-value-call #'funcall …)` was already wrong, and `%read-source`
would have fallen through the same gap. It now goes the same way as every
other call, and a test sends `%read-source` down all three side roads —
`funcall`, `apply` and `multiple-value-call`.

**Compiled code could not catch a primitive's error.** The interpreter turns
an error inside a primitive, such as `(car 5)`, into a condition that a
`handler-case` can catch; the VM simply stopped. It was fixed straight
afterwards, as #36, and turned out to be four faults rather than one —
chapter 28 has them. Since then a compiled `handler-case` catches a
`file-error` from a missing file, which is the two changes meeting.

**The image format lasted two days.** Adding a register added a word to the
image header, so `FORMAT_VERSION` went from 1 to 2. Chapter 30 had fixed that
format two days earlier. That is an honest measure of how stable a format is
before anything depends on it, and it is why #34 says the format is stable
for phase 1 rather than forever. The new word went at the *end* of the header,
so every earlier word kept its position and the byte offsets in chapter 30's
tests stayed true.

## Trying it

```bash
cargo run -p quokka -- bootstrap --compile
```

```
quokka> (%read-source "hello.lisp")
"(+ 1 2)
"
quokka> (cons (%read-source "hello.lisp") 7)
("(+ 1 2)
" . 7)
quokka> (%read-source "nope.lisp")
; FILE-ERROR: not found: nope.lisp
```

## Status

Built: the protocol, the first request, the synchronous driver, and an image
that can be saved while a request is waiting.

Not built: **the browser's half.** The browser console still interprets, and
compiled code cannot call the interpreted prelude until #33, so there is not
yet any compiled computation running in a tab that could block. When there
is, the browser host will be a small JavaScript loop: run a slice; if it
ended `Blocked`, call `pending`, start the Promise, and when it resolves call
`complete` and run the next slice.

Also not built: **output.** `write_stdout` is still the host writing to its
own screen. Output in Common Lisp means *streams* and the special variable
`*standard-output*`, and special variables are #34's third question. And
`load`, which reads a file and evaluates each form in it, will be ordinary
Lisp written on top of `%read-source` once there is a `read-from-string` to
write it with.

---

**References.** Tanenbaum and Bos, *Modern Operating Systems* (4th ed., 2014),
chapter 1 on system calls and chapter 2 on blocking and processes. That is
this chapter seen from the kernel's side, and the book Quokka's own book is
modelled on. Axboe, *Efficient IO with io_uring* (2019), for submission and
completion queues as a modern kernel interface. Schmidt, Stal, Rohnert and
Buschmann, *Pattern-Oriented Software Architecture, Volume 2* (2000), for the
Proactor pattern. Goldberg and Robson, *Smalltalk-80* (1983), for a language
whose processes wait on I/O by blocking on semaphores that the virtual machine
signals.
