# One word for everything

> **Where this fits.** The first question any runtime has to answer: how is a
> piece of data represented, and how do you tell a number from a list? Every
> later chapter is constrained by what is decided here, so it comes first —
> and it is the one part of the system where complete confidence is cheap,
> because nothing moves yet.

Every Lisp object in Quokka — a number, a character, a symbol, a list, a
function — is represented by exactly one 64-bit word.

That is a strong claim, so start with the obvious objection. A list of a
thousand elements is not going to fit in eight bytes. Neither is a string. The
claim is narrower than it sounds, and the distinction it rests on is the whole
subject of this chapter.

## Two kinds of word

A word is one of two things:

- **The data itself.** The number `10` is stored as the number 10. Nothing is
  allocated, nothing is pointed at.
- **A reference.** The list `(10 20)` is stored as "the object at position 2 of
  the heap".

This is the same distinction as an `int` versus a pointer in C, or a value
versus an index in Rust. Small things that fit in a word are kept in the word.
Big things live elsewhere, and the word says where.

Which raises the question the rest of the chapter answers: given a word, how do
you know which kind it is?

## The heap

Before the answer, the thing being referred to.

The heap is an array of 64-bit words that the VM owns. In Rust it is literally a
`Vec<u64>`. An object is a run of adjacent words in that array. A **cons cell** — the
two-slot box Lisp builds lists out of, introduced in
[What has to be built](../what-has-to-be-built.md) — is two words: the `car`
(the item) and the `cdr` (the rest of the list).

Here is a heap holding three cons cells. `→4` means "a reference to index 4":

```text
 index    0     1     2     3     4     5
        ┌─────┬─────┬─────┬─────┬─────┬─────┐
        │ 99  │ nil │ 10  │ →4  │ 20  │ nil │
        └─────┴─────┴─────┴─────┴─────┴─────┘
        └── cons C ──┘└── cons A ──┘└── cons B ──┘
```

Cons A is at index 2. Its car is `10` and its cdr is `→4`, a reference to cons
B, whose car is `20` and whose cdr is `nil`. A and B together are the list
`(10 20)`. Cons C is separate and holds `(99)`.

Notice that `10`, `nil` and `→4` are all sitting in the same array, the same
width, indistinguishable by position. Something in the word itself has to say
what it is.

## Tags

The answer is the low bits. Quokka uses three of them:

| Low bits | Meaning | What the rest of the word holds |
| --- | --- | --- |
| `xx0` | Fixnum | a 63-bit signed integer |
| `001` | Cons | where the cons cell is |
| `011` | Heap object | where the object is; its header says what type |
| `101` | Immediate | characters, `nil`, `t`, the unbound marker |
| `111` | Forwarding | where an object moved to, during a collection |

"Fixnum" is the traditional Lisp word for an integer small enough to live in a
machine word, as opposed to a *bignum*, an arbitrarily large integer that needs
heap storage.

The scheme is Symbolics' — their Ivory processor tagged memory in hardware, and
checked the tags in the same cycle as the operation. We are doing in software
what that machine did in silicon, which is a recurring theme: the Rust layer of
Quokka exists to play the part that a Lisp machine's microcode once played.

Three questions fall out of that table.

## Why do heap references have three bits spare?

Because every heap object is 8-byte aligned.

An object's offset from the start of the heap is always a multiple of 8, and a
multiple of 8 in binary always ends in three zeros. Those three bits are
guaranteed empty, so they are free for the tag. You are not giving anything up.

Since a word is 8 bytes, dividing that byte offset by 8 gives the object's index
into the word array — which is what the code actually wants:

```rust
pub fn word_index(self) -> Option<usize> {
    if self.is_heap_ref() {
        Some((self.0 >> TAG_BITS) as usize)
    } else {
        None
    }
}
```

That leaves 61 bits of index, which is more heap than any machine will have.

One thing to notice, because it matters a great deal later: these are
**offsets, not addresses**. The word says "index 4 of the heap array", not
"memory location 0x7fff8a2c0020". An offset means the same thing on a 64-bit Mac
and inside a WebAssembly sandbox, which is what lets a saved image be loaded by
either. [Saving a world](07-saving-a-world.md) is about what that buys.

## Why does `nil` need a tag of its own?

`nil` is the empty list, and also false, and it turns up constantly — at the end
of every list, in every conditional. It needs the cheapest possible test.

The first answer was to give it a fixed bit pattern in this tag — an
**immediate**, a value referring to nothing — so that testing for it was one
comparison and `Value::NIL` was a Rust compile-time constant.

That answer was wrong, and it took four chapters to find out. In Common Lisp,
`nil` **is a symbol**. `(symbolp nil)` is true. It has a name, a home package, a
value cell holding itself, and a property list you are allowed to modify. An
immediate has nowhere to keep any of that.

So `nil` is a symbol here too, living at a fixed index in a region of the heap
the collector never moves. The test is still one comparison, because the index
is a constant. [The trouble with nil](../part-2-reading-and-running/11-the-trouble-with-nil.md)
is the whole story, including why the obvious objection — that a heap object's
index cannot be a constant — turned out not to hold.

What made the correction cheap is worth noticing here, where the decision was
made. Every test for `nil` went through one function and every mention through
one constant, deliberately, so that nothing else in the system knew how it was
represented. When the answer changed, **none of the hundred-odd places that use
`nil` changed with it**.

> The useful thing to do with a decision you cannot yet make well is to make it
> *reversible*, and write down why.

The same tag still holds characters and the marker for "this variable has no
value", which is not a Lisp object and so has nothing to be a symbol for.

## Why does the fixnum tag look wrong?

`xx0` is not a three-bit pattern like the others. It is one bit: if the bottom
bit is clear, the word is a number, whatever the two bits above it are doing.

That means fixnums get half the entire encoding space, and every other kind of
value shares the other half. It looks lopsided. It is, deliberately: integers
are the most common value in any running program and the only kind whose
operations sit on the critical path.

But the reason for a *single zero bit*, rather than any other cheap tag, is
better than that. A fixnum `n` is stored as `n << 1`. Watch what happens when
you add two of them without untagging anything:

```text
tagged(a) + tagged(b)  =  2a + 2b  =  2(a + b)  =  tagged(a + b)
```

The sum of two tagged fixnums is *already* a correctly tagged fixnum. No
shifting, no masking, no unwrapping and rewrapping. The same works for
subtraction. Comparison too, since doubling preserves order — comparing the raw
words compares the numbers.

Multiplication is the one case that needs work: `2a × 2b` is `4ab`, twice what
it should be, so exactly one operand gets shifted down first.

```rust
pub fn checked_add(self, other: Value) -> Option<Value> {
    if !self.is_fixnum() || !other.is_fixnum() {
        return None;
    }
    let sum = (self.0 as i64).checked_add(other.0 as i64)?;
    Some(Value(sum as u64))
}
```

## The part that is genuinely elegant

Look at that `checked_add` again, and ask what happens on overflow.

A fixnum's payload runs from −2⁶² to 2⁶²−1. Doubling that range maps it onto
−2⁶³ to 2⁶³−2 — which is, to within one value, exactly the range of an `i64`.

So `i64::checked_add` on the *tagged* words returns `None` precisely when the
true mathematical sum leaves fixnum range. Not approximately. Exactly. The
boundary where the hardware's overflow flag fires is the boundary where you need
a bignum.

You get your range check for free, from the arithmetic unit, as a side effect of
a tag chosen for a different reason. And `None` becomes a meaningful signal
rather than an error: it means "this needs a bignum", which is a job for a layer
that does not exist yet and does not need to.

This is worth a test that says so out loud:

```rust
#[test]
fn overflow_detection_is_exact() {
    let max = Value::fixnum(MAX_FIXNUM).unwrap();
    let one = Value::fixnum(1).unwrap();

    assert_eq!(
        Value::fixnum(MAX_FIXNUM - 1).unwrap().checked_add(one).and_then(Value::as_fixnum),
        Some(MAX_FIXNUM)
    );
    assert_eq!(max.checked_add(one), None);
}
```

Not one value early, not one late.

## What a test suite is actually for

Two of the tests in this module are not checking for bugs. They are pinning down
facts that other parts of the system will quietly assume.

`the_encoding_is_stable` asserts the literal bit patterns — that `nil` is
`0b101`, that the fixnum 1 is the word `2`. Nothing is likely to break these by
accident today. But an image file saved by one build will be loaded by another,
possibly compiled for a different target, and the bytes in that file *are* these
bit patterns. Changing the encoding has to be a deliberate act accompanied by a
format version bump, and a failing test is how it gets noticed.

`raw_word_order_matches_numeric_order` asserts that comparing two tagged words
gives the same answer as comparing the numbers they represent. That is true by
construction, and the compiler will eventually be allowed to rely on it. Writing
it down is what makes relying on it safe.

A test does not have to be defending against something plausible. Sometimes its
job is to make an assumption explicit, so that breaking it is loud.

## WebAssembly finds a bug on the first day

One of Quokka's rules is that every change must build and pass tests for native
*and* for `wasm32`, from the first commit. This chapter's module is where that
rule proved itself, about ninety seconds after the tests first went green.

The constant describing the largest possible heap index was written like this:

```rust
pub const MAX_WORD_INDEX: usize = (1 << 61) - 1;
```

That compiles and runs correctly on a 64-bit machine. On `wasm32` it is a
**compile error**, because `usize` is 32 bits there and you cannot shift a
32-bit value left by 61.

The mechanical fix is to change the type. The interesting fix is to notice what
the error is telling you: the 61-bit limit is a property of *the encoding*, not
of whatever machine happens to be running. Written as a `usize`, the constant
quietly means different things on different targets, which is precisely what an
encoding must never do.

```rust
pub const MAX_WORD_INDEX: u64 = u64::MAX >> TAG_BITS;
```

Without that rule, the error would have waited until someone first tried the
browser build — by which time it would have looked like a problem with the
browser build.

## Where this leaves us

There is now a `Value`: one word, five kinds, arithmetic that mostly does not
need unwrapping, and an encoding stable enough to write to a file. Twenty-three
tests, passing on native and inside WebAssembly.

Nothing has been allocated. There is no heap yet — `word_index` returns an index
into an array that does not exist. That is deliberate. This module is pure
representation, which makes it the one part of the VM where complete confidence
is cheap, and it is worth banking that before anything starts moving.

Because the next thing that happens is that things start moving.
