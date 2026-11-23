# Lists, loops and sequences

> *Where this fits.* Chapters 43 and 44 finished the machinery: places, all
> 25 special operators, and macros that can see their environment. But most
> of what a Lisp programmer actually types was still missing: `dolist`,
> `case`, `mapcar`, `assoc`, `equal`, `find`, `remove`. This chapter adds that
> everyday vocabulary. All of it is Lisp, built from pieces already there,
> and the interesting decisions are about how to write a great many similar
> functions without repeating ourselves and without paying for an
> abstraction at run time. With it in place, Quokka code reads like ordinary
> Common Lisp, and `format` (step K) has what it needs.

*Status: done, issue #54, in three sub-steps: J1 (equality, control and
iteration, lists), J2 (sequence functions) and J3 (strings).*

## Taking stock

#39 called step J "sequences", meaning CLHS chapter 17: the functions that
work on lists, vectors and strings alike. Before writing any of them, we
asked a running world what it already had, and the answer was less than we
expected. There was no `eql` or `equal`, no `dolist`, `case` or `return`, no
`mapcar` or `assoc`, and not even `>=`. The prelude had been written for one
reader, the compiler, which had never needed them.

So we split step J into three sub-steps, each with its own commit and its own
differential file against SBCL (decision 1 in #54): J1 for the basics, J2 for
the sequence functions, and J3 for strings.

## Which kind of equal?

Common Lisp has several equality predicates, and the differences between
them are precise:

| | compares | `"ab"` vs `"ab"` | `'(1)` vs `'(1)` | `#\a` vs `#\A` |
| --- | --- | --- | --- | --- |
| `eq` | identity: the same object | NIL | NIL | NIL |
| `eql` | `eq`, plus numbers and characters by value | NIL | NIL | NIL |
| `equal` | structure: conses and strings by content | T | T | NIL |
| `equalp` | looser: case-blind characters, vectors by element | T | T | T |

`eql` exists because of **boxing**. A number too large for one word (a
bignum), or a float, has to live in the heap as an object of its own, so two
equal numbers can be two different objects that `eq` calls different. `eql`
compares such numbers by value. Quokka has no boxed numbers yet: every
integer is a fixnum held inside the word, and so is every character. On
everything Quokka can make, `eql` and `eq` give the same answer:

```lisp
(defun eql (a b) (eq a b))
```

We were tempted to wait for bignums before defining `eql` at all. But `eql`
is the default test of almost every function in this step (decision 2), and
writing them all with `eq` would have meant finding each one again in step
L. Defined now, `eql` is one function that step L changes, and everything
built on it becomes right at the same time.

## How is a loop built?

Every iteration macro expands into two special operators Quokka has had
since tier 0: `block`, which `return` leaves, and `tagbody`, which `go` jumps
within (both in chapter 14). Here is `dolist`:

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

Three of its details come straight from the standard. The `block nil` is why
`(return x)` works in any loop: `return` is just `(return-from nil x)`, and
every iteration macro opens a block named `nil`. The body is spliced into the
`tagbody` rather than wrapped in a `progn`, because CL says the body of
`dolist` *is* an implicit tagbody, which may contain tags of its own; the
differential file has a loop that skips an element with `(go skip)`. And the
variable is bound once and assigned on each pass, rather than bound afresh
each time. The standard allows either, the difference only shows when a
closure captures the variable, and the tests do not depend on it.

`do` adds stepping: `(do ((i 0 (+ i 1))) ((= i 4) result) body)`. Its steps
happen in parallel, every new value being computed before any variable
changes, so `(do ((a 1 b) (b 2 a)) ...)` swaps `a` and `b` on each pass. That
is `psetq`, and `psetq` on variables is just chapter 43's `psetf`. `do*`
steps one variable after another with `setq`. Both are one helper,
parameterised by the binder and the stepper.

## Taking a list apart

`destructuring-bind` takes a value apart the way a macro takes its arguments
apart:

```lisp
(destructuring-bind (a (b c) &optional (d 4)) '(1 (2 3)) (list a b c d))
; => (1 2 3 4)
```

Quokka already had the machinery for this. A `defmacro` lambda list is
rewritten, in Rust (chapter 18), into a simple parameter list plus a `let*`
that does the destructuring. Our first idea was to feed that rewrite into a
`lambda`, and it failed in a subtle way. The rewrite represents an
`&optional` argument that was not supplied by a special *unsupplied marker*,
and its `let*` replaces the marker with the default. A `lambda` rewrites its
own parameter list a second time, and that second rewrite turns the marker
into `nil` before the first `let*` ever sees it, so the default `4` was lost.

So we gave the compiler a five-line form, `%destructuring-lambda`, which
compiles the rewrite's output directly, the way `defmacro` does. The
library's macro is then one line:

```lisp
(defmacro destructuring-bind (lambda-list expression &body body)
  `(apply (%destructuring-lambda ,lambda-list ,@body) ,expression))
```

## Fixing a bug the new macros made urgent

Step J adds a dozen global macros, among them `case`, `return`, `do` and
`prog1`, and that made an old compiler bug matter. In
`(flet ((shout (x) ...)) (shout 1))` Common Lisp calls the local function,
because a local function **shadows** a global macro of the same name.
Quokka's compiler looked for global macros before local functions, so the
macro won. Chapter 44 had noted the gap; with so many new macro names it
could now happen by accident, so the compiler checks for a local function
first, and the library's `macroexpand-1` makes the same check through the
environment.

## What does `:test` mean?

The functions of CLHS chapter 14 are many and mostly simple: `nthcdr`,
`last`, `butlast`, `copy-list`, `nconc`, `mapcar` and its five relatives,
`assoc`, `subst`, `union` and more. Most of them take `:key`, `:test` and
`:test-not`, and these mean the same thing everywhere, so one helper turns
them into a single two-argument predicate:

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

`member` itself is redefined (decision 3). The prelude's version compared
with `eq` and took no keywords; the library's takes `:key`, `:test` and
`:test-not` and defaults to `eql`. The compiler calls `member` while it
compiles, and a call is looked up by name each time it runs, so after a warm
rebuild (chapter 41) the compiler runs the library's `member`. Its compiled
code does not change, and neither do its answers, since `eql` is `eq`. The
fixpoint, our check that the compiler compiles itself into exactly the same
code generation after generation (chapter 42), confirms this on every run.

`list-length` has to return `nil` for a circular list instead of looping
forever. It uses **Floyd's cycle-finding algorithm**, also known as "the
tortoise and the hare": one pointer moves two steps for every one step of
the other, and if they ever meet, the list is circular. Ours is the version
the CLHS gives as an example.

## How do you write fifty functions once?

The functions of CLHS chapter 17 (`find`, `position`, `count`, `remove`,
`substitute`, `search`, `sort` and the rest) each take any sequence, and
mostly the same keywords. We saw three ways to write them (decision 4): each
function twice, once for lists and once for vectors; once each, over a
run-time protocol that hands out the next element; or once each, over a
**macro** that writes the list version and the vector version at compile
time.

What decided it was a question we asked ourselves along the way: what would
it take to make Quokka inline calls, so that the run-time protocol was cheap?
Quite a lot, it turned out. It would need a table of inline declarations, the
saved source of each inline function, **let-conversion** of
`((lambda (x) body) a)` into a `let`, and above all **capture safety**: a
body pasted into a call site must not have its free names captured there,
which needs a code walker that expands every macro first. A protocol that
passes small functions around would need repeated simplification passes on
top of that. That is an optimising compiler, and a project for when a profile
asks for one. A macro, on the other hand, already is compile-time inlining,
and its hygiene problem was solved long ago with gensyms (chapter 15). SBCL
writes its own sequence functions this way, with macros it calls
`seq-dispatch`.

So there is one macro, `%do-sequence`, and every function is written once
over it:

```lisp
(defun %find (matches sequence from-end start end)
  (%do-sequence (element sequence :start start :end end :from-end from-end)
    (if (funcall matches element) (return element))))
```

The macro expands into four loops: forwards and backwards, over a list by
its tails or over a vector by its indices. A list cannot be walked
backwards, so for `:from-end` it first gathers the range, each element paired
with its index, in reverse. The specialising happens when the macro expands,
so no element costs a call. What it costs instead is code size, since the
body appears four times in every function that uses it; the image grew from
474 KB to 565 KB. That is the trade decision 4 made, and an inliner would
have made the same one.

The three flavours of each function are the other repetition. `find`,
`find-if` and `find-if-not` differ only in what counts as a match, so each
builds a one-argument predicate and calls one worker, `%find`.

Removal marks first and builds afterwards. `(remove 1 '(1 2 1 3 1) :count 2
:from-end t)` has to remove the *last* two ones, which is awkward in a single
forward pass. So `%marks` walks in the requested direction and flags matching
indices in a vector until `:count` runs out, and a second pass builds the
result from the unflagged elements, in order. `substitute` uses the same
marks and replaces instead of skipping.

Sorting is merge sort: split the sequence in half, sort each half, and merge
the two (John von Neumann, 1945). It takes O(n log n) time in every case, and
it is **stable**, meaning that elements that compare equal keep their
original order. That is the property `stable-sort` promises and `sort` is
allowed, but not required, to have, so both are the same function here. The
tests only give `sort` distinct keys, since with equal keys SBCL is free to
differ.

## Avoiding a near miss

The first draft of `sequences.lisp` had a helper called `%position`. The
compiler already has a `%position`, which `%lookup` uses to find a variable
in a scope, and all of Quokka's `%` names live in one package, `QUOKKA`. The
library's definition would have silently replaced the compiler's, and the
compiler would have broken the next time it ran. A search for clashes before
the first build caught it, and the library's helper is now `%position-of`.
Nothing structural prevents this from happening again yet; a check that a
library `defun` never replaces a system function, apart from the deliberate
redefinitions of decision 3, would.

## Comparing strings in one walk

A string is a vector of characters, so J2 already covers most of what
programs do with strings. J3 adds what is particular to text: case,
trimming, and comparison.

There are twelve string comparisons: `string=`, `string<` and four more, and
the same six ignoring case (`string-equal`, `string-lessp` and so on). All of
them come down to one walk, which finds the first position where the two
strings differ and notes which side was smaller there, or which ran out
first:

```lisp
(defun %string-compare (a b start1 end1 start2 end2 key)
  ...
  (values i (cond ((and (>= i e1) (>= j e2)) :equal)
                  ((>= i e1) :less)
                  (t :greater))))
```

Each comparison is then one line, generated by a small macro, which answers
when the order is one it accepts. The answer is not just `t`:
`(string< "ab" "abc")` returns `2`, the position where the strings differ.
That is a **generalised boolean**, where any non-nil value means true, so it
can carry information as well. Only the two equality tests return a plain
`t`.

Ignoring case means folding both characters to lower case first, as SBCL
does. CLHS leaves open how a letter compares with a character that sits
between the upper-case and lower-case letters in the code table, such as `[`
or `_`; folding up or down gives different answers there, and our tests stay
off that ground.

Each of these functions takes a **string designator**: a string, a symbol
(standing for its name), or a character (standing for a string of one).
`string` turns any of them into a string. It used to live in the printer,
and we moved it here, where every function uses it.

## What did it cost?

Step J needed no Rust beyond the new files' entries in `boot::LIBRARY`. The
compiler gained `%destructuring-lambda` and the check for local functions.
The Lisp is three files: `lisp/library/lists.lisp`;
`lisp/library/sequences.lisp`, which also redefines `reverse` (the prelude's
turned a string into a general vector) and `elt` (to check its bounds); and
`lisp/library/strings.lisp`, which redefines `char=`, `string=` and
`make-string` with CL's full signatures. Nine number functions (`<=`, `>=`,
`/=`, `1+`, `1-`, `evenp`, `oddp`, `max`, `min`) went into `numbers.lisp`,
because the loops needed `>=` and nothing had it.

## What is still missing?

The guard against a library name replacing a system one, described above,
does not exist yet. Some of the algorithms are the plainest that work:
`search` tries every position and `remove-duplicates` compares every pair,
both quadratic, which is plenty until a profile says otherwise. Declarations
at the head of a loop body are not understood, because the compiler does not
know `(declare ...)` yet, and splicing one into a `tagbody` would be wrong
anyway. And `equalp` compares hash tables by identity, not by their
contents.

---

**References.** The Common Lisp HyperSpec, chapters 5 (*Data and Control
Flow*: `case`, `prog1`, `destructuring-bind`), 6 (*Iteration*: `do`,
`dolist`, `dotimes`) and 14 (*Conses*), and the dictionary entries for
`eql`, `equal` and `equalp`; chapter 17 (*Sequences*), especially 17.1.1 on
bounding indices; chapters 13 (*Characters*) and 16 (*Strings*). Knuth, *The Art of Computer Programming*, volume 3, section
5.2.4, on merge sort. Robert W. Floyd's cycle-finding algorithm, as
presented in Knuth, *The Art of Computer Programming*, volume 2, exercise
3.1-6.
