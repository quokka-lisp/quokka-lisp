# What is a type?

> *Where this fits.* Phase 2 ended with tier 1 complete: the reader, the
> printer, the compiler and the library all run compiled, and the compiler
> reproduces itself (chapter 71). Phase 3 aims at a debugger in which you
> can fix a function from inside an error and retry it, and we decided to
> build an object system, CLOS, before anything else in it, because the
> condition system and every tool after it dispatch on the type of an
> object (`docs/clos.md`). An object system needs a type system first. This
> chapter builds one: `typep`, `subtypep`, `deftype` and the forms on top of
> them.

*Status: done, issue #124. Phase 3, step A, sub-step 1.*

## Two questions about types

Common Lisp describes a type with a *type specifier*. The simplest are
names, such as `integer`, `string` and `symbol`. Others are lists that
narrow a type or combine several: `(integer 0 9)` is the integers from 0
to 9, `(or string symbol)` is anything that is one or the other, and
`(cons integer null)` is a one-element list holding an integer. A program
can name a specifier of its own with `deftype`:

```lisp
(deftype small () '(integer 0 9))
(typep 5 'small)            ; => T
```

There are two questions anyone can ask about a type. The first is about
one object: is it of this type? That is `typep`. The second is about every
object at once: is every object of this type also of that one? That is
`subtypep`, and it answers with two values, the answer and whether the
answer is certain.

```lisp
(subtypep '(integer 1 5) '(real (0) 6))       ; => T, T
(subtypep '(and integer (not fixnum)) 'bignum) ; => T, T
(subtypep '(satisfies evenp) 'integer)         ; => NIL, NIL
```

The last line is the honest answer. `(satisfies evenp)` is every object
for which `evenp` returns true, and nothing short of calling `evenp` on
every object could say whether all of them are integers. (In fact `evenp`
signals an error on a string, so the question is badly posed.) The
standard lets `subtypep` say "don't know" whenever it cannot be sure, and
our task was to say it only where SBCL does.

Quokka had neither function until now. Phase 2's ansi-test runner listed
both as deferred to phase 3, along with `deftype`, `typecase` and
`check-type`, and every test that used them was explained by that.

## typep looks at one object

We built the two questions separately. `typep` takes the specifier apart
and asks the object directly: a name is looked up in a table of
predicates, a list is handled by its first element.

```lisp
(defun %typep-atom (object name)
  (let ((predicate (gethash name %atomic-types)))
    (cond (predicate (if (funcall predicate object) t nil))
          ((%structure-type-layout name)
           (%structure-is-a object (%structure-type-layout name)))
          ((%condition-type-name-p name)
           (and (%conditionp object) (%condition-subtype-p (%condition-type object) name)))
          (t (error "unknown type specifier" name)))))
```

For a structure's name it walks the structure's `:include` chain (chapter
61), and for a condition type's name the lattice of standard condition
types from chapter 69. A compound specifier such as `(integer 0 9)`
checks that the object is an integer and then compares it with the
bounds; `(and ...)`, `(or ...)` and `(not ...)` call `typep` again on
their parts.

We could have answered `typep` with the machinery `subtypep` needs,
described below, by turning the object into a type of its own and asking
whether it lies inside the other. It would have been one implementation
instead of two. But `typep` is asked about one object at a time and often
in a loop, `typecase` calls it for every clause, and building and
comparing two canonical types for a question that one predicate call
answers would make every `typecase` many times slower. So the two are
separate, and the differential tests check that they agree with SBCL, and
so with each other.

## subtypep looks at every object

`subtypep` cannot look at objects, because the question is about all of
them, including ones that do not exist yet. It works by rewriting each
specifier into a *canonical type* (SBCL calls them ctypes, and so do we),
a form in which two specifiers that mean the same set are written the
same way. Then it asks one question: is there anything in the first type
that is not in the second? If the type `A and not B` is empty, `A` is a
subtype of `B`.

Emptiness becomes easy to see once the world is divided into
kinds of object that do not overlap and a type is described kind by kind.
Quokka's kinds are integers, ratios, floats, `-0.0` (we come back to it
below), complex numbers, characters, keywords, other symbols, conses,
arrays, structures, conditions, two kinds of function, hash tables and a
few kinds Quokka has no objects of yet, such as pathnames. Every object is
of exactly one kind. For each kind, a canonical type says which objects of
that kind it holds, in a form chosen to suit the kind:

1. Integers, ratios and floats are lists of intervals on the number line,
   each end either open or closed or unbounded. Character codes are
   intervals too.
2. Symbols and keywords are a set, either "these symbols" or "every symbol
   except these".
3. Conses are a list of pairs, each pair a canonical type for the car and
   one for the cdr.
4. Arrays are a list of terms: an element type, whether the array is
   simple, a pattern of dimensions, and patterns to leave out.
5. Structures and conditions are a list of type names, each with names to
   leave out.
6. Every other kind is all or nothing.

With the kinds disjoint, `and`, `or` and `not` work kind by kind: the
integers of `(and A B)` are the integers of `A` intersected with those of
`B`, and so on. A type is empty when every kind's part of it is empty.
Henry Baker described this approach for Common Lisp in "A Decision
Procedure for Common Lisp's SUBTYPEP Predicate" (1992), and SBCL's type
system is built the same way.

Here is the union of two sets of intervals. It sorts them by their lower
ends and joins any interval that overlaps or touches the one before:

```lisp
(defun %intervals-or (a b integer)
  (let ((sorted (sort (append a b) #'%low-before-p))
        (result nil))
    (dolist (iv sorted (nreverse result))
      (if (and result (%intervals-join-p (car result) iv integer))
          (setf (car result) (%interval-join (car result) iv))
          (push iv result)))))
```

The `integer` flag matters for one case. Two integer intervals that only
touch, such as 0 to 4 and 5 to 10, have no integer between them, so they
join into 0 to 10, and `(integer 0 10)` is then a subtype of `(or (integer
0 4) (integer 5 10))`. Two intervals of ratios that touch at 4 and 5 leave
4½ out.

Conses show why each kind has a form of its own. A cons type
is a product: `(cons A D)` is the conses whose car is in `A` and whose cdr
is in `D`. The complement of one pair among the conses is "car not in
`A`, or cdr not in `D`", which is two pairs, `(not-A . t)` and `(t .
not-D)`. With that rule `not` never leaves the representation, and
`subtypep` can show that `(and cons (not (cons symbol symbol)))` and `(or
(cons (not symbol) t) (cons t (not symbol)))` are the same type, as one of
ansi-test's tests asks.

Arrays needed one observation to stay exact. A type such as `(array t (2
*))` minus `(array t (2 3))` is the two-dimensional arrays whose first
dimension is 2 and whose second is not 3. When is such a difference empty?
A pattern with a `*` in it holds infinitely many sizes in that place, and a
finite set of patterns can name only finitely many sizes there one by one,
so a pattern lies inside a union of patterns only if it lies inside one of
them. Checking that is a loop.

## Objects that are only themselves

`(member a b c)` and `(eql 5)` name types by listing objects. Numbers,
characters and symbols fit the kinds directly: 5 is the integer interval
from 5 to 5, `a` goes into a set of symbols. But `member` can list any
object, such as a particular string, and two strings with the same
characters are still two objects:

```lisp
(let ((a (copy-seq "abc")) (b (copy-seq "abc")))
  (subtypep `(member ,a) `(member ,b)))   ; => NIL, T
```

So a canonical type carries two lists beside its kinds: objects it holds
that no kind says it holds, and objects a kind holds that it leaves out.
`not` swaps the two lists, which is why both are needed. Each kind that
such an object can belong to, such as conses, strings and structures,
holds infinitely many objects whenever it holds any, because a program can
always make another, so leaving a few out never empties a kind.

Our first version got the example above wrong, and not in the algebra.
`subtypep` began with a shortcut: if the two specifiers are `equal`, the
answer is yes without looking further. But `equal` compares strings by
their characters, so `(member "abc")` and another `(member "abc")` were
"equal". We changed it to `tree-equal`, which compares the leaves with
`eql`, and the next run failed on two different lists `(a b c)` inside
`member`, because `tree-equal` looks inside them too. The shortcut now
applies only when the two specifiers are the same object. ansi-test found
this, along with three other mistakes: a missing case for
the empty set of symbols, which broke every `or` of two `not`s of symbols;
`deftype` bodies without the block the standard gives them; and `-0.0`.

## Negative zero

`-0.0` is equal to `0.0` with `=` but is not the same object under `eql`.
So it lies inside the range `(float 0.0 1.0)`, because `-0.0 >= 0.0`, but
not inside `(member 0.0)`. That makes `(float 0.0 1.0)` not a subtype of
`(or (float (0.0) 1.0) (member 0.0))`: the first holds `-0.0` and the
second does not. An interval of rationals cannot tell the two zeros
apart, so we made `-0.0` a kind of its own, all or nothing, which a float
range joins whenever the range contains zero.

## Types we cannot see into

Two kinds of specifier are opaque to the algebra. `(satisfies f)` is an
arbitrary predicate, and a function type such as `(function (integer)
string)` describes what a function accepts and returns, which no one can
check by looking at the function. We treat each as an unknown set.

The useful observation is that whether an object lies in `A and not B`
depends only on which of the unknown sets it belongs to, and each set can
hold any objects at all. So `A and not B` is empty for every possible
meaning of the unknown sets exactly when it is empty with each set
supposed to be everything or nothing, in every combination. With one
unknown set that is two checks; with six it is sixty-four.

```lisp
(defun %subtypep (type-1 type-2)
  (let ((%unknown-sets nil) (%unknown-assignment nil))
    (multiple-value-bind (empty certain) (%difference-empty-p type-1 type-2)
      (cond ((null %unknown-sets) (values empty certain))
            ((> (length %unknown-sets) 6) (values nil nil))
            (t (dolist (assignment (%assignments %unknown-sets) (values t t))
                 (setq %unknown-assignment assignment)
                 (multiple-value-bind (empty certain) (%difference-empty-p type-1 type-2)
                   (unless (and empty certain) (return (values nil nil))))))))))
```

If every combination is empty, the answer is a certain yes: `(and integer
(satisfies evenp))` is a subtype of `integer` whatever `evenp` does. If
some combination is not empty, the true set might still make it empty, so
the answer is "don't know". That is the answer SBCL gives in every case
our tests ask about.

## What one float format and no base strings cost

Quokka has one float format, the IEEE double (chapter 48), so
`short-float`, `single-float`, `double-float` and `long-float` are four
names for one type. The standard allows that. SBCL has two formats, so
`(subtypep 'float 'double-float)` is false there and true here, and
`(coerce 1 'float)` gives SBCL a single float that Quokka cannot make. Our
differential files leave such questions out, and say so at the top.

Strings are similar. SBCL has *base strings*, which hold only ASCII
characters, and so in SBCL `"abc"` is not a `base-string`. Quokka's
strings hold any character, so `base-char` upgrades to `character` as an
element type and `base-string` is the same type as `string`, which is
what the standard requires of an implementation that upgrades that way.

We first wrote the opposite, that no Quokka array is of type `(array
base-char)`, because that matched SBCL's answer for a string. It broke on
the first array made with `:element-type 'base-char`, which then was not
of the type it was made with. The rule that an array is of type `(array
E)` for the `E` it was made with is the one to keep, and SBCL's answer
comes from strings Quokka does not have.

## What did it cost?

No Rust. `types.lisp` holds `deftype`, `typep`, `type-of`, the `typecase`
family, `check-type` and `coerce`, and `type-algebra.lisp` holds the
canonical types and `subtypep`, about 1,400 lines of Lisp between them.
`upgraded-array-element-type` now asks `subtypep`, so `make-array` with
`:element-type '(member #\a #\b)` makes a string, as it should, with the
common names answered at once.

ansi-test's `types-and-classes` group joined the run. Its tests that do
not pass need CLOS (the group defines classes and asks about them),
complex numbers, or `loop`, or SBCL gives the same answer we do.
`ctypecase` and `check-type` signal their `type-error` but cannot yet
offer to store a new value, which needs restarts (step B of this phase).

The next sub-step builds classes on this: `defclass` makes a name that
`typep` and `subtypep` must know, and every object, from a fixnum to a
structure, gets a class.
