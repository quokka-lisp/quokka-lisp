# The machine's errors

> *Where this fits.* Chapter 80 wrote `signal` and `error` in Lisp, with
> handlers that run where a condition is signalled. One kind of error still
> went the old way: an error the machine itself finds, such as `(car 5)`, an
> unbound variable, or a call of something that is not a function. This
> chapter sends those through `signal` too, as conditions with the slots
> CLHS gives them, so that a handler can ask a `type-error` which value was
> wrong and what it should have been. After it, every error in a world with
> the library is signalled in one way, which is what the restarts of the
> next chapter need.

*Status: done, issue #125. Phase 3, step B, sub-step 3.*

## What the machine knew and did not say

When the machine meets `(car 5)`, the primitive `car` returns a Rust error
that says, in words, `CAR wants a list`. Until this step the machine turned
that into a condition object of its own, a type symbol and the message, and
unwound to the innermost `handler-case`. Two things were wrong with that
once `handler-bind` existed.

The first is where the handler runs. A `handler-bind` around `(car 5)` was
never asked: the unwind went straight past it to a `handler-case`, and with
none open the machine stopped. The review of chapter 80 found a worse case.
Inside a running handler, a `(car 5)` reached a `handler-case` the handler
should not have been able to see (CLHS 9.1.4.1), because the unwind knew
nothing of the floor that hides it.

The second is what the condition knows. CLHS gives a `type-error` two slots,
the value that had the wrong type and the type it should have had, and a
program reads them with `type-error-datum` and `type-error-expected-type`.
ansi-test does this hundreds of times. The machine's condition had only the
message, so the value 5 was somewhere in a string.

## Calling Lisp at the point of the error

The machine already had a way to hand a problem to Lisp in the middle of an
instruction. When `+` overflows a fixnum, it calls the library's
`%generic-+` with the same arguments, and the result is the result of the
`+` (chapter 47). An error is handled the same way. Once the library
defines `%signal-machine-error`, the machine's `fail` calls it, at the point
of the error, with the condition's type, the machine's message, and a
property list of what the machine knows:

```lisp
(%signal-machine-error 'type-error "CAR wants a list"
                       '(:operation car :operands (5)))
```

The function makes the condition and calls `error`, so the condition goes
through `signal` like any other: `handler-bind` sees it with the failing
frames still on the stack, the floor is respected, and the `handler-case`
that takes it is chosen by `signal`. It never returns, since `error` does
not. Before the library is loaded, in a world being built, and in the
interpreter, the machine does what it did before.

What the machine knows depends on where the error is. An unbound variable
or an undefined function is found by the instruction that reads the
symbol, and the machine passes the symbol as `:name`, which is the slot of
a `cell-error`. A call of a value that is not a function passes the value
as `:datum`, with `(or function symbol)` as the type expected, which is
SBCL's answer: a function or a name of one. An index out of bounds passes
the index and `(integer 0 (length))`. A primitive that failed passes its
name and its arguments.

## Which argument was wrong?

The last case leaves a question. `(+ 1 'a)` fails with the operands `(1 a)`,
and a `type-error` wants one datum and one type. The Rust knows, at the
line where it gave up, which argument it was looking at, but it says only
`+ wants numbers`, in words, and an error in Rust cannot carry a Lisp value:
it outlives the heap it came from.

We considered two ways. One was to change every place in the primitives
that reports a wrong type, about sixty of them, to say which argument and
which type, as a number and a type's name. The other was to let Lisp work
it out from a table of the types each primitive takes, by argument:

```lisp
(defvar %primitive-argument-types
  '((car list) (cdr list) (rplaca cons) (rplacd cons)
    (symbol-name symbol) ...
    (+ &rest number) (- &rest number) ...
    (< &rest real) (> &rest real) (rem real real) ...))
```

The first argument that is not of its type is the datum, and its type the
expected type: for `(+ 1 'a)`, `a` and `number`. We chose the table. The
types a function takes are facts about the language, which rule 1 puts
above the primitive table, and SBCL finds its datums the same way, from the
types its functions declare. A primitive missing from the table still
signals its `type-error`, without a datum, reporting the machine's
message; the table grows as tests ask for more.

## Errors inside errors

An error while making the condition for an error is a trap the machine
could fall into. If `%signal-machine-error` itself fails, every time, with
an error of the machine's, the machine calls it again for that error, and
so on. Two guards stop that. In Rust, a call of the function that cannot
even be started halts the machine. In Lisp, the function counts how deeply
the *making* of conditions is nested, and on the fourth level it stops the
machine with the condition's type and message, without signalling.

Our first version counted the whole signal, handlers included, and the
review of this step found a correct program it halted: a handler that
meets a `(car 5)` of its own, inside a handler that met one, four deep.
That is an ordinary program, and its error must be catchable. Only the
making is counted now; the handlers run outside the count.

## What did it cost?

A caught machine error costs more than it did. `(ignore-errors (car i))`
took about a thousand steps when the machine made its own small condition
and unwound; with a condition that is an instance of its class, initargs
worked out in Lisp, and `signal` asking `typep` of each handler, our first
version took about six and a half thousand. The review measured it, and a
loop of a thousand caught errors no longer fitted a form's step budget.
Most of it was `make-condition`, and most of that was `make-array` parsing
its keyword arguments and `member` parsing its own, in code that every
`make-instance` runs too; with the primitives under them called directly,
a caught `(car i)` takes about three and a half thousand. A program that
uses errors as ordinary control flow pays for that, as it does in most
Lisps.

The Rust is the machine's side of the call: `fail_with`, which takes what
the machine knows about the error and calls the library's function, the
property list it builds, and one slot of the symbol cache to find the
function quickly. The rest is Lisp, about sixty lines. A new differential
file, `machine-errors.lisp`, checks the datums, names and operations of the
machine's errors, `handler-bind` seeing them, and the floor, against SBCL.
ansi-test passes nineteen more tests, 10,824 in all, and the last of the
failures chapter 79 left is gone: every test that does not pass is again
explained by a feature deferred to a later step.

## What is still missing?

Restarts, and with them `warn`, `cerror`, `assert` and the `store-value`
restart of `check-type`. And the debugger, which a world without a handler
for an error should enter instead of stopping.
