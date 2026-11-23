# Compiling on request

> *Where this fits.* Since chapter 32 a built world holds no interpreted
> code, and since chapter 40 `eval` compiles its form before running it.
> Common Lisp also lets a program ask for compilation directly, with
> `compile`, and lets it supply its own rewrites of function calls for the
> compiler to apply, called compiler macros. This chapter, the fourth worked
> from ansi-test's list (chapter 49), adds both, and changes the compiler for
> the first time since the fixpoint (chapter 42) was established, which is
> the part worth following.

*Status: done, issue #65. Step R of phase 2.*

## What is left for `compile` to do?

In most Lisps `compile` turns interpreted code into machine code, and
`compiled-function-p` tells the two kinds of function apart. In a built
Quokka world there is only one kind. The interpreter is not in the machine
at all (it moved into its own crate, the cold build's, in #51), and `eval` is
`(funcall (compile-form form))`. So `compile` has one job left, turning a
lambda expression into a function, and `compiled-function-p` is true of
every function there is.

```lisp
(defun compile (name &optional (definition nil definition-p))
  (let ((function (cond ((not definition-p)
                         (or (macro-function name) (symbol-function name)))
                        ((functionp definition) definition)
                        (t (eval (list 'function definition))))))
    (cond ((null name) (values function nil nil))
          (t (if definition-p
                 (if (macro-function name)
                     (setf (macro-function name) function)
                     (setf (fdefinition name) function)))
             (values name nil nil)))))
```

Three details come from the standard rather than from us. A lambda
expression is compiled in the null lexical environment, so
`(let ((x 1)) (compile nil '(lambda () x)))` makes a function that reads a
global `x`, which `eval` of a `function` form gives for free. When a name is
given, `compile` returns the name, not the function; ansi-test's `compile.1`
checks exactly that. And `(compile 'f)` for a function that already exists
returns it unchanged: it is compiled already, and there is no source to
compile it from again until definitions keep their source (#55).

ansi-test also checks that compiling keeps literal objects as they are.
`(eq x (funcall (compile nil `(lambda () ',x))))` must be true for a list
`x`, and two equal strings in one lambda must stay two strings. Our compiler
keeps constants by reference in its constant vector (chapter 26), so these
passed without change.

`special-operator-p` came along, as a list of the standard's 25 names. The
compiler has its own `%special-operator-p`, which answers differently: in
Quokka `lambda` and `%handler-case` are compiled directly, and `progv` and
`load-time-value` are macros (chapter 44). The standard defines the answer
by name, whatever the implementation does underneath, so the two stay
separate.

## What is a compiler macro?

A **compiler macro** is an optional rewrite of calls to a global function.
The function stays a function, callable with `funcall` and `apply`, but when
the compiler sees a call to it written out in source, it may hand the call
form to the compiler macro first:

```lisp
(defun square (x) (* x x))
(define-compiler-macro square (&whole form x)
  (if (numberp x) (* x x) form))
```

Now `(square 5)` in compiled code becomes the constant 25 before the
compiler sees it, and `(square y)` is left alone, because the expander
returns its own `&whole` form, which is how a compiler macro declines. It is
the standard's hook for a library that wants calls with constant arguments
done at compile time without turning its functions into macros.

CLHS lets a compiler ignore compiler macros altogether (3.2.2.1.3). Our
proposal was to store them, return them from `compiler-macro-function`, and
never apply them, which conforms and leaves the compiler untouched. We chose
instead to apply them, as SBCL does. A program that relies on the rewrite
for speed gets it, and our own test of compiler macros could then tell
applying them from ignoring them.

## Where does the compiler look?

`%compile-cons` decides what a list form is: a special operator, a local
macro, a local function, a global macro, or a call. A compiler macro only
applies to the last of these, after everything else has had its say, and
only when no `flet` or `labels` function of the same name is in scope,
because a local function is not the global one the compiler macro was
written for:

```lisp
(t (let ((expander (if (symbolp head) (%macro-function head) nil)))
     (if expander
         (%compile (%expand expander (cdr form) env %local-macros) env tail)
         (let ((rewritten (%apply-compiler-macro form env)))
           (if (eq rewritten form)
               (%compile-call form env tail)
               (%compile rewritten env tail))))))
```

A call written `(funcall #'square 5)` is offered to the compiler macro too,
as the standard says it should be; `define-compiler-macro` then matches its
lambda list against the arguments after `#'square`.

The table of compiler macros lives in `compile.lisp`, the compiler's own
source, and not in the library. In the cold build the compiler runs
interpreted, before any library exists, and it must still be able to read
the table, which is empty then. `define-compiler-macro`,
`compiler-macro-function` and its `setf` are library code in `eval.lisp`
that fills it.

## Does the fixpoint survive a change to the compiler?

The fixpoint is our check that the compiler compiles itself into the same
code generation after generation (chapter 42): C1 is the compiler compiled
by the interpreted compiler in the cold build, C2 the one a C1 world compiles
in a warm rebuild (chapter 41), and C3 the one C2 compiles. Every named
function must be the same in all three.

Changing the compiler changes all three, which is fine, since the test
compares them with each other and not with a stored copy. The question is
whether the change makes the compiler compile *itself* differently. A
compiler macro rewrites a call, so if any compiler macro applied to a call
in the compiler's own source, C2 would differ from C1.

None does, and for a simple reason. C1 is compiled before the library
exists, so the table is empty. C2 and C3 are compiled after the library has
loaded, and the library defines no compiler macros, so the table is still
empty. The new code is in all three compilers and changes nothing they
compile of themselves. All 63 differential tests passed the first time,
`the_compiler_reproduces_itself` and `a_rebuilt_world_is_the_same_world`
among them. The comment beside the table says what would break this: a
compiler macro in the library for a function the compiler calls. That is the
rule chapter 34 put as "the rebuild has to compile the compiler in the
environment its source was written for", met once more.

## How do you test that a rewrite happened?

A compiler macro must not change what a call means, only how it is computed,
so a test of `square` gets the same answer with or without the rewrite and
cannot tell whether it happened. `tests/lisp/compile.lisp` uses one that
breaks the rule on purpose:

```lisp
(defun compile-test-tag (x) (list :function x))
(define-compiler-macro compile-test-tag (&whole form x &environment env)
  (declare (ignore env))
  (if (keywordp x) `(list :rewritten ,x) form))
(funcall (compile nil '(lambda () (compile-test-tag :k))))
```

SBCL answers `(:REWRITTEN :K)`, and so does Quokka, and both answer
`(:LOCAL :K)` when an `flet` of the same name surrounds the call.

## What did it cost?

This step needed no Rust at all. It took about 35 lines in the compiler for the table and its use, and
about 80 in `eval.lisp` for `compile`, `compiled-function-p`,
`special-operator-p` and the three compiler-macro operators. A rebuild now
reports `(:FORMS 950 :REPLACED 797)`, counting steps Q, R and S together.

Measured with steps Q and S, ansi-test went from 5,382 passing tests to
5,701, and `eval-and-compile` from 216 to 232. Most of the 380 tests that
named `compile` use it inside ansi-test's `signals-error` helper, which also
needs `handler-bind` (tier 2); they now count as needing that instead.

## What is still missing?

`compile` reports no warnings, because Quokka's compiler signals an error
for everything it objects to and has no warnings to collect. `(compile 'f)`
cannot recompile a function from source until definitions keep their source
(#55). An error inside a compiler macro's expander stops the compilation,
where SBCL warns and compiles the original call; that waits for warnings.
`compile-file` and `function-lambda-expression` need source too.

---

**References.** The Common Lisp HyperSpec, 3.2.2.1 (*Compiler macros*), and
the entries for `compile`, `compiled-function-p`, `define-compiler-macro`,
`compiler-macro-function` and `special-operator-p`.
