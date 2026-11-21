# Why compile at all

> **Where this fits.** Part II finished with a working Lisp. It reads, it
> evaluates, it has macros and conditions and structures, and seventeen test
> files say it agrees with SBCL. So the obvious question is why Part III
> exists at all — and the obvious answer, *speed*, is the less interesting
> half.
>
> **Status: this was a plan, and it is kept as it was written.** When this
> chapter was written almost none of Part III existed. It is the opposite of
> every chapter before it: the reasoning *before* the work, left standing so
> that what actually happened can contradict it rather than quietly agreeing
> with hindsight.
>
> It is now all built — the VM, the compiler, the image — and the plan held
> better than it deserved to. Three things it did not know: the instruction
> set needed thirty-eight opcodes rather than thirty-four — the thirty-seventh
> found by the *compiler* and not the VM, the thirty-eighth when `tagbody`
> wanted a one-step `go` (chapter 28); the compiler
> would be able to compile itself before anything could run it (chapter 29);
> and the hardest problem of the phase would be none of this, but the fact
> that a garbage collection cannot happen inside a mutation (chapter 28's
> interlude).
>
> The decisions below *are* settled — they were argued out in #25 and #26
> before a line of the compiler existed, which is the point of writing a plan
> down. One of them turned out to need code immediately, and the last section
> is about that.

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

### How wide is one instruction?

A second question, independent of the first, and the one where the obvious
answer lost.

The obvious answer is **one 64-bit word per instruction** — `[opcode: 8][a:
24][b: 32]` — because a `Code` object already *is* an array of raw 64-bit
words, so nothing new is needed, decoding is two shifts, and a disassembler is
a `for` loop. Lua does exactly this with 32-bit words and is not a slow
language.

The argument against it is memory, and it is specifically a **wasm32**
argument. Packed bytes average **2.6 bytes against eight** — measured, once
the instruction set was actually written out and a real function compiled by
hand; the guess while deciding was two, which was optimistic and is corrected
in `docs/bytecode.md`. So roughly three times smaller rather than four, which
is nothing at phase 1 — a couple of hundred kilobytes — and a few megabytes
for a mature image with the environment in it. Three things make that worth
having: wasm32 is a 32-bit address space, so the ceiling is real rather than
theoretical; a copying collector needs two semispaces, so every figure
doubles; and `docs/design.md` has the whole image being *downloaded* in a
browser.

The case *for* the word encoding was simplicity, and half of it did not
survive scrutiny. "Byte offsets cause off-by-one bugs" is true of almost
everything and is what tests are for. The half that is real has a name:

> **Branch relaxation.** If operand widths vary — varints, as CPython uses —
> then an instruction's size depends on its operands, a jump's distance
> depends on the sizes of everything between it and its target, and those
> sizes depend on the jump distances. That is a fixpoint, and an assembler
> solves it by iterating until it converges.

But that is a cost of *varints*, not of packing. Fix each opcode's operand
widths — `CONST` always two bytes, `JUMP` always two — and an instruction's
size is a function of its opcode alone. Sizes are known the moment the opcode
is chosen, backpatching is one pass, and the fixpoint never arises.

So: **packed, with fixed widths per opcode**, and ceilings that are compiler
errors rather than wraparound. A 16-bit jump offset means a function with more
than 64 KB of code is *reported*; the JVM has had the identical limit per
method for the identical reason. A silent truncation would be a bug with no
symptom anywhere near its cause.

One consequence worth stating because it makes the decision cheap: **the
encoding is a boundary**. The compiler emits instructions as lists —
`(LOCAL-REF 0 1)` — and an assembler encodes them. The assembler, the VM's
fetch and the disassembler are the only code that knows the byte layout. If
this turns out wrong, three places change and the compiler does not.

## Four things, in order

### 1 · The instruction set, on paper

Before any code. What are the instructions, what does a call frame look like,
how is a closure's captured environment addressed?

This is the decision with the longest shadow, because everything else encodes
it. It is also the one most worth arguing about on paper rather than in Rust,
where the cost of being wrong is a rewrite.

One question inside it is a real trade rather than a detail, and it has been
settled. A closure has to remember the variables it captured, and there are
two ways:

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

**The chain won**, but the first reason given for it was wrong, and that is
worth following because it is the most instructive thing in this chapter.

### A decision made on a premise nobody had checked

The original argument was interoperation. It said the two evaluators have to
call each other in both directions for a long time — an interpreted REPL form
calling the compiled `length`, a compiled compiler calling an interpreted
macro expander — so they had better share one environment representation and
one frame stack, or every unwind would have to cross a boundary.

Each of those examples is real *if* a running image contains interpreted code.
And that assumption was never stated, because it was never noticed. Stated
plainly, it does not survive:

> The interpreter is the **bootstrap** evaluator, not a second production one.
> It runs the compiler, the compiler builds the image, and after that only the
> bytecode VM runs. A form typed at a REPL, or handed to `eval`, is
> **compiled and then run**. The interpreter stays in the binary so that an
> image can be rebuilt from source with nothing but `cargo`, and is otherwise
> cold.

Under which every example dissolves. The REPL form is compiled before it calls
anything. The expander was compiled when it was defined. `eval` means
compile-then-run. Interpreted-versus-compiled testing is two separate runs in
two separate heaps. Rebuilding from source is entirely interpreted, because
nothing compiled exists yet.

This is not a clever workaround; it is what the reference implementation does:

```
* sb-ext:*evaluator-mode*
:COMPILE
* (sb-kernel:%fun-name (eval '(lambda (x) x)))
(LAMBDA (X))
```

Typing a form at SBCL's REPL compiles it. SBCL's full interpreter exists and
is switched off by default.

One rule keeps it true through the bootstrap, where the hole would otherwise
be: compiling a file whose later forms use a macro defined in an earlier one
means the compiler must expand a macro it has just processed. So **while the
compiler is interpreted, it evaluates `defmacro` into the interpreter**, and
an expander is never compiled while its caller is not. One evaluator at a
time, start to finish.

### What the corrected premise changed, and what it did not

The frames answer **reversed**. With no mixed execution there is no boundary
for an unwind to cross, so compiled frames become regions of one growable heap
vector — a call pushes words and allocates nothing, which is what every
production bytecode VM does. The vector is an ordinary traced heap object, so
slices, image save and the collector are untouched. One detail earns its
place: popped slots are cleared, one store each, because a traced object's
dead slots would otherwise keep whatever they last held alive.

The variables answer **stood**, on a worse reason. Not "the two
representations must interoperate" but "flat closures and stack-allocated
locals need escape analysis, that is compiler work, and phase 1 is already a
long walk before anything runs." `ObjectType::Environment` is still the right
shape — its doc comment, from chapters ago, says *"this is the layout lexical
addressing uses, so when the compiler arrives it replaces the name scan with
an index and the data structure does not change"* — but the chain is now a
deferral rather than a conclusion, and the instruction set will change when
the faster design lands.

### The part worth keeping

A decision can be correct and badly argued, and the two come apart more often
than is comfortable. The chain survived; the reason for it did not, and the
frames went the other way the moment the hidden assumption was said out loud.

Which is the strongest case this book can make for its own method. The wrong
premise was not a missing fact — everything needed to spot it was already in
`docs/design.md`, in one sentence about the interpreter staying "as a
debugging fallback". It was an assumption that had never been written in a
place where someone could disagree with it. Writing the plan down is what made
it contradictable.

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

The interpreter does not retire — it goes cold, which is not the same thing.
It stays in the binary for two jobs: rebuilding an image from source with
nothing but `cargo`, and being a second opinion when the compiler is the thing
under suspicion. Neither job runs it *alongside* compiled code; each is a
separate run of the whole system.

Which hands the testing a gift. Chapter 12 built a harness where one file runs
three ways — Quokka native, Quokka under WebAssembly, SBCL — and all three must
print the same thing. Phase 1 adds a fourth leg: run the same file *through the
compiler*, and the output must match the interpreted run, byte for byte.

That is a strong test, because the two paths share almost nothing. The
interpreter walks a tree in Rust; the compiled path runs Lisp that emits
instructions that a different Rust loop executes. A disagreement is a real bug
in one of them, and the harness says which file and which form.

Two runs, two heaps — which is exactly why keeping the interpreter costs
nothing in the design. A second evaluator you never run at the same time as
the first is not a second mechanism to maintain at every boundary; it is a
spare.

It is also the first time Quokka can check itself against *itself* rather than
against SBCL. Everything in `tests/lisp/` so far is constrained to portable
Common Lisp, because SBCL has to be able to run it — which is why chapter 22's
`%` names and chapter 21's structures are partly untestable. Interpreted
against compiled has no such constraint. Both halves are Quokka, so the whole
language is in scope.

## A thing found on the way: literals are not yours to edit

One question led somewhere unexpected, and it is the only part of this chapter
that produced code.

If a compiled image holds every function's constants, should two functions
that both contain `"hello"` share one string? The technique has a name —
**interning** for strings specifically, **hash consing** for the general form,
canonicalising every structure so that structurally equal objects are also
pointer-equal. Quokka already does it for symbols; that is what the obarray in
chapter 8 *is*.

The obvious split is: share literals when building the image, give run-time
strings their own storage so they stay mutable. Which is right in shape, and
the line is in the wrong place. It is not *when the string was made*, it is
**literal or freshly made** — and literals do not stay in the compiler. They
are returned, stored and passed around. Here is the state of the system before
any of this was touched:

```lisp
quokka> (defun greeting () "hello")
GREETING
quokka> (setf (aref (greeting) 0) #\J)
#\J
quokka> (greeting)
"Jello"
```

A program that meant to change a string it was handed has edited the source of
a function. That is conforming — CLHS 3.7.1 says the consequences of
destructively modifying a literal are undefined — and it was a live trap with
no interning anywhere near it.

**Sharing does not create that hazard. It widens it.** Today the mutation
spoils one function. With literals coalesced it spoils every function that
happened to contain `"hello"`, and the symptom surfaces arbitrarily far from
the cause. Which is the argument against reading "undefined behaviour" as
permission: the standard allows the sharing, it does not make the result
debuggable.

### Immutability is what makes sharing sound

There are two ways to protect a shared string, and the system was already
using one. `SYMBOL-NAME` hands back a **copy**, so `(eq (symbol-name 'foo)
(symbol-name 'foo))` is false here. Safe, conforming, and it protects a shared
thing by refusing to share it — the opposite of what interning is for.

The other way composes, because *sharing is only unsound when something can
write*. So a literal became a heap type of its own:

```rust
String         = 64,
Code           = 65,
Bignum         = 66,
ConstantString = 67,   // a literal, or a symbol's name
```

A type rather than a flag bit, for two reasons that are both chapter 5's. The
header's single subtag test is what makes the collector's linear scan one
comparison, and a flag would cost that. And 67 is above the 64 line, so
`slots_are_values()` keeps answering correctly without being told anything new
— the traced/raw split built for strings in chapter 19 absorbed a type it had
never heard of.

Every reader — `aref`, `length`, `stringp`, the printer, the hasher — asks
`is_string()` and gets yes for both. Exactly one place asks which:

```lisp
quokka> (handler-case (setf (aref (greeting) 0) #\J)
          (type-error () 'refused))
REFUSED
quokka> (greeting)
"hello"
```

This is chapter 22's distinction wearing new clothes. "Do not modify literals"
was a request, living in a document, enforced by nothing. Now it is a
guarantee, enforced by a type. *A rule that can be followed needs discipline;
a rule that cannot be broken needs none.*

### And a hole it opened

With literals read-only, there was suddenly **no writable string in the
system** — the prelude had a comment admitting as much, since `make-string`
had never been exposed to Lisp. So the change came with `%make-string`, and
`make-string` and `copy-seq` above it, which is also what a Common Lisp
programmer reaches for when they want a string they may edit.

That is worth noticing as a shape rather than an incident. Taking a capability
away from a system means checking what used it, and "nothing in the tests"
is not the same answer as "nothing".

### What the oracle cannot settle

`(setf (aref "abc" 0) #\z)` is undefined, so SBCL's interpreter permits the
write and Quokka signals, and **both conform**. A differential test cannot
adjudicate that, so the refusal is tested in Rust and `tests/lisp/README.md`
gained a fourth entry in its list of what may never go in a test file.

That list is becoming one of the more interesting documents in the repository:
each entry is a place where "agrees with SBCL" stops being the definition of
correct.

## What is deliberately not decided

Two questions remain, and [#23](https://github.com/quokka-lisp/quokka-lisp/issues/23)
holds them rather than this chapter:

1. What the image format guarantees across versions.
2. Where macroexpansion happens, once there are two evaluators that both need
   it.

Neither changes the instruction encoding, which is why they could be left.

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
