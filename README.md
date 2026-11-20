<p align="center">
  <img src="assets/brand/01_Logos/logo-horizontal-transparent-1440x360.png" alt="Quokka Lisp" width="560">
</p>

# Quokka

> A modern Lisp-machine-style environment: a small Rust VM running a Common
> Lisp subset, browser-first via WASM, with a live, presentation-based UI.

Quokka is one live Lisp world — VM, language, editor, UI and tools in a single
image — small enough for one person to understand completely. It takes its soul
from the Lisp machines, its malleability from Smalltalk and its discipline of
smallness from Oberon.

**The full design is in [`docs/design.md`](docs/design.md).** Read it first.

## Status: phase 0 complete, finishing tier 0

```
$ cargo run -p quokka-repl
Quokka 0.0.0 — phase 0 · Spike
Ctrl-D to leave.
quokka> (defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
FACT
quokka> (fact 20)
2432902008176640000
quokka> (defun adder (n) (lambda (x) (+ x n)))
ADDER
quokka> (funcall (adder 5) 37)
42
quokka> (defun down (n) (if (= n 0) 'done (down (- n 1))))
DOWN
quokka> (down 100000)
DONE
quokka> (defmacro my-when (test &body body) `(if ,test (progn ,@body) nil))
MY-WHEN
quokka> (my-when (= 1 1) 'yes)
YES
quokka> (defun describe-shape (kind &key (colour 'black) (sides 0))
          (list kind colour sides))
DESCRIBE-SHAPE
quokka> (describe-shape 'square :sides 4)
(SQUARE BLACK 4)
```

The phase 0 gate was *"`(fact 20)` runs; the GC survives an allocation stress
test."* Both pass, and there is evidence from outside this repository for the
first: over a hundred Lisp forms produce byte-identical output on Quokka
natively, Quokka under WebAssembly, and SBCL.

### What exists

| | |
| --- | --- |
| **Tagged values** | one 64-bit word per object; fixnums, conses, heap objects, immediates |
| **Heap** | object headers, bump allocation, a soft limit and a hard ceiling |
| **Garbage collector** | Cheney's copying collector, with a torture mode that collects at every opportunity and verifies the heap afterwards |
| **Rooting** | holding a value across a collection is a **compile error**, not a convention |
| **Reader** | tier 0 syntax: lists, dotted pairs, symbols, fixnums, strings, characters, `quote`, nested comments |
| **Evaluator** | an explicit-control machine — no recursive `eval`, so it can stop and resume, and Lisp recursion costs heap rather than Rust stack |
| **Tail calls** | eliminated, so recursive loops run in constant space |
| **Closures** | `lambda`, lexical scope, captured environments |
| **Special operators** | 19 of Common Lisp's 25, including `let`, `let*`, `flet`, `labels`, `block`/`return-from`, `catch`/`throw`, `tagbody`/`go` and `unwind-protect` |
| **Macros** | `defmacro`, expansion in the evaluator, backquote in the reader, `gensym` |
| **Lambda lists** | `&optional` and `&key` with default forms and supplied-p, `&rest`, `&aux` |
| **Keywords** | `:foo` evaluates to itself and is a different symbol from `foo` — two-sevenths of a package system, on purpose |
| **Vectors and strings** | `#(1 2 3)`, `aref`, `length` and `elt` over lists, vectors and strings alike |
| **Hash tables** | keyed by content rather than address, because the collector moves things |
| **`setf`** | `(setf (aref v i) x)`, `(setf (gethash k h) v)` — pulled forward from tier 1 so that mutation could be tested against SBCL |
| **`macroexpand`** | a macro's output as an ordinary value, so expansions can be read |
| **A prelude** | [`lisp/prelude.lisp`](lisp/prelude.lisp): `defun`, `when`, `unless`, `cond`, `and`, `or` and the list functions are **Lisp**, compiled into the binary and interpreted at boot |
| **REPL** | native terminal, and a browser dev console on the WASM build |

184 tests, green on native, under wasmtime, and with the GC torture mode on.
Fourteen Common Lisp files produce byte-identical output on Quokka and SBCL.

### What does not exist yet

Six special operators: `macrolet`, `multiple-value-call`,
`multiple-value-prog1`, and — outside tier 0 by design — `progv`,
`load-time-value` and `symbol-macrolet`. **No condition system**: no `error`,
no `handler-case`, so three things that want to report a problem currently
expand into a call to an undefined function instead. No `defstruct`. No
multiple values. Those, with the operators above, are what
[#18](https://github.com/quokka-lisp/quokka-lisp/issues/18) tracks, and what
the compiler needs before the interpreter can run it.

No packages — keywords exist, the rest is phase 2
([#20](https://github.com/quokka-lisp/quokka-lisp/issues/20) says why). No
bignums ([#16](https://github.com/quokka-lisp/quokka-lisp/issues/16)) or floats
([#17](https://github.com/quokka-lisp/quokka-lisp/issues/17)). No bytecode
compiler, no image save, no renderer, no editor — those are phases 1 through 6
in [`docs/design.md`](docs/design.md).

## The book

[`book/`](book/README.md) holds **The Road to Quokka**: the design and
implementation written up as a textbook, alongside the system rather than
afterwards. Nineteen chapters so far, assuming no prior knowledge of language
implementation — including what a garbage collector is, what `car` and `cdr`
are, and why a program would want to rewrite itself.

It records what decisions cost, not only which were chosen — including three
occasions where a rule was written down and then broken by the person who wrote
it, which is the part a book written afterwards would quietly omit.

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
lisp/                 Lisp sources: compiler and core library (tier 0 only)
tests/lisp/           differential tests — the same file run three ways
book/                 The Road to Quokka
docs/design.md        the design
assets/               mascot, icons, palette
```

## How the work is tracked

Every development step and open decision is a
[GitHub issue](https://github.com/quokka-lisp/quokka-lisp/issues) before it is
code, with the reasoning in the comments.
[#9](https://github.com/quokka-lisp/quokka-lisp/issues/9) tracked phase 0 and
[#18](https://github.com/quokka-lisp/quokka-lisp/issues/18) tracks tier 0; a
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
