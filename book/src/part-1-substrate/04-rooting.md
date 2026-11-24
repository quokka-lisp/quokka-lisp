# Rooting

> *Where this fits.* The last chapter left a hole: the garbage collector cannot
> see values held by the implementation language itself, so a Rust local can go
> on holding a reference to an object that has already been reclaimed, and the
> program quietly computes a wrong answer. This chapter closes that hole by
> making the mistake impossible to write rather than merely easy to catch. It
> takes thirteen lines of Rust and no `unsafe` to do, the whole guarantee turns
> on how one `PhantomData` is spelled, and it raises a question with no obvious
> answer: how do you test a claim that certain programs cannot exist? It comes
> before the heap and the collector it protects because what it settles is a
> contract, what every allocation hands back and what every function takes, and
> that contract lands in the signature of code that does not exist yet.

*Status: decided and built, issue #1.*

**Rooting** is telling the collector about a location it cannot find for
itself, and this chapter is about how.

## Where is rooting necessary?

The evaluator does not need rooting at all. Its state is four registers and a
frame stack, all in the heap and all already roots, and `step()` performs one
small transition and returns, so it barely holds a value long enough to lose
one. Three places do need it:

1. The reader, building a list by allocating repeatedly while holding the
   partial structure.
2. Primitives that allocate more than once, accumulating results or copying a
   tree.
3. Bootstrap and image load, constructing the initial world.

Three places, rather than all VM code. Any solution costing more than those
three places are worth is the wrong solution.

## Looking at the Rust stack anyway

The first option is to scan the Rust call stack word by word and treat anything
that looks like a heap reference as a root. This is *conservative stack
scanning*, and the Boehm collector does it for C and C++ precisely because C has
no way to enumerate live pointers either. It rules itself out here, twice over.

Tagged offsets are indistinguishable from integers. A cons at word index 6 is
stored as `0x31` and the fixnum 24 is stored as `0x30`, and small integers are
everywhere on a stack, as loop counters, lengths and indices, so half the heap
would be pinned by false positives.

You also cannot move what you are only guessing about. If a word might be an
integer you cannot rewrite it, so every conservatively discovered object has to
be *pinned* where it is, which means a mostly-copying collector in the style of
Bartlett's, with a compaction story full of holes. Our collector moves
everything, and that is the source of its simplicity. The option is worth
knowing as the road not taken, and not worth building.

## An explicit stack of roots

The second option is to keep our own stack of roots, outside the Rust call
stack, that the collector can walk, and have Rust code that needs a value to
survive an allocation put it there. This is what V8 does with `HandleScope` and
`Handle`, and what SpiderMonkey does with `JS::Rooted`, and they chose it for
exactly our reason: they are embedded in C++, whose stack is opaque to them.

In Quokka's terms:

```rust
/// A region of Rust code that may allocate. Roots pushed inside it are
/// traced by the collector and dropped when the scope ends.
pub struct Scope<'h> {
    heap: &'h mut Heap,
    base: usize,          // where this scope's roots begin
}

/// A slot in the root stack. Copy, because it is just an index.
#[derive(Clone, Copy)]
pub struct Root(usize);

impl Scope<'_> {
    /// Protect a value. Returns a handle, not the value.
    pub fn root(&mut self, v: Value) -> Root { /* push onto heap.roots */ }

    /// Read a rooted value. Always re-read after anything that can allocate.
    pub fn get(&self, r: Root) -> Value { self.heap.roots[r.0] }

    /// Allocate. May collect. Returns a handle, never a bare Value.
    pub fn cons(&mut self, car: Value, cdr: Value) -> Root { /* ... */ }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) { self.heap.roots.truncate(self.base); }
}
```

The broken reader from the last chapter becomes:

```rust
let tail = s.cons(Value::fixnum(20), Value::NIL);   // a Root
let head = s.cons(Value::fixnum(1), s.get(tail));   // re-read through the root
```

Two rules do most of the work. Allocation returns `Root` and never `Value`,
which the type system enforces, so you can never accidentally keep an
unprotected result of an allocation because you are never handed one. And a
`Value` must never outlive the statement that produced it if anything in
between can allocate, which is a convention rather than something enforced, but
one that torture mode catches immediately.

There is also a useful side effect in the signatures. A function taking `&mut
Scope` can allocate and therefore can collect, and a function taking `&Heap`
cannot, so one glance answers "can this move my objects?", which turns out to
be a question you ask constantly. The jargon for a point where collection may
happen is a **safepoint**, and here the type says so.

The cost is a vector push and pop per temporary, in three places in the
codebase, with the evaluator never touching it. Perhaps eighty lines.

## Making it a compile error

The third option is to give `Value` an invariant lifetime tied to a token that
exists only while collection cannot happen, so that holding a value across an
allocation does not typecheck. The `gc-arena` crate does this for Rust, and the
guarantee is real: not a convention but a proof, with no runtime cost.

There are two problems, one practical and one about what this project is for.
The practical one is that `gc-arena` is pointer-based, while Quokka's
references must be offsets so that images relocate and the same code runs on
64-bit native and wasm32, so you would be reimplementing the discipline rather
than using the crate.

The other is that the lifetime is viral. `'gc` lands in the signature of nearly
every function that touches a value, along with `PhantomData`, invariance
tricks, and higher-kinded-type emulation for the root type. For a system with a
15,000-line budget whose stated purpose includes being readable by someone
learning how this works, that is a permanent tax on every page. And what it
buys is the prevention of a bug class that is, here, a wrong answer rather than
a memory-safety hole. In a C runtime the calculation would come out
differently.

## What settled it

The argument above came out slightly in favour of the second option, on grounds
of proportion. It was wrong, and what overturned it was an observation about
the shape of the project rather than about Rust.

Rust is only the VM. Everything above the primitive table is Lisp, and Lisp
code cannot have this bug at all. Not "is unlikely to", but cannot: a Lisp value
is always in the heap or in an evaluator frame, and frames are heap objects
reachable from the registers, so there is no such thing as an unrooted Lisp
local.

Which means that rule 1 of the project, *Rust below the primitive table, Lisp
above it*, is a correctness boundary and not only a design preference. Every
line moved above that line is a line that structurally cannot have this class
of bug. Nobody planned that; it falls out of the explicit-control evaluator
from chapter 2, which we adopted for an entirely different reason.

Once you see that, the argument for the second option inverts. It was justified
by cost: three places need rooting, eighty lines covers them, and branding is
expensive. But if the exposed surface is small and permanently bounded, then the
cost of the stricter scheme is bounded by exactly the same argument. You do not
pay the lifetime tax across a growing system; you pay it once, over a part that
stops growing.

So we took the third option, with two exit conditions agreed in advance: we
would abandon it if the type machinery exceeded about a hundred lines before
any heap logic existed, or if the guarantee could not be proven sound.

## The mechanism

Thirteen lines, no `unsafe`, and no runtime check.

```rust
pub fn mutate<F, R>(&mut self, f: F) -> R
where
    F: for<'h> FnOnce(&mut Mutation<'h>) -> R,
{
    let mut mutation = Mutation {
        storage: &mut self.storage,
        _brand: PhantomData,
    };
    f(&mut mutation)
}
```

The higher-ranked bound `for<'h>` means the closure must work for *every*
brand. `R` is a type fixed outside the closure, so `R` cannot mention `'h`, and
a `Value<'h>` therefore has nowhere to go: not out through the return value,
and not into anything captured from outside, whose lifetimes are already fixed.
`Heap::collect` takes `&mut self`, which a live `Mutation` borrows, so
collecting during a mutation does not compile either. Every point where the
collector may run is a point where no Rust local holds a value.

The broken code from chapter 3 is now unwritable:

```rust
heap.mutate(|mc| {
    let tail = mc.cons(Value::fixnum(2).unwrap(), Value::NIL);
    let head = mc.cons(Value::fixnum(1).unwrap(), tail);  // holding `tail`: fine
    mc.set_root(head);
});
heap.collect();   // legal here, and only here
```

Holding `tail` across an allocation is safe *because* collection cannot happen
inside the mutation. The danger was never holding a value; it was holding one
across a collection, and the two are now the same thing.

## The trap

The brand is written like this:

```rust
pub type Invariant<'h> = PhantomData<fn(&'h ()) -> &'h ()>;
```

Not like this:

```rust
PhantomData<&'h ()>      // covariant — the guarantee silently evaporates
```

The obvious spelling is *covariant* in `'h`, which means a long brand can be
shortened to a short one at will, and that is precisely the escape the whole
scheme exists to prevent. Writing it that way gives you a proof that is not
one, and everything still compiles, every test still passes, and you have no
way of knowing. Putting `'h` in both argument and return position of a function
type forces invariance. It looks like noise, and it is the only thing making
any of this true.

Which raises the question the rest of the chapter is really about: how do you
test a guarantee?

## Testing something that is supposed to be impossible

An ordinary test runs code and checks the answer. Here there is no answer to
check, because the claim is that certain programs *do not exist*.

Rust has the tool built in. A doctest marked ```` ```compile_fail ```` fails if
the code compiles:

```rust
/// ```compile_fail
/// # use quokka_vm::heap::Heap;
/// # use quokka_vm::value::Value;
/// let mut heap = Heap::new();
/// let escaped = heap.mutate(|mc| mc.cons(Value::NIL, Value::NIL));
/// ```
```

We checked nine cases before building anything on the brand: legal use,
returning plain data, returning a value, assigning to an outer variable,
stashing in an outer `Vec`, smuggling through a `Cell`, collecting during a
mutation, shortening the brand, and the same shortening with a covariant brand.

That last one is not padding. A compile-fail test that fails for the wrong
reason proves nothing, and a test asserting "this does not compile" is uniquely
prone to it, because almost any mistake makes code not compile. So one test
asserts that the covariant spelling *would* allow shortening, and if that
stopped passing, the invariance test would be meaningless and we would be told.

This was not a hypothetical worry. The first version of the harness reported
five escape attempts correctly rejected. They were being rejected because
`rustc` will not accept a crate name containing a hyphen: five green ticks,
nothing tested. It is the same failure the whole decision was about, one level
up. A guarantee you believe but have not checked is worse than a convention you
know is a convention, because you stop looking. We added two positive controls.

## What the brand does not do

Two limits are worth being precise about.

It stops a value escaping a mutation, and it does not stop one being invented
outside one. If `Value::cons(42)` were public, anyone could conjure a heap
reference at an arbitrary brand and the guarantee would be worthless, so the
heap-reference constructors are crate-private, reachable only through a
`Mutation`. Immediates, meaning fixnums, characters and `nil`, stay public,
because they refer to nothing. The split falls exactly along *does this word
point into the heap*.

It also protects callers from the collector rather than the collector from
itself. It does nothing about tracing the wrong field, mis-sizing a header, or
botching the forwarding-pointer dance, and those produce the same silent wrong
answers. They are honestly the likelier bugs, since the collector is the part
being written from scratch for the first time. Branding narrows the problem,
and the torture mode in [Cheney's algorithm](06-cheneys-algorithm.md) is what
addresses the rest. A compile-time guarantee produces a feeling of safety wider
than the thing it actually guarantees, which is why it is worth saying where
this one stops.

## The cost, paid in full

Collection can only happen between mutations, so the heap must absorb whatever
one mutation allocates. In a general-purpose Rust runtime that is a real
discipline, and `gc-arena`, the crate this technique comes from, documents long
mutations as a known limitation.

Here it costs almost nothing, because rule 4 already forbids long mutations.
The evaluator must return control to the browser promptly, so one step per
mutation gives fine-grained collection for free. Two constraints adopted for
unrelated reasons turned out to demand the same thing, which usually means both
are downstream of something more basic. In this case, a runtime that can be
interrupted must keep its state somewhere the interrupter can see.

The heap this protects is [the next chapter](05-objects-in-the-heap.md), and
the collector it protects it from is
[the one after](06-cheneys-algorithm.md).

---

## Appendix: the argument that lost

Kept because the reasoning is more useful than the conclusion.

## The case for an explicit root stack

The case for the second option is that it is proportionate: three places need
it, eighty lines covers them, the enforcement goes where it is cheap, and the
cost falls on the code that actually allocates rather than on every signature
in the system.

The case against it is honest and should be stated plainly. It is discipline
plus testing, not a proof. The third option is the only one that makes the
mistake impossible, and choosing the second means accepting that a rooting bug
can be written, and betting that there are few enough places, that torture mode
finds it on the first run, and that the readability is worth it.

It is also a real fork rather than a deferred decision, since retrofitting
lifetime branding later would be a large mechanical change across the whole
crate.

## Which is why torture mode matters so much

If the safety net is tests, the tests have to be vicious. Collecting on every
single allocation is the obvious part, and it is only the start.

Poison the abandoned region after every flip, so that a stale offset reads
`0xDEADDEAD...` and fails loudly rather than reading plausible leftover data
and failing quietly three evaluations later. These bugs produce wrong answers
rather than crashes, and this is how you convert one into the other.

Shift the heap base by a word on every collection. Without this, a small heap
can copy an object back to the index it already had, a stale reference
accidentally still works, and your nastiest test passes for the wrong reason.
Forcing every offset to change every time is what makes torture mode prove
something.

The general principle is worth more than the specific tricks. Torture mode is
not a stress test in the usual sense, because it is not about load. It is a
*determinism amplifier*: it makes the rare interleaving the only interleaving,
so a latent bug fails the first time instead of the thousandth.

## Two things this is not

Write barriers are a different problem. A plain Cheney collector needs none.
They arrive with a generational collector, to track references from old objects
to young ones, and they are about *mutation* rather than about roots.

No interior pointers, ever. Every value refers to an object's header or is an
immediate. Allowing a reference into the middle of an object makes a copying
collector dramatically harder, and the rule is free while nothing relies on
breaking it.
