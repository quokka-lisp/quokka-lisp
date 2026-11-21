# Saving a world

> **Where this fits.** The last chapter of the substrate, and the one that shows
> why the earlier decisions were worth their cost. A Lisp machine does not start
> by running a build; it loads an **image** — the whole contents of a running
> system, written to a file and resumed exactly where it stopped. Two decisions
> made chapters ago are what make that possible at all.

> **Status: built, and this chapter was right.** It was written in phase 0 as a
> design, because the decisions that make images possible were made then and the
> reasoning belonged next to them. Phase 1 implemented it, and the prediction
> held: saving is `to_le_bytes` over the heap's words and loading is the
> inverse, with no pointer patching and no second representation. Chapter 30 is
> the working version, including the one thing this chapter did not foresee —
> that the GC's torture padding must be switched off before a heap is written.

A Lisp machine did not start by running a build. It started by loading an
image — the entire contents of memory, written to a file and read back, with
every object exactly where it had been. You did not launch the system; you
resumed it.

Quokka works the same way, and two decisions already made are what allow it.

## What is in an image

An image file is a heap and one reference:

```text
image file
┌──────────────────────────────────────┐
│ magic "QUOKKA\0"  ·  format version  │
│ heap size in words                   │
│ root: →1024                          │ ← the one persistent root
├──────────────────────────────────────┤
│ heap words 0 .. n                    │
│   every reference inside is an       │
│   offset into this same array        │
└──────────────────────────────────────┘
```

Index 1024 holds the root object, and everything meant to survive hangs off it:

```text
root object
 ├── obarray      → every symbol → every global value and function
 ├── packages     → namespaces
 ├── processes    → each process holds its registers and its frame stack
 └── ...
```

Loading is: read the words into a fresh array, read the root, carry on.

## Two different root sets

It is easy to confuse the roots of chapter 3 with the root in that diagram.
They are different things:

| | GC roots | The image root |
| --- | --- | --- |
| Purpose | what is live *right now* | what persists |
| Lifetime | transient; meaningless after a restart | permanent |
| How many | about a dozen slots | **one** |
| Saved? | no | yes — it *is* the save |

Saving happens between slices, when the shadow stack is empty and no Rust code
is mid-way through anything. The evaluator's registers get written into the
process object, which is a heap object like any other. What remains is one root
and a block of words.

## Why the same file loads in a browser

This is the decision from chapter 1 collecting its dividend.

References in Quokka are **offsets, not addresses**. `→4` means "index 4 of the
heap array". That statement is true whether the array sits at
`0x7fff8a2c0000` in a macOS process or at offset `0x10000` in a WebAssembly
linear memory. Nothing in the image refers to a machine address, so **nothing
needs relocating**. Load the bytes; every reference is already correct.

The alternative — storing real pointers — means either walking the entire heap
on load to rewrite every reference, or demanding that the operating system map
the file at exactly the address it was saved from. The first is slow and
error-prone. The second is impossible across native and WebAssembly.

A few other things have to hold, and do:

- **64-bit words even on wasm32.** The host's pointers are 32 bits there, but
  our values are offsets into our own array. The format does not change.
- **Little-endian everywhere.** WebAssembly is defined as little-endian, and so
  are x86-64 and ARM in practice. No byte swapping. The magic number catches it
  if that ever stops being true.
- **No host state in the image, ever.** No file handles, no callbacks, no
  addresses, no credentials. An image contains Lisp objects and nothing else.
- **A size ceiling, not a format difference.** WebAssembly linear memory tops
  out at 4 GiB, and browsers cap it lower. An image saved from a large native
  session may not load in a browser. The loader should say so clearly rather
  than truncating — which is why `Value::word_index` documents that case rather
  than silently narrowing.

## Why one root is enough

Here is the part that is not obvious.

The phase 1 gate is that an image "saves, reloads and resumes where it left
off" — not resumes at a fresh prompt, but continues the computation that was in
progress. For a single root to be enough, that computation has to be reachable
from it.

It is, because of chapter 2. The evaluator's state is not on the Rust call
stack; it is explicit frames in the heap, hanging off a process object, hanging
off the process list, hanging off the root. "What the VM was doing" is just more
heap data, and gets saved like anything else.

In a recursive-evaluator design this would be impossible. The live computation
would be spread across the Rust call stack, which cannot be serialised. You
could save a heap, but never a running system — you would have to quiesce to a
fresh REPL prompt first, and "resume where it left off" would be a sentence you
could not write.

That is the yielding decision paying for itself a third time. It bought
resumable execution, it bought a tiny GC root set, and now it buys saveable
images. Three chapters, one constraint.

## The idea underneath

"Offsets, not addresses" and "one root object" are the same idea applied at two
scales: make the heap **self-contained**, so that its meaning does not depend on
anything outside itself. Once that is true, saving is `write(bytes)`, loading is
`read(bytes)`, and portability is free rather than earned.

Most language runtimes cannot do this. CPython, V8 and the JVM are not missing
the idea; their heaps are full of raw pointers and references to host resources,
so "the heap" is not a closed world. Lisp machines and Smalltalk could do it
because they were closed worlds from the start — which is exactly what Quokka's
"one world" principle is buying, and why the discipline about what may not enter
the image is worth keeping from the first commit.

## What is still undecided

- **Versioning.** A magic number and a format version are in the sketch above.
  What happens when a newer VM meets an older image is not designed yet: refuse,
  or migrate?
- **Growing and shrinking.** A copying collector has two semispaces; an image
  presumably saves only the live half, compacted. The loader then has to decide
  how much space to allocate on the way in.
- **Sharing.** `docs/design.md` notes that opening someone else's image means
  running their code. That is a security model, not a detail, and it has not
  been worked out.
