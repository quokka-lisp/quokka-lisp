# Why compile at all

> **Where this fits.** Part II finished with a working Lisp. It reads, it
> evaluates, it has macros and conditions and structures, and seventeen test
> files say it agrees with SBCL. So the obvious question is why Part III
> exists at all — and the obvious answer, *speed*, is the less interesting
> half.
>
> **Status: none of this is built.** This chapter is written before the work
> rather than after it, which is the opposite of every chapter so far. It is
> the plan and the reasoning behind it, kept deliberately so that what
> actually happens can contradict it. Where it turns out to be wrong, the
> chapters after it will say so rather than quietly agreeing with hindsight.

## The interpreter works. That is the problem.

Here is what Quokka can do today:

```lisp
(defstruct point x y)
(let ((p (make-point :x 1 :y 2)))
  (setf (point-x p) 42)
  (point-x p))                        ; => 42
```

Nothing above is cheating. `defstruct` is a macro written in Lisp, `setf` is a
macro written in Lisp, the accessor it expands into is a function written in
Lisp. Several hundred lines of this system are already written in itself.

And yet **the language is defined in Rust**. Open
`crates/quokka-vm/src/eval.rs` and you will find, in a `match`, the decision
that `let` makes a new scope and `if` evaluates one of two branches. Those are
not library functions that a user could replace. They are a Rust program's
opinion about what Lisp means, and rule 1 says that is exactly where the line
is drawn:

> **Rust below the primitive table, Lisp above it.**

The interpreter sits on the wrong side of it. Not by mistake — something had to
run Lisp before Lisp could run Lisp, and chapter 2 is the whole story of why
that something is a Rust state machine. But it was always meant to be
temporary, and Part III is where it stops being load-bearing.

After phase 1, what `let` means is written in a Lisp file. The Rust underneath
does not know what `let` is. It knows how to execute instructions.

That is the point. Everything else in this chapter is consequences.

## The speed argument, properly

Speed is a real reason, and it is worth understanding precisely rather than as
folklore, because *which* work compilation removes tells you what the compiler
has to be.

Take one line:

```lisp
(+ x 1)
```

Every single time the interpreter meets that form, it does all of this:

1. Look at the form. Is it a symbol? A number? A list? It is a list.
2. Look at the head. Is `+` a special operator? Check the table. No.
3. Is `+` a macro? Look up its function cell, check the object's type. No.
4. So it is a call. Evaluate each argument.
5. For `x`: walk the environment chain, comparing symbols, until a binding for
   `x` is found.
6. Call the primitive.

Steps 1 to 4 are **analysis**: working out what kind of form this is. They give
the same answer every time, and the answer depends only on the source text. If
this line is inside a loop that runs a million times, that analysis happens a
million times and produces the identical conclusion a million times.

Step 5 is a **search**. `x` is found by walking a chain of environments
comparing symbols, so the cost depends on how deeply nested the code is and how
many variables are in scope.

A compiler removes both. It does the analysis once, before anything runs, and
writes down the conclusion:

```
  LOCAL-REF 0 1      ; x — one scope out, slot one
  CONST     1
  PRIM-ADD
```

Now the loop executes three instructions a million times, and none of them asks
what kind of form this is, because that question was answered in the past
tense.

**The name for this.** Moving work from run time to a time that happens once
is the entire idea of compilation, and there is a beautiful halfway house
between the two that is worth knowing: SICP §4.1.7 rewrites its interpreter so
that `analyze` runs over the source once and returns a *Rust-closure-like
procedure* which does only the execution part. No bytecode, no compiler — just
the realisation that the two phases were tangled and could be separated. It is
the shortest path from the thing Part II built to the thing Part III builds,
and reading it makes the rest of this part feel inevitable.

### Lexical addressing

Step 5 deserves its own name, because it is the single biggest difference
between interpreted and compiled code in a Lisp.

The compiler knows, when it looks at `(lambda (y) (+ x y))` inside
`(let ((x 1)) ...)`, that `x` is *one scope out, slot zero*. That is a fact
about the text, available before the program runs. So the instruction can carry
a pair of small integers, and the run-time operation becomes an array index.

The glossary has had an entry for this since chapter 10, ending "which is why
Quokka's interpreter cannot use it yet". Phase 1 is the *yet*.

Nothing at run time compares a symbol again, which also means the symbols
themselves can be dropped from the compiled form — one reason compiled code is
smaller as well as faster. (It also means a debugger has to be *given* the
names back deliberately, which is a cost, and phase 3's problem.)

## What a compiler is, concretely

A function from one representation to another:

```lisp
(compile '(lambda (x) (+ x 1)))   ; source, as a list
;; => #<code 7 instructions>
```

Source is a list, because Lisp. The output is an object of type
`ObjectType::Code`, which already exists in the heap and is already marked as
holding **raw words rather than references** — so the collector will not try to
follow an instruction that happens to look like a pointer. That distinction was
built in chapter 19 and guarded by a test called `raw_types_are_not_traced`,
for a feature that did not exist. It exists now.

### Stack machine or register machine

The instruction set has to choose how instructions name their operands, and
there are two families.

A **stack machine** has no operands. Instructions push and pop an implicit
stack:

```
  CONST 3        ; stack: 3
  CONST 4        ; stack: 3 4
  PRIM-ADD       ; stack: 7
```

A **register machine** names its operands, like real hardware:

```
  CONST  r1, 3
  CONST  r2, 4
  ADD    r3, r1, r2
```

The trade is well-worn. Stack instructions are tiny — often a single byte —
and the compiler is simple, because expressions nest and so does a stack. The
cost is more instructions, each doing less, and more dispatching. Register
instructions are bigger and fewer, and the compiler is harder, because it must
decide which value lives in which register — register allocation, which is a
genuinely difficult problem with a literature of its own.

`docs/design.md` chose **stack**, and the reason is in the sentence after the
choice: *"keeps the VM small and portable, including to WASM"*. Rule 7's 15,000
lines are not a suggestion, and register allocation is a large thing to spend
them on when the comparison is against a tree-walker that is a hundred times
slower than either.

It is worth knowing that this is not the universal answer. Lua famously moved
from a stack machine to a register machine in version 5.0 and got substantially
faster, and the paper explaining why is a good read. The answer depends on how
much you are willing to spend on the compiler, and Quokka's budget is small on
purpose.

## Four things, in order

### 1 · The instruction set, on paper

Before any code. What are the instructions, what does a call frame look like,
how is a closure's captured environment addressed?

This is the decision with the longest shadow, because everything else encodes
it. It is also the one most worth arguing about on paper rather than in Rust,
where the cost of being wrong is a rewrite.

One open question inside it, worth flagging because it is a real trade rather
than a detail. A closure has to remember the variables it captured, and there
are two ways:

- A **linked environment chain** — the closure holds a pointer to the scope it
  was made in, which points to the one outside it, and so on. Cheap to create
  (one pointer), more expensive to read (walk the chain), and it is what the
  interpreter already does.
- A **flat closure** — at the moment the closure is made, copy the captured
  values into it. More expensive to create, but every captured variable is then
  at a known index in one object, so reading is a single indexed access.

Dybvig's *Three Implementation Models for Scheme* is where this is set out
properly, and flat closures are his. The answer interacts with mutation — if a
captured variable can be `setq`'d, a copied value and the original have to stay
in step, which usually means boxing the ones that are assigned.

### 2 · The bytecode VM, in Rust

A loop that fetches an instruction and does it, in slices, with frames in the
heap — all the constraints from chapter 2, which do not relax just because the
thing being executed got simpler.

It can be tested before the compiler exists, by hand-assembling bytecode in
Rust tests. That is worth doing even though it is tedious: a bug found in a
hand-written three-instruction program is a bug in the VM, whereas the same bug
found after the compiler exists could be in either.

### 3 · The compiler, in Lisp

`lisp/compiler.lisp`, and it is the first Lisp source in this project large
enough to be a real program rather than a prelude.

It is also the exam that everything before it was revising for. Tier 0 was
defined as *enough Lisp that the compiler can be interpreted*, and the honest
status of that claim today is: as far as anyone can tell, without having
written the compiler. Chapter 13's inventory of special operators, chapter 18's
lambda lists, chapter 21's `defstruct` — all of them exist because something
guessed the compiler would want them. Writing it is how the guess gets marked.

Rule 5 binds here with full force: the compiler's own source may use only tier
0, no `loop`, no CLOS, only basic `format`. A compiler that needed features its
own output provides could never be bootstrapped.

### 4 · The image

Phase 1's gate is one sentence:

> An image saves, reloads and **resumes where it left off**.

The first two clauses are chapter 7's, and chapter 7 already explained why they
are nearly free: rule 3 makes every heap reference an offset rather than a
pointer, so an image relocates instead of being rewritten.

The third clause is the interesting one, and it is a claim about work already
done. Resuming mid-computation means the *state of the computation* has to be
in the heap — the frame stack, the registers, the pending continuation. A
conventional interpreter keeps those on the host language's call stack, where a
file cannot reach them, and could not pass this gate without being rebuilt.
Quokka's is a state machine with explicit frames, because the browser demanded
it in chapter 2.

So phase 1 collects on a bill phase 0 paid without being asked. This is the
third or fourth time in this book that chapter 2's constraint has turned out to
buy something unrelated, and at this point it is less a coincidence than a
property: *making state explicit makes it transportable*.

There is one thing in a running system that genuinely is not in the heap, and
it is worth saying before it becomes a bug. **Primitives are Rust function
pointers.** They cannot be saved. What an image can store is an *index into the
primitive table*, which quietly makes that table's order part of the image
format — an image saved by one build and loaded by another must agree about
what primitive 21 is.

There are already tests called `the_table_order_is_pinned` and
`the_intercepted_indices_are_still_right`, written after a bug where `apply`
was assumed to be at index 25 and was at 21. They were unit tests about an
internal detail. Phase 1 promotes them to a compatibility contract, which is a
nice illustration of a thing this book keeps running into: *a rule in prose is
a hope; put it where it fails the build.*

## Two evaluators that must agree

The interpreter does not retire. It stays for two jobs: rebuilding the system
from source with nothing but `cargo`, and being the fallback when the compiler
is the thing under suspicion.

Which hands the testing a gift. Chapter 12 built a harness where one file runs
three ways — Quokka native, Quokka under WebAssembly, SBCL — and all three must
print the same thing. Phase 1 adds a fourth leg: run the same file *through the
compiler*, and the output must match the interpreted run, byte for byte.

That is a strong test, because the two paths share almost nothing. The
interpreter walks a tree in Rust; the compiled path runs Lisp that emits
instructions that a different Rust loop executes. A disagreement is a real bug
in one of them, and the harness says which file and which form.

It is also the first time Quokka can check itself against *itself* rather than
against SBCL. Everything in `tests/lisp/` so far is constrained to portable
Common Lisp, because SBCL has to be able to run it — which is why chapter 22's
`%` names and chapter 21's structures are partly untestable. Interpreted
against compiled has no such constraint. Both halves are Quokka, so the whole
language is in scope.

## What is deliberately not decided

Four questions are open, and [#23](https://github.com/quokka-lisp/quokka-lisp/issues/23)
holds them rather than this chapter:

1. Instruction set granularity and the frame layout.
2. Flat closures or a linked environment chain.
3. What the image format guarantees across versions.
4. Where macroexpansion happens, once there are two evaluators that both need
   it.

Each becomes a `decision` issue when it is reached, and closes when the
reasoning is written into `docs/design.md` and into the chapter that describes
it — which is the rule that makes this book possible to write at all.

---

**References.** SICP §4.1.7 for separating analysis from execution — the
smallest possible step from an interpreter toward a compiler — and §5.5 for a
compiler for an explicit-control machine, which is the shape this one will
take. Queinnec, *Lisp in Small Pieces*, chapters 5 to 7: lexical addressing,
then a bytecode machine, then the compiler for it, in that order and for these
reasons. R. Kent Dybvig, *Three Implementation Models for Scheme* (1987), for
flat closures and what they cost. Ierusalimschy, de Figueiredo and Celes, "The
Implementation of Lua 5.0" (2005), for the stack-versus-register argument made
by people who changed their minds.
