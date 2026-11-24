# Errors you can catch

> *Where this fits.* Chapter 14 built unwinding, a way to abandon frames and
> deliver a value somewhere further out, and claimed in passing that the same
> machinery is what a condition system needs. This chapter collects on that
> claim. By the end a Lisp program can catch an error the evaluator signals,
> run its cleanups on the way out, and carry on. It is short, because the
> claim was true.

## What an error has been until now

An error has been a Rust value. `EvalError::UndefinedFunction("FOO")` goes
into the machine, the machine halts, and the REPL prints it.

```lisp
quokka> (no-such-function)
; the function NO-SUCH-FUNCTION is undefined
```

Which is a perfectly good REPL and a dead end for anything else. Nothing
written in Lisp could *do* anything about that error: not retry, not report
it and carry on, not run a cleanup and re-signal. A compiler that wants to
note a bad form and keep compiling the rest of the file cannot be written
against a system where the first bad form stops everything.

So errors have to become objects the program can catch.

## A condition is an object, not an exception

```lisp
(handler-case (car 5)
  (type-error () 'that-was-not-a-list))
```

The name matters. Common Lisp calls these *conditions* rather than
*exceptions*, and the difference is not vocabulary. A condition is signalled
*before* the stack unwinds, and a handler can in principle decide **not** to
unwind, fixing something and resuming where the problem was found. That is
what `handler-bind` and restarts are for, and it is the part of the system
that makes a Lisp debugger able to repair a running program rather than only
inspect a corpse.

None of that is in this chapter. `handler-case` *does* unwind, so it is the
exception-shaped half of the condition system, and it is what tier 0 needs.
Restarts and `handler-bind` are phase 3, where the debugger lives. It is worth
knowing that the thing being built here is the small half.

## A catch that matches on type

Here is the whole mechanism. Chapter 14's `catch` pushes a frame carrying a
marker, and `throw` unwinds the stack until it finds a frame whose marker is
the one it wants:

```lisp
(catch 'tag ... (throw 'tag 42) ...)
```

`handler-case` pushes a frame carrying **clauses**, and signalling unwinds the
stack until it finds a frame with a clause that matches. The only difference
is the question asked of each frame on the way out: *is your marker this
object?* versus *does one of your clauses want this condition?*

Everything else is reused exactly as it was. In particular `unwind-protect`
cleanups run while a condition unwinds past them, and they run innermost
first, because that code was written in chapter 14 and does not know or care
what kind of unwinding it is helping with:

```lisp
(let ((order nil))
  (handler-case
      (unwind-protect
           (unwind-protect (error "x") (setq order (cons 'inner order)))
        (setq order (cons 'outer order)))
    (error () nil))
  order)                              ; => (OUTER INNER)
```

That is the payoff from building the unwinding machinery once, properly,
rather than building `block` and then bolting on something else later.

### Telling the two kinds of unwind apart

The unwind target register holds a marker for a `throw` and the condition
itself for a signal. The code that walks the stack checks which it has:

```rust
if kind == frame::HANDLER && quokka_vm::condition::is_condition(self.mc, target) {
```

This is where the implementation went wrong, instructively.

A condition started as an `ObjectType::Instance`, a general heap object with
slots, which is exactly what a condition is. But `fresh_marker`, the thing
that makes a fresh identity for a `block` or `catch`, also allocates an
`Instance`:

```rust
fn fresh_marker(&mut self) -> Value<'h> {
    self.mc.alloc(ObjectType::Instance, 0)
}
```

So every block marker looked like a condition, and a `return-from` passing
through a `handler-case` was caught by it:

```lisp
(block out (handler-case (return-from out 'returned) (error () 'no)))
; => NO, and it should be RETURNED
```

Worse, a clause naming `error` matched *before* reading the condition's type
slot, so the bug was silent rather than a crash, and the handler simply ran
when it should not have. A clause that did read the slot crashed with "slot 0
is out of range for an Instance of 0 slots", which is what made it findable.

The fix is one line, conditions have their own object type, and the lesson is
older than this project: reusing a type is reusing a name. The collector did
not care, because both are traced objects with slots. Everything above the
collector cared a great deal.

*Later: markers got their own type too, `ObjectType::Marker`, when `defstruct`
needed `Instance` for structures (issue #73). So both of the things that were
confused here ended up with a type of their own, each time because something
else wanted the one they were sharing.*

## What a condition is here, and what it is not

Three slots: a type (a symbol), a message (a string or nil), and some data.

Matching is **flat**. A clause naming `error`, `condition` or `t` catches
everything; any other clause catches a condition whose type symbol is `eq` to
it. There is no hierarchy, because a hierarchy means classes and classes are
phase 3.

Common Lisp's are CLOS instances, and `handler-case` matches by *subtype*:
`simple-error` is an `error` is a `serious-condition` is a `condition`, and a
program defines its own with `define-condition` and inherits from any of them.
Quokka has none of that, which means a program cannot define a condition type
at all. `(error 'my-error)` works, but `my-error` is just a symbol, and in a
real Common Lisp it would have to name a class.

That has a consequence worth noticing, which is that the differential tests
cannot check that half. `(error 'boom)` is not portable, because SBCL requires
`boom` to be a defined condition class. The portable subset is `(error "text")`
and the condition types the evaluator signals by itself, which is what
`tests/lisp/conditions.lisp` sticks to. The rest is Rust tests again.

### The evaluator's errors use the standard's names

When an `EvalError` becomes a condition, it is given Common Lisp's type name:

| Rust error | Condition type |
| --- | --- |
| `UnboundVariable` | `unbound-variable` |
| `UndefinedFunction` | `undefined-function` |
| `WrongType`, `IndexOutOfBounds`, `NotAFunction` | `type-error` |
| `BadSyntax`, `WrongArgumentCount` | `program-error` |
| `NoSuchExit` | `control-error` |
| `IntegerOverflow` | `arithmetic-error` |

So a program written against Common Lisp catches what it expects to, and the
differential tests check that it does: `(handler-case (car 5) (type-error ()
'caught))` agrees with SBCL.

*Later: the table has grown rows rather than changed shape. A bad keyword
argument became its own `BadCall`, a catchable `program-error`, so that
`BadSyntax` could keep meaning something narrower in compiled code, where it
says the compiler emitted faulty bytecode and nothing should catch it. Floats
brought an `Arithmetic` error that carries its own condition name, since
`division-by-zero` and `floating-point-overflow` are different things to
catch.*

### Two errors are not conditions

`HeapExhausted` and `BudgetExhausted` stay Rust errors and stop the machine.

Neither is a fault in the program being run. One is the system out of room,
the other is the caller declining to wait any longer (chapter 2). A handler
that caught them would be catching the *machinery* rather than the program,
and `(handler-case (loop-forever) (error () 'caught))` quietly swallowing a
runaway is a worse outcome than stopping.

This is a judgement call rather than a rule. Common Lisp does have
`storage-condition` for the first, and `docs/design.md` has heap exhaustion
becoming one in phase 3, when there is a hierarchy to put it in and a debugger
to land in.

## Where the line fell

Small, for once.

| Rust | Lisp |
| --- | --- |
| the condition object and its type matching | `handler-case` (a macro over `%handler-case`) |
| a `HANDLER` frame and its clause search | `ignore-errors` |
| `error`, intercepted in the apply path | |

`error` is intercepted the way `funcall` and `apply` are, and for the same
reason: it unwinds the stack, which is the machine's business and not a
primitive's.

The special operator is `%handler-case`, and the name people write is a
prelude macro over it. That is not ceremony, since `handler-case` **is** a
macro in Common Lisp, over `handler-bind` and `block`, and writing it as an
operator here would have made the count of special operators a lie. Chapter
13's twenty-five only means anything while nothing is quietly added to it.

## What this unlocks, and what was waiting

Three things in the system were already written as placeholders waiting for
this. A `setf` with a bad place expanded into a call to
`%SETF-DOES-NOT-KNOW-THIS-PLACE`, an undefined function, because there was
nothing to signal, and the same went for an odd number of forms. And
destructuring patterns did not check their arity at all (chapter 18), because
emitting a check without something to signal into would have meant inventing
an error mechanism to throw away.

All three can now say what they mean. And the compiler, when it is written,
can report a bad form and carry on to the next one, which is the actual reason
tier 0 asks for this.

---

**References.** Kent Pitman, "Exceptional Situations in Lisp" (1990), on why
conditions are not exceptions and what signalling-before-unwinding buys.
*Common Lisp the Language, 2nd edition*, chapter 29, which is the condition
system's design document and reads like one. CLHS 9 for the standard's
version, and `define-condition` for the half this chapter does not build.
