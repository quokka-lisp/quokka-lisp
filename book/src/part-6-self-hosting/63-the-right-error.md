# The right error

> *Where this fits.* Chapter 20 gave Quokka conditions: `error` signals
> one, and `handler-case` catches the ones whose type it names. Chapter 62
> sorted ansi-test's remaining failures by cause. The second-largest group
> was not wrong answers at all, but right refusals with the wrong type.
> This chapter gives the library's own checks the condition types CLHS
> names, adds the checks that were missing, and records what that did to
> the warm rebuild's step budget.

*Status: done, issue #75. Step AB of phase 2.*

## Why the type matters

When a function refuses its arguments, a program that wants to recover
catches the refusal by type:

```lisp
(handler-case (elt sequence index)
  (type-error () :out-of-range))
```

CLHS says which type each refusal has. An index outside a sequence is a
`type-error`, a name with no package is a `package-error`, and a value
that does not fit a destructuring pattern is a `program-error`. If the
function signals some other type, the handler does not match, the error
goes on up, and the program stops where it meant to recover.

Quokka's Rust primitives already signalled the right types. When `car`
refuses a number it signals a `type-error`, because the machine turns each
kind of error it detects into a condition of the matching type. The library's checks were
another matter. Most were written `(error "text" datum)`, and an `error`
called with a string signals a `simple-error`, the type CLHS gives an
error that has nothing more specific to say. That was the easy thing to
write while the functions were new, and ansi-test found 26 of them.

## Saying which type

The change at each place is small. `elt` of a list past its end used to
be

```lisp
(error "index out of range for ELT" index)
```

and is now a `type-error`. `nthcdr`, which `last` and `nbutlast` use,
refuses a count that is not a non-negative integer with a `type-error`.
`make-sequence` and `map` of a type that names no sequence, such as
`symbol`, do the same. `package-name`, `package-nicknames` and
`in-package` of a name no package has signal a `package-error` through
the helper `delete-package` already used, and of something that cannot
name a package at all, such as the number 10, a `type-error`.
`fdefinition` of 10 used to claim the function was undefined. It is not a
function name in the first place, so that is a `type-error` too.

## Checks that were missing

Eleven more tests expected an error where Quokka signalled none.

`endp` is the test that ends a walk down a list. The prelude's was
`(null x)`, which answers nil for the number 1 as if it were a cons, so a
walk down a malformed list carried on past its end. The library now
redefines `endp` to signal a `type-error` for anything that is neither a
cons nor nil. `last`, `butlast` and `nbutlast` check their count, so
`(last list 10.0)` is refused instead of compared.

`elt` on a vector with a fill pointer read past the fill pointer, because
it went to `aref`, which checks against the array's full size. It now
checks the index against `length`, which respects the fill pointer, and
so does `(setf elt)`. SBCL does not make this check at its default safety
setting, and ansi-test runs at the highest, where it expects the error, so
the case is in ansi-test and not in our comparison with SBCL.

`gensym` refuses a negative `*gensym-counter*`, now that the counter is a
variable a program can set (chapter 60).

The last check turned out to be a question about what `nil` means. In

```lisp
(destructuring-bind (foo nil bar) (list 1 2 3) ...)
```

the `nil` reads like a variable, but `nil` is also the empty list `()`,
and in a destructuring pattern `()` is a sub-pattern that matches only an
empty list. SBCL reads it that way: the example signals a
`program-error`, because 2 does not match `()`, while `(list 1 nil 3)`
binds `foo` and `bar` without complaint. Quokka's pattern rewrite, which
is in Rust, saw a symbol and bound it like a variable. We fixed it in the
`destructuring-bind` macro instead. Each `nil` element becomes a fresh
variable, and before the body runs, a check signals the error if that
variable is not nil. The functions that report a pattern mismatch, which
the prelude had written to signal plain errors, are redefined in the
library to signal `program-error`.

## The rebuild's budget

The warm rebuild (chapter 41), in which a running world recompiles its
whole system, runs as one form with a step budget, so that a rebuild that
never finishes is stopped. The budget was forty times an ordinary form's,
200 million steps, and step AB's changes made the rebuild run out of it.

Our first guess was the new `endp`, since the compiler calls it on every
list it walks. Testing a cons before nil made no difference, and taking
the redefinition out entirely made none either. So we measured instead.
With twice the budget the rebuild finished in the same seventeen seconds
as before, so it was not looping, and with 41 times the budget it
finished as well. A rebuild had grown to about 201 million steps, and it
grows a little with every library form. The budget had had almost no
room left, and AB's six new definitions used up the last of it.

We raised it to sixty times an ordinary form's, in both places that set
it: the `quokka rebuild` command and the tests. The number is a guard
against a rebuild that never ends, not a performance target, so it should
sit well above what a rebuild needs. The old one had been set when the
rebuild needed much less, and it had stopped being that guard.

## What did it cost?

No Rust lines. The rebuild budget is a constant in `main.rs` whose
comment stayed the same length. About 60 lines of Lisp changed across
`lists.lisp`, `sequences.lisp`, `setf.lisp`, `packages.lisp` and
`symbols.lisp`. A new test file, `condition-types.lisp`, agrees with
SBCL.

ansi-test went from 7,943 tests passing to 7,980, the 37 the two causes
accounted for, with none of the old ones lost. The plain failures went from
185 to 148.

## What is still missing?

`(destructuring-bind (a &optional nil) ...)` is still refused, where SBCL
reads the `nil` as an empty pattern there too, and the conditions have no
hierarchy yet (phase 3): a handler for `stream-error` does not catch an
`end-of-file`. The next steps on #39's list are the lambda-list fixes that
need Rust, and the two that need a decision first.
