# Frames

Phase 3, step D (#130). The gate of phase 3 is *fix a function from inside
an error and retry the frame*. Step B gave the debugger restarts and step C
gave it definitions that keep their source. This step gives it the frames:
what function each one is running, what it was called with, and a way to
run it again. The book's chapters 86 and 87 tell the story; this file
records the decisions.

## What a frame keeps

A call frame keeps **the call that made it**: the callee, as the caller
called it, and the arguments as they were passed. They lie on the operand
stack below the frame's header, which grew from four words to seven:

```
   argument 0 .. argument n-1   as the call passed them
   n                            ARGUMENT_COUNT
   nil, or the call's token     TOKEN      made when first read
   the callee                   CALLEE
   saved pc                     PC
   saved frame pointer          FP
   saved function               FUNCTION   the caller's
   saved environment            ENV        the caller's
   ...the callee's operands     <- frame pointer
```

`vm::frame` names the header's slots; `vm::frame_bounds` is the one place
that reads a frame's extent, and refuses one a damaged stack would put out
of range (rule 10). Image format 9.

- **On the stack, not in a list.** A call already writes the stack, so
  keeping the call is n + 3 stores and as many clears on return, and no
  allocation. A fresh list `(callee . args)` per call would roughly double
  what a call allocates. Keeping a pointer to the parameter scope is one
  word, but a `setq` of a parameter changes it, and for `&optional` and
  `&key` it holds bound parameters, not arguments.
- **Always, with no debug setting.** #111 expected a switch, as SBCL's
  `(optimize (debug n))`. With the call kept on the stack the cost is small
  enough that every frame keeps it, so every frame can be retried. The cost
  that remains is that a frame keeps its arguments alive until it returns.
  An opt-out is added when a program is measured to need one.
- **As passed, not the parameters' current values.** SBCL reports the
  current values (after `(setq x 99)` its frame shows 99). A retry should
  repeat the call the caller made, so Quokka keeps the arguments as
  passed. Backtraces are not in the standard, so this is no departure from
  CL's semantics, only from SBCL's debugger.
- **Tail calls replace the call.** A tail call reuses its caller's frame, so
  the frame holds the tail callee's call and the caller is gone from the
  backtrace, as in SBCL. The exception is a call of `error` or
  `invoke-debugger`: the compiler never makes one a tail call (SBCL does not
  either), so the function that signalled is still on the stack for the
  debugger. `(funcall #'error ...)` and `(apply #'error ...)` in tail position
  are still tail calls; nothing in the library writes them. The function the machine was started on has no frame
  (`Vm::start`), and neither does its tail callee.

## Names

A local function is assembled under the name `(flet f)` or `(labels f)`,
and a setf function under `(setf f)`, as SBCL names them (without SBCL's
`:in`). So a backtrace says which `f` a frame is running, and a retry by
name never calls the global `f` for a local one.

## What Lisp reads

`%frames` is one primitive, a walk down the saved frame pointers that
returns `(place token callee . arguments)` for each frame, innermost first:
the frame of the function that called it first, since a primitive has none.
It is linear in the frames open within one step of the machine, as
`%handler-marks` is in the records (rule 9); 20,000 frames read without
trouble. The
rest is Lisp, in `lisp/library/frames.lisp`, with its public names in the
package **`QUOKKA-DEBUG`** (made in `packages.lisp` beside `QUOKKA-MOP`, and
used by `QUOKKA`), as SBCL's are in `SB-DEBUG`:

| | |
| --- | --- |
| `frames &optional count` | `frame` objects for the frames below the caller, innermost first |
| `frame-function`, `frame-arguments` | what a frame keeps |
| `frame-name`, `frame-call` | its function's name, and `(name . arguments)` |
| `list-backtrace &key count`, `print-backtrace &key count stream` | SBCL's: the calls, and one line a frame, `0: (FOO 1 2)` |
| `unwind-to-frame-and-call frame thunk` | SBCL's: unwind to the frame, then call THUNK in its place |
| `restart-frame frame` | call the frame's function again with its arguments |
| `return-from-frame frame &rest values` | return VALUES from the frame |

A `frame` is a snapshot. It names its frame to the machine by its place, the
frame pointer (an index into the stack vector, which a collection does not
move), and by its **token**. A place is reused by every call made there, and
a later call of the same function at the same depth has the same callee too
(the review of #130 found a frame kept from one call returning from the
next). So the header has a `TOKEN` word: a call stores `nil`, and `%frames`
stores a fresh cons the first time it reads the frame, and hands out the
same one after. Lazily, because a counter numbering every call would be
machine state in every image for something only a debugger asks about.

## Retrying a frame

`%unwind-to-frame place token thunk` is intercepted by the machine. It walks
the frame chain to check that the frame at PLACE is still open and holds
TOKEN, and a frame that is not is a `control-error`. Then it starts an unwind whose
target is the frame. The unwind passes every record the frame or its callees
pushed, one per step, exactly as a `throw` passes a record that is not its
catch: an `unwind-protect` cleanup runs, a special binding is undone. It has
arrived when the innermost record is below the frame pointer. There the frame
is popped as a return would pop it, and THUNK is called in its place, so its
values go to the frame's caller.

`restart-frame` follows SLIME's rule for SBCL in choosing what to call: if
the frame's function has a global name that names a function now, the
current definition, so that a function fixed while its frame waited runs
fixed; otherwise the frame's own function, as for a `lambda` or a local
function.

## The host's frame

The bottom frame is the host loop's, `%host-step`, whose value the host
reads as a step's outcome. `return-from-frame` can return anything from it,
so `vm::step_outcome` checks the shape and reports a malformed outcome as a
fault instead of reading a list that is not there (the review of #130 found
the panic).

## Left for step F

- A method's frame shows its function's name, `lambda`, because method
  functions are anonymous; SBCL names them `(:method name specializers)`.
  Retrying a method's frame runs the old method; retrying the frame of the
  function that called the generic function re-dispatches.
- A frame's local variables: the scopes carry their names for the debugger,
  and nothing reads them yet.
- Hiding the debugger's own frames (`signal`, `error`, the handler) from
  what it shows.
