# The trouble with nil

> **Where this fits.** A short chapter about one value, because it has turned up
> in four previous ones and is still undecided. `nil` is the most overloaded
> object in Lisp, and the places it strains are a good lesson in what a
> long-lived design decision feels like from the inside.

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

Quokka's tagged words (chapter 1) make that easy. `nil` is an **immediate** — a
fixed bit pattern, `0b101`, that refers to nothing:

```rust
pub const NIL: Value<'h> = singleton(SINGLETON_NIL);

pub const fn is_nil(self) -> bool {
    self.0 == Value::NIL.0        // one comparison
}
```

Perfect. Except that an immediate has nowhere to keep a name, a package or a
property list — and Common Lisp says `nil` has all three.

## The special cases, counted

They accumulate quietly. Here is the tally so far.

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

## Why it is still undecided

Because it does not need deciding, and the cost of waiting is almost nothing.

Every test for `nil` goes through `Value::is_nil`, and every mention through
`Value::NIL`. Those two functions are the only code that knows how `nil` is
represented. Changing it means changing them and the bootstrap — not hunting
through the system.

That is the general move, and it is more useful than this particular question:

> When you cannot yet make a decision well, make it **reversible** and write
> down why.

The reversibility is not luck. It took a few minutes of routing every
`nil` test through one function instead of comparing bit patterns at each site,
and that small discipline is what keeps the option open.

## What will settle it

Not argument — a count. The special cases go one way as the system grows, and
the places that will add to them are known:

- **Packages.** `nil`'s home package is `COMMON-LISP`, and an immediate has no
  home.
- **`symbol-plist`.** Common Lisp lets you set `nil`'s property list. There is
  nowhere to put it.
- **`symbol-name`.** Printing and `string` conversion both want it.

Any one of those turns the tally from three special cases into five or six, at
which point the argument is over and the change is still cheap. The honest
current state is that the alternative is *probably* better and the evidence is
*not yet* decisive — which is a perfectly respectable place for a design
decision to sit, provided it is written down.

It is [issue #7](https://github.com/quokka-lisp/quokka-lisp/issues/7), and it
has been open since chapter 1.
