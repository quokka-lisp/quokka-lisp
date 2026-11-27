# Phase 3 ends

> *Where this fits.* Phase 3 set itself one gate: fix a function from
> inside an error and retry the frame. Six steps built what that sentence
> needs. Classes and generic functions came first (Part VII), then
> conditions and restarts (Part VIII), definitions that keep their source
> (Part IX), frames that keep their calls, a console and a debugger
> (Part X), and processes and channels (Part XI). This chapter shows the
> gate passing through the real program, the way a person meets it, and
> then says what it means and what it does not.

*Status: done, issue #133. Step G of phase 3, and phase 3's gate.*

## The gate, through the program

Every test in the last few chapters drove the world through the test
harness, which plays the part of a host. A gate should be met the way a
person meets it, so the gate's test does something none of the others do.
It builds a world with the `quokka` program itself and runs it, typing a
session on its standard input:

```text
(quokka-debug:enable-debugger)
(defvar *calls* 0)
(defun broken (x) (error "broken ~a" x))
(defun caller (y) (incf *calls*) (list :got (broken (* y 2))))
(caller 5)
backtrace
source
(defun broken (x) (+ x 1))
restart-frame
*calls*
(caller 6)
```

Then it checks what the program wrote, in order:
1. The banner says `debugger invoked on a SIMPLE-ERROR: broken 10`.
2. The current frame is `(BROKEN 10)`.
3. The backtrace shows `BROKEN` called by `CALLER`.
4. `source` prints the definition as it was typed.
5. The new `defun` answers `BROKEN`.
6. `restart-frame` makes `(caller 5)` return `(:GOT 11)`.
7. `*calls*` is 1: `caller` ran once, so the retry ran `broken`'s frame, not
   the whole form again.
8. At the `quokka>` prompt, `(caller 6)` gives `(:GOT 13)`, the fix still in
   place.

The first line is needed because the terminal host turns the debugger on
only when its input is a terminal (chapter 89), and here the input is a
pipe.

The second scenario is the same error in a thread, which the main thread
waits for with `join-thread`. The debugger opens in that thread and says
so, *debugger invoked on a SIMPLE-ERROR in thread "worker"*. The fix and the
retry happen there, and `join-thread` in the main thread returns
`(:GOT 11)`. That one sentence of a session needs every part of the
phase:
- a condition signalled and not handled (chapter 80);
- a debugger reading the console from a thread, while the main thread waits
  (chapters 88, 89 and 91);
- a frame that kept its call (chapter 86);
- a redefinition that the retry finds by name (chapter 87);
- source text kept for `source` (chapter 84);
- the thread's own special bindings swapped in and out underneath all of it
  (chapter 90).

The design's test plan for this phase asked for two more kinds of evidence:
scripted handlers that invoke restarts programmatically, and seeded,
deterministic scheduler tests. Both exist. Chapter 82's tests and the
debugger's sessions invoke restarts by number, by name and through their
interactive functions. Chapter 90's scheduler is deterministic by
construction, and its tests check exact interleavings.

## What it means

A world can now be worked on while it runs. A mistake does not throw away
the computation that met it. The stack stays, the definition can be read and
changed, and the call can be made again. That is the feature that made the
Lisp machines and Smalltalk feel different from editing a file and
starting a program again, and it is the reason phase 3 came before any
graphics: everything the environment of phases 4 to 6 shows will be built
on it.

## What it does not mean

The debugger is a terminal one, and has the terminal's limits. A form typed
at it is evaluated where the error happened, but in the error's dynamic
environment and not its lexical one, so it cannot name the frame's local
variables; `list-locals` shows them. Ctrl-C does not reach it, because that
needs the machine to interrupt a process in the middle of an instruction,
which chapter 90 described and left for later. A method's frame retries the
method it was running, not a redefined one; the frame that called the
generic function has to be retried for that.

Processes are green threads in one world. The plan for using more than one
processor is decided and not built (#132). In the browser, nothing reads
the console yet, so the debugger stays off there until phase 4 gives the
page a listener.

## What did it cost?

The gate itself cost a test file in `crates/quokka/tests`, and the banner,
which said phase 2 until now. Phase 3 as a whole is chapters 72 to 93. Each
step had its design written down before it was built, and each was
reviewed by someone who had not written it. Those reviews found real faults
in every step, and chapters 87 to 92 say what they were. The fixpoint, the check that the compiler compiles
itself into the same code generation after generation (chapter 42), held
through all of it. ansi-test was run again after each step, and stayed at
12,568 of 17,739 passing, with no plain failure, as chapter 83 left it.

Phase 4 starts from here: a display protocol, a renderer in the browser,
and one viewer of live text in which a form can be edited and evaluated
(`docs/design.md`).
