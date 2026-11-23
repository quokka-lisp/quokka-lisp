# What the first measurement asked for

> *Where this fits.* Chapter 49 measured tier 1 against ansi-test and
> produced a list: what fails, what escapes, and which missing features block
> how many tests. This chapter works through the top of that list. Five gaps,
> each narrow, each blocking hundreds of tests: errors that no handler could
> catch, `defstruct` putting its functions in the wrong package, symbols that
> could not be called as functions, the reader's `#+` and `#-`, and
> declarations. None of them is large, and most were leftovers from an
> earlier step that had been right at the time. Together they raise the
> number of passing tests by more than a quarter, and the ratchet chapter 49
> built shows its worth by catching one change that made things worse.

*Status: done, issue #62. Step O of phase 2.*

## Why start here?

The table in `docs/ansi-test.md` sorts what is missing by how many tests it
blocks, which is a way of asking where an hour of work buys the most
conformance. We picked the five entries that were both near the top and
small, diagnosed each before deciding anything, and wrote the diagnoses into
#62 with the options for each. Two of the five turned out to be one line of
reasoning each; the last, declarations, was the one with a real choice in it.

## Errors no handler could catch

`(f :b 1)`, where `f` accepts only `:a`, is an error, and `(handler-case (f
:b 1) (error () :caught))` should return `:caught`. It stopped the machine
instead, in 1,455 tests. The error came from `lambda_list.rs`, which binds a
function's arguments, and it was raised as `BadSyntax`. When #36 made errors
catchable, it drew a line: `BadSyntax` in compiled code means the bytecode
itself is wrong, which a program must never be able to hide behind a
handler. That was right for the eleven places that raised it then. Argument
checking had later reused the same variant for something quite different: a
fault in the call, which CLHS says is a `program-error` the program may
catch.

We gave the bad call its own kind of error, `BadCall`, which becomes a
`PROGRAM-ERROR` condition, and left `BadSyntax` meaning what #36 meant. The
same tests also passed `:allow-other-keys t` to switch keyword checking off
for one call (CLHS 3.4.1.4.1), which `lambda_list.rs` had documented as
missing, so `bind_keys` now honours it. The escapes fell from 1,455 to 18.

## Structures in the wrong package

154 failures in the `cons` group alone said `make-scaffold` was undefined,
although the file defining the structure `scaffold` had loaded without a
word. The prelude's `defstruct` builds the name `make-scaffold` out of
`make-` and `scaffold` with `%symbol-concat`, which interns the result in
Rust's system symbol table. That was all there was when the prelude was
written, before packages existed (chapter 36), and such a symbol ends up in
`CL-USER`. So a structure defined in any other package, such as ansi-test's
`CL-TEST`, had its constructor somewhere else.

The library now redefines `%symbol-concat`, once packages exist, to intern
in the current package:

```lisp
(defun %symbol-concat (a b)
  (values (intern (%string-append (symbol-name a) (symbol-name b)) *package*)))
```

Calls are late-bound, so every `defstruct` expanded from then on uses it. We
checked the fixpoint, our check that the compiler compiles itself into
exactly the same code generation after generation (chapter 42), would not
notice: neither the compiler nor the prelude uses `defstruct`.

A unit test noticed something else. It defines a structure in a world and
reads its source with the Rust reader, which still serves the build and a few
tests, and which interns every symbol in the system table without asking any
package. The `defstruct` form ran first and made `point-x` in `CL-USER`; the
next form, read afterwards, made a second `point-x` in the system table, and
`setf` no longer recognised the accessor. Chapter 36's packages already
bridged one direction of this gap: a symbol the Rust reader made is adopted
into `CL-USER` the first time anything looks for it there. `intern` now
bridges the other, making a new `CL-USER` symbol in the system table, where
the Rust reader will find it.

## A symbol as a function

`(funcall 'car '(1))` failed with "CAR is not a function". In Common Lisp a
symbol is a **function designator**: wherever a function is expected, a
symbol stands for its global function, which is why `:test 'eql` and
`(mapcar 'abs list)` are everyday code. Both evaluators handed the symbol
straight to the code that applies functions, which knows compiled functions,
closures and primitives and nothing else.

One small function now turns a designator into a function, and both
evaluators call it before applying anything:

```rust
pub fn designated<'h>(mc: &Mutation<'h>, f: Value<'h>) -> Result<Value<'h>, EvalError> {
    if !mc.is_symbol(f) {
        return Ok(f);
    }
    let function = mc.slot(f, crate::heap::symbol::FUNCTION);
    if function == Value::UNBOUND || is_macro(mc, function) {
        return Err(EvalError::UndefinedFunction(mc.symbol_name(f).to_string()));
    }
    Ok(function)
}
```

We considered leaving the interpreter out, since it only runs the cold
build, and decided against it: a tier-0 test file using `(funcall 'car …)`
would then give different answers in the two legs of the differential test.

## Reading what another Lisp wrote

ansi-test's support files are written for many implementations at once:
`#+sbcl (…)` means "read this only on SBCL", `#-clisp` "anywhere but CLISP".
Quokka's reader refused `#+` outright, so every helper defined with one inside
it was missing, and with them about 1,100 tests.

`#+feature form` is simple once the reader can skip a form without
understanding it. A form written for SBCL may mention packages Quokka has
never heard of, or syntax it cannot read, so skipping it means reading it in
a mode where tokens are not turned into symbols or numbers and nothing is an
error. Common Lisp calls the mode `*read-suppress*` (CLHS 2.4.8): the token
reader returns nil, and every `#` syntax reads the object after it and
returns nil. With that, `#+` and `#-` are a few lines:

```lisp
(defun %read-sharp-feature (stream sub-char argument)
  (let ((test (let ((*package* (find-package "KEYWORD"))) (%read-required stream))))
    (if (and (not *read-suppress*)
             (eq (if (%feature-true-p test) t nil) (eql sub-char #\+)))
        (%read-required stream)
        (progn (let ((*read-suppress* t)) (%read-required stream))
               (values)))))
```

The feature expression is read in the `KEYWORD` package, so `#+sbcl` asks
about `:sbcl`, and combined with `and`, `or` and `not`. The one decision was
what Quokka should claim. `*features*` is `(:quokka :common-lisp
:ieee-floating-point)`: not `:ansi-cl`, which CLHS says means the
implementation claims to conform, and nothing about the machine, since an
image is the same on every target. `#.form`, which evaluates FORM while
reading, came along, guarded by `*read-eval*`.

## Declarations

`(let ((x 1)) (declare (ignore x)) 5)` failed with "DECLARE is undefined",
in 742 tests. A **declaration** is information about a program written at
the start of a body: that a variable is unused, that it holds a fixnum, that
a function should be fast. CLHS 3.3.1 lets an implementation ignore every
kind but one, and Quokka has no optimiser that could use them. The one is
`special`, which makes a variable dynamic, as `defvar` does (chapter 35),
but only for one binding or one region of code. Ignoring that would not be
an error but a wrong answer: the variable would quietly stay lexical.

Our recommendation was to accept and ignore declarations now and make
`special` an error until it could be done properly. We decided instead to do
it properly at once. The prelude makes `declare` a macro that expands to
nil, so both evaluators can meet one anywhere without stopping. The compiler
reads the declarations at the start of `let`, `let*`, `lambda` and `locally`
bodies. A name declared special there is bound with the same `DBIND` record
a proclaimed special uses. And every reference to it inside the body must go
to its value cell, even where a lexical binding of the same name is in
scope:

```lisp
(let ((y 1)) (declare (special y)) (let ((y 2)) (list y (free-y))))  ; => (2 1)
```

The compiler already had a scope that exists only while compiling, the one
`symbol-macrolet` pushes (chapter 44), which `%lookup` steps over without
counting it as a run-time link. A `special` declaration pushes another, and
`%lookup` stops at it for the names it lists:

```lisp
((eq (car (car env)) ':special)
 (if (and (eq tag ':variable) (member name (cdr (car env))))
     nil
     (%lookup name (cdr env) depth tag)))
```

A name the lookup does not find is a global read through the symbol, which
under shallow binding is the dynamic value. `proclaim` and `declaim` of
`special` work like `defvar` without a value. One gap remains: a macro that
puts its body somewhere other than the start of a binding form, as `dolist`
puts it inside a `tagbody`, loses a `special` declaration in that body.

## The ratchet catches a step backwards

The first measurement after these changes showed nine tests that had passed
before and now failed: `proclaim.error.3` to `.11`, which check that
`(proclaim '(inline . foo))` signals an error. They had passed because
`proclaim` did not exist, and calling a missing function is an error too.
Our new `proclaim` quietly accepted malformed specifiers. The run refused to
pass until it was fixed: `proclaim` now insists on a symbol followed by a
proper list, as CLHS requires.

## What did it cost?

This step took about 40 lines of Rust: the new error kind and `:allow-other-keys` in
`lambda_list.rs`, `closure::designated` and its two callers. The core is now
14,757 lines of the 15,000 guide. The rest is Lisp: about 30 lines in the
compiler, 60 in the reader, a macro in the prelude, `%symbol-concat`, and
`proclaim` and `declaim`. A warm rebuild now reads 862 forms. The fixpoint
held: the compiler's own source has no declarations, so the new code changes
what the compiler is, not what it produces. A new differential file,
`tests/lisp/first-gaps.lisp`, checks all five against SBCL.

ansi-test went from 4,058 passing tests to 5,219, of 12,325; escapes from
1,455 to 18.

## What is still missing?

The top of the list is now `symbol-plist`, behind about a thousand tests
through a helper in the `symbols` group, then `with-standard-io-syntax`,
`compile`, `macro-function` and the package functions. Below them, the 681
plain failures are now the larger part of what stands between tier 1 and
phase 2's gate, and each needs reading on its own.

---

**References.** The Common Lisp HyperSpec: 3.4.1.4.1 on `:allow-other-keys`,
1.4.1.5 on designators, 2.4.8 on `*read-suppress*`, 2.4.8.17 on `#+` and
`#-`, 24.1.2.1 on feature expressions, 3.3 on declarations and 3.3.4 on the
scope of `special` declarations.
