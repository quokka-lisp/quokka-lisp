# The debugger

> *Where this fits.* This is the chapter phase 3 has been building towards.
> Restarts (chapter 82) are the ways out of an error; definitions that keep
> their source (chapters 84 and 85) let a function be shown and changed;
> frames that keep their calls (chapters 86 and 87) let a call be run again;
> and the console (chapter 88) lets the world ask a person what to do. This
> chapter puts them together into a debugger at the terminal, in which you
> can fix a function from inside the error it caused and retry the frame,
> which is the gate of phase 3.

*Status: done, issue #131. Phase 3, step F, sub-steps 2 and 3. The design is
`docs/debugger.md`.*

## A session

Here are two functions, one of them wrong, and the call that finds out:

```
quokka> (defun broken (x) (error "broken ~a" x))
BROKEN
quokka> (defun caller (y) (list :got (broken (* y 2))))
CALLER
quokka> (caller 5)

debugger invoked on a SIMPLE-ERROR: broken 10

Type HELP for debugger help.

restarts (invokable by number or by name):
  0: [ABORT] Return to the top level.

0: (BROKEN 10)
0] backtrace
0: (BROKEN 10)
1: (CALLER 5)
0] (defun broken (x) (+ x 1))
BROKEN
0] restart-frame
(:GOT 11)
quokka>
```

The error did not end `(caller 5)`. The world stopped at the error with both
calls still on the stack, said what went wrong and what the ways out were,
and waited. `backtrace` showed the two calls. The `defun` typed at the `0]`
prompt was evaluated where the error happened, and replaced `broken`.
`restart-frame` ran `broken`'s call again, with the argument it was first
called with, through the new definition, and the 11 it returned went to
`caller`, which finished as if nothing had gone wrong. `(caller 5)` returned
`(:GOT 11)`.

That is a transcript from the terminal, and the same session is a test,
`the_debugger_fixes_and_retries`, with the three lines typed by the test
harness of chapter 88.

## As SBCL does it

A debugger is the part of a Lisp people use most when something goes wrong,
and they bring their habits with them. So we made Quokka's look and answer
like SBCL's: the same banner, the restarts numbered, the current frame, the
`0]` prompt, and SBCL's command names. The prompt's number counts the
debuggers open around this one. An error in something typed at the
debugger opens another debugger inside it, at `1]`, which has one restart
more, *Return to debug level 0*, and `abort` takes it.

What a person types is read with the console stream's `read`, and decided
in the order SBCL decides it:

1. A number invokes that restart, interactively, so a restart that needs a
   value asks for one.
2. A symbol that names a command runs it, whatever package the symbol is
   in: `backtrace`, `frame 3`, `up`, `down`, `list-locals`, `source`,
   `restart-frame`, `return 42`, `abort`, `help` and a few more. A
   command's arguments are the rest of its line, as in SBCL.
3. A symbol that names one of the restarts invokes it: `continue`,
   `store-value`.
4. Anything else is evaluated, and its values printed.

The arguments are why the command is read with `read-preserving-whitespace`
and not `read`. `read` may consume the character that ends a symbol, and
when that character is the newline, the rest of the line has gone with it
and `frame` waits for an argument on the next line.

Each command is a small function registered under its names:

```lisp
(%define-debugger-command ("RESTART-FRAME") ()
    "Run the current frame's call again, with its function's current definition."
  (if (%current-frame)
      (restart-frame (%current-frame))
      (format (%debug-out) "~&There are no frames.~%")))
```

The registration replaces a command of the same name where it stands
instead of adding another. The warm rebuild of chapter 41 loads this file
into a world that already has every command, and a list that grew by one
copy each time would make every rebuilt world differ from the one before,
which the fixpoint test notices.

## When there is nobody to ask

A world does not always have a person at its console. The differential
tests run thousands of forms that signal errors on purpose, scripts are
piped into `quokka world.qi`, the build runs the library, and the browser
page has no console input at all. Each of these, before this chapter, got
the error reported and carried on with the next form, and a debugger that
printed its banner and waited would break all of them.

So the debugger has a switch, SBCL's `enable-debugger` and
`disable-debugger`, and it is off in a new world. With it off,
`invoke-debugger` does what it did before: it calls a program's
`*debugger-hook*`, then the innermost `abort` restart, so the host reports
the error. The terminal host turns it on when its standard input is a
terminal, and off when it is not, every time it starts a world, since a
world saved at a terminal may next be given a script. SBCL instead enters
its debugger whatever the input, and a script has to say
`--non-interactive`; we kept the behaviour scripts already had.

With the debugger on, the end of the console's input does what an error did
without it. Our first version did that from inside its own command loop,
and there the innermost `abort` restart was the loop's own, so the debugger
aborted back into itself, read the end of the input again, and went round
for ever. Ending it outside the loop's restart fixed it.

## Which frames to show

When the debugger starts, the stack holds a good deal that is not the
program's: the debugger's own frames, the handler that declined the
condition, `error` and `%signal`, and at the bottom the host loop's frames
and `eval`. SBCL starts its backtrace at the frame that called `error`,
and so does Quokka. The frames are cut above the innermost of the
signalling functions (`error`, `signal`, `invoke-debugger`,
`%signal-machine-error` and their relatives), and the host loop's are cut
off the bottom. In a debugger opened by an error in something typed at
another debugger, the frames stop where that form was evaluated: the stack
underneath is the first debugger's, which it still shows.

## Methods, variables and source

A method's frame read `(LAMBDA 4)` at the end of chapter 87, because the
body of a method is an anonymous `lambda` that `defmethod` writes. SBCL
names it `(:METHOD AREA (INTEGER))`. A function is named by whoever compiles
its definition, through `%pending-name` (chapter 30), and `defmethod` does
not compile anything: it writes a `lambda` form for the compiler. So the
name travels in the form, as a declaration the compiler reads:

```lisp
           (call `(apply (lambda ,inner
                           (declare (%named (:method ,name ,@qualifiers ,specializers)))
                           ,@declarations (block ,block-name ,@forms))
                         ,args)))
```

#110 had turned down a `%named-lambda` operator, because every program that
walks code would have had to learn it. A declaration is different: every
code walker already passes declarations by, and the interpreter ignores
them.

We had planned for retrying a method's frame to call its generic function
again, so that a redefined method would be found, as redefining a function
takes effect. Writing it showed when that is wrong. A method reached through
`call-next-method`, or a primary method with before and after methods
around it, has generic-function machinery below its frame, and calling the
generic function again in its place would run those other methods twice. So
retrying a method's frame runs that method again, as in SBCL, and a
redefined method is reached by retrying the frame that called the generic
function.

`list-locals` shows a frame's variables. Chapter 86's `%frames` now also
returns each frame's environment, the one saved in the header of the frame
it called. A primitive, `%frame-locals`, walks the scopes from there down to
the scope the function closed over, below which the variables belong to
the code around the function. Names the compiler made up, such as the
names chapter 64's lambda lists bind `&optional` parameters under first,
are left out. `source` prints the definition's text, as chapter 84 kept it.

## What the review found

A reviewer who had not written the code tried to break it, and found nine
faults. Two of them show how the pieces of this part meet.

The first was the newline. The debugger reads a command with
`read-preserving-whitespace` so that its arguments stay on the line, but a
number, a restart's name or a form typed at the prompt leaves its newline
in the console's buffer as well. Nothing in the debugger reads it, so it
waited there, and the next program to call `(read-line)` after the
debugger had gone got an empty line. The debugger now skips the rest of a
line that holds nothing but blanks, and clears what was typed ahead when
it starts, with the standard `clear-input`, which tier 1 lacked.

The second was a method with `&optional` parameters, which was still named
`lambda`. Chapter 64's rewrite of such a lambda list builds a `let*` for the
defaults and moves the body's declarations into it, so by the time the
function was assembled the `%named` declaration was no longer at the front
of its body. The compiler now reads the name before the rewrite. The other
seven are listed in `docs/debugger.md`, with the test that holds each.

## What did it cost?

One primitive in the machine, `%frame-locals`, since only the machine knows
how scopes are laid out. The terminal host decides whether the debugger is
on. The compiler reads one more declaration. The debugger is Lisp, in
`lisp/library/debugger.lisp`, and the switch and `invoke-debugger`'s choice
are in `conditions.lisp`. Its tests are sessions typed by the harness: the
gate itself; nested levels, restarts by number and by name, `return`, and
the end of the input; and methods, locals and source.

## What is still missing?

A form typed at the debugger is evaluated in the dynamic environment of the
error, but not its lexical one, so it cannot name the frame's local
variables; `list-locals` shows them instead. Ctrl-C does not reach the
debugger yet: it needs the host to interrupt a running world, which comes
with processes in step E. And the browser page has no console to read, so
its worlds keep the debugger off until phase 4's listener gives them one.
