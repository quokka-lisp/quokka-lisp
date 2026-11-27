# Measuring conditions

> *Where this fits.* Chapters 79 to 82 built Common Lisp's condition
> system: conditions that are objects, handlers that run where an error
> happened, the machine's own errors signalled the same way, and restarts.
> This chapter measures it against the part of ansi-test written for it,
> and against everything else the suite could not test until it existed.
> It closes phase 3's second step; the debugger, which a person uses to
> choose among restarts, is built on what Part VIII made.

*Status: done, issue #125. Phase 3, step B, sub-step 5.*

## What `assert` had been hiding

ansi-test checks most of its results with `assert`, and until chapter 82
Quokka did not have it. Every test that used it was counted as waiting for a
deferred feature, which was true, and said nothing about whether the rest of
the test would pass. When `assert` arrived, 1,088 more tests passed at once,
and 426 that had been waiting now ran and failed. Most of what we did in this
step was work through those 426, and then the 333 tests of the `conditions`
group, which we had not measured before.

391 of the 426 were in the printer group, and nearly all of them were the
same few faults in `format`, which a family of tests checks from many sides
at once:

1. `~^` took only its first parameter. CLHS 22.3.9.2 gives it three forms:
   with one parameter it stops when that is zero, with two when they are
   equal, with three when they are in order. A parameter can be `#`, the
   number of arguments left, so `~{~#,3^~A~}` stops when three are left.
2. A `~^` inside `~?` ended the whole `format`, where it should end only the
   `~?`; inside `~(`, it threw away the text the case conversion had
   already made.
3. `~?` took only a string as its control. A control can also be a
   function, the kind `formatter` makes.
4. A `~{` whose body was only a tilde-newline, which prints nothing, was
   taken for an empty body, `~{~}`, which takes its control from the
   arguments. The parser now looks at the text between the brackets, not at
   the directives it parsed.
5. `~3F` of 0.000001 printed `.00`. SBCL prints `0.0`: a value that rounds
   to nothing but zeros is printed as zero is.

Two `~F` tests want `1.0` where SBCL prints `1.`, and Quokka prints what
SBCL prints, so they are counted as SBCL agreeing. Another uses `~vR` with a
nil radix, which asks for English numerals, a feature we have deferred; it
now says so in its error, so the test is counted as waiting for it.

The rest were small, and each was a place where the library signalled an
error the old way, without the initargs a test asks the condition for, or
offered no restart where CLHS says the error is correctable:

1. `endp`, the array functions and `fboundp` signalled a `type-error`
   without a datum, so a test asking `type-error-datum` got an error of its
   own. They pass `:datum` and `:expected-type` now.
2. `ecase` took a clause for `t` or `otherwise` as a catch-all, as `case`
   does. In `ecase` there is no catch-all, and `(ecase 1 (t nil))` must
   signal, as it does in SBCL.
3. `make-package` with a name in use, `defpackage` naming a symbol that
   does not exist, and `delete-package` of a package another uses or of no
   package at all each offer a `continue` restart now, as CLHS describes.

## The `conditions` group

The group's first run failed 58 tests, and 56 of them had the same cause,
which was not in Quokka at all. The suite defines its condition types with
a macro of its own, `define-condition-with-tests`, which defines the type
and a dozen tests about it, and that macro uses `loop`, which we defer. The
macro failed, so the type was never defined, and every test that used it
failed with `make-condition` saying the name named no condition type. Our
runner already knew that a definition that fails makes the tests that use
it wait for whatever the definition needed; it did not know that this macro
defines a condition and its readers. Once it did, those tests counted as
waiting for `loop`.

The last two call `make-condition` with a compound type, `(or program-error
type-error)`. The suite notes this as a problem in the standard, and SBCL
signals a `type-error` there, wanting a condition class. So does Quokka.

## The review of chapter 82

Chapter 82 was merged before it was reviewed, and the review found one
fault that mattered. `invoke-restart`, given a restart object, looked the
restart up again among the active ones, which asks each restart's `:test`
whether it applies, with no condition. A handler that had found the restart
with `(find-restart 'foo c)`, the usual way, could not invoke it if its test
looked at the condition. A restart object is now only checked to be still
active. Smaller ones: `abort` returned when a program's `abort` restart
returned, where it must signal a `control-error`; the host's `abort` was
told the condition of an error even when the restart the debugger invoked
was a program's own; and `assert`'s restart now describes itself in SBCL's
words.

## What did it cost?

No Rust. A few dozen lines in `format.lisp`, the library's error calls, and
the package functions; and the runner learned one of the suite's macros.

ansi-test now measures eighteen groups, 17,739 tests. 12,568 of them pass,
and every one that does not is explained by a feature deferred to a later
step or by SBCL failing it too: there is no plain failure, escape, timeout
or tier-1 gap. In the `conditions` group 236 of 333 pass, and the 97 that
do not use `loop`, the pretty printer, or complex numbers.

## What is still missing?

Of the condition system, nothing that CLHS asks for, though some of it has
nobody to talk to yet: the debugger that would show a person the restarts,
and console input for the restarts that ask for a value. Those are phase
3's step F, after definitions keep their source (step C) and frames keep
their arguments (step D).
