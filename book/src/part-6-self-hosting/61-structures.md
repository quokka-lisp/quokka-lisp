# Structures of their own

> *Where this fits.* Chapter 21 built `defstruct` for tier 0: one line that
> writes a constructor, a predicate, a copier and an accessor per slot. It
> made each record a vector with the type's name in its first element,
> which was all a compiler needed. Common Lisp asks for much more, and
> ansi-test (chapter 49) counted 58 tests that failed for want of it. This
> chapter gives a structure a heap object of its own, which is the first
> new kind of object since the arrays of chapter 55, and then writes the
> whole of `defstruct` in the library on top of it. The same kind of object
> is what a class instance will be when CLOS arrives in tier 2.

*Status: done, issue #73. Step Z of phase 2.*

## What was wrong with a vector

Tier 0's `(defstruct point x y)` made `(make-point :x 1 :y 2)` into the
vector `#(POINT 1 2)`. Every function that works on vectors therefore
worked on points:

```lisp
(vectorp (make-point))      ; => T, and NIL in SBCL
(length (make-point))       ; => 3, and an error in SBCL
(aref (make-point :x 1) 1)  ; => 1
```

and the printer wrote a point as a vector. Chapter 21 admitted this and
called the predicate "generous": `point-p` accepted any vector whose first
element was the symbol `POINT`. For the compiler none of this mattered,
because the compiler never asks whether a record is a vector.

Common Lisp's `defstruct` is a type definition. A structure is not a
vector, a sequence or an array, it prints as `#S(POINT :X 1 :Y 2)` so that
`#S` can read it back, and with `:include` one structure type can extend
another, so that a `point3d` is also a `point`. That last requirement is
the one a name in slot 0 cannot meet. `point-p` must now accept every type
that includes `point`, so whatever sits in the first slot has to lead to a
description of the type: its name, its slots, and the type it includes.

## Two ways to build it

We could have kept the vector and put a description of the type in slot 0
in place of the bare name. That costs no Rust at all. The printer would
then recognise a vector whose first element is a description and write it
as `#S(...)`. But `vectorp`, `length`, `aref` and every sequence function
would keep accepting a structure, and each of them would need a test added
to refuse one: the representation would keep saying that a structure is a
vector, and a dozen functions would have to contradict it.

The other way is a heap object with a type of its own in its header
(chapter 5), so that the functions that check for a vector refuse a
structure without being told anything about structures. The machine already
had a header type meant for exactly this, `ObjectType::Instance`, described
in the source as "an instance of a class". Only one thing used it: the
**markers** that `block`, `catch` and `tagbody` make, the objects with no
slots whose only job is to be equal to nothing but themselves (chapter 14).
They had been instances simply because a marker needs some type and that one
was free.

We chose the heap object. Chapter 20 had already recorded what happens when
two things share a type: conditions were once instances as well, every
marker looked like a condition, and a `return-from` passing through a
`handler-case` was caught by it. So the markers moved first, to a type of
their own, `ObjectType::Marker`, and `Instance` now means a structure and
nothing else. When tier 2 adds classes, an instance of a class will be the
same object, a description of its class followed by its slots. This is the
shape PCL, the CLOS implementation most Lisps descend from, gives its
instances, so the work is done once.

The cost was Rust, which the budget measures (rule 7). We had 32 lines left
of 15,500. The step needed 69: the new header type, four primitives, one
line in the Rust printer and the library file's entry in the build. We
raised the limit to 15,537, the second deliberate raise after chapter 55's.

## Four primitives and a layout

The Rust side is small because it knows almost nothing. A structure is an
`Instance` whose slot 0 holds its **layout**, the Lisp object that
describes its type, followed by its slots. Four primitives reach it:

```lisp
(%make-structure layout 2)        ; a structure of two slots, both nil
(%structure-layout object)        ; its layout, or nil for anything else
(%structure-ref object 0)         ; the first slot
(%set-structure-ref object 0 'x)  ; written
```

`%structure-layout` answers nil for anything that is not a structure, so it
is the test for being one as well as the way to the layout. The two slot
primitives count from 0, refuse an index outside the structure, and refuse
an object that is not a structure. What a layout contains is none of Rust's
business; it is an ordinary Lisp value that the machine stores and hands
back.

In `lisp/library/structures.lisp` a layout is a simple vector of thirteen
elements: the type's name, its slot descriptions, the layout of the type it
includes, its printing function, its constructors, its `:type`, and a few
more. Writing it as a vector looks like a step backwards, but it is exactly
what

```lisp
(defstruct (%layout (:type vector)) name slots include printer ...)
```

would produce, since `:type vector` asks for a structure that *is* a vector.
The file cannot say that, because it is the file that defines `defstruct`,
so it spells the thirteen accessors out by hand.

A layout is found by name, under the property `%layout` of the type's name.
Redefining a structure updates its layout in place rather than making a new
one. Common Lisp leaves redefinition undefined, but we need it to work: the
warm rebuild (chapter 41), in which a running world recompiles itself from
its source, loads `packages.lisp` again while the packages it defines are in
use. Each of those packages is a structure pointing at the `%package`
layout, and the reloaded `defstruct` must leave them pointing at a layout
that still describes them.

## The predicate and the accessors

With the layout in hand, the predicate walks the chain of included types:

```lisp
(defun %structure-is-a (object layout)
  (do ((l (%structure-layout object) (%layout-include l)))
      ((or (null l) (eq l layout)) (if l t nil))))
```

A `point3d` whose layout includes `point`'s passes `point-p` on the second
step; a list, whose layout is nil, fails on the first.

An accessor must accept the same objects and refuse everything else with a
`type-error`, as SBCL's does. Walking the chain on every slot access would
be slow, and some accessors are called very often: the reader calls the
string stream's position accessor once per character, and ansi-test's step
budget (chapter 58) already measures that loop. So each accessor first
compares the object's layout with its own, the common case, and walks the
chain only when that fails:

```lisp
(defun point-x (object)
  (%structure-ref (if (eq (%structure-layout object) layout)
                      object
                      (%check-structure object layout))
                  0))
```

This was not enough on the first attempt. One of the system tests reads
every file of the system with the Lisp reader and prints every form with
the Lisp printer, inside a budget of five million steps a form. The longest
form is the list of all 978 names in the `COMMON-LISP` package, about
15,000 characters of strings, and it no longer fitted. The printer wrote a
string one character at a time, and every write asks the stream whether it
is the console, which for a string stream is a predicate that misses: a
call, a walk up the chain of included types, and a read of each layout
through `svref`, a library function with checks of its own. We made the
predicates compare the layout with `eq` before walking, read layouts with
the `aref` primitive, and changed the printer to write each run of
characters up to the next one that needs a backslash in a single write.
After that the list fitted again, and printing a long string costs fewer
writes than it did before this step.

The `layout` in that definition is a variable. `defstruct` expands into a
`let` that binds it to the layout, around all the `defun`s it writes, so
every function the definition makes is a closure over the same object, and
none of them looks anything up by name when it runs.

## Options

The rest of CLHS 8 is options, and each one is a few lines of the parser
`%parse-defstruct`. `:conc-name` changes the prefix of the accessors' names,
and `(:conc-name nil)` removes it. `:predicate` and `:copier` rename those
two functions or, given nil, leave them out. `:include` takes the parent's
slots first, and may change an inherited slot's initial value. An accessor
the parent already defines under the same name is not written again, since
the parent's accessor accepts the child already. `:print-function` and
`:print-object` name a function to print the structure with.

`:constructor` is the option with the most in it. A structure may have
several constructors, and a constructor given a lambda list is a **BOA
constructor**, "by order of arguments" (CLHS 3.4.6): its parameters are
slot names, taken in order rather than as keywords.

```lisp
(defstruct (boa (:constructor boa-con (a &optional b &aux (c 7))))
  (a 1) (b 2) (c 3) (e 5))

(boa-con 9)   ; => #S(BOA :A 9 :B 2 :C 7 :E 5)
```

The rule is that an optional parameter which names a slot and gives no
default takes the slot's initial value, so `b` is 2 here. A slot the lambda
list does not mention, such as `e`, gets its initial value too, and an
`&aux` variable sets its slot to whatever the variable says. The issue
listed this as a decision, how to turn such a lambda list into code. It
turned out not to be one, because CLHS fixes the result.
`%boa-lambda-list` rewrites the list by adding the slot's initial value as
the default where a parameter has none, and the constructor's body puts each
slot's variable, or its initial value, into the new structure.

`:print-object` was also listed as a decision, since in a Lisp with CLOS it
defines a method of the generic function `print-object`, and Quokka has
neither yet. But the option names an ordinary function of two arguments,
the object and the stream, and the method `defstruct` would define does
nothing except call it. Calling the function directly behaves the same, so
we did that, and both printing options are inherited by an including type
that has none of its own.

## Structures that are lists

`(:type list)` and `(:type vector)` ask for something different: a
structure that *is* a list or a vector, with accessors that are `nth` and
`aref` at fixed positions. Nothing about these needs a new object; they are
the old tier-0 representation, made an explicit choice.

```lisp
(defstruct (lp (:type list) :named) x)
(defstruct (lp2 (:type list) :named (:include lp)) y)

(make-lp2 :x 1 :y 2)   ; => (LP 1 LP2 2)
```

`:named` puts the type's name into the list, which is what makes a
predicate possible, and an included type keeps its own name in its own
place. So `(lp-p (make-lp2))` is true, because position 0 still holds `LP`.
We expected `(LP2 1 2)` until we asked SBCL. `:initial-offset` leaves
that many unused elements before the type's own.

## Printing and reading back

The printer writes a structure as `#S(` followed by its type's name and
each slot's name and value. The slot names are keywords, and SBCL writes
their colons even under `princ`, which prints most keywords without one.
`#S` counts as one level of nesting for `*print-level*` and each slot as one
element for `*print-length*`, and with `*print-circle*` the slots are walked
for shared structure like a list's elements:

```lisp
(let ((*print-circle* t) (x (make-place)))
  (setf (place-x x) x)
  (prin1-to-string x))
;; => "#1=#S(PLACE :X #1# :Y 2)"
```

The reader's `#S`, which until now called `make-NAME` and hoped it existed,
asks the layout for the type's first keyword constructor. `equalp` descends
into two structures of the same type slot by slot, and `copy-structure`
copies one. An `equalp` hash table needed nothing new: chapter 59 already
sends every `equalp` key to the library to compare, and the Rust hash gives
every structure the same number, which is correct, if slow for a table full
of them.

## The library's own structures

Five structures in the library are not data anyone reads back: the string
input and output streams, the console stream, packages and the readtable. They are defined
in files that load after `structures.lisp`, so they are now structures too,
and the printer's special cases for them stayed, since SBCL writes a package
as `#<PACKAGE "CL">`, not as `#S(...)`. What went away was the test that
had stopped the printer from writing these five as vectors.

## What did it cost?

The Rust came to 69 lines, against an estimate of 60 to 90, and the limit
went from 15,500 to 15,537. The library gained `structures.lisp`, about 380
lines, and the printer and reader about 40 between them. The image format
went from 4 to 5, because a structure saved by an older world is a vector
and would mean something else. A new test file, `defstruct-options.lisp`,
agrees with SBCL.

ansi-test went from 7,851 tests passing to 7,891, with none of the old ones
lost. The tests needing a tier-1 feature went from 8 to none, and the plain
failures from 311 to 271. Two tests that had passed before came close to
being lost: they expect an error for option combinations CLHS forbids, and
the old `defstruct` had refused every option list, so they passed by
accident. The new parser checks both combinations, a predicate for an
unnamed list or vector and a name in a vector that cannot hold a symbol, so
they pass on purpose now.


## What is still missing?

`typep` and `type-of` do not know structure types, because Quokka has no
type system yet (phase 3), and most of the remaining tests in ansi-test's
`structures` group ask `typep` about a structure. The twelve it still
counts as plain failures are not failures of `defstruct`: their structures
are defined by a helper in the test suite that is written with `loop`
(tier 2), so the definitions never load. A slot's `:type` is recorded and never checked, as SBCL checks it
only at some safety settings. Redefining a structure with different slots
leaves structures made before the change describing the old slots; CLHS
leaves this undefined, and SBCL offers a restart, which needs the restarts
of phase 3.
