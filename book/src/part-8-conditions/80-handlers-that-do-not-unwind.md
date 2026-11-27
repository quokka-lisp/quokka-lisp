# Handlers that do not unwind

> *Where this fits.* Chapter 79 made conditions objects. Signalling one
> still went the old way: the machine unwound the stack to the innermost
> `handler-case`, and the clause that wanted the condition ran there
> (chapters 20 and 27). This chapter writes `signal` and `error` in Lisp,
> and adds `handler-bind`, whose handlers run where the condition was
> signalled, with the frames that signalled it still on the stack. That is
> what a warning needs, and what restarts need: a handler that has looked
> at a condition and decided what should happen must be able to reach back
> into the code that signalled it.

*Status: done, issue #125. Phase 3, step B, sub-step 2. The machine's own
errors, such as `(car 5)`, still unwind straight to a `handler-case`; the
next chapter makes them go through `signal` too.*

## Two ways to handle a condition

Here is a function that logs every warning-like note it is given, and lets
the computation carry on:

```lisp
(define-condition note (condition) ((text :initarg :text :reader note-text)))

(handler-bind ((note (lambda (c) (format t "note: ~A~%" (note-text c)))))
  (signal 'note :text "starting")
  (compute-something)
  (signal 'note :text "done"))
```

`signal` offers the condition to the active handlers. The handler prints
the note and returns, which means *I have looked at it and I decline to do
anything about it*, so `signal` returns nil and the body goes on to the next
form. Nothing was unwound. A handler that wants to stop the computation
says so itself, with a non-local exit (chapter 14): `return-from`, `throw`,
or, in chapter 82, a restart.

`handler-case` is the other way. Its clause runs *after* the stack has been
unwound to the `handler-case`, so by the time the clause sees the condition,
everything between the error and the handler is gone. CLHS defines
`handler-case` as a `handler-bind` whose handler immediately unwinds, so
the first way is the more general one. Quokka had only the second.

## A stack of handlers

The handlers active at any moment are a *dynamic* stack: each
`handler-bind` adds a *cluster* of handlers for the extent of its body,
and they go away when the body is left, whichever way it is left. Chapter
35 built exactly this for special variables: a binding is pushed on the
machine's mark chain, the stack of records that `block`, `catch` and
`unwind-protect` also use, and popped by whatever leaves the body. So a
`handler-bind` is a binding of a special variable, `%handler-clusters`, to
its cluster, the technique SBCL uses with `*handler-clusters*`:

```lisp
(defmacro handler-bind (bindings &body forms)
  ...
  `(let ((%handler-clusters
           (list ,@(mapcar (lambda (binding) `(cons ',(car binding) ,(cadr binding))) bindings))))
     ,@forms))
```

`signal` then walks the clusters from the innermost out and calls each
handler whose type the condition is of.

## The `handler-case` that could not change

That would have been the whole design, with `handler-case` rewritten as a
macro over `handler-bind` as CLHS shows, but for the fixpoint: our check
that the compiler compiles itself into exactly the same code, generation
after generation (chapter 42). The compiler is written with the prelude's
`handler-case`, which compiles into a `HANDLER` record on the mark chain,
and the library's macros must expand the compiler's own source exactly as
the prelude's do. A library `handler-case` over `handler-bind` would
compile the compiler differently.

So both kinds of handler exist, and they must be searched in the order they
were established, whatever their kind: a `handler-case` inside a
`handler-bind` comes before it, and one outside it after. Both are records
on the one mark chain, a `HANDLER` record for each `handler-case` and a
special binding for each `handler-bind`, so the order is already there.
One primitive, `%handler-marks`, walks the chain from the top and returns
both kinds as one list, innermost first. That is the only thing about
handlers Rust has to know; deciding what to do with each is Lisp's:

```lisp
(defun %signal (condition)
  (dolist (entry (%handler-marks '%handler-clusters '%handler-floor) nil)
    (let ((place (cadr entry)))
      (if (eql (car entry) 0)
          (dolist (clause (cddr entry))
            (when (typep condition (car clause))
              (%unwind-to-handler place condition)))
          (dolist (handler (cddr entry))
            (when (typep condition (car handler))
              (let ((%handler-floor place))
                (funcall (cdr handler) condition))))))))
```

An entry for a `handler-case` carries the record's clause list, and when
one of its clauses wants the condition, `signal` unwinds to that record
with a second primitive, `%unwind-to-handler`. Unwinding is the machine's
job; it runs every `unwind-protect` cleanup on the way and puts back every
special variable, as any unwind does. Once there, the clause dispatch the
compiler wrote for the `handler-case` runs as it always has.

## Which clause?

The clause dispatch tests each clause's type against the condition, and
until now the compiler decided most of that while compiling. It listed the
subtypes of a clause's type from the prelude's table of standard condition
types, so the test was a comparison of symbols, and it compiled a clause for
`error` into no test at all, since every condition the machine signalled
was an error.

Both stopped being true. A program can define condition types the
prelude's table does not know, and a condition need not be an error: a
`note` must go past `(handler-case ... (error () ...))` untouched. So a
clause now asks `typep` when it is tried, as `signal` does when it chooses
a record, and the two cannot disagree about which clause wanted the
condition. Only `t` and `condition` still compile to no test.

Our first version compiled every test into a call of a prelude function
that used `typep` when the library was loaded and the old table when it
was not. Ten of the compiler's own tests failed at once: they run
compiled code in a world whose prelude is still interpreted, and compiled
code cannot call an interpreted function (chapter 28). So the choice is
made inline, by calling only primitives and `typep`:

```lisp
(list 'if (list '%fboundp (list 'quote 'class-of))
      (list 'typep condition (list 'quote type))
      (%lattice-test condition type))
```

`class-of` is defined when the library's type system is loaded, and
`typep` is called only then; before, in a world still being built, the
test is the one the compiler always made.

The compiler's own code generation changed, which the fixpoint allows: C1,
C2 and C3 are all compiled by the new compiler, so they still agree.

## What a handler sees while it runs

A handler runs on top of the stack, above everything between the signal and
its own cluster. If it signals a condition itself, should the handlers
between be asked? CLHS 9.1.4.1 says no: while a handler runs, only the
handlers *outside* its cluster are active. Without care, the walk would
find them all again, since they are still on the stack.

`signal` binds a second special variable, `%handler-floor`, to the place of
the handler's cluster for the length of the call. When the walk meets that
binding, it hides every record from the floor up to the binding itself.
Records above the binding are the handler's own, such as a `handler-bind`
inside the handler, and stay visible:

```lisp
(handler-bind ((note (lambda (c) (push :outer log))))
  (handler-bind ((note (lambda (c)
                         (handler-bind ((note (lambda (c) (push :own log))))
                           (signal 'note))
                         (push :inner log))))
    (signal 'note)))
;; log, oldest first: (:OWN :OUTER :INNER :OUTER)
```

The inner handler's own signal reaches its own handler and the outer one,
but not the inner handler again; then the first signal goes on to the
outer handler. SBCL gives the same answer.

## `error` in Lisp

With a way to signal, `error` is three lines: make the condition, signal
it, and if no handler took a non-local exit, stop.

```lisp
(defun error (datum &rest arguments)
  (let ((condition (%designated-condition datum arguments 'simple-error)))
    (%signal condition)
    (%signalled (%condition-type-of condition) (%condition-text condition))))
```

CLHS says `error` should then call the debugger, which is phase 3's step F.
Until then the machine halts with the condition's type and report, as it
always has, and a host reports it.

The arguments to `error` *designate* a condition: a condition itself, a
condition type and its initargs, or a format control and its arguments,
which make a `simple-error` whose report formats them. The library's own
form from chapter 79, a type followed by a message, is a fourth way, and
the message goes into the condition's `%message` slot. Once `error` formats
its control string, a message with a tilde in it means something: one of
the library's messages mentioned `~R`, and became `~~R`.

## What did it cost?

The rebuild, in which a world compiles itself again (chapter 41), was
already within a few per cent of its step budget, sixty times an ordinary
form's, and this step took it over; it finished with a larger budget in
the same time, so it was slowness and not a loop, and the budget is now a
hundred times a form's.

The Rust is three primitives. `%handler-marks` walks the mark chain, as the
machine already did to see whether any `handler-case` was open;
`%unwind-to-handler` starts an unwind whose target is that one record, a
two-slot vector of its place and the condition, which no other record can
match; and `%signalled` stops the machine with an error no handler took.
The rest is Lisp: `handler-bind`, `signal`, `error` and the walk's
interpretation in `conditions.lisp`, and the clause test in the prelude.
A new differential file, `signalling.lisp`, checks the order of handlers,
the floor, and `handler-case` and `handler-bind` nested both ways, against
SBCL.

With `error` written in Lisp, the library can signal a condition with the
initargs CLHS gives it, and two places that ansi-test watches do now: a slot
that has no value signals an `unbound-slot` that knows the slot's name and
the object, and `coerce` a `type-error` that knows the value and the type.
ansi-test passes three more tests, 10,805 in all, and one of chapter 79's
four failures is left; it reads the name from an error the machine made,
which the next chapter turns into an instance.

## What is still missing?

When the machine fails, in `(car 5)` say, it still makes its own condition
and unwinds straight to the innermost `handler-case`, past any
`handler-bind` in between, and with no `handler-case` open it halts, so a
`handler-bind` alone never sees it. The next chapter makes those errors go
through `signal` like any other. `warn` and the restarts come after that.
