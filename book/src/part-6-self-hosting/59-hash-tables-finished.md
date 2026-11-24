# Hash tables, finished

> *Where this fits.* Chapter 19 built hash tables in Rust, keyed by what a
> key *is* rather than where it lives, because the collector moves objects
> (chapter 6). Chapter 52 let them take any key by its address too, with the
> collector marking a table stale when it moves one. Two kinds of key were
> still refused: a cons in an `equal` table, and every key of an `equalp`
> table, which did not exist. ansi-test (chapter 49) could not even load its
> list of sample objects because of it, and that list blocked 111 tests on
> its own. This chapter, the tenth worked from ansi-test's list, finishes
> the standard's chapter 18 on hash tables, and gives the standard streams a
> place to point at.

*Status: done, issue #71. Step X of phase 2.*

## Hashing and comparing are two jobs

A hash table does two things with a key. It turns the key into a number,
the **hash**, which picks a bucket, and it compares the key with the keys
already in that bucket, to find the one that is the same. For a symbol or a
number in an `eql` table both are easy, and our Rust code did both.

For a cons in an `equal` table they are not. `(gethash (list 1 2) table)`
must find the entry stored under another list `(1 2)`, a different object
with the same contents, so the hash has to come from the contents and the
comparison has to be `equal`, which walks both lists. An `equalp` table is
looser still: strings match ignoring case, `1` matches `1.0`, and vectors
and structures match element by element.

The issue first proposed doing both jobs in Rust, at about sixty lines. When
we read `hash.rs` again before deciding, we found that estimate was wrong.
The hash is the easy half. The comparison would be `equal` and `equalp`
written a second time, in Rust, beside the ones the library already has in
Lisp, and written recursively on the Rust stack, which Quokka avoids so that
the machine can always stop between steps (chapter 2). With every kind of
object `equalp` knows, that was nearer 150 lines, and two definitions of one
function that would have to be kept in step.

So we split the jobs. Rust computes the hash from the contents, and the
library compares with its own `equal` and `equalp`. The alternative, doing
everything in Lisp, would have meant two kinds of table everywhere a table
is used, and a hash computed in Lisp on every lookup, at a time when
chapter 58 had just shown us how close ansi-test runs to its step budget.

## A hash that stops

A hash computed from contents has to stop somewhere, or hashing a long list
costs as much as the list is long, and a Rust primitive must finish quickly
(rule 9). Ours looks four levels into conses and at the first four elements
of a vector, and stops there, as SBCL's does. Two keys that differ only
deeper down then share a hash and land in the same bucket, where the
comparison tells them apart; a hash only has to give equal keys equal
numbers, and it may give unequal keys the same one.

An object that has nothing but its identity, a function or a hash table,
gets a number for its type rather than for its address. That was a
deliberate choice: chapter 52 had to teach the collector to mark tables
stale because addresses change, and a hash that never looks at an address
never goes stale. For `equalp`, characters are hashed by their upper case,
numbers by their value so that `1` and `1.0d0` meet, and every vector, a
string included, by its length and its first elements, so that `"ab"` and
`#(#\A #\B)` meet as `equalp` says they should.

The same function is `sxhash`, which CLHS defines as a hash that agrees with
`equal`. It came for nothing once the content hash existed.

## Handing the call to Lisp

The prelude's `setf` turns `(setf (gethash k table) v)` into a call of the
primitive `%set-gethash`, and the compiler's own code is full of such forms.
Changing that expansion would change the bytes the compiler compiles itself
to, and the fixpoint (chapter 42), our check that the compiler reproduces
itself exactly, would rightly fail. So the primitives had to stay the way in,
and find a way to give the work to Lisp themselves.

Chapter 47 had already built that way for bignums. When `+` overflows, its
primitive fails, and the machine calls the Lisp function `%generic-+` with
the same arguments instead of reporting the error. We gave `%gethash`,
`%set-gethash` and `remhash` fallbacks of their own. When one of them meets a
key the library compares, it refuses, and the machine calls
`%generic-gethash` or its siblings, which ask Rust for the key's bucket and
search it:

```lisp
(defun %hash-entry (key table)
  (let ((test (if (eq (hash-table-test table) 'equalp) #'equalp #'equal)))
    (dolist (entry (%hash-bucket key table) nil)
      (if (funcall test (car entry) key) (return entry)))))

(defun %generic-set-gethash (key table value)
  (let ((entry (%hash-entry key table)))
    (if entry (setf (cdr entry) value) (%hash-insert key table value))
    value))
```

Every other key never leaves Rust, so the package system's string tables and
the compiler's symbol tables are exactly as fast as before.

## The smaller pieces

`make-hash-table` now takes its test as a function as well as a symbol;
`#'equal` is the function object in `equal`'s function cell, so comparing
objects tells which test was meant. `:size`, `:rehash-size` and
`:rehash-threshold` are accepted and reported, as the hints CLHS allows an
implementation to ignore: our tables start with eight buckets and double
when they hold more than one entry a bucket. `with-hash-table-iterator`
gives its body a local macro that returns the next key and value each time
it is called, from a list of the entries taken when the form starts.

The standard streams `*terminal-io*`, `*debug-io*`, `*query-io*`,
`*error-output*` and `*trace-output*` exist now, and each is the console
stream that `*standard-output*` already was; tier 1 has nowhere else to
write. Reading from the two-way ones waits for tier 2's streams. With them, a
stream designator of `t` means `*terminal-io*`, as CLHS says, except in
`format`, where `t` means `*standard-output*`, as CLHS also says.

## What the measurement found

With `universe.lsp` loading, ansi-test reached tests that had never run,
and one of them found a bug in the prelude. `(setf (gethash key table
default) value)` takes a default form, useless when storing but still to be
evaluated, in order, after the key and the table and before the value. Our
expansion dropped it. It now wraps the value, `(%set-gethash key table
(progn default value))`, which evaluates the four in the right order without
any temporaries. A `gethash` place without a default expands exactly as
before, which is all the compiler's code uses, so the fixpoint held.

One `sxhash` test still fails: SBCL counts an array whose element type is
`nil` as a string, so its empty one hashes like `""`, and ours does not.

## What did it cost?

ansi-test went from 7,757 passing tests to 7,807, of 13,558. The
hash-tables group went from 42 to 81 of 167, and data and control flow from
1,073 to 1,084, mostly tests that use a hash table on the way. With
`universe.lsp` loading, the tests needing a tier-1 feature fell from 195 to
22, almost all of them now waiting on reader syntax and on `defstruct`'s
`:include`. `tests/lisp/hash-tables-completed.lisp` checks `equalp` and
`equal` tables, growth, every form of the test argument, `sxhash`, the
iterator, the standard streams and the order `setf` evaluates a default in,
against SBCL.

This step cost 214 lines of Rust, three times our estimate of seventy. We
had counted the hash and the bucket primitive. We had not counted splitting
insertion and removal out of `put` and `remove` so that the library could
call them, the table's test becoming a type with three cases, the check
that decides which keys Lisp compares, or the comments that explain the
split. The Rust core is 15,468 lines of its 15,500-line guide, which leaves
little for the rest of the phase.

## What is still missing?

Hash tables are complete as far as CLHS 18 goes, apart from weak tables,
which are an extension. `hash-table-size` reports at least eight and at
least the count, since Rust does not report its bucket count. Reading from
`*terminal-io*` and `*query-io*` waits for two-way streams, in tier 2.

---

**References.** The Common Lisp HyperSpec, chapter 18 on hash tables, the
entries for `sxhash` and `equalp`, 5.1.1.1 on the order of evaluation in a
place, and 21.1.2 on the standard streams. Donald Knuth, *The Art of
Computer Programming*, volume 3, section 6.4, on hashing with chaining.
