# Taking turns

> *Where this fits.* Until now a Quokka world did one thing at a time: the
> host asked it to evaluate a form, and it did nothing else until the form
> was finished. The environment that later phases build needs more. A
> program's animation should go on running while the listener answers, and
> a debugger should be able to stop one computation while another carries
> on. This chapter gives the machine **processes**: several computations in
> one world, each with its own stack, taking turns on one machine. Chapter
> 91 is what a program sees of them.

*Status: done, issue #34. Phase 3, step E, sub-step 1. The design is
`docs/processes.md`.*

## An operating system after all

#34 began with a question while we were thinking about windows: with many
programs side by side, windows redrawing and files being read, aren't we
building an operating system? We are, and so were the systems Quokka
learns from. Genera, the Lisp machine's system, ran many *processes* in one
address space; Smalltalk-80 has `Process` and `ProcessorScheduler` objects
in its image. Both are *green threads*: the system switches between
computations itself, on one processor, instead of asking the operating
system for threads of its own.

Oberon, Wirth's system, chose differently. It has no processes at all: one
central loop that takes the next command, runs it to the end, and then runs
a few short background tasks before looking for the next. That is much
simpler, since nothing is ever interrupted, so nothing can be caught
half-way. The price is that a long command freezes everything, and a
program cannot run alongside the person using the machine. We wanted a game
loop to run beside the listener, so we took Genera's road. Oberon's is the
road not taken: no scheduler, no swapping, and no races, which this chapter
and the next have to deal with.

## Most of it was already there

Rule 4 has said since the first chapter that the machine must be able to
stop after any step and carry on later, because the browser's event loop
must keep turning. So the machine already keeps everything about a
computation in the heap: its stack is a vector, and its registers (the
program counter, the stack, frame and mark pointers, the function, the
environment) are a handful of words beside the heap. And the host already
gives the machine a *slice*: a budget of steps, after which the machine
stops and returns.

A process is one more of those sets of registers. The heap's registers are
the running process's, and a **process object** holds a saved set for each
of the others. Switching is storing the running set into its process object
and loading another's. The processes form a ring, and when a slice ends the
machine moves on to the next one round the ring that can run. This is
*preemption*, taking the machine away from a computation that has not
offered it, and it costs nothing extra, because the step budget was already
there. Erlang's scheduler does the same, counting what it calls reductions.

The process object is a new kind of heap object, which Lisp cannot look
inside. If a program could write a saved stack pointer, it could make the
machine read outside a stack the next time that process ran, and no input
may break the machine (rule 10). The check every image passes when it is
loaded now checks every saved set of registers as it checks the running
one.

## Where a switch happens

A process stops running for one of four reasons. Its slice ends; it calls
`%process-yield`; it asks the host for something, a file or a line from the
console, and has to wait; or it ends. The last three happen in the middle
of a slice, and we wanted the host to notice none of it, so the switching
is inside the machine's step:

```rust
    fn step(&mut self) -> Transition {
        let transition = self.step_here();
        if self.mc.register(Register::Process) == Value::NIL {
            return transition;
        }
        match transition {
            Transition::Continue => Transition::Continue,
            Transition::Halt => {
                let running = self.mc.register(Register::Process);
                if self.mc.slot(running, crate::process::MAIN) != Value::NIL {
                    return Transition::Halt;
                }
                self.end_process();
                self.give_way(true)
            }
            Transition::Block => self.give_way(false),
        }
    }
```

`step_here` is the step the machine always took. A world that has never
made a process has no ring, and its step is exactly what it was. With
processes, a halt only stops the machine if it is the *main* process's. The
main process is the one each host call runs in: a line at the REPL, a
build, a file being loaded. Any other process that halts has finished, so
its result is kept and the next process runs. A process that blocks
waiting for the host is *parked*. The machine runs the others, and the host
answers the parked request between slices, so a process waiting for a line
from the console does not stop the rest.

## Bindings, and why they must be swapped

Chapter 35 made special variables *shallow*: the current value of
`*print-base*` is in the symbol, and binding it pushes a record that keeps
the old value until the binding ends. With one computation that is all
there is to it. With two it is wrong, because the symbol has room for one
value, and a binding is supposed to be seen only by the process that made
it.

So a switch swaps bindings. When a process leaves, its binding records are
walked from the innermost out, and each symbol's value is exchanged with
the value in the record. Afterwards the symbols hold what they had outside
the process, and the records hold the process's own values. When a process
arrives, its records are walked from the outermost in and exchanged again,
which puts back exactly what it had:

```rust
    for at in records {
        let symbol = mc.slot(stack, at + mark::DATUM);
        if !mc.is_symbol(symbol) {
            continue;
        }
        let kept = mc.slot(stack, at + mark::SAVED);
        let now = mc.slot(symbol, symbol::VALUE);
        mc.set_slot(symbol, symbol::VALUE, kept);
        mc.set_slot(stack, at + mark::SAVED, now);
    }
```

The order matters when one process binds the same variable twice. Walking
out from the innermost, the inner binding is undone first, and the symbol
ends with the value from outside both. Walking in from the outermost, the
outer binding is redone first, and the symbol ends with the inner value.
This is the scheme Genera used; the standard reference for shallow binding
and its alternative, *deep* binding, which keeps bindings in a list per
process and makes switching free and every access a search, is Baker's
*Shallow Binding in Lisp 1.5*. A switch costs one exchange for each binding
open in the two processes, which in practice is a few dozen.

## Ending a process

A process that has to be stopped from outside, because it loops for ever or
because its work is no longer wanted, should still run its cleanups: the
`unwind-protect` that closes a file or releases a lock. That means running
code in the process being stopped, from the middle of whatever it was
doing.

The machine can already do that, because unwinding is one of its modes: a
`throw` sets a register saying what it is looking for, and from then on
every step passes one record on the way down, running cleanups as it goes
(chapter 27). So ending a process sets that register, in the process's
saved set, to a target no record matches. When the process next runs, it
unwinds through every record it has, running every cleanup in its own
context and undoing every binding, and when it reaches the bottom of its
stack with nothing found, it ends. Genera's `process-kill` does the same.

What we could not do the same way is *interrupt* a process: run a function
inside it and then let it carry on where it was. That is what Ctrl-C into
the debugger needs. A function called at an arbitrary point returns a
value, and the process's stack has no room for one there. It needs a new
kind of frame that returns nothing, and is left for later.

## What did it cost?

In the machine: the process object and its checks, the ring, the switch
with its swap, the scheduling in the step, finding a parked request for the
host, and eleven primitives. All of that is Rust because it is the machine's
registers that move. Image format 10. A test drives the primitives directly:
the order in which processes take turns, which is fixed by the ring and the
slice size, so a test can check it exactly; bindings kept apart; a process
ended with its cleanup run; one preempted without ever yielding; one the
machine stops with an error; and one parked on the console while the others
run.

## What is still missing?

The terminal REPL reads its next line itself, so while it waits, no process
runs. A world's processes run only while the host is running something, and
the listener of phase 4, which waits for input as a process like any other,
will be the first to keep them running. There are no priorities, and no
sleeping, since the machine has no clock to wake by. And whether a world
should ever use more than one processor, #34's last question, is still
open; nothing here decides it.
