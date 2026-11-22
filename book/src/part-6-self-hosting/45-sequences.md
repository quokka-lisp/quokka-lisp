# Lists, loops and sequences

> **Where this fits.** Chapters 43 and 44 finished the machinery: places,
> all 25 special operators, macros that can see their environment. Most of
> what a Lisp programmer *types* is still missing, though: `dolist`,
> `case`, `mapcar`, `assoc`, `equal`, `find`, `remove`. This chapter is about
> that everyday vocabulary. It is all Lisp, built from the pieces already
> there, and the interesting decisions are about how to write a lot of
> similar functions without either repeating yourself or paying for an
> abstraction at run time. What it unlocks is `format` (step K), and code
> that reads like ordinary Common Lisp.

*Status: J1 (equality, control and iteration, lists) done; J2 (sequence
functions) and J3 (strings) to come. Issue #54.*

## Taking stock

#39 called step J "sequences", meaning CLHS chapter 17: the functions
that work on lists, vectors and strings alike. Before writing any of them,
we asked a world what it actually had. The answer was less than expected:
no `eql` or `equal`, no `dolist`, `case` or `return`, no `mapcar` or
`assoc`, no `>=`. The prelude had been written for one reader, the
compiler, which never needed them.

So step J became three sub-steps, each with its own commit and its own
differential file against SBCL (decision 1): **J1**, the basics, which this
part of the chapter covers; **J2**, the sequence functions; and **J3**,
strings.

## Three kinds of equal

Common Lisp has several equality predicates, and the differences are
precise:

| | compares | `"ab"` vs `"ab"` | `'(1)` vs `'(1)` | `#\a` vs `#\A` |
| --- | --- | --- | --- | --- |
| `eq` | identity: the same object | NIL | NIL | NIL |
| `eql` | `eq`, plus numbers and characters by value | NIL | NIL | NIL |
| `equal` | structure: conses and strings by content | T | T | NIL |
| `equalp` | looser: case-blind characters, vectors by element | T | T | T |

`eql` exists because of **boxing**. A number too big for a word (a bignum)
or a float has to live in the heap as an object, so two equal numbers can
be two different objects, and `eq` would say they differ. `eql` compares
such numbers by value. Quokka has no boxed numbers yet: every integer is a
fixnum held inside the word, and so is every character. On everything
Quokka can make, `eql` and `eq` give the same answer:

```lisp
(defun eql (a b) (eq a b))
```

Waiting for bignums looked tempting, but `eql` is the default test of
almost every function in this step (decision 2). Writing them all with
`eq` would mean finding every one again in step L. Defining `eql` now means
step L changes one function, and everything built on it is right from
then on.

## Loops are a block around a tagbody

Every iteration macro expands into two special operators Quokka has had
since tier 0: `block`, which `return` leaves (chapter 14), and `tagbody`,
which `go` jumps within. Here is `dolist`:

```lisp
(defmacro dolist ((var list-form &optional result) &body body)
  (let ((tail (gensym "TAIL")) (top (gensym "TOP")) (end (gensym "END")))
    `(block nil
       (let ((,tail ,list-form) (,var nil))
         (tagbody
          ,top
            (if (endp ,tail) (go ,end))
            (setq ,var (car ,tail))
            ,@body
            (setq ,tail (cdr ,tail))
            (go ,top)
          ,end)
         (setq ,var nil)
         ,result))))
```

Three details here come straight from the standard:

- **`block nil`** is why `(return x)` works inside any loop. `return` is just
  `(return-from nil x)`, and every iteration macro opens a block named
  `nil`.
- **The body is spliced into the `tagbody`**, not wrapped in a `progn`. CL
  says the body of `dolist` *is* an implicit tagbody, so it may contain
  tags of its own and `go` to them. The differential file has a loop that
  skips an element with `(go skip)`.
- **The variable is bound once and assigned on each pass**, rather than
  bound fresh each time. The standard allows either. The difference only
  shows if a closure captures the variable, and the tests don't depend on
  it.

`do` adds stepping: `(do ((i 0 (+ i 1))) ((= i 4) result) body)`. Its
steps happen **in parallel**: every new value is computed before any
variable changes. `(do ((a 1 b) (b 2 a)) ...)` swaps `a` and `b` on each
pass. That's `psetq`, and `psetq` on variables is just chapter 43's `psetf`.
`do*` steps in sequence instead, with `setq`. Both are a single helper,
parameterised by the binder and the stepper.

## Taking a list apart

`destructuring-bind` takes a value apart the way a macro takes its
arguments apart:

```lisp
(destructuring-bind (a (b c) &optional (d 4)) '(1 (2 3)) (list a b c d))
; => (1 2 3 4)
```

Quokka already has this exact machinery: `defmacro`'s lambda list is
rewritten (in Rust, chapter 18) into a simple parameter list plus a `let*`
that destructures. The obvious move was to feed that rewrite into a
`lambda`. That fails, and the reason is subtle. The rewrite represents an
absent `&optional` argument by a special *unsupplied marker*, and its `let*`
then replaces the marker with the default. A `lambda` rewrites its own
parameter list a second time, and that second rewrite turns the marker into
`nil` before the first `let*` ever sees it. The default `4` would be lost.

So the compiler gained a five-line form, `%destructuring-lambda`, that
compiles the rewrite's output directly, the same way `defmacro` does. The
library's macro is then one line:

```lisp
(defmacro destructuring-bind (lambda-list expression &body body)
  `(apply (%destructuring-lambda ,lambda-list ,@body) ,expression))
```

## A bug that adding macros made urgent

Step J adds a dozen global macros: `case`, `return`, `do`, `prog1` and
more. That made an old compiler bug matter. In
`(flet ((shout (x) ...)) (shout 1))`, CL calls the local function, because
a local function **shadows** a global macro of the same name. Quokka's
compiler looked for global macros *before* local functions, so the macro
won. Chapter 44 had noted this. With so many new macro names, it could now
happen by accident, so the compiler checks for a local function first. The
library's `macroexpand-1` makes the same check through the environment.

## Lists, and what `:test` means

The CLHS 14 functions are many and mostly simple: `nthcdr`, `last`,
`butlast`, `copy-list`, `nconc`, `mapcar` and its five relatives, `assoc`,
`subst`, `union`, and more. Two things are worth pointing out.

**`:key`, `:test` and `:test-not` mean the same thing everywhere.** One
helper turns them into a single two-argument predicate:

```lisp
(defun %test (test test-not)
  (cond ((and test test-not) (error "both :TEST and :TEST-NOT given"))
        (test test)
        (test-not (lambda (a b) (not (funcall test-not a b))))
        (t #'eql)))
```

The predicate is always called with the item first and the element's key
second, which is the order the standard fixes. `(member 2 '(1 2 3) :test #'<)`
finds 3, because it asks `(< 2 3)`.

**`member` is redefined** (decision 3). The prelude's version compared with
`eq` and took no keywords. The library's takes `:key`, `:test` and
`:test-not` and defaults to `eql`. The compiler calls `member` while it
compiles. A call is looked up by name each time it runs, so after a rebuild
the compiler runs the library's `member`. The compiled code doesn't change,
and the answers don't either, because `eql` is `eq`. The fixpoint tests
confirm this on every run.

**`list-length`** has to return `nil` for a circular list instead of
looping forever. It uses **Floyd's cycle-finding algorithm**, also called
"the tortoise and the hare": one pointer advances two steps for every one
step of the other, and if they ever meet, the list is circular. The
version here is the one the CLHS gives as an example.

## What it cost

- **No Rust**, beyond the new file's entry in `boot::LIBRARY`.
- **The compiler:** `%destructuring-lambda`, and the local-function check.
- **Lisp:** `lisp/library/lists.lisp`, and nine number functions (`<=`,
  `>=`, `/=`, `1+`, `1-`, `evenp`, `oddp`, `max`, `min`) in `numbers.lisp`,
  because the loops needed `>=` and nothing had it.

## What is not here yet

- **J2 and J3**, below once they land.
- **Declarations** at the head of a loop body. `(declare ...)` is not
  understood by the compiler yet, and splicing one into a `tagbody` would be
  wrong anyway.
- **`equalp` on hash tables** compares them by identity, not by content.

---

**References.** The Common Lisp HyperSpec, chapters 5 (*Data and Control
Flow*: `case`, `prog1`, `destructuring-bind`), 6 (*Iteration*: `do`,
`dolist`, `dotimes`) and 14 (*Conses*), and the dictionary entries for
`eql`, `equal` and `equalp`. Robert W. Floyd's cycle-finding algorithm, as
presented in Knuth, *The Art of Computer Programming*, volume 2, exercise
3.1-6.
