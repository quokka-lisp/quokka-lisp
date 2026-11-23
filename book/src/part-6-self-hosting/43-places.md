# Places

> *Where this fits.* In chapter 19 we pulled a small `setf` forward into tier
> 0, because without it a test could not write a vector element in a way SBCL
> understands. Chapter 21 let `defstruct` teach that `setf` new places,
> through a table from a reader's name to a writer's name. Chapter 42 made the
> compiler reproduce itself and turned that into a test every later change
> has to pass. This chapter builds Common Lisp's real `setf`, where anyone can
> define a place, together with the macros that update places: `incf`,
> `push`, `pop` and the rest. It is the first tier-1 feature whose design was
> shaped by the fixpoint, and it is needed for step J, sequences, which use
> places everywhere.

*Status: done, issue #52. Step H of phase 2.*

## What is a place?

Most languages treat the left-hand side of an assignment as special syntax:
`x = 1`, `v[i] = 1`, `p.x = 1`. Common Lisp has one operator for all of
these, and calls the thing on the left a **place** (the standard also uses the
term *generalized reference*). You write a place as the form that would read
it:

```lisp
(setf x 1)                ; a variable
(setf (aref v i) 1)       ; a vector element
(setf (gethash k h) 1)    ; a hash-table entry
(setf (point-x p) 1)      ; a structure slot
```

`setf` is a macro. It looks at the reader, here `aref`, and writes the code
for the matching writer, `(%set-aref v i 1)`. The program describes what it
wants to change in the same words it would use to read it, so a reader can
never be paired with the wrong writer.

Tier 0's `setf` did this with a fixed list of places and one table. That was
enough to store a value into a place, but not to update one.

## Why isn't the writer's name enough?

Consider the most common update of all:

```lisp
(incf (aref v (next-index)))
```

`incf` adds one to a place. The obvious way to expand it is to read the
place, add one, and store the result:

```lisp
(setf (aref v (next-index)) (+ (aref v (next-index)) 1))
```

This calls `next-index` twice, so it reads one element and writes another.
`incf` has to evaluate `v` and `(next-index)` once, keep the results, and use
them both for reading and for writing. For that it needs to know how the place
is built: which of its parts are subforms to be evaluated, how to read the
place once they have been, and how to write it. A table that maps `aref` to
`%set-aref` tells it none of this.

Common Lisp solves the problem with the **setf expansion** (CLHS 5.1.1.2).
The function `get-setf-expansion` takes a place and returns five values:

| | | for `(aref v (next-index))` |
| --- | --- | --- |
| vars | temporaries | `(#:g1 #:g2)` |
| vals | the subforms they are bound to | `(v (next-index))` |
| stores | variables that will hold the new value | `(#:new)` |
| writer | stores it, and returns it | `(%set-aref #:g1 #:g2 #:new)` |
| reader | reads the place | `(aref #:g1 #:g2)` |

With these, every macro that reads and writes a place follows the same
recipe: bind the temporaries, compute the new value using the reader, bind
the store variable, and run the writer. Each subform is evaluated exactly
once, from left to right, whatever the macro does in between. This is what
our `incf` example turns into:

```lisp
(let* ((#:g1 v)
       (#:g2 (next-index))
       (#:new (+ (aref #:g1 #:g2) 1)))
  (%set-aref #:g1 #:g2 #:new))
```

The temporaries are gensyms (chapter 15), so they cannot clash with any name
the program uses. There can be more than one store variable because one
place, `(values a b)`, takes several values at once.

There are three ways to teach `setf` a new place. The simplest is the short
form of `defsetf`, `(defsetf name writer)`, which turns `(setf (name args...)
v)` into `(writer args... v)`; this is tier 0's table again, under its
standard name. The long form, `(defsetf name lambda-list (store) body...)`,
lets the body build the writer form itself. Its parameters are bound to the
temporaries rather than to the place's subforms, so the body can use each of
them as often as it likes. The most general is `(define-setf-expander name
lambda-list body...)`, whose body returns all five values.

`incf` is defined on top of these with `define-modify-macro`.
`(define-modify-macro incf (&optional (delta 1)) +)` defines a macro that
stores `(+ old-value delta)` back into a place, and `decf` is the same with
`-`. `push`, `pop`, `pushnew`, `rotatef`, `shiftf` and `psetf` each use the
five values in their own way, so we wrote them out by hand.

## Compiling the compiler with the new `setf`

The fixpoint is where this step became more than a library exercise. It is
the test from chapter 42 that rebuilds the system with itself, twice, and
requires the compiler to come out the same every time. The warm
rebuild (chapter 41) compiles the prelude and the compiler with the macros of
the running world, and by then the running world has the library loaded,
including its new `setf`. Suppose the library's `setf` expanded
`(setf (gethash k h) v)` in the general way, with temporaries. Then C2's
compiler would be compiled from different code than C1's, and
`the_compiler_reproduces_itself` would fail, even though nothing was wrong.
The compiler would simply have been compiled in a different environment from
the one its source was written for. We had expected this since chapter 34.

The system's own source uses `setf` on three kinds of place: `gethash` twelve
times, `aref` three times and structure accessors nine times. The question
for #52 was how the new `setf` should live beside the old one, and we
considered three answers.

The first was a separate compilation environment: the rebuild would compile
the system with the prelude's macros, and user code would see the library's.
This is the principled solution, and it is how SBCL builds itself. Its
compiler runs in a separate host Lisp and compiles the new system for a
target, keeping the two sets of definitions apart (the source calls this
`SB-XC`, the cross-compiler). For us it would have meant passing a macro
environment to every place in the compiler that looks up a macro. That changes
how the compiler compiles itself, to solve a problem the other answers do not
have.

The second was to expand every `setf` the general way, even on `gethash`. To
keep the fixpoint, the cold build would have needed the same expansion, so
the whole protocol would have moved into the prelude, into tier 0, where the
interpreter runs it at every boot. It would also give every `setf` in the
system two or three temporaries. A plain store never needs them; only a macro
that reads and writes a place does. Our compiler has no pass that removes a
temporary used only once, so each would cost a local slot and the
instructions that fill it.

We chose the third option, a shortcut, even though our first reaction was
against shortcuts. What settled it was asking what this one would actually
cost. The danger of a shortcut is two code paths that drift apart,
so that the same place means one thing to `setf` and another to `incf`. We
avoided that by giving both paths the same source of truth. The prelude's
knowledge of places was split out into a function that returns nil for a
place it does not know:

```lisp
(defun %setf-place (place value)
  (or (%setf-known-place place value)
      (%setf-unknown-place place)))
```

The library's `setf` asks that function first, and only sends a place it does
not know through the five values:

```lisp
(defun %setf-pair (place value)
  (or (%setf-known-place place value)
      (multiple-value-bind (vars vals stores writer) (get-setf-expansion place)
        ...)))
```

And `get-setf-expansion` builds the five values for a known place from the
same function, by asking it to write into temporaries:

```lisp
((%setf-known-place place nil)
 (multiple-value-bind (vars vals args) (%temporaries (cdr place))
   (let ((store (gensym "NEW")))
     (values vars vals (list store)
             (%setf-known-place (cons (car place) args) store)
             (cons (car place) args)))))
```

So a place the prelude knows still expands exactly as before: the same
function writes it, before and after the library is loaded. We did not have
to be careful to keep the two in step, and the fixpoint tests check on every
run that they are. The differential file also tests every place under both
`setf` and `incf`.

We found out afterwards that SBCL takes the same shortcut. If you ask it for
`(macroexpand-1 '(setf (car x) v))` you get `(SB-KERNEL:%RPLACA X V)`, with
no temporaries, while `(get-setf-expansion '(gethash k h))` returns the full
five values. The standard leaves the exact expansion of `setf` to the
implementation, as long as the order of evaluation is right.

## Finding a bug in tier 0

The order of evaluation turned out to matter sooner than expected. While
writing tests that log the order in which subforms run, we found that the
prelude expanded `(setf (car PLACE) VALUE)` like this:

```lisp
(let ((#:g VALUE))
  (rplaca PLACE #:g)
  #:g)
```

This evaluates VALUE first. CLHS 5.1.1.1 says the place's subforms come first
and the value after them, and SBCL agrees: the logging test showed SBCL
running `(PLACE VALUE)` and Quokka running `(VALUE PLACE)`. The fix was two
lines in the prelude, binding the cons before the value. It did not disturb
the fixpoint, since no system source sets a `car`; only `%setf-through`
itself changed, and it changed in the same way in C1, C2 and C3. Because both
paths share one table, `incf` and `push` on a `car` got the right order at
the same time.

The standard has one exception to the left-to-right rule: `push` evaluates
its item before the place's subforms, because the item comes first in
`(push item place)`.

## Writing a place inside a place

Most of the new places take one line. `cadr`, for example, is a long-form
`defsetf`:

```lisp
(defsetf cadr (x) (new) `(progn (rplaca (cdr ,x) ,new) ,new))
```

`getf` is more interesting, and it is the reason `define-setf-expander`
exists. A **property list**, or plist, is a list of alternating keys and
values such as `(:colour red :size 3)`, and `getf` looks up a key in one. If
the list already has the key, setting it just changes the value in place. If
it does not, the new pair goes on the front of the list, which makes a new
list, and whatever held the old list has to be updated as well:

```lisp
(let ((plist nil))
  (setf (getf plist :colour) 'red)   ; plist must now be (:colour red)
  plist)
```

Here `plist` is a variable, but it could just as well be `(aref v 0)` or a
structure slot; it is a place in its own right. So `getf`'s expander asks for
the expansion of that inner place and writes through it:

```lisp
(define-setf-expander getf (place indicator &optional (default nil default-p)
                            &environment environment)
  (multiple-value-bind (vars vals stores writer reader) (get-setf-expansion place environment)
    ...
              `(let ((,(%one-store stores place) (%putf ,reader ,key ,new)))
                 ,writer
                 ,new)
    ...))
```

A table from reader names to writer names has no way to say "write the new
list back to wherever the list came from". With five values it is easy,
because the inner place's writer is just one more form to put inside the
outer one.

## How is an expander made?

The lambda list of `define-setf-expander` is a macro lambda list, with
destructuring, and Quokka already has a complete implementation of those in
`defmacro`. So we make the expander the way `defmacro` makes a macro, under a
name nobody can write (a fresh uninterned gensym), and then take it back out
as a function:

```lisp
(defmacro define-setf-expander (name lambda-list &body body)
  (let ((hidden (gensym "SETF-EXPANDER")))
    (multiple-value-bind (plain environment) (%without-environment lambda-list)
      `(progn
         (defmacro ,hidden ,plain (let ((,environment nil)) ,environment ,@body))
         (%define-setf-expander ',name (%macro-function ',hidden))))))
```

This works because a macro function in Quokka takes the argument forms of the
macro call (chapter 15's `macroexpand-1` calls `(apply expander (cdr form))`),
which is exactly what an expander needs.

The `&environment` parameter is removed and bound to nil. In full Common Lisp
it carries the **lexical environment**: the local macros (from `macrolet`)
and the symbol macros in scope where the `setf` appears, so that a nested
`get-setf-expansion` can see them. When we wrote this, Quokka's `defmacro`
had no `&environment` and there was no environment object to pass, so a place
defined with `macrolet` was not recognised. Step I came back to this when
`symbol-macrolet` made symbols into places as well (chapter 44).

## What did it cost?

The step needed no Rust apart from the new file's line in `boot::LIBRARY`.
The Lisp is about 450 lines in `lisp/library/setf.lisp`, most of them
comments, a dozen lines in the prelude, and one name added to
`%library-files`. The image grew from 359,688 to 397,944 bytes, because
`define-modify-macro`, `defsetf` and `define-setf-expander` each leave
compiled code behind for every place they define.

The fixpoint, our check that the compiler compiles itself into exactly the
same code generation after generation (chapter 42), held the first time. The
only rebuild test that failed was the
one that counts how many forms a rebuild reads, which went from 373 to 421
because the library had grown. The comparison of C1, C2 and C3 passed without
change, which is what the design was meant to achieve: a place the prelude
knows is written by the same function before and after the library is
loaded.

## What is still missing?

Common Lisp also lets you define a writer as a function whose name is a
list, `(defun (setf foo) (new x) ...)`, and an otherwise unknown place
`(foo x)` then calls it. Quokka's function names are symbols only, and
allowing list names means changing how `defun` expands, which is exactly what
this chapter was careful not to do. Until that changes, an unknown place is
an error when the `setf` is expanded, where SBCL would report an undefined
function when the code runs.

When this chapter was written, `char`, `schar`, `subseq`, `get` and
`symbol-plist` were not places, because their readers did not exist yet.
Step J added the first three (chapter 45); symbol property lists still need a
slot on the symbol.

`pushnew` compared with `eq` here instead of `eql`, until step J1 defined
`eql` (chapter 45) and `pushnew` became `adjoin` into a place. And a place
defined with `macrolet` was not recognised, as described above, until step I
gave macros their environment (chapter 44).

---

**References.** The Common Lisp HyperSpec, section 5.1, *Generalized
Reference* (5.1.1.1 for the order of evaluation, 5.1.1.2 for setf
expansions, 5.1.2.7 for macro forms as places), and the dictionary entries
for `define-setf-expander`, `defsetf`, `define-modify-macro` and
`get-setf-expansion`. Paul Graham, *On Lisp* (1993), chapter 12,
*Generalized Variables*, which builds `incf`-style macros the naive way first
and shows exactly where they break. SBCL's `src/code/setf.lisp`, for how a
production implementation takes the shortcut.
