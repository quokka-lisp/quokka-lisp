# The fixpoint

> **Where this fits.** *The bootstrap* described the check a self-hosting
> system can make on itself. Chapter 34 planned for it, and chapter 41 built
> the rebuild that makes it possible: C1 by the cold build, C2 by C1 rebuilding
> itself, C3 by C2. This chapter compares them. It is phase 2's gate:
> *the compiled compiler rebuilds itself and the core library, and the rebuilt
> compiler matches the first build byte for byte.* What it unlocks is a
> standing test that every later change to the compiler, the prelude or the
> library has to pass, for the rest of the project.

*Status: done, issue #50. The gate passes, and stays as a test. Phase 2 goes
on: its scope is all of tier 1.*

## Two questions, three generations

Three worlds give two comparisons, and they ask different questions.

**C1 against C2: do the two compilers agree?** C1 was compiled by the
*interpreted* compiler: `compile.lisp` running on the Rust tree-walker. C2 was
compiled by the *compiled* compiler: the same source, compiled to bytecode and
running on the VM. Two very different machines ran the same program. If they
produce the same output, the interpreter and the VM agree on everything the
compiler does, which is most of the language. *The bootstrap* noted that
compiling a compiler with a *different* compiler and comparing is the known
defence against a compiler hiding something in itself. That technique has a
name, **diverse double-compiling** (Wheeler, 2005), and C1 against C2 is a
small instance of it.

**C2 against C3: is the compiler a fixed point?** Both were compiled by a
compiled compiler: C2 by C1's, and C3 by C2's. If C2 = C3, then compiling the
compiler changes nothing any more. The output is a compiler that produces
itself. That is the **fixpoint** proper.

## What "the same" means

The gate says *byte for byte*. Two saved images can't simply be compared with
`cmp`, and the reason why is worth knowing.

Some things differ between generations **by design**:

- **Gensym names.** A macro that needs a temporary variable makes one with
  `gensym`, which names it `G` followed by a counter: `G16464`. The counter only
  goes up, and C2 is compiled later than C1, so the same variable is `G16464`
  in one generation and `G18048` in the next. The name is not something the
  compiler decided. The variable's *role* is.
- **The header.** An image records how many times its world has been saved
  (the sequence number), the gensym counter, and a checksum over everything,
  which therefore covers the gensym names too.

Everything else is the compiler's business, and has to match.

That led Carsten to choose two comparisons:

**Every named function, across all three generations.** Test code walks each
world's symbols and describes every compiled function: its code bytes,
parameters, stack depth, name and constants, recursively, with symbols written
out with their package. Gensyms are numbered by the order in which they first
appear, so `G16464` and `G18048` both become `#:G0` when they play the same
part:

```text
COMMON-LISP::ABS
  Closure[(COMMON-LISP::NUMBER) fn[COMMON-LISP::LAMBDA params=(COMMON-LISP::NUMBER)
    stack=3 code=0a0000050000020000…0012 constants=Vector[COMMON-LISP::< COMMON-LISP::-]] …]
```

**The whole world, C2 against C3, word by word.** Compare the image files
themselves, and allow only the gensym names to differ, as long as they
correspond one to one. On top of that, the header's checksum, sequence and
gensym counter may differ. This sees what the first comparison cannot: code
held in *data* rather than in a function cell, such as the readtable's reader
macros; every symbol; and the layout of the heap itself.

## The result

| | named compiled functions | differing |
| --- | --- | --- |
| C1 against C2 | 360 and 360 | **0** |
| C2 against C3 | 360 and 360 | **0** |

And C2 against C3 as whole worlds: identical, apart from 118 gensym names and
the three header words. Every other word of a 45,000-word heap is equal.

It passed the first time it ran. The work that made it pass was done in the
steps before:
- The reader in Lisp had to expand backquote exactly as the Rust reader does
  (chapter 38).
- Every `eval`'d `defun` had been capturing an empty scope the cold build
  didn't make (chapter 41).
- `compile-form` had to become re-entrant (chapter 40).

Each was found and fixed because it made two generations differ, before
anything compared them formally.

## A worry that did not apply

Chapter 34 expected trouble from comparing whole images: they *"depend on
things that are not the compiler's business, such as the order objects were
allocated in."* C2 and C3 were certainly allocated in different orders. C3
started from a world that had already been rebuilt once, and its heap had
been through different collections.

It didn't matter, because of something decided in phase 1: **saving
collects**. Cheney's algorithm (chapter 6) copies the live objects in the
order it *reaches* them from the roots, breadth first, not the order they were
made. Two worlds with the same object graph, reached from the same roots,
come out of a collection laid out identically, whatever happened to them
before. A test in `image.rs` has said so since phase 1, *"so the same world
saved twice gives the same bytes, which phase 2's fixpoint test needs"*, and
here it is, needed.

## A comparison that cannot pass by comparing nothing

A check that passes first time deserves suspicion. Perhaps the descriptions
are empty, or the comparison looks at the wrong thing. So there is a test for
the test:

```lisp
(defun abs (n) (if (minusp n) (- 0 n) n))
```

Evaluated in C2, this changes one function. The function comparison must then
report exactly one difference, `COMMON-LISP::ABS`, and the whole-world
comparison must fail. Both do.

The whole-world comparison also has to know *which* header words may differ,
and it would be easy to allow the wrong ones. So it checks its own
assumptions against the running world. The sequence word must equal what the
loaded heap reports. The gensym word must predict the name of the next gensym.

Writing that check went wrong twice, and both times the check was right:

1. The first attempt evaluated `(symbol-name (gensym))` and got a name one
   higher than expected, because *compiling* that form made a gensym of its
   own first. So the check now calls `gensym` directly, through the
   trampoline (chapter 32), compiling nothing.
2. It was still one higher. The counter holds the *last* number issued, not
   the next.

Small things, but exactly what a check against reality is for.

## What it costs to keep

The comparisons are **test code**, with no Rust in the system and nothing in
the Lisp. They share the C2 and C3 that the rebuild tests build anyway, so the
gate adds seconds, not rebuilds, to every run. When it fails, it writes the
three generations' descriptions to `target/fixpoint/c1.txt`, `c2.txt` and
`c3.txt`, so the answer to *what changed?* is a `diff` away.

From now on, a change that makes the compiler compile itself differently
fails the build. Chapter 34 gives one way that could happen: a tier-1 `setf`
that expands differently from the prelude's. When it does, the fix might be in
the change, or the change might be right and the comparison need to learn
something new. Either way the difference has to be understood, not waved
through.

## What is not here

- **The rest of phase 2.** The phase's scope is all of tier 1. Still to come:
  `setf` expanders, the last three special operators, sequences, `format`,
  bignums and floats, and the ansi-test harness.
- **C1 against C2 as whole worlds.** C2 carries what running the rebuild left
  behind: the REPL's input line, and two symbols its file requests made. C1
  never ran a rebuild. The two are compared function by function.
- **Literally identical files.** Resetting the gensym counter at the start of
  a rebuild would make C2 and C3 differ only in sequence and checksum. That is
  cosmetic over what is checked, and it would need the counter reachable from
  Lisp (CL's `*gensym-counter*`).

---

**References.** *The bootstrap*, earlier in this book, for the check
described before it existed. Ken Thompson, *Reflections on Trusting Trust*
(1984). David A. Wheeler, *Countering Trusting Trust through Diverse
Double-Compiling* (2005). The GCC manual on `make bootstrap` and `make
compare`, which compares stage 2 and stage 3 object files and allows exactly
such things as timestamps to differ. Jones, Hosking and Moss, *The Garbage
Collection Handbook*, on Cheney's breadth-first copying order.
