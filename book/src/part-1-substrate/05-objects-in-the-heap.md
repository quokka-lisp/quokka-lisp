# Objects in the heap

Chapter 1 settled what a single value looks like. Chapter 3 said the heap is an
array of words and objects are runs of adjacent words in it. This chapter is
about the runs.

It is mostly a chapter about one question, which looks like a detail and is not.

## Two kinds of object

**Cons cells are two words.** A car and a cdr, nothing else. No type field, no
length — the tag on the reference already says "this is a cons", and every cons
is the same size. That is why cons cells were given a whole tag of their own in
chapter 1: they are the overwhelming majority of objects in a running Lisp, and
the tag buys them a representation with no overhead whatsoever.

**Everything else gets a header.** A symbol, a string, a vector, a closure. The
first word says what the object is and how many slots follow it:

```text
 index    0        1       2       3
        ┌────────┬───────┬───────┬───────┐
        │ header │ slot0 │ slot1 │ slot2 │
        └────────┴───────┴───────┴───────┘
          Vector / 3 slots
```

The header has to carry the length because, unlike conses, these objects vary in
size, and something has to be able to tell where one ends.

That "something" is the garbage collector, and here is where the interesting
problem appears.

## The question

The collector walks to-space **linearly** — start at the beginning, look at an
object, step over it, look at the next. (Chapter 6 explains why it walks rather
than recurses; for now take it as given.)

To step over an object you need its size. For a headed object the header says.
For a cons it is two. So at every position the collector must answer: *am I
looking at a header, or at a cons cell's car?*

And it cannot tell by looking at where it is, because both are "the first word
of an object". They occupy exactly the same kind of position.

```text
          ┌────────┬───────┬───────┐
   ... ?  │  ????  │       │       │   is this a header, or a car?
          └────────┴───────┴───────┘
```

Get this wrong once and every subsequent step is wrong. The collector would
read the middle of one object as the start of another, step by a nonsense
amount, and corrupt the heap in a way that surfaces somewhere else entirely.

## Three answers

**Give cons cells a header too.** Then every object looks the same and the
problem evaporates. It also makes the most common object in the system 50%
bigger — three words instead of two, on every cell of every list. For a
language built out of lists that is not a detail, it is a tax on everything.

**Put conses in their own region.** Allocate cells from one area and headed
objects from another. Each region is then homogeneous and the walk knows what it
is walking. This works, and Lisp machines did organise memory into areas for
related reasons. It costs two allocation pointers, two scan pointers, and a
collector that handles two spaces instead of one — real complexity, paid
forever, against a 15,000-line budget.

**Make a header impossible to mistake for a value.** If no legitimate value can
ever have the bit pattern of a header, the test is one comparison and costs
nothing at all.

Is there room for that? Chapter 1 left some. Immediates — tag `101` — carry a
five-bit subtag, of which exactly two were used: one for `nil`/`t`/the unbound
marker, one for characters. Thirty are spare. Reserve one, never hand it out,
and a header becomes a bit pattern that no value can hold:

```text
header:   [ length : 48 ][ type : 8 ][ subtag 31 : 5 ][ 101 ]
```

A cons cell's car is a value. Values cannot carry subtag 31. Therefore a car can
never impersonate a header, the walk is a single test, and conses stay two
words.

```rust
pub(crate) const fn is_header(self) -> bool {
    self.is_immediate() && (self.0 & SUBTAG_MASK) == SUBTAG_HEADER
}
```

## Believing it versus checking it

That argument is sound, and an argument is not a test. The claim "no value can
look like a header" is exactly the sort of thing that is true when written and
quietly false three features later, when someone adds a subtag and picks a
number.

So there are two tests, aimed at different failure modes.

One runs every interesting value past `is_header` — every singleton, every tag,
the boundary fixnums, ten thousand pseudorandom fixnums, characters including
`\0` and the highest code point — and asserts none of them says yes.

The other walks a deliberately awkward heap:

```rust
let awkward = mc.cons(
    Value::character('q'),
    Value::fixnum(i32::MAX as i64).unwrap(),
);
```

A cons whose car is a character — an immediate, same tag as a header, differing
only in the subtag — and whose cdr is a large fixnum, a word with plenty of bits
set. Those are the two words most likely to be misread. The test walks the heap
with the same logic the collector uses and asserts it recovers the exact set of
objects *and* lands precisely on the end rather than overrunning.

The second assertion matters as much as the first. A walk that finds the right
objects but finishes one word late has still lost, and would have corrupted
everything after it.

## What the type is for

The header's type field is not just for printing. It answers a question the
collector cannot work out for itself: **are this object's slots values?**

A vector's slots are values and must be traced. A string's slots are characters.
A code object's are bytecode. A bignum's are magnitude limbs. Trace those and
the collector follows numbers as if they were references — reading the letter
`q` as "the object at index 29", copying whatever is there, and writing a
corrupted reference back into your string.

So the distinction lives in the type, and it is load-bearing:

```rust
pub const fn slots_are_values(self) -> bool {
    (self as u8) < 64      // traced below 64, raw at or above
}
```

The discriminants are chosen so the question is a comparison. Which creates a
hazard: adding a new type on the wrong side of that line is a silent
heap-corrupting bug, invisible at the call site and invisible in review, because
the line is a number rather than a word.

The defence is a test that names every type explicitly and asserts which side it
is on. It would have been easy to write `assert!(ObjectType::String as u8 >= 64)`
and feel covered. Naming all eight means adding a ninth without thinking shows
up as a test that does not mention it.

The accessors follow the same principle: `slot` and `raw_slot` panic if used on
the wrong kind of object. Loudly wrong beats quietly wrong when the alternative
is a collector reading text.

## Allocation, and a small deliberate cost

```rust
pub fn alloc(&mut self, object_type: ObjectType, slots: usize) -> Value<'h> {
    let at = self.storage.alloc(HEADER_WORDS + slots);
    self.storage.words[at] = Header { object_type, slots }.encode();
    for slot in 0..slots {
        self.storage.words[at + HEADER_WORDS + slot] = Value::NIL.bits();
    }
    Value::object(at).expect("heap index exceeds the 61-bit encoding")
}
```

Allocation is a bump — take the next free words, no free list, no search. That
is a gift from the copying collector: reclaiming space by abandoning a whole
region means the space that remains is always contiguous.

The slots are initialised to `nil` rather than left as whatever was there.
That is a real cost, paid on every allocation, and it buys protection against a
specific sequence: a mutation allocates an object, ends before filling every
slot, and the next collection traces whatever words happened to be lying in
those slots. One `resize` is cheaper than that class of bug, which would look
like memory corruption and be nothing of the sort.

## Growth, and a bill arriving from chapter 4

Chapter 4 chose lifetime branding, and noted a cost: collection can only happen
between mutations. Here is where that cost becomes an API.

Allocation happens inside a mutation. Collection cannot. So when the heap is
full in the middle of a mutation, **it cannot collect — it has to grow.**

Which means allocation can never fail for want of space, and a limit cannot be
enforced where it is reached. What exists instead is a request:

```rust
pub fn wants_collection(&self) -> bool {
    self.storage.words.len() > self.storage.soft_limit
}
```

The driver checks it *between* mutations and collects if it says yes. A mutation
that allocates heavily simply overshoots, and asks for a collection afterwards.

This is affordable here for a reason established two chapters ago: rule 4
already forbids long mutations, because the evaluator must return control to the
browser promptly. One evaluator step per mutation means the overshoot is bounded
by whatever one step can allocate. Two constraints adopted for unrelated
reasons, agreeing.

In the browser the ceiling is not theoretical. WebAssembly linear memory stops
at 4 GiB, and browsers cap it lower.

## The same mistake twice

Chapter 1 described a constant that compiled on a 64-bit machine and was a
compile error on wasm32, because `usize` is 32 bits there. The fix was to notice
that the limit described an *encoding*, not a machine.

It happened again, in this chapter's code, to a different constant:

```rust
pub const MAX_OBJECT_SLOTS: usize = (1 << (64 - HEADER_LEN_SHIFT)) - 1;
```

Identical shape, identical error, caught by the same CI job. The second time is
more interesting than the first, because it says this is not an accident but a
*category*: a constant describing a width in a file format must not be typed as
`usize`, because `usize` means different things on different targets and a file
format must not.

Twice is a pattern. Three times and it is worth a lint rather than vigilance.

## Where this leaves us

The heap can hold cons cells and headed objects of eight kinds, a linear walk
can recover object boundaries exactly, and the type system knows which slots are
references.

Every one of those is a precondition for the next chapter, which is the one that
uses them.
