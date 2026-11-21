# Quokka — notes for Claude Code

Quokka is a modern Lisp-machine-style environment: a small Rust VM running a Common Lisp subset, browser-first via WASM, with a live, presentation-based UI. The full design is in `docs/design.md` — read it before making structural changes.

Current phase: **1 · Bytecode and image** (#23), well into it. Phase 0's gate
passed, **tier 0 is finished** (#18), and phase 1's gate — *an image saves,
reloads and resumes where it left off* — **is met**. Phase 1 itself is not
finished; see "What is left" below.

## What exists

**Tier 0**, which was the precondition for everything else because it is
*enough Lisp that the compiler can be interpreted*: **22 of Common Lisp's 25
special operators** (the missing three are `progv`, `load-time-value` and
`symbol-macrolet`, excluded by `docs/design.md` on purpose), plus `lambda` and
`defmacro`, which CL makes macros and Quokka has not. Macros with backquote,
`gensym`, `macroexpand`, `macrolet` and destructuring lambda lists;
`&optional`/`&key` with default forms and supplied-p; keywords; vectors and
strings as sequences; hash tables keyed by content; a condition system
(`error`, `handler-case`, `ignore-errors`); `defstruct` with an extensible
`setf`; and multiple values.

`setf` was pulled forward from tier 1 because without it mutation could not be
tested against SBCL at all — which is the recurring shape of these decisions:
the thing that decides is testability against the oracle.

A **prelude** (`lisp/prelude.lisp`, 658 lines, compiled into the binary with
`include_str!` and interpreted at boot) holding `defun`, `when`, `unless`,
`cond`, `and`, `or`, `setf`, `defstruct`, `macroexpand`, `handler-case`,
`multiple-value-bind`, the list and sequence functions and the hash-table
wrappers — Lisp, not Rust.

**Phase 1, steps A to C, done:**

| | |
| --- | --- |
| `docs/bytecode.md` | the instruction set on paper: 37 opcodes, frame layout, every ceiling (#27) |
| `src/code.rs` | encoding, the `Function` object, disassembler, assembler, **verifier** (#28) |
| `src/vm.rs` | the machine; implements `Steppable`, so `run_slice` drives it and rule 4 holds by construction |
| `lisp/compiler/compile.lisp` | the compiler, 464 lines of tier-0 Lisp (#31) |
| `src/image.rs` | save and load a whole heap, mid-computation included |
| `src/host.rs` | **host requests**: a program asks the outside world by leaving a request in the heap and blocking; the host answers between slices (#35). First caller: `%read-source`. The interpreter has no I/O |

The compiler compiles **every form of the prelude and every form of itself**
— 85 functions, all accepted by the verifier. `boot::compile_source` does that
and installs none of it; chapter 29 says why installing cannot happen in a
running interpreter.

**310 tests**; **17 differential files agree with SBCL**. The Rust core is
**12,336** lines of the 15,000 budget.

## What is left in phase 1

**The world inside an image is still interpreted.** `image::save` writes
whatever heap it is given, and the only heap that exists is one built by
interpreting the prelude — so `length` in a saved image is a tree being
walked, not bytecode.

- **#33 · step D**: compile the world and *keep* the results. Two passes
  (compile everything, then run everything on the VM, which is what installs),
  and macros installed twice — interpreted so the compiler can expand with
  them, compiled so the finished image can call them. **One decision is open:
  whether the build driver lives in Rust or in Lisp. Raise it before writing.**
- **step E**: the fourth differential leg, where interpreted and compiled
  results must agree. The verifier cannot see a program that is well-formed
  and computes the wrong thing.
- **Compiled code cannot catch a primitive's error.** `condition::from_eval_error`
  is called only by the interpreter; the VM's `fail` halts. So
  `(handler-case (car 5) (type-error () …))`, compiled, stops the machine.
  Found while building #35, not fixed there. #33 and step E will both hit it.

Also open: #34 (concurrency — what the code must not foreclose before phase
3's processes; item 1, the host interface, is done as #35), #29 (build the VM
without the interpreter baked in, so *one
evaluator at a time* is a build error rather than a rule in prose), #24 (what
records the work *between* two images — the header's sequence number is
reserved, the rest is not decided), #26 (string interning).

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

## How work is tracked

**Every development step is a GitHub issue before it is code.** Not an internal
agenda, not a plan that lives only in a chat window — an issue in
`quokka-lisp/quokka-lisp`, so the project survives being put down for a fortnight
and picked up again.

- **One issue per step or decision.** Open it before starting, with the
  background and the options written out. Issue #9 tracks phase 0 and links the
  rest.
- **Discussion outcomes go in the comments.** When a question is worked through
  — why an approach was rejected, what a test turned up, what a constraint
  actually costs — write the conclusion into the issue as a comment. A decision
  argued and then lost to scrollback has to be argued again.
- **Reference the issue in the commit**, and close it from there where it fits.
- **Labels:** `phase-0`…`phase-6` for stage, `decision` for an open design
  question, and `vm` / `lisp` / `book` / `infra` for area. Milestones mirror the
  phases in `docs/design.md`, each with its gate.
- **A `decision` issue is not closed by writing code.** It closes when the
  reasoning is recorded in `docs/design.md` (and in the book, if it has a
  chapter).

## The book

`book/` holds *The Road to Quokka*, written alongside the system in the spirit of
Tanenbaum's operating systems books. Plain Markdown; `book.toml` lets mdBook
build it as a site, but mdBook is not required for anything and is not in CI.

- **A chapter lands when the thing it describes lands.** Not afterwards from
  memory — the reasoning is the content, and it is perishable.
- **Write from the real questions.** Chapters come out of questions actually
  asked during development, including the ones that revealed a wrong assumption.
  Those are the good ones.
- **Record what a decision cost**, not only what was chosen. A chapter that
  presents the winning option as obvious teaches nothing.
- **Say when something is unbuilt or undecided.** Chapters carry a status note
  rather than pretending. An open `decision` issue and its chapter stay in sync.
- **Code in the book is real code** from the repo, shortened where it helps but
  never invented.
- **Assume no prior knowledge, including the vocabulary.** A reader may not know
  what a garbage collector, a REPL, a heap, `car` or `cdr` is. Explain a term the
  first time it appears, add it to `book/src/glossary.md`, and never let jargon
  stand unexplained because it feels basic. The jargon is the wall.
- **Every chapter opens with a "Where this fits" note**: what is already
  established, what this chapter adds, and what it unlocks. A reader who cannot
  see the shape of the whole cannot tell why a chapter exists.
- **`book/src/what-has-to-be-built.md` is the map** — the parts of a language
  runtime, and why they are built in that order. When a new chapter changes the
  order or adds a part, update it and the chapter table in it.
- `scripts/check.sh` verifies every chapter in `book/src/SUMMARY.md` exists.

## Rules that must not be broken

1. **Rust below the primitive table, Lisp above it.** Rust provides only the VM: tagged words, heap, GC, image save/load, the bootstrap interpreter, bytecode execution, and host glue. Anything a user could want to change belongs in Lisp. If you are about to add a feature in Rust, check whether it belongs above the line.
2. **WASM on every commit.** Every change must build and pass tests for both native and `wasm32`. The core VM crate has no host dependencies: no `std::fs`, no threads, no wall clock, no networking. Host services are reached through a host interface implemented separately for native and WASM.
3. **Heap references are offsets, never raw pointers.** Store references relative to the heap base so images relocate and the same code runs on 64-bit native and wasm32. Follow the tagged-word scheme in `docs/design.md` (low 3 bits: `xx0` fixnum, `001` cons, `011` heap object, `101` immediate, `111` GC forwarding).
4. **The VM must be able to yield.** The interpreter runs in slices and returns control to its caller (the browser event loop). No unbounded blocking loops in the core. Execution state therefore lives in our heap as explicit frames, never on the Rust call stack — no recursive `eval`. The caller decides when a slice ends by passing a step budget to `run_slice`; the VM has no clock and no yield callback. Slicing must stay semantically invisible: collect on allocation rather than on slice boundaries, and never expose the remaining budget to Lisp. See `crates/quokka-vm/src/slice.rs`.
5. **The tier 0 rule.** Lisp sources for the compiler and core library use only tier 0 of the CL subset: no `loop`, no CLOS, only basic `format`, no `progv`, `load-time-value` or `symbol-macrolet`. The phase 0 interpreter only needs to support tier 0.
6. **Common Lisp semantics.** Quokka is a CL subset (a Lisp-2), not a new dialect. Where Quokka implements a CL feature, behaviour must match SBCL. Quokka-specific extensions go in their own packages, never in `COMMON-LISP`.
7. **Stay comprehensible.** Budget: the Rust core stays under **15,000 lines**, which `scripts/check-budget.sh` enforces in CI. Prefer clear code over clever code, keep dependencies few and permissively licensed, and justify every `unsafe` block with a comment explaining why it is sound.

   **Tests do not count, and live in directories of their own.** The budget is about how much of the system a person must read to understand it; a test is evidence that it works, not part of how it works. A `#[test]` outside the two directories in *Where tests live* fails the build.
8. **Licence.** MIT. Do not add dependencies or copied code with incompatible (e.g. copyleft) licences.
9. **Primitives are bounded.** A step budget only bounds time if a single step is short, and once control is inside a Rust primitive nothing can interrupt it. Every primitive must either complete quickly or be chunked so it can be resumed across slices. A primitive whose cost grows with the size of its argument — sorting, searching, big-number arithmetic, hashing a large string — needs a plan for this before it is added.

## Testing

- **Differential tests:** `tests/lisp/*.lisp` are ordinary Common Lisp. The same file runs on Quokka native, Quokka under wasmtime, and SBCL, and all three must agree — Quokka against SBCL catches *we implemented Lisp wrong*, Quokka native against wasm32 catches *the VM differs by target*. SBCL is the oracle, not a build dependency: without it that leg skips and says so. A file must not depend on bignums, print a function, or test anything the standard leaves unspecified; `tests/lisp/README.md` says why.
- **Rust tests:** unit and property tests for tagged words, the heap, the reader and the GC.
- **GC torture mode:** a build or runtime flag that collects on every allocation. Run it in CI.
- **WASM:** run the same tests under wasmtime (WASI) and in a headless browser.
- Before Quokka has file hosts, Lisp source files are loaded through a single host primitive (`std::fs` natively, WASI in WASM tests, bundled files in the browser).

## Where tests live

Two directories with confusingly similar names, and they are different things
rather than a duplication:

| | | |
| --- | --- | --- |
| `crates/*/src/unit/` | **unit tests** | Pulled into their module by `#[cfg(test)] #[path = "unit/<name>.rs"] mod tests;`, so each file is still a child of what it tests and **sees private items**. That is why they are a separate *file* and not a separate crate: a separate crate reaches only the public API, and making `TABLE`, `Vm`'s fields and the heap's internals public to allow that would widen the public surface — which is itself part of what a reader has to understand. |
| `crates/*/tests/` | **integration tests** | Cargo's own directory, compiled as a separate crate that reaches **only the public API**. That is the point of it: `differential.rs` drives the whole system through `run_forms` alone, exactly as a host does. |

They are not merged, and the directory that could be renamed was. `src/unit/`
was `src/tests/` until two directories called `tests` in one crate proved
confusing to read. The integration test could technically move in beside the
unit tests, since it happens to use a single public function — but that would
give up the only check that the public entry point is sufficient on its own.

## Proposed layout (adjust if a better one emerges, and update this file)

```
crates/quokka-vm      core VM: tagged words, heap, GC, reader, interpreter (no host dependencies)
crates/quokka-repl    native terminal REPL binary
crates/quokka-wasm    WASM bindings plus a minimal browser dev console page
lisp/                 Lisp sources: compiler and core library (tier 0 only)
tests/lisp/           differential test files
docs/design.md        the design
book/                 The Road to Quokka, written alongside the system
scripts/check.sh      everything CI runs, in one command
.github/workflows/    CI: native, GC torture, both wasm targets, rustdoc, SBCL oracle
```

## Commands

`./scripts/check.sh` runs everything CI runs. Use it before every commit — it is
the cheapest way to honour rule 2. It takes about ten seconds.

**Tests run optimised** (`[profile.test]` in the workspace `Cargo.toml`), because
the time goes into *running* them rather than compiling them: this is an
interpreter booting a six-hundred-line prelude dozens of times, and a debug
build made that eight times slower. `cargo build` and the REPL keep the
ordinary unoptimised profile.

**Honour its exit code.** Piping it through `grep` for the success line hides a
failure, because `grep | tail` exits 0 whether or not `grep` matched, and an
`&&` chain then carries on to the commit. That exact mistake pushed a
formatting failure to CI. Run it as:

```bash
if ./scripts/check.sh > /tmp/check.log 2>&1; then echo ok; else tail -40 /tmp/check.log; fi
```

| What | Command |
| --- | --- |
| Everything CI runs | `./scripts/check.sh` |
| Build (native) | `cargo build --workspace` |
| Test (native) | `cargo test --workspace` |
| REPL | `cargo run -p quokka-repl` — interprets |
| REPL, compiling | `cargo run -p quokka-repl -- --compile` — loads `lisp/compiler/`, compiles each form and runs it on the bytecode VM |
| REPL, showing the code | `cargo run -p quokka-repl -- --show-code` — the same, printing each form's instructions first |
| Compile a file | `cargo run -p quokka-repl -- --compile-file lisp/prelude.lisp` — compiles and verifies every form, installing none of it (see `boot::compile_source`) |
| Save an image | `cargo run -p quokka-repl -- --save-image world.qi [SRC...]` — boots, loads any sources, writes the world |
| Start from an image | `cargo run -p quokka-repl -- --image world.qi` — does not interpret the prelude at all |
| GC torture | `cargo test -p quokka-vm --features gc-torture` — collects at every mutation boundary and verifies the heap afterwards |
| Differential tests | `cargo test -p quokka-vm --test differential` (Quokka) · `./scripts/oracle.sh` (SBCL) |
| Regenerate a baseline | `cargo run -p quokka-repl -- --run tests/lisp/NAME.lisp > tests/lisp/NAME.expected` |
| Build for the browser | `cargo build -p quokka-vm -p quokka-wasm --target wasm32-unknown-unknown` |
| Test under WASI | `CARGO_TARGET_WASM32_WASIP1_RUNNER='wasmtime --dir=.' cargo test -p quokka-vm --target wasm32-wasip1` |
| Dev console bundle | `wasm-pack build crates/quokka-wasm --target web --out-dir www/pkg` |
| Serve the dev console | `python3 -m http.server -d crates/quokka-wasm/www 8080` |
| Format and lint | `cargo fmt --all` · `cargo clippy --workspace --all-targets -- -D warnings` |
| Line budget | `./scripts/check-budget.sh` — rule 7, counting core and tests separately |
| Docs | `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --open` |
| Preview the book | `mdbook serve book --open` (needs `cargo install mdbook`; optional) |

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
