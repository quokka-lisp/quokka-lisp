# What is alive?

> *Where this fits.* A running program makes values it then stops needing, and
> the memory has to come back. Working out automatically which values those are
> is **garbage collection**, and doing it turns out to need about a dozen
> starting points, called **roots**, however much data the system holds. It
> fails in exactly one place, the implementation language's own local
> variables, which is the hole the next chapter exists to close. Chapter 6
> builds the collector itself.

A program that runs for a while makes things it later stops needing. The space
those things occupy has to come back, or the program eventually runs out of
heap. Deciding which things are no longer needed is the whole problem, and the
first thing to get straight is that you cannot ask them.

## You cannot ask an object whether it is wanted

Go back to the heap from chapter 1:

```text
 index    0     1     2     3     4     5
        ┌─────┬─────┬─────┬─────┬─────┬─────┐
        │ 99  │ nil │ 10  │ →4  │ 20  │ nil │
        └─────┴─────┴─────┴─────┴─────┴─────┘
        └── cons C ──┘└── cons A ──┘└── cons B ──┘
```

Cons C sits at index 0 holding the list `(99)`. Is it needed?

Nothing in C can tell you. It holds a 99 and a `nil`, and it has no idea whether
anything in the system still refers to it. Walking the heap and inspecting each
object in turn answers nothing, because the information is not in the objects at
all. It is in who points at them.

So we turn the question around. Instead of asking "is this wanted?", we ask
"can this be *reached*?":

> An object is alive if you can get to it by following references, starting from
> somewhere.

Which raises the question this chapter is named after: starting from *where*?

## Values live in three kinds of place

A value, as established in chapter 1, is one 64-bit word, either data or a
reference to somewhere in the heap. Words sit in locations, and there are three
kinds of location in Quokka:

1. Inside the heap. The `→4` at index 3 is a value living in the heap.
2. In one of the VM's registers, the handful of slots the VM keeps outside the
   heap: what the evaluator is working on, the result of the last step.
3. In a Rust local variable, as in `let tail = ...;` inside the VM's own code.

The third of those is where the difficulty in this chapter and the next lives.

## A root is a place

Everything rests on one definition:

> **A root is a location that holds a value and is not inside the heap.**

The "not inside the heap" part is what makes the definition work. A heap object
is only alive if something else refers to it, so starting a trace from a heap
object would assume the very thing we are trying to work out. Roots are the
locations that are alive by definition, the VM's own working slots, rather than
objects that might be garbage.

The two words in this chapter's vocabulary are therefore different kinds of
thing entirely. A **value** is a thing, a 64-bit word holding data or a
reference. A **root** is a place, a location holding a value, which the
collector knows to look at.

(The literature uses "root" a bit loosely, and you will see people say "root the
value" or "that object is a root", meaning the object reachable from such a
location. Both are common. The location sense is the one that makes the
algorithm make sense, so keep that one.)

## The algorithm

With roots defined, collection is five lines. Suppose the VM's result register
holds `→2`:

1. Start at the roots. The result register holds `→2`, so cons A is alive.
2. Follow A's contents. Its cdr is `→4`, so cons B is alive.
3. Follow B's contents. `20` and `nil` are plain data. Nothing new.
4. Nothing left to follow. Stop.
5. Cons C was never reached, so it is garbage and its space can be reused.

"Alive" is *defined* as "reachable from a root". There is no deeper truth about
whether the program will actually use cons A again, because reachability is an
approximation, and a deliberately conservative one. A program can hold a
reference to something it will never touch again, and the collector will keep it
forever. We accept that, since the alternative requires predicting the future.

If this feels familiar, it should. The heap is a directed graph, objects are
nodes, references are edges, roots are where you start the walk, and collection
is a traversal. Every tracing garbage collector ever written is that traversal
plus bookkeeping.

## Copying collection, and what it demands

Quokka uses a *copying* collector. Rather than marking survivors in place, it
copies them into a fresh region, packed together, and abandons the old one.
After collecting the heap above:

```text
 index    0     1     2     3
        ┌─────┬─────┬─────┬─────┐
        │ 10  │ →2  │ 20  │ nil │
        └─────┴─────┴─────┴─────┘
        └── cons A ──┘└── cons B ──┘
```

Two things happened. Cons C is gone, and the space it held is available again
without any bookkeeping, because abandoning a region reclaims everything in it
at once; this is why allocating in a copying collector is no more than moving a
pointer along.

The survivors also moved. A went from index 2 to 0, B from 4 to 2, and every
reference to them had to be updated to match: A's cdr changed from `→4` to `→2`,
and the result register from `→2` to `→0`. The collector could do that because
it knew where every reference was. It found the one in A's cdr by walking the
objects it had copied, and it found the one in the result register because that
register is a root it was told about.

Which is the catch, and it is a sharper requirement than it first looks. A
copying collector does not merely need to know what is alive. It needs to know
the location of every reference to every surviving object, because it has to
rewrite them all. A reference it does not know about does not get updated, and
an object reachable only from a reference it does not know about does not get
copied at all.

## The third kind of location

Look again at the three kinds of location, and at the third one.

```rust
let tail = heap.alloc_cons(Value::fixnum(20), Value::NIL);
//  ^^^^ a Rust local variable, holding →4
```

`tail` is a location, outside the heap, holding a value, and by the definition
above that makes it a root. But the collector cannot find it. Rust local
variables live on the Rust call stack, and there is no mechanism to enumerate
them, no stack map and no runtime metadata. It is a root the collector has no
way to know exists.

So when the next line allocates, and that allocation triggers a collection:

```rust
let head = heap.alloc_cons(Value::fixnum(1), tail);   // may collect
```

the collector traces from the roots it does know about. Cons B is not reachable
from any of them, because the only thing referring to it is `tail`. The
collector concludes that B is garbage and does not copy it, `tail` still holds
`→4`, and index 4 in the new heap contains something else entirely.

> An unrooted value is not *stale*. It is **dead**. The collector did not fail
> to update it — the collector correctly determined that nobody wanted it and
> reclaimed it. Your Rust local was the only thing in the universe that
> disagreed, and it had no way to say so.

## Why this is nastier in Quokka than in C

A `Value` is a tagged **offset** into a `Vec<u64>` we own, not a pointer, and
that cuts the opposite way from what you would hope. A stale reference is not a
dangling pointer. There is no `unsafe`, no segfault, no sanitiser report, no
memory-safety violation of any kind, and Rust's guarantees hold perfectly. You
index a different slot of a perfectly valid vector and get a perfectly valid
`u64` that happens to mean something else.

A rooting bug in Quokka is therefore a correctness bug rather than a crash,
which is worse. C would hand you a segfault somewhere near the scene of the
accident. You get a cons cell whose cdr is the integer 47, three evaluations
later, in code that is fine. This single fact determines how the next chapter
weighs its options, and why so much care goes into the garbage collector's
torture mode.

## A dividend from chapter 2

In a recursive evaluator, every nested `eval` call holds a form, an environment
and partial results in Rust locals, dozens of them, nested deep, every one of
them a root the collector cannot see. That is where hand-written runtimes drown.

Quokka's evaluator cannot be recursive, for reasons that had nothing to do with
garbage collection: it has to be able to stop and hand control back to the
browser. Its state lives in four registers and a frame stack, in the heap. So
the root set is tiny:

| Root | Lives in | Slots |
| --- | --- | --- |
| Evaluator registers (`control`, `env`, `value`, `stack`) | the long-lived `Vm` struct | 4 |
| The obarray — every interned symbol | one slot in `Vm` | 1 |
| The package table | one slot in `Vm` | 1 |
| The process list (later) | one slot in `Vm` | 1 |
| Rust temporaries | chapter 4's problem | a handful |

And it stays tiny however big the heap gets, because a root is a door rather
than a room. One root pointing at the obarray gives the collector every symbol
in the system, and through each symbol's value and function cells every global
binding, and through those essentially all your data. Ten million live objects,
still about a dozen roots. The root set does not scale with the data; it scales
with how many independent places the VM can start looking from, which is a
design constant. The Lisp call stack is itself one heap object, so the entire
chain of pending Lisp calls traces from a single root, however deep the Lisp
recursion goes.

## The stack that can still overflow

None of which means nothing can run out of stack. Two places genuinely can, and
it is worth knowing where before writing them.

The reader is the first. A recursive-descent reader handling `((((((...))))))`
recurses in Rust once per level of nesting, so deeply nested input overflows the
Rust stack and kills the process, and that input is attacker-controlled the
moment a file comes from anywhere but us. It needs an explicit stack, or a hard
nesting limit that signals a Lisp condition rather than panicking.

The second is WebAssembly, which has a smaller stack than native: typically
about 1 MiB, fixed at link time, against 8 MiB for a native thread. A recursion
depth that is comfortable on a laptop can overflow in a browser, so the browser
finds these bugs first.

One place specifically cannot overflow, by choice. Cheney's collector, the one
Quokka uses, needs no auxiliary stack at all. The naive copying collector
recurses to trace references; Cheney's trick is to use the destination region
itself as the work queue, with two pointers, `scan` and `free`, chasing each
other through the objects already copied. It is a loop rather than a recursion,
and it cannot overflow however deep or tangled the object graph is.

The design document says Cheney was chosen because it is "easy to get right and
to read". This is a third reason, and for a browser-first system it is the one
that matters. [Cheney's algorithm](06-cheneys-algorithm.md) is the collector in
full.

## Where this leaves us

There is a definition of alive, an algorithm that computes it, and one hole:
Rust code sometimes has to hold a value across an allocation, and when it does,
the collector cannot see it.

Three places in the system genuinely need that, the reader, primitives that
allocate more than once, and bootstrap. Three places rather than everywhere,
thanks to chapter 2. Telling the collector about them is called **rooting**, and
it is the next chapter.
