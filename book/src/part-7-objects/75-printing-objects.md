# Printing objects

> *Where this fits.* Chapter 74 gave Quokka generic functions, so code can
> be chosen by the class of its arguments. The printer (chapter 39) still
> chose by itself, with one long `cond` that knew every kind of object.
> This chapter hands the printing of instances and structures to a generic
> function, `print-object`, so that a program decides how its own objects
> look, and adds `describe`, `with-slots` and `with-accessors`. It is the
> first place where the system itself is extended by methods, which is how
> the inspector and the debugger of later chapters will be extended too.

*Status: done, issue #124. Phase 3, step A, sub-step 4.*

## How a point should look

Define a class and make an instance, and the printer has little to say
about it:

```lisp
(defclass point () ((x :initarg :x) (y :initarg :y)))
(make-instance 'point :x 1 :y 2)    ; prints #<POINT>
```

The printer knows the instance's class and nothing else. The program knows
that a point is two numbers and that `#<POINT 1 2>` would say more. Common
Lisp's answer (CLHS 22.1.3) is that every object is printed by calling the
generic function `print-object` with the object and a stream, so the
program adds a method:

```lisp
(defmethod print-object ((p point) stream)
  (print-unreadable-object (p stream :type t)
    (format stream "~S ~S" (slot-value p 'x) (slot-value p 'y))))
```

Now `prin1`, `princ`, `format`'s `~A` and `~S`, `print` inside a list and
the REPL all print `#<POINT 1 2>`. The macro `print-unreadable-object`
writes the `#<` and `>`, the type when asked, and refuses with a
`print-not-readable` error when `*print-readably*` asks for text that
`read` could turn back into the object, which `#<...>` never is.

## Who goes through the generic function

Taken literally, CLHS says the printer calls `print-object` for every
object it prints, every number in a list and every character of a symbol's
package prefix included. In a system where a generic function call costs a
class lookup and a cache search (chapter 74), that is a lot to pay for
printing `(1 2 3)`, and two of our tests would notice. ansi-test gives each
file a budget of machine steps (chapter 49), and the warm rebuild runs the
whole library under a budget of its own (chapter 41); a slower printer
shows up in both as work that runs out of steps.

So we kept the printer's own code for the objects it has always printed,
and send only three kinds to `print-object`: instances of classes,
structures, and conditions. Those are the objects a program may have its
own idea about. The printer's entry point says it in four lines:

```lisp
(defun %print-plain (object out level)
  (if %print-object-on-built-in
      (%print-with-method object out level)
      (%print-built-in object out level t)))
```

`%print-built-in` is the old `cond`, with integers, symbols, strings and
conses tested first, so none of them pays anything new. Further down, the
branches for instances and structures, and one for conditions, call
`%print-with-method`, which calls `print-object`.

Structures made us look again. Every structure now went through
`print-object`, and we measured printing a list of a hundred structures:
it took a third longer than before this chapter, for no visible reason,
since almost no structure type has a method of its own and the method
that ran was the default one printing `#S(...)` as before. The time went
into the call itself, which finds the class of the structure and of the
stream and searches the generic function's cache. So the printer keeps,
for each structure type, whether any method but the library's two
defaults could apply to it, together with the dispatch epoch of chapter 74
(the token that changes whenever a class or a method is defined):

```lisp
(defun %printed-plainly-p (object)
  (let* ((layout (%structure-layout object))
         (entry (gethash layout %plain-structures)))
    (if (and entry (eq (car entry) %dispatch-epoch))
        (cdr entry)
        (let ((plain (%only-default-print-methods-p object)))
          (setf (gethash layout %plain-structures) (cons %dispatch-epoch plain))
          plain))))
```

A structure printed plainly then costs one hash table lookup more than it
did before, and the measured difference disappeared. When a program adds a
method, the epoch changes and the answer is worked out again. We checked
that a test notices if it is not: with the epoch comparison taken out, the
differential file fails at the first structure that gains a method after it
was printed.

The flag `%print-object-on-built-in` covers the case we left out. CLHS
lets a program define a `print-object` method on a built-in class, such as
`cons`, though it leaves the result undefined; SBCL honours a method on
`cons` and, at the top level of a printout, ignores one on `integer`. We
honour both, everywhere: `add-method` sets the flag the first time a
`print-object` method is defined on a built-in class, and from then on
every object goes through the generic function. The flag is never cleared.
With it set, printing is slower but still right.

The method on `t`, the one every object reaches last, is the printer's own
code again:

```lisp
(defmethod print-object ((object t) stream)
  (%print-built-in object stream (or %print-nesting 0) nil)
  object)
```

Its last argument, nil, tells `%print-built-in` not to send instances and
structures back to `print-object`, which would never end. It also means
that a method can call `call-next-method` and get exactly what the printer
printed before the method existed. Methods on `standard-object`,
`structure-object` and the classes of chapter 73's metaobjects print
`#<POINT>`, `#S(POS :X 1 :Y 2)` and `#<STANDARD-CLASS POINT>`.

## A method inside a printout

A method prints its object, and often that means printing other objects:
the point's coordinates, a box's contents. It does so with `format` or
`prin1`, the same functions a program calls from outside. But a printout
carries state that those functions would normally start afresh. With
`*print-level*` set to 2, the printer stops descending two lists deep and
writes `#`; with `*print-circle*` true, it labels an object it will meet
twice (`#1=`) and refers back to it (`#1#`), and the labels are counted
across the whole printout.

We measured what SBCL does, with a box whose method prints its contents,
and it continues the printout. A box inside a list, printed with
`*print-level*` 2, prints its contents one level down from the list,
`(#<BOX (1 #)>)`, as though the method's `format` were part of the outer
`prin1`; the box itself adds no level. With `*print-circle*`, a list
shared between the outer list and the box's contents is labelled once,
`(#1=(1 2) #<BOX #1#>)`, and a box that contains itself prints as
`#1=#<BOX #1#>`.

Quokka does the same with one special variable, `%print-nesting`. While
the printer runs a method, it holds the depth the object was printed at;
otherwise it is nil. Every printing function ends in `%print-top`, which
looks at it:

```lisp
(defun %print-top (object out)
  (cond (%noting (%note-shared object %circle-table))
        (%print-nesting
         (let ((level %print-nesting) (%print-nesting nil))
           (%print-object object out level)))
        (t (let ((%circle-table (if *print-circle* (%find-shared object) nil))
                 (%circle-label 0))
             (%print-object object out 0)))))
```

The last clause is a printout starting: it finds the shared objects and
prints from depth 0. The middle clause is a method's own printing: it
prints at the method's depth with the table already there.

The first clause is the harder half of `*print-circle*`. Printing with it
takes two passes over the object (chapter 52). The first walks everything
the printer will print and notes, in a hash table, which objects it meets
more than once; the second prints, labelling those. The first pass used to
walk conses, vectors and structure slots by itself, which was possible
because it knew how each of them prints. It cannot know what a method
prints. So now it runs the method, into a string stream nobody reads, with
`%noting` true, and anything the method prints is noted instead of
written. This is SBCL's approach as well. A structure goes the same way,
through its method, so a structure whose method prints only one slot is
not labelled for the others.

## Structures with printers of their own

`defstruct` had its own way of choosing a printer before any of this: a
`:print-function` or `:print-object` option, kept in the structure's
layout (chapter 61) and inherited by any structure that includes it. With
methods in the picture, the two ways can disagree. If `pos3` includes
`pos`, has a `:print-function`, and a program has defined a `print-object`
method on `pos`, which one prints a `pos3`? The method on `pos` is more
specific than the default method on `structure-object`, so dispatch would
choose it, and `pos3`'s own printer would never run. SBCL prints with
`pos3`'s printer, because its `defstruct` turns the option into a
`print-object` method on `pos3`'s class, and the more specific class wins.

We did the same. `%note-structure-defined`, which every `defstruct` already
calls once its definition has run, now adds a method on the structure's
class when the definition has a printer option. The ordinary rules then
decide everything: a structure's own printer beats a method on what it
includes, a method on a structure that includes it beats both, and a
`call-next-method` from a program's method reaches the `defstruct`
printer. Like SBCL's, the method stays when the structure is later
redefined without the option.

## Spaces in `#<...>`

`print-unreadable-object` looks too simple to have a decision in it, but
its spacing is not fully specified, and we followed what SBCL writes,
measured case by case. The type, when asked for, is the object's
`type-of`, so 5 is `#<(INTEGER 0 4611686018427387903) x>`, and it is
always followed by a space, even when nothing comes after it: `#<POINT >`.
ansi-test checks exactly that.

The identity is where we had to choose. SBCL writes the object's address,
`{1001A2B3C3}`, so that two objects of the same type can be told apart.
Quokka has no address to give, because the collector moves objects
(chapter 6), and numbering objects as they are printed would need a table
that keeps every printed object alive for good. So the identity prints as
nothing, keeping only the space in front of it that SBCL's rules put
there. For the default `#<POINT>` of an instance with no method we left
the space out as well, because there it would only be noise.

## `describe`, `with-slots` and `with-accessors`

`describe` writes a description of an object for a person, and it calls
the generic function `describe-object` to do it, so a program can describe
its own objects. CLHS leaves the text to each implementation, and ours
follows SBCL's in outline:

```text
#<POINT 1 2>
  [standard-object]

Slots with :INSTANCE allocation:
  X                              = 1
  Y                              = 2
```

A symbol is described by what it names (a variable and its value, a
function, a macro, a class and its superclasses), a class by its
precedence list and slots, an array by its element type and length. Since
no two implementations print the same text, these are tested in Quokka
only, and the differential file tests only that a program's own
`describe-object` method is called and that `describe` returns no values.

`with-slots` and `with-accessors` let a method's body use slots as though
they were variables:

```lisp
(with-slots (x y) p
  (setq x (+ x y)))
```

Each is a macro that binds the instance to a fresh variable and wraps the
body in `symbol-macrolet` (chapter 44), which makes `x` stand for the form
`(slot-value instance 'x)`. Reading `x` reads the slot, and `setq`,
`setf` and `incf` of `x` write it, because a symbol macro is a place
wherever its expansion is one.

## What the review found

A review of the finished work ran its probes in Quokka and SBCL side by
side and found two faults, both in the first pass of `*print-circle*`.
The first appeared only after a program had defined a `print-object`
method on a built-in class, such as `hash-table`. From then on every list
went through the generic function in the first pass too, and the list
printer, which walks along a list's tail itself, never noted the tail
conses as it went. A shared tail was therefore never found, and a circular
list never finished printing, in that world for good, since the flag is
never cleared. The list printer now notes each tail as it walks into it,
as the first pass always had. The second fault was a cost. The first pass
ran the default method for every structure, through the generic function
and into a discarded stream, where it used to walk the slots, and printing
a list of structures with `*print-circle*` true fit 230 of them into the
step budget where it used to fit 350. A structure that prints plainly is
walked by its slots again. Each fault has a test of its own.

## What did it cost?

No Rust. The printer grew by about a hundred and fifty lines of Lisp and
lost `%instance-text` and `%type-name`; `describe.lisp` is new, about 150
lines. The differential file `print-object.lisp` holds about a hundred
forms, checked against SBCL.

That file ran into one of our own checks. `scripts/check-differential.sh`
refuses any baseline containing `#<`, because the `#<...>` an
implementation writes for one of its own objects never matches another's,
and a test that printed a function once slipped through to SBCL that way.
Here every `#<...>` is written by the file's own methods or by
`print-unreadable-object`, and SBCL and Quokka agree on all of them. We
kept the check and had the file show its results through a small function,
`shown`, that writes each `#<` as `#[`.

Our plan had one measurement wrong. We first measured SBCL with its
pretty printer on, the default in a script, and saw
`print-unreadable-object` count as a level of nesting, so the plan said
Quokka's would too. The differential file, run under the oracle's settings
with the pretty printer off, disagreed: SBCL's ordinary printer counts no
level there, and only its pretty printer does. Quokka has no pretty
printer yet, so it counts none.
