# The listener

> *Where this fits.* Chapters 94 to 97 built a protocol, records, viewers, a
> command loop and a renderer, and tested them with small viewers made for
> the purpose. This chapter builds the first real viewer, the listener: the
> place where a person types a form and sees what it does, as in every
> Lisp, but on a page of live text where every earlier form can be changed
> and run again. With it, the terminal debugger of chapter 89 also comes to
> the browser.

*Status: done, issue #141, step G of phase 4.*

## A transcript you can edit

A Lisp's listener is a read-eval-print loop: a prompt, a form, its value,
and the next prompt. The books print their examples that way, and Quokka's
listener looks the same, `CL-USER>` and all. The difference is that the
transcript is made of records, and each form in it is an editor. You can
scroll up to a form you typed ten minutes ago, change it, and run it again.

What should happen then was decided when the environment was designed
(`docs/design.md`, *The listener*). A notebook such as Jupyter runs the
edited cell in place and replaces its result. We chose to run it as a new
entry at the bottom instead, with the old result struck through and a note
on each entry pointing at the other, because in Lisp the order of
evaluation matters: a `defun` replaced in place leaves a page whose later
results were computed with the old definition, and nobody reading the page
could tell. Appending keeps the transcript a true record of what ran, in
the order it ran. Chapter 97 showed the screen this makes:

```text
CL-USER> [(defun twice (x) (* 3 x))]
~TWICE~
edited · ran again below
CL-USER> [(twice 21)]
42
CL-USER> [(defun twice (x) (* 3 x))]
↑ from 2 forms up
TWICE
CL-USER> [(twice 21)]
63
```

The first `defun` was edited from `(* 2 x)` to `(* 3 x)` and run again with
Cmd-Enter. Its old result is struck through, it says that it was edited
and ran again below, and the new entry says where it came from. The
`(twice 21)` that ran before the change keeps its 42, which is what it
returned at the time.

## Entries

Each entry is a block of records: a row with the prompt and the editor, a
block for what the form printed, set apart by a bar as mock-up 01 shows, a
block for its values, and any notes. The keys are a key table on the
editor (chapter 97). Enter at the bottom runs the form if it is complete
and makes a new line if it is not; the world's own reader decides, with the
function the terminal REPL already used to know whether to read another
line (#119). Cmd-Enter runs the form in any entry. While a form runs, Enter
in another entry starts nothing, so that one form never runs inside
another by accident.

The prompt is drawn by a generic function, so a person can change it from
inside the world, as mock-up 03 of the design does:

```lisp
(defgeneric listener-prompt (listener stream))
(defmethod listener-prompt ((listener listener) stream)
  (format stream "~A> " (%short-package-name *package*)))
```

`%short-package-name` gives the package's shortest name, as SBCL's REPL
does, so `COMMON-LISP-USER` is shown as `CL-USER`. A test defines another
method and finds the next prompt changed, and then asks the prompt's
record which function drew it, which is the method (chapter 96):

```text
(:METHOD QUOKKA-ENV:LISTENER-PROMPT (QUOKKA-ENV:LISTENER T))
```

## Running a form

A form runs in the command loop's process, inside the command that handles
Enter, with the standard streams bound to the listener's own stream. That
stream is a Gray stream (chapter 95) that goes both ways. What a form
writes goes into its entry's output block. What it reads comes from lines
typed into the entry: when a read finds nothing, the stream shows the
prompt the reader asked for, adds an editor for the line, and runs the
command loop until Enter is pressed in it.

```lisp
    (unwind-protect (read-event-loop (lambda () (%line-entered stream)))
      (setf (%listener-reading-editor stream) nil)
      ;; What follows the line is written after it.
      (setf (%listener-current-output stream) (make-record-stream (stream-container out))))
```

So `(list (read-line) :after)` shows an editor under the form, waits, and
returns `("hello there" :AFTER)` when you type the line. The listener also
sets the REPL's variables that CLHS describes, `*`, `**` and `***` for the
last values, `+`, `++` and `+++` for the last forms, and `/`, `//` and `///`
for the last lists of values, as SBCL's does.

## The debugger in the page

Until now the browser page answered every console read with end of file,
so the debugger stayed off there (chapter 89). The listener is a console
that can answer, so the environment turns the debugger on, and a form runs
under a handler that calls it. The debugger needed no change at all. It
reads its commands from `*debug-io*`, which is the listener's stream, and
writes its banner and backtrace there:

```text
CL-USER> (list (broken 5))
  debugger invoked on a TYPE-ERROR: The value 5 is not of type LIST

  Type HELP for debugger help.

  restarts (invokable by number or by name):
    0: [ABORT] Return to the listener.
    1: [ABORT] Return to the command loop.

  0: (BROKEN 5)
  0] (defun broken (x) (if (consp x) (car x) x))
  BROKEN
  0] restart-frame
(5)
```

That session, done in a browser, is phase 3's gate again: a function fixed
from inside its error and its frame retried. Two small things had to be
taught to the debugger's backtrace, as lists it already keeps: the
listener's evaluation is where a backtrace stops, as the evaluation of a
form typed at an outer debugger is, and the listener's handler is not shown,
as `error` itself is not.

## What did it cost?

One library file, `listener.lisp`, in the package `QUOKKA-ENV`, and no
Rust. Its tests in `crates/quokka-wasm` type into the
listener through the same object the page uses: a form, an unfinished
form, output, an edited form run again, `read-line`, an error with the
debugger and a restart chosen at its prompt, and a changed prompt. Using it
in a real browser found that the prompt sat on a line above its form, which
the tests could not see; a row in the record tree fixed it, and the way a
row is drawn is the renderer's business.

The review found a worse fault. Stopping a form from the page abandons the
computation without running its `unwind-protect` cleanups (chapter 94), and
one of those cleanups was what told the listener that no form was running
any more. After a Stop, or a form that ran out of heap, the listener
believed a form was still running and ignored every Enter. Now the command
loop, when it starts again, asks each viewer to put right what a cleanup
would have (`viewer-restarted`), and the listener's method clears its state
and opens a new prompt; the tests stop a form, run one out of heap, and
leave the debugger for the command loop, and then type another form.
