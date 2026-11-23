# What has to be built

If you have never built a language before, the hardest part is not any
individual piece. It is that you cannot see the shape of the whole thing, so you
cannot tell what to do first, or why anyone would do it in that order.

This chapter is the map. It says what the parts are, what each is for, and why
they arrive when they do. Nothing here is Quokka-specific. Every language
implementation has to answer these questions; they just answer them
differently.

No prior knowledge is assumed. If a word here is new, it is explained here or in
the [glossary](glossary.md).

## The problem, stated plainly

You type this:

```lisp
(* 6 7)
```

and something prints `42`.

Between those two events, a surprising amount has to happen. Text has to become
structure. Structure has to become action. The number 42 has to exist somewhere
in the machine's memory, in a form that can be handed around and eventually
printed. And when the program is finished with it, that memory has to come back,
or the system eventually runs out.

A **language runtime** is the machinery that makes all of that happen. It is not
the language — the language is the rules about what `(* 6 7)` means. The runtime
is the thing that actually does it.

## The five parts

Almost every runtime is built from the same five pieces.

```text
   text                                                     behaviour
     │                                                           ▲
     ▼                                                           │
 ┌────────┐    ┌───────────┐    ┌─────────────┐    ┌───────────┐ │
 │ reader │───▶│  values   │───▶│  evaluator  │───▶│  printer  │─┘
 └────────┘    └───────────┘    └─────────────┘    └───────────┘
                     │                 │
                     ▼                 │
              ┌─────────────┐          │
              │    heap     │◀─────────┘
              │  + collector│
              └─────────────┘
```

**1. The reader.** Turns text into structure. You type `(* 6 7)`; the reader
produces something the machine can work with — a list containing the symbol
`*` and the numbers 6 and 7. In most languages this part is called a *parser*
and it is a large, fiddly thing. In Lisp it is unusually small, for a reason we
will come to.

**2. Values.** How a piece of data is represented in memory. The number 42, the
character `q`, a list, a function — all of them have to be *some arrangement of
bits*, and something has to be able to tell which is which. This sounds like
plumbing. It turns out to constrain everything else.

**3. The heap.** A big region of memory where values that are too big to fit in
one machine word actually live. A list, a piece of text, an object — these
occupy space in the heap, and something has to hand that space out.

**4. The collector.** Memory is finite. A program running for an hour will
create millions of values it then stops needing. Something has to notice that
and reclaim the space. This is **garbage collection**: the system works out,
automatically and while the program runs, which values nobody can reach any more,
and takes their memory back. You never write "I am finished with this" — it is
deduced.

**5. The evaluator.** The part that actually runs your program. It takes the
structure the reader produced and *does* it: looks up what `*` means, works out
6 and 7, multiplies them.

Plus one more, which is not really a part but is how you meet the system:

**The REPL.** Read, Eval, Print, Loop. A prompt that reads what you type,
evaluates it, prints the result, and comes back for more. Lisp invented this in
1964 and it is now everywhere — a Python prompt, a browser console, a database
shell. It is four of the five parts wired in a circle, which makes it the
cheapest possible way to have something to show.

## Why Lisp makes this unusually tractable

Here is the trick that makes Lisp a sensible thing to build first, and it is
worth understanding before anything else.

In most languages, the text you write and the data your program manipulates are
entirely different worlds. `if (x > 3) { ... }` is syntax; it becomes a *parse
tree*, an internal structure that only the compiler sees, in a form that has
nothing to do with the strings and numbers your program works on.

In Lisp they are the same thing.

```lisp
(* 6 7)
```

is not special syntax. It is a **list** containing three items: the symbol `*`,
the number 6, the number 7. Exactly the same kind of list you would get from
writing a list of data. Lisp's syntax *is* its data structure, and both are
called an **S-expression** (short for "symbolic expression").

Two large consequences:

- **The reader is tiny.** It produces lists. It does not need to know about `if`
  or function definitions or operator precedence, because there is no
  precedence and no statement grammar. Everything is a list.
- **The evaluator consumes the same structure the reader produces.** No separate
  parse-tree types. One representation, used by everything.

And a third, which is the reason Lisp people make such a fuss: since code is
data, a program can build and manipulate programs as easily as it manipulates
lists. That is what *macros* are, and it is what makes a Lisp system able to
extend its own language.

### Lists, and two strange names

Lisp builds lists out of a piece called a **cons cell** — a tiny box with two
slots. The first slot holds an item; the second holds the rest of the list.

```text
   (10 20 30)

   ┌────┬───┐    ┌────┬───┐    ┌────┬─────┐
   │ 10 │ ──┼───▶│ 20 │ ──┼───▶│ 30 │ nil │
   └────┴───┘    └────┴───┘    └────┴─────┘
```

`nil` marks the end. It also means false, and it is the empty list — one value
doing three jobs, which is very Lisp.

The two slots have names you will meet constantly: **car** (the item) and
**cdr** (the rest, pronounced "could-er"). The names are terrible and they are
historical: on the IBM 704 in 1958, the two halves of a machine word were
reached by instructions called "Contents of the Address Register" and "Contents
of the Decrement Register". The machine is sixty-five years gone and the names
stayed. Read them as "first" and "rest" and you have lost nothing.

Building everything out of one two-slot box is why the representation of that
box matters so much, and why it gets special treatment throughout this book.

## The order things have to happen in

Now the part that is genuinely hard to see from outside: why build them in a
particular order?

Some of it is obvious dependency. You cannot write a collector without a heap to
collect. You cannot build a heap without knowing what a value looks like, since
the heap is full of them. So:

```text
  values  ──▶  heap  ──▶  collector  ──▶  reader  ──▶  evaluator
```

The reader needs the heap because reading `(10 20 30)` *allocates* — it builds
those cons cells. The evaluator needs everything.

But there is a second ordering principle that is much less obvious, and getting
it wrong is how projects like this go badly.

> **Contracts come before implementations.**

Some decisions are contained: you can change them later by rewriting one file.
Others are *pervasive* — they appear in the signature of every function you will
ever write, and changing them later means rewriting everything.

The garbage collector is the first kind. It is a few hundred lines in one file,
and it can be replaced without anything outside noticing.

The rule about *how the rest of the code is allowed to touch the heap* is the
second kind. It determines what every allocation returns and what every function
takes. By the time there is a reader and an evaluator, it is in hundreds of
places.

So the pervasive decision has to be made first, even though it sounds like a
detail and the collector sounds like the main event. That is why chapter 4 of
this book is about a contract, and chapter 6 — three chapters later — is the
collector itself.

The general lesson, which applies well beyond language runtimes: **entanglement,
not sophistication, determines what you have to decide early.**

### And one more ordering, which is the whole strategy

There is a third kind of order at work, and it is strange enough to have its
own chapter. The compiler is written **in Lisp** — the language it compiles —
so something must be able to run Lisp before the compiler exists. That
something is a deliberately slow tree-walking interpreter, written in Rust,
which is what most of this book builds. It runs the compiler exactly once, and
is then never needed again.

[The thing that builds itself](the-bootstrap.md) is that idea in full. It is
the most powerful thing in the project and the easiest to walk past.

## The constraint that cuts across everything

Quokka adds one requirement that most runtimes do not have, and it reshapes the
most important piece.

Quokka runs in a web browser.

A browser tab runs your code on a single thread, and it cannot interrupt you.
Once the page calls into your code, the browser gets control back when you
return, and not before. So a program that loops forever does not just loop — it
freezes the tab. No repainting, no clicks, no typing, until the browser offers
to kill the page.

Which means the evaluator must be able to **stop in the middle of a computation
and resume later**. And that turns out to be an enormous constraint, because the
natural way to write an evaluator — a function that calls itself for
sub-expressions — cannot stop. Its state lives on the host language's call
stack, and returning destroys it.

So the evaluator has to be built inside out: as a machine with explicit state
that takes one small step at a time. That is chapter 2, and it is the single
most consequential decision in the system. It also, unexpectedly, pays for
three other things — which is a recurring shape worth watching for.

## The map

Part I builds the substrate: everything underneath the evaluator. Part II turns
text into behaviour, and ends by asking whether any of it is correct. Together
they are **phase 0 and tier 0**, which are finished: `(fact 20)` runs, the
collector survives being run at every opportunity, and seventeen files of
Common Lisp agree with a real one. Part III starts **phase 1**, where the
language stops being defined in Rust; its first chapters were written before
the work and say so at the top, and are left as written so that the later ones
can correct them. Part IV is the compiler, which is a Lisp program rather than
a piece of the runtime — a different kind of thing, and the place where most
of the decisions are still provisional. Part V is the image, where the system
learns to save itself to a file and start from one, and where two decisions
made in Part I for unrelated reasons turn out to be what makes it possible.

Phase 1's gate is met — an image saves, reloads and resumes a computation it
stopped in the middle of — and since chapter 32 the world inside that image is
compiled, with nothing interpreted left in it. Chapter 33 checks that the
compiled world agrees with the interpreted one on every test file, and with
that **phase 1 is finished**: the language is defined by a Lisp file compiled
by itself, and the interpreter is only what builds the first image. Part VI is
**phase 2**, self-hosting, and it opens as Part III did: with a plan written
before the work, kept so that what happens can correct it.

Each chapter answers one question.

| | Chapter | The question it answers |
| --- | --- | --- |
| 1 | [One word for everything](part-1-substrate/01-one-word-for-everything.md) | How is a piece of data represented, and how do you tell a number from a list? |
| 2 | [Why the interpreter cannot call itself](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md) | How can a computation stop halfway and continue later? |
| 3 | [What is alive?](part-1-substrate/03-what-is-alive.md) | How does a system work out which memory is still wanted? |
| 4 | [Rooting](part-1-substrate/04-rooting.md) | How does the implementation language take part without breaking that? |
| 5 | [Objects in the heap](part-1-substrate/05-objects-in-the-heap.md) | What is actually stored in memory, and how do you find where one thing ends? |
| 6 | [Cheney's algorithm](part-1-substrate/06-cheneys-algorithm.md) | How is the memory actually reclaimed? |
| 7 | [Saving a world](part-1-substrate/07-saving-a-world.md) | How can an entire running system be written to a file and brought back? |
| 8 | [Text becomes structure](part-2-reading-and-running/08-text-becomes-structure.md) | How does typed text become objects in the heap — and why is that small in Lisp? |
| 9 | [What a form means](part-2-reading-and-running/09-what-a-form-means.md) | How does a list become an answer, and why must some forms be special? |
| 10 | [Functions and scope](part-2-reading-and-running/10-functions-and-scope.md) | How do you define a function, where do its variables live, and why can it call itself forever? |
| 11 | [The trouble with nil](part-2-reading-and-running/11-the-trouble-with-nil.md) | What happens when one value does three jobs? |
| 12 | [Agreeing with something else](part-2-reading-and-running/12-agreeing-with-something-else.md) | How do you find out whether any of this is *right*? |
| 13 | [The special operators, one by one](part-2-reading-and-running/13-the-special-operators.md) | What are the twenty-five forms everything else is built from, and why those? |
| 14 | [Leaving early](part-2-reading-and-running/14-leaving-early.md) | How do you abandon a computation from the middle of it? |
| 15 | [Code that writes code](part-2-reading-and-running/15-code-that-writes-code.md) | How does a program extend the language it is written in? |
| 16 | [The prelude](part-2-reading-and-running/16-the-prelude.md) | Where does the Lisp that is written in Lisp actually live, and how does it get loaded? |
| 17 | [Why macros matter, and what hygiene is](part-2-reading-and-running/17-why-macros-matter.md) | Why is any of this worth it, and why do Scheme and Common Lisp disagree? |
| 18 | [What a parameter list can say](part-2-reading-and-running/18-lambda-lists.md) | How do optional, keyword and defaulted arguments work, when binding cannot evaluate anything? |
| 19 | [Things with an index](part-2-reading-and-running/19-things-with-an-index.md) | How do you store a run of things, and how does the collector know which words are references? |
| 20 | [Errors you can catch](part-2-reading-and-running/20-errors-you-can-catch.md) | How does an error stop being the end of the computation and become an object? |
| 21 | [Records, and a macro that writes ten functions](part-2-reading-and-running/21-records.md) | How does one line define a type, and how does `setf` learn a place it has never seen? |
| 22 | [The names with a percent sign](part-2-reading-and-running/22-the-percent-names.md) | What are all those `%` names in the prelude, and what is missing that they stand in for? |
| 23 | [More than one answer](part-2-reading-and-running/23-more-than-one-answer.md) | How can a form return two things, when the machine has one register for results? |
| 24 | [Why compile at all](part-3-compiling/24-why-compile-at-all.md) | The interpreter works — so what is a compiler actually for? |
| 25 | [A machine made of instructions](part-3-compiling/25-a-machine-made-of-instructions.md) | What does compiled code actually look like, and what runs it? |
| 26 | [Where is the variable?](part-3-compiling/26-where-is-the-variable.md) | How does an instruction say which value it means — and where does the speed come from? |
| 27 | [Leaving early, again](part-3-compiling/27-leaving-early-again.md) | How does compiled code abandon a computation, and how does a cleanup run in the middle of one? |
| 28 | [A compiler in the language it compiles](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md) | What does a compiler actually do, and which of its decisions are permanent? |
| 29 | [Compiling a whole file](part-4-the-compiler/29-compiling-a-whole-file.md) | Why can a compiler compile the whole system and not be allowed to install any of it? |
| 30 | [Saving the world](part-5-the-image/30-saving-the-world.md) | How does a whole running system become a file, and come back? |
| 31 | [Asking the outside world](part-5-the-image/31-asking-the-outside-world.md) | How does a program read a file when the VM may never wait? |
| 32 | [A world made of bytecode](part-5-the-image/32-a-world-made-of-bytecode.md) | How does a system built by an interpreter end up with no interpreted code in it? |
| 33 | [Two evaluators, one answer](part-5-the-image/33-two-evaluators-one-answer.md) | How do you know the compiled world means what the interpreted one meant? |
| 34 | [Standing on its own](part-6-self-hosting/34-standing-on-its-own.md) | What does it take for a system to rebuild itself, and how would you know it had? |
| 35 | [Variables that follow the call](part-6-self-hosting/35-variables-that-follow-the-call.md) | How can a binding be visible to code written somewhere else entirely? |
| 36 | [Whose symbol is it?](part-6-self-hosting/36-whose-symbol-is-it.md) | How can two programs each have a symbol called `FOO`? |
| 37 | [Eight characters to a word](part-6-self-hosting/37-eight-characters-to-a-word.md) | How should text sit in memory, and why not the encoding everyone else uses? |
| 38 | [Reading, in Lisp](part-6-self-hosting/38-reading-in-lisp.md) | How does a program change the syntax it is read with, and how do you know a new reader reads the old source the same? |
| 39 | [Printing, in Lisp](part-6-self-hosting/39-printing-in-lisp.md) | How does a printer write a symbol so the reader gets the same one back, and why can a right answer fail a test? |
| 40 | [The loop closes](part-6-self-hosting/40-the-loop-closes.md) | What is `eval` when nothing interprets, and what changes when the whole REPL is Lisp? |
| 41 | [The image rebuilds itself](part-6-self-hosting/41-the-image-rebuilds-itself.md) | How does a running world replace its own compiler without being compiled by a half-replaced one? |
| 42 | [The fixpoint](part-6-self-hosting/42-the-fixpoint.md) | How do you show a compiler reproduces itself, when its output can never be literally the same file twice? |
| 43 | [Places](part-6-self-hosting/43-places.md) | How does `incf` update a place it has never seen, evaluating each part once — and how do you add that without changing how the compiler compiles itself? |
| 44 | [The last three, and what a macro can see](part-6-self-hosting/44-the-last-three.md) | How does a program bind a variable whose name it computes, and how can a macro know what the code around it means? |
| 45 | [Lists, loops and sequences](part-6-self-hosting/45-sequences.md) | How do you write fifty similar functions without repeating yourself or paying for an abstraction at run time? |
| 46 | [A little language for output](part-6-self-hosting/46-format.md) | How does a template with loops and conditionals in it fill itself in, and why parse it before running it? |
| 47 | [Numbers without a ceiling](part-6-self-hosting/47-bignums.md) | How does an integer grow past the machine word, and which half of that belongs in Rust? |
| 48 | [Numbers with a point](part-6-self-hosting/48-floats.md) | Where does a 64-bit float live in a 64-bit tagged word, and how does it become text and back exactly? |
| 49 | [Measuring against the standard](part-6-self-hosting/49-ansi-test.md) | How much of Common Lisp does tier 1 cover, and how do you count a test that cannot run yet? |
| 50 | [What the first measurement asked for](part-6-self-hosting/50-first-gaps.md) | Which small gaps block the most tests, and how does a dynamic variable declared in one place stay dynamic only there? |

Chapters 3 and 4 come before 5 and 6 because of the contracts-first rule: 3
explains what garbage collection needs, 4 settles the contract that follows from
it, and only then do 5 and 6 build the thing.

## What is not here yet

Still to come, and deliberately so:
- **The rest of the language.** All twenty-five of Common Lisp's special
  operators exist — twenty-two in tier 0, along with macros, lambda lists,
  keywords, vectors, strings, hash tables, `setf`, a minimal **condition
  system**, `defstruct` and multiple values; `progv`, `load-time-value` and
  `symbol-macrolet` in tier 1, compiled only (chapter 44); and the list,
  sequence and string functions (chapter 45); basic `format` (chapter
  46); integers of any size (chapter 47); and double floats (chapter 48).
  Tier 1 is measured against ansi-test (chapter 49); still to come is
  closing the gaps that measurement lists; then ratios
  (#59, tier undecided); beyond it, `loop`, CLOS and the rest, in later
  phases.
- **Bytecode**, which is two things. A **bytecode VM** in Rust that executes a
  compact instruction set faster than anything can walk a tree — and a
  **compiler**, written in Lisp, that translates source into those instructions.
  Both Rust pieces are deliberately dumb; all the judgement is in the Lisp one.
  This is phase 1; chapters 24 to 26 are the plan, the machine and the
  addressing, and chapter 12 is what makes it safe to attempt.
- **Self-hosting** — the moment the compiler compiles itself and the result is
  identical to what the interpreter produced. After that the Rust stops growing
  and every further feature is Lisp. See
  [The thing that builds itself](the-bootstrap.md).
- **The environment** — editor, debugger, windows, all of them Lisp objects
  inside the same running image.

That last one is what Quokka is actually for. Everything in Part I is the floor
it stands on.

## How to read this

In order, the first time. The chapters depend on each other and several of them
turn on a decision made two chapters earlier.

Each one opens with a short note saying where it fits. Code is real code from
the repository, sometimes shortened, never invented. Where something is not yet
built or not yet decided, the chapter says so rather than pretending — this book
is being written alongside the system, and some of it is genuinely unfinished.

And if a term goes past you, the [glossary](glossary.md) has every one of them in
a sentence.
