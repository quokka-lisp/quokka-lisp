# Rooting

> **Status: the decision in this chapter is still open.** The options and the
> argument are below; the outcome is being worked out in
> [issue #1](https://github.com/quokka-lisp/quokka-lisp/issues/1) and will be
> written up here when it is settled. This is what the book looks like when it
> catches up with the work rather than reporting on it.

The previous chapter ended with a hole. Rust code occasionally has to hold a
Lisp value across an allocation, the allocation can trigger a collection, and
the collector cannot see Rust local variables. The value is not merely moved —
it is reclaimed, because nothing the collector can see wanted it.

**Rooting** is telling the collector about such a location. This chapter is
about how.

## Three places, not everywhere

Worth restating the size of the problem, because it determines how much
machinery is proportionate.

The evaluator does not need rooting at all. Its state is four registers and a
frame stack, all in the heap, all already roots. `step()` performs one small
transition and returns; it barely holds a value long enough to lose one.

Three places do need it:

1. **The reader**, building a list by allocating repeatedly while holding the
   partial structure.
2. **Primitives that allocate more than once** — accumulating results, copying a
   tree.
3. **Bootstrap and image load**, constructing the initial world.

Three places. Not "all VM code". Any solution costing more than those three
places are worth is the wrong solution.

## Option A: look at the Rust stack anyway

Scan the Rust call stack word by word and treat anything that looks like a heap
reference as a root. This is *conservative stack scanning*, and the Boehm
collector does it for C and C++ precisely because C has no way to enumerate live
pointers either.

It rules itself out here, twice over.

**Tagged offsets are indistinguishable from integers.** A cons at word index 6
is stored as `0x31`; the fixnum 24 is stored as `0x30`. Small integers are
everywhere on a stack — loop counters, lengths, indices. Half your heap would be
pinned by false positives.

**You cannot move what you are only guessing about.** If a word might be an
integer, you cannot rewrite it. So every conservatively discovered object must
be *pinned* where it is, which means a mostly-copying collector in the style of
Bartlett's, with a compaction story full of holes. Our collector moves
everything; that is the source of its simplicity.

Worth knowing as the road not taken. Not worth building.

## Option B: an explicit stack of roots

Keep our own stack of roots, outside the Rust call stack, that the collector can
walk. Rust code that needs a value to survive an allocation puts it there.

This is what V8 does (`HandleScope` and `Handle`) and what SpiderMonkey does
(`JS::Rooted`), and they chose it for exactly our reason: they are embedded in
C++, whose stack is opaque to them.

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

Two rules do most of the work. **Allocation returns `Root`, never `Value`** —
type-enforced, so you can never accidentally keep an unprotected result of an
allocation, because you are never handed one. And **never let a `Value` outlive
the statement that produced it** if anything in between can allocate — a
convention, not enforced, but one that torture mode catches immediately.

There is also a useful side effect in the signatures. A function taking
`&mut Scope` can allocate and therefore can collect. A function taking `&Heap`
cannot. One glance answers "can this move my objects?", which turns out to be a
question you ask constantly. The jargon for a point where collection may happen
is a **safepoint**; here, the type says so.

Cost: a vector push and pop per temporary, in three places in the codebase. The
evaluator never touches it. Perhaps eighty lines.

## Option C: make it a compile error

Give `Value` an invariant lifetime tied to a token that exists only while
collection cannot happen. Holding a value across an allocation then simply does
not typecheck. The `gc-arena` crate does this for Rust, and the guarantee is
real — not a convention, a proof, with no runtime cost.

Two problems, one practical and one about what this project is for.

The practical one: `gc-arena` is pointer-based, and Quokka's references must be
offsets so that images relocate and the same code runs on 64-bit native and
wasm32. You would be reimplementing the discipline, not using the crate.

The other one: the lifetime is viral. `'gc` lands in the signature of nearly
every function that touches a value, along with `PhantomData`, invariance
tricks, and higher-kinded-type emulation for the root type. For a system with a
15,000-line budget whose stated purpose includes being readable by someone
learning how this works, that is a permanent tax on every page.

And consider what it buys: it prevents a bug class that is, here, a wrong answer
rather than a memory-safety hole. In a C runtime the calculation would come out
differently.

## The argument, such as it is

The case for option B is that it is proportionate: three places need it, eighty
lines covers them, the enforcement goes where it is cheap, and the cost falls on
the code that actually allocates rather than on every signature in the system.

The case against is honest and should be stated plainly. **It is discipline plus
testing, not a proof.** Option C is the only one that makes the mistake
impossible. Choosing B means accepting that a rooting bug can be written, and
betting that there are few enough places, that torture mode finds it on the
first run, and that the readability is worth it.

It is also a real fork rather than a deferred decision. Retrofitting lifetime
branding later would be a large mechanical change across the whole crate.

## Which is why torture mode matters so much

If the safety net is tests, the tests have to be vicious. Collecting on every
single allocation is the obvious part, and it is only the start.

**Poison the abandoned region** after every flip. A stale offset should read
`0xDEADDEAD...` and fail loudly, not read plausible leftover data and fail
quietly three evaluations later. Recall that these bugs produce wrong answers,
not crashes; this is how you convert one into the other.

**Shift the heap base by a word on every collection.** Without this, a small
heap can copy an object back to the index it already had, a stale reference
accidentally still works, and your nastiest test passes for the wrong reason.
Forcing every offset to change every time is what makes torture mode prove
something.

The general principle is worth more than the specific tricks. Torture mode is
not a stress test in the usual sense — it is not about load. It is a
*determinism amplifier*: it makes the rare interleaving the only interleaving,
so a latent bug fails the first time instead of the thousandth.

## Two things this is not

**Write barriers are a different problem.** A plain Cheney collector needs none.
They arrive with a generational collector, to track references from old objects
to young ones, and they are about *mutation* rather than about roots.

**No interior pointers, ever.** Every value refers to an object's header or is
an immediate. Allowing a reference into the middle of an object makes a copying
collector dramatically harder, and the rule is free while nothing relies on
breaking it.
