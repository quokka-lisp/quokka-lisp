# Who uses what

> *Where this fits.* Chapter 84 made a world keep its definitions: their
> forms, their text, and the versions they replaced. This chapter has the
> world keep how they depend on each other, which functions a definition
> calls and which macros it expanded, so that it can say who uses a name and
> what a changed macro has left out of date, and then recompile those
> definitions from the forms it kept. It finishes phase 3's step C; the
> debugger of step F, and the environment's inspector after it, ask these
> questions.

*Status: done, issue #55. Phase 3, step C, sub-steps 2 and 3.*

## A macro changes and its callers do not

Here is a macro and a function that uses it:

```lisp
(defmacro bump (x) `(+ ,x 1))
(defun next (n) (bump n))
(next 1)                        ; 2
```

Now change the macro:

```lisp
(defmacro bump (x) `(+ ,x 100))
(next 1)                        ; still 2
```

`next` still adds one. A macro is not called when `next` runs; it was
expanded when `next` was compiled, and `next`'s code holds the old
expansion. Every Lisp behaves like this, and in a file-based workflow the
answer is to reload the files, which recompiles everything. In a world that
is the truth, as #126 decided Quokka's is, there may be no file to reload,
and reloading everything to fix one function is a poor answer anyway. The
world has the form of `next` (chapter 84); what it lacked was the knowledge
that `next` expanded `bump`.

## The compiler takes notes

The compiler already passes the two places that matter. When it compiles a
call of a global function, it emits an instruction that fetches the
function from the symbol at run time, and when it meets a global macro, it
expands it. Both now note the name, if someone asked:

```lisp
(defun %note-reference (kind name)
  (if %references
      (if (eq kind ':macro)
          (if (member name (car %references))
              nil
              (rplaca %references (cons name (car %references))))
          (if (member name (cdr %references))
              nil
              (rplacd %references (cons name (cdr %references)))))
      nil))
```

`%references` is a global of the compiler, like the rest of its state,
because the compiler is tier-0 Lisp, which has no special variables. `eval`
sets it to a fresh pair around a defining form, puts it back afterwards
whatever happens, and gives the definition what was noted. The notes change
nothing the compiler produces, so the fixpoint, our check that the compiler
compiles itself into exactly the same code generation after generation
(chapter 42), holds without help.

With the notes kept, the world can answer two questions. We used SBCL's
names for them, from its `sb-introspect` module:

```lisp
(who-calls 'next)              ; the definitions that call NEXT
(who-macroexpands 'bump)       ; (#<DEFINITION :FUNCTION NEXT>)
```

## Stale, not recompiled at once

When a macro is defined, every definition that expanded it is marked
*stale*: its code is older than what its form now means. So is every
definition that called the name as a function, which matters when a
function is written before the macro it uses: compiled then, it called a
function that did not exist, and now it should expand a macro.

We considered recompiling stale definitions at once, as soon as the macro
changes, and decided against it. A program changing a set of macros
together would recompile everything after each one, half of it against a
set of macros that was not finished. Genera's answer was a command,
*Compile Changed Definitions*, which a person runs when they are ready, and
Smalltalk's `compileAll` is the same idea. Quokka's is
`recompile-stale-definitions`.

## Recompiling

Recompiling a definition evaluates its kept form again, in the package it
was read in, as a new version that keeps the same text and place:

```lisp
(stale-definitions)              ; (#<DEFINITION :FUNCTION NEXT>)
(recompile-stale-definitions)
(next 1)                         ; 101
```

Stale definitions are recompiled in the order they were first made, so that
a macro is recompiled before the functions that use it. Recompiling a macro
marks its users stale again, so the loop goes round until nothing is stale,
but it recompiles no definition twice: a macro that uses itself would
otherwise go round for ever.

## What did it cost?

No Rust. A dozen lines in the compiler, which note references and decide
nothing, and about a hundred in `definitions.lisp`. A Rust test checks the
cross-reference, the marking and the recompiling, since none of it is
Common Lisp's and SBCL cannot be the oracle.

## What is still missing?

The cross-reference covers functions called by name and macros expanded;
it does not record variables read or set, or functions inlined, since the
compiler inlines none. A function passed as a value, `(mapcar 'f ...)`, is
not a call the compiler can see. And the index is a walk over every
definition, which is quick for the hundreds a person types; the
environment's who-calls index (#126) keeps it up to date as definitions are
installed, when there are thousands.
