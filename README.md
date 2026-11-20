# Quokka

> A modern Lisp-machine-style environment: a small Rust VM running a Common Lisp
> subset, browser-first via WASM, with a live, presentation-based UI.

Quokka is one live Lisp world — VM, language, editor, UI and tools in a single
image — small enough for one person to understand completely. It takes its soul
from the Lisp machines, its malleability from Smalltalk and its discipline of
smallness from Oberon.

**The full design is in [`docs/design.md`](docs/design.md).** Read it first.

Current phase: **0 · Spike**. Tagged heap, reader and interpreter in Rust;
native and WASM builds; terminal REPL. Nothing here works yet.

## Layout

```
crates/quokka-vm      core VM: tagged words, heap, GC, reader, interpreter (no host dependencies)
crates/quokka-repl    native terminal REPL binary
crates/quokka-wasm    WASM bindings plus a minimal browser dev console page
lisp/                 Lisp sources: compiler and core library (tier 0 only)
tests/lisp/           differential test files
docs/design.md        the design
```

## Building

Requires a rustup-managed stable toolchain; `rust-toolchain.toml` pins it and
installs the `wasm32-unknown-unknown` and `wasm32-wasip1` targets.

```
cargo build --workspace
cargo test  --workspace
```

Every change must also build for wasm32 — see [`CLAUDE.md`](CLAUDE.md) for the
full command set and the rules that must not be broken.

## Licence

MIT. See [`LICENSE`](LICENSE).
