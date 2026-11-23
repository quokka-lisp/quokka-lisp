# The rest of a symbol

> *Where this fits.* Chapter 5 laid out a symbol as a small heap object with
> a name, a value cell, a function cell, a property list and a home, and
> chapters since have used the first three and the home. The property list
> was made empty with every symbol and never read again. This chapter, the
> second worked from ansi-test's list (chapter 49), gives Lisp the rest of
> what a symbol holds: its properties, a way to ask whether its function
> cell is full and to empty it, the global or local macro behind a name, and
> constants, which the compiler now refuses to let a program rebind.

*Status: done, issue #63. Step P of phase 2.*

## What is a property list for?

A **property list** is a list of alternating keys and values, `(colour red
size 3)`, and every symbol has one. A program uses it to hang information on
a name without declaring anything first: `(setf (get 'apple 'colour) 'red)`
and later `(get 'apple 'colour)`. It is one of the oldest ideas in Lisp; in
the first Lisp a symbol's value and function were themselves kept on its
property list, and Common Lisp separated them into cells and left the list
for everything else.

Quokka's symbols had the slot from the start (chapter 5) and nothing could
read it. We gave Lisp two primitives, `symbol-plist` and `%set-symbol-plist`,
and wrote `get`, `remprop` and `get-properties` in Lisp on top of the `getf`
code chapter 43 already had. The alternative was a Lisp hash table from
symbol to list, with no Rust, but it would have duplicated a slot that
exists, and since Quokka has no weak tables it would have kept every symbol
that ever had a property alive.

The biggest number on ansi-test's list, 997 tests behind `symbol-plist`,
came from one helper that reads a symbol's property list and then walks it
with `loop`. With `symbol-plist` those tests stop needing a tier-1 feature
and start needing `loop`, which is tier 2. They count as explained, which is
what phase 2's gate asks, but they will not pass until `loop` exists.

## Is a name a function?

`fboundp` asks whether a name has a function, `fmakunbound` takes it away,
and `functionp` asks whether an object is one. Each needs something only Rust
can do. Taking a function away means writing the unbound marker back into
the cell, a value Lisp never holds, for the same reason `makunbound` became a
primitive in chapter 44. And telling a function from a macro, or a closure
from a vector, means reading the object's header. We added three primitives
of a few lines each. We could have written `fboundp` in Lisp by calling
`symbol-function` and catching the error it signals for an undefined name,
but that throws an exception to answer a yes-or-no question.

## Which macro is meant here?

`macro-function` returns the expander behind a name. A primitive already
found global macros. Common Lisp's version also takes the environment a macro
is expanding in, so that a local `macrolet` macro hides a global one, and the
compiler has handed macros that environment since chapter 44.

The interesting part was the calling convention. A Common Lisp macro function
takes two arguments, the whole form and the environment: `(funcall
(macro-function 'when) '(when x y) nil)` returns the expansion. Quokka's own
macro objects are called with the form's arguments spread out, the way
`defmacro`'s lambda list binds them. So `macro-function` hands back a small
function that converts one convention to the other, and `(setf
macro-function)` wraps the other way, turning a two-argument function into a
macro object that puts the form back together before calling it.

## Constants, and refusing to rebind them

`defconstant` defines a variable whose value never changes. Our first
proposal was to record that and do nothing more, since CLHS leaves the
consequences of rebinding a constant undefined. We decided instead to refuse
it, as SBCL does: the compiler now keeps a table of constant names beside its
table of special variables (chapter 35), and a `let`, `let*`, lambda
parameter, `setq` or `symbol-macrolet` that names a constant, `t`, `nil` or a
keyword fails to compile with a `program-error`.

The ratchet found the first mistake. One test that had passed now failed:
`macrolet.39` defines a local macro whose lambda list is `(())`, in which the
empty list is a destructuring pattern that matches only an empty list.
Quokka's macro lambda-list rewrite turns that pattern into a parameter named
`nil`, and the new check refused it. The check belonged on ordinary lambda
lists only, where `(lambda (nil) …)` really is an error, and that is where it
now is.

`constantp` is true of numbers, strings and the other self-evaluating
objects, of `(quote x)`, of keywords, `t` and `nil`, and of names
`defconstant` defined. SBCL also says `(+ 1 2)` is constant, because it can
fold it; CLHS allows that and does not require it, and Quokka does not fold.

## What did it cost?

This step took 54 lines of Rust, against our estimate of 30: five primitives, their table
rows and their match arms. The core is now 14,811 lines of the 15,000 guide.
The rest is Lisp: a new library file, `lisp/library/symbols.lisp`, with the
property lists, `fdefinition` and `macro-function`; `defconstant` and
`constantp` beside `defvar` in `specials.lisp`; and about twenty lines in the
compiler for the table of constants and the refusals. The fixpoint, our
check that the compiler compiles itself into exactly the same code
generation after generation (chapter 42), held: the compiler's own source
binds no constant. `tests/lisp/symbol-cells.lisp` checks all four parts
against SBCL, taking care not to ask what CLHS leaves open, such as which
true value `remprop` returns.

ansi-test went from 5,219 passing tests to 5,382, of 12,324, and the tests
needing a tier-1 feature from 2,963 to 1,752.

## What is still missing?

Functions named `(setf foo)` do not exist, so `fdefinition` takes symbols
only. A macro object called as a function, as a `progn` might do when it
defines a macro and uses it in the same form, returns the expansion instead
of signalling an error. Next on ansi-test's list: `with-standard-io-syntax`,
`compile`, and the package functions.

---

**References.** The Common Lisp HyperSpec, chapter 10 (*Symbols*), the
entries for `fboundp`, `fmakunbound`, `fdefinition`, `macro-function`,
`functionp`, `defconstant` and `constantp`, and 3.1.2.1.1.3 on constant
variables. John McCarthy and others, *LISP 1.5 Programmer's Manual*, 1962,
for property lists as the first Lisp kept them.
