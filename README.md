<p align="center">
  <img src="assets/brand/01_Logos/logo-horizontal-transparent-1440x360.png" alt="Quokka Lisp" width="560">
</p>

# Quokka

> A modern Lisp-machine-style environment: a small Rust VM running a Common
> Lisp subset, browser-first via WASM, with a live, presentation-based UI.

Quokka is one live Lisp world, with the VM, the language, the editor, the UI
and the tools in a single image, small enough for one person to understand
completely. It takes its soul
from the Lisp machines, its malleability from Smalltalk and its discipline of
smallness from [Oberon](https://people.inf.ethz.ch/wirth/ProjectOberon/index.html),
whose revised edition is free, covers the operating system, the compiler, the
editor and the Verilog for the CPU it runs on, and is the standing proof that
a whole system can be small enough to read.

**The full design is in [`docs/design.md`](docs/design.md).** Read it first.

## Status: phase 2 complete — the system reads, prints, evaluates and rebuilds itself in Lisp

Phase 1 ended with a compiled world built by itself (below). Phase 2, *self-hosting*
([#39](https://github.com/quokka-lisp/quokka-lisp/issues/39)), has since moved the
reader, the printer and the read-eval-print loop into Lisp, and taught a running
world to rebuild itself from its own source:

```
$ cargo run --release -p quokka -- build c1.qi
$ cargo run --release -p quokka -- rebuild c1.qi c2.qi
c2.qi: rebuilt, Value("(:FORMS 1287 :REPLACED 1032)")
$ cargo run --release -p quokka -- c2.qi
quokka> (defpackage :geo (:use :cl) (:export :area))
#<PACKAGE "GEO">
quokka> (in-package :geo)
#<PACKAGE "GEO">
quokka> (defun area (r) (* r r))
AREA
quokka> (in-package :cl-user)
#<PACKAGE "COMMON-LISP-USER">
quokka> (list (geo:area 3) 'geo::area (eval (read-from-string "(+ 1 2)")))
(9 GEO:AREA 3)
```

**The phase's gate passes:** the compiled compiler reproduces itself. Every
compiled function is the same in the first build and in two rebuilds, and the
two rebuilt worlds are the same word for word, apart from gensym names. That
stays a standing test ([#50](https://github.com/quokka-lisp/quokka-lisp/issues/50)).
**And tier 1 is complete and measured**
([#82](https://github.com/quokka-lisp/quokka-lisp/issues/82)): of the 13,558
tests in ansi-test's fifteen tier-1 groups, 8,141 pass, and every one of the
rest is explained by a feature deferred to a later tier, such as `loop`, CLOS
or `typep`. Phase 3, conditions and processes, is next.
**How to build, run, rebuild and test it all is in the
[developer manual](docs/manual.md).**

### Phase 1: a compiled world, built by itself

At the end of phase 1 the plain REPL was the bootstrap interpreter; today
that is `quokka bootstrap`, and the same session runs unchanged in a world.

```
$ cargo run -p quokka -- bootstrap
boot> (defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
FACT
boot> (fact 20)
2432902008176640000
boot> (defun down (n) (if (= n 0) 'done (down (- n 1))))
DOWN
boot> (down 100000)
DONE
boot> (defstruct point x y)
POINT
boot> (let ((p (make-point :x 1 :y 2))) (setf (point-x p) 42) (point-x p))
42
boot> (handler-case (car 5) (type-error () 'that-was-not-a-list))
THAT-WAS-NOT-A-LIST
boot> (multiple-value-bind (value present) (gethash 'absent (make-hash-table))
          (list value present))
(NIL NIL)
boot> (macrolet ((twice (x) (list '+ x x))) (twice 21))
42
```

The phase 0 gate was *"`(fact 20)` runs; the GC survives an allocation stress
test."* Both passed. **Tier 0**, the harder target, is finished too: *enough
Lisp that the compiler, which is written in Lisp, can be interpreted*. And
phase 1's gate is met:

> *An image saves, reloads and resumes where it left off.*

```
$ cargo run -p quokka -- build world.qi
world.qi: 107144 bytes, 13347 words, sequence 1

$ cargo run -p quokka -- world.qi
world.qi: 13347 words, sequence 1 · compiled · tier 1
quokka> (reverse '(1 2 3))
(3 2 1)
```

That second start did not read `prelude.lisp`. It read a file of numbers into
a vector and began, which is what the design means by *booting means loading
an image, not running a build*. And the world in that file is **compiled**: the
prelude, every macro and the compiler are bytecode, every form typed is
compiled by the image's own compiler, and the interpreter never runs.

"Resumes where it left off" is the strong part, and it means what it says: a
sixty-deep recursion stopped after forty steps, saved, loaded into a
**different** heap and resumed there finishes and gives the right answer.
Nothing restarts it. That works because rule 4 put execution state in the heap
rather than on Rust's call stack, so there is no part of a computation a file
cannot reach.

### What exists

| | |
| --- | --- |
| **Tagged values** | one 64-bit word per object; fixnums, conses, heap objects, immediates |
| **Heap** | object headers, bump allocation, a soft limit and a hard ceiling |
| **Garbage collector** | Cheney's copying collector, with a torture mode that collects at every opportunity and verifies the heap afterwards |
| **Rooting** | holding a value across a collection is a **compile error**, not a convention |
| **Reader** | in Rust for the bootstrap: lists, dotted pairs, symbols, fixnums, strings, characters, vectors, keywords, `quote`, backquote, nested comments; **and in Lisp** for everything a built world reads — driven by a readtable (`set-macro-character`, `#` dispatch), reading `pkg:sym` and `pkg::sym` through `*package*`, from string input streams |
| **Printer, in Lisp** | `prin1`, `princ`, `print`, `write` and their `-to-string` forms, with package prefixes and `\|...\|` escapes by SBCL's measured rules; every standard `*print-…*` variable, `*print-readably*`, shared and circular structure as `#1=`/`#1#` under `*print-circle*`, and `with-standard-io-syntax` |
| **Output** | `*standard-output*` writes to the console through a host request; string output streams and `with-output-to-string` |
| **`eval` and `load`** | `eval` compiles and calls — a built world never interprets; `load` reads a file through the host or a stream; the read-eval-print loop itself is Lisp; `compile`, and compiler macros the compiler applies |
| **The warm rebuild** | `quokka rebuild`: a running world reads its source from disk with its own reader, compiles it with its own compiler in two passes, replaces every function, and proves it did |
| **Evaluator** | an explicit-control machine — no recursive `eval`, so it can stop and resume, and Lisp recursion costs heap rather than Rust stack |
| **Tail calls** | eliminated, so recursive loops run in constant space |
| **Closures** | `lambda`, lexical scope, captured environments |
| **Special operators** | **all 25 of Common Lisp's** — 22 in tier 0, which the interpreter runs; `progv`, `load-time-value` and `symbol-macrolet` in tier 1, compiled only, with macros that see their lexical environment through `&environment` |
| **Macros** | `defmacro` with full lambda lists, backquote, `gensym`, `macroexpand`, and `macrolet` for lexical ones |
| **Lists, loops, equality** | `dolist`, `dotimes`, `do`, `case`, `destructuring-bind`, `return`; `mapcar` and its five relatives, `assoc`, `subst`, `union`, `nconc`, `last`; `eql`, `equal`, `equalp` — CLHS chapters 5, 6 and 14, in Lisp |
| **Sequences** | `find`, `position`, `count`, `remove`, `substitute`, `remove-duplicates`, `search`, `mismatch`, `map`, `reduce`, `every`/`some`, `fill`, `replace`, `subseq`, `concatenate`, a stable merge `sort` — CLHS chapter 17, on lists, vectors and strings alike, each written once over one iteration macro |
| **Strings and characters** | `char=` and `char-equal` families with any number of arguments, the twelve string comparisons with ranges, `string-upcase`/`-downcase`/`-capitalize`, `string-trim`, `char`/`schar` — CLHS chapters 13 and 16 |
| **`format`** | basic `format`: `~A` `~S` with padding, integers in any radix with grouping and signs, `~C`, `~P`, `~*`, conditionals `~[`, iteration `~{` with `~^`, case conversion `~(`, `~?`, and `formatter` — control strings parsed to a tree, then run |
| **Integers of any size** | bignums: arithmetic, comparison, `truncate`/`floor`/`ceiling`/`round`, `gcd`, `expt`, `isqrt`, and `ash`/`logand`/`logior`/`logxor` on two's complement — the representation and the fixnum boundary in Rust, the algorithms in Lisp, with the fixnum fast path untouched |
| **Floats** | IEEE doubles, the one float format: read and printed as SBCL does (the shortest digits that read back, from Rust), `+ - * /` and exact comparison across fixnums, bignums and floats, `floor` and its family, `decode-float` and kin, `sqrt`, `exp`, `log`, the trigonometric and hyperbolic functions (the same bits on every target, from `libm`), `format`'s `~F ~E ~G ~$`; overflow and division by zero signal errors, as in SBCL |
| **Lambda lists** | `&optional` and `&key` with default forms and supplied-p, `&rest`, `&aux`, `&body`, and destructuring in macros |
| **Keywords** | `:foo` evaluates to itself and is a different symbol from `foo` — two-sevenths of a package system, on purpose |
| **Vectors and strings** | `#(1 2 3)`, `aref`, `length` and `elt` over lists, vectors and strings alike; a string written in source is **read-only**, so a literal cannot be edited by accident ([#26](https://github.com/quokka-lisp/quokka-lisp/issues/26)) |
| **Arrays** | all of CLHS 15: any number of dimensions (`#2A((1 2) (3 4))`), fill pointers, `vector-push-extend`, adjustable and displaced arrays, bit vectors (`#*1011`) and the bit operations; a non-simple array is a header Rust's `aref` follows ([#67](https://github.com/quokka-lisp/quokka-lisp/issues/67)) |
| **Hash tables** | any object as a key: by content where it has one, otherwise by address, with the collector marking a table stale as it moves the keys |
| **`setf`** | `(setf (aref v i) x)`, `(setf (gethash k h) v)` — pulled forward into tier 0 so that mutation could be tested against SBCL; in the library, **places anyone can define** (`defsetf`, `define-setf-expander`, `get-setf-expansion`) and the macros that update them, `incf`, `decf`, `push`, `pop`, `pushnew`, `rotatef`, `shiftf`, `psetf`, each subform evaluated once, left to right |
| **Conditions** | `error`, `handler-case`, `ignore-errors`; the evaluator's own errors carry the standard's type names, so `(handler-case (car 5) (type-error () …))` means what it means in SBCL |
| **`defstruct`** | all of CLHS 8: a structure is an object of its own type, printed as `#S(...)` and read back; `:include`, BOA constructors, `:conc-name`, `:type list`/`vector`, `:print-function`, slot options (#73) |
| **Special variables** | `defvar`, `defparameter`, and dynamic binding by `let`, `let*` and parameters — the first of tier 1, in a library the compiled world loads; shallow binding, undone however a binding is left |
| **Packages** | `defpackage` with all its options, `in-package`, `*package*`, `intern` and `find-symbol` with their statuses, `export`/`unexport`, `use-package`/`unuse-package`, `import`, `shadow`, `unintern`, `delete-package`, `rename-package`, `do-symbols` and `with-package-iterator`, nicknames; name conflicts are refused; `COMMON-LISP` holds all 978 standard names, Quokka's internals live in `QUOKKA` — Lisp objects in the library, adopted from the system's symbol table after the build, and read and printed with prefixes |
| **Strings, packed** | a constant string of Latin-1 characters takes one byte a character, not eight; the image shrank from 478 KB to 254 KB ([#44](https://github.com/quokka-lisp/quokka-lisp/issues/44)) |
| **Multiple values** | `values`, `multiple-value-bind`, `multiple-value-call`, `multiple-value-prog1` — and every form that should pass them through does |
| **A prelude** | [`lisp/prelude.lisp`](lisp/prelude.lisp): `defun`, `when`, `unless`, `cond`, `and`, `or`, `setf`, `defstruct`, `handler-case`, the list and sequence functions — about six hundred lines of **Lisp**, compiled into the binary and interpreted at boot |
| **A bytecode VM** | forty opcodes, a contiguous operand stack in the heap, and a **verifier** that checks stack depth, jump targets and constant indices once at load — the same bargain the JVM makes |
| **A compiler, in Lisp** | [`lisp/compiler/compile.lisp`](lisp/compiler/compile.lisp): tier-0 Common Lisp that compiles **every special operator**, the prelude, and **itself** — and refuses what the interpreter refuses, so the two agree on what a program is |
| **Images** | the whole heap to a file and back, *including a computation stopped in mid-air*. Saving is `to_le_bytes` and loading is the inverse, with no pointer patching, because rule 3 made references word indices rather than addresses |
| **A compiled world** | `quokka build` builds an image in which the prelude, every macro and the compiler are bytecode, and a scan of the whole heap proves nothing interpreted is left; `--image` compiles every form with the image's own compiler and never uses the interpreter |
| **Host requests** | a program asks the outside world for something by leaving a request in the heap and blocking; the host answers between slices, at once or when a browser Promise resolves. An image saved while a request is waiting reloads still waiting. The first request is `%read-source` |
| **REPL** | native terminal, and a browser dev console on the WASM build; `quokka WORLD.qi` runs a compiled world in its own Lisp and `:save` saves it, `quokka rebuild` rebuilds one, `quokka bootstrap --compile` uses the bootstrap compiler, `--show-code` prints the instructions first |

438 tests, green on native, under wasmtime, and with the GC torture mode on.
Fifty-one Common Lisp files produce byte-identical output on Quokka and SBCL:
seventeen on Quokka interpreted, compiled and under WebAssembly, thirty-four more
of tier 1 compiled only, and all fifty-one again in a world that has rebuilt
itself. The Rust core is 16,017 lines of the 16,017-line budget, raised from
15,000 when arrays moved into tier 1 (#67), from 15,500 when structures
became objects of their own (#73), from 15,537 when a macro's expander
began to take the whole form, as in Common Lisp (#76), and in smaller steps
since for arrays of element type nil (#77), the last tier-1 gaps (#79, #80),
ratios (#59) and string comparison (#82).

### What does not exist yet

**Tier 2.** Tier 1 is complete and measured against ansi-test, the Common
Lisp conformance suite: 8,141 of 13,558 tests in the groups it runs pass, and
the rest wait for features of later tiers
([`docs/ansi-test.md`](docs/ansi-test.md) has the table; #60, #82).
The first world is still built by the interpreter, which is the cold build,
and is then rebuilt by itself.

No single-floats or complex numbers (ratios arrived with
[#59](https://github.com/quokka-lisp/quokka-lisp/issues/59)).
No `loop`, no CLOS. No restarts
or `define-condition`: `handler-case` is the exception-shaped half of the
condition system, and the rest is phase 3. No renderer and no editor, which
are phases 4 through 6 in [`docs/design.md`](docs/design.md).

## The book

[`book/`](book/README.md) holds **The Road to Quokka**: the design and
implementation written up as a textbook, alongside the system rather than
afterwards. Sixty-four chapters in six parts, covering the substrate, reading
and running, the machine, the compiler, the image and self-hosting, assuming
no prior knowledge of language implementation, including what a garbage collector is,
what `car` and `cdr` are, what an opcode is, and why a program would want to
rewrite itself.

It records what decisions cost, not only which were chosen, including several
occasions where a rule was written down and then broken by the person who wrote
it, which is the part a book written afterwards would quietly omit. Part III
begins before the thing it describes exists, and says so: its first chapter is
a plan, kept so that what actually happens can correct it. Part IV is the
compiler, and ends on a list of the decisions in it that are expected to
change. Part V is the image, and spends most of its length on why two rules
written for unrelated reasons, references as indices and execution state in
the heap, turned the hardest thing in a runtime into a `memcpy`. Part VI is
self-hosting as it happens, and then everything after it: special variables,
packages, packing strings, the reader and printer moved into Lisp, `eval`, a
world rebuilding itself and the fixpoint that proves it, and then the rest of
tier 1 measured against the standard's own test suite.

```
mdbook serve book --open     # needs `cargo install mdbook`; optional
```

Start with [What has to be built](book/src/what-has-to-be-built.md), which is
the map.

## Building

Requires a rustup-managed stable toolchain; `rust-toolchain.toml` pins it and
installs the `wasm32-unknown-unknown` and `wasm32-wasip1` targets.

```
./scripts/check.sh                                     # everything CI runs
cargo run --release -p quokka -- build world.qi   # make a world
cargo run --release -p quokka -- world.qi         # work in it; :save FILE keeps it
```

`wasmtime`, `wasm-pack` and `sbcl` are optional, and `check.sh` skips those
legs and says which.

**[`docs/manual.md`](docs/manual.md) is the developer manual**: the three ways
the `quokka` program (`quokka WORLD.qi` runs a world, and `:save` keeps your work), making a world with the
cold build and rebuilding it warm, the bootstrap round, every kind of test,
and how to add a test, a library file, a primitive or a chapter.
[`CLAUDE.md`](CLAUDE.md) has the rules that must not be broken.

## Layout

```
crates/quokka-vm         the machine: tagged words, heap, GC, bytecode VM, images — no host dependencies
crates/quokka-bootstrap  the cold build: the Rust reader and the interpreter that make the first world
crates/quokka            the `quokka` program: runs a world, builds and rebuilds one
crates/quokka-wasm       the browser host (loads a world, runs it in slices) and a dev console
lisp/prelude.lisp        the core library, interpreted at boot
lisp/compiler/           the compiler, in Common Lisp (tier 0 only)
lisp/library/            tier 1, in Lisp: packages, reader, printer, eval, the rebuild
tests/lisp/              differential tests — the same file run interpreted, compiled, and in SBCL
book/                    The Road to Quokka
docs/design.md           the design
docs/manual.md           the developer manual: building, running, testing
docs/bytecode.md         the instruction set
assets/                  mascot, icons, palette
```

## How the work is tracked

Every development step and open decision is a
[GitHub issue](https://github.com/quokka-lisp/quokka-lisp/issues) before it is
code, with the reasoning in the comments.
[#9](https://github.com/quokka-lisp/quokka-lisp/issues/9) tracked phase 0,
[#18](https://github.com/quokka-lisp/quokka-lisp/issues/18) tracked tier 0, and
[#23](https://github.com/quokka-lisp/quokka-lisp/issues/23) tracked phase 1, now closed, and
[#39](https://github.com/quokka-lisp/quokka-lisp/issues/39) tracks phase 2; a
`decision` issue closes when the reasoning is recorded in the design and the
book, not when the code lands.

## What Quokka is measured against

Quokka is a Common Lisp subset, and rule 6 says that where it implements a CL
feature the behaviour must match. That means the standard is not background
reading: it is the specification the differential tests check against, and
when SBCL and Quokka disagree the first question is always what the standard
actually says.

- **[common-lisp.net/documentation](https://common-lisp.net/documentation)** —
  the hub: books, the standard, the libraries, and which rendering of the
  standard to read. Start here if you want one link.
- **[The Common Lisp Metaspec](https://metaspec.dev/)** — the draft standard
  (X3J13/94-101R) rendered from its original TeX sources, with errata,
  addenda and the X3J13 cleanup issues. It is also published **as
  s-expressions**, in a `(node-type plist children*)` format
  ([source](https://codeberg.org/dlowe/metaspectre)), which makes the
  specification something a Lisp program can read, not only a person.
  Alternatives: [Community Spec](https://cl-community-spec.github.io/pages/index.html)
  and [Novaspec](https://novaspec.org/).
- **[The Common Lisp Documentation Project](https://lisp-docs.github.io/)** —
  open-source and **MIT-licensed**: the standard with community-written
  explanations and examples, plus a tutorial and how-to guides.
  - [Technical reference](https://lisp-docs.github.io/cl-language-reference/) ·
    [Tutorial](https://lisp-docs.github.io/docs/tutorial) ·
    [How-to guides](https://lisp-docs.github.io/docs/howto)
- **[The Common Lisp Cookbook](https://lispcookbook.github.io/cl-cookbook/)** —
  task-shaped rather than specification-shaped, with worked code. The one
  reference here written from the user's chair, and so the one that answers
  *what does idiomatic Common Lisp look like*, which is the question that
  decides what the prelude should support. BSD-style licence.
- **[The Common Lisp HyperSpec](http://www.lispworks.com/documentation/HyperSpec/Front/index.htm)**
  — the hyperlinked edition most books and tools point to, and what *CLHS* in
  this repository means. **Not free**: LispWorks permits only complete,
  unmodified, non-commercial copies and no derivative works, so it can be
  linked and cited here but not quoted into the book or the tests.
- **ANSI INCITS 226-1994** is the published standard, and is neither free nor
  online. The final draft is, which is what every rendering above is made
  from.

## Licence

MIT. See [`LICENSE`](LICENSE). The brand assets in `assets/` are covered by the
same licence.
