# Changing classes

> *Where this fits.* Chapter 73 put a class's instances in the heap, each
> with a wrapper saying which slots its slot vector holds, and chapter 74
> made the instance protocol generic. Both assumed a class stays as it was
> defined. In a live system it does not: you change a `defclass` while
> instances of it are in use, or decide that an object should now be of
> another class. This chapter is how instances follow, and it is what lets
> a world that is never restarted keep its objects through a change of
> mind.

*Status: done, issue #124. Phase 3, step A, sub-step 5.*

## A class changes under its instances

Here is a point with two slots, and an instance of it:

```lisp
(defclass point () ((x :initarg :x) (y :initarg :y)))
(defvar *p* (make-instance 'point :x 1 :y 2))
```

Now we decide a point should have a colour too, and drop `y` while we are
at it, and evaluate a new `defclass`:

```lisp
(defclass point () ((x :initarg :x) (colour :initform :black)))
```

In a system that is restarted for every change, the old instance would be
gone and the question would never come up. In a world that lives on, as
Quokka's does, `*p*` is still there, with a slot vector of two places, `x`
and `y`, laid out by the old definition. Common Lisp's answer (CLHS
4.3.6) is that the instance is *updated*: it keeps its identity, every slot
the two definitions share keeps its value, `colour` is added and gets its
initform, and `y` is discarded. A program can watch and take part, because
the update calls a generic function,
`update-instance-for-redefined-class`, with the slots added, the slots
discarded, and the old values of the discarded ones:

```lisp
(defmethod update-instance-for-redefined-class :after
    ((p point) added discarded old-values &rest initargs)
  (when (member 'y discarded)
    (setf (slot-value p 'colour) (if (> (getf old-values 'y) 0) :red :black))))
```

## Obsolete wrappers

Chapter 73 already prepared most of this. An instance is a wrapper and a
slot vector, and the wrapper, which belongs to the class, lists the names
of the slots in the vector in order. When a redefinition changes those
names, the class gets a new wrapper, and the instances still pointing to
the old one are out of date. The question was when to bring them up to
date.

Going through every instance at the moment of the `defclass` would need a
list of them, and keeping such a list would keep every instance alive for
good. So CLOS does it lazily. The old wrapper is marked *obsolete*, and an
instance is brought up to date the next time one of its slots is touched;
`slot-value`, every accessor, `with-slots` and `describe` all go through
`%ensure-current-instance`, which compares the instance's wrapper with its
class's current one. CLHS lets an implementation do the update earlier.
SBCL also does it whenever the instance is passed to a generic function,
which we measured; we chose to do it only when a slot is touched, since
the check on every generic function call would cost every call something
and change nothing a program could rely on.

The obsolete wrapper also keeps one thing the update needs. A slot can be
*shared*, held once by the class for all its instances, or *local*, one in
each instance's vector. A redefinition can move a slot from one kind to the
other, and CLHS says a slot that was shared and is now local keeps the
shared value. By the time an instance is updated, the class no longer has
the shared slot, so when a wrapper becomes obsolete we record the old
class's shared slots in it:

```lisp
(defun %obsolete-wrapper (wrapper class)
  (setf (aref wrapper 2) (cons :obsolete (%class-shared-cells class))))
```

An instance that is not touched through two redefinitions is updated once,
from its own old wrapper to the class's current one, with the slots added
and discarded between those two.

`make-instances-obsolete` does the same without a redefinition: the class
gets a new wrapper with the same slots, and every instance runs the update
the next time it is touched, with nothing discarded and, as the next
section explains, its unbound slots counted as added. A program
calls it when it has changed what its update methods do.

## What we matched in SBCL

We measured the details in SBCL before writing any of this, and three of
them were not what we expected. A redefinition that keeps the same local
slots, changing only an initform, makes nothing obsolete, so no update
method runs; that suited us, because the warm rebuild (chapter 41)
redefines every library class and must leave every instance in the world
as it was. SBCL lists among the discarded slots only those that had a
value, though the CLHS text reads as if it meant every slot removed, and
we followed SBCL. And SBCL evaluates a shared slot's initform when the
`defclass` is evaluated, not when the first instance is made, which is
when Quokka used to do it; a shared slot added by a redefinition was then
unbound until something made an instance. Now `%finalize-class` fills
every unbound shared slot as the class is defined.

## What the review found

A review of the finished work, again probing Quokka and SBCL side by side,
found three faults. The first was older than this step but belonged to
it. A redefinition can name a superclass that is not defined yet, which
leaves the class unfinalized, and the update skipped any instance of an
unfinalized class. But the class still had the slots and the wrapper of
its last finalization, so such an instance was read through a layout that
was not its own, and two slots that had swapped places in between read
each other's values. Now the instance is brought up to the layout the
class was last finalized with.

The second was a difference from SBCL we had not measured. We had
expected *added* to mean the slots new to the class, and wrote that.
SBCL counts every local slot that is still unbound after the copy, a slot
the instance kept but never set included, so that slot gets its new
initform; CLHS 4.3.6.1 says it should stay unbound. SBCL also gives all
three lists in reverse slot order. Our tests had used one slot at a time
and could see neither. We followed SBCL, as we did for the discarded
slots.

The third concerned errors. The update installs the new layout and then
calls the generic function, and if a program's method signalled an error,
the instance was left with the new layout and its added slots never
filled, for good, since nothing would try again. SBCL undoes the change,
so the next touch retries, and in a world you never restart, fixing the
method and carrying on is the normal way to work. Both the update and
`change-class` now put the old wrapper and slot vector back when the
generic function does not return normally.

Three smaller findings were fixed after the step had been merged. A
shared slot removed by one redefinition and added again by a later one
came back with its old value, because the class kept the old cell; now a
redefinition forgets the cells of shared slots the class no longer
defines. `change-class` accepted a keyword declared by an
`update-instance-for-different-class` method on some other previous
class; now a method counts only if it applies to both instances. And the
copy of a funcallable instance as it was is funcallable too, calling what
the instance calls. SBCL 2.6.10 refuses both of the last two cases and
fails inside its own error reporting, so those tests hold Quokka to CLHS
alone.

## `change-class`

The other way a class changes under an instance is on purpose:

```lisp
(change-class *p* 'circle :radius 3)
```

`*p*` stays the same object, `eq` to what it was, and becomes a circle.
Slots both classes have keep their values, the new ones get initargs or
initforms, and the generic function `update-instance-for-different-class`
is called with two instances: the changed one, and a copy of it as it was
before, of the old class and with the old values, which a method can read
from:

```lisp
(defun %change-class (instance new-class initargs)
  (%ensure-current-instance instance)
  ...
  (let* (...
         (previous (%allocate-standard-instance old-wrapper (copy-seq old-slots)))
         ...)
    ...
    (%set-instance-slots instance new-slots)
    (%set-instance-wrapper instance new-wrapper)
    (apply #'update-instance-for-different-class previous instance initargs)
    instance))
```

The instance is brought up to date first, in case its own class was
redefined since it was last touched. This works only because the slot
vector is a separate object from the instance (chapter 73): the instance
keeps its place in the heap and its identity, and only its wrapper and
vector are replaced. A standard instance cannot become a funcallable one,
or the reverse, because those are different kinds of object in the heap;
SBCL refuses that too.

## What did it cost?

No Rust: about a hundred and fifty lines of Lisp in `clos.lisp` and the
generic functions in `generic-functions.lisp`. A new differential file,
`redefinition.lisp`, has about seventy forms checked against SBCL, written
so that they do not depend on when an instance is updated, since that is
where SBCL and Quokka are allowed to differ. The rebuild test now also
redefines a class before the rebuild and touches its instance only after
it.
