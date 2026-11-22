# The Quokka developer manual

How to build Quokka, run it, rebuild it, test it, and add to it. Every
command here was run before it was written down; if one stops working, the
manual is wrong and should be fixed in the same commit as whatever changed it.

Three documents, three jobs:

| | answers |
| --- | --- |
| **this manual** | *how* do I do X? |
| [`docs/design.md`](design.md) | *what* is Quokka, and what has been decided? |
| [the book](../book/src/SUMMARY.md) | *why* is it built this way? |

Run everything from the **repository's root**. Some commands ask for files by
path relative to it.

---

## 1. Setting up

**Rust**, through `rustup`. `rust-toolchain.toml` pins a stable toolchain and
the two WebAssembly targets, and `rustup` installs them the first time `cargo`
runs here.

> **The `PATH` trap.** `rust-toolchain.toml` is read only by rustup's own
> `cargo`, the one in `~/.cargo/bin`. A Homebrew or distro `cargo` earlier on
> your `PATH` ignores it, has no WebAssembly standard library, and fails with
> a misleading *can't find crate for `core`*. `scripts/check.sh` puts rustup's
> toolchain first itself. For your own commands, either do the same or run:
>
> ```bash
> export PATH="$HOME/.cargo/bin:$PATH"
> ```

**Optional tools.** Nothing below is needed to build Quokka. Each one turns on
a part of the checks, and `check.sh` says which parts it skipped.

| tool | install (macOS) | what it adds |
| --- | --- | --- |
| `sbcl` | `brew install sbcl` | the **oracle**: every differential test is also run on SBCL, a real Common Lisp, and must agree |
| `wasmtime` | `brew install wasmtime` | runs the tests compiled to WebAssembly (`wasm32-wasip1`); without it they are only built |
| `wasm-pack` | `cargo install wasm-pack` | bundles the browser dev console |
| `mdbook` | `cargo install mdbook` | renders the book as a website; the Markdown reads fine without it |

---

## 2. Three ways to start Quokka

This is the thing most worth knowing, because the three look alike and are
not.

| command | what runs your input | what exists |
| --- | --- | --- |
| `cargo run -p quokka-repl` | the **interpreter**, in Rust | **tier 0** only: the prelude. No packages, no Lisp reader or printer, no `eval`, `load` or streams |
| `cargo run -p quokka-repl -- --compile` | the **compiler**, run by the interpreter, then the VM | the same tier 0, compiled form by form |
| `cargo run --release -p quokka-repl -- --image world.qi` | **the image's own Lisp**: its reader, compiler and printer | **everything**: tier 0, the compiled compiler, and the tier-1 library (packages, `read`, `print`, `eval`, `load`, streams, …) |

**`--image` is Quokka.** The other two are the **bootstrap** made visible:
they are how the first compiled world gets built, and they stay useful for
working on the interpreter or the compiler themselves.

The browser dev console (`crates/quokka-wasm/www`) runs the interpreter, as
the first row does.

```text
$ cargo run -q -p quokka-repl
quokka> (+ 1 2)
3
quokka> (find-package :cl)
; the function FIND-PACKAGE is undefined          ← the interpreter: tier 0

$ cargo run -q --release -p quokka-repl -- --image world.qi
quokka> (find-package :cl)
#<PACKAGE "COMMON-LISP">                           ← the whole system
```

Two more ways to look at the compiler at work:

```bash
cargo run -p quokka-repl -- --show-code
```

Every form you type is printed as bytecode before it runs.

```bash
cargo run -p quokka-repl -- --compile-file lisp/prelude.lisp
```

Compiles and verifies every form of a file, without installing any of it.

---

## 3. Making a world

A **world** is a heap with everything in it. An **image** is a world saved to a
file. There are two ways to make one, and between them they form the
**bootstrap round** that the phase-2 fixpoint checks.

Use `--release` for anything to do with images. A debug build takes about 40
seconds for the cold build and minutes for a rebuild; a release build takes 5
and 18 seconds.

### The cold build: a world from nothing

```bash
cargo run --release -p quokka-repl -- --save-image world.qi
```

`boot::build_image`, in Rust, runs these steps:
1. The interpreter reads the prelude and the compiler.
2. The interpreted compiler compiles both.
3. The compiled code is run, which installs it.
4. The library is loaded.
5. A heap scan proves nothing interpreted survived.

The world is then saved. Book chapter 32 explains it.

You can add your own files, compiled into the world before it is saved:

```bash
cargo run --release -p quokka-repl -- --save-image world.qi my-code.lisp
```

### Starting from an image

```bash
cargo run --release -p quokka-repl -- --image world.qi
```

Nothing is built and nothing is interpreted. The REPL reads, evaluates and
prints with the image's own Lisp. Inside it, `(load "path/to/file.lisp")`
loads a file, with the path relative to the directory you started in.

### The warm rebuild: a world remakes itself

```bash
cargo run --release -p quokka-repl -- --rebuild-image world.qi rebuilt.qi
```

This loads `world.qi` and calls `(quokka::%rebuild-world)`. The running world:
1. reads `lisp/prelude.lisp`, `lisp/compiler/compile.lisp` and every file in
   `lisp/library/` **from disk**, with its own reader;
2. compiles the prelude and the compiler with its own compiler, installing
   nothing until everything has compiled;
3. installs it all;
4. loads the library;
5. checks that every function the source defines was replaced.

The result is saved as `rebuilt.qi`. Book chapter 41 explains it.

A rebuild reads from disk, so this is how to try a change to the compiler or
the library without recompiling any Rust:

1. Edit `lisp/compiler/compile.lisp`, or any file in `lisp/library/`.
2. Run `--rebuild-image world.qi new.qi`.
3. Run `--image new.qi` and try the change.

The prelude and the compiler are also built into the binary for the cold
build. Your change reaches *that* only after the next `cargo build`.

### The bootstrap round

```bash
cargo run --release -p quokka-repl -- --save-image c1.qi
```

**C1** is made by the interpreted compiler.

```bash
cargo run --release -p quokka-repl -- --rebuild-image c1.qi c2.qi
```

**C2** is made by C1's compiled compiler.

```bash
cargo run --release -p quokka-repl -- --rebuild-image c2.qi c3.qi
```

**C3** is made by C2's compiler.

What to expect today:
- C1 is 359,688 bytes; C2 and C3 are 360,224 each.
- Every rebuild reports `(:FORMS 373 :REPLACED 316)`.
- `cmp c2.qi c3.qi` still reports differences, and that is expected. Gensyms
  are numbered by a counter that keeps rising, the image checksum covers those
  numbers, and the header counts saves.
- **The fixpoint** (phase 2's gate, chapter 42) ignores exactly those
  differences, and checks everything else:
  ```bash
  cargo test -p quokka-vm --test differential -- the_compiler_reproduces_itself a_rebuilt_world_is_the_same_world
  ```
  The first test compares every named compiled function in C1, C2 and C3. The
  second compares C2 and C3 word by word as whole worlds. Both build their own
  C1, C2 and C3 from the source in the binary. When one fails, it writes
  `target/fixpoint/c1.txt`, `c2.txt` and `c3.txt`, one description per
  function, and `diff` between them shows what changed.

---

## 4. Running the tests

### Everything CI runs

```bash
./scripts/check.sh
```

Run this before every commit. With Rust already built it takes about two and a
quarter minutes (134 seconds on the machine this was written on), so it is worth
running in a second terminal. It runs, in order:
1. formatting and clippy;
2. the native build and every test;
3. the book's structure, and the Rust line budget;
4. the differential test coverage, and the SBCL oracle;
5. rustdoc;
6. GC torture;
7. the WebAssembly builds and tests.

**Use its exit code, not its output.** Piping it into `grep` or `tail` hides a
failure, because the pipe succeeds whatever happened. The safe form is:

```bash
if ./scripts/check.sh > /tmp/check.log 2>&1; then echo ok; else tail -40 /tmp/check.log; fi
```

### Rust tests only

```bash
cargo test --workspace
```

```bash
cargo test -p quokka-vm --lib heap::
```

The second runs one module's unit tests.

```bash
cargo test -p quokka-vm --test differential tier1::reader
```

That runs one differential file. Test builds are optimised (`[profile.test]`
in `Cargo.toml`), because the time goes into running Lisp, not compiling Rust.

Tests live in two places, and the difference matters (CLAUDE.md, *Where tests
live*):
- **Unit tests**, in `crates/*/src/unit/`, can see private items.
- **Integration tests**, in `crates/*/tests/`, see only the public API, as a
  host does.

### Differential tests

`tests/lisp/*.lisp` are ordinary Common Lisp files, each with a `.expected`
baseline of one line per form (`=> value` or `!! error`). Each file runs in
several **legs**, and every leg must match the baseline:

| leg | test name | reads with | prints with |
| --- | --- | --- | --- |
| interpreted (tier 0 files only) | `lists` | the Rust reader | the Rust printer |
| compiled, cold-built world | `compiled::lists`, `tier1::reader` | the image's Lisp reader | the image's Lisp printer |
| compiled, rebuilt world | `a_rebuilt_world_passes_every_differential_file` | the same | the same |
| SBCL | `./scripts/oracle.sh` | SBCL | SBCL |

**Tier 0** files (17) run in every leg. **Tier 1** files (5) use the library,
which the interpreter never learns, so they skip the interpreted leg.
`tests/lisp/README.md` lists what a test file must not contain. In short: no
bignums, no printed functions, nothing the standard leaves unspecified, and no
binding of `COMMON-LISP` symbols other than the standard special variables.

To run SBCL on one file and see its output:

```bash
sbcl --script scripts/oracle.lisp tests/lisp/reader.lisp
```

### Other checks

| | |
| --- | --- |
| GC torture: collect at every mutation boundary, verify the heap | `cargo test -p quokka-vm --features gc-torture` |
| the tests under WebAssembly | `CARGO_TARGET_WASM32_WASIP1_RUNNER='wasmtime --dir=.' cargo test -p quokka-vm --target wasm32-wasip1` |
| the browser build | `cargo build -p quokka-vm -p quokka-wasm --target wasm32-unknown-unknown` |
| the Rust line budget (15,000, tests not counted) | `./scripts/check-budget.sh` |

The slowest tests are the rebuild and fixpoint tests, about 40 seconds
together. They rebuild the system twice, and share the two rebuilt worlds.

**If the fixpoint fails** after a change to the compiler, the prelude or the
library, the change made the compiler compile itself differently. Read
`diff target/fixpoint/c1.txt target/fixpoint/c2.txt`, and find out why before
doing anything else. Chapter 34 gives one way this can happen: a tier-1 macro
that expands differently from the prelude's.

---

## 5. Adding things

### A differential test

1. Write `tests/lisp/NAME.lisp`.
2. Make its baseline. **Tier 0**, from Quokka's interpreter:
   ```bash
   cargo run -p quokka-repl -- --run tests/lisp/NAME.lisp > tests/lisp/NAME.expected
   ```
   **Tier 1**, from SBCL, since the interpreter cannot run it:
   ```bash
   sbcl --script scripts/oracle.lisp tests/lisp/NAME.lisp > tests/lisp/NAME.expected
   ```
3. List it in `crates/quokka-vm/tests/differential.rs`: tier 0 in
   `differential_cases!`, tier 1 in `tier1_cases!`.
   `scripts/check-differential.sh` fails if you forget.
4. Read the baseline before committing it. A baseline is a claim about what is
   *right*. For tier 0, `./scripts/oracle.sh` then checks it against SBCL.

### A file in the library

The tier-1 library, `lisp/library/`, is loaded in a fixed order by **two**
builders, and both lists must change:

1. `boot::LIBRARY` in `crates/quokka-vm/src/boot.rs`, for the cold build.
2. `%library-files` in `lisp/library/rebuild.lisp`, for the warm rebuild. A
   test (`the_rebuild_reads_the_library_the_cold_build_loads`) fails if the
   two lists differ.

**`packages.lisp` stays last.** Its final form puts every symbol the Rust
reader has made into a package, and only symbols that exist by then are
placed. A special variable must be **proclaimed before anything binds it**: a
`let` of a variable not yet `defvar`'d compiles to an ordinary local
(chapter 40).

### A primitive

Rule 1 says Lisp first, so check whether it can be Lisp. If it can't:
1. Add a `Prim` variant, its row in `TABLE` (`crates/quokka-vm/src/primitives.rs`),
   and its implementation.
2. Add its name to the pinned list in `src/unit/primitives.rs`.

It costs Rust budget (1,324 lines are left), and the issue for the step should
say how much.

### A book chapter

1. Add it to `book/src/SUMMARY.md`.
2. Add a row to the table in `book/src/what-has-to-be-built.md`.
3. Add new terms to `book/src/glossary.md`.

`scripts/check-book.sh` checks that every chapter listed exists. CLAUDE.md,
*The book*, has the rules for writing one.

---

## 6. How work is done here

- **An issue before code.** Every step or decision gets an issue in
  `quokka-lisp/quokka-lisp` first, with the options written out. Decisions
  reached in discussion go into it as comments.
- **A commit references its issue**, and closes it where it fits.
- **`check.sh` passes before every commit.** CLAUDE.md rule 2: native and
  WebAssembly, every commit.
- **Keep this manual true.** A step that changes a command, a flag or a file
  list changes this manual in the same commit.

---

## 7. When something goes wrong

| symptom | cause | what to do |
| --- | --- | --- |
| *can't find crate for `core`* | a non-rustup `cargo` on your `PATH` (§1) | put `~/.cargo/bin` first |
| *image format 2, but this build reads 3* | the image format changed since the file was saved | make a new one with `--save-image` |
| `--rebuild-image` fails with *not found: lisp/prelude.lisp* | run from somewhere other than the repository's root | `cd` to the root |
| *gave up after the step budget* | the form ran longer than the REPL allows one line, usually an infinite loop | the budget is `EVAL_BUDGET` in `crates/quokka-repl/src/main.rs` |
| `check.sh` "passed" but CI failed | its output was piped, and the pipe's success hid the failure (§4) | use the exit-code form |
