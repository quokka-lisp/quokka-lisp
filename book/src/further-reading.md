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

1. **Provenance.** MIT text can be reused with attribution, and HyperSpec text
   cannot. Mixing sources in one directory means tracking which line came from
   where, forever. Writing them here keeps the question from arising.
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

## The implementations

- **[SBCL](https://www.sbcl.org/)** — the oracle (chapter 12). Reading its
  source is also instructive: the `%` convention in chapter 22 is
  `sb-impl::` there, with a package system behind it.
- **[Project Oberon](https://people.inf.ethz.ch/wirth/ProjectOberon/)** —
  Wirth and Gutknecht's whole system, documented end to end. Not a Lisp, and
  the direct ancestor of what Quokka wants to be when it grows up
  (`docs/design.md`, and issue #21 on tiling).
