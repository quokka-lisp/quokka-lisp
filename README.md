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

## Status: phase 0 complete

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
| **Closures** | `lambda`, `defun`, lexical scope, captured environments |
| **REPL** | native terminal, and a browser dev console on the WASM build |

112 tests, green on native, under wasmtime, and with the GC torture mode on.

### What does not exist yet

6 of Common Lisp's 25 special operators. No `let`, `cond`, `labels`, `&rest`, or
macros. No bignums ([#16](https://github.com/quokka-lisp/quokka-lisp/issues/16))
or floats ([#17](https://github.com/quokka-lisp/quokka-lisp/issues/17)). No
bytecode compiler, no image save, no renderer, no editor — those are phases 1
through 6 in [`docs/design.md`](docs/design.md).

## The book

[`book/`](book/README.md) holds **The Road to Quokka**: the design and
implementation written up as a textbook, alongside the system rather than
afterwards. Twelve chapters so far, assuming no prior knowledge of language
implementation.

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
[#9](https://github.com/quokka-lisp/quokka-lisp/issues/9) tracks phase 0 and
links the rest; a `decision` issue closes when the reasoning is recorded in the
design and the book, not when the code lands.

## Licence

MIT. See [`LICENSE`](LICENSE). The brand assets in `assets/` are covered by the
same licence.
