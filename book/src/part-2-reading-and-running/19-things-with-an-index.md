# Things with an index

> *Where this fits.* Everything stored so far has been a list, and a list is a
> chain: to reach the thousandth element you walk a thousand cells. This
> chapter adds the other shape, a block of slots you can index in one step. On
> the way it has to answer a question the heap has been quietly carrying since
> chapter 5, which is how the collector knows which words in an object are
> references and which are just numbers. The answer is one of the better small
> decisions in the system, and it is three lines.

## Why a chain is not enough

Lisp is named after lists and they are extraordinarily useful, but they have
one cost that never goes away:

```lisp
(nth 999 long-list)     ; walks 999 cons cells
```

For most of what a program does that is fine. For three things it is not.

The obarray is the first. Every symbol the reader produces is looked up by
name, in a list (chapter 8). `intern` compares names one at a time, so reading
a file of *n* symbols costs *n²* comparisons. It works today because the
prelude has about sixty symbols in it, and it will not work for a compiler.

The compiler's own tables are the second. A compiler maps variables to stack
slots, labels to addresses, forms to the code they produced. All of those are
lookups by key, and all of them are inner loops.

Bytecode is the third. A bytecode VM fetches instruction *n* and then
instruction *n+1*, and walking a list to do it would make the bytecode slower
than the interpreter it replaced, which rather defeats the point.

So a **vector**: a run of slots, indexed in one step.

## The question the collector has been carrying

A vector is easy to allocate. Chapter 5 already gave every heap object a
header saying what it is and how many slots follow, and a vector is a header
with slots that are elements. Two lines.

The hard part is a question that only becomes urgent now. The collector
(chapter 6) walks a live object and, for each slot, follows the reference and
copies what it finds. That works because every slot it has ever seen *was* a
reference: a cons has two, a symbol has its name, value, function and plist.

But a string is a heap object too, and its slots hold character codes. The
letter `A` is 65. In the tagged-word scheme from chapter 1, 65 is `1000001`,
and the low three bits `001` say **cons cell**. A collector that treated a
string's slots as references would follow the letter `A` to heap offset 8 and
copy whatever it found there, as a cons, into the new semispace. That is not a
bug that shows up as a wrong answer; it is a bug that shows up as a corrupted
heap, some time later, somewhere else.

### The line is drawn in the type number

```rust
/// table: traced types are below 64, raw ones at or above it. Adding a type
/// on the wrong side of that line is a silent heap-corrupting bug, which is
/// why `raw_types_are_not_traced` exists.
pub const fn slots_are_values(self) -> bool {
    (self as u8) < 64
}
```

Symbol is 1, Vector is 2, Closure is 3, Environment is 7, Macro is 8, all
traced. String is **64**, Code 65, Bignum 66, all raw, their slots copied word
for word and never followed.

This is a nice piece of design and worth naming what it does. The alternative
is a flag somewhere, or a `match` listing every raw type, which is a list
someone will eventually forget to add to. Here the *number you choose when you
add a type* is the decision, so there is one obvious place to think about it,
and a test, `raw_types_are_not_traced`, that fails if a type lands on the
wrong side.

It is the same instinct as chapter 12's: a rule you must remember is a rule
that gets broken, so put it where forgetting is a failing test.

*Later: the scheme has since taken four more raw types without anything
changing. Constant and packed strings (chapter 37), double floats (chapter
48) and bit vectors (chapter 55) are 67 to 70, and each one was a number
chosen and nothing else.*

## Both of them are vectors

Common Lisp calls a general vector and a string both *vectors*, and both
*sequences*. `length` and `aref` work on either:

```lisp
(length #(1 2 3))    ; => 3
(length "abcd")      ; => 4
(aref #(a b c) 1)    ; => B
(aref "abc" 1)       ; => #\b
```

Inside, they could hardly be more different: one holds traced references, the
other raw codes on the far side of the line above. Nothing above the primitive
table can tell, and that is the point, because the difference is the
collector's business and not the program's.

In Rust that is one function answering "how long, and are the slots raw?":

```rust
fn sequence_shape<'h>(mc: &Mutation<'h>, v: Value<'h>, name: &'static str)
    -> Result<(usize, bool), EvalError>
```

and then `aref` either reads a slot or builds a character out of a code.

## Where the line between Rust and Lisp fell this time

Seven primitives went in: `%make-vector`, `vector`, `aref`, `%set-aref`,
`%sequence-length`, `vectorp`, `stringp`. Everything else is prelude:

```lisp
(defun make-array (size &key (initial-element nil))
  (%make-vector size initial-element))

(defun elt (sequence index)
  (if (listp sequence) (nth index sequence) (aref sequence index)))

(defun length (sequence)
  (cond ((null sequence) 0)
        ((consp sequence) ...walk it...)
        (t (%sequence-length sequence))))
```

`make-array` is the first thing in the system to use a `&key` parameter with a
default, so the machinery from chapter 18 earns its place in the first library
function that wanted it. That is usually how it goes: a feature looks
speculative until the next thing you write needs it.

Note what `length` became. It was list-only, with a comment saying it would be
*replaced, not extended, when sequences arrive*. Sequences arrived, and it was
extended after all, in three clauses, one per kind. The note was wrong in a
harmless direction, and the honest thing is to say so rather than quietly
rewrite history.

## A cost of `%` that only showed up here

`tests/lisp/vectors.lisp` was written with mutation in it:

```lisp
(let ((v (vector 1 2 3)))
  (%set-aref v 0 'changed)
  v)
```

SBCL said `!! error`, and it was right: there is no `%SET-AREF` in Common
Lisp. Writing a vector element is `(setf (aref v i) x)`, and `setf` is tier 1.

So that behaviour could not be checked against the oracle at all. Not
"differs", but "is not expressible in the language the oracle speaks". It was
covered by Rust tests instead, which are weaker, since they check Quokka
against our expectations rather than against the standard.

Every `%` primitive costs exactly this, and by the time hash tables landed,
two of the three things this chapter added were largely unverified against the
standard, which is a hole in the main defence chapter 12 argued for.

### So `setf` was pulled forward

`docs/design.md` puts `setf` in tier 1, and the *extensible* mechanism still
belongs there: `define-setf-expander`, `defsetf`, and the machinery
`defstruct` will need to make its accessors settable. What moved into tier 0
is a fixed-place `setf` for variables, `aref`, `svref`, `gethash`,
`symbol-function`, `car` and `cdr`, which is about twenty lines of prelude:

```lisp
(setf (aref v 0) 'changed)     ; => (%set-aref v 0 'changed)
(setf (gethash k h) 1)         ; => (%set-gethash k h 1)
(setf (car c) 'a)              ; => (let ((g 'a)) (rplaca c g) g)
```

The `car` case needs the gensym because `rplaca` returns the *cons* and `setf`
must return the *value*. Binding that value to a name the caller might also
have used would be the capture bug from chapter 15, in code generated by the
implementation rather than by a user, which is a good reminder that the
implementation is subject to its own hazards.

With `setf` in place the mutation tests moved out of Rust and back into
`tests/lisp/`, where SBCL checks them. The `%` primitives are still there,
because something has to be underneath `setf`, but nothing is tested only
against our expectations any more.

This is worth drawing out, because it is a decision made for an unusual
reason. `setf` was not pulled forward because the code was awkward without it.
It was pulled forward because without it a feature could not be tested against
the standard at all, and a phase that ships untestable features accumulates
exactly the kind of error the whole differential apparatus exists to catch.
Testability is a reason to move a feature, and it is the only reason this one
moved.

The same run turned up a second thing. `(make-array 3)` with no
`:initial-element` gives `#(0 0 0)` in SBCL and `#(NIL NIL NIL)` here, and
both conform, because CLHS says the contents are undefined until written. It
is in `tests/lisp/README.md`'s category of things that must not be tested, and
it took the oracle to notice that this was one of them.

## What is missing

- An extensible `setf`. The one that exists knows a fixed list of places; a
  program cannot teach it a new one, which `defstruct` will need.
- Fill pointers and adjustable arrays, so no `vector-push`. A compiler that
  accumulates code will want them, and they can be a vector plus a count in
  Lisp before they are anything in Rust.
- Multidimensional arrays. `make-array` takes a size, not a list of
  dimensions. Row-major indexing over a flat vector is a Lisp-level job.
- `make-string`, which is why `(reverse "abc")` gives a vector of characters
  rather than a string, where Common Lisp returns a sequence of the same type
  as its argument.
- The sequence functions: `map`, `find`, `position`, `subseq`, `concatenate`.
  All Lisp, all waiting on nothing but someone writing them.

*Later: all five are built. `make-string` came first, still in tier 0, with
issue #26. The rest are tier 1: the sequence functions are
[one chapter](../part-6-self-hosting/45-sequences.md) written over a single
iteration macro, the extensible `setf` is
[Places](../part-6-self-hosting/43-places.md), and fill pointers, adjustable
and multidimensional arrays arrived together in
[Arrays](../part-6-self-hosting/55-arrays.md), where an array that is not
simple became a header object of its own.*

## What this unlocks

Three things that have been blocked.

Hash tables, which need somewhere to put buckets, and which have a genuinely
hard question attached: a hash table keyed by object *identity* wants to hash
an object's address, and chapter 6's collector moves objects.

`defstruct`, which is a macro that writes accessors over a vector, and which
is how the compiler will represent nearly everything it works with.

An obarray that is not a list, which turns `intern` from a linear scan into a
lookup, and which the reader will need long before a compiler does.

---

**References.** CLHS 15.1 for arrays and 3.2.1 for why `make-array`'s
uninitialised contents are undefined. Richard Jones, Antony Hosking and Eliot
Moss, *The Garbage Collection Handbook* (2nd edition, 2023), chapter 11, on
the ways a collector can be told which words are references, tagged values and
type maps being two of them, and the one here being a third.
