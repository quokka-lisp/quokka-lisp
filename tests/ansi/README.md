# Quokka against ansi-test

ansi-test is the Common Lisp conformance suite Paul Dietz wrote for GCL, now
maintained at <https://gitlab.common-lisp.net/ansi-test/ansi-test> (MIT
licence). Quokka measures tier 1 against fifteen of its groups (#60, #67). The
latest numbers are in [`docs/ansi-test.md`](../../docs/ansi-test.md).

```bash
scripts/ansi-test.sh                      # run; fail if a test that passed no longer does
ANSI_TEST_UPDATE=1 scripts/ansi-test.sh   # run; rewrite baseline.txt and docs/ansi-test.md
```

The script clones the suite into `target/ansi-test/` at a pinned commit (the
`PIN` in the script) the first time; nothing of it is committed. A run takes
about five minutes.

| file | what it is |
| --- | --- |
| `runner.lisp` | the runner, in tier-1 Lisp: `deftest`, the comparison, the helpers tests call (`signals-error` …), and the scan that reports a test as *needs X* |
| `deferred.lisp` | operators Quokka lacks on purpose, each with the tier or issue it waits for; any other missing operator is a gap in tier 1 |
| `cl-operators.lisp` | the names of `COMMON-LISP`'s functions, macros and special operators, generated from SBCL by `scripts/generate-cl-operators.lisp` |
| `baseline.txt` | every test that passed at the last update: the ratchet |
| `../../crates/quokka-bootstrap/tests/ansi.rs` | the driver, an ignored integration test: builds a world, loads the runner, feeds it one form at a time with a step budget each |

A test's result is one of:

- **pass**: its values were the expected ones — unless the test expects an
  error from a form that uses an operator Quokka lacks, which is *needs*: the
  missing operator supplies an error whatever the test meant to provoke
  (`%caught-needs` in `runner.lisp` has the story).
- **fail**: Quokka had everything the test uses and got a different answer, or
  signalled.
- **escaped**: an error no Lisp handler could catch reached the driver, which
  is a fault in the machine.
- **timeout**: ten million steps were not enough.
- **needs tier 1**: the test uses an operator, a helper or reader syntax that
  Quokka lacks and tier 1 should have.
- **needs later**: it uses only features deferred to a later tier or issue.

`target/ansi-test-results.txt` has every test's result with its detail, and
`target/ansi-test-load.txt` what happened to every definition in the suite's
support files. To run a few files only, after the support files:

```bash
ANSI_TEST_ONLY=auxiliary/cons-aux.lsp,cons/sublis.lsp \
  cargo test --release -p quokka-bootstrap --test ansi -- --ignored --nocapture
```

To run whole groups as the full run reaches them, which matters when a test
depends on what earlier files made (the keywords the `packages` tests sort,
for instance), and print what did not pass:

```bash
ANSI_TEST_GROUPS=symbols,packages \
  cargo test --release -p quokka-bootstrap --test ansi -- --ignored --nocapture
```

The runner runs a file's tests after reading the whole file, as `rt.lsp`
runs tests after loading. That includes the tests the suite defines through
its own macros (`def-syntax-test`, `def-print-test`, …): the runner's
`deftest` macro records such a test for the end of the file, and a form
whose macro cannot expand is still reported, under the name it gives, as
needing whatever stopped it (#122). A failure or a timeout is explained by the
deferred operators the test uses; a failure SBCL shares, listed with
`%follows-sbcl` in `deferred.lisp`, is explained as `sbcl[...]`.

The runner replaces ansi-test's own harness, `rt.lsp`, which needs `loop`,
`handler-bind`, `typep` and `compile-file`. Moving to it once tier 2 allows is
#61.
