# Conditions are objects

> *Where this fits.* Until now an error in Quokka was a small object the
> machine made for itself, with a type, a message and some data, and the
> only thing a program could do with one was catch it with `handler-case`
> (chapter 20 for the interpreter, chapter 27 for the compiler). Part VII
> built CLOS. This chapter makes a condition an instance of a class, so that
> a program can define its own kinds of condition with slots of their own,
> and say how each is reported. It is the first of five steps that turn
> Quokka's error handling into Common Lisp's condition system, with
> handlers that run where the error happened and restarts that let a
> program carry on; those need conditions that are objects first.

*Status: done, issue #125. Phase 3, step B, sub-step 1. Conditions are
still signalled the old way; the next chapter changes that.*

## A condition with slots of its own

Suppose a program reads configuration files, and wants to report a bad
line by saying which file and which line. In Common Lisp it defines a
condition type for that:

```lisp
(define-condition bad-line (error)
  ((file :initarg :file :reader bad-line-file)
   (line :initarg :line :reader bad-line-line))
  (:report (lambda (condition stream)
             (format stream "line ~D of ~A makes no sense"
                     (bad-line-line condition) (bad-line-file condition)))))
```

`define-condition` looks like `defclass`, and does nearly the same thing:
it makes a class, `bad-line`, whose superclass is `error`, with two slots
and a reader for each. `make-condition` makes one:

```lisp
(make-condition 'bad-line :file "site.conf" :line 12)
```

and a handler that catches it can ask it which file and which line, with
the readers, or print it. `princ` prints its *report*:

```lisp
(princ (make-condition 'bad-line :file "site.conf" :line 12))
;; line 12 of site.conf makes no sense
```

The report is what a person sees when the error reaches the top. Without
`:report`, the condition would print as `Condition BAD-LINE was
signalled.`, which is true and not much help.

Quokka's old condition object had none of this. Its type was a symbol, and
everything else had to fit into a message string and a list of data. A
program could not define a type with slots, nor a report, and a handler
could not ask a `type-error` which value had the wrong type: the value was
somewhere in the message, as text.

## Why conditions became instances

We had two ways to give conditions slots. One was to keep the machine's
condition object and teach `slot-value` and the readers about it as a
second kind of object with slots. The other was to make a condition an
instance of a class, exactly like the instances of chapter 73. We chose the
second, as SBCL does: CLOS already has slots, readers, inheritance,
redefinition and `print-object`, and a condition with all of them for free
is better than a second set of the same things for one kind of object.

A condition class is not quite a standard class, though. CLHS leaves it
open whether a condition is a `standard-object`, and in SBCL it is not: a
condition class has its own metaclass, `condition-class`, and a condition
cannot be a superclass of a standard class or the other way round.
Chapter 73 already made a `condition-class` for each standard condition
type, so that `class-of` had something to return for an error. What was
missing was that `ensure-class`, the function `defclass` expands into,
refused to make or redefine any class but a standard one. It now makes
condition classes too, with three rules of their own: a condition class
redefines only a condition class, its default superclass is `condition`
rather than `standard-object`, and its superclasses must be condition
classes. `define-condition` is then a macro over `ensure-class`, with the
same slot syntax as `defclass`:

```lisp
`(progn
   (ensure-class ',name :metaclass 'condition-class
                 :direct-superclasses ',parent-types
                 :direct-slots (list ,@(mapcar #'%canonical-slot slot-specs))
                 ,@(%canonical-class-options (nreverse class-options)))
   (%set-condition-report (find-class ',name) ,(%report-form report))
   ',name)
```

`make-condition` allocates an instance and fills its slots from the
initargs, the class's default initargs and the slots' initforms, with the
same code `make-instance` uses. Unlike `make-instance`, it does not check
the initargs: SBCL ignores one that no slot takes, and so do we.

## The standard types get their slots

The standard condition types had classes but no slots. Each type CLHS gives
slots to is now defined again in the library, which keeps the class and
adds the slots:

```lisp
(define-condition type-error (error)
  ((datum :initarg :datum :reader type-error-datum)
   (expected-type :initarg :expected-type :reader type-error-expected-type)))
```

A `type-error` now knows the value that had the wrong type and the type it
should have had, and `simple-condition` keeps a format control and its
arguments, formatted only when the condition is reported. The slot names
are in Quokka's own package, as SBCL's are in one of its internal packages:
CLHS names the readers and not the slots, so a program cannot depend on
them.

## The library's own way of signalling

There was one complication, and it came from Quokka's own history. The
library signals about 240 errors, and the compiler signals all of its
errors, in a form that is not Common Lisp's:

```lisp
(error 'program-error "a lambda list wants symbols" form)
```

The second argument is a message and the rest is data, which is what the
old condition object held. In Common Lisp, the arguments after a condition
type are initargs, keyword and value in pairs, and SBCL refuses this call.
Rewriting 240 calls was possible, but the compiler's own calls cannot be
changed freely: the fixpoint, our check that the compiler compiles itself
into the same code generation after generation (chapter 42), depends on
the compiler's source staying what it is unless we mean to change it.

So we kept the form for the library. Class `condition` has two slots of its
own, `%message` and `%data`, which no standard names. Once `error` is
written in Lisp, in the next step, a condition made this way keeps its
message and data there and reports its message. SBCL's
conditions also carry slots of their own that CLHS does not mention, so a
program loses nothing it could rely on. When ansi-test asks for the datum
of a `type-error` that the library signalled, the call that signals it is
rewritten in Common Lisp's form, with `:datum` and `:expected-type`; that
happens in the steps that follow, as the tests find them.

## Reports

A condition prints in two ways. With `*print-escape*` true, as `prin1`
prints, it is an unreadable object, `#<BAD-LINE>`. With it false, as
`princ` and `~A` print, it is its report (CLHS 9.1.3). That is one
`print-object` method on class `condition`:

```lisp
(defmethod print-object ((condition condition) stream)
  (if *print-escape*
      (print-unreadable-object (condition stream :type t :identity t))
      (%report-condition condition stream))
  condition)
```

A report is kept on its class's property list, and the report of a
condition is the first one found in its class precedence list, so a
condition type without `:report` reports as its parent does. A report can
be a string, a function, or the name of a function, which is called by
name when the condition is reported, so that it may be defined after the
condition. The standard types have reports too, in SBCL's words, kept on
one line where SBCL breaks them with the pretty printer, which Quokka does
not have yet: `The value 5 is not of type LIST.`

## Condition types in the type algebra

Chapter 72's `subtypep` treated condition types as a kind of object of
their own, named by symbols, because a condition was the machine's object
and not an instance. Once a condition is an instance, a condition type is a
class, and that kind went: condition types are now terms of the same kind
as the classes `defclass` makes, and a type a program defines with
`define-condition` takes part in `subtypep` like any class. Two condition
types neither of which includes the other may still share members, since a
program can define a condition type with both as parents, so `(subtypep
'(and type-error warning) nil)` answers "don't know", as SBCL's does and as
ours did before.

The machine still makes its own condition objects when `(car 5)` fails,
until the third step of this part. `class-of` maps such an object to the
class its type names, so `typep` and `handler-case` treat both kinds of
condition alike in the meantime.

## What did it cost?

No Rust. A new library file, `conditions.lisp`, of about 230 lines; a few
lines in `ensure-class`; and the condition kind taken out of the type
algebra. A new differential file, `condition-objects.lisp`, checks
`define-condition`, `make-condition`, the readers, reports and types
against SBCL. ansi-test passes 29 more tests, 10,802 in all. Four tests
fail that used to count as waiting for a deferred feature: they read
`type-error-datum` or `cell-error-name` from an error the machine
signalled, which is still the old kind of condition, without slots. They
pass once errors are made as instances, two steps from here.

## What is still missing?

A program can make a condition, but `error` and `signal` cannot signal one
yet: `error` is still the machine's primitive, which makes the old kind of
condition. `handler-bind`, `signal` and `warn` come next, then the
machine's own errors made as conditions in Lisp, then restarts.
