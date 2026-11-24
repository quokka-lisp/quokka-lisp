# What the compiler skipped

> *Where this fits.* The compiler (chapter 31) and the prelude it is
> compiled with were written to compile Quokka's own source, and they do;
> the fixpoint (chapter 42) checks every day that the compiler compiles
> itself into the same code. ansi-test asks about many programs Quokka's
> source never contains. Chapter 62 grouped the failures by cause, and the
> last group was a dozen small things the compiler and the prelude got
> wrong or did not do. This chapter goes through them. None needed a
> design decision; each had one right answer, given by CLHS and confirmed
> by SBCL.

*Status: done, issue #80. Step AG of phase 2.*

## Declarations that went missing

A declaration is a form like `(declare (special x))` at the start of a
body. It is not evaluated; it tells the compiler something about the
variables the surrounding form binds. Quokka's compiler reads only one
kind, `special`, which makes a variable dynamically bound (chapter 36),
and looks for it in the bodies of `let`, `let*`, `lambda` and `locally`.

Several other forms have bodies that may start with declarations, and in
each of them the declaration was lost. In `flet`, `labels` and `macrolet`
the compiler compiled the body with `%compile-body`, which treats a
`declare` form as a call of the macro `declare`, and that macro expands
to `nil`. Those three now look at their body's declarations the way
`locally` does.

`multiple-value-bind` is a prelude macro. It expands into a function
that takes the values as optional parameters, and it put a reference to
its own extra parameter before the body, which pushed the body's
declarations off the front of the function. The new expansion puts the
declarations first. While there we found that `(multiple-value-bind ()
:foo)` returned `(:foo)`, the list of extra values, because a body with no
forms left that reference as the last form; an empty body now gives
`nil`.

The subtlest one was in the rewrite of a lambda list that has `&optional`
or `&aux` parameters (chapter 64). The rewrite binds those parameters in
a `let*` inside the function and moves the body into it, declarations
included. In

```lisp
(lambda (x &aux (y (f)))
  (declare (special x))
  y)
```

the declaration then sat inside the `let*`, where it no longer reached
the function's parameter `x`, so `x` was bound lexically and `f` saw the
old dynamic value. CLHS 3.4.1 binds each parameter before the next one's
default form runs, so `x` should already be special when `(f)` is called.

Our first fix copied all the leading declarations in front of the `let*`,
and ansi-test's next run caught a dozen new failures. In

```lisp
(let ((x :good))
  ((lambda (&optional (y x)) (declare (special x)) y)))
```

the lambda binds no `x` of its own, so its declaration is *free*: it says
something about references in the body, and CLHS 3.3.4 says a free
declaration does not reach the default forms of the lambda list. `y`'s
default should read the lexical `x`, `:good`. Copied in front of the
`let*`, the declaration reached it. The rewrite now puts in front only
`special` declarations of the required parameters, the ones the function
itself binds, and leaves everything else inside the `let*`.

## Small things in the compiler

A `tagbody` tag may be a symbol or an integer, and a `go` names it. The
compiler found the tag with `eq`, which is right for symbols and for
small integers, which are fixnums, but two bignums read separately are
two objects. The compiler's `%position` now also compares integers with
`=`.

`flet` and `labels` hide a global macro of the same name (chapter 45 did
that), but not a local one made by an outer `macrolet`, because the
compiler asked its list of local macros before anything else. The body of
a `flet` or `labels` now gets an entry in that list for each of its
functions, with no expander, which hides the macro further out.

A compiler macro (chapter 53) is an optional rewrite of a call that the
compiler may apply. CLHS says it must not apply one to a call of a
function declared `notinline`. Quokka ignored that declaration; it is now
read in the same places as `special` and kept in the environment as a
scope that exists only while compiling.

A macro's lambda list may have a nested pattern in its `&optional` and
`&key` parameters: `(&optional ((x y) '(1 2)))`. The generated
destructuring code (chapter 64) handled nested patterns among the
required parameters only, and the others were refused while the macro was
being defined. A refusal there cannot be caught by the program, so
ansi-test counted these as escapes. The Rust that parses the entries now
allows a pattern where a destructuring lambda list allows one.

## A throw that nothing catches

`(throw tag value)` unwinds to the nearest `catch` with that tag. If
there is none, CLHS says `throw` signals a `control-error`. Quokka found
out that there was no catch only by unwinding the whole stack looking for
one, and by then every handler the program had set up was gone, so the
error stopped the machine instead of reaching the program. The `THROW`
instruction now walks the stack's records first, as the machine already
did to find out whether any handler exists, and signals the error before
anything unwinds.

## lambda and defmacro as macros

CLHS makes `lambda` and `defmacro` macros. Quokka's interpreter evaluates
both directly, and the compiler compiles them directly to match, so
`macro-function` said neither was a macro. They now have expanders in the
library, which only `macro-function` and `macroexpand` see.

`lambda` expands as SBCL's does, into `(function (lambda ...))`. For
`defmacro` we needed a form that compiles to what the compiler makes of a
`defmacro`: a call that puts a macro object in the name's function cell.
The expander function itself was the missing piece, so we gave the
compiler one more internal form, `(%macro-lambda lambda-list body...)`,
which compiles to exactly the function `defmacro` builds, and the
expansion is

```lisp
(progn
  (%set-symbol-function 'name (%make-macro (%macro-lambda lambda-list body...)))
  'name)
```

## The order inside incf

`(incf place delta)` adds DELTA to the value of PLACE. CLHS 5.1.3 fixes
the order: the place's own subforms first, then DELTA, and only then is
the place read. With a variable place, `(incf x (setf x 1))` therefore
reads `x` after the delta has set it to 1, and gives 2. Quokka's `incf`
read the place before evaluating DELTA and gave 1. The library's
`define-modify-macro` now binds the arguments to temporaries before the
place is read.

## What did it cost?

About forty lines of Rust: the stack walk for `throw`, the declarations
in front of the rewrite's `let*`, and nested patterns in `&optional` and
`&key`. The compiler grew by a dozen small functions and the prelude by
two. Because the compiler and the prelude changed, the fixpoint had real
work to do, and the three generations still came out the same.
