# Leaving early

> **Where this fits.** Everything so far has computed values that flow *up* the
> frame stack, one frame at a time. This chapter is about the seven operators
> that leave a computation from the middle of it — and about the single
> mechanism underneath all of them, which turns out to be the same machinery a
> condition system needs.

## The thing that cannot be built out of what exists

Here is a search that gives up as soon as it finds what it wants:

```lisp
(block search
  (walk-the-tree big-tree
    (lambda (node)
      (if (interesting-p node)
          (return-from search node)))))
```

`walk-the-tree` is thirty frames deep by the time `return-from` runs. Those
thirty frames are all waiting for values: a `cons` wants its second argument, an
`if` wants its test. Every one of them must be abandoned, and the node has to
appear as the value of the `block` as if nothing in between had ever happened.

The machine from chapter 9 cannot do that. It has exactly one way to move a
value: hand it to the top frame and let that frame decide. To discard thirty at
once there is nothing to build on.

The same shape appears four more times. A `throw` that unwinds to a caller. A
`go` that jumps backwards to a label. A cleanup that must run even though its
body is being abandoned. None of them is expressible as "return a value".

## A third half-step

Chapter 9's machine alternates between two halves: **Eval** looks at a form,
**Return** hands a value to the top frame. The whole of this chapter is one
more:

**Unwind** — discard frames until one matches, running any cleanups passed on
the way.

And one more register, saying what to match:

```rust
pub enum Register {
    Control,  // the form being worked on
    Env,      // where names are looked up
    Value,    // the result the last step produced
    Stack,    // frames
    Unwind,   // what an unwind is searching for, or nil
}
```

That is the entire addition. Seven operators, one new mode, one new register.

## Three ways in, one way through

`return-from`, `throw` and `go` look like three different features. They are
three ways of finding a target, and nothing else:

```rust
// return-from: look the name up in the environment
let marker = env::lookup_marker(mc, env, env::BLOCK, name);

// go: the same, in the tag namespace
let marker = env::lookup_marker(mc, env, env::TAG, tag);

// throw: no lookup at all — the tag IS the target, and the stack is searched
```

Once a target is in hand all three do exactly the same thing: put it in the
Unwind register, put the value in Value, switch mode. The loop that follows does
not know or care which operator started it.

That uniformity is the design, and it is worth noticing because the surface
syntax hides it completely. The Common Lisp specification describes these as
five separate operators with five separate sections. Underneath they are one
mechanism and three lookups.

## Lexical and dynamic, which is the real distinction

The difference that *does* matter is not between the operators but between two
ways of finding a target.

**`block` and `return-from` are lexical.** The block name is bound in the
environment, so `return-from` can only reach a block it can *see* in the source:

```lisp
(block done
  (helper))          ; helper cannot return-from done.
                     ; It is not written inside the block.
```

**`catch` and `throw` are dynamic.** The tag is an ordinary value, and `throw`
searches the frame stack at run time:

```lisp
(catch 'done
  (helper))          ; helper CAN (throw 'done ...).
                     ; It need not know the catch exists.
```

Both are useful and the trade is clear. Lexical is *safe*: you can see every
place that might jump, and the compiler can too. Dynamic is *flexible*: a
library function can abandon a computation on behalf of a caller it has never
heard of — which is exactly what an error does.

Most languages give you only one. `return` and `break` are lexical; exceptions
are dynamic. Common Lisp is unusual in exposing both as separate primitives
rather than one general mechanism, and that honesty is useful: you say which
you mean.

## Why the target is an object rather than a name

A subtlety that is easy to get wrong and whose consequences are intermittent.

```lisp
(defun f (n)
  (block b
    (if (= n 0)
        (return-from b 'bottom)
        (f (- n 1)))))

(f 5)
```

Five calls are in flight, each inside a `block b`. The innermost
`return-from b` must reach *its own* block, not the outermost one with the same
name.

So a block's identity is not its name. On entry, `block` allocates a **fresh
object** that is equal to nothing but itself, binds the name to it, and stores
it in the frame. `return-from` looks up the name — getting the marker for the
block it is lexically inside — and unwinds to the frame holding *that* object.

```rust
fn fresh_marker(&mut self) -> Value<'h> {
    self.mc.alloc(ObjectType::Instance, 0)
}
```

A zero-slot object: one word, no contents, useful only for being distinguishable
from every other one. The same trick gives `tagbody` its identity.

Had the name been the target, the recursion above would have returned to the
wrong frame — and only when the names collided, which is to say in recursive
functions, which is to say rarely enough to ship.

## `tagbody` and `go`: the primitive loop

`tagbody` is the strangest operator in Common Lisp and one of the most
important.

```lisp
(tagbody
   top
     (setq n (+ n 1))
     (if (< n 5) (go top)))
```

A body containing *labels* — bare symbols — and forms. Execution runs the forms
in order, skipping labels, and `go` jumps to just after one. It always yields
`nil`.

This is assembly language with parentheses, and almost nobody writes it
directly. Its importance is entirely in what expands into it:

> `do`, `dotimes`, `dolist` and `loop` are **macros over `tagbody` and `go`**.

Common Lisp has perhaps twenty iteration constructs and one iteration
*primitive*. That is why they can all exist without twenty implementations in
the VM — and why a language with no `tagbody` must either build looping into the
compiler or do without.

`go` is also the odd one out in this chapter: it does not *leave* a computation,
it jumps within one. But the machinery is identical — unwind to the tagbody's
marker, then resume at the labelled point — which is a decent argument that the
machinery was the right shape.

## `unwind-protect`, which is where it gets hard

```lisp
(unwind-protect
     (process file)
  (close file))
```

The cleanup runs however the body ends: normally, or because something unwound
straight past it. The second case is the one that costs.

An unwind is in the middle of discarding frames. It reaches an `unwind-protect`
frame. The cleanup forms are ordinary Lisp — they must be *evaluated*, which
means Eval mode, a fresh stack of frames, possibly calls. And when that is done
the unwind has to continue to wherever it was going, with the value it was
carrying.

So the unwind **pauses**:

```rust
if kind == frame::PROTECT {
    self.pop();
    let cleanup = self.mc.slot(f, frame::PROTECT_CLEANUP);
    let value = self.mc.register(Register::Value);
    let resume = frame::make_cleanup(self.mc, value, target);  // ← remembers
    self.push(resume);
    return self.begin(cleanup, env);                           // ← ordinary Eval
}
```

The `CLEANUP` frame holds both the value being carried and the target being
sought. When the cleanup finishes and returns to it, it restores them and
restarts the unwind.

Without that frame, `unwind-protect` would be a comment: the cleanup would run
and the unwind would have forgotten where it was going.

Two details fall out:

**Cleanups nest innermost first**, because the unwind meets the inner frame
first. A test pins it, since it is the only order that makes sense and the sort
of thing an implementation could get backwards without anyone noticing for
months.

**The body's value survives and the cleanup's is discarded.**
`(unwind-protect 'kept 'discarded)` is `KEPT`. That also needs the saved value
in the frame, since the cleanup overwrites the Value register as it runs.

## Measuring, and the test that guards the test

Two claims in this chapter are about *stack depth*, which means they should be
measured rather than inferred from the fact that nothing crashed.

A `tagbody` loop must run in constant space — otherwise every looping macro in
the language inherits a limit from the construct it expands into:

```rust
let (deepest, answer) = deepest_stack(
    "(setq n 0) (tagbody top (setq n (+ n 1)) (if (< n 20000) (go top))) n", 50);
assert_eq!(answer, "20000");
assert!(deepest < 20);
```

Twenty thousand iterations, fewer than twenty frames, because the tagbody frame
is *replaced* each time rather than pushed.

And then the one that guards it:

```rust
// 200 nested catches
assert!(deepest > 200 && deepest < 1_000);
```

Nested `catch`es **must** grow the stack. A catch is a frame that has to be
there to be found. A machine that dropped frames it should have kept would sail
through the first test while being catastrophically wrong — so something has to
assert that the stack still grows where it should.

This is the third time the book has made this point, and it generalises past
tests of depth: **whenever a test asserts that something is absent, ask what
else would make it absent.**

## `nil`, for the fourth time

Chapter 11 left `nil` undecided and said what would settle it. Here is the next
instalment, arriving exactly where that chapter predicted.

```lisp
(block nil
  (return-from nil 'done))
```

`(block nil ...)` is not an edge case. It is what `loop`, `do` and `dolist` all
expand into, and what a bare `return` targets. And `nil` is not a symbol in
Quokka, so `block` and `return-from` need a special case:

```rust
fn is_block_name(&self, v: Value<'h>) -> bool {
    self.mc.is_symbol(v) || v.is_nil()
}
```

That was the fourth, after two in `intern` and one in the printer — and it was
the last. It is also not one of the three sources chapter 11 listed: packages,
`symbol-plist` and `symbol-name` were the predicted costs, and block names were
not on the list.

**The places a decision costs something are more numerous than enumerating them
suggests.** That was the argument that finally settled it, and
[The trouble with nil](11-the-trouble-with-nil.md) is the ending: `nil` is a real symbol now, living
in a region of the heap the collector never moves, and the special case shown
above has been deleted.

## A tooling lesson worth more than the operators

When this work was pushed, SBCL disputed fifteen lines of the new test file. The
cause was one word: an accumulator called `log`, and `CL:LOG` is the logarithm.
Binding a `COMMON-LISP` symbol is undefined behaviour, and SBCL's package lock
refuses.

This had happened before, with `car`. The obvious reading is that the same
mistake was made twice and more care was needed.

That reading is wrong, and the difference is the useful part.

**`CAR` was carelessness.** It looks exactly like a Common Lisp symbol. The rule
was written down, in the file next to the tests, and ignored.

**`LOG` was not.** It does not look like one — and neither do `count`,
`search`, `last`, `merge`, `values`, `step`, `time`, `space`, `union` or the
several hundred other ordinary English words among the standard's thousand
external symbols. Following that rule requires recalling membership of a
thousand-element set, every time you name a variable. Nobody can do that, and
asking them to is not a safeguard.

So the two failures need different fixes. The first needs attention. The second
needs a **lookup**:

```lisp
(defun cl-symbol-p (s)
  (multiple-value-bind (found status) (find-symbol (symbol-name s) :cl)
    (and found (eq status :external) (eq found s))))
```

A script that walks every test file and fails with the offending name. It lives
in the SBCL job because SBCL is the only thing here that knows what is in the
package — which is a better answer than approximating the list by hand.

> A rule that can be followed needs discipline. A rule that *cannot* be followed
> reliably needs a machine. Telling them apart is the skill; they look
> identical from the outside, because both present as somebody getting it wrong
> again.

Its first version crashed on the literal `(1 . 2)`, because it recursed with
`remove-if-not`, which needs a proper list. **A checker that crashes on valid
input is worse than no checker**, since its failure looks like the thing it was
meant to detect. It is a walk over car and cdr now, total by construction.

## What this unlocks

Seven operators, and the thing they were really for arrives two phases later.

A **condition system** — `error`, `handler-case`, `signal`, and the restarts
that make "errors are conversations" true — is built from exactly this
machinery. `handler-case` is a `block` plus a dynamically found handler. A
restart is `catch` with a protocol. And a handler that runs *during* an unwind,
with the option to resume rather than continue, is `unwind-protect`'s pause
with a decision attached.

None of that is built. All of it now has something to be built from, which is
why group B came before the rest of tier 0 rather than after it.
