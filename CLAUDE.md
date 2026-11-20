# Quokka — notes for Claude Code

Quokka is a modern Lisp-machine-style environment: a small Rust VM running a Common Lisp subset, browser-first via WASM, with a live, presentation-based UI. The full design is in `docs/design.md` — read it before making structural changes.

Current phase: **0 · Spike** (tagged heap, reader and interpreter in Rust; native and WASM builds; terminal REPL).

## Who this is for, and how to work with me

Carsten is an experienced programmer but **new to language implementation**. No
prior work on languages, compilers, interpreters or runtimes. Background: Scheme
and Lisp during a CS degree, and a basic Lisp interpreter written in Modula-2 as
a university project — about thirty years ago. Assume the general programming
knowledge is there and the domain knowledge is not.

Quokka is **deliberately a learning exercise**, for Carsten and for other people.
A companion repository, `quokka-book`, will describe the whole design and
implementation as a textbook for anyone who wants to understand how a system
like this works — in the spirit of Tanenbaum's operating systems books.

This changes how to work on the repo:

- **Explain, do not just deliver.** When a design choice is made, say what the
  alternatives were and why this one won. A decision with no recorded reasoning
  is a hole in the book.
- **Name the technique.** When the code uses a known idea — Cheney's algorithm,
  an explicit-control evaluator, a trampoline, NaN boxing, inline caches — call
  it by its name and point at the standard reference (SICP, the GC Handbook,
  Lisp in Small Pieces, the CLHS). Jargon that is named is searchable; jargon
  that is not is a wall.
- **Teach at decision points, not everywhere.** Long explanations belong where
  something is genuinely non-obvious or load-bearing, not on boilerplate.
- **Prefer the comprehensible implementation** when two are close in
  performance. Rule 7 already says this; here it is also a pedagogical rule.
  Where the fast version is genuinely needed, keep the simple one next to it as
  the explanation.
- **Comments say why.** The what is in the code. Chapters get written from the
  why.
- **Stop and explain rather than guess.** If a choice turns on knowledge Carsten
  has flagged as unfamiliar, lay out the options and the consequences and let
  him decide, instead of quietly picking one.

## Rules that must not be broken

1. **Rust below the primitive table, Lisp above it.** Rust provides only the VM: tagged words, heap, GC, image save/load, the bootstrap interpreter, bytecode execution, and host glue. Anything a user could want to change belongs in Lisp. If you are about to add a feature in Rust, check whether it belongs above the line.
2. **WASM on every commit.** Every change must build and pass tests for both native and `wasm32`. The core VM crate has no host dependencies: no `std::fs`, no threads, no wall clock, no networking. Host services are reached through a host interface implemented separately for native and WASM.
3. **Heap references are offsets, never raw pointers.** Store references relative to the heap base so images relocate and the same code runs on 64-bit native and wasm32. Follow the tagged-word scheme in `docs/design.md` (low 3 bits: `xx0` fixnum, `001` cons, `011` heap object, `101` immediate, `111` GC forwarding).
4. **The VM must be able to yield.** The interpreter runs in slices and returns control to its caller (the browser event loop). No unbounded blocking loops in the core. Execution state therefore lives in our heap as explicit frames, never on the Rust call stack — no recursive `eval`. The caller decides when a slice ends by passing a step budget to `run_slice`; the VM has no clock and no yield callback. Slicing must stay semantically invisible: collect on allocation rather than on slice boundaries, and never expose the remaining budget to Lisp. See `crates/quokka-vm/src/slice.rs`.
5. **The tier 0 rule.** Lisp sources for the compiler and core library use only tier 0 of the CL subset: no `loop`, no CLOS, only basic `format`, no `progv`, `load-time-value` or `symbol-macrolet`. The phase 0 interpreter only needs to support tier 0.
6. **Common Lisp semantics.** Quokka is a CL subset (a Lisp-2), not a new dialect. Where Quokka implements a CL feature, behaviour must match SBCL. Quokka-specific extensions go in their own packages, never in `COMMON-LISP`.
7. **Stay comprehensible.** Budget: the Rust core stays under 15k lines. Prefer clear code over clever code, keep dependencies few and permissively licensed, and justify every `unsafe` block with a comment explaining why it is sound.
8. **Licence.** MIT. Do not add dependencies or copied code with incompatible (e.g. copyleft) licences.
9. **Primitives are bounded.** A step budget only bounds time if a single step is short, and once control is inside a Rust primitive nothing can interrupt it. Every primitive must either complete quickly or be chunked so it can be resumed across slices. A primitive whose cost grows with the size of its argument — sorting, searching, big-number arithmetic, hashing a large string — needs a plan for this before it is added.

## Testing

- **Differential tests:** Lisp test files hold forms with expected printed results. They run on Quokka native, Quokka in WASM, and SBCL; all three must agree. SBCL is the oracle, not a build dependency.
- **Rust tests:** unit and property tests for tagged words, the heap, the reader and the GC.
- **GC torture mode:** a build or runtime flag that collects on every allocation. Run it in CI.
- **WASM:** run the same tests under wasmtime (WASI) and in a headless browser.
- Before Quokka has file hosts, Lisp source files are loaded through a single host primitive (`std::fs` natively, WASI in WASM tests, bundled files in the browser).

## Proposed layout (adjust if a better one emerges, and update this file)

```
crates/quokka-vm      core VM: tagged words, heap, GC, reader, interpreter (no host dependencies)
crates/quokka-repl    native terminal REPL binary
crates/quokka-wasm    WASM bindings plus a minimal browser dev console page
lisp/                 Lisp sources: compiler and core library (tier 0 only)
tests/lisp/           differential test files
docs/design.md        the design
scripts/check.sh      everything CI runs, in one command
.github/workflows/    CI: native, GC torture, both wasm targets, SBCL oracle
```

## Commands

`./scripts/check.sh` runs everything CI runs. Use it before every commit — it is
the cheapest way to honour rule 2.

| What | Command |
| --- | --- |
| Everything CI runs | `./scripts/check.sh` |
| Build (native) | `cargo build --workspace` |
| Test (native) | `cargo test --workspace` |
| REPL | `cargo run -p quokka-repl` |
| GC torture | `cargo test -p quokka-vm --features gc-torture` |
| Build for the browser | `cargo build -p quokka-vm -p quokka-wasm --target wasm32-unknown-unknown` |
| Test under WASI | `CARGO_TARGET_WASM32_WASIP1_RUNNER='wasmtime --dir=.' cargo test -p quokka-vm --target wasm32-wasip1` |
| Dev console bundle | `wasm-pack build crates/quokka-wasm --target web --out-dir www/pkg` |
| Serve the dev console | `python3 -m http.server -d crates/quokka-wasm/www 8080` |
| Format and lint | `cargo fmt --all` · `cargo clippy --workspace --all-targets -- -D warnings` |
| Docs | `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --open` |

`quokka-repl` is a native host and is deliberately **not** built for wasm32.

### Toolchain gotcha

`rust-toolchain.toml` pins stable and the two wasm targets, but it is only read
by the **rustup shim** (`~/.cargo/bin/cargo`). A Homebrew or distro `cargo`
earlier on `PATH` ignores it and has no wasm32 `std`, which fails the wasm legs
with a misleading "can't find crate for `core`". `scripts/check.sh` puts the
rustup toolchain first on `PATH` itself, so prefer it over bare `cargo`.

Optional, not required to build: `wasmtime` (runs the WASI tests — without it
`check.sh` builds that leg only), `wasm-pack` (bundles the dev console) and
`sbcl` (the differential-test oracle).

## Working agreements

- Do not silently change a design decision. If the code needs something the design does not cover, or contradicts it, stop and raise it, and propose an update to `docs/design.md`.
- Keep commits small and phase-focused. Each phase ends at a demonstrable gate (see the roadmap in `docs/design.md`); don't start work belonging to a later phase without asking.
