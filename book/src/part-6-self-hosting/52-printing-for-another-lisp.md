# Printing for another Lisp to read

> *Where this fits.* Chapter 39 built the printer in Lisp, with the rule that
> what `prin1` writes, `read` reads back as the same object or an equal one.
> Chapter 38 built the reader. Both are steered by a family of special
> variables, `*print-base*`, `*print-escape*` and the like (chapter 35 is
> where special variables came from). This chapter is the third worked from
> ansi-test's list (chapter 49). It adds the variables Quokka was missing and
> the macro that sets them all at once, makes the printer able to print
> structure that refers to itself, and in doing that lets a hash table take a
> cons as a key, which chapter 19 had ruled out.

*Status: done, issue #64. Step Q of phase 2.*

## Why did 494 tests want one macro?

The biggest gap on ansi-test's list after chapter 51 was
`with-standard-io-syntax`, named by 494 tests, 392 of them printer tests.
The printer tests are written like this one:

```lisp
(deftest print.string.1
  (with-standard-io-syntax
   (write-to-string "" :escape nil :readably nil))
  "")
```

Whatever a test wants to check about printing, it first has to know what all
the other printer settings are, or its answer depends on what the previous
test left behind. `with-standard-io-syntax` is how a Common Lisp program says
that: it binds every variable that controls reading and printing to its
standard value, `*package*` to `COMMON-LISP-USER`, `*print-base*` and
`*read-base*` to 10, `*print-escape*` to true, and so on, twenty-one in all. The
standard says it is meant for writing data that any Lisp can read back.

The macro itself is one `let`. What it needed was the variables. Quokka had
twelve of the twenty-one, and when we wrote the printer in chapter 39 we had left
the others out on purpose. `printer.lisp` said so in a comment: a variable
that existed and was ignored would be worse than none.

## Should a variable exist before it does anything?

Six of the nine missing variables belong to features Quokka does not have:
`*print-pretty*`, `*print-lines*`, `*print-miser-width*`,
`*print-right-margin*` and `*print-pprint-dispatch*` belong to the **pretty
printer**, which lays long forms out over several lines and is tier 2, and
`*print-circle*` to printing shared structure, which we come to below. The
other three, `*print-readably*`, `*print-array*` and the reader's
`*read-base*`, were simply not written yet.

We changed our minds about the comment in `printer.lisp`. The argument for it
was that a program binding `*print-pretty*` to true and getting unpretty
output is misled. But the standard makes these variables part of every
program's world, and code that only binds or reads them, as
`with-standard-io-syntax` does, failed with an unbound-variable error while
printing exactly what Quokka would have printed anyway. So every one of them
now exists with its standard value. The pretty printer's are written down as
present and not yet consulted, and `*print-pprint-dispatch*`, which will hold
a table of printing functions, holds nil until there are any.

For `*print-circle*` we went further, and built what it means.

## What does it mean to print readably?

`*print-readably*` asks the printer for a promise: either it prints an object
so that `read` gives back an equal object, or it signals an error of type
`print-not-readable`. Most of what Quokka prints is readable already. What is
not is everything chapter 39 writes as `#<...>`, such as a package, a
stream, a function or a hash table, and printing one readably now signals.
The standard also says that printing readably implies escapes, gensym
prefixes and whole arrays, and ignores `*print-length*` and `*print-level*`,
since a list cut short with `...` reads back as something else. We bind
those in `write-to-string`, the one function every way of printing goes
through, so `prin1`, `format`'s `~S` and the REPL all keep the promise.

`princ` is the exception. CLHS defines it as `write` with both `*print-escape*`
and `*print-readably*` false, so that `(princ "x")` writes `x`, without
quotes, even inside `with-standard-io-syntax`. Our `princ-to-string` had only
turned escapes off, and the first test inside the macro showed it.

## How does the reader read in base 16?

`*read-base*` is the radix the reader reads integers in. In base 16, `ff` is
255 and `1e5` is the integer 485, not the float 100,000.0, because the
reader tries the token as an integer before it tries it as a float. A
trailing point always means decimal, so `10.` is ten in any base. A token
that is not digits in the base, like `fg`, is a symbol. The reader's integer
path already took a token one digit at a time, and now takes the radix as
well:

```lisp
(defun %token-integer (text &optional (base *read-base*))
  (let* ((length (length text))
         (start (%skip-sign text 0 length))
         (point (and (> length (+ start 1)) (eq (aref text (- length 1)) #\.)))
         (end (if point (- length 1) length))
         (radix (if point 10 base)))
    (if (and (< start end) (eq (%skip-digits text start end radix) end))
        (let ((value (%digits-value text start end 0 radix)))
          (if (eq (aref text 0) #\-) (- value) value))
        nil)))
```

The printer also asks `%token-integer`, to decide whether a symbol's name
would read back as a number and so needs bars around it. We measured what
SBCL does: with `*read-base*` 16, it prints the symbol `FF` without bars, and
with `*print-base*` 16, it prints `|FF|`. So the printer asks about base 10
and checks `*print-base*` separately, as it did before, and the reader alone
follows `*read-base*`.

## How do you print a list that contains itself?

Make a list of three numbers and point its last cons back at its first:

```lisp
(let ((cycle (list 1 2 3)))
  (setf (cdr (cddr cycle)) cycle)
  (write-to-string cycle :circle t))
```

Printed the ordinary way, this never ends: after `3` comes `1` again. With
`*print-circle*` true, the result is `"#1=(1 2 3 . #1#)"`. The `#1=` labels
the object about to be printed, and `#1#` stands for it afterwards, and the
reader understands both, so the printout reads back as a list with the same
cycle in it. The same notation shows sharing that is not circular:
`(#1=(1 2) #1# (1 2))` is a list whose first two elements are one list and
whose third is an equal but different one.

The printer does this in two passes. The first walks the object exactly as
the printer would and records every object it meets: once, or many times if
it meets it again. It does not walk into an object the second time, which is
why a cycle ends. The second pass prints, and the first time it reaches an
object recorded as many, it writes the next label before it. Which objects
can be shared we measured from SBCL: anything except a number, a character
or a symbol that has a home package. Two references to one string are
labelled, and so are two to one gensym, `(#1="ab" #1# #2=#:G #2#)`.

The first pass needs a table that answers "have I met *this* cons before?"
It must compare by identity, and it is the one place where Quokka could not
simply write the Lisp.

## Why couldn't a hash table hold a cons?

Chapter 19 made hash tables hash a key's contents. A symbol hashes by its
name, a number by its value, a string, in an `equal` table, by its
characters. A cons or a vector was refused as a key, because the only thing
that identifies one is its address, and the collector (chapter 6) moves
objects: an address-based hash is right until the next collection and
silently wrong afterwards. Issue #22 had weighed four ways round this and
chosen content hashing, and `hash.rs` wrote down that refusing a cons was a
divergence from Common Lisp, to be revisited when something needed it.
`*print-circle*` needed it.

We looked at three ways to give the first pass its table. A list searched
with `eq` costs no Rust but grows with the square of the object's size, so a
5,000-cons structure takes about twelve million comparisons. A table written
in Lisp, bucketed by a content hash and searched with `eq` within a bucket,
costs no Rust either and would have given us `sxhash`, but slows down again
for structures whose parts look alike, a thousand `(nil)`s landing in one
bucket. We chose the third, which was #22's third option too: hash tables
take any object as a key, hashed by its address, and the collector tells a
table when that has gone stale. That is what SBCL does, and it closes the
divergence rather than working round it.

A table now has a fourth slot saying how its keys were hashed: all by
content, some by address, or some by address and since moved. Storing a key
with no content to hash sets the second state. The collector, as it copies a
table, turns the second state into the third:

```rust
// Everything is moving, so a table that hashed a key by
// its address must rehash before its next use (hash.rs).
if header.object_type == ObjectType::HashTable {
    let stamp = scan + HEADER_WORDS + crate::hash::STAMP;
    to[stamp] = crate::hash::moved(to[stamp]);
}
```

and the next `gethash`, `setf gethash` or `remhash` on a stale table rehashes
every entry into the same number of buckets before it looks.

Our first plan was a counter instead: a table would remember how many
collections had happened when it last hashed, and rehash when the count had
moved on. It fails across images. Saving an image collects first (chapter
30), and a loaded heap starts counting from zero again, so a table stamped
"hashed at collection 0" in the process that saved it looks fresh in the
process that loads it, though every object in it has moved. Marking the table
during the move cannot be fooled that way, because the mark is in the heap
and is saved with everything else.

An `equal` table still refuses a cons, since `equal` compares conses by their
contents and an address cannot stand for that. Every other object is equal
only to itself, so an `equal` table takes it by address too. In an `eq` or
`eql` table even a string is hashed by address, since there two strings are
one key only if they are one object. Our first version hashed such a string
by address and forgot to set the table's stamp for it, so the collector
would never have marked the table stale; we found it while rewriting the
unit test that had asserted the old refusal, and a string key now has a test
of its own in both evaluators.

With that, the first pass is one `eq` table:

```lisp
(defun %note-shared (object table)
  (if (%shareable-p object)
      (if (gethash object table)
          (setf (gethash object table) :many)
          (progn
            (setf (gethash object table) :once)
            (cond ((consp object)
                   (%note-shared (car object) table)
                   (%note-shared (cdr object) table))
                  ((%printed-as-vector-p object)
                   (dotimes (i (length object))
                     (%note-shared (aref object i) table))))))))
```

The one subtle part of the second pass is a list's tail. In `(a . #1#)` the
label stands for the rest of the list, so when the printer is about to
continue a list with a cons that is shared, it writes ` . ` first and prints
the cons as an object of its own, instead of carrying on with its elements.

## What did it cost?

81 lines of Rust, against an estimate of 60, about half of them the
comments that say why: the stamp, the address hash and the rehash in
`hash.rs`, six lines in the collector, and the image format's version
number, now 4, since a hash table has a slot more than an older reader
expects. The core is now 14,892 lines of the 15,000 guide. The rest is Lisp in `printer.lisp` and `reader.lisp`.
The tier-0 file `tests/lisp/hash-tables.lisp` gained cases that store lists
and vectors as keys, make enough garbage to collect several times, and find
them again, in both evaluators. The new tier-1 file
`tests/lisp/standard-io-syntax.lisp` checks the macro, `*read-base*`,
printing readably and nine cases of shared and circular structure against SBCL.

We measured steps Q, R and S together (chapters 52 to 54). ansi-test went
from 5,382 passing tests to 5,701, of 12,324, and the tests needing a tier-1
feature from 1,752 to 882. The printer group went from 80 passing to 154 and
the reader group from 50 to 75. Most of the 494 tests that named
`with-standard-io-syntax` now name `loop`, `random` or the pretty printer
instead, all of them tier 2: explained, as phase 2's gate asks, though not
yet passing.

## What is still missing?

The pretty printer and its five variables are tier 2. A typed condition such
as `print-not-readable` carries the object as data but has no message a
person can read, until tier 2 gives conditions slots. `with-standard-io-syntax`
binds `*readtable*` to a fresh copy of the standard readtable each time,
where SBCL binds the standard one and refuses changes to it; the copy makes
a change harmless rather than an error. And `readtable-case`, which changes
how the reader folds case and how the printer writes symbols, is its own
step.

---

**References.** The Common Lisp HyperSpec, 22.1.3 (*Default print-object
methods*) and 22.1.1 (*Printer variables*), 2.3.1 on how the reader decides
a token is a number, 2.4.8.15 and 2.4.8.16 on `#n=` and `#n#`, and the
entries for `with-standard-io-syntax`, `*print-readably*`, `*print-circle*`
and `*read-base*`. Richard Jones, Antony Hosking and Eliot Moss, *The Garbage
Collection Handbook*, 2nd edition, 2023, on address-based hashing under a
moving collector.
