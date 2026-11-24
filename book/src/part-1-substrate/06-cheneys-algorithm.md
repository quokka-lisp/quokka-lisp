# Cheney's algorithm

> *Where this fits.* Chapter 3 defined what "still wanted" means and left the
> collector as a stub. This chapter is the collector, and it comes to about two
> hundred lines: a copying collector that reclaims garbage without looking at
> it, traces an arbitrarily tangled object graph in a loop that uses no extra
> memory at all, and ends by checking its own work rather than asking to be
> trusted. The piece that sounded like the hard part at the start of the book
> turns out to be the cheap one, because chapters 4 and 5 did the work that
> made it possible.

## The shape

Two semispaces. One is live and holds everything, and the other is empty. To
collect, copy the survivors from the live space into the empty one, then swap.

(Almost everything. The **static area** at the front of the heap is copied
verbatim and keeps its indices, so references into it need no updating. It holds
`nil` and `t`, for reasons that are
[chapter 11's](../part-2-reading-and-running/11-the-trouble-with-nil.md).)

```text
before                                   after
┌──────────────────────────────┐         ┌──────────────┐
│ A   garbage   B   garbage  C │   ───▶  │ A   B   C    │
└──────────────────────────────┘         └──────────────┘
  live                                     live (was spare)
```

Everything not copied is gone, and reclaiming it costs nothing: no walking, no
free lists, no coalescing. You abandon the region. That is the trade a copying
collector makes, in which you pay for the live data and the garbage is free.

It also explains the bump allocation from the last chapter. After a collection
the live data is packed at one end and everything beyond it is empty, so
allocating is "take the next words and move the mark", with no search and no
fragmentation. The price is the obvious one: you need room for two copies of the
live data.

## Copying once, and only once

The interesting problem is not copying. It is copying *once*.

Consider a cell referred to twice. Copy it when the first reference is followed,
copy it again when the second is followed, and you now have two objects where
there was one. The sharing is silently gone, `eq` stops working, and mutating
through one reference no longer shows through the other. Worse, consider a
cycle: follow the references naively and you never stop.

Both are solved by one trick. When an object is copied, overwrite its first word
in from-space with a **forwarding pointer**, a reference tagged `111` (chapter 1
reserved that tag for exactly this) saying where the object went.

```text
from-space, after A has been copied to to-space index 0

          ┌──────────────┬───────┐
   A ───▶ │ forward → 0  │  cdr  │
          └──────────────┴───────┘
```

Now the second reference to A arrives, finds a forwarding pointer, and uses it
instead of copying again. Sharing is preserved, and a cycle terminates the
moment it comes back round to something already copied. Scribbling over the
object is safe for a reason that is almost funny: from-space is about to be
thrown away, and there is no one left to care what is in it.

```rust
fn forward(from: &mut [u64], to: &mut Vec<u64>, word: u64) -> u64 {
    let value = Value::from_bits(word);
    let Some(at) = value.word_index() else {
        return word; // a fixnum, a character, nil: refers to nothing
    };

    let first = Value::from_bits(from[at]);
    if first.is_forwarding() {
        return retag(value, first.word_index().expect("forwarding pointer"));
    }

    let size = match Header::decode(from[at]) {
        Some(header) => HEADER_WORDS + header.slots,
        None => CONS_WORDS,
    };

    let moved_to = to.len();
    to.extend_from_slice(&from[at..at + size]);
    from[at] = Value::forwarding(moved_to).unwrap().bits();

    retag(value, moved_to)
}
```

Note `retag`. The forwarding pointer records *where* the object went, not *what*
it is, so a reference to a cons must come back a reference to a cons, and the
original reference's tag is reapplied at the new index. Note also the `match` in
the middle, working out the size. That is chapter 5's header trick, being used
for the first time.

## Tracing without a stack

Tracing means copying an object, then looking at the references inside it,
copying those, looking inside those, and so on. The obvious implementation is
recursive, and the obvious implementation is unusable, because a linked list of
a million cells means a million stack frames.

For a browser-first system that is not a theoretical concern. The WebAssembly
stack is about a megabyte and fixed at link time, against eight for a native
thread, so a recursive collector would die in the browser first, on data that
worked fine on the development machine. The usual fix is an explicit work stack,
which costs memory proportional to the depth of the structure, exactly when you
are collecting because memory is short.

Cheney's observation is that the work stack already exists. The objects already
copied into to-space are precisely the objects whose contents still need
examining. So keep two positions in to-space: `free`, where the next copy goes,
which advances as objects are copied, and `scan`, where the next examination
happens, which trails behind. Copying an object pushes work by placing it ahead
of `scan`, examining an object advances `scan`, and when the two meet,
everything reachable has been copied and every reference updated.

```rust
let mut scan = pad;
while scan < to.len() {
    let (size, slots) = match Header::decode(to[scan]) {
        Some(header) => {
            let size = HEADER_WORDS + header.slots;
            let slots = if header.object_type.slots_are_values() {
                scan + HEADER_WORDS..scan + size
            } else {
                scan..scan          // raw words: copied, never followed
            };
            (size, slots)
        }
        None => (CONS_WORDS, scan + 0..scan + CONS_WORDS),
    };

    for at in slots {
        let word = to[at];
        let updated = forward(from, &mut to, word);
        to[at] = updated;
    }
    scan += size;
}
```

A loop, not a recursion, with no auxiliary memory at all. Depth costs nothing
and an arbitrarily tangled graph costs nothing, and there is a test that
collects a hundred-thousand-element list for no other reason than to say so.
`to.len()` is `free`, and it grows inside the loop as references are forwarded,
which is why the condition is re-read each time round.

## A worked collection

A heap with three cons cells. The root points at A, and C is garbage.

```text
from-space
 index    0     1     2     3     4     5
        ┌─────┬─────┬─────┬─────┬─────┬─────┐
        │ 99  │ nil │ 10  │ →4  │ 20  │ nil │
        └─────┴─────┴─────┴─────┴─────┴─────┘
        └── cons C ──┘└── cons A ──┘└── cons B ──┘

root = →2
```

| Step | What happens | to-space | scan | root |
|---|---|---|---|---|
| 0 | start | *empty* | 0 | →2 |
| 1 | forward the root: A is not forwarded, copy 2 words, leave `forward→0` at from[2] | `10, →4` | 0 | **→0** |
| 2 | scan 0: no header, so a cons. Examine both words. `10` is a fixnum, unchanged. | `10, →4` | 0 | →0 |
| 3 | `→4` → forward: B not forwarded, copy 2 words, leave `forward→2` at from[4] | `10, →2, 20, nil` | 2 | →0 |
| 4 | scan 2: a cons. `20` is a fixnum, `nil` refers to nothing. Nothing to do. | `10, →2, 20, nil` | 4 | →0 |
| 5 | `scan == 4 == free`. They met. Flip. | | | |

```text
live space, after
 index    0     1     2     3
        ┌─────┬─────┬─────┬─────┐
        │ 10  │ →2  │ 20  │ nil │
        └─────┴─────┴─────┴─────┘
        └── cons A ──┘└── cons B ──┘

root = →0
```

C was never reached and is gone. A and B moved, and the reference between them
was rewritten to match. The queue was never longer than to-space itself.

## Three tests about meaning

It is easy to write a collector that terminates and returns a heap. Three tests
check that it returned the *right* heap.

1. Sharing. Two references to one cell must still be two references to one
   cell, so the test asserts the two references are `eq` afterwards, and also
   that the heap is exactly two cons cells, which is how you detect that the
   cell was copied twice. Without the second assertion the test passes on a
   collector that silently duplicates everything.
2. Cycles. A cons whose cdr is itself must come back with its cdr still
   itself, because "it terminated" is not the same as "it was right", and a
   collector could break the cycle and still finish.
3. Raw slots. A string whose words are bit patterns that *look* like a cons
   reference, an out-of-bounds object reference, and a forwarding pointer. If
   anything traces them the collection explodes or corrupts, so they must come
   back byte for byte.

## What torture mode became

The plan for this chapter, written three chapters ago, said the torture mode
would collect on every allocation. That is the classical technique: make the
rare interleaving the only interleaving, so a latent bug fails on the first test
run rather than the thousandth.

It is now impossible, because collection cannot happen inside a mutation and
allocation only happens inside one. It is also unnecessary, which is the better
half of the sentence, because the bug that technique hunts is a value lost
across an allocation, precisely the bug chapter 4's branding made
unrepresentable. The detector outlived the thing it detected.

That generalises. When you make a class of bug impossible, the machinery built
to catch it does not become valuable, it becomes dead weight, and the honest
response is to retarget it rather than keep running it for the comfort of a
green tick.

So what can still go wrong? Not callers losing values, but the collector itself.
A header whose length is wrong. A slot nobody traced. A root nobody forwarded,
and there will be more roots soon, the evaluator's registers and the obarray,
each one an opportunity to forget. Those bugs produce silently wrong answers,
because a stale offset indexes a perfectly valid vector and returns a perfectly
valid word.

The retargeted mode does three things:

1. It collects at every mutation boundary, the finest granularity that
   exists now, standing in for every allocation.
2. It pads the front of to-space by a varying amount, so that no object can
   land on the index it had. Without this, a small heap can copy an object back
   to where it was, a stale reference accidentally still works, and the
   nastiest test passes for the wrong reason.
3. It verifies, which is the next section.

## Verification

```rust
pub fn verify(&self) -> Result<(), Inconsistency>
```

Two passes. The first walks the heap linearly, recording where each object
begins, so a header that lies about its length makes this overrun and is caught
directly. The second checks that every reference, in every cons, in every
object's value slots, and the root, points at one of those recorded boundaries.

That catches the whole family. A slot the collector failed to update now points
into the middle of some other object, or past the end, or at a forwarding
pointer that should never have survived, and all of them become a named error
instead of a wrong answer three evaluations later. Because a verifier nobody has
seen fail is not known to work, four tests corrupt a heap deliberately, with an
interior reference, an out-of-bounds one, a lying header and a stray forwarding
pointer, and check that each is caught.

The poison idea from the original plan was dropped. It was going to fill the
abandoned space with `0xDEADBEEF` so that a stale read failed loudly, but with
branding no stale `Value` can survive a collection, so there is nothing to
catch. Verification replaces it and catches strictly more.

## What happened when the feature was switched on

Five tests failed. Then, after fixes, three. Then two. Not one of them was a
collector bug. Every single one was a test asserting on garbage that the torture
mode promptly collected, or poking a fixed word index that the padding had
moved.

```rust
heap.mutate(|mc| {
    mc.cons(Value::fixnum(1).unwrap(), Value::NIL);
    mc.alloc(ObjectType::Vector, 3);
    // ...
});
assert_eq!(heap.used_words(), /* ... */);
```

Nothing there is rooted. With no collector that heap persists; with one it is
empty the moment the mutation ends. The test was measuring something the system
never promised.

This is the feature working, on its first run, and it is a better outcome than
finding a collector bug would have been. Those five tests were written before a
collector existed and were correct at the time. There will be many more like
them, because the reader and the evaluator will be full of the same assumption,
and every one is cheaper to find now than to debug through three layers later.
The fixes were to root what the test measures, or to assert inside the mutation
where the measurement is still meaningful, and `used_words` excludes the padding
so the number means the same thing either way.

One more is worth recording, because it was the guarantee catching its own
author:

```rust
assert!(heap.mutate(|mc| mc.root()).is_nil());   // does not compile
```

The value cannot leave the mutation. It had to become `heap.mutate(|mc|
mc.root().is_nil())`. Chapter 4's compile error, arriving unannounced in
ordinary code, which is the only real test of whether a guarantee is usable.

## Where this leaves us

The substrate is finished: values, a heap, objects, and a collector that is
checked rather than trusted. The phase 0 gate asks for two things, that
`(fact 20)` runs and that the garbage collector survives an allocation stress
test, and the second half is done.

One thing is deferred, and worth naming so it is not forgotten. This collector
moves everything, every time. Most objects die young, so most of that copying is
of data that was already copied last time and will be copied again next time. A
generational collector fixes that by collecting the young often and the old
rarely, and it needs a *write barrier*, machinery to notice when an old object
is made to point at a young one. That is a different mechanism from everything
in this chapter, and it belongs in a later one.

Next the heap gets something to put in it, which means reading.
