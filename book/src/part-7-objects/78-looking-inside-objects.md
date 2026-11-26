# Looking inside objects

> *Where this fits.* Chapters 72 to 77 built CLOS: types, classes,
> generic functions, printing, redefinition and method combination. Each
> of them made the parts of the object system into objects themselves, a
> class an instance of `standard-class`, a method an instance of
> `standard-method`, but only Quokka's own code could read them. This
> chapter gives a program the same view, through the *metaobject
> protocol*, and then measures the whole of CLOS against the part of
> ansi-test written for it. It is the last chapter of CLOS; the inspector
> and the debugger of later phases are built on what it adds.

*Status: done, issue #124. Phase 3, step A, sub-step 7.*

## Asking a class about itself

A program that has defined a class can ask it what it is made of:

```lisp
(defclass point () ((x :initarg :x :reader point-x) (y :initarg :y)))
(defclass coloured-point (point) ((colour :initform :black)))

(class-precedence-list (find-class 'coloured-point))
;; => (#<STANDARD-CLASS COLOURED-POINT> #<STANDARD-CLASS POINT>
;;     #<STANDARD-CLASS STANDARD-OBJECT> #<BUILT-IN-CLASS T>)

(mapcar #'slot-definition-name (class-slots (find-class 'coloured-point)))
;; => (X Y COLOUR)

(slot-definition-readers (first (class-direct-slots (find-class 'point))))
;; => (POINT-X)
```

The same questions can be put to a generic function (its methods, its
lambda list, its method combination), to a method (its specializers and
qualifiers), and to a slot definition (its initargs, its initform, where
in an instance its value is kept). None of this is in the Common Lisp
standard, which defines classes and generic functions but not how to look
inside them. It comes from *The Art of the Metaobject Protocol* (Kiczales,
des Rivières and Bobrow, 1991), whose names every CLOS since has adopted.
An inspector needs it to show an object's slots without knowing its class
in advance, and so does a debugger that shows what a generic function
would do with the arguments in a broken frame.

The *metaobject protocol*, the MOP, has two halves. The *introspective*
half is what we have just seen: reading. The *intercessory* half lets a
program change how the system works, with a metaclass of its own whose
instances keep their slots in a database, or a generic function that
chooses its methods by some rule of its own. We decided when CLOS was
planned (`docs/clos.md`) to build only the first. The second is large,
and nothing Quokka needs before the inspector depends on it.

## A package of its own

The names of the MOP are not Common Lisp's, and rule 6 says that what is
not Common Lisp's must not be in the `COMMON-LISP` package. SBCL puts them
in a package called `SB-MOP`; we put them in `QUOKKA-MOP`. A program that
wants them without a prefix uses the package, and a test that runs on both
Lisps does it this way:

```lisp
(defpackage :mop-user (:use :common-lisp #+sbcl :sb-mop #+quokka :quokka-mop))
```

`#+sbcl` and `#+quokka` are *reader conditionals*: the reader keeps the
form after one only when the name is among the Lisp's `*features*`, so
SBCL reads `:sb-mop` and Quokka reads `:quokka-mop`.

Making the package meant going back to how symbols get a package at all
(chapter 36). The cold build reads every file with the Rust reader, which
knows nothing of packages and puts every name in one table; the last form
of the library then *adopts* every symbol in that table into a package,
the standard's 978 names into `COMMON-LISP` and everything else into
`QUOKKA`. The MOP's names are now adopted into `QUOKKA-MOP` before the
rest, and `QUOKKA` uses `QUOKKA-MOP`, so the library goes on reading
`class-slots` without a prefix, and a warm rebuild (chapter 41), which
reads the source again in package `QUOKKA`, finds the same symbols.

The readers themselves are short. Each checks that it was given the kind
of object it reads, then calls the function Quokka's own code has used
since chapter 73, whose name starts with `%`:

```lisp
(defun class-precedence-list (class)
  (%check-metaobject class 'class 'class-precedence-list)
  (%class-precedence-list class))
```

The check was added after review. A `%` reader only indexes a slot vector,
so without it `(class-precedence-list #'print-object)` answered with
whatever a generic function keeps in that place, and an inspector, which
passes these readers whatever it is shown, would have shown nonsense
instead of reporting an error, as SBCL does.

AMOP makes every reader a generic function, so that a program's own
metaclass could add methods to them. Without the intercessory half there
is no such metaclass, and a method on a reader could change only what the
system says about itself, never what it does; so we kept them ordinary
functions.

A structure class had no slot definitions at all: a structure's slots are
described by its layout (chapter 61), not by CLOS. The review found that
`class-slots` gave nil for one, and now the slot definitions are made from
the layout the first time they are asked for, and kept until a
redefinition of the structure replaces them.

## A table that broke the fixpoint

One reader needed something Quokka did not keep: `specializer-direct-methods`,
which asks a class for every method that has it among its specializers.
Our first version kept a hash table from specializer to methods, updated
whenever a method is added or removed. Every differential test passed, and
one test of the warm rebuild failed. The rebuild test compares C2, the
world rebuilt by C1, with C3, the world rebuilt by C2, word for word
(chapter 42); they must be identical, and now every symbol's name sat
somewhere else in C3 than in C2.

The table was an `eq` table, which hashes an object by its address
(chapter 52). A collection moves objects, so after one the table is marked
stale and rehashed on its next use, and rehashing walks the old buckets in
the order of the old addresses, consing as it goes. C1 and C2 had their
objects at different addresses, so the two rebuilds consed in different
orders, and everything allocated after that landed differently. The other
`eq` tables in the library never met this: they are empty or untouched
while a world rebuilds.

The fix was to keep each specializer's methods with the specializer, a
class on its property list and an `eql` specializer in a slot of its own,
which is also where PCL keeps them. Nothing then depends on an address.
The lesson is worth keeping: anything the rebuild runs must allocate in an
order that depends only on what it does, never on where things are.

## Measuring CLOS

ansi-test has a group for CLOS, `objects`, which we had not measured
while CLOS was being built, since most of it would have failed for want of
the parts not yet written. Run now, it found 27 tests that fail and 21 we
cannot read at all; the 21 use complex numbers, written `#C(1 2)`, which
Quokka defers. Of the 27, four fail in SBCL too, and the rest had causes
we fixed.

The largest was the number of arguments. Calling a reader with two
arguments, `(point-x p 1)`, must signal a `program-error`, and Quokka
called the method and ignored the extra one. We now check, around each
cached effective method, that a call gives no more arguments than the
generic function's lambda list takes, which costs nothing for a generic
function that accepts `&rest` or `&key`. Measuring this in SBCL turned up
something odd: a writer called with an extra argument reported
`no-applicable-method` the first time, before any call had been cached,
and `program-error` afterwards. The standard asks for the second, and we
give it every time.

The others were smaller:

1. `class-name` and its `setf` function are generic functions in the
   standard, and a program may define methods on them; they were ordinary
   functions.
2. A class's name can be changed with `(setf class-name)`, after which the
   class no longer has a *proper name* (CLHS 4.3.1): its name and what
   `find-class` finds for that name no longer agree. A `defclass` of the
   old name must then make a new class, and Quokka redefined the old one.
3. `defgeneric` and `defclass` options that may be given once, and a
   default initarg given twice, are errors, and were accepted.
4. `slot-value` returns only the first value of `slot-unbound` when a slot
   has no value, and `slot-boundp` asked `slot-missing` and then ignored
   its answer.
5. `make-load-form`, which a file compiler asks how to write an object
   into a compiled file, did not exist. Quokka has no file compiler yet,
   but a program may define methods and call it, so it is now a generic
   function with the standard's default methods, and
   `make-load-form-saving-slots` writes the two forms SBCL writes for an
   instance (for a structure SBCL's use its internal functions, and ours
   use `allocate-instance` and `slot-value`).

The four that SBCL fails too are listed in `tests/ansi/deferred.lisp`
with what SBCL does. The standard says, for instance, that
`call-next-method` given arguments for which other methods would apply
should signal an error, and SBCL does not check; neither do we.

## What did it cost?

No Rust. The readers are a new file, `mop.lisp`, and the package a few
lines of `packages.lisp`. Two new differential files check them against
SBCL: `mop.lisp` asks classes, slot definitions, generic functions and
methods about themselves, and `class-details.lisp` holds what the
`objects` group found. ansi-test now measures seventeen groups,
16,829 tests, of which 10,773 pass; in `objects`, 739 of 843. In every
group each test that does not pass is explained by a feature deferred to a
later step or by SBCL failing it too.

## What is still missing?

The intercessory half of the MOP, by decision. Conditions are still not
instances of their classes: `define-condition`, `make-condition`,
`handler-bind`, `signal` and restarts are the next step of phase 3, which
builds them on the classes CLOS now provides.
