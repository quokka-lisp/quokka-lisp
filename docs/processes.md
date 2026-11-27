# Processes

Phase 3, step E (#34). A world runs several computations at once, taking
turns on one machine: a program can run a game loop beside the listener,
a background job beside the debugger. The book's chapters 90 and 91 tell the
story; this file records the decisions, which are also on #34.

## What a process is

A **process** is a heap object (`ObjectType::Process`, `crate::process`)
holding a saved set of the machine's registers: environment, value, values,
unwind, function, operand stack, host request, and the program counter and
the stack, frame and mark pointers. The heap's registers are the running
process's, cached; `Register::Process` says which that is. The processes
are a ring through their `NEXT` slots. A switch stores the running set into
its process and loads the next one's.

The type is opaque to Lisp: only the primitives read or write it. A program
that could write a saved stack pointer could make the machine index outside
a stack (rule 10). `Heap::verify` checks every saved set as it checks the
live one, and that the ring is made of processes. Image format 10.

A process has a state: alive, or finished (its values kept), terminated, or
failed (the machine's error kept as text). A process that has ended is
taken out of the ring (`process::unlink`); whoever holds it can still ask
how it ended.

## Special bindings

Bindings are shallow (#40): the value is in the symbol, and the old value
is in a record on the process's own stack. So a switch **swaps** them. The
leaving process's records are walked from the innermost, each symbol's
value exchanged with the record's; the arriving process's from the
outermost. Afterwards each symbol holds the global value, or the running
process's binding, and each record holds what it held before its process
left. This is Genera's scheme. A switch costs one exchange per binding open
in either process.

## Scheduling

In the machine, round robin, deterministic. A process runs until:

1. its slice ends: the step budget is the preemption, as Erlang counts
   reductions (`one_slice` calls `Vm::preempt`);
2. it calls `%process-yield`;
3. it blocks on a host request, and is then **parked**: the next process
   runs, and the host answers the parked request between slices
   (`process::waiting_request`, `vm::drive`);
4. it ends.

All of it happens inside the machine's step (`Vm::step` around
`step_here`), so `Status` and the hosts are unchanged. Only the **main
process** halts the machine. Each host call (a REPL line, `eval_source`, a
build) runs in it: `Vm::start` and the host entry points switch to it first
(`process::focus_main`). Other processes run interleaved with it whenever
the host runs the machine. The terminal REPL gives them no time while it
waits for a line, because it reads standard input itself.

Rejected: a scheduler process in Lisp, which puts a trip through Lisp into
every switch for a policy nobody needs yet. Priorities and sleeping (the VM
has no clock) are not built.

## Ending a process

`%process-terminate` only **asks**: it sets the target's `TERMINATE` slot
and withdraws its host request. The machine acts at the target's next step
at which ending is safe (`Vm::deliver_termination`). It then sets the unwind
register to a symbol no record matches, `%TERMINATE-PROCESS`, and the
ordinary unwinder passes every record, running each `unwind-protect` cleanup
in the process's own context and undoing each binding. At the bottom of the
stack the process ends as terminated. Unwinding is a mode of the machine
rather than a call, so this works at any instruction boundary. The main
process cannot be ended.

A step is **not safe**, and the end waits, in two cases:
1. The innermost record is a `PAUSED` one: a cleanup that an unwind is
   running has not finished.
2. The library's special `%interrupts-deferred` is true. The library sets
   it around taking and letting go of a lock and around a channel's
   bookkeeping (`%without-interrupts`), and clears it again while a thread
   waits (`%with-interrupts` in `%wait-until`), so a waiting thread can
   always be ended.

These are SBCL's `without-interrupts` and `with-interrupts`. The first
version ended a process at once. The second review of the channels found
the consequence: a thread ended while letting go of a mutex skipped the rest
of the cleanup, and the mutex stayed held by a thread that no longer
existed. `ending_a_thread_waits_until_it_is_safe` sweeps eighty moments;
with the old `with-mutex`, 29 of them left the mutex held. Image format 11
(the slot).

## The compiler's state

#111 decided that the compiler's globals (`%code`, `%constants`, ...) become
special variables bound in `compile-form`. That needs special variables in
tier 0, which means the Rust interpreter learning dynamic binding. Instead,
**every thread binds them at its base**, in `%thread-run` (`threads.lisp`),
with a `let` and a local `special` declaration: the compiler's `setq` then
writes the thread's own binding, which the swap keeps apart from every other
process's. This is bordeaux-threads' `*default-special-bindings*`. The
compiler and the interpreter are unchanged, and so is the fixpoint.
`threads_share_a_world` compiles in three threads at once; without the
bindings, the three corrupt each other.

## Shared memory

One heap, shared, as principle 1 decides. A primitive is one step and no
switch falls inside a step, so `%compare-and-swap-svref` is atomic, and the
mutexes are built on it. Races between processes on shared Lisp data are the
program's to prevent, as in SBCL. The system's own shared tables (the
definitions, the dispatch caches, the compiler's four persistent tables) are
not locked; two processes defining things at the same moment could race.

## The Lisp interface

`lisp/library/threads.lisp`, package **`QUOKKA-THREAD`**, SBCL's `sb-thread`
names, so that code written for SBCL, or bordeaux-threads' SBCL backend,
reads the same:

- `make-thread function &key name arguments`, `join-thread thread &key
  default`, `thread-yield`, `current-thread`, `main-thread`,
  `list-all-threads`, `thread-name`, `thread-alive-p`, `terminate-thread`;
- `thread-error`, `join-thread-error`;
- `make-mutex`, `mutex-owner`, `grab-mutex`, `release-mutex`, `with-mutex`;
- `make-waitqueue`, `condition-wait`, `condition-notify`,
  `condition-broadcast`.

Waiting is a loop that yields until its condition holds. A thread's base
binds the compiler's state, and offers an `abort` restart, *Terminate this
thread*. An error nobody handles ends the thread, with a line on the
console. With the debugger on (#131) it opens in that thread, and the
banner names it.

## The console

The console has one reader at a time, and it is the main process's.
A background thread that reads the console waits, in `%await-console`,
until two things hold:

1. The main process is itself waiting on other threads: it is in
   `join-thread`, `grab-mutex` or `condition-wait`, which set
   `%main-waiting` through `%wait-until`.
2. No other background thread holds the console.

It then takes the console with the atomic compare-and-swap, for a line.
A debugger in a background thread takes it for its whole session, so its
banner appears only when the person can answer it. Each thread reads into
a buffer of its own, its binding of `%console-input`, so two threads' lines
never mix. SBCL also makes a background thread wait for the terminal.

The review of #34 found what happened without this. A background
`read-line` took the next line typed for the REPL. A debugger in a thread
read lines meant for the main process. Two threads reading at once lost
characters from each other's lines.

## What the review found

The review of #34 reproduced three faults, each fixed with a test:

1. **A saved register set the machine could not resume.** A hand-made image
   whose background process had a function that is not compiled code, or a
   request that is not a request, was accepted, and panicked the machine
   when the process ran. `Heap::verify` now checks every set that will be
   resumed as it stands (`a_process_that_cannot_be_resumed_is_refused`).
2. **The console**, above (`a_thread_waits_for_the_console`).
3. **A waiter terminated inside `condition-wait`** left its token in the
   queue, and the next notification woke the dead token. `condition-wait`
   now takes its token out as it unwinds.

It also found some minor faults, fixed:

- `with-mutex` took the mutex before entering its `unwind-protect`.
- A background thread reading lines renewed the main form's step budget.
- This file said `progv` where the code binds with a `let`.

## Channels (layer 1 of the plan for item 5)

`lisp/library/channels.lisp`, in `QUOKKA-THREAD`: Go's model with Lisp
names, built on the threads above.

- `make-channel &key capacity`: 0, the default, is a meeting place; with a
  capacity it is a buffer.
- `send` waits while the channel is full, and on an unbuffered channel until
  a receiver has taken the value.
- `receive` returns the value and t, or nil and nil once the channel is
  closed and empty.
- `try-send` and `try-receive` never wait.
- `close-channel`; sending on a closed channel is a `closed-channel-error`.
- `select` takes the first clause that can go, *in the order written*.
  Go chooses at random; Quokka keeps its programs deterministic.
- `spawn-task function &rest arguments` returns a channel that receives the
  function's value and is then closed: a future. An error in the task is
  carried to `receive` and signalled there, as Clojure's futures do, and so
  is a task ended without a value (a `thread-error`). `function` may be a
  name, which layer 2 will require, since a closure cannot cross into
  another heap.

**The algorithm is Go's runtime's** (`runtime/chan.go`):
- Every operation is one critical section, under one lock for all channels.
  A `select` therefore takes the lock once for all its channels and cannot
  deadlock with another.
- An operation completes at once, through a waiting partner or the buffer,
  or else queues a *waiter* on the channel and waits.
- Only a partner completes a waiter, under the lock, so a waiting thread
  reads one flag that goes from nil to t once.
- A select's waiters share a *selection*: the first partner to complete
  one completes the select, and the others are passed over.
- An operation and its cleanup are one region in which the thread cannot
  be ended. Only its waiting can be interrupted, and the cleanup then
  cancels the waiter whole.
- Arguments are checked before the lock is taken, so a handler for the
  error can use a channel.
- Closing a closed channel is an error, as in Go.

**What the review found.** The first version matched partners less
directly, and the review reproduced six faults, each now a test
(`channels_review_findings`):
1. A sender waiting when the channel was closed could both deliver its value
   and be told it failed.
2. A select that sends and a select that receives deadlocked.
3. `try-send` could wait, after a receiver was terminated.
4. A terminated sender left its value in the channel.
5. A task ended without a value left its receiver waiting for ever.
6. A select clause whose body returned `:none` was taken for no clause.

The plan, decided on #34 for item 5 (27 Nov 2026):
- one OS thread per world for good;
- a pool of worker worlds, one per core, sharing nothing, with copying
  channels and code fetched from the main world on demand;
- the same names on both layers.

Layer 2 is not built; #132 holds its design.

## Left for later

- A `Fault` that ends a background thread is kept as the process's result,
  but nothing is written on the console, since the thread's own Lisp never
  runs again. `join-thread` reports it.

- **`interrupt-thread`, and Ctrl-C into the debugger.** Both need to call a
  function inside a process at an arbitrary instruction boundary, and to
  return from it leaving the process's operand stack exactly as it was. The
  plan is an *interrupt frame*: a frame whose return restores the saved
  `Values` register and pushes nothing, marked in its header. It is not
  built.
- **Layer 2**, the worker pool (#132). One OS thread per world is decided
  (#34 item 5).
- **Timeouts and sleeping**, which need a clock from the host.
- **Locking the system's shared tables.**
