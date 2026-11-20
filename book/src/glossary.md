# Glossary

Every term this book uses that is not ordinary English, in a sentence or two,
with the chapter that explains it properly.

---

**Allocation.** Setting aside space in the [heap](#heap) for a new value. In
Quokka it is a *bump*: take the next free words and move the mark along. No
searching, no free list — a gift from the [copying collector](#copying-collector).
([5](part-1-substrate/05-objects-in-the-heap.md))

**Bignum.** An integer too large to fit in one machine word, stored in the heap
as several. Contrast [fixnum](#fixnum). Not yet implemented.

**Brand.** A lifetime attached to a type purely so the compiler can refuse
certain programs — it points at nothing and exists at compile time only. Quokka
uses one to make it impossible to hold a value across a garbage collection.
([4](part-1-substrate/04-rooting.md))

**Bytecode.** A compact artificial instruction set that a [VM](#vm) executes,
rather than the machine's own instructions. Smaller and more portable than
compiling to real machine code, and much faster than re-reading the source
structure each time. Not yet built.

**car.** The first slot of a [cons cell](#cons-cell) — the item. The name is
an abbreviation of an IBM 704 instruction from 1958 and means nothing now. Read
it as "first". ([Preface](what-has-to-be-built.md))

**cdr.** The second slot of a [cons cell](#cons-cell) — the rest of the list.
Pronounced "could-er". Read it as "rest".
([Preface](what-has-to-be-built.md))

**Cheney's algorithm.** The [copying collector](#copying-collector) Quokka uses.
Its distinguishing trick is needing no extra memory to keep track of its work:
the objects it has already copied *are* the list of things still to examine.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Collection.** One run of the [garbage collector](#garbage-collection).

**Cons cell.** The two-slot box Lisp builds lists out of: a [car](#car) and a
[cdr](#cdr). Two words in the heap, with no header, because its type is known
from the reference that points at it. "Cons" is short for "construct".
([1](part-1-substrate/01-one-word-for-everything.md),
[5](part-1-substrate/05-objects-in-the-heap.md))

**Copying collector.** A [garbage collector](#garbage-collection) that reclaims
memory by copying everything still wanted into a fresh region and abandoning the
old one. The garbage costs nothing to reclaim — you simply stop using the space
it was in. ([6](part-1-substrate/06-cheneys-algorithm.md))

**Differential testing.** Running the same test on several implementations and
requiring them to agree. Quokka runs its Lisp tests natively, under WebAssembly
and on SBCL, which acts as the *oracle* — the reference answer.

**Dotted pair.** A [cons cell](#cons-cell) whose [cdr](#cdr) is not a list, written `(a . b)`. A list is the common case of a cons cell, not the only one; the dot is how you write the general case. Also called an *improper list*. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Evaluator.** The part of a runtime that actually executes a program.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Explicit-control evaluator.** An evaluator built as a machine with visible
state — registers and a stack it owns — rather than as a function that calls
itself. The only kind that can stop halfway and resume. The name and the
technique are from *Structure and Interpretation of Computer Programs* §5.4.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Fixnum.** An integer small enough to be stored directly in one machine word,
with no heap space used. Quokka's are 63 bits. The traditional Lisp word;
contrast [bignum](#bignum).
([1](part-1-substrate/01-one-word-for-everything.md))

**Forwarding pointer.** A note left behind in the abandoned space during a
[collection](#collection), saying where an object was moved to. It is what stops
a shared object being copied twice and a circular structure looping forever.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Garbage collection.** Working out automatically, while a program runs, which
values it can no longer reach, and reclaiming their memory. The program never
says "I am finished with this"; it is deduced from what is still reachable.
([3](part-1-substrate/03-what-is-alive.md))

**Generational collector.** A collector that exploits the fact that most values
die young, by examining recently created ones often and long-lived ones rarely.
Faster than copying everything each time, and more complicated. Planned, not
built. ([6](part-1-substrate/06-cheneys-algorithm.md))


**Header.** The first word of a heap object, saying what kind it is and how many
slots follow. Cons cells have none.
([5](part-1-substrate/05-objects-in-the-heap.md))

**Heap.** The region of memory where values too big for a single word live. In
Quokka it is an array of 64-bit words the VM owns.
([3](part-1-substrate/03-what-is-alive.md))

**Image.** The entire contents of a running system, written to a file and
loadable again — not a saved document but a saved *world*, resuming exactly
where it was. Booting means loading one rather than running a build.
([7](part-1-substrate/07-saving-a-world.md))

**Immediate.** A value stored entirely in its own word, referring to nothing:
a character, `nil`, `t`. Contrast a *reference*, which points into the heap.
([1](part-1-substrate/01-one-word-for-everything.md))

**Interning.** Keeping one object per name, so that two reads of `foo` produce the *same* [symbol](#symbol) rather than two equal ones. Without it, `eq` on symbols fails and every variable lookup becomes a string comparison. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Invariance.** A property of a type's lifetime parameter meaning it cannot be
silently substituted for a shorter or longer one. Quokka's [brand](#brand) must
be invariant or the guarantee evaporates while everything still compiles.
([4](part-1-substrate/04-rooting.md))

**Lisp-2.** A Lisp where a [symbol](#symbol) has separate value and function cells, so `(list list)` is legal — the first is the function, the second the variable. Common Lisp is one. Scheme, a *Lisp-1*, has a single cell. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Mutation.** In Quokka, one bounded stretch of work on the heap. No
[collection](#collection) can happen inside one, which is what makes holding a
value safe. ([4](part-1-substrate/04-rooting.md))

**nil.** The empty list, and also false, and also the end-marker of every list.
One value doing three jobs.

**Obarray.** The table of every [interned](#interning) symbol. The name is from "object array" and is a 1960s MIT detail that stuck. Quokka's is a list for now, and a [root](#root) the collector must not forget. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Parser.** What the [reader](#reader) is called in most languages, where it is far larger — it must know operator precedence, statements versus expressions, declarations. A Lisp reader needs none of that, because the text is already the data structure. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Primitive.** An operation the [VM](#vm) provides directly in Rust rather than
in Lisp — arithmetic, allocation, input and output. The *primitive table* is the
only door between the two languages.

**Printer.** Turns heap objects back into text. The *P* in [REPL](#repl), and how a [reader](#reader) is tested: text that survives read-then-print was almost certainly read correctly. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Quote.** `'x` reads as the list `(quote x)` — ordinary structure, not syntax. The evaluator later treats `quote` as "give me this rather than running it", which is how a program says *data* rather than *code*. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Reachable.** Able to be found by following references from a [root](#root).
This is the definition of "still wanted" that garbage collection uses.
([3](part-1-substrate/03-what-is-alive.md))

**Reader.** The part of a runtime that turns text into structure. In Lisp it is
unusually small, because the structure it produces is ordinary lists.
([Preface](what-has-to-be-built.md))

**REPL.** Read, Eval, Print, Loop — a prompt that reads what you type, runs it,
prints the answer and comes back for more. Invented for Lisp in 1964; now in
every language. ([Preface](what-has-to-be-built.md))

**Root.** A location holding a value that is *not* inside the heap — a machine
register, a global table, a local variable in the implementation language.
Garbage collection starts its search from these. A root is a *place*; a
[value](#value) is a *thing*.
([3](part-1-substrate/03-what-is-alive.md))

**Rooting.** Telling the collector about a location it cannot find by itself.
([4](part-1-substrate/04-rooting.md))

**S-expression.** "Symbolic expression": Lisp's single notation, used for both
code and data. `(* 6 7)` is a list of three things, not special syntax.
([Preface](what-has-to-be-built.md))

**SBCL.** Steel Bank Common Lisp, a mature Common Lisp implementation. Quokka
uses it as a reference to check its own answers against, not as something it
depends on to build.

**Semispace.** One of the two halves a [copying collector](#copying-collector)
works between. At any moment one holds everything and the other is empty.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Slice.** A bounded stretch of execution after which the VM returns control to
whoever called it, so a browser can repaint and respond. The caller says how
many steps. ([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Symbol.** A named thing — `car`, `+`, `my-variable`. Lisp symbols are objects
in their own right, not just text, and carry a value and a function.

**Tag.** A few bits inside a word saying what kind of value it holds. Quokka
uses the low three bits, which are free because heap objects are aligned to
8-byte boundaries. A *tagged word* is a value plus its tag.
([1](part-1-substrate/01-one-word-for-everything.md))

**Torture mode.** A build setting that makes rare conditions happen constantly —
here, collecting garbage at every opportunity and checking the heap afterwards —
so latent bugs fail on the first test run rather than the thousandth.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Tracing.** Following references from the [roots](#root) to find everything
[reachable](#reachable). Also the thing you must *not* do to a string's
characters, which are not references.
([3](part-1-substrate/03-what-is-alive.md))

**Unbound.** The marker a [symbol](#symbol)'s value or function cell holds before anything is assigned. Distinct from `nil`: the difference between "this variable does not exist" and "this variable holds the empty list", which Lisp signals an error about. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Value.** One piece of Lisp data as the machine holds it: in Quokka, exactly one
64-bit word, either the data itself or a reference to somewhere in the heap.
([1](part-1-substrate/01-one-word-for-everything.md))

**VM.** Virtual machine: a program that behaves like a computer for the language
running on it, with its own memory and instructions. Quokka's is written in Rust
and does what a Lisp machine's hardware once did.

**WebAssembly (WASM).** A compact instruction format that runs inside a web
browser at close to native speed. Quokka compiles to it, which is why the VM
must be able to stop and hand control back.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Write barrier.** Machinery that notices when an old object is made to point at
a new one. A [generational](#generational-collector) collector needs it; the
simple copying collector here does not.
([6](part-1-substrate/06-cheneys-algorithm.md))
