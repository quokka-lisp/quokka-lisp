# Arrays, all of them

> *Where this fits.* Since chapter 19 Quokka has had vectors and strings:
> one dimension, a fixed length, elements found by index in one step. Common
> Lisp's arrays are much more than that, and the plan put the rest in tier 2.
> This chapter, the sixth worked from ansi-test's list (chapter 49), began as
> a small fix to `make-array` and became the whole of the standard's chapter
> 15: arrays of any number of dimensions, fill pointers, arrays that grow,
> arrays that share another's elements, and bit vectors. It adds the first
> new kind of heap object since floats (chapter 48), and it is the step that
> took the Rust core past its first 15,000-line guide.

*Status: done, issue #67. Step T of phase 2.*

## Why did one message cover half the failures?

After chapters 52 to 54, 725 tests in the measured groups failed with
nothing to explain them, and phase 2's gate allows none. We sorted them by
the message they failed with, and 388 said the same thing: `bad call:
unknown keyword argument`. Almost all came from `make-array`, through
helpers in ansi-test's own auxiliary files such as this one:

```lisp
(defun make-special-string (string &key fill adjust displace base)
  (let* ((len (length string))
         (len2 (if fill (+ len 4) len))
         (etype (if base 'base-char 'character)))
    (if displace
        (let ((s0 (make-array (+ len2 5)
                              :initial-contents
                              ...
                              :element-type etype)))
```

Quokka's `make-array` took a size and `:initial-element`, which was all the
compiler needed in tier 0. The helpers ask for `:element-type`,
`:initial-contents`, `:fill-pointer`, `:adjustable` and `:displaced-to`, to
test that a sequence function works on every kind of vector and not only
the simple kind.

## Should we do half now?

Our first proposal split the keywords in two. The first three describe a
one-dimensional vector or string, which tier 1 had; the others are what the
plan had put in tier 2. So: take the first three now, and teach the runner
that a test mentioning `:fill-pointer` needs a tier-2 feature, which would
explain those failures without fixing them. The alternative we listed was
to add fill pointers and adjustable vectors as well.

We were asked why not the second, and whether it was wrong or only too much
work for now: if it will be needed later, do it now. It was not wrong. But
written as we had written it, it was half of a design. The standard has one
array model, and implementations build it once: a **simple array** has one
dimension and nothing else to it, and every other array is a **header**, a
small object in front of a simple array that holds its elements in
row-major order, with the fill pointer, the dimensions and the displacement
in the header. Adding a fill-pointer slot to the vectors we had would have
been taken out again when several dimensions and displacement arrived. So
the step became the whole chapter of the standard, bit vectors included,
moved from tier 2 into phase 2.

## What is a header?

The simple arrays stay exactly as they were: a general vector, one of the
three string layouts (chapter 37), and a new **bit vector**, raw words with
64 bits to a word. Everything else is an `ArrayHeader`, a heap object of six
slots:

| slot | |
| --- | --- |
| data | the array whose elements, in row-major order, are this one's |
| offset | where in the data this array's first element is |
| fill | the fill pointer, or nil |
| dimensions | a list of fixnums |
| displaced | true if the data is an array this one was displaced to |
| adjustable | true if `adjust-array` may change it in place |

**Row-major order** is how a grid becomes a line: the element at row `i`,
column `j` of a 2-by-3 array is element `3i + j` of its data. An array
**displaced** to another has no elements of its own; its data is the other
array, and the offset says where in it this one starts, so writing through
either is seen through both. An **adjustable** array can change size with
`adjust-array` and stay the same object, which is the reason the header
exists at all: an object in our heap cannot grow where it stands, so the
header keeps its place and gets new data, and every reference to it sees the
new size.

## Where does the header live, Rust or Lisp?

That was the second decision. A header could be a heap object type that
Rust knows, so that `aref` and `length` look through it in one step; or a
Lisp structure, with the primitives handing anything they do not recognise
back to Lisp, as arithmetic hands a bignum to `%generic-+` (chapter 47). The
second would have cost about 50 lines of Rust instead of about 200, but
every access to a non-simple array would have been a failed primitive and a
Lisp call, and the type predicates would have had to be redefined in Lisp,
slowing every call to them. We chose the first. What an object looks like
in memory is Rust's side of rule 1, and one path through `aref` for every
array is easier to follow.

Rust follows a header to its storage, through any chain of displacements:

```rust
pub fn storage<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<Storage<'h>> {
    let (mut v, mut start) = (v, 0);
    for _ in 0..MAX_CHAIN {
        if let Some((kind, _)) = simple(mc, v) {
            return Some(Storage { simple: v, start, kind });
        }
        if !is_header(mc, v) {
            return None;
        }
        start += mc.slot(v, OFFSET).as_fixnum().unwrap_or(0) as usize;
        v = mc.slot(v, DATA);
    }
    None
}
```

The chain has a bound because `adjust-array` can make a cycle, and a
primitive must stay short (rule 9). `aref` now takes any number of
subscripts, checks each against its dimension and computes the row-major
index; `length` answers a fill pointer when there is one; `vectorp` and
`stringp` look through a header too. `vectorp` also became true of strings
and bit vectors, which CLHS has always said they are and Quokka had not.

Everything else is Lisp, in `lisp/library/arrays.lisp`: `make-array` with
every keyword, `adjust-array`, `vector-push-extend`, `fill-pointer`,
`array-displacement`, the eleven bit operations, and the type predicates
built on one small primitive, `%array-kind`. The printer learned `#*1011`
for bit vectors and `#2A((1 2) (3 4))` for arrays of other ranks, and the
reader learned to read both back. Because the library's sequence functions
were all written over `aref` and `length` (chapter 45), `find`, `remove`,
`reverse` and the rest worked on strings with fill pointers the moment
those two did; only the functions that *make* a sequence needed telling that
a bit vector's copy is a bit vector.

## What did the measurement find?

ansi-test's driver loads our runner as a single form with one test's budget
of ten million steps, and it stopped loading. Nothing looped: the runner had
always needed just under ten million steps, and this step tipped it over. We
looked for the cost and found one: the library's `make-array` parsed eight
keywords on every call, which made a plain `(make-array 10)` twelve times
slower than the prelude's, and the sequence functions make plain vectors all
the time. It now goes straight to the primitive when it is given a size and
nothing else. The driver also gives a whole file a file's budget now, as
`quokka build` does.

`equal` and `equalp` gained bit vectors and arrays of every rank. The first
version used `dotimes`, which `lists.lisp` defines further down than
`equal`, so the compiler took `(dotimes ...)` for a call to a function and
the loop variable for an unbound variable; the loops are `labels` now, as
`equalp`'s already was.

The ratchet caught four tests that had stopped passing, `array-fill-7` to
`-10`. Each fills `(make-array '(5))` with a bad `:start` or `:end` and
expects a `type-error`. They had passed by accident: the old `make-array`
refused a list as its size, with a type error, before `fill` was reached.
Now the array was made, and `fill` complained with a plain error. SBCL
signals a `type-error` for a range that does not fit a sequence, so
`%bounds`, which every sequence function uses to check one, now does too.

An automated review of the new Rust found a hole we had not tested. `aref`
checked each subscript against the header's dimensions, and then read the
simple array at the end of the chain without asking whether that element
was there. A header's slots are Lisp's to write, and ordinary code can
leave one pointing past the end: displace an array onto an adjustable one,
then make the adjustable one smaller. Rust's vectors check every index, so
nothing in memory was overwritten, but the check that caught it was a
panic, and a panic ends the whole machine. Now the element's place in the
simple array is checked against that array's real length, and the program
gets an error it can handle, as SBCL signals one.

The runner's own comparison needed the same lesson as `equal`. ansi-test's
`equalp-with-case` compares arrays of any rank by their dimensions and their
elements; ours compared only vectors, so a bit operation on a 2-by-2 array
returned the right answer and failed the test.

## What did it cost?

This step took 345 lines of Rust, against our estimate of 150 to 250: the module
`array.rs` with its documentation, two object types, five primitives and the
changes to `aref`, `length` and the predicates, and `equal` hash tables
hashing a string with a fill pointer by its active characters. The core is
15,237 lines, so we raised the guide in `scripts/check-budget.sh` to 15,500
and wrote down why beside it. The Lisp is about 300 lines.
`tests/lisp/arrays.lisp` checks all of it against SBCL, leaving out the two
things CLHS lets implementations differ on: the value of
`array-rank-limit` (SBCL's is 129, ours 1024) and whether `adjust-array` of
an adjustable array returns the very same object, which SBCL's does not
always do.

ansi-test now measures fifteen groups, `arrays` among them, and 7,280 of
13,558 tests pass. In the fourteen groups measured before, passing tests went
from 5,701 to 6,651, and the failures with nothing to explain them from 725
to 378. The sequences group alone went from 1,741 passing to 2,480 and from
277 plain failures to 70; strings from 293 to 370. In the new group, 629
tests pass and none fails. Of the other 605, 584 need only deferred features
(`typep`, `loop`, CLOS, ratios), 20 need ansi-test's sample hash tables and
streams, which a later step adds, and one runs out of steps.

## What is still missing?

Quokka still has no specialised arrays other than those of characters and
bits: an array of `(unsigned-byte 8)` is upgraded to an array of anything,
which CLHS allows.
`array-element-type` answers `character` for every string, where SBCL tells
`base-char` from `character`. A structure made by `defstruct` is still a
vector underneath, so `vectorp` and `simple-vector-p` are true of it; that
waits for structure types. `adjust-array` can make a displacement cycle,
which `aref` refuses rather than follows, where CLHS calls the consequences
undefined.

---

**References.** The Common Lisp HyperSpec, chapter 15 (*Arrays*), and
15.1.2 on array upgrading; 2.4.8.4 and 2.4.8.12 on `#*` and `#nA`.
