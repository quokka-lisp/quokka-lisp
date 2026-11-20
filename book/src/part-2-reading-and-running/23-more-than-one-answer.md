# More than one answer

> **Where this fits.** This is the last chapter of Part II, and it finishes
> tier 0. It adds the three special operators that were outstanding, and one
> of them needs something the machine has not had: the ability for a form to
> return a number of values other than one. That turns out to touch almost
> every other operator, which is the interesting part.

## The problem with returning one thing

```lisp
(gethash 'missing table)   ; => NIL
```

Did the table hold `nil` under that key, or hold nothing at all? From one
return value you cannot tell, and the difference matters to anything using a
hash table seriously.

Common Lisp's answer is that a form may return **any number of values**:

```lisp
(gethash 'k table)         ; => NIL, NIL   — not there
(gethash 'k table)         ; => NIL, T     — there, and it is nil
```

The same shape appears all over the standard — `macroexpand-1` returns the
expansion and whether it expanded, `floor` returns a quotient and a remainder,
`intern` returns a symbol and how it was found. Each is a case where one
answer leaves a question open and a second value closes it, without the cost
of allocating a list or defining a structure.

This is genuinely different from returning a tuple. A tuple is one value that
happens to have parts, and every caller has to take it apart. Multiple values
are **free to ignore**: a caller that wants one writes nothing special, and
the extras evaporate.

```lisp
(+ (gethash 'a table) 1)   ; the second value simply is not there
```

That evaporation is the whole design, and it is what makes the feature
pleasant rather than a tax.

## How a machine with one Value register does this

Chapter 9's machine has a `Value` register holding the result of the last
step. The obvious change is to make it hold a list always, and the obvious
change is wrong: every form in the system returns one value, so every form
would pay for a cons that nothing reads.

Instead there is a second register, and a convention:

```rust
/// **All** the values the last step produced, or [`Value::UNBOUND`].
///
/// ...this holds `UNBOUND` to mean "one value, in the Value register" — the
/// common case, costing nothing.
Values,
```

`UNBOUND` is the marker from chapter 8 that means "no value here", reused to
mean "nothing unusual happened". The ordinary path is untouched: a step
delivers into `Value`, sets `Values` to `UNBOUND`, and no allocation occurs.
Only `(values ...)` and the handful of forms that look fill the second
register in.

The clearing is not an optimisation, it is the semantics:

```rust
fn deliver(&mut self, v: Value<'h>) -> Transition {
    self.mc.set_register(Register::Value, v);
    self.mc.set_register(Register::Values, Value::UNBOUND);
    ...
}
```

Forget that line and a stale second value survives into the next form that
looks — which would be a bug appearing a very long way from its cause.

## The part that touched everything

With the registers in place, the two operators are small. `multiple-value-call`
is an ordinary call whose argument frame collects *lists* and appends them
rather than collecting values and consing them. `multiple-value-prog1` is a
frame that holds the first form's values while the rest run.

The work was elsewhere, and it was not anticipated. A great many forms are
**transparent to values** — they return whatever their last form returned,
however many that was:

```lisp
(multiple-value-list (block b (values 1 2)))          ; => (1 2)
(multiple-value-list (let ((x 1)) (values x 2)))      ; => (1 2)
(multiple-value-list (progn 'ignored (values 1 2)))   ; => (1 2)
(multiple-value-list (if t (values 1 2) 'no))         ; => (1 2)
(multiple-value-list (unwind-protect (values 1 2) 'c)); => (1 2)
```

Most of those were already right, because a form in tail position simply does
not re-deliver — the inner form's delivery stands and the outer frame never
touches it. Tail position, built in chapter 10 for an entirely different
reason, turns out to carry multiple values for free.

Three were wrong, and each in the same way: the frame explicitly read the
`Value` register and delivered it again.

```rust
// A block or catch that was left normally: its value is the value.
frame::BLOCK | frame::CATCH => {
    let value = self.mc.register(Register::Value);
    self.deliver(value)          // ← clears Values
}
```

That is *almost* right, and almost right is the dangerous kind. It returns the
correct primary value and silently discards the rest. The fix is a function
that does nothing at all to the registers:

```rust
fn deliver_unchanged(&mut self) -> Transition {
    self.mc.set_mode(Mode::Return.as_u8());
    Transition::Continue
}
```

`unwind-protect` was harder, and worth following. Its cleanup runs *between*
the protected form finishing and its value being returned, so the value has to
be held somewhere across the cleanup — in a `CLEANUP` frame. That frame saved
one value, so `(unwind-protect (values 1 2) 'cleanup)` returned `(1)`. Saving
the whole list fixed the normal case, and a second line was needed for the
other one: a `throw` can carry several values, and if it passes a cleanup on
its way out, the unwind must resume carrying all of them.

Four lines in total, in three places, and each one found by a differential
test rather than by thinking about it.

## macrolet, which cost nothing

```lisp
(macrolet ((twice (x) (list '+ x x)))
  (twice 5))                            ; => 10
```

A macro with a lexical scope. This is the third outstanding operator and it
needed no new machinery whatsoever, because of two decisions made much
earlier:

- the evaluator looks an operator up in the **function namespace** through the
  environment, so `flet` and `labels` already shadow global definitions
  (chapter 10);
- it then asks whether what it found is a **macro object**, which is a
  distinct heap type rather than a flag (chapter 15).

So binding a macro object in the function namespace is all `macrolet` is.
`eval_flet` grew one more boolean and the dispatch grew a line. The whole
operator is:

```rust
special::MACROLET => self.eval_flet(control, false, true),
```

There is one subtlety worth knowing, because it surprises people. A
`macrolet` expander runs in a **null lexical environment** — it cannot see the
variables around it:

```lisp
(let ((n 5))
  (macrolet ((get-n () n))     ; error: N is unbound
    (get-n)))
```

Which looks arbitrary until you remember *when* the expander runs. Expansion
happens before the code runs, so `n` does not have a value yet — there is
nothing for the macro to see. The restriction is not a rule the standard
imposed; it is a fact about time, written down.

## Three apologies withdrawn, again

Chapter 15 said `macroexpand` diverged from the standard because there were no
multiple values. Chapter 19 said the same of `gethash`. Chapter 20 said it of
`ignore-errors`. All three carried a note saying the divergence would go when
`multiple-value-call` arrived.

```lisp
(multiple-value-list (gethash 'k h))        ; => (NIL T) — stored nil
(multiple-value-list (gethash 'x h))        ; => (NIL NIL) — not there
(multiple-value-list (macroexpand-1 '(m 1))); => ((LIST 1) T)
```

`macroexpand` in particular is now *more* correct than the thing it replaced:
it loops on the second value as the standard says, rather than on whether the
form changed, so the one case where the two differed is gone.

That is the second time this book has recorded a batch of deliberate
shortcuts being repaid. It works because each one was written down **in the
code**, next to the shortcut, saying what would fix it. A note in a commit
message would have been lost; a note in the source was read again by the
person who could act on it.

## Tier 0 is finished

Twenty-two of Common Lisp's twenty-five special operators, which is what
`docs/design.md` asked for — the missing three are `progv`,
`load-time-value` and `symbol-macrolet`, excluded on purpose because an
interpreter can add them later at little cost.

Macros with full lambda lists. Keywords. Vectors, strings, hash tables,
structures. `setf`. A condition system. Tail calls. A collector that survives
being run at every allocation. Seventeen Common Lisp files that produce
byte-identical output on Quokka and on SBCL.

And a prelude of several hundred lines of Lisp doing work that is not in the
Rust at all.

Which means the next thing is phase 1, and it is the one this book has been
pointing at since [The thing that builds itself](../the-bootstrap.md): a
compiler, written in Lisp, interpreted once by the machine Part II built, to
produce an image containing a compiler that never needs interpreting again.

---

**References.** CLHS 3.1.7 for multiple values and the exhaustive list of
which forms pass them through — worth reading precisely because the list is
longer than anyone guesses. *Common Lisp the Language, 2nd edition*, §7.10,
for the design argument against returning a list instead.
