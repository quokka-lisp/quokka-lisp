# Ratios

> *Where this fits.* Chapter 47 gave Quokka integers of any size, and
> chapter 48 double floats. Between them one kind of number was missing:
> the exact fraction. Without it `(/ 1 3)` was an error, and 687 tests in
> ansi-test's measured groups could not run at all, because they divide
> integers or contain a number like `1/2`. This chapter adds ratios, the
> last of the numbers phase 2 builds, and with them the measurement of tier
> 1 reaches its end: one plain failure was left before this step, and it
> was a ratio.

*Status: done, issue #59. Step AI of phase 2.*

## One third

Most languages answer `1 / 3` with 0, because the division of two integers
there is integer division, or with 0.3333333333333333, because the answer
is made a float. Common Lisp gives neither. The answer is the exact number
one third, written `1/3`, and arithmetic on it stays exact:

```lisp
(/ 1 3)            ; => 1/3
(+ 1/3 2/3)        ; => 1
(* 2/3 3/4)        ; => 1/2
```

A number of this kind is a *ratio*. With the integers, the ratios make up
the *rationals*, and with the floats the rationals make up the *reals*.

The question #59 had left open was which tier ratios belonged to. Tier 1
named bignums and floats and not ratios, and we could have left `/` on two
integers an error until a later phase. Our guideline settled it: do it the
right way now, and when in doubt, do as SBCL does. SBCL has ratios, the
numbers group of ansi-test tests them everywhere, and they depend on
nothing that is not already built. So they are tier 1.

## One number, one representation

A ratio is a numerator and a denominator, both integers, and the obvious
risk is that one number has many representations: `1/2`, `2/4` and `-1/-2`
are all one half. CLHS settles it by requiring every ratio to be kept in
a single form, which it calls *rational canonicalisation* (12.1.3.2): in
lowest terms, with a positive denominator, and as an integer whenever the
denominator would be 1. So `(/ 4 2)` is the integer 2, not a ratio with
denominator 1.

All of that is one function in `lisp/library/ratios.lisp`, and nothing
makes a ratio except through it:

```lisp
(defun %ratio (n d)
  (if (eql d 0) (error 'division-by-zero "division by zero" n))
  (let ((g (gcd n d)))
    (let ((n (truncate n g))
          (d (truncate d g)))
      (if (minusp d) (setq n (- n) d (- d)))
      (if (eql d 1) n (%make-ratio n d)))))
```

Because the form is unique, two ratios are equal exactly when their
numerators and denominators are, and `eql` can compare them part by part.

## Representation in Rust, algorithms in Lisp

We split the work the way chapter 47 split bignums. Rust knows the
representation: a new heap object, `ObjectType::Ratio`, with two slots
the collector traces, since the numerator or denominator may itself be a
bignum. Four primitives make one, recognise one and take it apart. Every
algorithm is Lisp.

The connection between the two is the *fallback* of chapter 47. An
arithmetic primitive such as `+` does fixnums and doubles itself, in Rust,
and when it meets an argument it cannot handle it hands the whole call to
a Lisp function, `%generic-+`. Bignums already took that path, and now a
ratio does too. Division by two fixnums needed one more change: Rust used
to refuse `(/ 1 3)` outright, and now it treats an inexact quotient the
way it treats an overflow, by handing the call to Lisp, where `%ratio`
makes the answer.

In Lisp, arithmetic on ratios is school arithmetic on numerators and
denominators. a/b + c/d is (ad + cb)/bd, put in lowest terms by `%ratio`.
Comparison uses cross products, which is exact because both denominators
are positive. A ratio compared with a float is compared as the exact
rational the float is, as CLHS 12.1.4.1 requires, so that `(< 1/3
0.3333333333333333)` is false: the double nearest to one third is a
little less than it.

## Between ratios and floats

Two conversions needed more care than the arithmetic.

`(float 1/3)` asks for the double nearest to one third. Dividing the
numerator's double by the denominator's is not good enough once either is
a bignum, because each conversion rounds and the two roundings compound.
`%ratio-to-double` divides the integers exactly, to 55 bits, keeps a
*sticky bit* that says whether anything was left over, and rounds once, to
the nearest double with ties to even, as a correctly rounded division does.
The one case it does not get right is a result too small for the normal
range of doubles, which is rounded a second time when it is scaled down.
SBCL avoids that, so a subnormal result can differ from SBCL's in its last
bit; the differential file does not ask for one.

`rationalize` goes the other way and has a pleasant answer. `(rational
0.1)` is exactly the double nearest to one tenth,
3602879701896397/36028797018963968. `(rationalize 0.1)` is 1/10: the
simplest rational that would read back as the same double. We wrote SBCL's
algorithm, which takes the interval between the double's neighbours
halfway down and halfway up, and walks it as a continued fraction, taking
the integer part and turning the rest upside down, until an integer fits
inside.

## Hash tables

A ratio used as a key in an `eql` table must be found by an equal ratio
made separately, so Rust hashes it by its parts. `equalp` is harder,
because it compares numbers with `=`, under which 1/2 and 0.5 are equal,
so they must land in the same bucket. A ratio is `=` to some double only
when its numerator is below 2^53 and its denominator is a power of two,
since that is what a double with a fraction is. For such a ratio Rust
computes the double and hashes it as the double; every other ratio equals
no double at all and can hash however it likes.

## Reading and printing

The reader reads `2/4` as 1/2, in `*read-base*`, so that in base 16
`a/f` is two thirds. A token that is ratio syntax in decimal but not in
the current base is a symbol, as SBCL reads it. The printer writes the
numerator, a slash and the denominator in `*print-base*`. With
`*print-radix*` true an integer in base 10 gets a trailing point, `5.`,
but a ratio cannot, so CLHS gives it a prefix in every base: `#10r1/3`.

## What did it cost?

126 lines of Rust: the object type and its module, the primitives, the
fallback, hashing, and the Rust printer. `ratios.lisp` is about 150 lines,
and `integers.lisp`, `floats.lisp`, the reader and the printer each
changed a little. The differential file `ratios.lisp` checks all of it
against SBCL. Two things it cannot check: SBCL's `float` of a ratio, with
no prototype, makes a single-float where Quokka makes a double, since
Quokka has only doubles (chapter 48); and for the same reason `(sqrt 1/4)`
is 0.5 in Quokka and 0.5f0 in SBCL.

ansi-test went from 8,112 passing tests to 8,139, with no plain failure
left in the measured groups, no test escaping every handler, and nothing
that needs a part of tier 1 Quokka lacks. The runner also learned one more
explanation on the way: a definition that runs out of steps while it is
being loaded, such as a structure-test helper written with `loop`, now
marks the tests that call it with what it uses, as a definition that
fails to load already did.
