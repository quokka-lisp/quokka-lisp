# Threads

> *Where this fits.* Chapter 90 gave the machine processes: saved sets of
> registers, taken in turn, with their special bindings swapped as they
> come and go. This chapter is what a Lisp program sees of them: threads
> with names and results, locks, and a way to wait until something has
> happened. It also settles a question #111 left open: what becomes of the
> compiler's own working state when two processes compile at once.

*Status: done, issue #34. Phase 3, step E, sub-step 2. The design is
`docs/processes.md`.*

## SBCL's names

Genera called them processes and so does Quokka's design, but a program
written today for a Lisp with threads is most likely written for SBCL, or
for bordeaux-threads, a portability library whose SBCL backend calls
SBCL's own functions. So we used SBCL's names from its `sb-thread` package,
in a package `QUOKKA-THREAD`:

```lisp
(let ((th (make-thread (lambda (a b) (+ a b)) :name "adder" :arguments '(1 2))))
  (list (join-thread th) (thread-name th) (thread-alive-p th)))
;; => (3 "adder" NIL)
```

`make-thread` makes a process that calls the function, and `join-thread`
waits for it to end and returns its values. The rest are as SBCL has them:
`current-thread`, `main-thread`, `list-all-threads`, `thread-yield`,
`terminate-thread`, and the conditions `thread-error` and
`join-thread-error`.

Waiting, in `join-thread` and everywhere else, is a loop that checks whether
what it waits for has happened and yields if not. With a round-robin
scheduler that costs the waiting thread a few steps each time round, which
is cheap while there are few threads, and needs no machinery for putting a
process to sleep and waking it.

## A thread's base

Every thread starts in the same function, which sets up what a thread needs
before calling the program's function:

```lisp
(defun %thread-run (thread function arguments)
  (%set-process-data (%current-process) thread)
  (let ((%code nil) (%constants nil) (%constant-count nil) (%constant-table nil)
        (%pending-name nil) (%toplevel-form nil) (%references nil))
    (declare (special %code %constants %constant-count %constant-table
                      %pending-name %toplevel-form %references))
    (let ((unhandled nil))
      (restart-case
          (handler-bind ((error (lambda (condition) (setq unhandled condition))))
            (setf (thread-values thread) (multiple-value-list (apply function arguments))))
        (abort ()
          :report "Terminate this thread."
```

The `abort` restart is the thread's way out of an error. With the debugger
off, an error nobody handles invokes the innermost `abort` restart, which
here is the thread's: the thread ends, and a line on the console says which
thread and why, so that an error in the background is not lost. With the
debugger on, the debugger opens in that thread, its banner names the
thread, and `abort` ends it. The `handler-bind` around the call is how the
thread learns why it is being aborted. Being outermost, its handler is only
reached by a condition nobody else handled, so it notes the condition and
declines.

## The compiler's state

The long `let` at the top is the answer to a question #111 left for this
step. The compiler is tier-0 Lisp, run by the interpreter during the cold
build, and tier 0 has no special variables, so the compiler keeps its work
in progress in ordinary globals: the instructions so far, the constants,
the name of the function being assembled. A compilation inside another,
when a macro calls `eval`, saves them and restores them. But two processes
compiling at the same time do not nest, they interleave: the scheduler
switches from one to the other in the middle of both, and each overwrites
the other's half-made function.

#111 decided to make them special variables bound by `compile-form`. That
would have needed the interpreter to learn dynamic binding, since it runs
the compiler too. Building processes showed a simpler way, the one
bordeaux-threads offers as `*default-special-bindings*`. Every thread binds
the compiler's globals at its base, and chapter 90's swap does the rest:
when the compiler in one thread `setq`s `%code`, it writes that thread's
binding, and when another process runs, the binding is swapped out. The
compiler did not change at all, so neither did the fixpoint.

We wanted to be sure the test for this was testing something. It compiles
in three threads at once, a dozen small functions each, with the scheduler
switching between them in the middle of their compilations. With the
bindings taken out, the threads did corrupt each other: one compiled
function came back as a value its caller tried to add, and the error was
`The value #<function LAMBDA> is not of type NUMBER`. With the bindings,
all three threads compile correctly.

## Locks

A thread that reads a shared variable, computes, and writes it back can be
interrupted between the read and the write, and another thread's write in
between is lost. Every system with shared memory has this problem, and
principle 1, *one world*, says Quokka's memory is shared: the inspector
looks at a running program's objects directly.

The answer is a lock, and a lock needs one operation that cannot be
interrupted. In Quokka a primitive is one step of the machine, and no
switch falls inside a step, so any primitive is atomic. We added the one a
lock needs, `%compare-and-swap-svref`, which sets an element of a vector
only if it still holds what the caller expects, and says what it held.
Taking a mutex is swapping its owner from `nil` to oneself, and trying again
after a yield if someone else got there first:

```lisp
(defun grab-mutex (mutex &key (waitp t) timeout)
  (declare (ignore timeout))
  (let ((me (current-thread)))
    (when (eq (mutex-owner mutex) me)
      (error "~S is already held by this thread." mutex))
    (do () ((null (%compare-and-swap-svref (mutex-cell mutex) 0 nil me)) t)
      (unless waitp (return nil))
      (thread-yield))))
```

The test runs three threads that each add one to a shared counter forty
times. Each reads the counter, yields deliberately, and writes it back,
inside `with-mutex`, and the counter ends at 120. Wait queues
(`condition-wait`, `condition-notify`) are built on the mutex in the same
way, as SBCL's are.

## Whose console is it?

The review of this step found that we had given every thread a console and
no rules for sharing it. A background thread calling `read-line` took
whatever line was typed next. Usually that was the next form meant for the
REPL, and the REPL then read the line meant for the thread. Two threads
reading at once were worse: both read from the console's one buffer, so
each took characters out of the middle of the other's lines.

The rule we chose is the one SBCL has: the console belongs to the main
process. A background thread that wants a line waits until the main
process is itself waiting for other threads, in `join-thread`, `grab-mutex`
or `condition-wait`, because only then is nobody else reading. It then
takes the console for one line, with the same compare-and-swap as a mutex.
A debugger in a background thread takes it for the whole session, so its
banner appears only when the person can answer it. And each thread now
reads into a buffer of its own, so a line is always one thread's.

## What did it cost?

No Rust beyond chapter 90's. `threads.lisp` is the interface, and the
debugger learned to name the thread it stopped in. The tests are a session
with threads (results, errors, the counter, a wait queue, termination, and
the three compilations) and a thread stopped in the debugger.

## What is still missing?

The system's own shared tables (the definitions, the generic functions'
caches, the compiler's tables of macros and constants) are not locked. Two
threads defining things at the same moment could race, and will need the
same answer as any shared data. There are no timeouts, since there is no
clock. And Ctrl-C, which should stop the main thread in the debugger, needs
chapter 90's missing interrupt.
