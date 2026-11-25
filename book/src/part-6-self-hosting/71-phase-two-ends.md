# Phase 2 ends

> *Where this fits.* Phase 2 set itself a gate with two parts. The first,
> that the compiled compiler reproduces itself byte for byte, has held
> since chapter 42 and has been checked on every change since. The second
> is that tier 1 is complete and measured: every test in ansi-test's tier-1
> groups either passes or fails for a reason we chose, a feature we
> decided to build later (chapter 49). After chapter 70 four tests were
> left that did neither; they ran out of steps. This chapter is about
> those four, and then about what the gate means now that it is met.

*Status: done, issue #82. Step AJ of phase 2, and phase 2's gate.*

## Four slow tests

None of the four was a loop. Each was correct and too slow, and in each
case the slowness came from the library doing in many small steps what
SBCL does in a few.

Two tests in the `packages` group, `keyword.2` and `do-external-symbols.5`,
collect every keyword and sort them by name. There are about five hundred
keywords by then, so a sort makes several thousand comparisons, and each
comparison is a call of `string<`. That call cost about 500 steps for two
eleven-letter names. `string<` and its eleven relatives share one function,
`%string-compare`, which took the strings' bounds apart, walked them a
character at a time with library calls, and returned two values that the
caller took apart again with `multiple-value-bind`, which builds a closure.
Each part was reasonable alone; together they made sorting a few thousand
names run past ten million steps.

We tried Lisp first. Comparing equal characters with `eq` before calling
the key function, and using the primitive `<`, halved the cost, and that
was not enough. So the walk became a primitive, `%string-mismatch`: given
two simple strings and two ranges, it returns where they first differ and
which way, packed into one fixnum. Rule 9 asks a primitive to be short or
to have a plan, and this one is linear in the length of the strings, as
`equal` on two strings already is in Rust. Every comparison of two simple
strings with no `:start` or `:end` now calls it directly and decodes the
answer inline.

The sort needed attention too. The library's merge sort copied each half
of its list with `subseq` at every level and built each merged list
backwards and reversed it. It now relinks the conses of the fresh copy it
was given: split at the middle, sort each half, and merge by pointing a
tail at whichever head comes first.

The other two slow cases were definitions in ansi-test's support files,
lists of standard symbols made with `set-difference` and `union` of two
lists of nearly a thousand symbols each. Comparing every element of one
with every element of the other is a million comparisons. SBCL's set
functions put the second list in a hash table when it is long and the test
is one a hash table can use, and so do ours now, as `remove-duplicates`
already did (chapter 69).

## The gate

With those, the full ansi-test run over the fifteen tier-1 groups gives
this. Of 13,559 tests, the rest pass or are explained: each fails or does
not finish because it uses `loop`, CLOS, the full condition system,
pathnames, `typep`, complex numbers, random numbers or something else
decided to come later, or because SBCL gives the same answer we do. No
test fails for a reason we have not chosen, no error escapes every handler,
and nothing needs a part of tier 1 that Quokka lacks.

It is worth saying what this does not mean. Passing tests are evidence,
not proof, and ansi-test does not test everything. More than five thousand
of the tests in these groups do not run at all yet, because nearly every
test of a sequence function also uses `loop` to build its data, and those
tests will first run when tier 2 brings `loop`. When they do, some will
find bugs in tier 1. The ratchet (chapter 49) will then do its job:
each test that starts to pass is added to the baseline and cannot quietly
fail again.

The other half of the gate is the fixpoint, and it held through every
step of this part of the book. The compiler changed many times on the way,
in its handling of declarations, local functions and condition clauses,
and every time the rebuilt compiler compiled itself into the same code.

## What did it cost?

53 lines of Rust for `%string-mismatch`. The rest is Lisp: the fast path
in the string comparisons, the merge sort, and the set functions. The
driver gained a way to run a few groups as the full run reaches them,
`ANSI_TEST_GROUPS`, which is how we found that the keyword tests depended
on what earlier files had made.

Phase 3 starts from here: conditions and restarts, processes, and a
debugger in which a function can be fixed from inside an error and the
frame retried (`docs/design.md`). Tier 2, which `loop`, CLOS and the full
condition system belong to, arrives over phases 3 to 5, and most of the
tests that do not run yet are waiting for it.
