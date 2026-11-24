# Saving the world

> *Where this fits.* Chapter 29 left the system able to compile everything and
> allowed to install nothing, and said the way out was an image. This is the
> image: the whole heap written to a file and read back, which is how a Lisp
> machine started, how Smalltalk still starts, and how Quokka will start from
> now on. It is also phase 1's gate, *an image saves, reloads and resumes
> where it left off*, and by the end you will see why that took so little
> code, for reasons decided long before anyone was thinking about files.

Everything you have built in this book lives in one `Vec<u64>`. Symbols,
functions, the macro table, a half-finished computation: all of it is words in
the heap of chapter 5.

An **image** is that heap, written to a file.

```bash
cargo run -p quokka -- build world.qi
```

```
world.qi: 107144 bytes, 13347 words, sequence 1
```

A hundred kilobytes is the entire system above the Rust line: the prelude, the
compiler, every symbol either of them interned. And:

```bash
cargo run -p quokka -- world.qi
```

```
Loaded world.qi: 13347 words, sequence 1.
quokka> (reverse '(1 2 3))
(3 2 1)
```

That start did not interpret the prelude. It did not read `prelude.lisp` at
all. It read a file of numbers into a vector and began. This is what the
design means by "booting means loading an image, not running a build", and it
is the normal condition of a Lisp machine or a Smalltalk: the system you are
using is a *saved state*, not a program that was run.

## Why this is normally the hard part

Writing a heap to a file sounds like the easiest thing in a runtime and is
usually one of the hardest. The reason is pointers.

In a typical runtime, an object that refers to another holds that object's
**address**, meaning where it sits in the process's memory. Say a cons cell at
`0x7f3a_1c00_4080` holds `0x7f3a_1c00_41a0` in its `car`. Write those sixteen
bytes to a file and you have saved a fact about a process that no longer
exists. The next run will map its heap somewhere else, and
`0x7f3a_1c00_41a0` will be nothing, or worse, something.

So the file format has to be a *different representation* from the in-memory
one, and loading has to rebuild the graph: walk every object, allocate a new
one, and patch every reference to point at the new address. That is a real
traversal, with a worklist and a table mapping old locations to new, and it
has to know the layout of every object type, which means it is a second
description of your object layouts, which has to be kept in step with the
first one forever. Jones, Hosking and Moss survey the variations in *The
Garbage Collection Handbook* (2012), §11.

Quokka's entire save is this:

```rust
for word in header.iter().chain(words.iter()) {
    bytes.extend_from_slice(&word.to_le_bytes());
}
```

and the entire load is that backwards.

## The rule that made it free

Look again at **rule 3**, from the very first pages of the design:

> Heap references are offsets, never raw pointers.

A `Value` that names a cons cell does not hold an address. It holds the
**word index** of that cell within the heap's vector: reference number 4,312,
meaning "the 4,312th word of the heap", whichever memory that vector happens
to occupy today.

The rule was written for two reasons and neither was files. One: wasm32 has a
32-bit address space and 64-bit native does not, and a system that stores
addresses has two different object layouts and one of them has to be
maintained blind. Two: the collector of chapter 6 moves every object it keeps,
and a reference that survives moving is a reference that was never tied to a
location.

The consequence nobody was aiming at is that index 4,312 means the same thing
in any process that has the same words. The heap is already
position-independent. There is nothing to relocate, because nothing was ever
located.

This is worth sitting with, because it is the strongest example in this book
of a decision paying for itself somewhere else. Rule 3 cost something real:
every heap access is an index into a vector rather than a dereference, which
is an extra bounds check the hardware would not have charged for. It bought
wasm32 and a moving collector. And then, three phases later, it made the image
a `memcpy`. Representations that avoid naming a particular machine tend to
keep being portable, including through time.

## Saving a computation in mid-air

The gate says "resumes where it left off", and that is a stronger claim than
"the definitions come back". Here is the test:

```rust
// Start it, and stop after a handful of steps — far short of finishing.
let finished = heap.mutate(|mc| {
    let mut machine = Machine::resume(mc);
    matches!(run_slice(&mut machine, 40), Status::Done { .. })
});
assert!(!finished, "40 steps should not finish a 60-deep recursion");

// Save it exactly there, and throw the original away.
let bytes = save(&mut heap);
let mut reloaded = load(&bytes, heap.soft_limit()).expect("loads");
drop(heap);
```

The computation then runs to completion **in the other heap** and produces
1830. Nothing restarted it; the recursion was fifty-something frames deep when
the file was written, and it was fifty-something frames deep when it was read.

This works for the same kind of reason as the last one. **Rule 4**: the VM
must be able to yield, so execution state lives in the heap and never on the
Rust call stack, with no recursive `eval`. That rule exists because the
browser's event loop must keep turning (chapter 2), and it is why chapter 9's
evaluator is an explicit state machine with its frames as heap objects rather
than the twenty-line recursive `eval` that every textbook starts with.

The cost was paid in chapter 9 and it was not small: an explicit-control
evaluator is harder to write and harder to read than the recursive one. What
it bought, as far as anyone knew at the time, was a browser that did not
freeze.

What it also bought is this. There is no part of a Quokka computation that
lives somewhere a file cannot reach. An image written between two instructions
reloads between those two instructions. Green threads, a debugger that can
resume a frame, and "save your work in the middle of a long computation" all
fall out of the same property, and none of them needed a line of code here.

## The format

Little-endian `u64`s, all the way down.

| word | what |
| --- | --- |
| 0 | magic — `"QUOKKA\0\0"`, so a file that is not an image is refused as one |
| 1 | format version |
| 2 | how many heap words follow |
| 3 | checksum of everything after word 3 |
| 4–21 | the *scalars*: 18 fields that are not in the heap's words |
| 22… | the heap |

The scalars are everything about the heap that is not one of its words: the
root, the obarray (the list of every interned symbol), the evaluator's eight
registers, the bytecode VM's program counter, stack pointer, frame pointer and
mark pointer, the evaluator's mode, the `gensym` counter, and where the static
area ends.

That list *is* the resumable state. If you want to know what a Quokka
computation consists of, it is those eighteen numbers plus the heap, and the
honest way to say so is that the image format is the definition.

### What is deliberately not saved

Four things, each for a reason worth stating because each one looks like an
oversight.

The heap limits. `soft_limit` and `hard_limit` are the *host's* policy, not
the image's property. An image moved from a desktop to a phone should obey the
phone's limits. So `load` takes a limit from its caller, and the browser will
pass something smaller than the native REPL does for the same file.

`exhausted`, the flag meaning "an allocation ran past the hard limit". A heap
that could not allocate is not a heap worth preserving, and restoring the flag
would halt the evaluator for a reason that no longer applies.

`collections`. It counts collections *in this process*, and it is used only by
torture mode to vary its padding. Restoring it would be restoring a fact about
a process that has exited.

The torture padding. GC torture mode (chapter 6) pads the front of the dynamic
area by a varying number of words, so that no object can land where it was and
a stale reference cannot accidentally still be right. That is the correct
thing to do to a *running* heap and precisely the wrong thing to write to a
file, so `save` collects without it.

This one was a bug before it was a decision, and the shape of it is worth
keeping. The first version called the ordinary `collect`, and every image test
passed. Under `--features gc-torture` three of them failed, one reporting a
*forwarding pointer left behind at word 24*. There was no forwarding pointer.
The saved words had objects starting after the padding; the loaded heap put
its `dynamic_start` back at `static_end`, because that is what the code said;
and the verifier, walking from the wrong place, read padding as an object and
reported the first thing that did not make sense.

Two halves disagreeing about where the heap starts produces an error message
about something else entirely, several words away. That is the normal
behaviour of a layout bug, and the reason the torture mode and the verifier
both exist is that nothing else would have asked the question.

### Collecting first

`save` collects the heap before writing it, with padding switched off as
above, and three separate reasons point the same way:

1. No garbage in the file. Every dead word is a word downloaded in a browser.
2. Nothing to skip. A copying collector (chapter 6, Cheney's algorithm) leaves
   the survivors packed contiguously from the start of the heap, so the live
   set *is* a run of words and the writer does not need to know where the
   holes are.
3. Canonical output. The same world saved twice gives the same bytes. That is
   what phase 2's gate needs, since "rebuilt compiler matches the first build
   byte for byte" is only true because collection puts the survivors in a
   deterministic order.

The test asserts exactly that, with one exception:

```rust
assert_eq!(first[40..], second[40..], "the same world saved twice is the same bytes");
assert_ne!(first[32..40], second[32..40], "except the sequence number");
```

### The sequence number

Word 4 is a counter that goes up by one on every save. It is the only part of
the design's change-log work (issue #24) that could not be postponed, and the
reason is pure format politics: a field cannot be added to images that already
exist. Everything else about recording what happened between two snapshots can
be decided later; a header field has to be reserved now or never.

What it is for: given two images of the same world, saying which came after
which without comparing their contents.

### Verifying on load

The last thing `load` does is run `Heap::verify`, which walks the heap
linearly and checks that every reference lands on an object boundary.

The checksum and the verifier answer different questions, and conflating them
is easy. The checksum proves *the bytes are the bytes that were written*, so
it catches a truncated download, a half-written file, a flipped bit. It says
nothing about whether those bytes were a **sound heap**, which a collector bug
could perfectly well have made them not be: a slot the collector forgot to
update is written faithfully and is still a reference into nowhere.

Checking once at load rather than on every access is the same bargain the
bytecode verifier makes in chapter 25, and the JVM makes when it loads a
class: pay for one linear pass when the thing arrives, and then trust it
completely for as long as it runs. Checking each access instead would be a
cost on every single operation forever, to catch something that can only have
gone wrong once.

## Trying it

A world with your own code in it:

```bash
cargo run -p quokka -- build mine.qi mine.lisp
```

where `mine.lisp` is

```lisp
(defun double (x) (* 2 x))
(defmacro twice (form) (list 'progn form form))
(defstruct point x y)
```

and then, in a new process:

```
quokka> (double 21)
42
quokka> (twice (double 5))
10
quokka> (point-x (make-point :x 7 :y 9))
7
```

A function, a macro and a record type, three quite different kinds of thing,
one of which generates ten functions of its own (chapter 21), all present in
a process that never read a line of Lisp.

## Saving from inside

*Added in phase 2 (#51).* For a long time an image could be made only from
outside: build one from source, or rebuild one. Anything typed into a running
world was lost when the program ended, which for a system whose pitch is *one
world you work inside* is backwards. We noticed it while reading the
developer manual: the REPL seemed to exist only to build images.

The fix is a REPL command:

```text
quokka> (defun hello () :saved-in-the-world)
HELLO
quokka> :save mine.qi
mine.qi: 360416 bytes, 45005 words, sequence 2
```

It is a command and not a Lisp function, and that was a choice. The more
Lisp-machine-like design is a function, `(save-world "mine.qi")`, asking the
host through a request like any other (chapter 31). This chapter has already
shown what that would mean: the save would happen *during* the call, so the
saved world would contain the call, and loading it would resume inside
`save-world`, returning a second time in a different process. That is
possible here, and it is exactly the kind of cleverness that needs thinking
through first. The command is handled by the host *between* two forms, when
nothing is running, so the world it saves is always at rest.

The same change made the program say which machine you are talking to. Plain
`quokka` now needs a world and refuses without one. The world's banner names
it and says *compiled · tier 1*. The interpreter that builds the first world
has its own subcommand, `quokka bootstrap`, introduces itself as the
bootstrap, and prompts `boot>`.

## Status

Built, and phase 1's gate is met: images save, reload, and resume
mid-computation.

When this chapter was written, the world in that image was still
*interpreted*: `prelude.lisp` had been interpreted into it, so `length` in
`world.qi` was a tree being walked, not bytecode. Installing compiled code
became possible here, since a heap about to be written to a file and abandoned
is exactly where it is safe, and chapter 32 does it, in two passes, with a
scan that proves nothing interpreted survives. `--save-image` (today `quokka
build`) now builds that compiled world.

---

**References.** Jones, Hosking and Moss, *The Garbage Collection Handbook*
(2012), §11, on heap layouts and what pointer representation costs and buys.
Goldberg and Robson, *Smalltalk-80: The Language and its Implementation*
(1983), part four, for the original image-as-the-system design and the
snapshot primitive. Moon, *Garbage Collection in a Large Lisp System* (1984),
for the Lisp machine's view of a heap that outlives the process. The CLHS has
no `save-image`: it is one of the places where every real Common Lisp has an
extension (`sb-ext:save-lisp-and-die`) and the standard is silent.
