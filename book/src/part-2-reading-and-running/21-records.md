# Records, and a macro that writes ten functions

> **Where this fits.** This is the last chapter of tier 0, and it is the one
> that spends everything. `defstruct` needs `&key` lambda lists from chapter
> 18, vectors from chapter 19, `gensym` from chapter 15, a hash table from
> chapter 19, `setf` from chapter 19 and `error` from chapter 20 — and it
> needs `setf` to grow a feature it did not have. A macro that needs six
> previous chapters is a good test of whether they were built right.

## One line, ten definitions

```lisp
(defstruct point x y)
```

From that, a program gets:

```lisp
(make-point :x 1 :y 2)      ; a constructor taking keywords
(point-x p) (point-y p)     ; an accessor per slot
(setf (point-x p) 3)        ; each one settable
(point-p p)                 ; a predicate
(copy-point p)              ; a copier
```

Ten definitions, each of them a `defun` the user could have typed. That is
what makes `defstruct` the standard demonstration of why macros matter: it is
not sugar over an expression, it is a *program that writes a program*, and the
program it writes is as long as the number of slots.

It is also the single most useful thing in tier 0 for what comes next. A
compiler represents nearly everything it works with as a record — a variable
binding, a lexical environment, a basic block, an instruction — and without
`defstruct` each of those is an untyped list that the whole compiler agrees
to interpret the same way.

## What a structure is here

A vector, with the type name in slot 0:

```lisp
(make-point :x 1 :y 2)   ; is really  #(POINT 1 2)
```

Common Lisp's is a genuine type. `typep` knows it, the printer shows
`#S(POINT :X 1 :Y 2)`, `:include` gives single inheritance, and the compiler
can lay the slots out however it likes. All of that needs a class system,
which is phase 3.

The gap has two visible consequences, and both are written down rather than
discovered:

**A structure prints as a vector.** So `tests/lisp/structures.lisp` never
prints one — it asks accessors and predicates, which both implementations
agree about. This is the fourth time a feature has turned out to be partly
untestable against the oracle, and the first where the cause is a missing
*type system* rather than a missing name.

**The predicate is generous.** `point-p` checks for a vector whose first
element is the symbol `POINT`, so a vector someone else built that way would
pass. SBCL would say no. The test file does not ask, and the prelude says why.

## The accessor problem, which `setf` could not solve

Chapter 19 built a `setf` that knows a fixed list of places: variables,
`aref`, `gethash`, `symbol-function`, `car`, `cdr`. `defstruct` immediately
breaks that, because `(setf (point-x p) 3)` names a place the prelude has
never heard of and cannot have heard of — the whole point is that the user
invents it.

So `setf` needs to be *taught*. The mechanism here is a table from a reader's
name to a writer's name:

```lisp
(setq %setf-setters (make-hash-table))

(defun %define-setf-writer (reader writer)
  (setf (gethash reader %setf-setters) writer))
```

and one more clause in the expander:

```lisp
(t (let ((setter (gethash head %setf-setters)))
     (if setter
         (cons setter (append args (list value)))
         (%setf-unknown-place place))))
```

`defstruct` then registers `point-x → %set-point-x` as part of its expansion,
and `setf` can write a place that did not exist when `setf` was written. This
is the first use of the hash tables from chapter 19 by anything other than a
test, which is a reasonable sign they were worth building.

### Why Common Lisp's mechanism is bigger

`define-setf-expander` does not hand back a function name. It hands the
*macro* five values: temporary variables, the forms to bind them to, a store
variable, a form that writes, and a form that reads.

That looks like over-engineering until you want `incf`:

```lisp
(incf (aref v (next-index)))
```

`incf` must **read** the place and then **write** it, and `(next-index)` must
be evaluated exactly once. A reader-to-writer table cannot express that — it
would expand into something that called `next-index` twice, which is a bug
that only appears when a subform has an effect. So Quokka has no `incf`,
`decf`, `push` or `pop`, and will not until the real expander exists.

The table is not a worse version of `define-setf-expander`. It is the part of
it that `defstruct` needs, which is a different claim and an honest one.

## Making a name out of two names

```lisp
(defun %symbol-concat (a b)
  (%intern (%string-append (symbol-name a) (symbol-name b))))
```

`POINT` and `-X` become `POINT-X`. Symbols are interned by name (chapter 8),
so *making* a symbol out of two others is making a string and interning it —
which is why `%intern` and `%string-append` had to be added. Common Lisp
spells them `intern` and `concatenate`, and both take arguments Quokka cannot
express yet: `intern` takes a package, `concatenate` takes a sequence type. So
they are `%` names until the real ones can exist, which is the same bargain as
everywhere else.

Note what this means: `defstruct` is a macro that **creates symbols that were
never read from any source file**. Nothing in `point.lisp` contains the text
`POINT-X`, and after the macro runs it is an ordinary interned symbol with a
function cell. A reader, an obarray and a macro system are between them enough
to invent new names at compile time, and that is most of what a code generator
does.

## Three apologies withdrawn

Chapter 18 said destructuring did not check arity, and chapter 19 said `setf`
reported a bad place by calling an undefined function. Both were waiting on
chapter 20:

```lisp
(exactly-two (1))      ; now: a macro call is missing arguments
(setf (bogus x) 1)     ; now: SETF does not know how to set this place
(setf x)               ; now: SETF wants place and value in pairs
```

Each had been written as a placeholder with a note saying it would become a
real `error` once there was something to signal into. That is a pattern worth
naming: when a thing cannot be done properly yet, do the crude version, say in
the code that it is crude and what would fix it, and then — this is the part
that usually does not happen — *go back and fix it when the blocker clears*.
The notes are what made going back possible.

## Tier 0 is finished

Almost. `macrolet`, `multiple-value-call` and `multiple-value-prog1` remain,
and the last two need a second value register, which is a small change to the
machine rather than a new idea.

What exists is enough to interpret a compiler: twenty-two of the
twenty-five special operators that matter, macros with full lambda lists,
vectors, strings, hash tables, structures, `setf` and a condition system — and
a prelude where most of it is written in Lisp rather than Rust.

Which means the next thing is phase 1, and the gate is the sentence this book
opened with: the interpreter runs the compiler, the compiler produces an
image, and the image contains a compiler that no longer needs the interpreter.

---

**References.** CLHS 3.4.6 for `defstruct` in full — the option list alone is
longer than this chapter. *Common Lisp the Language, 2nd edition*, §7.2, for
`setf` expanders and exactly why five values are needed. Paul Graham, *On
Lisp*, chapter 12, on places and why they are harder than they look.
