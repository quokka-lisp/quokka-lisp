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
they are **phase 0**, which is finished: `(fact 20)` runs, the collector
survives being run at every opportunity, and a hundred forms agree with a real
Common Lisp. Each chapter answers one question.

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

Chapters 3 and 4 come before 5 and 6 because of the contracts-first rule: 3
explains what garbage collection needs, 4 settles the contract that follows from
it, and only then do 5 and 6 build the thing.

## What is not here yet

Still to come, and deliberately so:
- **The rest of the language** — most of Common Lisp's 25 special operators,
  `&rest` parameters, and **macros**, which are what let a program extend its
  own language.
- **Bytecode**, which is two things. A **bytecode VM** in Rust that executes a
  compact instruction set faster than anything can walk a tree — and a
  **compiler**, written in Lisp, that translates source into those instructions.
  The VM is hardware; the compiler is something a user might want to change, so
  the line between them falls exactly where rule 1 puts it. This is phase 1, and
  chapter 12 is what makes it safe to attempt.
- **Self-hosting** — the moment the compiler, written in Lisp, can compile
  itself. The system stops needing anything outside it to be rebuilt.
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
