# Retrying a frame

> *Where this fits.* Chapter 86 made every frame on the stack keep the call
> that made it, and gave Lisp a backtrace. This chapter uses the kept calls
> to act on a frame: leave everything above it, cleanly, and run something
> in its place. Running the frame's own call again is *retrying* it, and
> with chapter 84's definitions it gives phase 3's gate in miniature: a
> function fails, you redefine it, you retry its frame, and the caller gets
> the fixed function's answer as if nothing had gone wrong. Step F builds
> the debugger a person uses for this on top of these few functions.

*Status: done, issue #130. Phase 3, step D, sub-step 3. The design is
`docs/frames.md`.*

## The gate, as a test

This is the test that holds this step to its purpose:

```lisp
(defun broken (x) (error "broken ~a" x))
(defun caller (y) (list :got (broken (* y 2))))

(handler-bind ((error (lambda (c)
                        (eval '(defun broken (x) (+ x 1)))
                        (quokka-debug:restart-frame (find-frame 'broken)))))
  (caller 5))
;; => (:GOT 11)
```

`caller` calls `broken` with 10, and `broken` signals an error. The
handler runs where the error happened (chapter 80), with `broken`'s frame
still on the stack below it. It redefines `broken` and asks for the frame
to be restarted, and `caller` receives 11, the fixed `broken`'s answer to
the original call. `find-frame` is the test's own three lines, a search of
`frames` by name. In step F a person will do the same through the debugger:
see the backtrace, edit `broken`, and choose *retry*.

## Leaving a frame cleanly

Retrying a frame means first getting rid of everything that happened inside
it, and this has to be done with the same care as any other exit. Between
the frame and the handler there may be `unwind-protect` forms whose
cleanups must run, and special variables bound with `let` whose old values
must come back. Quokka already knows how to leave a computation like that:
it is what `throw` does on its way to a `catch` (chapter 27). An unwind
walks down the records on the stack, one per step of the machine, running
a cleanup when it meets an `unwind-protect` and putting back a special
variable's value when it meets a binding, until it finds the record it was
looking for.

So a frame became a new kind of target for the same unwind. SBCL has the
operation under the name `sb-debug:unwind-to-frame-and-call`, and we took
the name and the meaning: unwind to a frame, then call a function of no
arguments in its place, whose values become the frame's values. The
machine's half is one intercepted primitive, `%unwind-to-frame`, and a few
lines at the top of the unwinder:

```rust
        if let Some(place) = self.frame_target(target) {
            if at == usize::MAX || at < place {
                return self.arrive_at_frame(target, place);
            }
        }
```

A frame has no record of its own, so the unwind cannot stop at one. It
needs a different test for having arrived, and the stack gives one: every
record the frame or anything it called pushed lies above the frame pointer,
and every record below it belongs to a caller. When the innermost record is
below the frame pointer, or there are none, everything inside the frame
has been undone. Arriving pops the frame as a return would, and calls the
function in its place:

```rust
        let thunk = self.mc.slot(target, 2);
        self.drop_to(base - n);
        self.mc.set_pc(pc);
        self.mc.set_fp(caller_fp);
        self.mc.set_register(Register::Function, function);
        self.mc.set_register(Register::Env, env);
        self.mc.set_register(Register::Unwind, Value::UNBOUND);
        self.mc.set_register(Register::Values, Value::UNBOUND);
        self.call_value(thunk, &[], false)
```

With the caller's registers back, the new call pushes a frame where the old
one was, with the same return address, so whatever it returns goes to the
caller exactly as the old frame's value would have.

## Is the frame still there?

A `frame` object from `frames` names its frame by its place, the frame
pointer, which is an index into the stack vector. A collection moves the
vector but not the indexes in it, so the place stays right for as long as
the frame lives. But a frame that has returned leaves its place to the next
call made there, and a debugger holding an old `frame` must not unwind to
whatever has taken its place.

Our first check compared the function as well as the place, and the
review of this step showed it was not enough. A loop that calls the same
function twice from the same place makes two calls with the same place and
the same callee, one after the other:

```lisp
(defun loopy (k)
  (if (= k 0) (setq *saved* (first (quokka-debug:frames)))
      (if (= k 1) (quokka-debug:return-from-frame *saved* :hijack)))
  k)
(defun twice () (list (progn (loopy 0) (loopy 1))))
```

The frame saved during the first call was accepted during the second, and
`twice` returned `(:HIJACK)`. The same thing happens in a listener whenever
someone keeps a frame from one error and uses it after running the same
form again.

What was missing is an identity for each call, and the cheap way to give
one would have been a counter, numbering every call. It would also have
been a new piece of machine state, saved in every image, for something only
a debugger ever asks about. So the identity is made lazily. The frame
header has a `TOKEN` word, and a call stores `nil` there. The first time
`%frames` reads a frame it stores a fresh cons in the word, and hands the
same cons out every time after. A cons is an object nobody else has, and it
stays alive as long as anyone holds it, so it can never be confused with
another frame's token. `%unwind-to-frame` is given the token, and refuses
unless the frame at the place holds that very token. A later call at the
same place starts with `nil`, or has a token of its own.

The check is a walk down the chain of frames from the top, the walk a
`throw` makes through the records to check that its `catch` exists
(chapter 68), so the cost grows with the depth of the stack, and it is paid
once per retry. A frame that is gone is a `control-error`, the condition
Common Lisp signals for a `return-from` to a block that has already been
left.

## Which function runs again

`restart-frame` calls the frame's function again with the arguments it
was first called with. Which function is the one real decision, and it is
the reason a fix takes effect:

```lisp
(defun restart-frame (frame)
  (let ((name (frame-name frame))
        (arguments (frame-arguments frame)))
    (let ((function (if (%retry-by-name-p name) (fdefinition name) (frame-function frame))))
      (unwind-to-frame-and-call frame (lambda () (apply function arguments))))))
```

The frame keeps the function object it was running, and after `broken` is
redefined that object is the old, broken function. Calling it again would
fail again. So if the frame's function has a global name, and that name
names a function now, the current definition is called; otherwise, for an
anonymous `lambda` or a local function, the frame's own function is. This
is the rule SLIME, the Emacs environment for Common Lisp, uses when it
retries a frame in SBCL. It is also why chapter 86 renamed local functions
to `(flet f)`: under the old plain name, retrying a local `f` would have
called the global one.

`return-from-frame` is the other use of the same primitive. It returns
values of the debugger's choosing from a frame, as if its function had
returned them, which is SBCL's debugger command `return`:

```lisp
(defun return-from-frame (frame &rest values)
  (unwind-to-frame-and-call frame (lambda () (values-list values))))
```

## What did it cost?

The machine's part is Rust, because only the machine can unwind: the
primitive with its check, the frame as a target, and the arrival. The rest
is the few functions of `frames.lisp` above. The
test the chapter opened with is the first of the cases in `a_frame_can_be_retried`,
which also retries a frame whose function changed its own parameter (and
gets the arguments as first passed), retries a local function, returns two
values from a frame, checks that returning from a frame runs its cleanup
and undoes its special binding, and checks that a frame that has returned
cannot be unwound to, `loopy`'s case included.

The review found one more thing. The frame at the very bottom of the
stack belongs to the host's loop, `%host-step`, whose value the host reads
as the outcome of a step, `(:value "text")`. Returning anything else from it
made the host read a list that was not there, and panic. The host now
checks the shape of what it is given and reports a malformed outcome as a
fault of the machine, and the world carries on with the next form.

## What is still missing?

A retry of a method's frame runs the old method, because the frame's
function is the method function and method functions have no global name;
retrying the frame that called the generic function dispatches afresh and
finds the new method. Naming method functions, as SBCL names them, would
let a method's frame be retried by its generic function, and is step F's.
So is everything a person sees: choosing a frame, choosing what to do with
it, and doing it from a terminal while the program waits.
