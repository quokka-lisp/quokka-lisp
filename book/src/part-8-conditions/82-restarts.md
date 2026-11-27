# Restarts

> *Where this fits.* Chapters 79 to 81 made every error a condition that a
> handler sees where it was signalled, with the frames that signalled it
> still on the stack. A handler that has seen an error can decline, or
> leave with a non-local exit. This chapter gives it a third choice: to
> take one of the ways out that the code near the error offered. These ways
> out are *restarts*, and they are what lets a program, or later a person in
> the debugger, decide how to carry on from an error without the code that
> found the error having to know who decides.

*Status: done, issue #125. Phase 3, step B, sub-step 4.*

## Who decides what happens next

A function that reads settings from a file finds a line it cannot parse.
It knows several reasonable things to do: skip the line, use a default
value for that setting, or let the caller supply a value. What it does not
know is which one its caller wants, and the caller may be several calls
away. Common Lisp separates the two. The function offers the choices:

```lisp
(defun parse-setting (line)
  (restart-case (error 'bad-line :line line)
    (skip-line () :report "Ignore this line." nil)
    (use-value (value) :report "Use a value instead." value)))
```

and the code that knows what it wants chooses, from a handler:

```lisp
(handler-bind ((bad-line (lambda (c) (invoke-restart 'skip-line))))
  (read-settings "site.conf"))
```

`invoke-restart` finds the innermost restart of that name and calls it.
The restart unwinds to its `restart-case`, which returns what the clause
returns, so `parse-setting` returns nil and the reading goes on with the
next line. The handler never needed to know how `parse-setting` skips a
line, and `parse-setting` never needed to know who was reading.

## Restarts are a dynamic stack too

Restarts are established for the extent of a form and gone when it is
left, exactly as handlers are, so they are kept the same way: in a special
variable, `%restart-clusters`, which `restart-bind` binds to its cluster of
restarts consed onto the clusters outside it. `compute-restarts` lists
them innermost first and `find-restart` looks one up by name. Handlers
needed a primitive to walk the machine's record chain because the
prelude's `handler-case` keeps its own records (chapter 80); restarts have
no such companion, so the binding is all there is to them.

`restart-bind` is the primitive form: each restart is a name and a
function, and invoking it calls the function, on top of the stack.
`restart-case` is the form people write, and CLHS describes its expansion.
Each clause becomes a restart whose function stores its arguments and
leaves by `go` to a tag of the clause's own, where the clause's body runs
with them, after the unwind:

```lisp
(block #:restart-case
  (let ((#:arguments nil))
    (tagbody
       (restart-bind ((skip-line (lambda (&rest temp)
                                   (setq #:arguments temp)
                                   (go #:restart-1))))
         (return-from #:restart-case (error 'bad-line :line line)))
     #:restart-1
       (return-from #:restart-case (apply (lambda () nil) #:arguments)))))
```

The `go` reaches a `tagbody` from inside a closure called much further down
the stack, a non-local exit of the kind chapter 27 built for compiled code,
and it runs every `unwind-protect` cleanup on the way.

## Which restarts belong to which error

A handler often runs while two errors are in flight: one it is handling,
and one signalled by the code it called. A restart that the inner error's
code offered should not be taken for the outer one. CLHS lets a restart be
*associated* with a condition, with `with-condition-restarts`, and then
`find-restart` and `compute-restarts`, given another condition, leave it
out. `restart-case` makes the association by itself when its form is a
call of `error`, `signal`, `cerror` or `warn`: it makes the condition
first, then associates its restarts with that condition while signalling
it.

## The standard restarts

CLHS names five restarts and gives each a function of the same name.
`abort` and `muffle-warning` signal a `control-error` when there is no such
restart; `continue`, `store-value` and `use-value` return nil. The
operators that offer them are now in Lisp:

1. `warn` signals a warning with a `muffle-warning` restart, and if nothing
   muffles it, prints it on `*error-output*` and returns.
2. `cerror` signals an error with a `continue` restart, which returns nil
   from `cerror`.
3. `assert` signals an error until its test is true, with a `continue`
   restart that tries again, asking for new values of the places it names
   first, as SBCL's does.
4. `check-type`, `ccase` and `ctypecase` signal a `type-error` with a
   `store-value` restart that stores a new value in the place and checks
   again. Until now they could only signal; they moved into
   `conditions.lisp`, where `restart-case` is defined.

## Entering the debugger

An error nobody handles goes to `invoke-debugger`. The debugger is phase
3's step F; until it exists, entering it means what CLHS says comes first.
A program's `*debugger-hook*` is called with the condition and itself,
bound to nil while it runs, and if it returns, the innermost `abort`
restart is invoked. With no `abort` restart either, the machine halts with
the condition, as an unhandled error always has.

Writing the test for the hook found a mistake in Quokka's own top level.
The host loop, `%host-step`, ran each form inside a `handler-case` that
caught every error and returned it to the host, so an error at the top
level was handled before it could reach the debugger, and a program's hook
was never called. SBCL calls it. Decision 5 on #125 had said that the top
level should offer an `abort` restart rather than catch errors, and with
restarts in place it does: an unhandled error goes to the debugger, the
hook runs, and the debugger invokes the top level's `abort`, which tells
the host what went wrong.

The test itself could not be one of our differential files. Run as a
script, SBCL disables its debugger by installing a hook of its own, which
runs before `*debugger-hook*` and ends the process, so the oracle never
gets to the form. That test is in Rust, beside the others SBCL cannot
check.

Another test taught us something about SBCL. We wrote:

```lisp
(let ((x 'a))
  (handler-bind ((type-error (lambda (c) (store-value 5 c))))
    (check-type x integer))
  x)
```

expecting 5, and SBCL reported an unhandled `type-error`. Its compiler had
seen that `x` is always `a`, and replaced the `check-type` with a
type error signalled when the form was compiled, with no restart to offer.
With `x` read from a string, so that the compiler cannot know it, SBCL
returns 5, and so does Quokka.

## What did it cost?

No Rust. Restarts, the restart functions, the debugger's entry and the
operators that offer restarts are Lisp, in `conditions.lisp`, and the host
loop is a few lines of `eval.lisp`. A new differential file,
`restarts.lisp`, checks them against SBCL.

ansi-test passes 1,088 more tests, 11,912 in all. Most of them had been
waiting for `assert`, which the suite uses to check what a test computed.
Others that had been waiting for it now run and fail, 426 of them, most in
`format`'s directives; the next chapter goes through them.

## What is still missing?

The debugger, which shows a person the condition and the restarts and lets
them choose; and with it, input from the console. `store-value` and
`assert` ask a person for a new value through `*query-io*`, which is the
console's output for now, so an interactive restart cannot yet be answered.
