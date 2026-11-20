# The trouble with nil

> **Where this fits.** A short chapter about one value, because it turned up in
> four others before anyone got it right. `nil` is the most overloaded object in
> Lisp, and the places it strains are a good lesson in what a long-lived design
> decision feels like from the inside — including what it feels like to be
> wrong about one for four chapters.
>
> **Status: decided and implemented.** The argument is left as it stood, because
> the reasoning that lost is most of what makes the answer meaningful.

## One value, three jobs

```lisp
()            ; the empty list
nil           ; false
'nil          ; a symbol, whose name is "NIL"
```

All three are the same object. That is not an implementation shortcut — it is
deliberate, and it has been that way since 1958.

**It is the empty list.** Every list ends with it. `(cdr '(1))` is `nil`.

**It is false.** Lisp has no separate boolean type for falsity: `nil` is false,
and *everything else* is true, including `0` and the empty string.

**It is a symbol.** `(symbolp nil)` is true. It has a name, a home package, and
a property list you are allowed to modify.

## Why that is a good idea

The convenience is enormous, and you see it in almost every line of list code:

```lisp
(if (cdr list)
    (keep-going (cdr list))
    (finished))
```

"If there is more list" and "if the rest is not empty" are the same test,
written once. In a language where the empty list and false are different things,
that reads:

```lisp
(if (not (empty? (cdr list))) ...)
```

Multiply by every loop ever written. A single value doing two jobs removes an
entire category of conversion from the language.

The symbol part matters too: because `nil` is a symbol, it can be *quoted*,
stored, compared and printed like any other, with no special case at the
boundaries of the language.

## Why it is also a problem

An implementation has to make `nil` fast. Testing for the end of a list is
possibly the single most common operation in a Lisp program, so `null` wants to
be one machine instruction.

Quokka's tagged words (chapter 1) make that easy, and the first answer was to
make `nil` an **immediate** — a fixed bit pattern, `0b101`, referring to
nothing:

```rust
pub const NIL: Value<'h> = singleton(SINGLETON_NIL);

pub const fn is_nil(self) -> bool {
    self.0 == Value::NIL.0        // one comparison
}
```

Perfect. Except that an immediate has nowhere to keep a name, a package or a
property list — and Common Lisp says `nil` has all three.

## The special cases, counted

They accumulated quietly. Here is the tally that eventually settled it.

**Two in the reader** (chapter 8), in the one function that should need none:

```rust
pub fn intern(&mut self, name: &str) -> Value<'h> {
    if name.eq_ignore_ascii_case("nil") { return Value::NIL; }
    if name.eq_ignore_ascii_case("t")   { return Value::T; }
    // ... the actual symbol table
```

**One in the printer**, which prints `nil` from its immediate branch and so
never consults `symbol-name`.

**And a conformance break**, which is the one that matters: `is_symbol(nil)`
answers *no*. `(symbolp nil)` would be false where every real Common Lisp says
true — a violation of the project's own rule 6, sitting in the code with a
comment on it rather than a fix.

The evaluator (chapters 9 and 10) added **none**. That is a real data point and
it cuts the other way: `defun` asks whether its name is a symbol, and nobody
writes `(defun nil ...)`, so the question never came up.

## The other way to do it

SBCL — the Common Lisp Quokka tests itself against — makes `nil` a **real
symbol**, allocated first, at a position known to the whole system.

The thing that is easy to miss, and that made this look like a close call for
much longer than it should have:

> **The fast test is available either way.**

If `nil` lives at a fixed, known heap index, then `is_nil` is *still* a single
comparison against a constant — just a constant that happens to be a heap
reference rather than an immediate. Nothing is lost.

What the immediate actually buys is a Rust *compile-time* constant, which is a
convenience for the VM's own code rather than a property of the language. That
is a much weaker argument than "it is faster", which is what it looked like at
first glance.

## The apparent dilemma

By this point the case for changing it was strong, and one objection kept it
open: a heap object **moves**.

The collector copies live objects to fresh space, so an object's index changes.
If `nil` were a heap object, `is_nil` would need the heap to find out where it
is — and `Value::NIL`, which appears at eighty-odd sites, could not be a Rust
constant at all. Two properties that the whole system leans on, apparently
traded away for conformance.

That is what kept this open. It is also wrong, and the mistake is instructive:
**both horns assume that everything in the heap moves.**

## The way out: a region that does not move

SBCL puts `NIL` at a fixed address in **static space**, which the collector
never touches. The same idea works over offsets.

```text
 index   0 ....... static_end ................................ used
       ┌──────────────┬────────────────────────────────────────┐
       │ NIL  T  ...  │ everything else, copied and compacted  │
       └──────────────┴────────────────────────────────────────┘
         never moves    collected normally
```

The collector's change is four lines: a reference below `static_end` is returned
unchanged rather than forwarded.

```rust
if at < static_end {
    return word;
}
```

And everything follows. `nil` is a real symbol with a name, a value cell and a
property list. `Value::NIL` is a compile-time constant naming index zero.
`is_nil` is one comparison. `intern` finds `NIL` in the obarray like any symbol;
the printer prints it through `symbol-name`; `block` needs no special case;
`is_symbol` answers yes because the header says `Symbol`.

**None of the hundred-odd call sites changed.**

The static objects are still *scanned* — a property list set on `nil` can point
into collected space — so they are roots that happen not to move. And the
mechanism is not about `nil`: `t` came with it, and the keyword package and the
standard readtable will use it later. A decision that looked like special
pleading for one value turned out to be a missing *category*.

## Four things broke, and none was in the new code

Worth recording, because this is characteristic of representation changes.

**A loop terminated on the wrong question.** Environment lookup walked
`while scope.is_object()`, which was right until `nil` became one. It walked
into `nil`'s name slot and tried to read a string as a scope. It asks what
*kind* of object now, which is also total — anything unexpected ends the walk
rather than being misread.

**Padding was applied after the static scan**, truncating everything the scan
had just copied and leaving a root pointing one word past the end of the heap.

**The limits counted the static area**, so a heap with a soft limit of eight
words was already over it before anything was allocated.

**And the nastiest.** The torture mode collects after every mutation —
including the mutations that *build* the static area, before its end is known.
So the padding moved `nil` away from the index `Value::NIL` names, **after the
assertion that checks it had already passed**.

An assertion that holds and is then invalidated by something running later is a
shape worth remembering for any bootstrap sequence. The check was in the right
place and still proved nothing.

None of the four was in the new code. That is the lesson: the risk in a
representation change is not what you write, it is what already quietly depended
on what you replaced.

## What the deferral bought

This was open for four chapters. Was waiting worth it?

The argument available at chapter 1 was *the fast null test is available either
way* — true, and thin. It would have been a judgement call made on a hunch.

The argument available at the end was **four special cases in places nobody
predicted**, including `block` names, which was not among the three costs this
chapter forecast. That is not the same kind of claim. It is evidence, and it
also said something the enumeration had missed: the places a representation
costs you are more numerous than listing them suggests.

> Deferring a decision only pays if the waiting produces information. Here it
> did — and what made the wait affordable was bought deliberately in chapter 1,
> by routing every test through one function so that nothing else knew.

Not every decision can be made reversible. For the ones that can, it converts a
guess into a measurement, which is worth a few minutes of indirection.

## And one thing that is gone for good

```
quokka> (symbolp nil)
T
```

It had been `NIL` — a violation of the project's own rule 6, sitting in the code
with a comment on it, and **invisible to the differential tests because the test
that would have failed was deliberately not written**.

That is the worst of the states this could have been in: a known-wrong
behaviour, hidden from the machinery built to find wrong behaviour. Being able
to ask the oracle a question you have been avoiding is most of what fixing it
was for.
