# Frames keep their calls

> *Where this fits.* Phase 3 is building towards one moment: a program
> stops with an error, you fix the function that went wrong, and you tell
> the world to run that function's call again. Chapter 82 gave the world
> restarts, the ways out of an error, and chapters 84 and 85 gave it
> definitions that keep their source, so that a function can be shown and
> fixed. What was still missing was the call itself. This chapter makes
> every frame on the machine's stack keep the call that made it, the
> function and the arguments as they were passed, and gives Lisp a way to
> read them, which is what a backtrace is. Chapter 87 uses them to run a
> frame again.

*Status: done, issue #130. Phase 3, step D, sub-steps 1 and 2. The design
is `docs/frames.md`.*

## What a backtrace needs

When a program fails, the first question is how it got there. A
**backtrace** answers it: a list of the calls in progress when the error
happened, innermost first, each with the function and the arguments it was
called with. Here is one from the world at the end of this chapter, taken
inside a handler for an error that `deeper` signalled three calls down:

```lisp
(defun deeper (n)
  (if (= n 0) (error "bottom") (car (list (deeper (- n 1))))))
```

```
0: (LAMBDA #<SIMPLE-ERROR "bottom">)
1: (QUOKKA::%SIGNAL #<SIMPLE-ERROR "bottom">)
2: (ERROR "bottom")
3: (DEEPER 0)
4: (DEEPER 1)
5: (DEEPER 2)
6: (QUOKKA::COMPILED-FORM)
7: (EVAL (HANDLER-BIND ((ERROR #)) (CATCH :X (DEEPER 2))))
...
```

Reading from the bottom, the world evaluated a form, which called `deeper`
with 2, which called itself with 1 and then 0; that call called `error`,
which signalled, and the handler (the `lambda` at the top) is where we
stopped to look. Every line is a **call frame**, the record the machine
keeps of one call while it runs: where to return to, and what the caller
was doing. Chapter 25 built frames as a few words on the operand stack.
Until this step those words could not have produced the lines above.

## What a frame was

A frame was four words, pushed by `CALL` on top of the caller's operands:
the caller's program counter, its frame pointer, its function and its
environment. Everything in it was about the caller, because a frame's job
was to let `RET` carry on where the call came from. The arguments were not
there. `CALL` popped them, and the callee's parameters were bound to them
in a fresh environment scope, since in Quokka variables live in a chain of
scopes (chapter 26) and not in frame slots. Nor was the callee: while a
function runs, its own code is in the `Function` register, and it only
reaches the stack, as the *saved* function, when it calls something else.

So after a call had begun, nothing on the stack said what the call was.
The parameter scope holds the values, but it is the wrong place to look
for two reasons. A function may `setq` its own parameter, after which the
scope holds the new value. And for a function with `&optional` or `&key`
parameters the scope holds the parameters after they were sorted out,
under the fresh names the compiler gives them (chapter 64), and not the
arguments the caller wrote.

## Where to keep the call

The issue that decided this step, #111, said that a debug setting should
keep a function's original arguments in its frame, as SBCL's
`(optimize (debug 3))` does, and rejected keeping them always, because
every call would pay. When we came to build it, the question was what a
call would pay, which depends on where the call is kept.

The obvious place is a list, `(callee arg1 arg2 ...)`, made fresh by every
call and pointed at from a fifth word of the frame. That costs one cons per
argument on every call. A call already allocates one object, the parameter
scope, and the collector of chapter 6 copies everything alive at every
collection, so allocating a second structure per call would roughly double
what calls cost the collector. That was the cost #111 was worried about.

The other place is the stack itself. A call has its arguments on the stack
when it starts, because the caller pushed them there; `CALL` popped them
only because nothing wanted them afterwards. If `CALL` writes them back
below the frame's header, together with their count and the callee, the
call is kept with no allocation at all. That costs a few stores, n + 3 for
a call with n arguments, and as many clears when `RET` pops them. The frame
grows to this:

```text
   argument 0 .. argument n-1   as the call passed them
   n                            ARGUMENT_COUNT
   nil, or the call's token     TOKEN: made when the frame is first read
   the callee                   CALLEE: what was called, before a
                                funcallable instance is opened
   saved pc                     PC
   saved frame pointer          FP
   saved function               FUNCTION: the caller's
   saved environment            ENV: the caller's
   ...the callee's operands     <- frame pointer
```

The header is seven words now, and the arguments sit below it, where a
return can find them from the count. The `TOKEN` word is for chapter 87,
and is `nil` when a call is made. The callee is kept as the caller had
it, which for a generic function is the generic function and not the
closure inside it that the machine actually runs (chapter 74).

With the cost that low we dropped the debug setting: every frame keeps its
call. A Lisp machine's debugger should be able to retry any frame, and
needing to remember a `declaim` before the error happens is the kind of
thing nobody remembers.

We measured the cost on this machine, running the old and the new world
side by side. A recursive `fib`, which does almost nothing but call, is
about 2.5% slower. The warm rebuild of chapter 41, the world recompiling
itself, went from 24.9 to 26.5 seconds, about 6%, and part of that is the
new library file and its package, which every symbol the rebuild reads is
now also looked up in. The first version cost 7% on the rebuild, and when
we kept only the callee and no arguments it was still 6% slower, so most of
the cost was not the arguments at all. We looked at what every call and
return now did besides store, and found that a return checked its frame the
way a primitive reading a damaged stack must, asking each time whether the
stack was a vector and how large it was. The machine always has a stack
vector, so we made the check every image passes when it is loaded,
`Heap::verify`, guarantee that much of any world, and a return now checks
only that the frame pointer is inside the stack. That helped `fib` and
barely moved the rebuild. These measurements are noisy to a percent or
two, and we did not look further, since a few percent is what a backtrace
is worth.

What is left of the cost is not time. A frame keeps its arguments alive
until it returns, so a function that walks a long list by `setq`-ing its
parameter keeps the head of the list, and everything after it, from being
collected until it returns. SBCL at high debug settings does the same. If a
program is ever measured to suffer from it, an opt-out, the standard
`(declare (optimize (debug 0)))`, is easy to add then.

## Kept as passed

SBCL keeps no copy; its debugger reads the parameters' current values. We
checked what that means with a function that changes its parameter before
failing:

```lisp
(defun f (x) (let ((y (* x 2))) (setq x 99) (error "boom ~a" y)))
(f 5)
```

SBCL's backtrace shows the frame as `(F 99)`, a call nobody made. Quokka
shows `(F 5)`. The difference matters most for retrying, in chapter 87:
running `(F 99)` again is not running the call that failed. Backtraces are
not part of the standard, so this is a difference from SBCL's debugger and
not from Common Lisp.

## Tail calls, and the call of `error`

A **tail call** reuses the caller's frame (chapter 10): when a function's
last act is to call another, nothing of it is needed afterwards, so the
callee takes over its frame and returns straight to its caller. In the new
layout that means rewriting the kept call, since the frame now belongs to
the tail callee:

```rust
            let saved = if tail {
                let Some((base, n)) = open_frame(self.mc, fp) else {
                    return self.damaged_frame("TCALL");
                };
                let stack = self.mc.register(Register::Operands);
                let header = [
                    self.mc.slot(stack, base + frame::PC),
                    self.mc.slot(stack, base + frame::FP),
                    self.mc.slot(stack, base + frame::FUNCTION),
                    self.mc.slot(stack, base + frame::ENV),
                ];
                self.drop_to(base - n);
                header
            } else {
```

The caller that made the tail call is gone from the backtrace, as it is in
SBCL. Usually that is the right trade: a loop written as recursion runs in
constant space, and the frame that vanished had nothing left to do.

There is one call for which it is the wrong trade. Our first test of
retrying wrote the failing function the way people write them:

```lisp
(defun broken (x) (error "broken ~a" x))
```

and found no frame for `broken` at all. The call of `error` was in tail
position, so `error` took over `broken`'s frame, and the one frame the
debugger wanted was the one the machine had thrown away. SBCL has the same
tail calls and still showed `F` in the experiment above, because it knows
that `error` never returns and does not bother to reuse a frame for a call
that will not come back. We did the same in the compiler:

```lisp
(defun %never-returns-p (head env)
  (if (if (eq head 'error) t (eq head 'invoke-debugger))
      (not (%lookup head env 0 ':function))
      nil))
```

A call of the global `error` or `invoke-debugger` in tail position is
compiled as an ordinary `CALL`. Nothing is lost, since a frame for a call
that never returns cannot be reused by anything anyway.

## Names a backtrace can use

A frame keeps its function, and a function knows the name it was compiled
under (chapter 30 put the name there for error messages). For a `defun` that
is the function's name. For a local function made by `flet` or `labels` it
was the local name, `f`, which made a backtrace ambiguous and, worse, would
have made chapter 87 call the global `f` when retrying a local one. SBCL
names local functions `(FLET F :IN OUTER)`; the compiler now names them
`(flet f)` and `(labels f)`. A setf function, which was assembled as an
anonymous `lambda`, is named `(setf f)`.

## Reading the frames from Lisp

One primitive reads the stack, `%frames`, which walks down the chain of
saved frame pointers and returns, for each frame, its place on the stack,
the callee and the arguments:

```rust
pub fn frames<'h>(mc: &mut Mutation<'h>) -> Result<Value<'h>, EvalError> {
    let mut found: Vec<(usize, Value<'h>, Value<'h>, Vec<Value<'h>>)> = Vec::new();
    let mut fp = mc.fp();
    while let Some((base, n)) = frame_bounds(mc, fp) {
        let mut token = mc.slot(mc.register(Register::Operands), base + frame::TOKEN);
        if !token.is_cons() {
            token = mc.cons(Value::NIL, Value::NIL);
            let stack = mc.register(Register::Operands);
            mc.set_slot(stack, base + frame::TOKEN, token);
        }
        let stack = mc.register(Register::Operands);
        let callee = mc.slot(stack, base + frame::CALLEE);
        let arguments = (base - n..base).map(|i| mc.slot(stack, i)).collect();
        found.push((fp, token, callee, arguments));
        match mc.slot(stack, base + frame::FP).as_fixnum() {
            Some(caller) if caller >= 0 && (caller as usize) < fp => fp = caller as usize,
            _ => break,
        }
    }
```

Each saved frame pointer must be below the one before it, so a damaged
stack ends the walk instead of sending it round in a loop, and
`frame_bounds` refuses a frame that would put an argument outside the
stack. The walk also gives each frame a token if it has none, a fresh
cons that names this call and no other; chapter 87 says why. A primitive
has no frame of its own, so the first frame `%frames` returns is that of
the Lisp function that called it.

Everything else is Lisp, in `lisp/library/frames.lisp`. A `frame` is a
structure holding the place, the function and the arguments, and the
public names are in a package of their own, `QUOKKA-DEBUG`, as SBCL's are in
`SB-DEBUG`, so that Quokka's own names stay out of `COMMON-LISP`. Where
SBCL has a function for the job we used its name and meaning:
`list-backtrace` returns the calls as lists, and `print-backtrace` prints
them one to a line, as at the top of this chapter. `frame-name` is the
only one with any judgement in it:

```lisp
(defun frame-name (frame)
  (let ((function (frame-function frame)))
    (if (%generic-function-p function)
        (%gf-name function)
        (or (%function-name function) 'lambda))))
```

These are snapshots. A `frame` object says where a frame was and what it
held when `frames` was called; the frame itself may have returned since,
and chapter 87 has to check before acting on one.

## What did it cost?

In the machine, Rust where it has to be: the frame layout, the tail call,
the walk, and the check that a return can trust, since only the machine
can write its own frames. A world saved in the
middle of a computation has frames in the old shape, so the image format
went from 8 to 9 and older worlds are refused. The compiler gained the two
naming rules and `%never-returns-p`. Two tests in the compiler's own unit
tests changed, both because of the new rules: an arity error now says
`(FLET INNER) wants 1 arguments`, and a `handler-case` that re-signals an
unwanted condition now calls `error` with an ordinary `CALL`. The
backtraces are Quokka's own, so the tests for them are Rust tests over a
compiled world, not differential files; SBCL's backtraces differ in every
line.

## What is still missing?

A generic function's frames read poorly. The generic function hands its
arguments to its methods by tail calls, so what the backtrace shows is the
method's function, and method functions are anonymous: `(LAMBDA 4)` where
SBCL shows `((:METHOD AREA (CIRCLE)) 4)`. Naming method functions is step
F's, along with showing a frame's local variables, whose scopes already
carry their names for a debugger, and hiding the debugger's own frames,
the top three lines above.
