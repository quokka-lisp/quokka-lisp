# Why the interpreter cannot call itself

> **Where this fits.** With values settled, the next question is how a
> computation runs — and in a browser, how it can *stop halfway and continue
> later*. The answer rules out the obvious design for an evaluator and reshapes
> the whole system. It is the most consequential decision in the book, and it
> turns out to pay for three other things in later chapters.

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

Short, clear, obviously right. Evaluating a nested expression evaluates its
parts, which is a recursive definition, so you write a recursive function.

Quokka cannot use this, and discovering why is the most consequential thing that
happened in the first week of the project. The reason is not about Lisp at all.

## One thread, no interruptions

A browser tab runs your code on one thread, and nothing can take it away.
JavaScript and WebAssembly both run to completion: once the page calls into your
code, the browser gets control back when you return, and not before.

So consider what the interpreter above does with this:

```lisp
(defun spin () (spin))
(spin)
```

The call never returns. The browser cannot repaint, cannot handle a click,
cannot process a keystroke, cannot run a timer. The tab is frozen. After a few
seconds the browser offers to kill the page.

Natively this problem does not exist, which is why it is easy to not see
coming. An operating system *preempts* — it takes the CPU away from your process
every few milliseconds whether you like it or not, and hands it to the window
server. Your REPL can loop forever and the machine stays responsive. A browser
tab has no such mechanism.

The only way out is for the interpreter to give control back **voluntarily**. It
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

The jargon for this is **cooperative multitasking**, as opposed to preemptive.
It is an old idea — it is how Windows worked before 95, and classic MacOS until
OS X — and it has an old, well-known failure mode: one piece of code that
refuses to cooperate hangs everything. We will come back to that.

## What "stop and resume" actually demands

Now look again at the recursive `eval`, and ask where the state is when you are
forty frames deep evaluating `(fact 20)`.

It is on the Rust call stack. Each nested `eval` is holding its own `form`, its
own `env`, its own partially evaluated arguments, in Rust local variables. To
return to the browser you must return out of all forty frames — and the instant
you do, Rust destroys them. There is nothing left to resume. You would have to
start the computation again from the beginning.

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
frame stack, each one returning promptly. Stopping is then trivial: you just
stop. Every scrap of state is in `self`, which is in our heap, which we own.

## This has a name

This is SICP §5.4 — the *explicit-control evaluator*, the chapter where Abelson
and Sussman stop using Scheme's own recursion to implement Scheme and build the
evaluator as a register machine with an explicit stack. If you want one
reference for this chapter, that is the one, and it is freely available.

Queinnec's *Lisp in Small Pieces*, chapter 3, arrives at the same structure from
the direction of continuations. The general technique of turning a recursive
function into a loop over explicit state is *defunctionalisation*, and the
general trick of returning to a driver loop instead of recursing is a
*trampoline*.

If you write modern Rust, you already have an intuition for this, because the
compiler does it for you. An `async fn` is rewritten into a state machine whose
local variables live in a heap-allocated struct, exposing a `poll` method that
returns `Ready` or `Pending`. Quokka's `run_slice` *is* `Future::poll`, written
by hand.

Which raises the fair question: why not just use `async`?

Because the state has to be *inspectable*. Those frames are going to be shown to
a user in a debugger, walked by a garbage collector, and written to a file.
`async` puts them in a compiler-generated struct that nothing can look inside.
We need them to be ordinary Lisp objects.

## Who decides when to stop?

Having established that the VM must stop, something has to decide where. This
was a real decision with three candidates, and it is worth seeing why the
unglamorous one won.

First, the unit. A **step** is one `step()` call, one transition of the machine.
Later, when there is bytecode, it will be one instruction. The traditional word
is a *reduction*. It is a rough unit — `(car x)` and a large multiplication are
both one step and take very different amounts of time.

**Option A: the VM asks the host.** A `should_yield()` method the interpreter
calls to ask whether to stop. Calling through a function pointer on every single
step is too expensive, so you would call it every thousand steps — which means
you have built a counter anyway, plus an indirect call on top. It is option C
with extra machinery.

**Option B: the host sets a deadline.** The host says "stop at 11:42:03.016" and
the VM watches the clock. This is the responsive answer: a thousand cheap steps
and a thousand expensive steps both stop at the right moment.

But reading a clock from WebAssembly means calling out to JavaScript, which
costs about as much as a step. So you would check every thousandth step — a
counter again, with a clock on top. And it drags a wall clock into the core VM,
which has a rule against exactly that.

**Option C: the caller passes a budget.**

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
about slices again. The browser passes a number and adjusts it based on how long
the last slice actually took:

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

## Why the boring option won

The feedback loop in that JavaScript gets you option B's responsiveness. The
difference is *where the clock lives* — outside the VM, in the host, where
measuring time is cheap and cannot affect anything the VM computes.

That last part is the real argument, and it is about testing.

Quokka's test strategy is **differential**: the same Lisp file runs on Quokka
natively, on Quokka under WebAssembly, and on SBCL, and all three must print the
same thing. That only works if "the same program" means "the same answer, every
time, everywhere".

Today, with one thread of execution, where you stop makes no difference to what
gets printed. Stopping after a thousand steps or ten thousand produces identical
results; slicing is semantically invisible. The risk arrives later, when the VM
gains green threads. Then the slice boundary decides *when another process gets
to run*, and the interleaving decides the output. If slice length comes from a
wall clock, two runs of the same test interleave differently, and the test suite
starts failing for reasons that have nothing to do with the code.

A step budget makes execution deterministic and therefore replayable: record the
budgets, replay the session exactly. For a system whose debugger is meant to be
a conversation, that is worth having.

Two smaller things fall out. `run_slice(machine, 1)` is a single-step debugger
primitive, for free. And the native host has nothing to implement at all.

So the property the design is actually buying is: **how a computation is cut up
must not change what it computes.** Which is a property, which means it can be
asserted:

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

Honesty about the bill.

**A budget only bounds time if a single step is short.** Once control enters a
Rust primitive, nothing can interrupt it. A primitive that sorts a million-element
list blows any budget, and no scheduler can help. So every primitive must be
either fast or chunked into resumable pieces — a standing constraint on the
whole primitive table, and the cooperative-multitasking failure mode showing up
in its modern form. Option B does not rescue you here; it has exactly the same
problem.

**Two invariants keep slicing invisible.** Collect garbage when something
allocates, not at slice boundaries, so collection timing depends only on what
the program did. And never let Lisp code observe how much budget is left — the
moment it can, the slice becomes part of the language's semantics and everything
above collapses.

## The dividend

This was expensive. The clear recursive evaluator had to be thrown away before
it was written, in favour of a state machine, for a reason that has nothing to
do with Lisp and everything to do with a browser repainting.

It turns out to be the best-value decision in the system, because three other
hard problems quietly become easy:

- **The debugger.** Showing a user their stack, and resuming a frame after they
  fix something, requires stack frames to be inspectable objects. They already
  are — that is where the evaluator keeps its state.
- **Garbage collection.** The collector needs to know every place a live value
  might be hiding. In a recursive evaluator those places are scattered through
  dozens of Rust stack frames, invisible. Here there are four registers and one
  frame stack. [What is alive?](03-what-is-alive.md) is about why that matters
  enormously.
- **Saving a running world.** Writing the image to a file, loading it later, and
  resuming the computation that was in progress. Possible only because the
  computation *is* heap data. [Saving a world](07-saving-a-world.md).

One constraint, three features. That is the shape worth watching for: the
expensive decisions in a system are rarely expensive in only one place, and
occasionally the compensation is spread just as widely.

The machine itself is built in [What a form means](../part-2-reading-and-running/09-what-a-form-means.md), and
[Functions and scope](../part-2-reading-and-running/10-functions-and-scope.md) adds a fourth payoff nobody
planned: because frames are popped before they are acted on, tail-recursive
calls cost no stack at all.
