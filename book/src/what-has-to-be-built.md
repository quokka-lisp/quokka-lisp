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
the language; the language is the rules about what `(* 6 7)` means. The runtime
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

**1. The reader.** Turns text into structure. You type `(* 6 7)`, and the reader
produces something the machine can work with: a list containing the symbol
`*` and the numbers 6 and 7. In most languages this part is called a *parser*
and it is a large, fiddly thing. In Lisp it is unusually small, for a reason we
will come to.

**2. Values.** How a piece of data is represented in memory. The number 42, the
character `q`, a list and a function all have to be *some arrangement of bits*,
and something has to be able to tell which is which. This sounds like
plumbing. It turns out to constrain everything else.

**3. The heap.** A big region of memory where values that are too big to fit in
one machine word actually live. A list, a piece of text or an object
occupies space in the heap, and something has to hand that space out.

**4. The collector.** Memory is finite. A program running for an hour will
create millions of values it then stops needing. Something has to notice that
and reclaim the space. This is **garbage collection**: the system works out,
automatically and while the program runs, which values nobody can reach any more,
and takes their memory back. You never write "I am finished with this"; it is
deduced.

**5. The evaluator.** The part that actually runs your program. It takes the
structure the reader produced and *does* it: looks up what `*` means, works out
6 and 7, multiplies them.

Plus one more, which is not really a part but is how you meet the system:

**The REPL.** Read, Eval, Print, Loop. A prompt that reads what you type,
evaluates it, prints the result, and comes back for more. Lisp invented this in
1964 and it is now everywhere: a Python prompt, a browser console, a database
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
Others are *pervasive*: they appear in the signature of every function you will
ever write, and changing them later means rewriting everything.

The garbage collector is the first kind. It is a few hundred lines in one file,
and it can be replaced without anything outside noticing.

The rule about *how the rest of the code is allowed to touch the heap* is the
second kind. It determines what every allocation returns and what every function
takes. By the time there is a reader and an evaluator, it is in hundreds of
places.

So the pervasive decision has to be made first, even though it sounds like a
detail and the collector sounds like the main event. That is why chapter 4 of
this book is about a contract, and chapter 6, three chapters later, is the
collector itself.

The general lesson applies well beyond language runtimes. What you have to
decide early is settled by how entangled a decision is with everything else,
not by how sophisticated it is.

### And one more ordering, which is the whole strategy

There is a third kind of order at work, and it is strange enough to have its
own chapter. The compiler is written **in Lisp**, the language it compiles,
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
return, and not before. So a program that loops forever does not just loop; it
freezes the tab. No repainting, no clicks, no typing, until the browser offers
to kill the page.

Which means the evaluator must be able to **stop in the middle of a computation
and resume later**. And that turns out to be an enormous constraint, because the
natural way to write an evaluator, a function that calls itself for
sub-expressions, cannot stop. Its state lives on the host language's call
stack, and returning destroys it.

So the evaluator has to be built inside out: as a machine with explicit state
that takes one small step at a time. That is chapter 2, and it is the single
most consequential decision in the system. It also, unexpectedly, pays for
three other things, which is a recurring shape worth watching for.

## The map

Part I builds the substrate: everything underneath the evaluator. Part II turns
text into behaviour, and ends by asking whether any of it is correct. Together
they are **phase 0 and tier 0**, which are finished: `(fact 20)` runs, the
collector survives being run at every opportunity, and seventeen files of
Common Lisp agreed with a real one. Part III starts **phase 1**, where the
language stops being defined in Rust; its first chapters were written before
the work and say so at the top, and are left as written so that the later ones
can correct them. Part IV is the compiler, which is a Lisp program rather than
a piece of the runtime, a different kind of thing, and the place where most
of the decisions are still provisional. Part V is the image, where the system
learns to save itself to a file and start from one, and where two decisions
made in Part I for unrelated reasons turn out to be what makes it possible.

Phase 1's gate is met, in that an image saves, reloads and resumes a
computation it stopped in the middle of, and since chapter 32 the world inside that image is
compiled, with nothing interpreted left in it. Chapter 33 checks that the
compiled world agrees with the interpreted one on every test file, and with
that **phase 1 is finished**: the language is defined by a Lisp file compiled
by itself, and the interpreter is only what builds the first image. Part VI is
**phase 2**, self-hosting, and it opens as Part III did: with a plan written
before the work, kept so that what happens can correct it. Its gate falls in
the middle rather than at the end, at chapter 42, where the compiler is shown
to compile itself into exactly the same code; the chapters after it build the
rest of tier 1 and measure it against the standard's own test suite.
Part VII is **phase 3**, which builds towards a debugger in which a
function can be fixed from inside an error and the call retried. It starts
with objects: first a type system, then CLOS, the object system that the
condition system and every later tool dispatch through. Part VIII builds the
condition system on it: conditions that are objects, handlers that run where
the error happened, and restarts.

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
| 51 | [The rest of a symbol](part-6-self-hosting/51-symbol-cells.md) | What else does a symbol hold besides a value and a function, and how does a compiler stop a constant from being rebound? |
| 52 | [Printing for another Lisp to read](part-6-self-hosting/52-printing-for-another-lisp.md) | How do you print a list that contains itself, and how does a hash table key on an object the collector keeps moving? |
| 53 | [Compiling on request](part-6-self-hosting/53-compiling-on-request.md) | What is left for `compile` to do when everything is compiled already, and can a compiler change without changing how it compiles itself? |
| 54 | [When two packages disagree](part-6-self-hosting/54-packages-that-disagree.md) | What happens when two symbols of one name meet in a package, and how does a package choose in advance? |
| 55 | [Arrays, all of them](part-6-self-hosting/55-arrays.md) | How can an array grow and stay the same object, share another's elements, or have several dimensions, and what does Rust need to know about it? |
| 56 | [The small functions](part-6-self-hosting/56-small-functions.md) | How do you write twenty-three functions once, and why can't an oracle be trusted on the last bit of a logarithm? |
| 57 | [Bit fields](part-6-self-hosting/57-bit-fields.md) | How does an integer behave as an endless row of bits, and how can a `setf` place sit inside another place? |
| 58 | [The reader's last gaps](part-6-self-hosting/58-the-readers-last-gaps.md) | How does a readtable fold the case of names, how does the printer follow it, and where does a character's syntax type live? |
| 59 | [Hash tables, finished](part-6-self-hosting/59-hash-tables-finished.md) | How can a table find a key by its contents when Rust hashes it and Lisp compares it? |
| 60 | [Labels, limits and a counter](part-6-self-hosting/60-labels-and-limits.md) | How does a reader return an object that contains itself before the object is finished, and how do you measure a limit a program may rely on? |
| 61 | [Structures of their own](part-6-self-hosting/61-structures.md) | What does a record type need that a vector cannot give it, and how does one type extend another? |
| 62 | [What the sequence functions skipped](part-6-self-hosting/62-sequence-gaps.md) | How do you turn a list of failing tests into causes, and what does a standard ask for that a simpler implementation skips? |
| 63 | [The right error](part-6-self-hosting/63-the-right-error.md) | Why does the type of an error matter as much as the error, and how do you tell a loop from a budget that is too small? |
| 64 | [What a macro is given](part-6-self-hosting/64-what-a-macro-is-given.md) | What should a macro's expander receive, and what does a default form see when a parameter shadows a variable? |
| 65 | [An array that holds nothing](part-6-self-hosting/65-an-array-that-holds-nothing.md) | What is an array of element type `nil`, and what do you do when the standard and the oracle disagree? |
| 66 | [Functions named (setf name)](part-6-self-hosting/66-setf-functions.md) | How can a list be a function's name, and where do you keep such a function so that nothing else has to change? |
| 67 | [Lists that do not end in nil](part-6-self-hosting/67-list-gaps.md) | Which list functions must accept dotted and circular lists, and when are two hash tables `equalp`? |
| 68 | [What the compiler skipped](part-6-self-hosting/68-compiler-gaps.md) | Where do declarations go once a body has been rewritten, and why must `throw` look before it leaps? |
| 69 | [The last gaps](part-6-self-hosting/69-the-last-gaps.md) | How should a measurement count a test the oracle fails too, or one that cannot finish without a deferred feature? |
| 70 | [Ratios](part-6-self-hosting/70-ratios.md) | Why is `(/ 1 3)` neither 0 nor 0.333, and how does one number keep one representation? |
| 71 | [Phase 2 ends](part-6-self-hosting/71-phase-two-ends.md) | What does it mean that tier 1 is complete and measured, and what does it not mean? |
| 72 | [What is a type?](part-7-objects/72-what-is-a-type.md) | How can a program ask whether every object of one type is also of another? |
| 73 | [Classes](part-7-objects/73-classes.md) | How can a class be an object, and an instance of itself? |
| 74 | [Generic functions](part-7-objects/74-generic-functions.md) | How does a call choose its code by the classes of its arguments, and stay fast? |
| 75 | [Printing objects](part-7-objects/75-printing-objects.md) | How can a program decide how its own objects print, without slowing the printing of everything else? |
| 76 | [Changing classes](part-7-objects/76-changing-classes.md) | How do instances follow when their class is redefined, or when an instance is given another class? |
| 77 | [Method combination](part-7-objects/77-method-combination.md) | How can a generic function add up, collect or chain what its methods return, in a way the program defines? |
| 78 | [Looking inside objects](part-7-objects/78-looking-inside-objects.md) | How can a program ask a class or a generic function what it is made of, and how is a whole object system measured? |
| 79 | [Conditions are objects](part-8-conditions/79-conditions-are-objects.md) | How does an error become an object with slots and a report of its own, and what does a program define to have one? |
| 80 | [Handlers that do not unwind](part-8-conditions/80-handlers-that-do-not-unwind.md) | How does a handler run where the error happened, and decline, and how do two kinds of handler find each other on one stack? |
| 81 | [The machine's errors](part-8-conditions/81-the-machines-errors.md) | How does an error the machine finds become a condition that knows which value was wrong? |
| 82 | [Restarts](part-8-conditions/82-restarts.md) | How does the code that found an error offer ways out, and the code that knows what it wants choose one? |
| 83 | [Measuring conditions](part-8-conditions/83-measuring-conditions.md) | What did ansi-test find once `assert` and the condition system existed, and how was each failure explained? |
| 84 | [Definitions keep their source](part-9-definitions/84-definitions-keep-their-source.md) | How does a world remember what its definitions said, and which comments belong to each? |
| — | [The environment, on paper](looking-ahead/the-environment-on-paper.md) | What will a person see and use once phases 4 to 6 are built, and what changes when a program is a live database of definitions instead of a pile of files? |

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
  46); integers of any size (chapter 47); double floats (chapter 48); and
  ratios (chapter 70).
  Tier 1 is measured against ansi-test (chapter 49), and chapters 50 to 69
  close the gaps it listed: declarations, symbols' cells, printing readably
  and with shared structure, `compile`, the rest of the package system, every
  kind of array, which moved from tier 2 into tier 1, bit fields, the reader's
  readtable case, character names and syntax types, `equalp` hash tables and
  `sxhash`, reading `#1=` labels, `defstruct` with all its options, the
  condition types the standard names, what a macro's expander is given,
  arrays of element type `nil`, setf functions, dotted and circular
  lists, what the compiler skipped, and the standard condition types'
  lattice.
  Chapter 70 adds ratios, the last numbers of tier 1. Chapter 72 begins
  tier 2 with the type system, `typep` and `subtypep`, and chapter 73 adds
  classes and instances, chapter 74 generic functions, chapter 75
  printing objects through `print-object`, chapter 76 what happens to
  instances when their class changes, chapter 77 the other ways of
  combining a generic function's methods, and chapter 78 the metaobject
  protocol's readers and the measurement of CLOS as a whole. Chapter 79
  makes conditions instances of condition classes, and chapter 80 adds
  handlers that run where a condition is signalled, and chapter 81 sends
  the machine's own errors through them; chapter 82 adds restarts, and
  chapter 83 measures the whole. Chapter 84 begins Part IX, the world's
  definitions, which keep their source. Still to come are the
  rest of the condition system, `loop` and the rest, in phases 3 to 5.
- **Bytecode**, which is two things. A **bytecode VM** in Rust that executes a
  compact instruction set faster than anything can walk a tree, and a
  **compiler**, written in Lisp, that translates source into those instructions.
  Both Rust pieces are deliberately dumb; all the judgement is in the Lisp one.
  This is phase 1; chapters 24 to 26 are the plan, the machine and the
  addressing, and chapter 12 is what makes it safe to attempt.
- **Self-hosting**, the moment the compiler compiles itself and the result is
  identical to what the interpreter produced. After that the Rust nearly stops
  growing, and every further feature of the language is Lisp. See
  [The thing that builds itself](the-bootstrap.md).
- **The environment**: editor, debugger and windows, all of them Lisp objects
  inside the same running image. It is designed, not built: *The environment,
  on paper*, after Part VIII, describes what phases 4 to 6 aim at and why.

That last one is what Quokka is actually for. Everything in Part I is the floor
it stands on.

## How to read this

In order, the first time. The chapters depend on each other and several of them
turn on a decision made two chapters earlier.

Each one opens with a short note saying where it fits. Code is real code from
the repository, sometimes shortened, never invented. Where something is not yet
built or not yet decided, the chapter says so rather than pretending. This book
is being written alongside the system, and some of it is genuinely unfinished.

And if a term goes past you, the [glossary](glossary.md) has every one of them in
a sentence.
