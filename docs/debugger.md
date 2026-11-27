# The terminal debugger

Phase 3, step F (#131). An error nobody handles stops the program and talks
to the person at the terminal. They can look at the stack, evaluate forms
where the error happened, redefine a function, and retry a frame or choose a
restart. The book's chapters 88 and 89 tell the story; this file records the
decisions.

```
quokka> (caller 5)

debugger invoked on a SIMPLE-ERROR: broken 10

Type HELP for debugger help.

restarts (invokable by number or by name):
  0: [ABORT] Return to the top level.

0: (BROKEN 10)
0] (defun broken (x) (+ x 1))
BROKEN
0] restart-frame
(:GOT 11)
quokka>
```

## Console input

The REPL is driven by the host, which hands `%host-step` a string and prints
what it returns. A debugger has to wait for a command with the failed
computation still on the stack, so it cannot be driven that way: returning to
the host ends the computation. So **Lisp asks the host for a line.**

- **`%read-console prompt`** is a host request, as `%read-source` is (#35): the
  machine blocks and the host answers between slices. The answer is the line
  with its newline, or `nil` at the end of the input.
  `Host::read_line(prompt) -> Option<String>` defaults to `None`, so a host
  with nobody at a console (the tests, the browser page) needs no code. The
  terminal host reads standard input, the same input the REPL reads its
  forms from.
- **The console input stream** is a string input stream over the line it was
  last given, which refills itself when a read reaches the end of the line
  (`%refill-console` in `reader.lisp`). Only reaching the end costs
  anything, so the reader's per-character path is unchanged. It is
  `*standard-input*`. The console output stream reads as the console too
  (`%input-stream`), so `*terminal-io*`, `*debug-io*` and `*query-io*`,
  two-way streams in CLHS, can be read; `store-value`'s prompt works.
  `%console-prompt` is what the host shows before the next line.
- **`read-line`** was missing from tier 1, and is added.
- **The step budget is renewed when a line arrives.** The REPL gives each line
  5 million steps, and a debugging session runs inside the line that failed.
  The budget stands in for an interrupt nobody can press yet, and a person
  typing is the boundary between two pieces of work (`vm::drive`). The end
  of the input renews nothing, or a program reading it in a loop could
  never be stopped.

Rejected: the host running a nested REPL when `%host-step` reports "in the
debugger". It would need the machine to keep a stack across host calls,
which `Vm::start` throws away, and it would be a second protocol for what a
request already does.

## When the debugger runs

`invoke-debugger` calls `*debugger-hook*` first, as CLHS says. Then, if
**`%debugger-enabled`**, it enters the debugger; otherwise it does what it did
before step F: invoke the innermost `abort` restart, so a host reports the
error and carries on. The switch is off in a new world, because a world also
runs where nobody is at a console: the differential tests, scripts piped into
`quokka world.qi`, the build, the browser page. `quokka-debug:enable-debugger`
and `disable-debugger` are SBCL's names (`sb-ext`). **The terminal host turns
it on when standard input is a terminal**, and off when it is not, each time
it starts a world (`%host-debugger`), since a world saved at a terminal may
next run a script. At the end of the input, the debugger does what an
unhandled error did without it.

SBCL enters its debugger whatever the input, and with no terminal reads end of
file and quits; scripts use `--non-interactive`. We kept scripts going on past
an error, as they did before.

## What it shows

- **The banner** is SBCL's: the condition's type and text, the restarts
  numbered (names padded as SBCL pads them), and the current frame. The
  prompt is `0] `, the number being how many debuggers are open around this
  one.
- **Frames** start at the one that called the innermost signalling function
  (`%signalling-functions`: `error`, `signal`, `invoke-debugger`,
  `%signal-machine-error`, ...). The debugger's own frames and the handler's
  are above it and not shown. The host loop's frames at the bottom are cut
  off (`%host-step`, `%eval-read-form`, `eval`, the compiled form). In a
  nested debugger, the frames stop where the outer debugger evaluated the
  typed form, so it shows that form's own stack.

## Commands

Read with `read-preserving-whitespace`, so that a command's arguments are the
rest of its line (`frame 3`, `return (+ 1 2)`), as in SBCL.

1. A number invokes that restart, interactively.
2. A symbol naming a command runs it, whatever its package: `help`,
   `backtrace`/`bt` [n], `frame` n, `up`, `down`, `top`, `bottom`,
   `print`, `list-locals`, `source`, `error`, `restart-frame`, `return`
   form, `abort`, `toplevel`, `continue`.
3. A symbol naming an available restart invokes it: `store-value`,
   `use-value`.
4. Anything else is evaluated where the error happened, in its dynamic
   environment, and its values are printed.

An error inside the debugger opens one a level deeper. Each level offers an
`abort` restart, *Return to debug level N*, around everything it does.
Commands are defined by `%define-debugger-command`, which replaces a command
of the same name in place: a warm rebuild loads the file into a world that
has them all, and a list that grew would make C2 and C3 differ.

## Method frames, locals and source

- **A method's function is named as SBCL names it**, `(:method area :around
  (circle))`. The name comes from a declaration the compiler reads when it
  assembles a function, `(declare (%named name))` (`%declared-name` in
  `compile.lisp`), which `defmethod` puts in the method body's `lambda`. It
  is a declaration and not an operator: #110 rejected a `%named-lambda`
  operator because every code walker would have to learn it, and every walker
  already passes over a declaration. **Retrying a method's frame runs that
  method's own function**, as in SBCL. #131 first proposed re-dispatching
  the generic function instead, which is wrong whenever generic-function
  machinery is below the method: `call-next-method`, or before and after
  methods, would run twice. To reach a redefined method, retry the frame
  that called the generic function.
- **Locals.** `%frames` gives each frame's environment, the one saved in the
  header of the frame above it, and **`%frame-locals env callee`** lists the
  variable scopes' bindings from there down to the scope the callee closed
  over. Below that are the variables of the code around the function.
  `frame-locals` leaves out names the compiler made (uninterned), and names
  an inner binding hides.
- **`source`** prints the text step C kept for the frame's function, if it was
  defined at the top level.

## What the review found

The review of #131 reproduced nine faults, all fixed with a test
(`the_debugger_review_findings`):

1. A method with `&optional` or `&key` lost its name. The lambda-list
   rewrite moves the body's declarations into the `let*` it makes, so
   `%compile-lambda` reads `%named` before rewriting.
2. The newline after a number, a restart's name or an evaluated form stayed
   in the console's buffer, and the next read of the console after the
   debugger got an empty line. The debugger skips the rest of a blank line
   after such a form, and clears the input on entry (`clear-input`, added).
3. Catching `program-error` around a command, to report wrong arguments, also
   caught one from the form `return` evaluates. A command's argument counts
   are kept with it and checked before it runs.
4. A `*debug-io*` bound to a stream that cannot be read made every level fail
   and open another until the budget ran out. The debugger falls back to
   `%without-a-debugger` then.
5. The browser page could inherit an enabled debugger from a world saved at a
   terminal; it switches the debugger off when it loads a world.
6. `load` from the console recorded a definition's text from the wrong
   string; no text is kept for a form read from the console.
7. `*standard-output*` and `*error-output*` read the console, being the same
   object as `*terminal-io*`. The two-way streams are now a console stream
   of their own, `%console-io`, as in SBCL.
8. Bytes that are not UTF-8 at the console were silently the end of input;
   the host now says so.
9. `%frame-locals` had no bound on a cyclic scope chain from a damaged image.

## Left for later

- The browser page answers console reads with end of file, so its world
  keeps the debugger off. Phase 4's listener gives it a console.
- Evaluating a form in a frame's lexical environment (SBCL's `debug 2`
  `eval` in frame): forms are evaluated in the dynamic environment only.
  `list-locals` shows the values.
- Ctrl-C landing in the debugger: needs the host to interrupt a running
  slice, with processes (step E).
