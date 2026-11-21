# Further reading

Every chapter ends with references to the specific things it leans on. This
page is the other kind: the handful of sources you would want open in a tab
while reading the whole book, and the ones Quokka itself is checked against.

## The standard, and where to read it

Quokka is a Common Lisp subset. Rule 6 says that where it implements a CL
feature the behaviour must match, which makes the standard something stronger
than background reading — it is the specification the differential tests
(chapter 12) measure against. When Quokka and SBCL disagree, the first
question is never "which looks right" but "what does the standard say", and
several times in this book the answer has been *neither of you is wrong, that
is unspecified*.

### [common-lisp.net/documentation](https://common-lisp.net/documentation)

The hub. Books, the standard, the libraries, and — usefully — an opinion
about *which* rendering of the standard to read. If you want one link, this
is it.

### [The Common Lisp Metaspec](https://metaspec.dev/)

The draft standard (X3J13/94-101R) rendered from its **original TeX sources**,
with the errata, addenda and X3J13 cleanup issues included and the editors'
margin notes kept.

It is also published **as s-expressions** — a document format of
`(node-type property-plist children*)` nodes, with the provenance of the
original text preserved and every mechanical deviation recorded as an erratum
([source](https://codeberg.org/dlowe/metaspectre)). Which means the
specification is not only a document: it is **data a Lisp program can read**.
That is a fact worth filing away; it comes back below.

Two other free renderings of the same draft, if you prefer their presentation:
the [Common Lisp Community Spec](https://cl-community-spec.github.io/pages/index.html)
and [Novaspec](https://novaspec.org/).

### [The Common Lisp Documentation Project](https://lisp-docs.github.io/)

An open-source, **MIT-licensed** effort to gather the standard, a technical
reference, a tutorial and how-to guides in one place. The friendliest of
these if you are learning the language rather than implementing it.

### [The Common Lisp Cookbook](https://lispcookbook.github.io/cl-cookbook/)

Task-shaped rather than specification-shaped: *how do I do this*, with worked
code. Where the standard tells you what `handler-case` means, the Cookbook
shows you what people actually write.

That makes it useful for a question the standard cannot answer, and which
comes up constantly when building a *subset*: **what does idiomatic Common
Lisp look like, and therefore what should the prelude support?** The pages
worth reading before writing the corresponding part of Quokka:

| | |
| --- | --- |
| [Data structures](https://lispcookbook.github.io/cl-cookbook/data-structures.html) · [Strings](https://lispcookbook.github.io/cl-cookbook/strings.html) · [Arrays](https://lispcookbook.github.io/cl-cookbook/arrays.html) | what chapter 19 built, and the sequence functions it did not |
| [Error handling](https://lispcookbook.github.io/cl-cookbook/error_handling.html) | restarts and `handler-bind`, which chapter 20 left for phase 3 |
| [Macros and backquote](https://lispcookbook.github.io/cl-cookbook/macros.html) | chapters 15 and 17, from a user's side rather than an implementer's |
| [Equality](https://lispcookbook.github.io/cl-cookbook/equality.html) | `eq`, `eql`, `equal`, `equalp` — Quokka has one of the four, and this is the clearest account of what the others are for |
| [Iteration](https://lispcookbook.github.io/cl-cookbook/iteration.html) | `loop`, `dolist`, `do` — all outside tier 0 by rule 5, all eventually prelude macros |

#### The chapters that look irrelevant, and are not

It is tempting to write off the Cookbook's "Editor support" and "Outside
world" chapters — Quicklisp, Emacs, databases, foreign function interfaces —
as things Quokka will never have. That is wrong, and the way it is wrong is
worth the paragraph.

Every one of those chapters assumes the same thing: that **Lisp is a process on
a Unix box, talking to other processes**. The editor is a different program.
The database is a different program. The library archive is a directory on
disk. Common Lisp grew up in that world and its tooling is shaped by it.

Quokka is not a process. It is a *world* — one image holding the VM, the
language, the editor and the tools, in the Lisp machine and Smalltalk tradition
(see [The thing that builds itself](the-bootstrap.md) and `docs/design.md`).
So those chapters do not transfer as instructions. But each one names a
capability Quokka still owes its users, and reading it tells you what that
capability has to *do* — which is the harder half, and independent of
architecture.

| Chapter | What it assumes | Quokka's answer |
| --- | --- | --- |
| [Editor support](https://lispcookbook.github.io/cl-cookbook/editor-support.html) | the editor is another program, talking to Lisp over a socket | the editor is Lisp objects inside the image — phases 4 to 6, and `docs/design.md`'s "Environment and UI" |
| [Databases](https://lispcookbook.github.io/cl-cookbook/databases.html) | the database is another process you connect to | objects stay live in the image; persistence is a property of the image, not a server |
| [Quicklisp](https://lispcookbook.github.io/cl-cookbook/getting-started.html) | libraries are downloaded to disk, built, and loaded into a session | code lives on file hosts and is loaded into a world that is then saved |
| [Foreign function interfaces](https://lispcookbook.github.io/cl-cookbook/ffi.html) | the foreign world is C, reached by loading a shared library | the foreign world is the browser; the general problem is open ([#19](https://github.com/quokka-lisp/quokka-lisp/issues/19)) |

Three of those deserve more than a table row.

**The editor.** Emacs and SLIME (*Superior Lisp Interaction Mode for Emacs*,
whose Lisp-side server is called Swank) exist **because** the editor is outside
the Lisp. Every feature in SLIME is a way of reaching across that boundary:
evaluate the form at point, jump to a definition, show a function's arguments
while you type it, macroexpand in place, inspect the last value, find callers.

Quokka has no boundary to reach across, so none of SLIME's *machinery* applies.
Its *list* applies completely — it is thirty years of people finding out what a
Lisp environment must do, and it is the specification for phase 4 whether or not
anybody writes it down as one. The Cookbook even carries the counterexample
itself: its LispWorks IDE page describes an IDE written in Common Lisp and
running in the same image as your program, which is Quokka's model, not Emacs's.

**The database.** The Cookbook's own taxonomy has four categories, and the
third is *persistent object databases* — libraries where objects simply stay
live and a transaction log lets them come back after a restart. That technique
has a name, **prevalence** (from the Prevayler project), and it is the
Quokka-shaped answer: a log of changes plus periodic snapshots, instead of a
separate process you serialise into and out of.

Which is worth noticing, because Quokka has **half of it already**. Saving an
image is a snapshot. What `docs/design.md` does not yet describe is a log of
what happened since the last one — the difference between losing the work since
your last save and losing nothing. A user who wanted a database in Quokka would
not connect to one; they would define structures and rely on the image, and the
quality of that experience is a VM decision nobody has made yet ([#24](https://github.com/quokka-lisp/quokka-lisp/issues/24)).

**Quicklisp** is Common Lisp's library manager: an index of several thousand
libraries plus a downloader, so that `(ql:quickload "mito")` fetches a library
and everything it depends on and loads it into the running Lisp. It sits on top
of ASDF (*Another System Definition Facility*), which is the build system that
knows how to compile and load a set of files in dependency order.

Two things about it survive the change of architecture. The first is that
loading is **into a live image**, not into a binary — `quickload` is something
you do to a running system, which is already Quokka's shape. The second is the
design choice that made Quicklisp trusted: a release is a **curated snapshot**,
a set of library versions known to build together, rather than "the latest of
everything". Any eventual Quokka equivalent will face that decision, and
Quicklisp is the evidence about which way to go.

So: not things Quokka will never have. Things Quokka will have to answer
differently, from a reference that is honest about what the question is.

**Licence**: BSD-style. Redistribution and use, with or without modification,
provided the copyright notice, the condition and the disclaimer are reproduced
with it. Permissive and compatible with rule 8 — with an obligation attached,
which matters below.

| | |
| --- | --- |
| [Technical reference](https://lisp-docs.github.io/cl-language-reference/) | the standard's text with community-written explanations and examples |
| [Tutorial](https://lisp-docs.github.io/docs/tutorial) | Common Lisp from the beginning, for someone who has not written any |
| [How-to guides](https://lisp-docs.github.io/docs/howto) | task-shaped: how to do a particular thing |
| dpANS3R+ | the draft ANSI Common Lisp was made from, freely available, linked from the site |

Two things make this the right first stop for a reader of *this* book.

The first is the **licence**. It is MIT, which is Quokka's licence (rule 8), so
its text and examples can be used here without a lawyer. The HyperSpec's
cannot — it is derived from the published standard under permission that does
not extend to copying it into other projects. That distinction has had
practical consequences for how Quokka's tests are written; see below.

The second is that the technical reference carries **examples**. The standard
itself is a specification, written to be unambiguous rather than teachable,
and it frequently says something precise that is hard to picture. A worked
example beside it is often the difference between reading a paragraph and
understanding it.

### [The Common Lisp HyperSpec](http://www.lispworks.com/documentation/HyperSpec/Front/index.htm)

The hyperlinked edition that most books, posts and tools point to, hosted by
LispWorks. Cited throughout this book as **CLHS** with a section number —
`CLHS 3.4.1` for ordinary lambda lists, `CLHS 9` for conditions.

It is **not free**. LispWorks permits only complete, unmodified,
non-commercial copies and no derivative works, which means it can be linked
and cited here, as this book does constantly, but not quoted into the book or
into the tests. That restriction is the practical reason the free renderings
above matter: they are the ones whose text could be *used* rather than only
pointed at.

The published standard is **ANSI INCITS 226-1994**, which is neither free nor
online. Its final draft is, and is what every rendering above is made from.

## On using documentation as test cases

An obvious idea, and worth stating what Quokka does and does not do with it.

`tests/lisp/` holds Common Lisp files that run three ways — Quokka natively,
Quokka under WebAssembly, and SBCL — and all three must agree (chapter 12).
Any example from the standard or the reference is, in principle, a ready-made
test case: it is a form with a documented result, written by people who
understood the specification better than we do.

In practice Quokka's tests are **written here, from the specification's
claims, rather than copied from anybody's examples**. Three reasons, and only
the first is about licensing:

1. **Provenance.** The free references here are permissive but not identical:
   lisp-docs is MIT, the Cookbook is BSD-style and wants its notice and
   disclaimer carried along, and the HyperSpec permits no derivative works at
   all. Borrowing means tracking which line came from where, and carrying the
   right notice for each, forever. Writing them here keeps the question from
   arising — not because borrowing is forbidden, but because the bookkeeping
   buys the tests nothing.
2. **A test file has a job the examples do not.** Look at any file in
   `tests/lisp/`: the comments explain *why a case is there*, and several say
   what must not be tested and why — `(make-array 3)` with no
   `:initial-element` is unspecified, a structure prints differently in every
   implementation, `%`-prefixed primitives have no SBCL counterpart at all. A
   borrowed example cannot carry that, because it was written to illustrate a
   feature rather than to pin an implementation's behaviour.
3. **The interesting cases are not in the examples.** The bugs this book
   records were found by *leftmost keyword wins*, by a default form that must
   not be evaluated when an argument was supplied, by `unwind-protect` losing
   a second value during a `throw`. Documentation examples show a feature
   working; differential tests are for the corners where two implementations
   might quietly differ.

What the documentation **is** used for, constantly, is deciding what a test
should expect — which is the harder half. Writing `(handler-case (car 5)
(type-error () 'caught))` requires knowing that `car` of a non-list signals a
`type-error` specifically, and that is a fact to look up rather than guess.

### The one that might change this

Everything above argues against *copying prose*. The Metaspec's s-expression
form is a different proposition, because it is not prose: it is the
specification as a tree a program can walk, with every defined name, its
section, its argument list and its description in known positions.

That makes some things possible that hand-written tests cannot do at all:

- **Coverage, mechanically.** Walk the dictionary entries for, say, the Conses
  chapter and list which defined names Quokka has and which it does not. Today
  that list lives in issue comments and in somebody's memory, and chapter 22
  found dead code precisely because an inventory had to be written down.
- **Finding the corners.** The parts of the standard that mark behaviour as
  *undefined* or *implementation-dependent* are exactly what
  `tests/lisp/README.md` says must never be tested. Those markers are in the
  text, and therefore in the tree.

Neither is a small afternoon, and neither is on the roadmap. But "the
specification is available as data" is the kind of fact that is worth knowing
some time before you need it, which is why it is written down here rather than
rediscovered later.

## Books this one leans on

Named here because they come up repeatedly, with the chapters that use them.

| | |
| --- | --- |
| Abelson & Sussman, *Structure and Interpretation of Computer Programs* | §5.4's explicit-control evaluator is the shape of chapter 9 |
| Queinnec, *Lisp in Small Pieces* (1996) | interpreters and compilers for Lisp, as a sequence of increasingly honest designs — chapters 9, 15, 16 |
| Graham, *On Lisp* (1993) | macros, chapters 7–10; chapter 9 is entirely about variable capture — chapters 15, 17, 21 |
| Jones, Hosking & Moss, *The Garbage Collection Handbook* (2nd ed., 2023) | chapters 3–6, and chapter 11 on how a collector knows which words are references |
| Steele, *Common Lisp the Language*, 2nd edition (1990) | predates the standard and is wrong in places, but explains *why* where the standard only says *what* — especially its chapter 29 on conditions |
| Pitman, "Exceptional Situations in Lisp" (1990) | why conditions are not exceptions — chapter 20 |

## Systems worth reading

### [SBCL](https://www.sbcl.org/)

The oracle (chapter 12). Reading its source is also instructive: the `%`
convention from chapter 22 is `sb-impl::` there, with a package system behind
it rather than a naming convention.
### [Project Oberon](https://people.inf.ethz.ch/wirth/ProjectOberon/index.html)

Not a Lisp, and the closest thing this project has to a precedent. The
**complete revised edition (2013)** is free on Wirth's own site, and it is
worth being precise about what "complete" means:

| | |
| --- | --- |
| [The System](https://people.inf.ethz.ch/wirth/ProjectOberon/PO.System.pdf) | chapters 1–9: the operating system |
| [Applications](https://people.inf.ethz.ch/wirth/ProjectOberon/PO.Applications.pdf) | chapters 10–15: editor, graphics, networking |
| [The Computer](https://people.inf.ethz.ch/wirth/ProjectOberon/PO.Computer.pdf) | chapters 16–17: the RISC5 **processor** |
| Sources | every module of the system, as plain text |
| Verilog | the CPU itself — `RISC5.v`, `Divider.v`, `FPAdder.v`, `VID.v` |

There is no layer of that system which is not in the book. The compiler is in
there, the file system is in there, the window manager is in there, and so is
the gate-level design of the processor it all runs on. You can read the whole
thing, and then build it on an FPGA.

That is the claim Quokka's second principle makes — *the whole system should
be readable by one person* — and Oberon is the existence proof. It is also a
standing rebuke: Wirth got an operating system, a compiler, an editor and a
CPU into something a person can read, and this project has a 15,000-line
budget for a Lisp interpreter alone.

**The licence is ISC-style** — use, copy, modify and distribute, with the
notice preserved. Compatible with rule 8, which makes it unusual among the
references on this page: the HyperSpec can only be cited, but Oberon's actual
code could be adapted with attribution if it were ever the right thing to do.

Four modules are worth reading before the corresponding part of Quokka gets
built:

| | |
| --- | --- |
| `Viewers.Mod`, `MenuViewers.Mod` | the tiled viewer system — the direct source of issue #21 |
| `TextFrames.Mod` | text as the interface, which is what "live text" means in `docs/design.md` |
| `ORS`, `ORB`, `ORG`, `ORP.Mod` | a complete compiler in four modules: scanner, symbol table, code generator, parser. Phase 1's shape, by somebody who had done it before |
| `Kernel.Mod` | memory, allocation and the heap, in about a page |
