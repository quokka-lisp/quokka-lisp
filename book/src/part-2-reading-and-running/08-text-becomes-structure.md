# Text becomes structure

> **Where this fits.** Part I built a place to put things. This is the first
> chapter that puts something in it. The **reader** turns what you type into
> objects in the heap — and in Lisp that job is unusually small, for a reason
> worth understanding before the evaluator arrives to consume what it produces.

In most languages this part is called a *parser*, and it is where a great deal
of a compiler's complexity lives. It has to know that `*` binds tighter than
`+`, that `if` is a statement and `x + 1` is an expression and the two are
different kinds of thing, that a declaration looks like a call until you reach
the end of it. The output is a *parse tree*: an internal structure, made of
types that exist only inside the compiler, that your program can never touch.

The Lisp reader does none of that.

## One structure, not two

```lisp
(* 6 7)
```

This is not syntax for multiplication. It is a **list** of three things: the
symbol `*`, the number 6, the number 7. Precisely the same kind of list you
would get by writing out a list of data.

So the reader's job is: read a `(`, read things until you reach a `)`, make a
list of them. That is nearly all of it. There is no precedence to resolve
because there are no operators. There is no statement grammar because there are
no statements. `if` is not special to the reader — it is a symbol, like `*`, and
discovering that it means something particular is the evaluator's problem,
several chapters away.

Three consequences follow, and they compound:

- **The reader is small.** Four hundred lines, most of it characters and strings
  and error messages.
- **There is one representation.** The reader produces cons cells; the evaluator
  consumes cons cells. No parse-tree types, no conversion layer, no second
  version of every structure.
- **Programs can build programs.** Because code is data, a Lisp program can
  construct a list and have it evaluated as code with no more ceremony than
  building a list. That is what macros are, and it is the reason Lisp systems
  can extend their own language.

Here is `(* 6 7)` after reading, in the heap from chapter 5:

```text
 index    0     1     2     3     4     5
        ┌─────┬─────┬─────┬─────┬─────┬─────┐
        │ →*  │ →2  │  6  │ →4  │  7  │ nil │
        └─────┴─────┴─────┴─────┴─────┴─────┘
        └── cons ────┘└── cons ────┘└── cons ────┘
```

Three cons cells. The first holds a reference to the symbol `*`; the rest hold
the numbers. Nothing here knows it is code.

## Symbols have to be shared

A symbol is a named thing — `*`, `fact`, `my-variable`. The obvious
implementation is "an object holding its name", and the obvious implementation
is wrong in one specific way.

If every occurrence of `foo` made a *new* symbol object, then two occurrences of
`foo` would be different objects. Looking up a variable would mean comparing
strings, every time, everywhere. Worse, `eq` — Lisp's "is this the same object?"
— would be false for two `foo`s, which breaks the thing programs most rely on
symbols for.

So symbols are **interned**: the reader keeps a table of every symbol it has
made, and reading a name that is already there returns the existing object. The
table has a traditional name, the **obarray** (from "object array", a 1960s MIT
detail that stuck).

```rust
pub fn intern(&mut self, name: &str) -> Value<'h> {
    let mut cursor = Value::from_bits(self.storage.obarray);
    while !cursor.is_nil() {
        let symbol = self.car(cursor);
        if self.string_is(self.slot(symbol, symbol::NAME), name) {
            return symbol;
        }
        cursor = self.cdr(cursor);
    }
    // not found: make it, and put it at the front of the list
}
```

The obarray here is a **list**, so interning walks it and is O(n) in the number
of symbols. That is slow and it is deliberate. A hash table is the right answer
eventually, and the right time to write it is when something measures that it
matters — not now, when the alternative is five lines anyone can read.

What makes that safe to defer is that nothing outside this function knows. The
obarray could become a hash table tomorrow and no other file would change. It is
the opposite of the rooting decision in chapter 4: contained, so it can wait.

### A symbol's four slots

Common Lisp gives a symbol a name, a value, a function and a property list. All
four are here, and one detail matters:

```rust
self.set_slot(symbol, symbol::VALUE, Value::UNBOUND);
self.set_slot(symbol, symbol::FUNCTION, Value::UNBOUND);
```

Not `nil` — **unbound**, the marker from chapter 1 that no value can be mistaken
for. The difference is the difference between "this variable holds nothing" and
"this variable does not exist", and Lisp cares: reading an undefined variable
must signal an error, not quietly return the empty list.

That a symbol has a value *and* a function, separately, is what makes Common
Lisp a **Lisp-2**. `(list list)` is legal: the first `list` is the function, the
second is whatever the variable holds. Scheme, a Lisp-1, has one slot and
forbids it.

## The `nil` problem, arriving on schedule

Chapter 1 left a question open: `nil` is implemented as an immediate, a fixed
bit pattern, but Common Lisp says `nil` *is a symbol* — with a name, a package,
and a settable property list.

Here is where that bites, in the reader, exactly as predicted:

```rust
pub fn intern(&mut self, name: &str) -> Value<'h> {
    if name.eq_ignore_ascii_case("nil") {
        return Value::NIL;
    }
    if name.eq_ignore_ascii_case("t") {
        return Value::T;
    }
    // ...
```

Two special cases, in the one function that should not need them. And a
consequence that is wrong by the standard: `is_symbol(nil)` answers **no**, so
`(symbolp nil)` would be false where SBCL says true.

It is written down rather than hidden, the code says so where someone will read
it, and the decision stays open. The argument for changing it has got slightly
stronger each time the subject has come up, and it will be settled when there is
an evaluator to measure the cost against. Three special cases would be a
different matter from two.

## Strings, wastefully

A string is a heap object whose slots are characters. One Unicode scalar per
64-bit word.

That wastes most of every word — a byte's worth of information in eight bytes.
It is deliberate, and the reasoning is the same as the obarray's: indexing a
character is O(1), the code is five lines, and nothing outside depends on the
layout. Packing four characters per word, or storing UTF-8 and dealing with
variable-width indexing, is a real optimisation available at any point and
nobody has measured a need for it.

*Later: the need was measured in phase 2, and constant strings now pack
eight characters to a word. Chapter 37 tells that story, including why UTF-8
lost.*

Being deliberately wasteful is a defensible position when the waste is contained
and the alternative costs clarity. The trick is writing down *that* it is
deliberate, so the next reader knows it is a decision rather than an oversight.

## A new root, and the test that proves it

The obarray must survive garbage collection, and the collector can only find
what it is told about. So it becomes the system's **second root**:

```rust
self.storage.root = forward(from, &mut to, self.storage.root);
self.storage.obarray = forward(from, &mut to, self.storage.obarray);
```

One line. Forget it and every symbol in the system is silently reclaimed; the
next read builds fresh ones, `eq` quietly starts failing, and the bug surfaces
somewhere unrecognisable. Chapter 3 warned that adding roots is where this goes
wrong, and this is the first new root since.

The test is worth looking at, because of its second assertion:

```rust
let before = heap.mutate(|mc| mc.intern("PERSISTENT").bits());
heap.collect();
heap.collect();
heap.mutate(|mc| {
    let after = mc.intern("PERSISTENT");
    assert_ne!(after.bits(), before, "nothing moved, so nothing was tested");
    assert_eq!(mc.intern("PERSISTENT"), after, "interning made a second one");
});
```

`assert_ne!` first. If the symbol happened to land at the same index it already
had, the test would pass whether or not the obarray was forwarded — proving
nothing while looking green. Checking that it *moved* is what makes checking
that it *survived* mean something.

This is the same shape as the padding trick in chapter 6, and it is a general
habit worth forming: when a test could pass by coincidence, assert that the
coincidence did not happen.

## Depth, and a crash that is not an error

The reader is recursive. Reading a list calls itself for each element, which is
the clear way to write it and has one failure mode:

```lisp
((((((((((((((((((((  ... ))))))))))))))))))))
```

One Rust stack frame per level. Enough levels and the stack overflows, which is
not a failed read — it is a dead process, with no condition to handle and
nothing to report.

Three facts make that worse than it sounds:

- The input is **attacker-controlled** the moment a file comes from anywhere but
  us, and `(` is cheap to repeat.
- The **wasm32 stack is about a megabyte**, fixed at link time, against eight
  megabytes for a native thread. The browser dies first, on input that was fine
  on the development machine.
- A crash here takes the whole image with it, and the image is the system.

So depth is counted:

```rust
if depth > MAX_DEPTH {
    return Err(ReadError::TooDeep { at: self.at });
}
```

Two hundred, which is far beyond anything hand-written and far below trouble.

The better answer is an explicit stack — the same move the evaluator is forced
into for different reasons in chapter 2. It is not built here, and the reason is
that **this reader is temporary**. Tier 1 of the language moves the reader into
Lisp itself, at which point this file is deleted. Building the careful version
of code with a known deletion date is a poor trade; building the version that
cannot kill the process is not optional.

Knowing which of those two you are doing is most of engineering judgement.

## Reading allocates, and contains no rooting code

Every list the reader builds is cons cells, and every cons cell is an
allocation. Chapter 3 named the reader as the canonical place where a value held
across an allocation gets reclaimed — the bug that chapter 4 spent itself
preventing.

There is no rooting code in this module. Not one line.

Everything happens inside a single `mutate`, so no collection can intervene, so
holding a partially built list across an allocation is simply safe. Chapter 4's
cost — collection only between mutations — and chapter 4's benefit arrive
together, in the same file, and the benefit is the absence of code.

That absence is the clearest measure of whether the decision was right. A
shadow-stack design would have put a `Scope` and a `Root` and a `get` on every
line of this chapter's code.

## The printer, and the P in REPL

A reader with no printer is invisible. You cannot see what was read, the REPL
cannot show its work, and the differential tests of chapter 10 have nothing to
compare. So there is a printer, deliberately minimal, and what it *cannot* do is
worth naming.

**Cycles.** A list whose tail points back at itself prints forever. Common Lisp's
answer is `*print-circle*`, which detects sharing and prints `#1=(a . #1#)`.
It is deferred, and until then a blunt length limit stops the printer rather
than the process. That is a crude stand-in and the code says so.

**`princ` versus `prin1`.** Common Lisp distinguishes printing *for a program to
read back* — strings quoted, characters as `#\a` — from printing *for a person*,
which shows contents only. Only the first exists here, because that is what a
REPL and a test oracle want.

The round trip is also how the reader gets tested. Read text, print it, compare
to what went in. If the text survives, the structure was almost certainly right,
and one assertion covers the whole path.

## Four bugs

Three were found by tests, which is the system working.

**`'x` was broken.** Quote reads as a list: `'foo` becomes `(QUOTE FOO)`. The
reader upcases every symbol it reads, as Common Lisp does, so `foo` and `FOO`
are one symbol. But the `quote` the reader inserted was interned *lowercase* —
so it was a different symbol from the `QUOTE` any program would write, and
nothing would ever have matched it. Invisible without a test that read `'foo`
and looked at the result.

**An unterminated `#|` comment was swallowed.** Skipping whitespace hit the end
of input, returned quietly, and left the position at the end — so the reader
reported "nothing to read" rather than an error. The fix is a signature: skipping
whitespace is *fallible*, and saying so in the type makes the caller handle it.
A function that can fail and returns nothing is a trap.

**`((()))` is `((NIL))`.** My test said `(((NIL)))`. The reader was right. Worth
including because a test disagreeing with the code is not evidence about which
is wrong, and the habit of checking by hand rather than adjusting whichever is
easier to change is what keeps a test suite honest.

### And one the compiler found

The fourth is the one worth remembering.

Threading the `'h` brand from chapter 4 through every reader signature is
tedious. The first draft avoided it:

```rust
fn rebrand(v: Value<'_>) -> Value<'static> {
    Value::from_bits(v.bits())
}
```

A `'static` value outlives every mutation. That function is a door through the
exact wall chapter 4 is about — written by the person who wrote chapter 4, four
chapters later, while thinking about signature noise rather than safety.

The compiler refused it, and the correct version turned out not to be a
workaround at all: making the reader generic over the brand is *right*, because
doing anything with a form needs the heap anyway, so every caller is already
inside a mutation.

This is the honest argument for compile-time guarantees over careful
conventions, and it is an argument from evidence rather than principle. The rule
was known. The chapter explaining it was written. The shortcut still looked
reasonable in the moment, because at that moment it presented as a *signature*
problem rather than a *safety* problem. A convention would have let it through.

## Where this leaves us

```text
quokka> (fact 20)
(FACT 20)
quokka> '(1 "two" #\3)
(QUOTE (1 "two" #\3))
quokka> (1 2
; input ended inside a list
```

Text goes in, structure comes out, structure comes back as text. The REPL has
its R and its P, and collects between lines when the heap asks — which is the
first use of `wants_collection` from chapter 5 by something that is not a test.

What is missing is the E. `(fact 20)` reads as a list of two things and that is
all that happens to it; nothing yet knows that the first is a function and the
second an argument.

That is [the next chapter](../part-2-reading-and-running/09-what-a-form-means.md), and it is the one chapter 2
has been waiting for.
