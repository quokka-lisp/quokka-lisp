# The small functions

> *Where this fits.* After the arrays of chapter 55, the gaps at the top of
> ansi-test's list (chapter 49) were no longer large features but a long
> row of small ones: `parse-integer`, `prog`, `ccase`,
> `multiple-value-setq`, the four-letter `c…r` accessors, a few character
> and symbol functions, the inverse hyperbolic functions, and global symbol
> macros. Each is a few lines. This chapter is about the three that hid a
> decision, and about what a small function can still teach: how a family of
> twenty-three functions can be written once, why an oracle cannot be
> trusted on the last bit of a logarithm, and why the fixpoint (chapter 42)
> keeps surviving changes to the compiler.

*Status: done, issue #68. Step U of phase 2.*

## How do you write twenty-three functions once?

Common Lisp has an accessor for every path of two, three or four `car`s and
`cdr`s through a list: `cadr` is the car of the cdr, `cdadar` the cdr of the
car of the cdr of the car. Each is also a `setf` place, so `(setf (cadar x)
'new)` replaces that element. Quokka had the two-letter ones and some of the
three-letter ones, and ansi-test wanted the rest.

The name says everything about the function: read the letters between `C`
and `R` from left to right, and each `A` is a `car` and each `D` a `cdr`, the
outermost first. So one macro writes both the function and its `setf`
expander from the name alone:

```lisp
(defmacro %define-cxr (symbol)
  (let* ((name (symbol-name symbol))
         (end (- (length name) 1)))
    `(progn
       (defun ,symbol (x) ,(%cxr-reader name 1 end 'x))
       (defsetf ,symbol (x) (new)
         (list 'progn
               (list ',(if (eql (aref name 1) #\A) 'rplaca 'rplacd) ,(%cxr-builder name 2 end 'x) new)
               new)))))
```

The function is the nested calls; the place stores with `rplaca` or
`rplacd`, chosen by the first letter, into the cons the remaining letters
reach. Twenty-three lines of `(%define-cxr cadar)` follow it. The macro runs
while `lists.lisp` is loading, so it may use only what is defined before it
in that file and in the prelude: `aref` on the name rather than `char`, and
recursion rather than `dotimes`, which `lists.lisp` defines further down.

## What should `ccase` do without restarts?

`ccase` is `ecase` with a correctable error: when no clause matches, it
offers a *restart*, a way for the person at the keyboard (or a handler) to
store a new value in the place and run the `case` again. Restarts belong to
the full condition system, which is tier 2. We decided to make `ccase` act
as `ecase` until then: a program using it works whenever a key matches,
which is nearly always, and the restart arrives later without changing
anything else.

Writing it turned up a small mistake in `ecase`. When nothing matched, it
signalled a plain error; CLHS says a `type-error`, and SBCL's `case-failure`
is one. Both macros signal a `type-error` now.

## Why not compute `atanh` from `log`?

`asinh`, `acosh` and `atanh` can be written in a line each from logarithms
and square roots, `asinh x = log(x + √(x² + 1))`, and that was the
Rust-free option. We chose instead to add three primitives over the `libm`
crate, as `sinh` and its kin already were (chapter 48), because the
formulas lose precision exactly where they are used most. For a tiny `x`,
`x + √(x² + 1)` is 1 plus a few bits, and `log` of a number that close to 1
throws most of them away; a library implementation switches to another
formula there.

Then the tests disagreed with SBCL anyway. `asinh` and `acosh` matched digit
for digit, but SBCL on macOS answered `(atanh 0.5d0)` with
0.5493061443340549 and `libm` with 0.5493061443340548. The exact value is
½ ln 3 = 0.549306144334054845…, and `libm`'s answer is the double nearest
to it. SBCL calls the operating system's C library, which rounds this one
case up by a unit in the last place, and CI runs SBCL on Linux with yet
another C library. Most C libraries promise only to be within one unit of
the right answer, so two honest implementations can disagree in the last
bit. The test checks `atanh` to within one unit instead of comparing digits.

## Where does a global symbol macro live?

`define-symbol-macro` makes a name stand for a form everywhere, unless a
lexical binding hides it: after `(define-symbol-macro head (car *cell*))`,
reading `head` reads the car and `(setf head 'x)` writes it. The local kind,
`symbol-macrolet`, has lived in the compiler's environment since chapter 44,
found by a function that walks the scopes outward. We put the global ones in
a table the compiler owns, and the walk consults it when it runs out of
scopes:

```lisp
(defun %symbol-macro (name env)
  (cond ((null env) (%global-symbol-macro name))
        ((eq (car (car env)) ':symbol-macro)
         (or (%assq name (cdr (car env))) (%symbol-macro name (cdr env))))
        ((and (eq (car (car env)) ':variable) (%position name (cdr (car env)) 0)) nil)
        ((and (eq (car (car env)) ':special) (member name (cdr (car env)))) nil)
        (t (%symbol-macro name (cdr env)))))
```

Because the global table is reached only past the outermost scope, any
`let` of the same name hides it, as CLHS requires. And it is the third time
the same argument has kept the fixpoint, our check that the compiler
compiles itself into the same code in C1, C2 and C3: compiler macros
(chapter 53) and now global symbol macros are things the compiler applies,
and the tables they live in are empty whenever the compiler compiles
itself. A library symbol macro named after a variable the compiler reads
would break it, and the comment beside the table says so.

## The rest

`parse-integer` reads an integer with whitespace around it, a sign, a radix
and a range, and with `:junk-allowed` stops at the first character it cannot
use and says where. `prog` and `prog*` are `let` and `tagbody` inside `block
nil`, with any declarations at the start of the body moved onto the `let`,
the lesson of chapter 54. `multiple-value-setq` is `setf` of `values`
(chapter 43) with the first value as its result. `nset-exclusive-or` and
`nsubst-if` are their non-destructive twins, which CLHS allows.
`copy-symbol`, `copy-structure`, `char-int` and `standard-char-p` are a line
or two each. `function-lambda-expression` says the expression is unknown,
which CLHS allows, until definitions keep their source (#55).

`print-unreadable-object` writes `#<`, the object's type, what its body
writes and `>`. It showed a limit of Quokka's macro lambda lists, which take
no `&key` inside a nested pattern such as `((object stream &key type
identity) &body body)`; the macro takes the inner list whole and
destructures it with `destructuring-bind`. The type is a plain name for the
kind of object until there is a type system (phase 3), and Quokka's objects
have no stable address to show for `:identity`, so it adds nothing, which
CLHS leaves to the implementation.

## What did the measurement find?

The first run after this step passed 306 more tests and failed eight more,
and each of the eight was an old mistake that the new functions had let
ansi-test reach. `nset-exclusive-or` is `set-exclusive-or`, and that called
`set-difference` twice, the second time with the lists the other way round,
so the `:test` function received its arguments swapped; CLHS says it always
gets the element of the first list first, and the tests pass a test that
notices. `multiple-value-setq` with no variables returned nil instead of
the form's first value. And `symbol-macrolet` ignored declarations at the
start of its body, so `(declare (special x))` there did nothing; its file
could not be loaded before, because a helper in it used
`define-symbol-macro`. The compiler now reads those declarations as `let`
does, and refuses one that declares a symbol macro of the same form special,
with a `program-error`. That is a compiler change, and the fixpoint held:
the compiler's own source has no `symbol-macrolet` with declarations.

## What did it cost?

This step took eleven lines of Rust, for the three primitives, which brings
the core to 15,248 lines of 15,500. The rest is about 200 lines of Lisp, 30
of them in the compiler, and `tests/lisp/small-functions.lisp` checks all of
it against SBCL.

ansi-test went from 7,280 passing tests to 7,600, of 13,558, with the
three fixes above; the tests needing a tier-1 feature from 865 to 492, and
the plain failures from 378 to 372. The cons group alone gained 156 passing
tests, most of them the `c…r` accessors.

## What is still missing?

Three things are still missing. `ccase` has no restart yet; it arrives with
the rest of the restarts in tier 2. Outside their real domains `acosh` and
`atanh` signal an error where CL answers a complex number, which waits for
complex numbers. And `function-lambda-expression` has no source to return
until definitions keep theirs (#55).

---

**References.** The Common Lisp HyperSpec, the entries for `parse-integer`,
`prog`, `ccase`, `multiple-value-setq`, `define-symbol-macro`,
`print-unreadable-object` and `asinh`; 14.2 on the `c…r` accessors. Jean-Michel
Muller and others, *Handbook of Floating-Point Arithmetic*, 2nd edition,
2018, on correctly rounded elementary functions and why libraries promise
one unit in the last place.
