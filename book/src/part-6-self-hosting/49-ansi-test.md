# Measuring against the standard

> *Where this fits.* Chapters 35 to 48 built tier 1, the self-hosting core:
> special variables, packages, the reader and the printer in Lisp, `eval`,
> the warm rebuild and its fixpoint, `setf` places, sequences, `format`,
> bignums and floats. Every one of them was tested against SBCL, file by
> file, with tests we wrote ourselves (chapter 12). This chapter turns the
> question around. Instead of asking whether what we built works, it asks
> how much of Common Lisp tier 1 actually covers, using a test suite we did
> not write: *ansi-test*, the standard's own conformance suite. The answer
> is a table of numbers, a list of what is missing ordered by how much it
> matters, and a check in CI that keeps every passing test passing. It is
> the instrument phase 2's last stretch is steered by.

*Status: done, issue #60. Step N of phase 2. The gate it defines is not yet
met.*

## Why test against someone else's tests?

Every test in `tests/lisp/` was written by us, after we had decided what a
feature should do. That catches the mistakes we can imagine. It cannot catch
the ones we cannot: a corner of the standard we never read, an argument
nobody passes, a function we did not know existed. A **conformance test
suite** is written against the standard instead of against an
implementation's intentions, so it asks the questions we would not have.

Common Lisp has one. Paul Dietz wrote it between 2002 and 2005 while working
on GNU Common Lisp, and it is now maintained at
gitlab.common-lisp.net, where ECL, Clasp, CLISP and others run it. It holds
some seventeen thousand tests in one directory per chapter of the standard,
and its licence is MIT. A test is a name, a form and the values the form
should return:

```lisp
(deftest cons-of-symbols
  (cons 'a 'b)
  (a . b))
```

The design had always said tier 1 would be "measured against its ansi-test
groups", without saying what measuring meant. We decided that in #60, five
questions in all.

## Where does the suite come from?

We do not copy it into the repository. `scripts/ansi-test.sh` clones it into
`target/ansi-test/` the first time it runs and checks out one pinned commit,
so every run measures exactly the same tests and nothing third-party is
committed. Without a network connection and without a clone, the script
skips and says so, as the SBCL oracle does; in CI a failed fetch is an
error, since a silent skip there would hide a regression.

## Who runs the tests?

ansi-test comes with its own harness, `rt.lsp`, Richard Waters's RT
regression tester from 1990, and a loader that brings in support code and
every test file. We read both before deciding anything. The loader deletes
old compiled files with `directory` and `delete-file` and loads everything
with `compile-file`; `rt.lsp` itself uses `loop` nine times, `handler-bind`,
`typep` and multi-dimensional arrays. All of that is tier 2 or later, so the
suite cannot measure tier 1 with its own harness until tier 1 is no longer
what is being measured.

We wrote a small runner of our own in tier-1 Lisp, `tests/ansi/runner.lisp`,
and kept ansi-test's files unmodified. A driver in Rust,
`crates/quokka-bootstrap/tests/ansi.rs`, builds a world, loads the runner,
and then hands the world one file at a time and one form at a time. Each
form is a separate call with its own **step budget**, the limit rule 4 gave
every computation (chapter 2), so a test that never finishes is reported as
a timeout and the run carries on. Being an ignored integration test, the
driver does not count against the Rust budget. When tier 2 makes `rt.lsp`
loadable, issue #61 replaces our runner with it.

## What counts as passing?

Tier 1 corresponds to fourteen of the suite's directories: `cons`,
`sequences`, `strings`, `characters`, `symbols`, `packages`, `numbers`,
`reader`, `printer`, `data-and-control-flow`, `eval-and-compile`,
`iteration`, `hash-tables` and `structures`. Some only in part: the
`numbers` group also tests ratios and complex numbers, and `iteration` is
mostly `loop`.

We considered a percentage per group and rejected it, because a percentage
can be reached while real bugs remain and it hides which tests fail and why.
Instead the run is a **ratchet**. Every test that passes is written to
`tests/ansi/baseline.txt`, which is committed, and a run fails if any of
them no longer passes; a change that makes more tests pass updates the list
with `ANSI_TEST_UPDATE=1`. Phase 2's gate is then precise: it is met when
every test in those groups that does not pass is explained by a feature
deferred to a later tier, and none is a plain bug.

## Failing, or not applicable?

The obvious way to count gives a useless number. A great many tests in the
tier-1 groups use `loop`, `typep` or `handler-bind` in their own forms, even
when the function they test is one we have. Counted as failures, those would
drown everything else.

So the runner tells a failure apart from a test that is **not applicable**
yet. When a test does not pass, the runner walks its form, symbol by symbol,
looking for operators of the `COMMON-LISP` package that Quokka does not
have. It knows which symbols are operators from a list generated from SBCL
(`tests/ansi/cl-operators.lisp`), so a symbol that is only data or a type
name is not mistaken for one. It follows the helpers a test calls, since
ansi-test's own support files define hundreds, and it counts reader syntax
Quokka cannot read. Such a test is reported as *needs X*. A second file,
`tests/ansi/deferred.lisp`, lists the operators Quokka lacks on purpose, each
with the tier or issue it waits for:

```lisp
(%deferred "tier-2:loop" :loop :loop-finish)
(%deferred "issue-59:ratios" :numerator :denominator :rational :rationalize)
```

Anything Quokka lacks that is not on that list is a gap in tier 1. That
gives every test one of six results: it passes; it fails, meaning Quokka had
everything it uses and got it wrong; it escapes, meaning an error reached the
driver that no Lisp handler could catch; it times out; it needs something
tier 1 should have; or it needs only deferred features. The gate is the last
four columns emptied of everything but the last.

Our first version asked what a test needed *before* running it, and skipped
the test if the answer was anything. That lost tests that pass: a helper
might use `loop` only in a branch the test never takes. The runner now runs
every test first and asks only once it has failed:

```lisp
(if (and (not (and (consp values) (eq (car values) :signalled)))
         (%same values expected))
    (format nil "PASS ~A" name)
    (let ((needs (%needs test)))
      (if needs
          (format nil "NEEDS ~A ~{~A~^ ~}" name needs)
          (format nil "FAIL ~A ~A" name (%one-line (%describe values))))))
```

The helpers tests rely on most, `signals-error` above all, are written in
ansi-test with `handler-bind` and `typep`. The runner provides its own, over
the `handler-case` tier 0 already has, so that nearly two thousand tests of
error behaviour can run at all.

## What did the first runs find?

The first run reported 37 tests. Two of the reasons were in the runner. A
nested load file names its files relative to its group, not to itself, and
our driver had looked in the wrong directory. And the runner used a hash
table keyed by conses to avoid walking a circular form twice, which Quokka's
hash tables cannot do: they cannot hash an object's address, because the
collector moves objects (chapter 19), and a cons has no other content they
know how to hash. The walk now stops after a fixed number of nodes
instead.

The next run reported 1,031 timeouts. Only eleven were real. The rest were
errors that had escaped every handler: calling a function with a keyword it
does not accept, `(f :b 1)` where `f` takes only `:a`, signals an error that
no `handler-case` in Lisp can catch. That is a fault in the machine, so the
driver now reports such a test as *escaped*, apart from timeouts. The same
tests showed that `:allow-other-keys` is not accepted either.

Sorting the plain failures by message found more. 154 tests failed because
`make-scaffold` was undefined, though the support file defining it had
loaded without complaint. The cause was `defstruct`: the prelude's version
makes the constructor and accessor names in `CL-USER`, whatever package is
current, so a structure defined in the suite's package `CL-TEST` has no
constructor there. And `(funcall 'eql 1 1)` fails, because calling a symbol
as a function, which Common Lisp allows everywhere, does not work at all.

## Where does tier 1 stand?

Over 12,488 tests in the fourteen groups (more than the files' own count,
since the suite's macros generate tests), 4,058 pass. 573 fail, 1,455
escape, 13 time out, 3,221 need something tier 1 should have and 3,168 need
only deferred features. The whole run takes about five minutes.
`docs/ansi-test.md` has the table per group, rewritten with every update.

The list of gaps ordered by how many tests each blocks is the most useful
thing the run produces, because it says what to build next. At the top are
`#+` and `#-`, the reader's feature conditionals, behind about 1,100 tests
(mostly through helpers in the support files); `declare`, which Quokka does
not accept at all, behind 742; then `with-standard-io-syntax`, `compile`,
`macro-function`, the package functions `delete-package`,
`package-used-by-list` and `unuse-package`, `readtable-case` and `fboundp`.
Among the deferred features, `loop` alone blocks 2,907 tests.

## What did it cost?

No Rust in the virtual machine. The runner is about 530 lines of Lisp and the
driver about 430 lines of Rust, both under `tests/`, which the budget does
not count; `deferred.lisp` is a page, and `cl-operators.lisp` is generated.
The fixpoint, our check that the compiler compiles itself into exactly the
same code generation after generation (chapter 42), was not touched: nothing
in the system changed. A CI job of its own runs the suite and the ratchet.

## What is still missing?

The gate itself. Phase 2 ends when every test in the fourteen groups that
does not pass is explained by a deferred feature, and today 573 plain
failures, 1,455 escapes and 3,221 tier-1 needs stand in the way. The order is
set by the table: the reader's `#+` and `#-`, declarations, the escaping
keyword errors and `:allow-other-keys`, `defstruct` in the current package,
calling a symbol as a function, then the long tail of missing functions.
Some entries may turn out to belong on the deferred list instead, which is a
decision to make in the open, one entry at a time. And the runner itself is
temporary: when `loop`, `handler-bind` and `typep` exist, #61 replaces it
with ansi-test's own harness.

---

**References.** Paul F. Dietz, "The GNU ANSI Common Lisp Test Suite",
International Lisp Conference 2005, and the suite itself at
gitlab.common-lisp.net/ansi-test/ansi-test. Richard C. Waters, "Supporting
the Regression Testing of Lisp Programs", *ACM SIGPLAN Lisp Pointers* 4(2),
1991, for RT. The Common Lisp HyperSpec, 2.4.8.17 on `#+` and `#-`, and 3.3
on declarations.
