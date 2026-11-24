# What a macro is given

> *Where this fits.* Chapter 18 showed how Quokka binds a parameter list: it
> rewrites what people write into a simple list the machine can bind, plus a
> `let*` that computes the rest, and it takes a macro's patterns apart in
> generated `car` and `cdr`. Chapters 62 and 63 sorted ansi-test's remaining
> failures by cause. This chapter takes the four causes that live in
> parameter lists. One of them changes what a macro is: from here on an
> expander is given the whole macro form and the environment, as in Common
> Lisp, and no longer just its arguments.

*Status: done, issue #76. Step AC of phase 2.*

## The form that went missing

`&whole` lets a macro see the form it was called with, head and all:

```lisp
(defmacro show (&whole w arg)
  `(list ',w ',arg))

(show x)   ; => ((SHOW X) X)
```

A macro wants this when it has to return its own form unchanged, which is
how a compiler macro declines to rewrite a call, or when it reports an error
and wants to quote what the programmer wrote. Chapter 18 listed `&whole` as
missing, and gave the reason: Quokka called an expander with the argument
forms only. The compiler expanded `(show x)` with

```lisp
(apply expander (cdr form))
```

so `show`'s expander received `x`, and the list `(show x)` never reached it.
Since the rewrite did not know `&whole`, it read `&whole` and `w` as two
ordinary parameters, and every such call failed with "wants 2 arguments and
got 1".
Eleven ansi-test tests failed this way.

Inside a nested pattern, `&whole` is easy: it names the list that pattern
matches, which the generated code already holds in its cursor (chapter 18).
At the top of a macro it has to name the form, and we had three ways to get
the form to the expander.

The first was to leave the calling convention alone and record the form
beside the arguments, in a compiler variable, the way chapter 44 already
recorded the environment an expansion happens in. That is small, about ten
lines of Lisp. The second was to call every expander as Common Lisp defines
one, as a function of two arguments, the form and the environment (CLHS
3.1.2.1.2.2). Then `&whole` is just the first argument and `&environment` the
second. The third was to rebuild the form from the macro's name and its
arguments, `(cons 'show args)`, which needs nothing new but produces a copy,
and the wrong copy whenever an expander is called on a form whose head is not
the macro's own name, which `macro-function` allows.

Our first choice was the first option, as the smallest change. Then we asked
whether the second was not better, being what Common Lisp does, and whether
it had any consequence worse than touching more places. So we looked for
one. Every macro in the system would change, in the interpreter as well
as the compiler, and the fixpoint (chapter 42, our check that the compiler
compiles itself into exactly the same code generation after generation) would
see every `defmacro` compile differently. Expansion would get a little
slower. A world saved before the change would no longer work with the new
machine. None of these was worse than a lot of change, so we chose the
second.

## Every pattern is a pattern now

Once an expander's own parameters are fixed, `(form env)`, nothing the
programmer wrote can be bound by the machine's argument binder any more. The
binder matches arguments to a parameter list, and the arguments are now
always those two. Everything else, required parameters, `&optional`,
`&rest`, `&key`, has to be taken apart from `(cdr form)` by the generated
code that until now handled only nested patterns:

```lisp
(defmacro show (&whole w arg) ...)

;; becomes an expander of two parameters, #:g1 the form and #:g2 the
;; environment, whose body begins
(let* ((w   #:g1)
       (#:c (cdr #:g1))
       (arg (if (consp #:c) (car #:c) (%destructuring-too-short)))
       (#:c (cdr #:c))
       (#:d (if (null #:c) nil (%destructuring-too-long))))
  ...)
```

`&whole` and `&environment` are bound first, so every later default form can
use them. That fixed one more ansi-test test on the way, MACROLET.38, whose
default form calls `macroexpand` in the macro's environment: when the
compiler bound `&environment` around the body (chapter 44), no default could
see it.

The generated code had never handled `&key` or `&aux`. Chapter 18 refused
them inside nested patterns, because keyword scanning in generated Lisp was
more than it needed then. Now a top-level `&key` in a macro goes through the
same code, so it had to be written. The rest of the list is searched as a
property list by two small prelude functions. `%destructuring-keys` checks
the list once: an even length, and no keyword the pattern does not name,
unless `&allow-other-keys` or a true `:allow-other-keys` among the arguments
allows it. `%destructuring-key` finds the leftmost value for a keyword, or
returns the unsupplied marker (chapter 18) when there is none, and the
default is filled in exactly as an `&optional` one is. As a result `&key` now
works in nested patterns as well, which CLHS allows and Quokka used to
refuse.

`destructuring-bind` uses the same rewrite, as a function of the one list it
takes apart, and `&whole` names that list. It used to `apply` its function to
the list, which failed for a dotted list such as `'(1 . 2)`; now the list is
an ordinary argument, and the pattern `(&whole (a . b) c . d)` takes it apart
like any other. A `nil` in a pattern, which chapter 63 handled in the
library macro, is now the empty pattern in the rewrite itself, in `&optional`
too.

`setf` expanders followed. A `define-setf-expander` is made with `defmacro`
under a hidden name, so its function became a function of the place and the
environment as well, and so did the long form of `defsetf`, which
`get-setf-expansion` now calls the same way. The first test run found this:
`(setf (ldb ...) ...)` and `(setf (values ...) ...)` failed until it was done.

## A default form that saw too much

CLHS 3.4.1 says a default form is evaluated in a scope that holds only the
parameters to its left. ansi-test checks it with a parameter that shadows the
variable its own default reads:

```lisp
(let ((x 1))
  ((lambda (&optional (x (1+ x))) x)))   ; => 2
```

Quokka signalled "+ wants a number". The rewrite bound every parameter in the
simple list first, `x` to the unsupplied marker since no argument was given,
and only then ran the `let*` that fills in defaults. So `(1+ x)` saw the
parameter `x`, still holding the marker, and not the `x` outside.

The fix is to bind each `&optional`, `&rest` and `&key` parameter in the
simple list under a fresh name, and its own name only in the `let*`, in
order:

```lisp
(lambda (&optional #:g1)
  (let* ((x (if (eq #:g1 '<unsupplied>) (1+ x) #:g1)))
    x))
```

Now `(1+ x)` is evaluated before the `let*` binds `x`, so it sees the outer
one. We rename every such parameter, not only the ones a default could see:
deciding that would need each default's free variables, and a macro inside a
default can hide them. The cost is one more local slot per optional or
keyword parameter; the `let*` binding was there already, so a call does no
more work than before.

## Two small checks

`(lambda (&key) ...)` names no keys, but it is still a function that takes
keyword arguments, so `((lambda (&key) 'good) :allow-other-keys nil)` must
return `good`. The binder decided whether to read keyword arguments by
whether there were any keys to read, found none, and complained about two
arguments too many. It now remembers that it saw `&key`.

`values-list` is intercepted by the machine, which turns a list into multiple
values directly (chapter 23). The interception came before the primitive
table's arity check, so `(values-list)` and `(values-list nil nil)` were
accepted, and a dotted list such as `'(a b . c)` lost its tail without a
word. It now signals a `program-error` for the wrong number of arguments and
a `type-error` for a list that is not a proper one.

## What did it cost?

The Rust grew by 176 lines, against an estimate of 80 to 120 made before we
chose the faithful convention, so we raised the budget's limit to 15,713 in
the same commit. Most of it is in `lambda_list.rs`: the macro rewrite,
`&whole`, `&environment`, `&key` and `&aux` in generated code, and the fresh
names. `values-list`'s checks are 17 lines of `vm.rs`. The image format went
to 6, because a world saved before this step would call the new expanders the
old way.

In Lisp, the prelude gained the four small keyword functions, while the
compiler lost its own `&environment` handling and the library its `nil`
pattern walk from chapter 63, since the rewrite does both now.

The fresh names have a cost we had not predicted. Each is an uninterned
symbol, and a compiled function keeps its simple lambda list, so every
function with an optional or keyword parameter now carries one more symbol
per parameter in the image. The renaming alone made the cold-built world
grow from 1,024,576 bytes to 1,098,008, about seven per cent. The slower expansion we expected did not
show: the differential tests, which rebuild the whole system twice, took
45 seconds before and after, and the fixpoint and the warm rebuild passed
unchanged.

ansi-test went from 7,980 tests passing to 8,018, and none was lost. That is
12 more than the 26 the four causes accounted for: with `&key` and `&rest`
now handled by the generated code, `&key` inside a nested pattern and a
pattern after `&rest` work too, and tests that used them had been refused
before they ran. The plain failures went from 148 to 121, and the tests that
escaped the runner altogether from 18 to 7.

## What is still missing?

SBCL refuses a pattern after a macro's top-level `&whole`, as in
`(&whole (head a b) c d)`, although CLHS 3.4.4.1 allows a pattern anywhere a
parameter name can appear and ansi-test expects it to work. Quokka accepts
it, so that case is tested by ansi-test and not in our comparison with SBCL.
A parameter of `&optional` or `&key` is still a name, not a pattern. The
next steps on #39's list need a decision first: arrays whose element type is
`nil`, and `(setf name)` functions.
