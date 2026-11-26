# Classes

> *Where this fits.* Chapter 72 gave Quokka a type system: `typep` asks
> whether an object is of a type, and `subtypep` whether one type lies
> inside another. Phase 3 is building CLOS, Common Lisp's object system,
> before the condition system and the debugger, because those and every
> tool after them dispatch on the type of an object (`docs/clos.md`). This
> chapter adds classes and instances: every object gets a class, and a
> program can define its own with `defclass` and make objects with
> `make-instance`. Generic functions, which choose code by class, come in
> the next chapter.

*Status: done, issue #124. Phase 3, step A, sub-step 2.*

## What a class is

A class names a kind of object and says what its instances hold:

```lisp
(defclass point ()
  ((x :initarg :x :initform 0 :accessor point-x)
   (y :initarg :y :reader point-y)))

(defvar *p* (make-instance 'point :x 3 :y 4))
(point-x *p*)        ; => 3
(setf (point-x *p*) 10)
```

`x` and `y` are *slots*, named places in each instance. `:initarg` names
the keyword that fills a slot when the instance is made, `:initform` the
value it gets otherwise, and `:accessor` and `:reader` ask for functions
that read and write it. A class can name superclasses, and then it has
their slots too: `(defclass point3 (point) ((z ...)))` makes points with
three slots, and every `point3` is also a `point`.

Classes are not only for objects a program defines. Every object in
Common Lisp has a class, which `class-of` returns: 5 is a `fixnum`, `"abc"`
is a `string`, `(1 2)` is a `cons`, and the class of a structure is a class
named after the structure. So a function that is to behave differently for
different kinds of argument can ask one question of any argument, and the
next chapter's generic functions are built on that.

## Classes are objects too

Here CLOS does something that looks circular. A class is itself an object,
and every object has a class, so a class has a class: `point` is an
instance of `standard-class`. Such a class of classes is called a
*metaclass*. And `standard-class` is a class like any other, so it too is
an instance of a class, which is `standard-class` again:

```lisp
(class-of (find-class 'point))           ; => #<STANDARD-CLASS STANDARD-CLASS>
(eq (class-of (find-class 'standard-class))
    (find-class 'standard-class))        ; => T
```

This is what makes CLOS a *metaobject protocol*. Because a class is an
ordinary object with slots of its own (its name, its superclasses, its
slots), a program can ask it about itself, and later, change how it
behaves by defining a new metaclass. Phase 5's inspector will ask a class
for its slots this way. The design follows Part I of *The Art of the
Metaobject Protocol* by Kiczales, des Rivières and Bobrow (1991), which
builds a complete CLOS called Closette in about a thousand lines of Lisp
and explains each step.

## Instances in the heap

An instance needs a place in memory, and this is the one part of CLOS that
had to be Rust, since only Rust allocates heap objects. We added two heap
types. A *standard instance* has two slots: a *wrapper*, which belongs to
the instance's class and lists the names of its slots in order, and a
*slot vector*, which holds their values. A *funcallable instance* has the
same two with a function in front of them.

Keeping the values in a vector of their own, instead of inline after the
wrapper, costs one extra step on every slot access. It buys something a
running world needs: when a class is redefined with another slot, its
existing instances must gain that slot while staying the same objects, `eq`
to what they were. An object in our heap cannot grow, but its vector can be
replaced. This is how PCL, SBCL's CLOS, represents instances.

Our first version did not use it yet, and a review of the finished work
showed what that cost. Redefine `point` with a slot `w` before `x`, and an
old point's slot vector still had `x` first while the class now said `x`
was second: `(slot-value old 'w)` quietly returned the value of `x`. CLHS
4.3.6 says an instance of a redefined class is updated before any of its
slots is touched, and now it is: the first slot access sees that the
instance's wrapper is no longer its class's, makes a new slot vector in
the class's layout, keeps every value whose slot kept its name, and fills
the new slots from their initforms. Sub-step 5 will add the generic
function a program can specialise around that,
`update-instance-for-redefined-class`. The same review found that
`defclass` would quietly turn a structure's class into a standard class,
and accepted a built-in class such as `integer` as a superclass, which
SBCL refuses; both are refused now.

Our design document first said an instance would be the `Instance` heap
type that structures already use (chapter 61). When we came to build it we
found that every structure path reads an `Instance`'s first slot as a
structure layout, so a standard object would have needed a check in each of
them. A heap type of its own needed none, and cost nothing, since the
header has room for more types.

The funcallable instance exists for one reason. A generic function is
called like any function, `(area shape)`, and is also an object with slots
(its name, its methods). The machine has to know how to call such an
object, and it does it in `call_value`:

```rust
        // A funcallable instance, such as a generic function, is called
        // through the function it holds, with the same arguments (#124). Its
        // function is never another funcallable instance (the setter refuses
        // one), so one replacement is enough.
        let callee = if crate::instance::is_funcallable(self.mc, callee) {
            let function = crate::instance::function(self.mc, callee);
            if !crate::instance::is_plain_function(self.mc, function) {
                return self.fail(EvalError::NotAFunction(String::from(
                    "#<funcallable-instance with no function>",
                )));
            }
            function
        } else {
            callee
        };
```

The alternative was to make a generic function a closure and keep its
slots in a table from closure to object. It needed no Rust, but every
generic function would have been two objects to keep in step, and the
table would have kept every one of them alive. Two hundred and forty
lines of Rust bought the simpler model.

## Breaking the circle

If `standard-class` is an instance of itself, how is the first one made?
`defclass` cannot make it, because `defclass` makes instances of
`standard-class`, which does not exist yet. And a class's slots are
described by *slot definitions*, which are instances of classes too.

The answer is the one Quokka already gave for its compiler, which is
compiled by an interpreter the first time and by itself after that (*The
thing that builds itself*, and chapter 32): build the first ones by hand,
without the machinery they describe, and let the machinery take over once
it exists. Our bootstrap,
in `clos.lisp`, works in four stages:

1. Make an empty object for each of the first twenty classes (`t`,
   `standard-object`, `class`, the metaclasses, the slot definition
   classes) and enter it in the table of classes.
2. Give each metaclass a wrapper. Every metaclass's instances are classes,
   with the same twelve slots, so these wrappers can be written down
   without computing anything. Then give each of the first classes its
   metaclass's wrapper, which makes it an instance of that metaclass.
3. Give each class its superclasses and its direct slots, now that slot
   definition objects can be made.
4. Compute each class's precedence list and effective slots.

After the fourth stage `standard-class` is an instance of itself, and
`defclass` works.

Each stage can also run a second time. The warm rebuild (chapter 41)
reloads every library file while the world is running, `clos.lisp`
included, and every class and instance in the world stays in use while it
does. So the bootstrap looks a class up before making it and refills an
existing one, and a class whose slots have not changed keeps its wrapper,
which keeps every instance made before the rebuild an instance of it. A
test makes a class and an instance, rebuilds the world, and checks that
both still work.

## The class precedence list

With several superclasses, a class needs an order in which to consult
them: when two superclasses both define a slot or, later, a method, which
one wins? CLOS answers with the *class precedence list*, one total order
of the class and all its superclasses. Each class's definition asks for two
things: the class comes before its superclasses, and those come in the
order it lists them. The precedence list must keep every such request from
every class involved, so computing it is a topological sort (CLHS 4.3.5):

```lisp
(defun %compute-class-precedence-list (class)
  (let* ((classes (%all-superclasses class))
         (pairs (let ((all nil))
                  (dolist (c classes all) (setq all (append (%local-precedence c) all)))))
         (result nil))
    (do () ((null classes) (nreverse result))
      (let ((candidates (remove-if (lambda (c) (some (lambda (pair) (eq (cdr pair) c)) pairs))
                                   classes)))
        (when (null candidates)
          (error "the superclasses of this class cannot be put in one order"
                 (%class-name class)))
        (let ((next (if (null (cdr candidates))
                        (car candidates)
                        (%precedence-tie candidates result))))
          (push next result)
          (setq classes (remove next classes))
          (setq pairs (remove-if (lambda (pair) (eq (car pair) next)) pairs)))))))
```

A candidate is a class that nothing still waiting has to come before. When
there are several, CLHS picks the one that is a direct superclass of the
class furthest to the right in the list so far, which keeps each class's
superclasses close behind it. If there is no candidate the requests
contradict each other, as in `(defclass c (a b) ())` where `b` is a
subclass of `a`: `c` asks for `a` before `b`, and `b`'s own definition asks
for `b` before `a`. That is an error, and `defclass` then removes the class
it had begun to make, so the table of classes never holds one that could
not be finished.

Python and Dylan use a related algorithm called C3, which refuses some
hierarchies that CLOS accepts. Since Quokka follows Common Lisp, it uses
the CLHS rule.

## Every object has a class

Built-in classes take a table: the classes of CLHS figure 4-8 (`number`,
`cons`, `string`, `hash-table` and the rest) with the superclasses the
standard gives them, and a few more that SBCL has and `class-of` returns,
such as `fixnum` and `simple-vector`. The standard condition types become
classes of their own metaclass, `condition-class`, as in SBCL, where a
condition is not a standard object. A condition is still the machine's own
object for now, and `class-of` finds its class by its type's name; the
next step of this phase makes conditions instances of these classes.

A structure's class is made the first time something asks for it, from
its layout, so `structures.lisp` did not need to know about classes. That
choice had one consequence we found through ansi-test. `defstruct`
installs a structure's layout while it is being macroexpanded, so that a
later `defstruct` can include it before the first has run, and so
`find-class` saw the structure as defined too early. ansi-test's reader
tests guard a definition with `(unless (find-class 'x nil) (defstruct x
...))`, and our `find-class` answered before the `defstruct` had run, so
the `unless` skipped it and the structure was never defined. Now the
expansion marks the structure defined when it runs, and only then does
`find-class` make its class.

Every class is also a type. `(typep x 'point)` asks whether `point` is in
the precedence list of `x`'s class, and `subtypep` has a new kind of object
for instances (chapter 72), whose terms name classes. Classes can have
several superclasses, so two classes neither of which inherits from the
other may still share instances, and `subtypep` answers "don't know" for
their intersection, as it does for conditions. When it can prove an
answer, it does: ansi-test asks whether a long combination of `and`, `or`
and `not` over three classes is empty, which is true for reasons of logic
alone. Our first version built every combination of terms and ran out of
steps, and dropping each term as soon as it is known to be empty fixed it.

## Functions now, generic functions next

`make-instance`, `initialize-instance` and `shared-initialize`, which
CLHS 7.1 describes, are ordinary functions in this sub-step, and so are
the readers and writers `defclass` makes. In CLOS they are generic
functions that a program can add methods to, and they become that in the
next sub-step, with these definitions as their standard methods. Closette
builds them in the same order, since a generic function is itself an
instance of a class and cannot exist before classes do.

## What did it cost?

Rust: two heap types, eight primitives that each take constant time, a
branch in `call_value`, and the image format going from 7 to 8, about 240
lines. The rest is Lisp, in `clos.lisp`, about 800 lines. ansi-test now
counts `defclass`, `make-instance`, `slot-value` and their relatives as
present, and 111 more of its tests pass (10,030 of 15,984, with no plain
failure); the ones that also use
`defgeneric` or `defmethod` wait for the next chapter.
