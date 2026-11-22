# Standing on its own

> **Where this fits.** Part V ended phase 1: Quokka starts from an image in
> which the prelude, every macro and the compiler are bytecode, and the
> compiled world gives the same answers as the interpreted one. But that image
> was still *built* by the interpreter. This part is phase 2, in which the
> compiled compiler rebuilds itself, and the language grows from the minimum
> needed to run a compiler — tier 0 — to the core a working Lisp needs: tier 1.
>
> **Status: this is a plan.** Like chapter 24, it is written before the work,
> and kept as written so that what actually happens can contradict it. The
> decisions marked as made were made while planning; the ones marked open are
> open, and each has an issue. #39 tracks the phase.
>
> *Decided since, and recorded where the chapters that build them will say
> more:* packages (#41) — Lisp objects in the library, adopted after the
> build; the useful core of the package system; one internal package,
> `QUOKKA`; the `%` prefix kept. That moved the Lisp reader and printer to
> straight after packages in the order below.

## What self-hosting means

A compiler is **self-hosting** when it can compile its own source, so that the
system needs nothing outside itself to rebuild. Quokka's compiler can already
*compile* itself — chapter 29 showed that, and chapter 32 used it. What it has
never done is rebuild the system from inside a running image, with no
interpreter involved.

That is phase 2's gate, and the design states it precisely:

> *The compiled compiler rebuilds itself and the core library. Rebuilt compiler
> matches the first build byte for byte.*

Why "byte for byte", and why is that a test of anything? Follow the stages.

- **Stage 0** is the interpreted compiler. It compiles the compiler's source and
  produces **C1** — this is `boot::build_image`, already built.
- **C1**, running on the VM, compiles the same source and produces **C2**.
- **C2** compiles it again and produces **C3**.

If C1 and C2 are identical, the compiled compiler and the interpreted one agree
on the biggest, most demanding program either has seen: the compiler itself.
If C2 and C3 are identical, the compiler is a **fixpoint** — compiling it with
itself changes nothing, and it reproduces itself exactly. GCC's own bootstrap
does the same: it builds itself three times and compares the second and third
results, and a difference fails the build.

There is a famous warning attached to this idea. In his 1984 Turing Award
lecture, *Reflections on Trusting Trust*, Ken Thompson showed that a compiler
can be made to insert a back door into anything it compiles — including the
compiler itself — so that the back door survives even when the compiler's
source is perfectly clean. A self-hosting compiler is only as trustworthy as the
first binary that compiled it. Quokka's answer is the one the design already
gave: the first compiler is not a binary at all, but the compiler's own source
run by an interpreter you can read, and the fixpoint compares against *that*.

## Why all of tier 1, and what that costs

Tier 1 is the design's name for the self-hosting core of Common Lisp:

> *All 25 special operators; `setf` expanders, packages; reader, printer,
> `eval`, `load`; sequences, basic `format`; double floats, bignums.*

The gate itself needs very little of it. The compiler is written in tier 0 —
rule 5 says so — and so it never uses bignums or `format`. What the fixpoint
actually needs is a reader inside the image (to read the compiler's source), a
way to evaluate and load, and packages (for a reason given below).

So there was a choice: end phase 2 at the gate and let the rest of tier 1
arrive when something needs it, or make all of tier 1 part of phase 2.
**Carsten chose all of tier 1.** The phase is much longer that way, and it
means that when phase 3 begins, the Lisp underneath it is a recognisable Common
Lisp rather than a compiler's working subset. Floats and bignums, the reader
and the printer in Lisp, `setf` expanders and `format` all land here.

## The constraint: 1,915 lines

Rule 7 caps the Rust at 15,000 lines across every crate, and phase 1 left
**1,915** of them — for phases 2, 3, 4, 5 and 6 together.

That is not a crisis, but it decides how tier 1 is built. Rule 1 already says
anything a user could want to change belongs in Lisp, above the primitive table.
With this little Rust left, the default for every tier-1 feature is: **in Lisp,
with no Rust at all**. The reader, the printer, packages, `format`, the
sequence functions — all Lisp. Where Rust is unavoidable — a new kind of
binding record, the representation of a bignum or a float — the step says so
and says how much.

## A third body of Lisp

Until now Quokka's Lisp came in two parts: the **prelude**, interpreted at every
boot and compiled into the image, and the **compiler**. Both are tier 0, because
the interpreter has to be able to run them.

Tier 1 cannot go into either. It would put tier-1 features in front of an
interpreter that, by rule 5, only understands tier 0. So tier 1 becomes a
**library** — a third body of Lisp, loaded into the *compiled* world by the
image's own compiler, after the build. The library may use tier-1 features
itself, because by the time it is loaded the compiler that reads it understands
them.

It follows that **tier 1 is compiled only**. The interpreter stays what #25 made
it: the evaluator that builds the first image and is then cold. A test file that
uses special variables or packages runs compiled and against SBCL, and not
interpreted — the differential harness will need a way to say so.

## Special variables first, by shallow binding

The first step surprised the plan: Quokka has **no special variables at all**.
Every `let` is lexical. A **special variable** — what Common Lisp calls a
dynamically scoped variable, conventionally written with asterisks, like
`*package*` — is one whose binding is visible to everything called while the
binding is in force, not just to the code written inside it:

```lisp
(defvar *depth* 0)
(defun show () *depth*)
(let ((*depth* 5)) (show))   ; => 5, because show runs inside the binding
(show)                       ; => 0, because the binding has ended
```

Packages need one — `*package*` — and so does every reader and printer control
variable. So special variables come first.

There are two classic ways to implement them, and the choice had to be made
with phase 3 in mind, because phase 3 brings threads, and with threads "visible
to everything called while the binding is in force" must mean *in this thread
only*.

- **Shallow binding**: the current value lives in the symbol. Binding a special
  saves the old value somewhere and stores the new one; ending the binding puts
  the old value back. Reading is one load. The cost comes with threads: switching
  from one thread to another has to swap each thread's bindings out and in.
- **Deep binding**: bindings live in a list belonging to the thread, and reading
  a special searches the list. Switching threads costs nothing; every read costs
  a search.

**Carsten chose shallow binding.** Genera's processes worked this way. In Quokka
it fits what already exists: the "somewhere" the old value is saved is an unwind
record on the stack, exactly like the ones `block`, `catch` and `unwind-protect`
push (chapter 27), so that leaving a binding by a `throw` or an error restores
the old value through machinery that is already written and tested. Henry
Baker's *Shallow Binding in Lisp 1.5* (1978) is the classic account of the
trade.

## Packages before the fixpoint

The design made one scheduling decision long before this chapter: **packages
belong at the start of phase 2**. The compiler's source is written; put it into
packages after the fixpoint has been established and every compiled function's
symbols change — and the fixpoint has to be established again. So packages come
second, straight after the special variable they depend on, and before anything
is compared.

## The fixpoint early, then everywhere

The order that follows from all this puts the gate in the *middle* of the phase
rather than at its end:

| | step | needs |
| --- | --- | --- |
| A | special variables, by shallow binding | — |
| B | packages | A |
| C | the reader, in Lisp | B |
| D | `eval` and `load` | C |
| E | the image rebuilds itself | D |
| F | **the fixpoint**: C1 = C2 = C3 | E |
| G | the printer, in Lisp | B |
| H | `setf` expanders | — |
| I | `progv`, `load-time-value`, `symbol-macrolet` | A |
| J | sequences | H |
| K | basic `format` | G |
| L | bignums | — |
| M | double floats | L |
| N | the ansi-test harness, from early on | D |

The reason is that once the fixpoint holds, it can be checked on every change
for the rest of the phase. Every later step — the printer, bignums, floats —
must keep the compiler reproducing itself exactly. A standing test that strong is
worth having for as much of the phase as possible.

There is one trap in it already visible. The fixpoint compiles the compiler in
the rebuilt world, and the rebuilt world will eventually have a tier-1 `setf`
with expanders. If that `setf` expands differently from the prelude's, the
compiler's source compiles to different code, and the fixpoint fails for a
reason that has nothing to do with correctness. The rebuild has to compile the
compiler in the environment its source was written for.

## What is still open

Each of these has an issue, or will when its step begins:

- **Packages** (#41) — which packages, how much of CL's package system, and how
  the bootstrap, which reads everything with the Rust reader, copes.
- **Streams** (#42) — the design puts them in tier 2, but `read`, the printer,
  `format` and `load` all reach for them. Minimal string and console streams in
  tier 1 is the leading option.
- **What the fixpoint compares** — every compiled function, structurally, or the
  whole image byte for byte. The second is stronger and also depends on things
  that are not the compiler's business, such as the order objects were
  allocated in.
- **Where a rebuilding image finds its source** — on files, through host
  requests, or carried inside the image.
- **The ansi-test suite** — its licence must be checked before anything is
  vendored (rule 8), and its groups mapped onto tier 1.
- **Bignums and floats** (#16, #17) — decisions written long ago that this phase
  finally has to make.

---

**References.** Thompson, *Reflections on Trusting Trust*, Communications of the
ACM 27(8), 1984 — the case against trusting a self-hosting compiler's first
binary. Baker, *Shallow Binding in Lisp 1.5*, Communications of the ACM 21(7),
1978 — shallow and deep binding, and how to move between them. The GCC
installation manual's section on building, for the stage-two/stage-three
comparison. Hart and Levin, MIT AI Memo 39 (1962), for the first compiler that
compiled itself (chapter 32).
