# The last gaps

> *Where this fits.* Phase 2 ends when tier 1 is complete and measured:
> every test in ansi-test's tier-1 groups either passes or fails because of
> something we decided to build later (chapter 49). After the steps of
> chapters 62 to 68, a few dozen failures were left, along with a handful
> of tests that ran out of steps. This chapter clears them. Some were bugs,
> some were slowness, and two kinds were not Quokka's to fix at all, which
> made us decide how the measurement should count them.

*Status: done, issue #81. Step AH of phase 2.*

## Handlers that name a supertype

Every condition has a type, and the types form a lattice: `end-of-file`
is a kind of `stream-error`, which is a kind of `error`, which is a kind
of `serious-condition`, which is a kind of `condition`. A handler that
names `stream-error` should catch an `end-of-file`. Quokka's did not,
because a compiled `handler-case` clause tested the condition's type
with `eq` (chapter 20). The only exceptions were `error`, `condition` and
`t`, which caught everything.

Programs define their own condition types with `define-condition`, which
belongs to the full condition system of tier 2. But the standard types are
fixed, CLHS lists each one's supertypes, and SBCL has them all, so there
was no reason to wait. The prelude now has the table, thirty entries
taken from CLHS chapter 9. When the compiler compiles a clause, it lists
the named type and every type below it, and the clause tests the
condition's type against each with `eq`, as before.

We first made the clause call a prelude function that walked the table
when it ran. The bootstrap's own tests caught that: they run compiled code
in a world whose prelude is still interpreted (chapter 29), and compiled
code cannot call interpreted code. Listing the subtypes at compile time
keeps the generated code to primitives. When tier 2 brings
`define-condition`, a program will be able to add types after a clause is
compiled, and the test will have to move back into the running code, as
SBCL's `typep` does.

## A control string from the arguments

`format`'s `~{...~}` repeats its body for each element of a list. When
the body is empty, `~{~}`, CLHS says the body is taken from the arguments
instead: a control string, or a function such as `formatter` makes, which
is called with the stream and the arguments and returns the ones it did
not use. Quokka ran the empty body, which used no arguments, so the
iteration never reached the end of its list and ran until its step budget
was gone. `%format-iteration` now takes the control from the arguments
first.

## Tests that ran out of steps

Ansi-test gives each test ten million steps. A test that needs more is
reported as a timeout, and seven of the remaining ones were slowness in
the library, not loops.

`(length (make-list 200000))` was the clearest. `make-list` loops with
`dotimes`, and `dotimes` tested its counter with the library's `>=`,
which takes any number of arguments and walks them with a closure, so a
single comparison cost a function call and a loop of its own. `dotimes`
now uses the primitive `<`. `(logbitp most-positive-fixnum 0)` asked
`ash` to shift right by more than any number has bits, and `ash` did it by
computing two to that power first; it now answers 0 or -1 straight away
when the shift is past the last bit. `(elt nil 1000000)` took the
millionth tail of an empty list one step at a time; `nthcdr` now stops at
`nil`. And the tests of `do-external-symbols` and of the keyword package
collect several thousand keywords and remove the duplicates, which in
Quokka compared every pair. When the test is one of the four a hash table
can use, `remove-duplicates` now finds a duplicate in a table, as SBCL's
does.

## What a measurement should count

Two kinds of failure were left that no change to Quokka should fix.

The first is a test SBCL fails too. `print.array.level.8` prints an array
nested inside another with `*print-level*` set so low that the inner one
is too deep. The suite expects `#2A((#))`, and SBCL prints `#2A((#2A#))`,
which is what Quokka printed as well. Rule 6 says Quokka behaves as SBCL
does, so we kept the output and told the runner: `tests/ansi/deferred.lisp`
now lists the tests where we asked SBCL and it agrees with us, and a
failure of one of them counts as explained, as `sbcl[same-as-sbcl]`.

The second is a test that ran out of steps because it uses something we
have deferred. A test written with `loop` may not fail when `loop` is
missing; with the small `loop` the prelude has, which only repeats its
body, it may never stop. The runner already explained a failing test by
the operators it uses. It now does the same for a test that ran out of
steps, so a timeout in a test that uses `loop` is counted as needing
`loop`.

## Tests read before what they test

One failure turned out to be the runner's. In `defun.lsp`, the test of a
setf function comes before the form that defines the function. ansi-test's
own harness loads every file and then runs the tests, so the order does
not matter to it. Our runner ran each test as soon as it read it. It now
reads a whole file first, loading its definitions as they come, and runs
the file's tests at the end, which is closer to what the suite expects.

## What did it cost?

No Rust. The prelude gained the condition table and the compiler a
function to list a type's subtypes. The differential file
`remaining-gaps.lisp` checks every fix against SBCL, including SBCL's way
of printing the too-deep array.

Together with chapter 68's step, ansi-test went from 8,065 passing tests
to 8,112, and from 34 plain failures to one, a division that makes a
ratio, which is the next chapter's subject. No test escapes every handler
any more.
