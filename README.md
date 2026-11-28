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

## Status: phase 4 complete, and the review after it

Phase 4, *the renderer and live text*
([#134](https://github.com/quokka-lisp/quokka-lisp/issues/134)), put a world
in a browser tab with a screen of its own. Open the page and you are at a
listener: type a form, press Enter, and its value appears under it. Any
earlier form can be edited where it stands and run again with Cmd-Enter; it
runs as a new entry, and the old results are struck through. An error opens
the debugger in the listener, with the same restarts and commands as at a
terminal, and Stop abandons a form that will not end.

Every part of that screen is drawn by Lisp. The world keeps **output
records** (a piece of text, an editor, a block of them, a picture), each with
an identity, and sends the page messages through a display protocol saying
what was added, changed or removed; the page's renderer only follows them.
A **viewer** can redraw itself after every event, and reconciliation keeps a record's
identity when it is drawn again in the same place, so the page changes only
what changed. The listener is one viewer, written in
[`lisp/library/listener.lisp`](lisp/library/listener.lisp). Pictures are
records too:

```lisp
(use-package :quokka-ui)
(with-drawing (t :width 120 :height 60)
  (draw-rectangle* 0 0 120 60 :ink +dim+)
  (draw-circle* 60 30 20 :ink +accent+))
```

draws a circle in the accent ink on a dim rectangle, under the form, as SVG.
In a terminal, which cannot show a picture, it says so. The design and its
thirteen decisions are [`docs/display.md`](docs/display.md). The gate,
*edit and evaluate a form in the browser*, was met on 27 Nov 2026 and is
tested three ways: the world's half from Rust, the whole screen through the
renderer's model in Node (it dumps the screen as text and compares it), and
both pages in a headless Chrome
([#142](https://github.com/quokka-lisp/quokka-lisp/issues/142)).

### The review after phase 4

We then reviewed everything built in phases 3 and 4
([#144](https://github.com/quokka-lisp/quokka-lisp/issues/144)) and found 54
things to fix (#145 to #198). Most were small, and are fixed. The largest was
cost: the system had been measured for being right and never for being fast.
A generic function call had come to cost 1,446 steps of the machine, against
4 for a plain call. After the review's fixes it costs about 120, an accessor
268, and `make-instance` 1,468 where it was 11,193; a hundred lines printed at
the listener take 0.48 s in the browser where they took 3.75 s. A **step
benchmark** now holds those gains: it counts what 36 common forms cost and
fails the check if one becomes dearer
([#169](https://github.com/quokka-lisp/quokka-lisp/issues/169)). Steps are
the same on every computer, so it needs no quiet machine.

The review also compared the system with the three tutorials in
[`docs/tutorials/`](docs/tutorials/), which had been written as the target
and which nothing ran. So the roadmap now has nine phases, and each
remaining phase's gate is a tutorial that runs as written: the Tower of
Hanoi for phase 5, the Rolodex for phase 6, Quokka Hop for phase 7 and daily
work inside the system for phase 8
([#156](https://github.com/quokka-lisp/quokka-lisp/issues/156);
[`docs/design.md`](docs/design.md), *Phases 5 to 8*). Phase 5,
presentations and commands, is next.

### Phase 3: fix a function from inside its error and retry the frame

Phase 3, *conditions and processes*
([#123](https://github.com/quokka-lisp/quokka-lisp/issues/123)), ends with the
gate it set itself. An error no longer throws away the computation that met
it: the world stops, shows the stack, and waits while you redefine the
function that failed, and then runs the failed call again.

```
quokka> (defun broken (x) (error "broken ~a" x))
BROKEN
quokka> (defun caller (y) (list :got (broken (* y 2))))
CALLER
quokka> (caller 5)

debugger invoked on a SIMPLE-ERROR: broken 10

Type HELP for debugger help.

restarts (invokable by number or by name):
  0: [ABORT] Return to the top level.

0: (BROKEN 10)
0] backtrace
0: (BROKEN 10)
1: (CALLER 5)
0] (defun broken (x) (+ x 1))
BROKEN
0] restart-frame
(:GOT 11)
```

That needed, in this order:
- CLOS with a small metaobject protocol;
- conditions and restarts, and the machine's own errors signalled through
  them;
- definitions that keep their source;
- frames that keep their calls;
- a console a world can read, and the debugger;
- processes (green threads, with SBCL's thread interface) and Go-style
  channels.

The gate is tested through the real `quokka` program, in the main thread and
in a thread (#133). The fixpoint still holds, and ansi-test now runs eighteen
groups, 12,568 of 17,739 tests passing, with every other one explained.

### Phase 2: the system reads, prints, evaluates and rebuilds itself in Lisp

Phase 1 ended with a compiled world built by itself (below). Phase 2, *self-hosting*
([#39](https://github.com/quokka-lisp/quokka-lisp/issues/39)), has since moved the
reader, the printer and the read-eval-print loop into Lisp, and taught a running
world to rebuild itself from its own source:

```
$ cargo run --release -p quokka -- build c1.qi
$ cargo run --release -p quokka -- rebuild c1.qi c2.qi
rebuilt: (:FORMS 2468 :REPLACED 1786)
c2.qi: 2435744 bytes, 304418 words, sequence 2
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
([#82](https://github.com/quokka-lisp/quokka-lisp/issues/82)): of the 15,472
tests in ansi-test's fifteen tier-1 groups, 8,970 pass, and every one of the
rest was explained by a feature deferred to a later tier, such as `loop`, CLOS
or `typep` (the last two arrived in phase 3).
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
| **Ratios** | `(/ 1 3)` is `1/3`, always in lowest terms, exact in arithmetic with integers and contagious to floats; `rationalize` as SBCL's ([#59](https://github.com/quokka-lisp/quokka-lisp/issues/59)) |
| **Lambda lists** | `&optional` and `&key` with default forms and supplied-p, `&rest`, `&aux`, `&body`, and destructuring in macros |
| **Keywords** | `:foo` evaluates to itself and is a different symbol from `foo` — two-sevenths of a package system, on purpose |
| **Vectors and strings** | `#(1 2 3)`, `aref`, `length` and `elt` over lists, vectors and strings alike; a string written in source is **read-only**, so a literal cannot be edited by accident ([#26](https://github.com/quokka-lisp/quokka-lisp/issues/26)) |
| **Arrays** | all of CLHS 15: any number of dimensions (`#2A((1 2) (3 4))`), fill pointers, `vector-push-extend`, adjustable and displaced arrays, bit vectors (`#*1011`) and the bit operations; a non-simple array is a header Rust's `aref` follows ([#67](https://github.com/quokka-lisp/quokka-lisp/issues/67)) |
| **Hash tables** | any object as a key: by content where it has one, otherwise by address, with the collector marking a table stale as it moves the keys |
| **`setf`** | `(setf (aref v i) x)`, `(setf (gethash k h) v)` — pulled forward into tier 0 so that mutation could be tested against SBCL; in the library, **places anyone can define** (`defsetf`, `define-setf-expander`, `get-setf-expansion`) and the macros that update them, `incf`, `decf`, `push`, `pop`, `pushnew`, `rotatef`, `shiftf`, `psetf`, each subform evaluated once, left to right |
| **Conditions and restarts** | all of CLHS 9, in Lisp: `define-condition`, `signal`, `error`, `warn`, `cerror`, `handler-bind`, `handler-case`, `restart-case`, `invoke-restart`, `check-type` with `store-value`; the machine's own errors are signalled through the same code at the point they happen, so `(handler-case (car 5) (type-error (c) (type-error-datum c)))` answers `5`, as in SBCL ([#125](https://github.com/quokka-lisp/quokka-lisp/issues/125), [`docs/conditions.md`](docs/conditions.md)) |
| **Types** | `deftype`, `typep`, `typecase`, `coerce`, and a `subtypep` that answers by asking whether one type minus the other is empty |
| **CLOS** | classes, generic functions with every standard method combination and `define-method-combination`, `change-class`, instances updated when their class is redefined, `print-object`, and the introspective part of the metaobject protocol in package `QUOKKA-MOP`, after AMOP ([#124](https://github.com/quokka-lisp/quokka-lisp/issues/124), [`docs/clos.md`](docs/clos.md)) |
| **Definitions that keep their source** | every `defun`, `defmacro`, `defclass` and the rest evaluated in a world keeps its form, its text with the comments above it, and its earlier versions; `who-calls`, `who-macroexpands`; redefining a macro marks what used it stale, and `recompile-stale-definitions` recompiles them; a name used and never defined draws a warning when the file or the listener entry ends ([#55](https://github.com/quokka-lisp/quokka-lisp/issues/55), [`docs/definitions.md`](docs/definitions.md)) |
| **Frames and the debugger** | every call frame keeps its arguments, so a backtrace shows `(BROKEN 10)` and a frame can be run again; the terminal debugger follows SBCL's, with restarts by number, `backtrace`, `list-locals`, `source`, `restart-frame` and `return` ([#130](https://github.com/quokka-lisp/quokka-lisp/issues/130), [#131](https://github.com/quokka-lisp/quokka-lisp/issues/131), [`docs/debugger.md`](docs/debugger.md)) |
| **Processes and channels** | green threads in one world, preempted at the end of a slice, with SBCL's `sb-thread` interface in package `QUOKKA-THREAD`: `make-thread`, `join-thread`, mutexes, wait queues; and Go's channels, `send`, `receive`, `select` and `spawn-task` ([#34](https://github.com/quokka-lisp/quokka-lisp/issues/34), [`docs/processes.md`](docs/processes.md)) |
| **Streams** | string streams, console streams for output, input and errors, two-way streams, and Gray streams (SBCL's `SB-GRAY` names in `QUOKKA-GRAY`), so a program can make a stream of its own class |
| **The display** | output records with identities, viewers that redraw by reconciliation, a command loop that reads the page's events, the listener as live text, and pictures (`with-drawing`, `draw-rectangle*` and kin, `with-translation`, `with-scaling`) sent as SVG; all in Lisp, in packages `QUOKKA-UI` and `QUOKKA-ENV` ([#134](https://github.com/quokka-lisp/quokka-lisp/issues/134), [`docs/display.md`](docs/display.md)) |
| **`defstruct`** | all of CLHS 8: a structure is an object of its own type, printed as `#S(...)` and read back; `:include`, BOA constructors, `:conc-name`, `:type list`/`vector`, `:print-function`, slot options (#73) |
| **Special variables** | `defvar`, `defparameter`, and dynamic binding by `let`, `let*` and parameters — the first of tier 1, in a library the compiled world loads; shallow binding, undone however a binding is left |
| **Packages** | `defpackage` with all its options, `in-package`, `*package*`, `intern` and `find-symbol` with their statuses, `export`/`unexport`, `use-package`/`unuse-package`, `import`, `shadow`, `unintern`, `delete-package`, `rename-package`, `do-symbols` and `with-package-iterator`, nicknames; name conflicts are refused; `COMMON-LISP` holds all 978 standard names, Quokka's internals live in `QUOKKA` — Lisp objects in the library, adopted from the system's symbol table after the build, and read and printed with prefixes |
| **Strings, packed** | a constant string of Latin-1 characters takes one byte a character, not eight; the image shrank from 478 KB to 254 KB ([#44](https://github.com/quokka-lisp/quokka-lisp/issues/44)) |
| **Multiple values** | `values`, `multiple-value-bind`, `multiple-value-call`, `multiple-value-prog1` — and every form that should pass them through does |
| **A prelude** | [`lisp/prelude.lisp`](lisp/prelude.lisp): `defun`, `when`, `unless`, `cond`, `and`, `or`, `setf`, `defstruct`, `handler-case`, the list and sequence functions — about nine hundred and fifty lines of **Lisp**, compiled into the binary and interpreted by the cold build |
| **A bytecode VM** | forty opcodes, a contiguous operand stack in the heap, and a **verifier** that checks stack depth, jump targets and constant indices once at load — the same bargain the JVM makes |
| **A compiler, in Lisp** | [`lisp/compiler/compile.lisp`](lisp/compiler/compile.lisp): tier-0 Common Lisp that compiles **every special operator**, the prelude, and **itself** — and refuses what the interpreter refuses, so the two agree on what a program is |
| **Images** | the whole heap to a file and back, *including a computation stopped in mid-air*. Saving is `to_le_bytes` and loading is the inverse, with no pointer patching, because rule 3 made references word indices rather than addresses |
| **A compiled world** | `quokka build` builds an image in which the prelude, every macro and the compiler are bytecode, and a scan of the whole heap proves nothing interpreted is left; `--image` compiles every form with the image's own compiler and never uses the interpreter |
| **Host requests** | a program asks the outside world for something by leaving a request in the heap and blocking; the host answers between slices, at once or when a browser Promise resolves. An image saved while a request is waiting reloads still waiting. The first request is `%read-source` |
| **REPL** | in a terminal, `quokka WORLD.qi` runs a compiled world in its own Lisp, with the debugger when its input is a terminal, and `:save` saves it; `quokka rebuild` rebuilds one; `quokka bootstrap --compile` uses the bootstrap compiler, `--show-code` prints the instructions first. In the browser, the environment page with its listener, and the older dev console beside it |

582 tests, green on native, under wasmtime, and with the GC torture mode on.
Sixty-eight Common Lisp files produce byte-identical output on Quokka and
SBCL: seventeen on Quokka interpreted, compiled and under WebAssembly,
fifty-one more of tier 1 compiled only, and all sixty-eight again in a world
that has rebuilt itself. Phase 3's gate is a test that types a session at the
real `quokka` program, and phase 4's drives the page in Node and in a
headless Chrome. Everything above the primitive table is Lisp.

### What does not exist yet

**Tier 2.** Tier 1 is complete and measured against ansi-test, the Common
Lisp conformance suite: 12,568 of 17,739 tests in the eighteen groups it runs
pass, and the rest wait for features of later tiers or agree with SBCL
([`docs/ansi-test.md`](docs/ansi-test.md) has the table; #60, #82, #125).
The first world is still built by the interpreter, which is the cold build,
and is then rebuilt by itself.

No single-floats or complex numbers. No `loop` yet (a port of MIT LOOP is
early in phase 5, [#146](https://github.com/quokka-lisp/quokka-lisp/issues/146)),
no pretty printer and no pathnames.

The screen can show text, editors and pictures, but not yet things you can
point at: presentations, commands and the inspector are phase 5, with the
Tower of Hanoi tutorial as its gate. A world in the browser cannot yet keep
its work across a reload except by Save world, which downloads it; stores,
journals and the start page are phases 5 and 6. Images, sound and a frame
clock for games are phase 7, and tiled viewers, the system browser, history,
export and file sync, the environment you would do daily work in, are
phase 8 ([`docs/design.md`](docs/design.md), *Roadmap*).

## The book

[`book/`](book/README.md) holds **The Road to Quokka**: the design and
implementation written up as a textbook, alongside the system rather than
afterwards. A hundred chapters in twelve parts, covering the substrate,
reading and running, the machine, the compiler, the image, self-hosting,
objects, conditions, definitions, the debugger, processes and the display,
assuming
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
tier 1 measured against the standard's own test suite. Parts VII to XI are
phase 3: CLOS, conditions and restarts, definitions, frames and the
debugger, and processes. Part XII is phase 4, the display, from Gray streams
and host requests for a screen to output records, viewers, the listener and
drawing.

```
mdbook serve book --open     # needs `cargo install mdbook`; optional
```

Start with [What has to be built](book/src/what-has-to-be-built.md), which is
the map.

## Building

Requires a rustup-managed stable toolchain; `rust-toolchain.toml` pins it and
installs the `wasm32-unknown-unknown` and `wasm32-wasip1` targets.

```
./scripts/check.sh                                # everything CI runs, and GC torture and wasmtime
cargo run --release -p quokka -- build world.qi   # make a world
cargo run --release -p quokka -- world.qi         # work in it; :save FILE keeps it
scripts/environment.sh                            # the environment in a browser, on port 8080
```

`wasmtime`, `wasm-pack`, `node` and `sbcl` are optional, and `check.sh` skips
those legs and says which. The environment needs `wasm-pack` and Python's
`http.server`.

**[`docs/manual.md`](docs/manual.md) is the developer manual**: running a
world in a terminal and in the browser, making a world with the cold build
and rebuilding it warm, the bootstrap round, every kind of test, and how to
add a test, a library file, a primitive or a chapter.
[`CLAUDE.md`](CLAUDE.md) has the rules that must not be broken.

## Layout

```
crates/quokka-vm         the machine: tagged words, heap, GC, bytecode VM, images — no host dependencies
crates/quokka-bootstrap  the cold build: the Rust reader and the interpreter that make the first world
crates/quokka            the `quokka` program: runs a world, builds and rebuilds one
crates/quokka-wasm       the browser host (loads a world, runs it in slices); www/ holds the
                         environment page, its renderer, and the dev console
lisp/prelude.lisp        the core library, interpreted by the cold build
lisp/compiler/           the compiler, in Common Lisp (tier 0 only)
lisp/library/            everything else, in Lisp: packages, reader, printer, eval, the rebuild,
                         CLOS, conditions, the debugger, threads, the display and the listener
tests/lisp/              differential tests — the same file run interpreted, compiled, and in SBCL
tests/ansi/              the runner and baseline that measure Quokka against ansi-test
tests/steps/             the step benchmark's forms and baseline
book/                    The Road to Quokka
docs/design.md           the design, and the roadmap
docs/manual.md           the developer manual: building, running, testing
docs/tutorials/          three programs a user should be able to type; each is a phase's gate
docs/*.md                one design document for each step that needed one: bytecode, CLOS,
                         conditions, definitions, frames, debugger, processes, display
assets/                  mascot, icons, palette
```

## How the work is tracked

Every development step and open decision is a
[GitHub issue](https://github.com/quokka-lisp/quokka-lisp/issues) before it is
code, with the reasoning in the comments.
[#9](https://github.com/quokka-lisp/quokka-lisp/issues/9) tracked phase 0,
[#18](https://github.com/quokka-lisp/quokka-lisp/issues/18) tracked tier 0, and
[#23](https://github.com/quokka-lisp/quokka-lisp/issues/23) tracked phase 1,
[#39](https://github.com/quokka-lisp/quokka-lisp/issues/39) phase 2,
[#123](https://github.com/quokka-lisp/quokka-lisp/issues/123) phase 3 and
[#134](https://github.com/quokka-lisp/quokka-lisp/issues/134) phase 4, all
closed; [#144](https://github.com/quokka-lisp/quokka-lisp/issues/144) is the
review after phase 4, with a table of its decisions; a
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
