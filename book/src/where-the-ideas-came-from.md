# Where the ideas came from

Nearly nothing in Quokka is new. The interesting work was choosing, and the
choosing is only checkable if the alternatives are named — so this page is the
book cut a different way: **by decision**, with the source that informed it,
and, where it matters more, what was deliberately *not* taken from it.

[Further reading](further-reading.md) is the other page: what to read, and
why. This one is what was borrowed.

Two reasons it exists.

A decision with a source attached can be argued with, because the source can
be read and the reasoning checked. A decision with no source is indistinguish-
able from a guess that happened to work, and this book's whole claim is that
the reasoning is the content.

And the second reason is smaller and more practical: **jargon that is named is
searchable.** "The trick where the closure has to work for every lifetime" is
a sentence you cannot look up. *Rank-2 branding, as in Haskell's `ST` monad*
is three words that lead somewhere.

---

## The substrate

| decision | where it came from | what was left |
| --- | --- | --- |
| One tagged word per value, type in the low bits | The Lisp machines — the Ivory processor's tagged memory in particular ([1](part-1-substrate/01-one-word-for-everything.md)) | Their hardware did the tag checks; ours are instructions, which is why the fixnum tag is a single zero bit |
| A copying collector, scan trailing free | Cheney, "A Nonrecursive List Compacting Algorithm" (*CACM* 13:11, 1970); Jones, Hosking & Moss, *The Garbage Collection Handbook* (2nd ed., 2023) ([6](part-1-substrate/06-cheneys-algorithm.md)) | Generational collection, until the environment allocates enough to want it |
| Holding a value across a collection is a **compile** error | Launchbury & Peyton Jones, "Lazy Functional State Threads" (*PLDI* 1994) — the `ST` monad's rank-2 trick, where a computation must work for *every* state thread and so can leak nothing out of one. In Rust the same idea is called **generativity**, or branded types; Yanovski et al., "GhostCell" (*ICFP* 2021), is the careful treatment ([4](part-1-substrate/04-rooting.md)) | A shadow stack of roots, which every real collector uses and which costs a runtime check instead of a type |
| Saving the whole world to a file | Smalltalk-80, Interlisp and Genera all did it; Goldberg & Robson, *Smalltalk-80: The Language and Its Implementation* (1983) ([7](part-1-substrate/07-saving-a-world.md)) | Smalltalk's *changes file* beside the image — not rejected, just not built; see [#24](https://github.com/quokka-lisp/quokka-lisp/issues/24) |
| Asking the host by leaving a request in the heap, not by calling it | The blocking system call — Tanenbaum & Bos, *Modern Operating Systems* (2014); completion queues — Axboe, *Efficient IO with io_uring* (2019); the Proactor pattern — Schmidt et al., *POSA 2* (2000) ([31](part-5-the-image/31-asking-the-outside-world.md)) | Rust `async` and callbacks, both rejected because neither can be saved in an image |
| Building the compiled world by compiling everything with the interpreted compiler, then running the results | Hart and Levin's Lisp compiler at MIT, *The New Compiler*, AI Memo 39 (1962) — the first compiler written in the language it compiles, brought up through the interpreter ([32](part-5-the-image/32-a-world-made-of-bytecode.md)) | A one-pass build, which cannot work while the compiler is interpreted Lisp calling the functions being replaced |
| Torture mode | The classical version collects on **every allocation** | Ours collects at every *mutation boundary*, because the bug the classical one hunts is unrepresentable here — the rooting discipline above makes it so. The same idea, adapted to a system where half of what it looks for cannot happen |

## Reading and running

| decision | where it came from | what was left |
| --- | --- | --- |
| An evaluator with explicit registers and heap frames | SICP §5.4's explicit-control evaluator ([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md), [9](part-2-reading-and-running/09-what-a-form-means.md)) | A recursive `eval`, which is shorter and cannot stop. The browser decided this, not taste |
| Two namespaces — a Lisp-2 | Common Lisp itself; Gabriel & Pitman, "Technical Issues of Separation in Function Cells and Value Cells" (1988) ([10](part-2-reading-and-running/10-functions-and-scope.md)) | Scheme's single namespace, which would have made `(flet ((list …)) …)` impossible to mean |
| Unhygienic macros with `gensym` | Graham, *On Lisp* (1993), chapters 7–10 ([15](part-2-reading-and-running/15-code-that-writes-code.md), [17](part-2-reading-and-running/17-why-macros-matter.md)) | Hygiene: Kohlbecker et al., "Hygienic Macro Expansion" (*LFP* 1986), and Flatt's "Binding as Sets of Scopes" (*POPL* 2016). Rejected for a reason, and chapter 17 is the argument |
| Conditions that are objects, signalled before unwinding | Pitman, "Exceptional Situations in Lisp" (1990); *CLtL2* chapter 29 ([20](part-2-reading-and-running/20-errors-you-can-catch.md)) | Restarts and `handler-bind` — the half that makes it a *condition* system rather than exceptions. Phase 3 |
| `%` for the implementation's own names | SBCL's `sb-impl::` and `sb-int::` ([22](part-2-reading-and-running/22-the-percent-names.md)) | The package system behind it, which is what makes theirs a mechanism and ours a convention |
| A second value register, `UNBOUND` for "just one" | *CLtL2* §7.10 argues against returning a list instead ([23](part-2-reading-and-running/23-more-than-one-answer.md)) | — |

## Compiling

| decision | where it came from | what was left |
| --- | --- | --- |
| Compiling as *moving work to a time that happens once* | SICP §4.1.7, which separates `analyze` from execution without introducing bytecode at all ([24](part-3-compiling/24-why-compile-at-all.md)) | — |
| A **stack** machine | Goldberg & Robson's Blue Book, part four; Queinnec, *Lisp in Small Pieces*, chapter 7 ([25](part-3-compiling/25-a-machine-made-of-instructions.md)) | A register machine — and this is the clearest case of going *against* a source. Ierusalimschy, de Figueiredo & Celes, "The Implementation of Lua 5.0" (2005), is Lua moving from a stack machine to a register machine and getting substantially faster. We read it and stayed, because the cost is a register allocator and rule 7's budget is 15,000 lines |
| Packed bytes, **operand widths fixed per opcode** | The JVM and CPython both pack ([24](part-3-compiling/24-why-compile-at-all.md), `docs/bytecode.md`) | CPython's variable-width operands and `EXTENDED_ARG`. Variable widths make an instruction's size depend on its operands, which makes jump distances depend on sizes, which makes an assembler iterate to a fixpoint — *branch relaxation*. Fixing the widths avoids it entirely. Lua's fixed 32-bit instruction word was the other option and lost on size |
| `max-stack` in a compiled function | The JVM class file's `max_stack`, checked once at call time instead of bounds-checking every push | — |
| Ceilings are compiler errors — 64 KB of code per function | The JVM's identical per-method limit, for the identical reason | — |
| A **verifier**: one pass at load, checking stack discipline | Lindholm & Yellin, *The Java Virtual Machine Specification*, §4.10 ([27](part-3-compiling/27-leaving-early-again.md)) | Built, after two bugs argued for it. It checks stack discipline and jump targets, not types |
| Lexical addressing — `(depth, index)` instead of a name | Queinnec, *Lisp in Small Pieces*, chapter 5 ([26](part-3-compiling/26-where-is-the-variable.md)) | — |
| Keeping the environment chain | — | Flat closures and stack-allocated locals: Dybvig, *Three Implementation Models for Scheme* (1987). Deferred rather than rejected, with the cost written down: the instruction set changes when they arrive |
| Not adding `LREF0`, `CONST8`, `CALL1` | The JVM's `iload_0` and CPython's `LOAD_FAST` are exactly these | Deliberately not taken. Each is a second spelling of an instruction that already exists, and rule 7 says the readable version goes first |
| Speed later from dispatch and caching, not a JIT | Ertl & Gregg, "The Structure and Performance of Efficient Interpreters" (2003); Deutsch & Schiffman, "Efficient Implementation of the Smalltalk-80 System" (*POPL* 1984), for inline caches | A JIT, which is a second compiler per processor |
| One evaluator at a time: the interpreter bootstraps, then goes cold | SBCL, where `sb-ext:*evaluator-mode*` is `:compile` and the full interpreter is off by default ([24](part-3-compiling/24-why-compile-at-all.md)) | A mixed image, which would have bought an earlier running system at the price of a boundary every unwinding path had to understand |
| Literals are read-only | CLHS 3.7.1 leaves modifying one undefined; SBCL puts constants in a read-only space and signals ([#26](https://github.com/quokka-lisp/quokka-lisp/issues/26)) | Treating "undefined" as permission to share without protecting, which is legal and makes the resulting bug undebuggable |
| Inlining `+` but not a user's function | CLHS 11.1.2.1.2, which makes redefining a `COMMON-LISP` symbol undefined — the package-lock rule, read as a licence ([25](part-3-compiling/25-a-machine-made-of-instructions.md)) | — |
| Sharing identical literals in an image | **Interning**, or *hash consing* in general: Ershov (1958); Goto, "Monocopy and associative algorithms in extended Lisp" (1974) | Not built. A general run-time intern table needs weak references and collides with rule 9 |

## The shape of the project

| decision | where it came from |
| --- | --- |
| A whole system small enough for one person to read | **Project Oberon** — Wirth got an operating system, a compiler, an editor and a CPU into a book you can read and then build on an FPGA. The standing rebuke to a 15,000-line budget for an interpreter alone |
| Tiled windows, not overlapping ones | Oberon's `Viewers.Mod` and `MenuViewers.Mod`, which did it in 1988 and then waited two decades for Linux to catch up ([#21](https://github.com/quokka-lisp/quokka-lisp/issues/21)) |
| Everything changeable from inside, live | Smalltalk |
| Presentations, commands, a system you work *inside* | The Lisp machines, Genera in particular |
| A book written alongside the system rather than after it | Tanenbaum's operating systems books, which explain a real system by building one |

---

## Where we went against a source, on purpose

The five worth knowing, because a book that only cites agreement is a reading
list rather than an argument.

1. **Lua moved from a stack machine to a register machine and got faster. We
   stayed with a stack.** Their gain costs a register allocator; our budget is
   15,000 lines and the comparison is against a tree-walker a hundred times
   slower than either.
2. **CPython and the JVM use variable-width operands. We fixed them per
   opcode.** Smaller code, at the price of an assembler that iterates to a
   fixpoint. We wanted the one-pass assembler more than the bytes.
3. **Scheme solved variable capture with hygiene. We took `gensym`.** Chapter
   17 argues it; the short version is that the useful cases hygiene makes
   awkward are cases Common Lisp programmers actually write.
4. **The classical GC torture mode collects on every allocation. Ours collects
   at every mutation boundary.** Not a weakening: the rooting discipline makes
   the bug the classical version hunts unrepresentable, so what is left to
   hunt is the collector's own mistakes.
5. **The JVM verifies bytecode before running it.** So do we, now — this
   entry was "we do not, yet, and it is a gap rather than a decision" until
   two bugs of exactly the shape a verifier catches made the case. Left in the
   list because what it does *not* check is the part worth remembering: it
   proves a function cannot corrupt the machine, and says nothing about
   whether it is correct.

## A note on the early chapters

Chapters 15 onwards each end with a **References** paragraph. Chapters 1 to 14
do not: they cite inline where it matters — SICP in chapter 2, Cheney in
chapters 3 and 6, Ivory in chapter 1 — and the convention of a closing
paragraph started later.

This page is partly a repair of that, and the sources above for Part I are the
ones those chapters should have named. Giving them their own paragraphs is
outstanding work, not a decision.
