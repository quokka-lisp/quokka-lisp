# Functions and scope

> **Where this fits.** The machine from chapter 9 can evaluate arithmetic and
> make decisions. It cannot define anything. This chapter adds functions of your
> own — which needs somewhere for their parameters to live, and raises a
> question about recursion that decides whether loops are usable at all.

## Three operators that are easy to confuse

`lambda`, `defun` and `progn` turn up together constantly, and a newcomer could
be forgiven for thinking they are variations on one idea. They are not. They do
three unrelated jobs, and seeing the difference clearly makes the rest of this
chapter simple.

### `lambda` makes a function

```lisp
(lambda (x) (* x x))
```

This **is** a function. It has no name, and it does not need one. Evaluating it
produces an object you can pass around, store in a variable, or call.

What it does *not* do is evaluate `(* x x)`. That happens later, when the
function is called, possibly many times, possibly never. Which is why `lambda`
must be a special operator: an ordinary function would have evaluated its
arguments on the way in, and `x` does not exist yet.

### `defun` gives a function a name

```lisp
(defun square (x) (* x x))
```

This makes exactly the same function and then puts it in the symbol `SQUARE`'s
**function cell** — the Lisp-2 slot from chapter 9. It is barely more than:

```lisp
(setf (symbol-function 'square) (lambda (x) (* x x)))
```

In Common Lisp `defun` really is defined that way: it is a *macro* that expands
into the longer form. Quokka has no macros yet, so for now it is a special
operator, with a comment saying it should stop being one.

Note what `defun` returns: the **name**, not the function. That is a small thing
and a deliberate one — it is what makes a file of definitions readable when each
line echoes at a REPL.

### `progn` lets many forms stand where one is wanted

```lisp
(if (> x 0)
    (progn (log "positive") (handle x))
    (handle-zero))
```

`if` takes exactly one form per branch. Sometimes you want three. `progn`
evaluates them in order and yields the **last** one's value, so a sequence can
stand wherever a single form is expected.

It is the least glamorous of the three and the one most woven into the others,
because:

> The body of a `lambda`, and the body of a `defun`, are each an **implicit
> `progn`**.

```lisp
(defun f (x)
  (log x)        ; evaluated for its effect
  (* x 2))       ; this one is the answer
```

Nobody writes `progn` there; it is simply how bodies work. In the implementation
all three share one helper, `begin`, which is also where the tail-call rule
below is enforced — one place rather than three.

So: **`lambda` makes a function, `defun` names one, `progn` sequences forms.**
They co-occur because a named function usually has a multi-form body.

## What a closure actually is

Evaluating a `lambda` does not produce code. It produces a **closure**: three
things bundled together.

```text
   closure
   ├── params   (x)
   ├── body     ((* x n))
   └── env      ← the scope it was written in
```

The third is the interesting one, and it is what the word "closure" refers to —
the function is *closed over* its surroundings.

```lisp
(defun adder (n)
  (lambda (x) (+ x n)))

(setq add5 (adder 5))
(funcall add5 37)        ; => 42
```

Look at what has to be true for that to work. By the time `add5` is called,
`adder` has long since returned. Its parameter `n` belongs to a call that no
longer exists. And yet the inner function still knows `n` is 5.

It knows because the closure captured the scope, and the scope is a heap object
that stays alive exactly as long as something refers to it — which the closure
does. Garbage collection and lexical scope turn out to be the same mechanism
seen from two directions.

(Note the `funcall`. `add5` is a *variable* holding a function, so `(add5 37)`
would look in `ADD5`'s function cell and find nothing. That is chapter 9's
Lisp-2 tax, arriving exactly where you would expect it.)

## Scopes, and a question that was badly posed

A scope is one heap object: a link to the enclosing scope, then names and values
in pairs.

```text
   ((n . 5))  ──▶  enclosing  ──▶  nil
```

Looking up a name scans this scope's names, follows the link, and finally falls
back to the symbol's own value cell, which is where globals live. Shadowing
works for free: inner scopes are searched first.

```lisp
(setq x 'global)
((lambda (x) x) 'local)     ; => LOCAL
((lambda (y) x) 'local)     ; => GLOBAL
```

### Three ways to do this

The textbook choice is an **association list** — `((x . 1) (y . 2))`, extended
by consing a new pair onto the front. It is shorter than what Quokka does, and
calling a two-argument function allocates four cons cells.

Quokka uses a **frame chain** instead: one object per call, six words for the
same two arguments, with better locality.

But the real answer is neither. The fast way to look up a variable is **lexical
addressing**: resolve `x` to "scope 2, slot 0" once, and make lookup an index
rather than a search. Every serious Lisp does this.

It is not available here, and the reason is worth understanding. Assigning those
slot numbers requires walking the function body *before* running it, and
recording the answer somewhere — which is a compiler. A tree-walking interpreter
re-reads the source list on every evaluation and has nowhere to put the result.
Quokka's compiler arrives in phase 1.

So the frame chain was chosen for a forward-looking reason rather than a
performance one: **it is the layout lexical addressing uses.** When the compiler
lands, the name scan becomes an index and the data structure does not change. An
association list would have had to be thrown away.

### A note on the question, not the answer

This decision was first put as "association list or vector — which do you
prefer?", which was the wrong question. One option required a component that did
not exist. Offering a choice that is not open wastes the other person's
judgement on a decision governed by sequencing rather than preference.

The useful version is: *here is what is possible now, here is what becomes
possible later, and here is which of the possible ones survives the transition.*

## Tail calls

Now the question that decides whether loops are usable.

### What a tail call is

A call is in **tail position** when its result is immediately the result of the
enclosing function — nothing happens afterwards.

```lisp
(defun down (n)
  (if (= n 0)
      'done
      (down (- n 1))))     ; ← tail: its answer IS down's answer

(defun sum (n)
  (if (= n 0)
      0
      (+ n (sum (- n 1))))) ; ← NOT tail: the + still has to happen
```

In `down`, once the recursive call is made there is nothing left for the outer
call to do. Its frame is dead weight. In `sum` the outer call still has to add
`n`, so the frame is genuinely needed.

### Why this matters more in Lisp

In most languages you write a loop with `while` and recursion is a special
technique. In Lisp, recursion *is* the ordinary way to iterate — and `loop`,
`dolist` and friends are macros that expand into it.

Without tail-call elimination, a loop of a million iterations builds a million
frames and dies. With it, a tail-recursive function uses constant space and is
exactly a loop. The distinction is not an optimisation; it decides whether an
entire natural style of programming works.

### How it fell out of the design

Here is the pleasant part. Look at the Return half of the machine:

```rust
let f = self.mc.car(stack);
let rest = self.mc.cdr(stack);
self.mc.set_register(Register::Stack, rest);   // pop, THEN dispatch
```

The frame is popped *before* it is acted on. And applying a closure pushes
nothing — it just sets `control` to the body and `env` to the new scope.

So trace `(down 5)` to the recursive call. The `IF` frame was popped when the
branch was chosen. The `ARGS` frame was popped when the arguments were complete.
The stack is empty, and `down` is entered with nothing outstanding. That is a
tail call, with no mechanism.

And `sum` still works correctly for the same reason: when `(sum (- n 1))` is
evaluated, the `ARGS` frame for `+` is *still on the stack*, because `+` has not
had its arguments yet. The frame is retained precisely when it is needed.

Tail-call elimination was not added. It was already there, as a consequence of
"pop before you dispatch".

### What had to be done deliberately

One rule, and it is the whole of the work:

> An operator whose last sub-form is in **tail position** must start that
> sub-form with no frame outstanding.

`progn` is where this bites. Evaluating all but the last form needs a frame to
remember the rest — but the last form must be started with that frame *popped*,
not pushed again. Get it wrong and every function with a two-form body loses its
tail calls, silently.

The `begin` helper does this once, and `progn`, `lambda` bodies and `defun`
bodies all go through it. That is why it is one function and not three.

This is also why the decision was worth making *before* writing the remaining
nineteen special operators rather than after. One rule applied in one place is
easy. The same rule rediscovered in twenty places, some of them wrong, is a
month of confusing bug reports.

### Measuring it, and the test that guards the test

A tail call is a claim about **stack depth**, so the test measures depth rather
than inferring it from the fact that nothing crashed:

```rust
let mut machine = Machine::start(mc, call, Value::NIL);
while run_slice(&mut machine, 50) == Status::Yielded {
    deepest = deepest.max(machine.stack_depth());
}
assert!(max_depth < 20, "the stack reached {max_depth} frames");
```

Twenty thousand iterations, fewer than twenty frames.

And then a second test, which is the one worth copying:

```rust
assert!(max_depth > 100, "only {max_depth} frames; the stack should grow here");
```

Non-tail recursion must *still consume frames*. Because a machine that silently
dropped frames — that simply forgot to push when it should have — would sail
through the first test while being catastrophically wrong. The first test says
"this is efficient"; the second says "and it is still correct".

Any time a test asserts that something is absent, it is worth asking what else
would make it absent, and testing that the thing is present when it should be.

## A bug worth the retelling

`(setq a 1 b 2)` assigned `a` and silently ignored `b`.

The cause is specific to this style of machine. The function that handles the
next assignment is reached from **both halves of the cycle** — from the Eval
half when `setq` is first seen, and from the Return half after a value arrives.
It set `control` correctly but did not set the *mode*. Arriving from Return, the
machine's next step was therefore another Return, which popped the frame that
had just been pushed.

The general hazard: in a state machine where the mode is implicit, any function
reachable from two states is a trap. Every transition now says which half it is
entering, even where it is obvious, because "obvious" is what made this one hard
to see.

## Where this leaves us

```
quokka> (defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
FACT
quokka> (fact 20)
2432902008176640000
quokka> (down 100000)
DONE
```

That first one is the phase 0 gate, and `20!` fitting in a fixnum is almost
certainly why it asks for twenty rather than a rounder number — the largest
factorial needing no bignums.

Six special operators of twenty-five. Missing: `let`, `cond`, `labels`,
`&rest` parameters, and macros — which is what finally makes `defun` stop being
a special operator and become what it is in real Common Lisp.

All of them follow the shape these six established, which is exactly why they
were left until the shape existed.
