<p align="center">
  <img src="assets/brand/01_Logos/logo-horizontal-transparent-1440x360.png" alt="Quokka Lisp" width="560">
</p>

# Quokka

> A modern Lisp-machine-style environment: a small Rust VM running a Common
> Lisp subset, browser-first via WASM, with a live, presentation-based UI.

Quokka is one live Lisp world — VM, language, editor, UI and tools in a single
image — small enough for one person to understand completely. It takes its soul
from the Lisp machines, its malleability from Smalltalk and its discipline of
smallness from [Oberon](https://people.inf.ethz.ch/wirth/ProjectOberon/index.html),
whose revised edition — operating system, compiler, editor and the Verilog for
the CPU it runs on — is free and is the standing proof that a whole system can
be small enough to read.

**The full design is in [`docs/design.md`](docs/design.md).** Read it first.

## Status: phase 1 — the compiler runs, and an image saves and resumes

```
$ cargo run -p quokka-repl
Quokka 0.0.0 — phase 1 · Bytecode and image
Ctrl-D to leave.
quokka> (defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
FACT
quokka> (fact 20)
2432902008176640000
quokka> (defun down (n) (if (= n 0) 'done (down (- n 1))))
DOWN
quokka> (down 100000)
DONE
quokka> (defstruct point x y)
POINT
quokka> (let ((p (make-point :x 1 :y 2))) (setf (point-x p) 42) (point-x p))
42
quokka> (handler-case (car 5) (type-error () 'that-was-not-a-list))
THAT-WAS-NOT-A-LIST
quokka> (multiple-value-bind (value present) (gethash 'absent (make-hash-table))
          (list value present))
(NIL NIL)
quokka> (macrolet ((twice (x) (list '+ x x))) (twice 21))
42
```

The phase 0 gate was *"`(fact 20)` runs; the GC survives an allocation stress
test."* Both passed. **Tier 0** — the harder target — is finished too: *enough
Lisp that the compiler, which is written in Lisp, can be interpreted*. And
phase 1's gate is met:

> *An image saves, reloads and resumes where it left off.*

```
$ cargo run -p quokka-repl -- --save-image world.qi
world.qi: 107144 bytes, 13347 words, sequence 1

$ cargo run -p quokka-repl -- --image world.qi
Loaded world.qi: 13347 words, sequence 1.
quokka> (reverse '(1 2 3))
(3 2 1)
```

That second start did not read `prelude.lisp`. It read a file of numbers into
a vector and began — which is what the design means by *booting means loading
an image, not running a build*.

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
| **Reader** | tier 0 syntax: lists, dotted pairs, symbols, fixnums, strings, characters, vectors, keywords, `quote`, backquote, nested comments |
| **Evaluator** | an explicit-control machine — no recursive `eval`, so it can stop and resume, and Lisp recursion costs heap rather than Rust stack |
| **Tail calls** | eliminated, so recursive loops run in constant space |
| **Closures** | `lambda`, lexical scope, captured environments |
| **Special operators** | **22 of Common Lisp's 25** — everything except `progv`, `load-time-value` and `symbol-macrolet`, which are outside tier 0 by design |
| **Macros** | `defmacro` with full lambda lists, backquote, `gensym`, `macroexpand`, and `macrolet` for lexical ones |
| **Lambda lists** | `&optional` and `&key` with default forms and supplied-p, `&rest`, `&aux`, `&body`, and destructuring in macros |
| **Keywords** | `:foo` evaluates to itself and is a different symbol from `foo` — two-sevenths of a package system, on purpose |
| **Vectors and strings** | `#(1 2 3)`, `aref`, `length` and `elt` over lists, vectors and strings alike; a string written in source is **read-only**, so a literal cannot be edited by accident ([#26](https://github.com/quokka-lisp/quokka-lisp/issues/26)) |
| **Hash tables** | keyed by content rather than address, because the collector moves things |
| **`setf`** | `(setf (aref v i) x)`, `(setf (gethash k h) v)` — pulled forward from tier 1 so that mutation could be tested against SBCL, and extensible, which is what `defstruct` needs |
| **Conditions** | `error`, `handler-case`, `ignore-errors`; the evaluator's own errors carry the standard's type names, so `(handler-case (car 5) (type-error () …))` means what it means in SBCL |
| **`defstruct`** | one line generating a constructor, a predicate, a copier and a settable accessor per slot |
| **Multiple values** | `values`, `multiple-value-bind`, `multiple-value-call`, `multiple-value-prog1` — and every form that should pass them through does |
| **A prelude** | [`lisp/prelude.lisp`](lisp/prelude.lisp): `defun`, `when`, `unless`, `cond`, `and`, `or`, `setf`, `defstruct`, `handler-case`, the list and sequence functions — about six hundred lines of **Lisp**, compiled into the binary and interpreted at boot |
| **A bytecode VM** | thirty-seven opcodes, a contiguous operand stack in the heap, and a **verifier** that checks stack depth, jump targets and constant indices once at load — the same bargain the JVM makes |
| **A compiler, in Lisp** | [`lisp/compiler/compile.lisp`](lisp/compiler/compile.lisp): 464 lines of tier-0 Common Lisp that compiles **every form of the prelude and every form of itself** — 85 functions, all accepted by the verifier |
| **Images** | the whole heap to a file and back, *including a computation stopped in mid-air*. Saving is `to_le_bytes` and loading is the inverse, with no pointer patching, because rule 3 made references word indices rather than addresses |
| **Host requests** | a program asks the outside world for something by leaving a request in the heap and blocking; the host answers between slices, at once or when a browser Promise resolves. An image saved while a request is waiting reloads still waiting. The first request is `%read-source` |
| **REPL** | native terminal, and a browser dev console on the WASM build; `--compile` compiles each form and runs it on the VM, `--show-code` prints the instructions first |

310 tests, green on native, under wasmtime, and with the GC torture mode on.
Seventeen Common Lisp files produce byte-identical output on Quokka natively,
Quokka under WebAssembly, and SBCL. The Rust core is 12,336 lines of the
15,000-line budget.

### What does not exist yet

**The world inside an image is still interpreted.** An image is a faithful
snapshot of whatever heap it is handed, and the only heap that exists is one
built by *interpreting* `prelude.lisp` — so `length` in a saved image is a
tree being walked, not bytecode. The compiler can compile all of it, and is
deliberately not allowed to install any of it: the compiler is itself
interpreted Lisp that calls `length`, so replacing `length` mid-compile breaks
the compiler on its next form.

The way out is to compile into a heap that is immediately written to a file
and abandoned, which needs two passes and is
[#33](https://github.com/quokka-lisp/quokka-lisp/issues/33). After that the
language is defined by a Lisp file rather than by
`crates/quokka-vm/src/eval.rs`, and phase 1
([#23](https://github.com/quokka-lisp/quokka-lisp/issues/23)) is done.

No packages — keywords exist, the rest is phase 2
([#20](https://github.com/quokka-lisp/quokka-lisp/issues/20) says why), which
is also why the implementation's own names wear a `%` prefix instead. No
bignums ([#16](https://github.com/quokka-lisp/quokka-lisp/issues/16)) or floats
([#17](https://github.com/quokka-lisp/quokka-lisp/issues/17)). No `loop`, no
CLOS, no `define-setf-expander` and therefore no `incf` or `push`. No restarts
or `define-condition`: `handler-case` is the exception-shaped half of the
condition system, and the rest is phase 3. No renderer and no editor — phases
4 through 6 in [`docs/design.md`](docs/design.md).

## The book

[`book/`](book/README.md) holds **The Road to Quokka**: the design and
implementation written up as a textbook, alongside the system rather than
afterwards. Thirty-seven chapters in five parts — the substrate, reading and
running, the machine, the compiler, and the image — assuming no prior
knowledge of language implementation, including what a garbage collector is,
what `car` and `cdr` are, what an opcode is, and why a program would want to
rewrite itself.

It records what decisions cost, not only which were chosen — including several
occasions where a rule was written down and then broken by the person who wrote
it, which is the part a book written afterwards would quietly omit. Part III
begins before the thing it describes exists, and says so: its first chapter is
a plan, kept so that what actually happens can correct it. Part IV is the
compiler, and ends on a list of the decisions in it that are expected to
change. Part V is the image, and spends most of its length on why two rules
written for unrelated reasons — references as indices, execution state in the
heap — turned the hardest thing in a runtime into a `memcpy`.

```
mdbook serve book --open     # needs `cargo install mdbook`; optional
```

Start with [What has to be built](book/src/what-has-to-be-built.md), which is
the map.

## Building

Requires a rustup-managed stable toolchain; `rust-toolchain.toml` pins it and
installs the `wasm32-unknown-unknown` and `wasm32-wasip1` targets.

```
./scripts/check.sh       # everything CI runs
cargo run -p quokka-repl # the REPL
```

`wasmtime`, `wasm-pack` and `sbcl` are optional — `check.sh` skips those legs
and says which. See [`CLAUDE.md`](CLAUDE.md) for the full command set, the rules
that must not be broken, and a toolchain gotcha worth knowing.

## Layout

```
crates/quokka-vm      the VM: tagged words, heap, GC, reader, evaluator — no host dependencies
crates/quokka-repl    native terminal REPL
crates/quokka-wasm    WASM bindings and a browser dev console
lisp/prelude.lisp     the core library, interpreted at boot
lisp/compiler/        the compiler, in Common Lisp (tier 0 only)
tests/lisp/           differential tests — the same file run three ways
book/                 The Road to Quokka
docs/design.md        the design
docs/bytecode.md      the instruction set
assets/               mascot, icons, palette
```

## How the work is tracked

Every development step and open decision is a
[GitHub issue](https://github.com/quokka-lisp/quokka-lisp/issues) before it is
code, with the reasoning in the comments.
[#9](https://github.com/quokka-lisp/quokka-lisp/issues/9) tracked phase 0,
[#18](https://github.com/quokka-lisp/quokka-lisp/issues/18) tracked tier 0, and
[#23](https://github.com/quokka-lisp/quokka-lisp/issues/23) tracks phase 1,
where steps A to C are closed and
[#33](https://github.com/quokka-lisp/quokka-lisp/issues/33) is what is left; a
`decision` issue closes when the reasoning is recorded in the design and the
book, not when the code lands.

## What Quokka is measured against

Quokka is a Common Lisp subset, and rule 6 says that where it implements a CL
feature the behaviour must match. That means the standard is not background
reading — it is the specification the differential tests check against, and
when SBCL and Quokka disagree the first question is always what the standard
actually says.

- **[common-lisp.net/documentation](https://common-lisp.net/documentation)** —
  the hub: books, the standard, the libraries, and which rendering of the
  standard to read. Start here if you want one link.
- **[The Common Lisp Metaspec](https://metaspec.dev/)** — the draft standard
  (X3J13/94-101R) rendered from its original TeX sources, with errata,
  addenda and the X3J13 cleanup issues. It is also published **as
  s-expressions**, in a `(node-type plist children*)` format
  ([source](https://codeberg.org/dlowe/metaspectre)) — which makes the
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
  *what does idiomatic Common Lisp look like* — which is the question that
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
