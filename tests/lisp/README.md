# Differential tests

Each `.lisp` file is ordinary Common Lisp. The same file runs on Quokka
interpreted, Quokka compiled (in the image `boot::build_image` makes, and again
in one that has rebuilt itself), each natively and under wasmtime, and on SBCL —
and all of them must produce the same output. The compiled runs read every form
with the image's Lisp reader and print with its Lisp printer (#47); the
interpreted run uses the Rust ones. **Tier 1** files use the library, which the
interpreter never learns, so they have no interpreted run and take their
baseline from SBCL. The [developer manual](../../docs/manual.md) has the
commands.

Three different comparisons share one file:

- **Quokka against SBCL** catches *we implemented Lisp wrong*. Semantics.
- **Quokka native against Quokka wasm32** catches *the VM behaves differently
  depending on where it runs*. Portability.
- **Quokka interpreted against Quokka compiled** catches *the compiler and the
  interpreter disagree* (#38). In the compiled run, each form is compiled and
  then run before the next is read, and a form that fails to compile counts as
  `!! error` — a deliberately malformed form errs at compile time compiled and
  at run time interpreted, and either way it is an error.

## Format

One line per top-level form:

```text
=> 42          the form's value, printed as the reader would want it back
!! error       the form signalled
```

Errors are flattened to one token on purpose. Quokka says "the function NOPE is
undefined", SBCL says something else with a condition class and a restart list,
and the text will never agree. Comparing condition *classes* would be stronger
and waits for the condition system; this still catches the case that matters,
which is one implementation signalling where the other does not.

The `.expected` files hold that output. They are the baseline the Rust test
compares against, and the SBCL job in CI checks them independently — so a
disagreement surfaces as a failing build rather than as nothing at all.

## Running them

```
cargo test -p quokka-vm --test differential          # Quokka, native
CARGO_TARGET_WASM32_WASIP1_RUNNER='wasmtime --dir=.' \
  cargo test -p quokka-vm --test differential --target wasm32-wasip1
./scripts/oracle.sh                                  # SBCL, needs sbcl installed
target/debug/quokka --run tests/lisp/arithmetic.lisp # regenerate a baseline
```

`scripts/check.sh` runs all of them it can and says which it skipped.

## What may not go in a test file

These kinds of form would disagree for reasons that are nobody's fault:

- **Anything needing a bignum.** SBCL has them and Quokka does not yet, so
  `(* 2432902008176640000 21)` is an error here and an answer there. That is a
  real difference and not one a test should be asserting about.
- **Printing a function.** SBCL prints `#<FUNCTION (LAMBDA (X)) {1001A2B3}>`,
  with an address. Nothing will ever match that.
- **Anything the standard leaves unspecified.** `(eq 1 1)` is the trap: the
  standard says `eq` on numbers is implementation-dependent, SBCL happens to
  answer `T`, and Quokka happens to agree. A test asserting it would pass for
  the wrong reason and would be testing an accident rather than a rule.
- **Binding or redefining a `COMMON-LISP` symbol.** A specific and non-obvious
  case of the above. `(flet ((car (x) ...)) ...)` is undefined behaviour under
  CLHS 11.1.2.1.2, and SBCL enforces it with a package lock. Quokka has no
  package locks and allows it, so the two disagree for a reason that is the
  test's fault. Use a name of your own; this file caught it the hard way.
  `scripts/check-cl-names.lisp` refuses such a file. The one exception is a
  `let` of a standard special variable, such as `*package*` or `*print-base*`,
  which the standard expects programs to bind.

- **Modifying a literal.** `(setf (aref "abc" 0) #\z)` is undefined under
  CLHS 3.7.1. Quokka makes source literals read-only and signals a
  `type-error`; SBCL's interpreter lets the write through. Both conform, and
  the disagreement is the standard's, not either implementation's. Use
  `(copy-seq "abc")` when a test needs a string it can write to. The refusal
  itself is tested in `crates/quokka-vm/src/boot.rs`, where behaviour the
  oracle cannot adjudicate belongs. See issue #26.

Adding a file means adding a line to `crates/quokka-vm/tests/differential.rs`
— `differential_cases!` for tier 0, `tier1_cases!` for tier 1;
`scripts/check-differential.sh` fails if one is left out. A tier-1 baseline
comes from SBCL: `sbcl --script scripts/oracle.lisp tests/lisp/NAME.lisp >
tests/lisp/NAME.expected`.
