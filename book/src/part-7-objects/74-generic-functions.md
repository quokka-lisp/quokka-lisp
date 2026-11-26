# Generic functions

> *Where this fits.* Chapter 73 gave every object a class and let a
> program define its own with `defclass`. A class on its own only
> describes data. This chapter adds the half of CLOS that chooses code by
> class: generic functions and their methods. With it, the system can be
> extended by adding a method for a new kind of object instead of editing
> a function that lists every kind it knows. The printer, the debugger and
> the inspector in later chapters are all extended this way.

*Status: done, issue #124. Phase 3, step A, sub-step 3.*

## Choosing code by class

Suppose a program draws shapes, and each kind of shape has its area
computed differently. Without generic functions, one function holds every
case:

```lisp
(defun area (s)
  (typecase s
    (circle (* pi (r s) (r s)))
    (square (* (side s) (side s)))))
```

Adding a triangle means editing `area`, and the next function that cares
about the kind of shape, and every one after it. A generic function splits
the cases into *methods*, which can be defined anywhere, by anyone:

```lisp
(defgeneric area (shape))
(defmethod area ((s circle)) (* pi (r s) (r s)))
(defmethod area ((s square)) (* (side s) (side s)))
```

The parameter list `((s circle))` says that this method applies when its
argument is a circle. `circle` is the method's *specializer*. When `area`
is called, the system finds the methods whose specializers its arguments
satisfy and runs the most specific one. A method may specialize on every
required argument, not only the first, which is what CLOS calls *multiple
dispatch*: `(defmethod meet ((a circle) (b square)) ...)` runs only for a
circle meeting a square. Most object systems, including Smalltalk's, Java's
and Python's, choose code by the first argument alone, the receiver of a
message.

A specializer can also be `(eql 7)`, which applies only to the object 7,
and a method can be on a built-in class such as `integer` or `string`, on
a structure type, or on `t`, which every object belongs to.

## When several methods apply

Usually more than one method applies. A small circle is a circle and a
shape, so methods on all three apply to it. CLOS orders them from most to
least specific (CLHS 7.6.6), comparing their specializers one argument at a
time. An `eql` specializer is the most specific. Of two classes, the one
that comes first in the argument's class precedence list (chapter 73)
wins:

```lisp
(defun %more-specific-p (a b args order)
  (dolist (i order nil)
    (let ((sa (nth i (%method-specializers a)))
          (sb (nth i (%method-specializers b))))
      (unless (eq sa sb)
        (return (cond ((%eql-specializer-p sa) t)
                      ((%eql-specializer-p sb) nil)
                      (t (let ((cpl (%class-precedence-list (class-of (nth i args)))))
                           (< (position sa cpl) (position sb cpl))))))))))
```

ORDER is normally the arguments from left to right, so the first argument
decides before the second. `defgeneric`'s `:argument-precedence-order`
option changes it.

Running only the most specific method would waste the others, so the
ordered methods are combined into one *effective method*. In *standard
method combination* (CLHS 7.6.6.2) a method without a qualifier is a
*primary* method, and the most specific primary method runs. It can pass
control to the next one in the order by calling `call-next-method`,
perhaps with new arguments, and `next-method-p` says whether there is a
next one. Methods qualified `:before` and `:after` run before and after
the primary methods, every one of them, for their side effects; their
values are thrown away. An `:around` method wraps all of it and decides by
calling `call-next-method` whether the rest runs at all. This is how an
`:after` method on `initialize-instance` can fill in a slot computed from
the others, without touching the code that made the instance.

## Building the effective method

A method's code is a function of two arguments: the list of arguments it
was called with, and the effective method of the methods after it, which
`call-next-method` calls. An effective method is a function of the
argument list. So a chain of primary methods is built by giving each
method the rest of the chain as its next:

```lisp
(defun %method-chain (methods last)
  (if (null methods)
      last
      (let ((function (%method-function (car methods)))
            (next (%method-chain (cdr methods) last)))
        (lambda (args) (funcall function args next)))))
```

The around methods are a chain in the same way, ending in a function that
runs the `:before` methods, the primary chain and the `:after` methods. So
the whole effective method is a handful of closures, made once.

We could have built it as a Lisp form and compiled it, as SBCL does. The
compiled form would run faster. But it would mean calling the compiler
from inside a function call, and the compiler's state is shared by the
whole world until phase 3 makes it per-process. Closures needed none of
that, and the compiler is called only when `defmethod` itself is compiled.

`call-next-method` and `next-method-p` are local functions that
`defmethod` wraps around the method's body, closing over the arguments
and the next effective method. A closure that outlives the method, such as
one the method returns, can still call `call-next-method` afterwards, and
the standard allows it. `defmethod` makes the two functions only for a
body that mentions them, since a closure costs an allocation on every
call.

## The discriminating function and its cache

A generic function is the funcallable instance of chapter 73. Its function
is a *discriminating function*, the same small closure for every generic
function, which hands the arguments to `%dispatch`. Finding the applicable
methods and sorting them is the slow part of a call, and it depends only
on the classes of the required arguments. So `%dispatch` makes a list of
those classes and looks it up in the generic function's cache. On a miss
it computes the effective method and keeps it under that key. This is the
idea of AMOP §1.8 and of PCL's caches, in its plainest form: a list of
entries searched in order.

An argument with an `eql` specializer needs more care, because two objects
of the same class may need different methods: 7 and 8 are both fixnums,
but only 7 matches `(eql 7)`. Where some method has an `eql` specializer
for an argument, an argument equal to one of those objects is keyed by its
specializer rather than by its class.

A cache is right only as long as the methods and the classes stay as they
were. Adding or removing a method changes which methods apply, and
redefining a class can change a precedence list. So each generic function
remembers the *epoch* its cache was filled in, and a single global epoch
changes whenever a class is defined or a method added or removed; a call
that finds its generic function's epoch out of date empties the cache
first.

Our first epoch was a number, incremented each time. It broke the test
that a world rebuilt twice is the same world word for word (chapter 42):
the counter went on counting through each rebuild, so the second rebuild
held a larger number than the first. Resetting it at each rebuild would
have been worse, since a generic function whose cache dated from an old
epoch could then meet a new epoch with the same number. Now each epoch is
a fresh cons compared with `eq`, which no stale cache can ever match, and
which a rebuild allocates at the same place every generation.

## The rest of CLOS becomes generic

Chapter 73 made `make-instance`, `initialize-instance`,
`shared-initialize` and the slot accessors ordinary functions, because a
generic function is itself an instance and could not exist before classes
did. Now they are generic, with standard methods that do what the
ordinary functions did. The standard methods call each other through the
generic functions (`make-instance`'s calls `initialize-instance`, which
calls `shared-initialize`), so a program's methods on any of them run.
The workers in `clos.lisp` that do the real work have names beginning with
`%`, and `clos.lisp` defines no function with a public name.

That last rule came from the warm rebuild. It reloads `clos.lisp` before
`generic-functions.lisp`, and if `clos.lisp` defined an ordinary
`make-instance`, each rebuild would first replace the generic function
with that ordinary function, and `generic-functions.lisp` would then make
a new generic function. Every method a program had added to
`make-instance` would be lost. The rebuild test now defines a generic
function and a method before rebuilding and checks afterwards that
`#'make-instance` is the same object and the method still applies.

Making instances checks its initialization arguments against the slots'
`:initarg`s, and now also against the keyword parameters of the applicable
`initialize-instance`, `shared-initialize` and `allocate-instance`
methods (CLHS 7.1.2). An `:after` method with `&key extra` makes `:extra`
a valid argument to `make-instance` for its class.

## A test that took twenty gigabytes

The fixpoint test (chapter 42) describes every function in the world in
C1, C2 and C3 and compares the descriptions, walking each function's
constants. It walked any other object by its slots, down to a depth of
64. A generic function's slots lead to its methods, their classes, the
classes' subclasses, their wrappers and back, and a walk bounded only by
depth follows every branch of that graph. The first full test run after
this sub-step took 24 minutes and 20 gigabytes of memory before the
operating system stopped it. An instance is not something the compiler
produced, so the test now describes one by its class's name, and the run
takes under a minute again. The other half of the fixpoint, comparing C2
and C3 word for word, still covers every byte of every method.

## What the review found

A review of the finished work ran some sixty probes in Quokka and in SBCL
side by side, and five of them disagreed. Our plan said that `defmethod`
on a name holding an ordinary function would replace the function, which
seemed friendly; CLHS and SBCL make it an error, and the review showed
why: `(defmethod reverse ((x integer)) x)` replaced `reverse`, which the
reader calls, and every form typed after it failed. It is an error now. A
method with `&rest` but no `&key` had been counted as accepting every
keyword, which CLHS 7.6.5 says it does not. An `:argument-precedence-order`
that left out a parameter was accepted and then dispatched on the wrong
argument. Redefining a class left the reader methods of its old definition
in place, so a reader the class no longer had still worked. And
`make-instance` cost about twenty thousand steps, a third of them spent
working out again, on every call, which keywords the class's
initialization methods accept; that answer now waits in a cache until the
next class or method changes, and the cost is about half. Each of these
has a test beside the others.

## What did it cost?

No Rust: `generic-functions.lisp` is about 700 lines of Lisp. The file
runs while the library is still loading, before `eval.lisp`, and that
showed in three places where it reached for something not yet defined
(`lambda-list-keywords`, `special-operator-p`, and `fdefinition`, which
asks `special-operator-p`). ansi-test's count rose only a little, to
10,034, because most of its CLOS tests also print instances, use
`with-slots`, or signal conditions, which come in the next sub-steps.
Method combinations other than the standard one wait for sub-step 6.
