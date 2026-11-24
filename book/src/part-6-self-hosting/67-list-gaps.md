# Lists that do not end in nil

> *Where this fits.* Chapter 45 wrote the list functions of CLHS 14 in the
> library, and chapter 62 sorted ansi-test's remaining failures by cause.
> One group was a handful of small mistakes that share a theme: functions
> that assumed every list is proper, ends in `nil` and ends at all. This
> chapter fixes those, and a few neighbours from the same group: `equalp`
> on hash tables, strings that are not simple used as names, contradictory
> `defpackage` options, and `values` places inside `values` places.

*Status: done, issue #79. Step AF of phase 2.*

## Three kinds of list

A *proper* list ends in `nil`: `(a b c)` is three conses whose last cdr
is `nil`. A *dotted* list ends in some other atom: `(a b . c)` is two
conses, and the cdr of the second is the symbol `c`. A *circular* list
does not end; some cons's cdr points back to an earlier cons. CLHS says
for each list function which of the three it accepts, and many accept
all three.

`last` is one of them. `(last '(a b . c))` is `(b . c)`, the last cons,
and `(last '(a b . c) 0)` is `c`, the atom at the end. Quokka's `last`
counted the list's elements with `length` and then took the tail it
wanted with `nthcdr`. `length` refuses a dotted list, as it should, so
`last` refused one too. The new `last` walks two pointers: the first goes
N conses ahead, then both move until the first falls off the end, and the
second is the answer.

```lisp
(defun last (list &optional (n 1))
  (%check-count n)
  (let ((lead list))
    (do ((i 0 (+ i 1)))
        ((or (not (< i n)) (atom lead)))
      (setq lead (cdr lead)))
    (do ((tail list (cdr tail))
         (lead lead (cdr lead)))
        ((atom lead) tail))))
```

The lead pointer stops at the end of the list, so a count larger than any
fixnum, which ansi-test also tries, costs no more than the list's length.
`butlast` and `nbutlast` count conses with a small helper,
`%cons-count`, instead of calling `length`.

The first version of this `last` tested `(>= i n)`, and ansi-test's run
showed what that cost. The compiler calls `last`, and in a compiled world
it calls the library's. The library's `>=` takes any number of arguments
and walks them in a loop with a closure, so every step of `last` paid for
a function call, and one large function in ansi-test's structure tests
ran out of steps while it was being compiled. `last` now uses the
primitive `<`, and `<=` and `>=` go straight to the primitives when they
are given two arguments, which is nearly always.

`nconc` was the one with a circular list. `(nconc x x)` joins a list to
itself, making it circular; Quokka's `nconc` found the last cons of every
argument, the last included, and the last argument was by then circular,
so the search never ended and the test ran out of steps. CLHS has `nconc`
point the previous list's last cons at the last argument and never look
inside it, which is also why the last argument may be an atom. The new
`nconc` finds the end of every argument but the last.

`tree-equal` had a different small fault. Its test may return more than
one value, and `tree-equal` returned whatever its test did. It now
returns `t` or `nil`.

## Hash tables under equalp

`equalp` is the loosest of the four equality predicates: it ignores the
case of characters and compares arrays and structures element by
element. CLHS extends it to hash tables. Two tables are `equalp` if they
have the same number of entries and the same test, and if every key of
one maps to an `equalp` value in the other. The keys themselves are
looked up with the table's own test, so `"a"` and `"A"` are different
keys in an `equal` table even though `equalp` would call them the same.
That is a dozen lines in `lists.lisp`, using `maphash` and a
`return-from` out of it at the first difference.

## Strings that are not simple

`make-symbol` and `intern` are primitives, and the primitives wanted a
*simple* string, the kind with no fill pointer or displacement (chapter
55). A string with a fill pointer is still a string, and its name is its
active characters, those before the fill pointer. The primitives now
take any string, reading a non-simple one through the same function that
`equal` uses to compare two strings.

## defpackage options that contradict each other

`defpackage` takes a list of options, and CLHS forbids some combinations:
`:size` or `:documentation` given twice, a symbol name that appears in
more than one of `:shadow`, `:shadowing-import-from`, `:import-from` and
`:intern`, and a name both interned and exported. Each would ask for the
same symbol to be put in the package two ways. Quokka did whatever the
last option said; it now signals a `program-error` before it makes or
changes the package. SBCL finds these when it expands the macro, at
compile time; we find them when the package is defined, which in a
system whose `eval` compiles and then runs is a moment later.

## A values place inside a values place

`(setf (values a b) (floor 7 2))` stores each value in its own place, and
CLHS lets a place inside be a `values` place too. ansi-test asks what
happens with

```lisp
(setf (values a (values b c) (values d) (values e f))
      (values 0 1 2 3 4 5 6))
```

and the answer is that each place takes one value, so `b` gets 1 and `c`
gets nil, `e` gets 3 and `f` gets nil. Quokka refused the inner `values`
place, because it has two stores where the outer place expected one. The
outer expander now gives an inner place's first store the value and binds
its other stores to `nil` among the temporaries. `psetf`, which must
compute every value before it writes any place, was rewritten as nested
`multiple-value-bind` forms so that a place with several stores takes
several values there too.

## What did it cost?

About eight lines of Rust, to let the name primitives read a string that
is not simple. The rest is Lisp. One differential file,
`list-gaps.lisp`, holds all of it to SBCL, the defpackage refusals
included.
