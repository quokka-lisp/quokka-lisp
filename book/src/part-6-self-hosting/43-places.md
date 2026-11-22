# Places

> **Where this fits.** Chapter 19 pulled a small `setf` forward into tier 0,
> because without it nothing could write a vector element in words SBCL
> understands. Chapter 21 let `defstruct` teach it new places through a table
> from a reader's name to a writer's name. Chapter 42 made the compiler
> reproduce itself, and made that a test every later change must pass. This
> chapter builds Common Lisp's real `setf`: places anyone can define, and the
> macros that update them — `incf`, `push`, `pop` and their relatives. It is
> the first tier-1 feature that the fixpoint constrains, and most of its
> design comes from that. What it unlocks is step J, sequences, whose
> functions are full of places.

*Status: done, issue #52. Step H of phase 2.*

## What a place is

In most languages an assignment's left-hand side is special syntax: `x = 1`,
`v[i] = 1`, `p.x = 1`. Common Lisp has one operator for all of them, and the
thing on the left is called a **place** (the standard also says *generalized
reference*). A place is written as the form that would *read* it:

```lisp
(setf x 1)                ; a variable
(setf (aref v i) 1)       ; a vector element
(setf (gethash k h) 1)    ; a hash-table entry
(setf (point-x p) 1)      ; a structure slot
```

`setf` is a macro. It looks at the reader, `aref`, and writes the code for
the matching writer, `(%set-aref v i 1)`. A program names what it wants to
change in the same words it would use to read it, and the reader and writer
can never be paired wrongly.

Tier 0's `setf` did that with a fixed list and one table. It could *store* into
a place. It could not *update* one.

## Why a writer's name is not enough

Here is the update everyone writes:

```lisp
(incf (aref v (next-index)))
```

`incf` adds one to a place. The obvious expansion reads the place, adds, and
stores:

```lisp
(setf (aref v (next-index)) (+ (aref v (next-index)) 1))
```

That calls `next-index` twice, so it reads one element and writes another.
`incf` has to evaluate `v` and `(next-index)` **once**, keep the results, and
use them for both the read and the write. To do that it must know how the
place is made: which parts are subforms to evaluate, how to read the place
once they are evaluated, and how to write it. A table that maps `aref` to
`%set-aref` holds none of that.

Common Lisp's answer is the **setf expansion** (CLHS 5.1.1.2).
`get-setf-expansion` takes a place and returns five values:

| | | for `(aref v (next-index))` |
| --- | --- | --- |
| vars | temporaries | `(#:g1 #:g2)` |
| vals | the subforms they are bound to | `(v (next-index))` |
| stores | variables that will hold the new value | `(#:new)` |
| writer | stores it, and returns it | `(%set-aref #:g1 #:g2 #:new)` |
| reader | reads the place | `(aref #:g1 #:g2)` |

Any macro that reads and writes a place now has a recipe: bind the
temporaries, compute the new value from the reader, bind the store, run the
writer. Every subform is evaluated once, left to right, whatever the macro
does in between. Here is `incf` after it has been through that recipe:

```lisp
(let* ((#:g1 v)
       (#:g2 (next-index))
       (#:new (+ (aref #:g1 #:g2) 1)))
  (%set-aref #:g1 #:g2 #:new))
```

The temporaries are **gensyms** (chapter 15), so they cannot capture a name
the program uses. "Stores" is plural because one place,
`(values a b)`, takes several values at once.

Three operators teach `setf` a new place, from simplest to most general:

- `(defsetf name writer)` — the **short form**. `(setf (name args...) v)`
  becomes `(writer args... v)`. This is exactly tier 0's table, under its
  standard name.
- `(defsetf name lambda-list (store) body...)` — the **long form**. The body
  writes the writer form. Its parameters are bound to the *temporaries*, not
  to the place's subforms, so it can mention each one as often as it likes.
- `(define-setf-expander name lambda-list body...)` — the general one. The
  body returns all five values itself.

On top of them, `(define-modify-macro incf (&optional (delta 1)) +)` defines
`incf`: a macro that stores `(+ old-value delta)` back into a place. `decf`
is the same with `-`. `push`, `pop`, `pushnew`, `rotatef`, `shiftf` and
`psetf` are written out by hand, because each uses the five values
differently.

## The constraint: the compiler is compiled with this `setf`

This is where the fixpoint comes in.

The warm rebuild (chapter 41) compiles the prelude and the compiler using
the macros **of the running world**, and the running world has the library
loaded. If the library's `setf` expanded `(setf (gethash k h) v)` the general
way, with temporaries, then C2's compiler would be compiled from different
code than C1's. `the_compiler_reproduces_itself` would fail. Nothing would be
*wrong*: the compiler would just have been compiled in a different
environment from the one its source was written for. Chapter 34 saw this
coming.

The system's own source uses `setf` on three kinds of place: `gethash` twelve
times, `aref` three times, and structure accessors nine times. So the
question was how the new `setf` should sit beside the old one. There were
three answers (#52).

**A separate compilation environment.** The rebuild compiles the system with
the prelude's macros, and user code sees the library's. This is the
principled answer, and SBCL's own build works this way: while SBCL is being
built, its compiler runs in a separate host Lisp and compiles the new system
for a target, keeping the two sets of definitions apart (its source calls
this `SB-XC`, the cross-compiler). But the compiler would have to take a
macro environment as an argument everywhere it looks up a macro. That
changes how the compiler compiles itself, to solve a problem the other
options avoid.

**Uniform expansion everywhere.** Every `setf`, even on `gethash`, goes
through the five values. To keep the fixpoint, the cold build would need it
too, which means moving the whole protocol into the prelude, into tier 0,
where the interpreter runs it at every boot. And every `setf` in the system
would get two or three temporaries, which a plain store never needs. Only a
macro that reads *and* writes needs them. Our compiler has no pass that
removes a temporary used once, so each one costs a local slot and the
instructions to fill it.

**A shortcut, with one table behind both paths.** This is the one chosen.
Carsten's first reaction was against shortcuts, and the question that
settled it was what a shortcut actually costs. It costs **two paths that can
drift apart**: the same place could mean one thing to `setf` and another to
`incf`. The design closes that gap. The prelude's knowledge of places is split
into a function that returns nil for a place it does not know:

```lisp
(defun %setf-place (place value)
  (or (%setf-known-place place value)
      (%setf-unknown-place place)))
```

The library's `setf` asks that function first, and only a place it does not
know goes through the five values:

```lisp
(defun %setf-pair (place value)
  (or (%setf-known-place place value)
      (multiple-value-bind (vars vals stores writer) (get-setf-expansion place)
        ...)))
```

`get-setf-expansion` gets its five values for a known place from **the same
function**, by asking it to write into temporaries:

```lisp
((%setf-known-place place nil)
 (multiple-value-bind (vars vals args) (%temporaries (cdr place))
   (let ((store (gensym "NEW")))
     (values vars vals (list store)
             (%setf-known-place (cons (car place) args) store)
             (cons (car place) args)))))
```

A known place therefore expands exactly as it did, because the same code
writes it. That isn't a matter of care. The fixpoint tests confirm it on
every run. The differential file tests every place under both `setf` and
`incf`.

It turned out that SBCL takes the same shortcut. Ask it for
`(macroexpand-1 '(setf (car x) v))` and you get `(SB-KERNEL:%RPLACA X V)`,
with no temporaries. Ask it for `(get-setf-expansion '(gethash k h))` and you
get the full five values. The standard leaves `setf`'s exact expansion to the
implementation, as long as the order of evaluation is right.

## A bug the order rule found

Writing tests that log the order of evaluation turned up a bug in tier 0. The
prelude expanded `(setf (car PLACE) VALUE)` as:

```lisp
(let ((#:g VALUE))
  (rplaca PLACE #:g)
  #:g)
```

That evaluates VALUE first. CLHS 5.1.1.1 says a place's subforms come first,
then the value, and SBCL agrees. A logging test showed SBCL running
`(PLACE VALUE)` and Quokka running `(VALUE PLACE)`. The fix is two lines in
the prelude: bind the cons first. The fixpoint did not move, because no
system source sets a `car`. Only `%setf-through` itself changed, and it
changed the same way in C1, C2 and C3. Because of the single table, `incf`
and `push` on a `car` get the same order for free.

There is one exception to left-to-right in the standard: `push` evaluates the
item *before* the place's subforms, because the item is written first in
`(push item place)`.

## A place inside a place

The new places are mostly one line each. `cadr`, for example, is a long-form
`defsetf`:

```lisp
(defsetf cadr (x) (new) `(progn (rplaca (cdr ,x) ,new) ,new))
```

`getf` is the interesting one, and it is why `define-setf-expander` exists. A
**property list** (plist) is a list of alternating keys and values,
`(:colour red :size 3)`, and `getf` looks up a key in one. Setting a key the
list already has is easy: change the value in place. Setting a key it does
*not* have puts the new pair on the front, and that makes a new list. So
whatever holds the plist has to be written too:

```lisp
(let ((plist nil))
  (setf (getf plist :colour) 'red)   ; plist must now be (:colour red)
  plist)
```

`plist` is itself a place. It could just as well be `(aref v 0)`, or a
structure slot. `getf`'s expander asks for the inner place's expansion and
writes through it:

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

A table from reader names to writer names can't express "write the new list
back into whatever the list came from". Five values can, because the inner
place's writer is just another form to put into the outer one.

## How an expander is made

`define-setf-expander`'s lambda list is a macro's, with destructuring
included, and Quokka already has a full macro lambda list: `defmacro`'s. So
the expander is made the way `defmacro` makes one, under a name nobody can
write (a fresh, uninterned gensym), and taken back out as a function:

```lisp
(defmacro define-setf-expander (name lambda-list &body body)
  (let ((hidden (gensym "SETF-EXPANDER")))
    (multiple-value-bind (plain environment) (%without-environment lambda-list)
      `(progn
         (defmacro ,hidden ,plain (let ((,environment nil)) ,environment ,@body))
         (%define-setf-expander ',name (%macro-function ',hidden))))))
```

This works because a macro function in Quokka takes the argument forms
(chapter 15's `macroexpand-1` calls `(apply expander (cdr form))`), and that
is exactly what an expander takes.

`&environment` is removed and bound to nil. In full Common Lisp it carries the
**lexical environment**: the local macros (`macrolet`) and symbol macros in
scope where the `setf` appears, so a nested `get-setf-expansion` sees them.
Quokka's `defmacro` has no `&environment` yet, and there is no environment
object to pass. The cost is that a place defined by `macrolet` is not
recognised. That will need revisiting in step I, when `symbol-macrolet`
makes symbols into places too.

## What it cost

- **No Rust** beyond the file's line in `boot::LIBRARY`. About 450 lines of
  Lisp, most of them comments, in `lisp/library/setf.lisp`; a dozen in the
  prelude; and one name in `%library-files`.
- **The image** grows from 359,688 to 397,944 bytes. `define-modify-macro`,
  `defsetf` and `define-setf-expander` each leave compiled code behind for
  every place they define.
- **The fixpoint held first time.** The only rebuild test that failed was
  the one that counts the forms a rebuild reads (373 became 421), because
  the library had grown. The comparison of C1, C2 and C3 passed unchanged.
  That was the point of the design rather than luck: a known place is
  written by the same function before and after the library loads.

## What is not here

- **`(setf foo)` functions.** In Common Lisp, `(defun (setf foo) (new x) ...)`
  defines a writer as a function whose *name* is a list, and an unknown place
  `(foo x)` calls it. Quokka's function names are symbols only. Adding list
  names means changing how `defun` expands, which is the very thing this
  chapter took care not to do. Until then, an unknown place is an error when
  the `setf` expands, where SBCL would report an undefined function when it
  runs.
- **`char`, `schar`, `subseq`, `get` and `symbol-plist` as places.** Their
  readers don't exist yet. Strings and subsequences come with step J, and
  symbol plists need a slot on the symbol.
- **`pushnew` by `eql`** was `eq` here, until step J1 defined `eql`
  (chapter 45) and `pushnew` became `adjoin` into a place.
- **A `macrolet` place**, as above.

---

**References.** The Common Lisp HyperSpec, section 5.1, *Generalized
Reference* (5.1.1.1 for the order of evaluation, 5.1.1.2 for setf
expansions, 5.1.2.7 for macro forms as places), and the dictionary entries
for `define-setf-expander`, `defsetf`, `define-modify-macro` and
`get-setf-expansion`. Paul Graham, *On Lisp* (1993), chapter 12,
*Generalized Variables*, which builds `incf`-style macros the naive way first
and shows exactly where they break. SBCL's `src/code/setf.lisp`, for how a
production implementation takes the shortcut.
