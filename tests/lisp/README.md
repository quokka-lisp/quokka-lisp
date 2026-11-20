# Differential tests

Each `.lisp` file is ordinary Common Lisp. The same file runs three ways —
Quokka natively, Quokka under wasmtime, and SBCL — and all three must produce
the same output.

Two different comparisons share one file:

- **Quokka against SBCL** catches *we implemented Lisp wrong*. Semantics.
- **Quokka native against Quokka wasm32** catches *the VM behaves differently
  depending on where it runs*. Portability.

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

Three kinds of form would disagree for reasons that are nobody's fault:

- **Anything needing a bignum.** SBCL has them and Quokka does not yet, so
  `(* 2432902008176640000 21)` is an error here and an answer there. That is a
  real difference and not one a test should be asserting about.
- **Printing a function.** SBCL prints `#<FUNCTION (LAMBDA (X)) {1001A2B3}>`,
  with an address. Nothing will ever match that.
- **Anything the standard leaves unspecified.** `(eq 1 1)` is the trap: the
  standard says `eq` on numbers is implementation-dependent, SBCL happens to
  answer `T`, and Quokka happens to agree. A test asserting it would pass for
  the wrong reason and would be testing an accident rather than a rule.

Adding a file means adding a line to `crates/quokka-vm/tests/differential.rs`;
`scripts/check-differential.sh` fails if one is left out.
