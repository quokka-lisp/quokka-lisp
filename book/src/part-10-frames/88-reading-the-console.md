# Reading the console

> *Where this fits.* Chapters 86 and 87 gave Lisp the frames on the stack
> and a way to run one again. A debugger also has to ask the person at the
> terminal what to do, and wait for the answer while the failed program
> stays where it stopped. Until this chapter a Quokka world could write to
> its console and could not read from it: everything it read came from the
> host, a form at a time. This chapter gives a world a console it can read,
> which chapter 89's debugger reads its commands from, and which any
> interactive program can use too.

*Status: done, issue #131. Phase 3, step F, sub-step 1. The design is
`docs/debugger.md`.*

## Who reads whom

Chapter 40 built the read-eval-print loop in Lisp, and chose to have the
host drive it. The terminal host reads lines until it has a whole form, and
hands the text to the world, which reads it, evaluates it, and returns what
it printed:

```lisp
(defun %host-step (ignored)
  (let ((%aborted-condition nil))
    (restart-case
        (let* ((start (%stream-position %host-input))
               (form (read %host-input nil %end-of-input))
```

`%host-input` is a string stream over what the host handed in. The world
never waits for a person; when it has finished with the text, it returns.

A debugger needs the opposite arrangement. When `(broken 10)` signals an
error, the frames of `caller` and `broken` are on the stack, and they must
stay there while the person types `backtrace`, then a new `defun`, then
`restart-frame`. If the world returned to the host to collect each command,
it would be returning out of the computation it is supposed to be
debugging. So the world has to be the one that asks, in the middle of a
computation, and wait for the answer.

## A host request for a line

The world already had a way to ask its host for something and wait. A
world has no files of its own, so `%read-source` leaves a *request* in the
heap and the machine stops; between two slices the host sees the request,
reads the file, and puts the answer where the machine will find it
(chapter 31). Console output works the same way: every write to
`*standard-output*` is a `%write-console` request.

Reading the console is a third request of the same kind. `%read-console`
takes a prompt, the host shows it and reads a line, and the answer is the
line with its newline, or `nil` at the end of the input. On the host's side
it is one new method with a default:

```rust
    fn read_line(&mut self, prompt: &str) -> Option<String> {
        let _ = prompt;
        None
    }
```

The default means a host with nobody at its console has nothing to write:
the test harness and the browser page answer every read with the end of
the input, as a terminal does after Ctrl-D. The terminal host reads
standard input, the same input the REPL reads forms from, so a program that
reads a line gets the line typed after the form that asked for it.

## A stream that refills itself

Lisp reads through streams, and the debugger wants to call `read` on the
console as it would on any stream, so that a command can span lines when
it is a long `defun`. Quokka's input streams are string streams: a string,
a position, an end. The reader takes characters from them one at a time
through `%next-char`, and chapter 58 found that the cost of that function
is most of what reading costs.

So the console is a string stream over the line it was given last, and the
only change is at the end of the string, where a read used to report end of
file:

```lisp
(defun %next-char (stream)
  (let ((at (%string-input-stream-position stream)))
    (if (< at (%string-input-stream-end stream))
        (progn (setf (%string-input-stream-position stream) (+ at 1))
               (aref (%string-input-stream-string stream) at))
        (if (%refill-console stream) (%next-char stream) nil))))
```

`%refill-console` does nothing unless the stream is the console; for the
console it asks the host for a line and makes it the stream's string. The
path a character takes when there is one is the same as before, so reading
a file costs what it did. `*standard-input*` is now the console. The
standard streams that Common Lisp makes two-way, `*terminal-io*`,
`*debug-io*` and `*query-io*`, were Quokka's console output stream, and
reading from that stream now reads the console. That also mended
`check-type`: its `store-value` restart asks for a new value on
`*query-io*`, and until now there was nothing there to read.

Writing a test for this turned up a gap in tier 1. Common Lisp's
`read-line` did not exist, because nothing in the library had needed it.
It does now, with a differential file against SBCL.

## The step budget

There was one more thing in the way. The terminal REPL gives each line it
evaluates five million steps, the *step budget* of chapter 2, because
without a way to interrupt a program nothing else stops one that never
finishes. A debugging session runs entirely inside the line that failed,
so a person looking at a few backtraces and redefining a function or two
would have run the budget out and lost the session.

The budget stands for an interrupt nobody can press yet, and a person
typing a line is where one piece of work ends and the next begins. So the
machine's driver now renews the budget when the host answers a console read
with a line:

```rust
                if let Some(crate::host::Request::ReadConsole(prompt)) = crate::host::pending(heap) {
                    let line = host.read_line(&prompt);
                    if line.is_some() {
                        spent = 0;
                    }
                    crate::host::complete_line(heap, line);
                } else if !crate::host::serve(heap, host) {
```

Our first version renewed it for the end of the input too, and the first
test of the debugger never finished. A mistake in the debugger made it read
the end of the input over and over, each read renewed the budget, and the
budget was the only thing that could have stopped it. Only a line renews it
now.

## What did it cost?

In the machine, the request and the budget, since only the machine can
block and only the driver counts steps. The terminal host reads a line. The
stream, the refill and `read-line` are Lisp. The test harness types lines
for a test (`run_compiled_typing` in `differential.rs`) and writes them
into the transcript after their prompts, so a test's expected output reads
like a session at a terminal.

## What is still missing?

The browser page answers every console read with the end of the input. Its
input box hands whole forms to the host loop, and making it answer a
program's reads as well belongs with the listener of phase 4. Streams are
still the few kinds of struct chapters 38 and 39 began with; general streams, which
a program can define for itself, are tier 2.
