# Method combination

> *Where this fits.* Chapter 74 built generic functions with one rule for
> putting their methods together: the most specific primary method runs,
> `:before`, `:after` and `:around` methods run around it, and
> `call-next-method` passes control down the line. That rule, *standard
> method combination*, is the right one most of the time, but not always.
> This chapter is about the others: the nine that Common Lisp defines, and
> `define-method-combination`, with which a program writes its own. After
> it, a generic function can add up, collect or chain what its methods
> return, in whatever way the program needs.

*Status: done, issue #124. Phase 3, step A, sub-step 6.*

## When one method is not the answer

Here is a price that is made of parts. A basic item has a base price, a
gift-wrapped item adds the wrapping, and a gift-wrapped item sent abroad
adds postage on top:

```lisp
(defclass item () ())
(defclass wrapped (item) ())
(defclass abroad (wrapped) ())

(defgeneric price (thing) (:method-combination +))
(defmethod price + ((x item)) 10)
(defmethod price + ((x wrapped)) 2)
(defmethod price + ((x abroad)) 5)

(price (make-instance 'abroad))   ; => 17
```

Under standard combination we would write each method to call
`call-next-method` and add its own part to what comes back, and every
method would have to remember to do it. With `(:method-combination +)`
the generic function does it: each applicable method returns its part,
and the effective method, the single function a call actually runs
(chapter 74), adds them up. The qualifier `+` in each `defmethod` says
that the method takes part in the sum.

Common Lisp defines nine of these *operator combinations*, named after the
operator they apply: `+`, `and`, `append`, `list`, `max`, `min`, `nconc`,
`or` and `progn`. A `progn` generic function runs every applicable method
for its effect, which suits a `shutdown` that each class in a hierarchy
should take part in; an `and` generic function asks every method and
answers true only if all of them do. Each also accepts `:around` methods,
which work as in standard combination, and the option
`:most-specific-last`, which reverses the order in which the methods are
asked:

```lisp
(defgeneric parts (x) (:method-combination list :most-specific-last))
```

## What SBCL does with one method

The details of the nine are where we measured SBCL before writing
anything, and two of them we would not have guessed. The first concerns a
generic function with only one applicable method. `(+ 7)` is 7, so it
would be natural for a `+` generic function with one method to return
that method's value through `+`. The standard says more: each combination
has a property, `:identity-with-one-argument`, and when it is true a
single method is called on its own, so that all of its values come back,
not only the first. Every built-in combination has it except `list`,
because `(list 7)` is not 7. So a `progn` generic function whose only
method returns `(values 1 2)` returns both values, and a `list` one
returns `(1)`, a list of the method's first value.

The second concerns `and` and `or`. These are not functions in Lisp but
operators that stop early: `(or a b)` never evaluates `b` if `a` is true.
The combinations stop too. An `or` generic function stops at the first
method that returns true and returns that one value, and an `and` generic
function stops at the first false one; only the last method asked can
return several values, as in the operators themselves.

## A name and a definition

Chapter 74 left room for this. A generic function already had a slot for
its method combination, which held the symbol `standard`. Now it holds a
*method combination object*, the combination's name together with the
options the `defgeneric` gave, such as `(list :most-specific-last)`. The
name leads to a definition through a table:

```lisp
(defvar %combinations (make-hash-table :test 'eq))
```

A definition is a function of the options that returns a *combiner*, and a
combiner is a function of the generic function and its applicable methods,
most specific first, that returns the effective method. Dispatch calls
the combiner on a cache miss, exactly where it used to call standard
combination directly. We put the name in the generic function and look it
up again on every miss, instead of storing the combiner itself, so that a
combination defined again changes every generic function that uses it;
`define-method-combination` empties every cache, as `defmethod` does
(chapter 74).

Each operator combination is registered by one function:

```lisp
(defun %define-operator-combination (name operator identity)
  (setf (gethash name %combinations)
        (lambda (options)
          (unless (or (null options)
                      (and (null (cdr options))
                           (member (car options) '(:most-specific-first :most-specific-last))))
            (method-combination-error "~S method combination takes one option, an order: ~S"
                                      name options))
          (let ((last (eq (car options) :most-specific-last)))
            (lambda (gf methods)
              (%operator-effective-method gf methods name operator identity last)))))
  (%invalidate-dispatch)
  name)

(%define-operator-combination '+ '+ t)
(%define-operator-combination 'list 'list nil)
```

and the effective method it builds is a closure, as standard combination's
always was. For `and`, `or` and `progn` the closure walks the methods and
stops when the operator would; for every other operator it calls each
method and applies the operator to the values:

```lisp
(lambda (args)
  (apply operator (mapcar (lambda (function) (funcall function args nil)) functions)))
```

The `nil` there is the method's next method. A primary method of an
operator combination has none, so `call-next-method` in one is an error,
and `next-method-p` says false.

## Defining a combination: the short form

A program defines a combination of its own the same way:

```lisp
(defun join (&rest lists) (apply #'append lists))
(define-method-combination join)
```

This is the *short form* of `define-method-combination`. It says that
methods qualified `join` are combined by calling the function `join` on
their values; `:operator` names a different operator, and
`:identity-with-one-argument t` lets a lone method's values through. The
short form is a call to `%define-operator-combination`, so a program's
combinations are exactly as fast as the built-in ones.

The operator may also be a macro. A closure cannot apply a macro to
values, because a macro works on code, so for that case we build the code
and compile it, once per cache miss:

```lisp
(compile nil `(lambda (%emf-args)
                (,operator ,@(mapcar (lambda (function) `(funcall ',function %emf-args nil))
                                     functions))))
```

## The long form

The short form covers combinations that apply one operator to every
method. Anything else, such as a combination with methods of several
kinds, needs the *long form*, and the standard's own example of it is
standard combination written out:

```lisp
(define-method-combination standard2 ()
    ((around (:around))
     (before (:before))
     (primary () :required t)
     (after (:after)))
  (flet ((call-methods (methods)
           (mapcar #'(lambda (method) `(call-method ,method)) methods)))
    (let ((form (if (or before after (rest primary))
                    `(multiple-value-prog1
                         (progn ,@(call-methods before)
                                (call-method ,(first primary) ,(rest primary)))
                       ,@(call-methods (reverse after)))
                    `(call-method ,(first primary)))))
      (if around
          `(call-method ,(first around) (,@(rest around) (make-method ,form)))
          form))))
```

The second line lists *method groups*. Each has a name, here `around`,
`before`, `primary` and `after`, and says which methods belong to it by
their qualifiers: `(:around)` takes methods qualified `:around`, `()`
takes unqualified ones, and `*` would take any. A group can also name a
predicate, a function that decides from the qualifiers, and can ask for
its methods in the other order or insist on having at least one. A method
that no group takes is an error, reported with `invalid-method-error`.

The body then runs with each group's name bound to its methods, and
returns a Lisp form. That form is the *effective method form*, a
description in code of what a call should do, written with two operators
that exist only there. `(call-method m next-methods)` calls the method
`m`, and if `m` calls `call-next-method`, control goes to the first of
`next-methods`, and from there to the next. `(make-method form)` makes
something that can stand in such a list, or be called by `call-method`,
in place of a method: here, the before, primary and after methods
together become one "method" that the around methods can call next.

## Why the long form compiles

The body of a long form returns code, and code is something only the
compiler, or an interpreter, can run. Quokka has no interpreter in a built
world (chapter 32), so the effective method form must be compiled. This
was decided when CLOS was planned (`docs/clos.md`), and it is the one place
where dispatch calls the compiler. It happens once per cache miss, the
first time a generic function is called with a new combination of
argument classes, and the compiled function is cached like any other
effective method:

```lisp
(defun %form-effective-method (gf methods spec form)
  (let* ((shape (%lambda-list-shape (%gf-lambda-list gf)))
         (code `(lambda (%emf-args)
                  (let* ,(mapcar (lambda (entry) (%argument-binding entry (first shape) (second shape)))
                                 spec)
                    (macrolet ((call-method (method &optional next-methods)
                                 (%expand-call-method method next-methods))
                               (make-method (form)
                                 form
                                 (error 'program-error "make-method is used outside call-method")))
                      ,form)))))
    (%with-keyword-check gf methods (compile nil code))))
```

`call-method` and `make-method` are local macros, which the compiler
expands while it compiles the form. The arguments of the call are in
`%emf-args`, a symbol of Quokka's own package `QUOKKA`, so no form a
program writes can capture or shadow it by accident. A `call-method`
expands into a call of the method's function with those arguments and a
chain of the next methods:

```lisp
(defun %expand-call-method (method next-methods)
  (if (%make-method-form-p method)
      (second method)
      `(funcall ',(%method-function method) %emf-args
                ,(if next-methods
                     `(%function-chain (list ,@(mapcar #'%next-method-form next-methods)))
                     nil))))
```

The question was what a `make-method` should be in that chain, and the
answer came from `call-next-method`. A method may call `call-next-method`
with new arguments, and the next method must then see them. A method's
function is a function of the argument list and its next method (chapter
74), so we made a `make-method` a function of the same shape, `(lambda
(%emf-args next) form)`. Inside it `%emf-args` is its own parameter, and
every `call-method` in its form uses the arguments it was called with.

Compiling a form is far more work than making a closure, which costs a
few allocations, so we kept closures for every combination that does not
need the compiler. Standard combination and the nine built-in ones never
compile.

## What we matched in SBCL

The long form leaves several details to the implementation, and we
measured each in SBCL before choosing. A long form may take options, as
`ordered` does here:

```lisp
(define-method-combination ordered (&optional (order :most-specific-first))
  ((methods * :order order))
  `(list ,@(mapcar (lambda (m) `(call-method ,m)) methods)))

(defgeneric g (x) (:method-combination ordered :most-specific-last))
```

SBCL checks an operator combination's options when the `defgeneric` is
evaluated, but a long form's only when the generic function is first
called, and we do the same. SBCL also signals an error for two methods in
one group with the same specializers, which the standard asks for, except
when the combination has a single group that takes every method; we
followed it there too.

Error types turned out to matter, because a differential test prints the
type of every error (chapter 63). `invalid-method-error` and
`method-combination-error` signal a `program-error`, as SBCL's do. A call
with no primary method signals an error that is neither a
`program-error` nor a `simple-error`, and the error for a call to which no
method applies at all turned out to be of the same kind, which Quokka had
been signalling as a `simple-error` since chapter 74. We changed both.

A program can watch the cache being emptied. If `defgeneric` gives a
generic function another combination after it has been called, its old
methods no longer fit (their qualifier is `+`, the new combination wants
`list`), and the next call reports them. The same test defines a
combination again and checks that the next call uses the new definition.

## What the review found

A review of the finished work, probing Quokka and SBCL side by side, found
two faults. The first was in `:arguments`. We had bound each of its
variables, in the body, to a fresh uninterned symbol, which the effective
method then bound to the argument. That handled plain variables, but a
default that named an earlier variable, such as `&optional (y x)`, could
not see it, a supplied-p variable was never bound at all, and `&aux` was
ignored. SBCL does something simpler: in the body each variable is bound
to its own name, and the effective method binds those same names, one
after the other, as `let*` does, so every default sees the variables
before it. We changed to that.

The second was an order of checks. We had made a call with no primary
method report that first under the operator combinations, as SBCL does,
and then left standard combination reporting a method with a qualifier it
does not accept before it looked for a primary method. Both now ask for
the primary method first.

## What did it cost?

No Rust. The table, the operator combinations, both forms of
`define-method-combination` and the compiled effective methods are Lisp in
`generic-functions.lisp`, and the rest of dispatch did not change.
A new differential file, `method-combination.lisp`, has about two hundred
forms checked against SBCL, among them the standard's own long-form
example. The warm rebuild (chapter 41), in which the world recompiles
itself from its source while it runs, reloads this file while generic
functions are alive; the rebuild test now defines a short and a long
combination before the rebuild and calls generic functions using them
after it.

## What is still missing?

The metaobject protocol's side of method combination: `find-method-combination`,
`compute-effective-method` as a generic function a program can
specialize, and `method-combination` objects that describe themselves.
They belong to sub-step 7, together with the rest of the introspection
that the MOP provides. `documentation` of a method combination is
deferred with every other `documentation` (tier 2).
