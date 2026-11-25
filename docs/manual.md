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

## 2. The `quokka` program

Everything is one program, `quokka`, built by the crate of the same name in
`crates/quokka`. Run it through
cargo, with `--release` for anything to do with worlds:

```bash
cargo run --release -p quokka -- ARGUMENTS
```

or build it once with `cargo build --release` and run `target/release/quokka`.

| command | does |
| --- | --- |
| `quokka WORLD.qi` | **run a world**: a REPL in which every form is read, evaluated and printed by the world's own Lisp. This is Quokka |
| `quokka build OUT.qi [SRC...]` | make a world from nothing, the cold build (§3) |
| `quokka rebuild IN.qi OUT.qi` | have a world rebuild itself from the files on disk (§3) |
| `quokka bootstrap` | the **tier-0 interpreter** that makes the first world. Not Quokka itself: no packages, no Lisp reader, printer or `eval` |
| `quokka bootstrap --compile` / `--show-code` | the same, with each form compiled by the bootstrap compiler; `--show-code` prints the bytecode first |
| `quokka bootstrap --run FILE` | interpret a file and print the differential format (tier-0 baselines, §5) |
| `quokka bootstrap --compile-file FILE` | compile and verify every form of a file, installing none of it |

`quokka` on its own refuses, because Quokka always runs *from* a world, and
says how to make one. `quokka help` prints the table above.

### Where the code is

| crate | holds |
| --- | --- |
| `crates/quokka-vm` | the machine a world runs on: values, heap, collector, bytecode VM, primitives, images, the host protocol. No interpreter: it cannot reach one (#29) |
| `crates/quokka-bootstrap` | what only the cold build needs: the Rust reader, the tree-walking interpreter, `boot::build_image`, and the whole-system tests (`tests/differential.rs`) |
| `crates/quokka` | the `quokka` program |
| `crates/quokka-wasm` | the browser host and its dev console |
| `lisp/` | everything above the primitive table: the prelude, the compiler, the tier-1 library |

### Working in a world

```text
$ quokka build world.qi
world.qi: 359688 bytes, 44914 words, sequence 1

$ quokka world.qi
Quokka 0.0.0 — phase 2 · Self-hosting
world.qi: 44914 words, sequence 1 · compiled · tier 1
:save FILE saves this world · Ctrl-D leaves
quokka> (defun hello () :saved-in-the-world)
HELLO
quokka> (find-package :cl)
#<PACKAGE "COMMON-LISP">
quokka> :save mine.qi
mine.qi: 360416 bytes, 45005 words, sequence 2
quokka> ^D

$ quokka mine.qi
quokka> (hello)
:SAVED-IN-THE-WORLD
```

- **The banner says what is running**: which world, how many times it has
  been saved (its sequence), compiled, tier 1. The bootstrap interpreter says
  it is the bootstrap and prompts `boot>` instead of `quokka>`, so the two
  can't be confused.
- **`:save FILE`** saves the world as it is between two forms: your
  definitions, your packages, your data. It is a REPL command, handled
  between forms, so a world is never saved halfway through a computation. A
  line starting with `:` is a command; everything else is Lisp.
- **`(load "path/to/file.lisp")`** loads a file into the world, with the path
  relative to the directory you started in.
- **A saved world keeps working across a rebuild.** `quokka rebuild mine.qi
  new.qi` replaces the system's own functions and leaves yours alone, since
  they are not in the system's source.

### The bootstrap, for working on the interpreter or the compiler

```text
$ quokka bootstrap
Quokka 0.0.0 — bootstrap · the tier-0 interpreter, which makes the first world
Not Quokka itself: no packages, no Lisp reader, printer or eval. To run Quokka, give it a world: quokka WORLD.qi
boot> (+ 1 2)
3
boot> (find-package :cl)
; the function FIND-PACKAGE is undefined          ← tier 0 has no packages
```

`quokka bootstrap --show-code` prints every form you type as bytecode before
running it.

### In the browser

The browser runs a world too (#84), and carries no interpreter: the page
fetches an image and the machine runs it. From the repository root:

```bash
cargo run --release -p quokka -- build crates/quokka-wasm/www/world.qi
wasm-pack build crates/quokka-wasm --target web --out-dir www/pkg
python3 -m http.server -d crates/quokka-wasm/www 8080
```

and open `http://localhost:8080`. A line runs in slices of about eight
milliseconds, so the page stays live while it does; **Stop** gives up a line
that will not end, and **Save world** downloads the world as an image —
serve it as `world.qi` to carry on from it. The page's script is the host:
it sizes each slice from how long the last one took, and collects what the
world wrote with `take_output`. `crates/quokka-wasm/src/lib.rs` says why it
is shaped that way.

---

## 3. Making a world

A **world** is a heap with everything in it. An **image** is a world saved to a
file. There are two ways to make one, and between them they form the
**bootstrap round** that the phase-2 fixpoint checks.

Use `--release`. A debug build takes about 40 seconds for the cold build and
minutes for a rebuild; a release build takes 5 and 18 seconds. The commands
below are written as `quokka ...`; through cargo they are
`cargo run --release -p quokka -- ...`.

### The cold build: a world from nothing

```bash
quokka build world.qi
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
quokka build world.qi my-code.lisp
```

### The warm rebuild: a world remakes itself

```bash
quokka rebuild world.qi rebuilt.qi
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
2. Run `quokka rebuild world.qi new.qi`.
3. Run `quokka new.qi` and try the change.

The prelude and the compiler are also built into the binary for the cold
build. Your change reaches *that* only after the next `cargo build`.

### The bootstrap round

```bash
quokka build c1.qi
```

**C1** is made by the interpreted compiler.

```bash
quokka rebuild c1.qi c2.qi
```

**C2** is made by C1's compiled compiler.

```bash
quokka rebuild c2.qi c3.qi
```

**C3** is made by C2's compiler.

What to expect today:
- C1 is 976,856 bytes; C2 and C3 are 978,464 each. A rebuild takes about
  seventeen seconds.
- Every rebuild reports `(:FORMS 1290 :REPLACED 1035)`. It runs as one form
  with sixty times an ordinary form's step budget (300 million steps) and
  needs about 201 million; a rebuild that runs out says `BudgetExhausted`.
- `cmp c2.qi c3.qi` still reports differences, and that is expected. The
  library's `gensym` starts from `*gensym-counter*` = 0 in every rebuild, so
  its names repeat exactly (#72), but the machine still makes a few gensyms of
  its own (rewriting lambda lists) from a counter that keeps rising; the image
  checksum covers those names, and the header counts saves.
- **The fixpoint** (phase 2's gate, chapter 42) ignores exactly those
  differences, and checks everything else:
  ```bash
  cargo test -p quokka-bootstrap --test differential -- the_compiler_reproduces_itself a_rebuilt_world_is_the_same_world
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
half minutes (152 seconds on the machine this was written on), so it is worth
running in a second terminal. It runs, in order:
1. the book's structure;
2. formatting and clippy;
3. the native build and every test;
4. the Rust line budget;
5. the differential test coverage, and the SBCL oracle;
6. rustdoc;
7. GC torture;
8. the WebAssembly builds and tests.

The book comes first because it takes a moment, and a missing chapter should
not have to wait for two and a half minutes of compiling to be reported.

**Use its exit code, not its output.** Piping it into `grep` or `tail` hides a
failure, because the pipe succeeds whatever happened. The safe form is:

```bash
if ./scripts/check.sh > /tmp/check.log 2>&1; then echo ok; else tail -40 /tmp/check.log; fi
```

### One half at a time

The prose changes far more often than the VM underneath it, so the two halves
can be run apart:

```bash
./scripts/check.sh book
```

```bash
./scripts/check.sh code
```

`book` runs the book checks alone. It uses no cargo, so it works in a checkout
with no Rust toolchain installed, and it finishes at once. `code` runs
everything else. With no argument you get both, which is what to run before a
commit; CI runs the book as a job of its own in either case.

### Rust tests only

```bash
cargo test --workspace
```

```bash
cargo test -p quokka-vm --lib heap::
```

The second runs one module's unit tests.

```bash
cargo test -p quokka-bootstrap --test differential tier1::reader
```

That runs one differential file. Test builds are optimised (`[profile.test]`
in `Cargo.toml`), because the time goes into running Lisp, not compiling Rust.

Tests live in two places, and the difference matters (CLAUDE.md, *Where tests
live*):
- **Unit tests**, in `crates/*/src/unit/`, can see private items.
- **Integration tests**, in `crates/*/tests/`, see only the public API, as a
  host does.
- The tests of `host`, `image` and `lambda_list` that set their cases up with
  the interpreter are integration tests in `crates/quokka-bootstrap/tests/`,
  since `quokka-vm` cannot reach the interpreter (#29).

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

**Tier 0** files (17) run in every leg. **Tier 1** files (27) use the library,
which the interpreter never learns, so they skip the interpreted leg.
`tests/lisp/README.md` lists what a test file must not contain. In short: no
bignums in a tier-0 file, no printed functions, nothing the standard leaves unspecified, and no
binding of `COMMON-LISP` symbols other than the standard special variables.

To run SBCL on one file and see its output:

```bash
sbcl --script scripts/oracle.lisp tests/lisp/reader.lisp
```

### Other checks

| | |
| --- | --- |
| GC torture: collect at every mutation boundary, verify the heap | `cargo test -p quokka-vm -p quokka-bootstrap --features quokka-vm/gc-torture` |
| the tests under WebAssembly | `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=. --dir=$PWD" cargo test -p quokka-vm -p quokka-bootstrap --target wasm32-wasip1` |
| the browser build | `cargo build -p quokka-vm -p quokka-wasm --target wasm32-unknown-unknown` |
| the Rust line budget (15,713, tests not counted) | `./scripts/check-budget.sh` |

The slowest tests are the rebuild and fixpoint tests, about 40 seconds
together. They rebuild the system twice, and share the two rebuilt worlds.

**If the fixpoint fails** after a change to the compiler, the prelude or the
library, the change made the compiler compile itself differently. Read
`diff target/fixpoint/c1.txt target/fixpoint/c2.txt`, and find out why before
doing anything else. Chapter 34 gives one way this can happen: a tier-1 macro
that expands differently from the prelude's. Chapter 43 shows how the
library's `setf` avoids it: a place the prelude knows goes to the prelude's
own `%setf-known-place`. Chapter 53 gives another: a compiler macro
(`define-compiler-macro`) in the library for a function the compiler calls.
The compiler applies compiler macros, so one would rewrite the compiler's
own calls in C2 and not in C1. Keep the library free of them.

### ansi-test: conformance, measured

ansi-test is the Common Lisp conformance suite (#60). It is not part of
`check.sh`: it takes about five minutes and fetches the suite over the
network the first time, so it has a script and a CI job of its own.

```bash
scripts/ansi-test.sh
```

The script clones the suite into `target/ansi-test/` at the commit it pins,
runs the fifteen groups tier 1 is measured by (`arrays` joined in #67), and fails if a test listed in
`tests/ansi/baseline.txt` no longer passes: that list only grows. When a
change makes more tests pass, record them:

```bash
ANSI_TEST_UPDATE=1 scripts/ansi-test.sh
```

That rewrites the baseline and the summary in `docs/ansi-test.md`; commit
both with the change. Every result, with its detail, is in
`target/ansi-test-results.txt`. A test reported as *needs X* uses something
Quokka lacks; whether X is deferred or a gap in tier 1 is decided by
`tests/ansi/deferred.lisp`. `tests/ansi/README.md` explains the files and how
to run a few test files only. To run a few groups as the full run reaches
them, and see what does not pass, without touching the baseline:

```bash
ANSI_TEST_GROUPS=cons,packages cargo test --release -p quokka-bootstrap --test ansi -- --ignored --nocapture
```

A failure SBCL shares is listed in `tests/ansi/deferred.lisp` with
`%follows-sbcl`, after asking SBCL, and then counts as explained.

---

## 5. Adding things

### A differential test

1. Write `tests/lisp/NAME.lisp`.
2. Make its baseline. **Tier 0**, from Quokka's interpreter:
   ```bash
   cargo run -p quokka -- bootstrap --run tests/lisp/NAME.lisp > tests/lisp/NAME.expected
   ```
   **Tier 1**, from SBCL, since the interpreter cannot run it:
   ```bash
   sbcl --script scripts/oracle.lisp tests/lisp/NAME.lisp > tests/lisp/NAME.expected
   ```
3. List it in `crates/quokka-bootstrap/tests/differential.rs`: tier 0 in
   `differential_cases!`, tier 1 in `tier1_cases!`.
   `scripts/check-differential.sh` fails if you forget.
4. Read the baseline before committing it. A baseline is a claim about what is
   *right*. For tier 0, `./scripts/oracle.sh` then checks it against SBCL.

### A file in the library

The tier-1 library, `lisp/library/`, is loaded in a fixed order by **two**
builders, and both lists must change:

1. `boot::LIBRARY` in `crates/quokka-bootstrap/src/boot.rs`, for the cold build.
2. `%library-files` in `lisp/library/rebuild.lisp`, for the warm rebuild. A
   test (`the_rebuild_reads_the_library_the_cold_build_loads`) fails if the
   two lists differ.

Two library files are data generated once from SBCL, and are regenerated by
their scripts rather than edited: `cl-symbols.lisp`
(`sbcl --script scripts/generate-cl-symbols.lisp`) and `char-names.lisp`, the
names of the 256 Latin-1 characters
(`sbcl --script scripts/generate-char-names.lisp > lisp/library/char-names.lisp`).

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

It costs Rust budget (none is left of 15,713, which is a guide rather than a
ceiling: raise `LIMIT` in `scripts/check-budget.sh` in the same commit, with
the reason), and the issue for the step should say how much.

### A book chapter

1. Add it to `book/src/SUMMARY.md`.
2. Add a row to the table in `book/src/what-has-to-be-built.md`.
3. Add new terms to `book/src/glossary.md`, which is in alphabetical order,
   each with an `<a id="..."></a>` line so that other entries can link to it.

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
| *image format 5, but this build reads 6* | the image format changed since the file was saved | make a new one with `quokka build` |
| `quokka rebuild` fails with *not found: lisp/prelude.lisp* | run from somewhere other than the repository's root | `cd` to the root |
| *gave up after the step budget* | the form ran longer than the REPL allows one line, usually an infinite loop | the budget is `EVAL_BUDGET` in `crates/quokka/src/main.rs` |
| `quokka: no world to run` | `quokka` was started without a world | `quokka build world.qi` once, then `quokka world.qi` |
| `check.sh` "passed" but CI failed | its output was piped, and the pipe's success hid the failure (§4) | use the exit-code form |
