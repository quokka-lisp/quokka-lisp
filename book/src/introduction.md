# Introduction

<p align="center">
  <img src="logo-horizontal.png" alt="Quokka Lisp" width="520">
</p>


This book is being written while the system it describes is being built. There
is no finished thing to look back on and tidy up. Each chapter comes out of a
question that had to be answered before the next commit could happen, and it is
written while the answer is still fresh — including the parts that turned out to
be wrong.

## What Quokka is

Quokka is one live Lisp world: a small virtual machine written in Rust, running
a subset of Common Lisp, with the editor, the debugger, the window system and
your own code all living as objects in the same image. It runs in a browser
through WebAssembly, and natively inside a desktop shell. It takes its soul from
the Lisp machines of the 1980s, its malleability from Smalltalk, and its
insistence on being small from Oberon.

The design is in [`docs/design.md`](https://github.com/quokka-lisp/quokka-lisp/blob/main/docs/design.md).
This book is not that document. The design says what Quokka is; this book says
*why*, and what it was like to find out.

## Who it is for

Someone who can program, and who has never built a language.

That is not a rhetorical position — it is literally the situation this was
written in. The author has written a great deal of software and has never
designed a compiler, an interpreter or a runtime. Thirty years ago there was
some Scheme at university, and a small Lisp interpreter written in Modula-2 as a
student project. That is the starting point.

So nothing here assumes you know what a tagged word is, or a root, or why a
garbage collector needs to be told where to start looking. Where a technique has
a standard name, the name is given and so is a reference, because jargon you can
search for is a door and jargon you cannot is a wall.

## What makes it different from a textbook

A textbook presents a finished subject in the order that makes it easiest to
learn. This book cannot do that, because the subject is not finished. What it
can do instead is show the shape of the decisions:

- A constraint that looks like a nuisance turns out to buy three other things.
  The rule that the interpreter must be able to stop and hand control back to
  the browser is the single most expensive decision in the system, and it pays
  for the debugger, for garbage collection, and for saving a running world to a
  file.
- A decision that looks central turns out to be local and replaceable, while one
  that looks mundane turns out to be in every line you will ever write. The
  garbage collector is the first kind. The rule about how you hand it a value to
  look after is the second.
- Things get caught by tests that were written for another reason entirely.

Where a decision is still open, the book says so rather than pretending. Open
decisions live as issues labelled `decision` in the repository, with the
argument in the comments, and the outcome comes back here when it is settled.

## How to read it

**Start with [What has to be built](what-has-to-be-built.md).** It is the map:
what the parts of a language runtime are, what each is for, and why they arrive
in the order they do. If you have never thought about this before, that chapter
is the one that makes the rest navigable, and it assumes nothing at all.

Then Part I in order. The chapters depend on each other, and several turn on a
decision made two chapters earlier. Each opens with a short note saying where it
fits in the whole.

The [glossary](glossary.md) has every unfamiliar term in a sentence, with the
chapter that explains it properly. Nothing in this book is a word you are
expected to already know.

Code is real code from the repository, not pseudocode, though it is sometimes
shortened. Where a chapter describes something not yet built or not yet decided,
it says so.
