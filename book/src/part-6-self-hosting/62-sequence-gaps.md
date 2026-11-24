# What the sequence functions skipped

> *Where this fits.* Chapter 45 wrote the sequence functions of CLHS 17
> once each, over one iteration macro, `%do-sequence`. Chapter 61 closed the
> last gap that ansi-test (chapter 49) had blamed on a missing tier-1
> feature. What remained were 271 *plain* failures: tests that use nothing
> Quokka lacks and still get the wrong answer. This chapter sorts them by
> cause and fixes the five causes that live in the sequence functions. It
> also corrects the measuring tool, which was counting 35 failures as plain
> when their cause had been deferred on purpose.

*Status: done, issue #74. Step AA of phase 2.*

## Sorting the failures

A failure only tells you which test went wrong. To plan the work we needed
to know why, so we read every one of the 271 failing tests and gave each
one cause. Thirty-eight causes came out. A few are large, such as arrays
of element type `nil`, which CLHS counts as strings (30 tests), and the
conditions we signal where CLHS names a particular type (26). Many are one
or two tests each. The full list is a comment on the phase's tracking
issue, #39, with a step proposed for each group of causes.

We started with the largest group that needed neither Rust nor a decision.
Five causes sat in `lisp/library/sequences.lisp` and accounted for 52 tests
between them. Each was a place where chapter 45 had done something simpler
than the standard asks for.

## Five shortcuts

`count` accepted `:from-end` and ignored it. That looks harmless, since
counting from either end gives the same number. But the key and the test
are ordinary functions, and a program can watch the order in which they
are called. One of ansi-test's tests counts with a key that changes on
every call:

```lisp
(let ((c 0))
  (count 1 '(1 2 3 7 4 5 7 6 2 8)
         :from-end t
         :key (lambda (x) (prog1 (- x c) (incf c)))))
;; => 3, and 4 walking from the front
```

The fix was to hand `:from-end` on to `%do-sequence`, which already knew
how to walk backwards.

`nsubstitute` called `substitute` and returned a new sequence, leaving the
one it was given alone. CLHS lets `delete` work either way, but it requires
`nsubstitute` to change its argument, and a program may depend on that. The
clearest case is a displaced array (chapter 55), a window onto another
array:

```lisp
(let* ((base (vector 1 2 3 4))
       (window (make-array 2 :displaced-to base :displaced-index-offset 1)))
  (nsubstitute 9 3 window)
  base)
;; => #(1 2 9 4)
```

`%nsubstitute` marks the matching elements exactly as `substitute` does,
with the same handling of `:count` and `:from-end`, and then writes over
the marked ones in place.

A type such as `(vector * 3)` names a length as well as a kind of sequence,
and so do `null` (no elements) and `cons` (at least one). `make-sequence`,
`concatenate`, `map`, `merge` and `coerce` read only the kind and built
whatever length they had. CLHS says they must signal a `type-error` when the
type cannot hold the result. All five now build through one function,
`%of-type`, which reads the length a type fixes, if any, and checks it
first. Quokka has no type system yet (phase 3), so `%length-of-type` reads
the common spellings only: `(vector * n)`, `(string n)`, `(simple-array t
(n))` and their relatives.

`map-into` with a vector that has a fill pointer stopped at the fill
pointer. CLHS says it ignores the fill pointer, fills the vector up to its
full size, and then sets the fill pointer to the number of elements it
stored. A fill pointer (chapter 55) is the index that says how much of a
vector is in use, so the rule lets `map-into` fill a buffer that starts out
empty.

Finally, `(make-sequence 'string 10)` failed outright. With no
`:initial-element` it tried to store nil into a string, and a string holds
only characters. It now fills a string as `make-string` does, and a bit
vector with zeros.

One of these fixes failed on the first build. `map-into` set the fill
pointer with `setf`, and the `setf` place for `fill-pointer` is defined in
`arrays.lisp`, which loads after `sequences.lisp`. At the moment
`map-into` was compiled, `setf` did not know the place. We call the setter
function, `%set-fill-pointer`, directly. Load order is a constraint every
library file lives with, and this is the second time a definition had to
be written around it (chapter 61 met the first).

## Correcting the measurement

The runner explains a failure by looking for deferred operators in the
test's source (chapter 49). Thirty-five failures escaped that, in three
ways.

1. Twelve structure tests use a helper of the suite's,
   `defstruct-with-tests`, which is written with `loop` (tier 2). The
   runner already marks the names a failed definition would have defined,
   but it knew only `defstruct`'s names. It now treats
   `defstruct-with-tests` the same way, since it takes the same syntax.
2. Eight printer tests call a helper that reaches `loop` only when it
   runs, and seven others failed for similar reasons: a division that
   makes a ratio (#59), or a `format` directive of the pretty printer. Their
   source never mentions the deferred feature. The runner now also reads
   the error message: "LOOP is undefined", "ratios do not exist yet" and
   "a directive basic format does not have" each name a deferred cause.
3. Eight tests of `exp` and `expt` expect an underflow to signal an error.
   The suite marks them with the note `:no-floating-point-underflow-by-default`,
   because an implementation may return zero instead, as SBCL does and as
   Quokka does. SBCL's own run of the suite turns these tests off by that
   note. Ours now does the same, and reports them as needing the note, so
   they are counted with the deferred causes.

None of this changes what passes. It changes which failures are counted as
ours to fix.

## What did it cost?

No Rust. About 80 lines changed in `sequences.lisp` and 30 in the runner.
A new test file, `sequence-gaps.lisp`, agrees with SBCL.

ansi-test went from 7,891 tests passing to 7,943, exactly the 52 the five
causes accounted for, and none of the old ones was lost. The plain failures
went from 271 to 185: 52 fixed, and 34 counted with the deferred causes
where they belong.

## What is still missing?

The other causes on #39's list, in the order proposed there. The next step
gives the library's errors the condition types CLHS names, and adds the
argument checks that are still missing.
