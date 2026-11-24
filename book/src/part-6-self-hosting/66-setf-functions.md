# Functions named (setf name)

> *Where this fits.* Chapter 43 gave Quokka `setf` expanders, the general
> way of teaching `setf` a new place, and decided then that Quokka would
> not have the other way, a function named `(setf name)`. ansi-test
> counted eighteen failures that came from that decision. This chapter
> adds setf functions and shows how little machinery they need once they
> are stored the right way. It also fixes two things they brought to
> light: the implicit block around a local function, and `apply` as a
> place.

*Status: done, issue #78. Step AE of phase 2.*

## Another kind of function name

Suppose you keep a value in the car of a box and want `setf` to work on
it. With what chapter 43 built you would write an expander:

```lisp
(defun box-value (box) (car box))
(defsetf box-value (box) (new) `(setf (car ,box) ,new))
```

Common Lisp also lets you write the writer as an ordinary function, named
by the list `(setf box-value)`:

```lisp
(defun (setf box-value) (new box) (setf (car box) new))
```

When `setf` meets a place `(box-value b)` and knows no expander for it,
CLHS 5.1.2.9 says it calls that function with the new value first and the
place's arguments after it. So a function name is a symbol or a list
`(setf symbol)`, and the list form works everywhere a function name does:
in `defun`, in `flet` and `labels`, after `#'`, and as the argument of
`fdefinition`, `fboundp` and `fmakunbound`.

That is six places in the system that receive a function name and, until
now, expected a symbol. Each of them ends up at a symbol's function cell:
`#'f` compiles to an instruction that reads `f`'s cell, `flet` binds `f`
in a scope the compiler keeps by symbol, and `fboundp` is a primitive
that looks in the cell.

## Where to keep one

We considered two ways of storing a setf function. The first is a table
from names to functions beside the symbols, which is roughly what SBCL
does: it keeps an object for each function name, symbol or list, in a
global database. In Quokka every one of the six places would then need a
second path for a name that is a list, and the compiler would need a new
instruction to reach the table.

The second is what CLISP does. For each name `foo` that gets a setf
function, CLISP makes an uninterned symbol whose name is the string
`"(SETF FOO)"` and keeps it on `foo`'s property list. The function lives
in that symbol's function cell. Now every place that receives a function
name only has to turn `(setf foo)` into that symbol, and from there the
existing machinery does the rest. We chose this way, because it adds a
mapping and no mechanism.

The mapping is in the prelude, since `defun` needs it and the prelude is
where `defun` is defined:

```lisp
(defun %setf-function-symbol (name)
  (let ((found (%setf-function-symbol-if-any name)))
    (if found
        found
        (let ((symbol (make-symbol (%string-append "(SETF "
                                                   (%string-append (symbol-name name) ")")))))
          (%set-symbol-plist name (cons '%setf-function (cons symbol (symbol-plist name))))
          symbol))))
```

The symbol is uninterned, so it belongs to no package and nothing can
find it by reading `(SETF FOO)`; only the property list leads to it.
`%setf-function-symbol-if-any` is the same search without the making,
for `fboundp` and `fdefinition`, which should answer a question about a
name without changing anything.

With that, `defun` of a list name expands into the same
`%set-symbol-function` it always used, on the hidden symbol. The compiler
turns `#'(setf foo)` into `#'` of the hidden symbol. `flet` and `labels`
put the hidden symbol in their scope, so `#'(setf foo)` inside the body
finds the local function first, as it should. `fboundp` and `fmakunbound`
were primitives that wanted a symbol; we renamed the primitives
`%fboundp` and `%fmakunbound` and wrote the standard functions in Lisp on
top of them, mapping the name first.

## What setf does with an unknown place

The last piece is in the library's `get-setf-expansion`. A place that is
a variable, a place the prelude knows, a place with an expander, and a
macro that expands to a place were all handled already. Anything else
used to be an error, "SETF does not know how to set this place". It is
now a call of the setf function:

```lisp
(defun %setf-function-expansion (place)
  (if (not (symbolp (car place))) (%setf-unknown-place place))
  (multiple-value-bind (vars vals args) (%temporaries (cdr place))
    (let ((store (gensym "NEW")))
      (values vars vals (list store)
              `(funcall #'(setf ,(car place)) ,store ,@args)
              (cons (car place) args)))))
```

The place's arguments are bound to temporaries first, then the new value,
and only then is the function called, so `(setf (box-value (f)) (g))`
calls `f` before `g` as CLHS 5.1.1.1 requires. The expansion does not ask
whether the function exists. SBCL does not either: you can write a
function that sets a place before you define the setf function for it.

`incf`, `push` and every other macro that reads and writes a place go
through `get-setf-expansion`, so they work on such a place without any
change of their own.

## The block around a local function

`defun` puts a function's body in a block named after the function, so
`(return-from f 15)` leaves `f`. CLHS gives `flet` and `labels` functions
the same block, and Quokka's had none: `(flet ((f () (return-from f 15)
35)) (f))` was refused at compile time. ansi-test has a test for it, and
setf functions needed the answer to a small question, which is what the
block of `(setf f)` is called. It is called `f`.

The prelude's `defun` adds its block only when the body mentions
`return-from` with the function's name (chapter 15 explains why: a block
cancels the tail call that lets a recursive loop run in constant space).
We moved that test into a function, `%block-body`, so the compiler's
`flet` and `labels` use the same rule. While doing that we found that the
block had always been put around the declarations at the start of the
body, where a `special` declaration is no longer a declaration but an
ordinary form, so it declared nothing. `%block-body` now keeps
declarations and a documentation string in front of the block.

## apply as a place

CLHS 5.1.2.5 makes `(apply #'aref v subscripts)` a place, and more
generally `(apply #'name args)` for any name with a setf function. We
wrote it as a `define-setf-expander` in `setf.lisp`: for `aref`, `bit`
and `sbit` the writer is a small function that puts the new value last,
where `%set-aref` wants it, and for any other name it is
`(apply #'(setf name) new args...)`.

## What did it cost?

No new Rust, apart from the two renamed primitives. The prelude grew by
the mapping and `%block-body`, about fifty lines; the compiler by two
small functions for local names and local bodies. The differential file
`setf-functions.lisp` checks all of it against SBCL, including the order
of evaluation and the local setf functions.

Because the prelude and the compiler changed, the fixpoint had something
to check: the compiler compiles itself with the new `defun`, and C1, C2
and C3, the three generations of the compiled world (chapter 42), still
came out the same.
