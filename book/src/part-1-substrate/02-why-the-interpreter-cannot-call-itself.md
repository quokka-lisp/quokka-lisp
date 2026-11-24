# Why the interpreter cannot call itself

> *Where this fits.* With values settled, the next question is how a
> computation runs, and in a browser, how it can stop halfway and continue
> later. The obvious design for an interpreter cannot do that at all, and this
> chapter is about what to write instead: an evaluator whose whole state is data you can look at, stop, save
> to a file and resume. That shape costs us the clearest code in the book, and
> pays for the debugger, the garbage collector and the saved image.

Here is how you write a Lisp interpreter. It is how almost everyone writes their
first one, and it is correct:

```rust
fn eval(&mut self, form: Value, env: Value) -> Value {
    match form.tag() {
        Tag::Symbol => self.lookup(form, env),
        Tag::Cons => {
            let f    = self.eval(car(form), env);
            let args = self.eval_args(cdr(form), env);
            self.apply(f, args)
        }
        _ => form,
    }
}
```

Evaluating a nested expression means evaluating its parts, which is a recursive
definition, so a recursive function is the natural way to write it.

Quokka cannot use this, and the reason has nothing to do with Lisp.

## One thread, no interruptions

A browser tab runs your code on one thread, and nothing can take it away.
JavaScript and WebAssembly both run to completion: once the page calls into your
code, the browser gets control back when you return, and not before.

Consider what the interpreter above does with this program:

```lisp
(defun spin () (spin))
(spin)
```

The call never returns. The browser cannot repaint, cannot handle a click,
cannot process a keystroke, cannot run a timer. The tab is frozen, and after a
few seconds the browser offers to kill the page.

Natively the problem does not exist, which is why it is easy to not see coming.
An operating system *preempts*. It takes the CPU away from your process every
few milliseconds whether you like it or not, and hands it to the window server,
so your REPL can loop forever and the machine stays responsive. A browser tab
has no such mechanism.

The only way out is for the interpreter to give control back voluntarily. It
runs for a while, stops, returns to JavaScript, lets the browser do a frame of
work, and is called again to carry on from exactly where it stopped.

```text
┌──────────── one animation frame (~16 ms) ────────────┐
│  JS calls vm.run_slice(...)                          │
│      └─ the VM evaluates Lisp ............ ~8 ms     │
│      └─ the VM stops mid-computation, returns        │
│  the browser handles clicks, paints, runs timers     │
└──────────────────────────────────────────────────────┘
          ↓ requestAnimationFrame
┌──────────── the next frame ──────────────────────────┐
│  JS calls vm.run_slice(...)  ← resumes where it left │
```

The name for this is **cooperative multitasking**, as opposed to preemptive. It
is an old idea, the way Windows worked before 95 and classic MacOS until OS X,
and it has an old, well-known failure mode: one piece of code that refuses to
cooperate hangs everything. We come back to that at the end of the chapter.

## What stopping and resuming demands

Look again at the recursive `eval`, and ask where the state is when you are
forty frames deep evaluating `(fact 20)`.

It is on the Rust call stack. Each nested `eval` is holding its own `form`, its
own `env`, its own partially evaluated arguments, in Rust local variables. To
return to the browser you have to return out of all forty frames, and the
instant you do, Rust destroys them. There is nothing left to resume, so the
computation would have to start again from the beginning.

This is not a difficulty to be worked around. It is a proof that the shape is
wrong. **An evaluator that can stop and resume cannot keep its state on the host
language's call stack.**

So the state has to live somewhere we control:

```rust
struct Vm {
    control: Value,  // the form currently being worked on
    env:     Value,  // the environment it is being worked on in
    stack:   Value,  // pending frames — in OUR heap, a Lisp object
    value:   Value,  // the result the last step produced
}

impl Vm {
    /// One state transition. Never recurses.
    fn step(&mut self) { /* ... */ }
}
```

The evaluator becomes a machine with registers. Evaluating a form is a sequence
of small transitions, each one reading and writing those four registers and the
frame stack, each one returning promptly. Stopping is then trivial: you stop.
Every scrap of state is in `self`, which is in our heap, which we own.

## The explicit-control evaluator

This design is SICP §5.4, the chapter where Abelson and Sussman stop using
Scheme's own recursion to implement Scheme and build the evaluator as a
register machine with an explicit stack. If you want one reference for this
chapter, that is the one, and it is freely available. Queinnec's *Lisp in Small
Pieces*, chapter 3, arrives at the same structure from the direction of
continuations. The general technique of turning a recursive function into a
loop over explicit state is called *defunctionalisation*, and the general trick
of returning to a driver loop instead of recursing is a *trampoline*.

If you write modern Rust, you already have an intuition for this, because the
compiler does it for you. An `async fn` is rewritten into a state machine whose
local variables live in a heap-allocated struct, exposing a `poll` method that
returns `Ready` or `Pending`. Quokka's `run_slice` *is* `Future::poll`, written
by hand.

Which raises a fair question: why not use `async` and let the compiler do the
work? Because the state has to be inspectable. Those frames are going to be
shown to a user in a debugger, walked by a garbage collector, and written to a
file, and `async` puts them in a compiler-generated struct that nothing can
look inside. We need them to be ordinary Lisp objects.

## Who decides when to stop?

Something has to decide where a slice ends, and we had three candidates. The
unit they all work in is the **step**, one `step()` call, one transition of the
machine; later, when there is bytecode, it will be one instruction. The
traditional Lisp word is a *reduction*. It is a rough unit, since `(car x)` and
a large multiplication are both one step and take very different amounts of
time.

The first candidate was for the VM to ask the host, through a `should_yield()`
method the interpreter calls to find out whether to stop. Calling through a
function pointer on every single step is too expensive, so in practice you call
it every thousand steps, which means building a counter anyway and paying for
an indirect call on top of it.

The second was for the host to set a wall-clock deadline: the host says "stop
at 11:42:03.016" and the VM watches the clock. This is the responsive answer,
because a thousand cheap steps and a thousand expensive steps both stop at the
right moment. But reading a clock from WebAssembly means calling out to
JavaScript, which costs about as much as a step does, so this too ends up
amortised behind a counter. It also drags a wall clock into the core VM, which
rule 2 forbids.

The third was for the caller to pass a budget, and that is what we chose:

```rust
pub fn run_slice<M>(machine: &mut M, budget: u64) -> Status
where
    M: Steppable + ?Sized,
{
    let mut steps = 0;
    while steps < budget {
        steps += 1;
        match machine.step() {
            Transition::Continue => {}
            Transition::Halt => return Status::Done { steps },
        }
    }
    Status::Yielded
}
```

That is the whole mechanism. The native REPL passes `u64::MAX` and never thinks
about slices again. The browser passes a number and adjusts it from how long the
last slice actually took:

```js
let budget = 50_000;
function pump() {
  const t0 = performance.now();
  const status = vm.run_slice(budget);
  const elapsed = performance.now() - t0;
  budget = Math.max(1000, Math.round(budget * 8 / Math.max(elapsed, 0.1)));
  if (status === "yielded") requestAnimationFrame(pump);
}
```

`Steppable` is a trait rather than a method on `Vm`, which let us write and test
the slice accounting before there was an evaluator to slice. `Status` had two
cases when this was written, `Done` and `Yielded`. It has a third now,
`Blocked`, for a machine waiting on an answer from the host; we left it out
deliberately until there was a real caller to shape it, and
[Asking the outside world](../part-5-the-image/31-asking-the-outside-world.md)
is where it arrived.

## Why a counter beat a clock

The feedback loop in that JavaScript recovers the responsiveness of the clock
option. The difference is where the clock lives: outside the VM, in the host,
where measuring time is cheap and cannot affect anything the VM computes.

That last part is the real argument, and it is about testing. Quokka's test
strategy is **differential**: the same Lisp file runs on Quokka natively, on
Quokka under WebAssembly, and on SBCL, and all three must print the same thing.
That only works if "the same program" means "the same answer, every time,
everywhere".

Today, with one thread of execution, where you stop makes no difference to what
gets printed. Stopping after a thousand steps or ten thousand produces identical
results, so slicing is semantically invisible. The risk arrives later, when the
VM gains green threads. Then the slice boundary decides when another process
gets to run, and the interleaving decides the output. If slice length comes from
a wall clock, two runs of the same test interleave differently, and the test
suite starts failing for reasons that have nothing to do with the code.

A step budget makes execution deterministic and therefore replayable: record the
budgets, replay the session exactly. For a system whose debugger is meant to be
a conversation, that is worth having. Two smaller things fall out of it.
`run_slice(machine, 1)` is a single-step debugger primitive for free, and the
native host has nothing to implement at all.

The property the design is buying is that how a computation is cut up must not
change what it computes. Being a property, it can be asserted:

```rust
#[test]
fn slicing_does_not_change_the_result() {
    let reference = { /* run it in one slice */ };

    for budget in [1, 2, 3, 7, 10, 99, 100, 101, 1000] {
        let mut m = Countdown::new(100);
        while run_slice(&mut m, budget) == Status::Yielded {}
        assert_eq!(m, reference, "budget {budget} produced a different state");
    }
}
```

Nine different slicings of the same computation, all required to land in
identical state. When green threads arrive and this test needs a scheduler, it
will be the first thing to notice if that scheduler has leaked any
nondeterminism.

## What it costs

A budget only bounds time if a single step is short. Once control enters a Rust
primitive, nothing can interrupt it, so a primitive that sorts a
million-element list blows any budget and no scheduler can help. Every
primitive must therefore be either fast or chunked into resumable pieces, which
is a standing constraint on the whole primitive table, written down as rule 9.
It is the cooperative-multitasking failure mode from the start of the chapter
showing up in its modern form, and the wall-clock option does not rescue you
from it: it has exactly the same problem.

Two invariants keep slicing invisible. Garbage is collected when something
allocates rather than at slice boundaries, so collection timing depends only on
what the program did. And Lisp code may never observe how much budget is left,
because the moment it can, the slice becomes part of the language's semantics
and everything above collapses.

## What one constraint paid for

This was expensive. The clear recursive evaluator had to be thrown away before
it was written, in favour of a state machine, for a reason that has nothing to
do with Lisp and everything to do with a browser repainting. It turned out to
be the best-value decision in the system, because three other hard problems
quietly became easy.

Showing a user their stack in a debugger, and resuming a frame after they fix
something, requires stack frames to be inspectable objects, and they already
are, because that is where the evaluator keeps its state. The garbage collector
needs to know every place a live value might be hiding, and in a recursive
evaluator those places are scattered through dozens of invisible Rust stack
frames, where here there are four registers and one frame stack;
[What is alive?](03-what-is-alive.md) is about why that matters enormously. And
saving a running world, writing the image to a file, loading it later and
resuming the computation that was in progress, is possible only because the
computation *is* heap data, which is
[Saving a world](07-saving-a-world.md).

One constraint, three features. The expensive decisions in a system are rarely
expensive in only one place, and occasionally the compensation is spread just as
widely.

The machine itself is built in [What a form means](../part-2-reading-and-running/09-what-a-form-means.md), and
[Functions and scope](../part-2-reading-and-running/10-functions-and-scope.md) adds a fourth payoff nobody
planned: because frames are popped before they are acted on, tail-recursive
calls cost no stack at all.
