# Leaving early, again

> **Where this fits.** Chapter 25 showed instructions and a stack; chapter 26
> showed how an instruction names a variable. Both assumed control goes
> forwards, or jumps somewhere inside the same function. This chapter is
> everything else: `return-from`, `throw`, `go`, `unwind-protect`,
> `handler-case`, and forms that return more than one value.
>
> It is the second time this book has built that machinery. Chapter 14 built
> it for the interpreter and chapter 20 reused it for conditions. Doing it
> again on a different machine is the interesting part, because what changes
> and what does not is not what you would guess.
>
> **Status: built.** These are the last of the opcodes the specification
> listed, and the VM
> now implements all of them.

## The problem, restated for a stack

```lisp
(block out
  (mapcar (lambda (x) (when (bad x) (return-from out 'nope)))
          items)
  'all-fine)
```

The `return-from` is inside a closure, inside a call, maybe several calls
deep. It has to abandon all of that and deliver `'nope` where the `block`
was.

Chapter 14 did this with a frame stack made of cons cells: push a frame
carrying a **marker**, and unwind by popping frames until one carries the
marker you want. The idea survives. What does not is the cons cells, because
chapter 24 replaced the frame stack with one contiguous vector.

So where does a marker live now?

## Records on the stack

A record is **seven slots on the operand stack**, written where the stack
happened to be:

| | |
| --- | --- |
| `PREV` | where the next record down is |
| `KIND` | block, catch, tag, protect, handler |
| `DATUM` | the marker, the tag, the clause list, the cleanup's address |
| `FP` `PC` `FUNCTION` `ENV` | enough to carry on from |

Pushing one allocates nothing, which is the whole reason for the contiguous
stack.

The last four slots deserve a name: they are a **frame**. A record is a frame
plus a reason. Which is not a coincidence — "carry on from here" is the same
question whether you are returning normally or abandoning six frames, and the
answer has the same shape.

### Why there is a chain at all

`PREV` looks redundant. The records are already in order on the stack; why
not scan down from the top?

Because **nothing distinguishes a record slot from a value slot by looking**.
A record is seven words, and so is a run of seven numbers a program is in the
middle of computing. There is no tag bit for "this word is the start of a
record" — the words are ordinary values, and the only thing that knows where
a record begins is whatever wrote it down.

So the VM keeps one register, `mp`, holding where the innermost record is,
and each record holds the index of the next. Finding the next one down is a
read rather than a search.

It is the same reasoning as chapter 5's object headers, arriving from the
other direction. There, the collector scans *forwards* and every object has
to say how big it is. Here nothing scans at all, because a chain was cheaper
than making every word self-describing.

## Unwinding is a mode

```rust
if self.is_unwinding() {
    return self.step_unwind();
}
```

One record per step, not a loop that runs to completion. Rule 4 does not stop
applying because something went wrong: an unwind through ten thousand frames
is ten thousand steps, and the browser gets control back in the middle of it
if the budget runs out.

Which is exactly what chapter 9's explicit-control evaluator was for, arriving
in a machine that otherwise shares none of its design.

> **A small thing that would have been a bug.** "Is it unwinding?" has to be
> answered from somewhere, and the obvious spelling is *the `Unwind` register
> holds `nil` when nothing is*. But `(catch nil ...)` is perfectly legal and
> `nil` is a fine tag to throw to, so that spelling cannot tell "throwing to
> nil" from "not throwing". The register holds **`UNBOUND`** instead —
> chapter 8's marker for "no value here", which no program can produce.
>
> The interpreter does store `nil` there and is fine, which is worth
> understanding rather than waving at. It has a *mode*: chapter 9's machine
> is always in one of Eval, Return or Unwind, so which half-cycle it is in
> answers the question and the register only has to say *what it is looking
> for*. This machine has no modes — it fetches an instruction and does it —
> so the register had to answer both questions, and one value had to be
> reserved.
>
> `(catch nil (throw nil 42))` returns 42 on both. It was worth checking: the
> first draft of this chapter claimed the interpreter had a latent bug here,
> and it does not.

## What each operator became

### `block` and `return-from`

`BLOCK` makes a fresh marker, binds it in the environment under the block's
name, and pushes a record. `RETFROM` reads the marker back out — by
`(depth, index)`, like any other variable, except in the block namespace —
and starts unwinding towards it.

Why go through the environment, when the block is right there lexically and
its record is at a known stack depth? Because of the example at the top of
this chapter. The `return-from` is inside a closure that has been passed to
`mapcar`, and when it runs, nobody knows how deep the stack is. A marker in
the environment travels with the closure; a stack depth does not.

`tagbody` and `go` work identically, in the tag namespace, for the same
reason — and only for tags a closure can actually reach. Chapter 25 said most
`go`s become `JMP`; this is the remainder.

### `catch` and `throw`

The same, minus the environment. A tag is an ordinary value, evaluated at run
time, so `CATCH` pops it and puts it straight in the record. There is nothing
lexical to look up, which is the entire difference between the two mechanisms
and the reason Common Lisp has both.

### `handler-case`, and a decision about where judgement lives

A `HANDLER` record stops **any** condition. It does not look at its clauses.

The obvious design is the other one: put the clause list in the record, and
have the unwinder check whether this handler wants this condition. That is
what the interpreter does, and what `docs/bytecode.md` originally specified.

It is wrong here, and rule 1 says why:

> **Rust below the primitive table, Lisp above it.**

Deciding whether `(error 'my-thing)` matches a clause naming `my-thing` means
knowing what a `handler-case` clause looks like. That is a question about the
*language*, and the moment it is answered in Rust, changing how handlers match
means changing the VM.

So the record says only *a handler is here*, the unwinder resumes into
compiled code, and the clause dispatch is **ordinary instructions** — type
tests and jumps, compiled from the clause list like anything else. If no
clause matches, that compiled code signals the condition again, which searches
further out because this record has already been popped.

The VM got smaller and the language got more changeable. That trade is
available surprisingly often once you are looking for it.

## `unwind-protect`, which is the hard one

```lisp
(unwind-protect (do-something) (close-the-file))
```

The cleanup must run **however** the body ends: normally, by a `throw` past
it, by a condition, by a `return-from` from inside a closure three frames
down. And when it has run, whatever was happening must carry on happening.

That last clause is what makes it hard. A cleanup reached by an unwind is not
the end of anything — the unwind was on its way somewhere and still is.

### Putting the unwind down and picking it up again

When the unwinder meets a `PROTECT` record it does not run the cleanup and
continue. It **puts everything it is carrying into another record**: the
target it was looking for, the value, and the full list of values. That is a
`PAUSED` record. Then it jumps to the cleanup and stops unwinding.

The cleanup is now ordinary code, running normally, with no idea it was
reached by a catastrophe. It ends with one instruction, `UNPROTECT`, which
takes the three pieces back out of the paused record and starts the unwind
again from where it had got to.

```lisp
(let ((order nil))
  (catch 'done
    (unwind-protect
         (unwind-protect (throw 'done 1)
           (setq order (cons 'inner order)))
      (setq order (cons 'outer order))))
  order)                                  ; => (OUTER INNER)
```

(`setq` rather than `push`, because `push` needs the `setf` expander Quokka
has not got yet — chapter 21 says why.)

Each cleanup pauses the unwind, runs, and resumes it; the next one out does
the same. Innermost first, for free, because each is just the next record
down.

### The cleanup is compiled twice

There are two ways to reach it — falling into it normally, and being sent to
it by an unwind — and they need different things afterwards. The normal path
carries on with the next form; the unwind path has to resume unwinding.

The cheap answer is to emit the cleanup's code **twice**, once for each path.
It costs a little space and buys the alternative away: sharing one copy would
need the cleanup to know who called it, which means a return address, which
means a frame — and allocating a frame for a cleanup is exactly what the
contiguous stack was for.

A compiler emitting the same source twice feels wrong the first time you meet
it. It is routine, and it is a good illustration of the difference between a
compiler and an interpreter: the interpreter has one cleanup because it has
one `unwind-protect` *object*, and the compiler has no objects at all — only
instructions, which it can put anywhere, as many times as it likes.

## More than one answer, on a stack

Chapter 23 built multiple values for the interpreter: a second register
holding `UNBOUND` to mean "just one, in the `Value` register", so the ordinary
case costs nothing.

That convention survives, and one thing about it does not.

**In the interpreter, both the primary value and the extras are in
registers.** Collecting every value is two register reads.

**In the VM the primary value is on the stack** — that is what a stack machine
*is* — and only the extras are in a register. So `MVSAVE`, the instruction
that turns "whatever the last form returned" into a list, has to **pop before
it builds**.

I did not notice, and pushed the list on top of the value instead of in place
of it. The symptom:

```
; (1 2) is not a function
```

`multiple-value-call` popped a fixed number of things off a stack with one
extra item on it, got the wrong ones, and tried to call a list. The error was
three instructions away from the mistake and named something that had nothing
to do with it.

Which is the shape of nearly every bug in a stack machine. The stack is an
agreement about what is where, enforced by nothing at run time, and an
instruction that leaves one item too many does not fail — it makes every
instruction after it quietly wrong.

That was the second such bug in two days, and both were decidable *before the
program ran*. So there is now a **verifier**: one pass over a function when it
arrives, walking every reachable instruction and tracking how deep the stack
would be. It refuses a jump that lands anywhere but the start of an
instruction, a pop with nothing to pop, two paths that reach the same place
disagreeing about the depth, a function that runs off its end, and one that
needs more stack than it reserved. The JVM has done this since 1995, and its
specification's §4.10 is the fullest published description.

It found something immediately. The test helper had been declaring sixteen
slots for a program that needed twenty-three, and nothing had noticed, because
the stack grows on demand — `max-stack` was decorative. It is computed now,
by the verifier, which is what an assembler should do and what the compiler
will do.

What it cannot check is worth saying too. Types: a verified function will not
corrupt the machine, but it may still add a symbol to a string. And the
miscounted scope depth from chapter 26 is out of reach, because both numbers
are in range and simply name the wrong variable — only the run-time namespace
check sees that one. A verifier removes a class of bug, not a feeling of
doubt.

## Tier 0 can now be compiled

Every instruction in the specification is implemented, which means every
special operator in chapter 13 has somewhere to go.

(Thirty-six of them. A thirty-seventh arrived later, when the compiler wanted
to write the function namespace and found that nothing could — `flet` does not
need it and `labels` does, and no amount of working on the VM was going to
notice. `docs/bytecode.md` records how. A thirty-eighth, `JMP-TAG`, came with
`tagbody`, and chapter 28 tells the story of why the `TAG` and `GO` in this
chapter turned out not to be enough on their own.)

What that is *not* is a compiler. There is a machine that can run compiled
code and no compiler to produce it — everything in this chapter was tested
by assembling bytecode by hand, which is tedious on purpose: a bug in a
twelve-instruction program written by hand is a bug in the VM, where the same
bug found after the compiler exists could be in either.

The next chapter is the compiler, written in Lisp, and the one after that is
the image. Then the gate: an image saves, reloads, and resumes where it left
off.

---

**References.** CLHS 5.2 for the exact rules about transfers of control and
the order cleanups run in — worth reading once, because the corner cases are
not guessable. Queinnec, *Lisp in Small Pieces*, chapter 3, on continuations
and what `unwind-protect` costs a compiler. For the verifier this chapter
asks for: Lindholm & Yellin, *The Java Virtual Machine Specification*,
chapter 4.10, which is the most complete published description of checking
stack discipline ahead of time.
