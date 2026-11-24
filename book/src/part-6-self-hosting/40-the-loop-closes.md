# The loop closes

> **Where this fits.** Chapter 38 put the reader in Lisp, and chapter 39 the
> printer. Both worked, but the system did not use them for its own work: the
> REPL and the tests still read with the Rust reader, and that had just cost
> chapter 39 sixty-six disagreeing tests. This chapter adds the missing
> letter of R-E-P-L, `eval`, and `load` with it, and moves the whole loop into
> Lisp. From here, a built image reads, evaluates and prints entirely in its
> own Lisp. What it unlocks is step E, the image rebuilding itself from
> source, and through that the fixpoint.

*Status: done, issue #47. Library files are still read by the Rust reader
while the image is built; that is step E.*

## eval is compile, then call

A Lisp `eval` can work in two ways. An **interpreting** `eval` walks the form,
as Quokka's Rust interpreter does. A **compiling** `eval` compiles the form and
runs the result. SBCL does the second by default (its `*evaluator-mode*` is
`:compile`), and #25 decided long ago that a built Quokka world never
interprets, so the decision was already made:

```lisp
(defun eval (form)
  (funcall (compile-form form)))
```

One line of Lisp, and it did not work the first time. `compile-form` returns a
raw compiled `Function`: code, constants, nothing else. The VM only knew how to
call a **closure**, a function paired with the variables it captured (chapter
10), so `funcall` refused it. The fix was six lines in `vm.rs`, where a
`Function` on its own is called as a closure that captured nothing. That also
makes `compile-form`'s result a real function, which is what CL's `compile`
returns.

`eval` sees the **dynamic** environment, meaning special variables, but not the
**lexical** one:

```lisp
(let ((*dynamic* 'inner)) (eval '*dynamic*))   ; INNER
(let ((lexical 1)) (eval 'lexical))           ; error: LEXICAL is unbound
```

That is the standard's rule, and the reason is visible in the code. The form is
compiled on its own, in an empty scope, so the compiler has no way to know that
`lexical` was a local somewhere else. A special variable's value lives in the
symbol (chapter 35), and the compiled code finds it there.

### Compiling inside a compilation

`compile-form` keeps its work in progress in five global variables: `%code`,
`%constants`, `%constant-count`, `%open` and `%local-macros`. It started each
compilation by resetting them, which made it not **re-entrant**, meaning not
safe to call again before the previous call had finished.

Nothing seemed to call it that way, until you notice when macros run. A macro's
expander runs *during* the compilation of the form that uses it, so an expander
that calls `eval` starts a second compilation while the first is half done. We
asked whether this limit would cause trouble later, so it was tested:

```lisp
(defmacro at-expansion () (list 'quote (eval '(list 1 (+ 1 1)))))
(list (at-expansion) :between (at-expansion))
```

SBCL answers `((1 2) :BETWEEN (1 2))`. Quokka answered `(1 2)`, with no error.
The inner compilation reset the globals, and the outer one finished with the
inner one's code. That is the worst kind of failure, a wrong answer that looks
like a right one.

The compiler already knew the cure. A `lambda` inside a form is compiled on its
own, and `%assemble-lambda` saves the outer accumulators first and restores
them afterwards. `compile-form` now does the same, inside an `unwind-protect`,
so that a nested compilation that fails halfway, such as `(eval '(if))` caught
by `ignore-errors`, still gives the outer one its work back. It cost ten lines
of Lisp and no Rust, and the tests now cover three levels of nesting and a
nested failure.

Re-entrant is not the same as safe for concurrency, though. Once phase 3 has
processes, the scheduler can switch from one compilation to another in the
middle of both, and saving and restoring around a call does not help with that,
because the two compilations are interleaved rather than nested. That case is
recorded in #34, the issue collecting what the code must not rule out before
phase 3. The options there are a lock that lets one process compile at a time,
or compiler state passed explicitly instead of kept in globals. Special
variables would be the idiomatic third option, but they are out for now,
because the Rust interpreter that runs the compiler during the bootstrap has no
dynamic binding.

## load

`load` reads a file form by form and `eval`s each one. A file name goes to the
host as a `%read-source` request (chapter 31), and a stream is read directly,
which is how the tests load without a file system.

The interesting part is two variables it rebinds:

```lisp
(defun load (source)
  (let ((stream (if (stringp source)
                    (make-string-input-stream (%read-source source))
                    source))
        (*package* *package*)
        (*readtable* *readtable*))
    (%load-forms stream)
    t))
```

A file usually begins with `(in-package :something)`. Because `load` binds
`*package*`, that change lasts until the end of the file and no further, and
the caller's package is unchanged when `load` returns.

That `let` contained the step's one real bug. The first test of `load` found
the package change *leaking*: after loading a file containing `in-package`, the
caller was in the wrong package. The cause was the order of the library's
files. `*package*` is given its value in `packages.lisp`, the last file,
because there are no packages before that, so when `eval.lisp` was compiled,
`*package*` was not yet a special variable. A `let` of an ordinary variable
compiles to an ordinary local, so `load` made a local copy that nothing read,
while `in-package` changed the real one.

This is a classic trap. A variable must be **proclaimed special** before any
code that binds it is compiled, because the compiler decides at compile time
whether a binding is lexical or dynamic. It is why Common Lisp programs put
their `defvar`s first. The fix is one line at the top of `reader.lisp`, the
first file to use `*package*`:

```lisp
(defvar *package*)
```

A `defvar` with no value proclaims the variable special and gives it nothing.
`packages.lisp` gives it its value later.

## The loop, in Lisp

A **REPL** is four steps in a loop: read, evaluate, print, and go round again.
Until now the loop was Rust (`vm::compile_and_run_forms`), reading with the
Rust reader, calling the image's compiler, running the result and printing with
the Rust printer. It could live in Lisp, with each host only calling it, or
stay in Rust, with each step a call into Lisp.

We chose Lisp. The loop is `%host-step`, in `eval.lisp`:

```lisp
(defun %host-step (ignored)
  (handler-case
      (let ((form (read %host-input nil %end-of-input)))
        (if (eq form %end-of-input)
            (list :eof)
            (list :value (prin1-to-string (eval form)))))
    (error (condition) (list :error (%condition-text condition)))))
```

A host hands it some text once, then calls it until it says `:eof`. Each call
reads one form, `eval`s it, and prints the value, or catches the error so the
next form still runs, as a REPL must. The Rust side, `vm::eval_source`, is only
a loop that calls this and relays what comes back. The REPL with an image uses
it, and so does the compiled leg of the differential tests.

What this buys is that everything a person might want to change is now
changeable from inside the running image: how a form is read, how it is
evaluated, how it is printed, and what happens on an error. That is rule 1, and
for a Lisp-machine-style system it is the point, because a debugger, when it
comes, will be a change to this loop, made in Lisp.

## The gap closes

The second decision was which hosts switch now: all that run an image, or only
`eval` and `load`. We chose all of them, and the result is the measurement
chapter 39 was waiting for.

The compiled leg of the differential tests now reads every test file with the
Lisp reader, in `CL-USER`, and prints every result with the Lisp printer. A
test's `a` is now `CL-USER::A`, a symbol of its own, and no longer the `QUOKKA`
symbol the prelude happened to use. The sixty-six forms that printed `QUOKKA::A`
print `A`. **All twenty-two differential files agree with SBCL**, and the gap
chapter 36 opened, where the Rust reader ignored `*package*`, is closed for
everything a built image reads.

The interpreted leg still reads with the Rust reader and prints with the Rust
printer, against the same baselines. Both printers now answer to SBCL for every
test.

The new differential file, `eval.lisp`, tests `in-package` the way a real file
uses it. At top level, it changes how the *next form* is read:

```lisp
(package-name (in-package :eval-test))
(package-name (symbol-package 'fresh-here))   ; "EVAL-TEST"
```

That could not be tested before. With the Rust reader, `fresh-here` was read
before anything could tell it which package to be in.

## What it cost

**Rust: 75 lines.** Six of them make a compiled function callable, about
fifty-five are `vm::eval_source` and its `Outcome` type, which is the one loop
every host shares, and the rest switches the REPL over. That is more than the
issue estimated, because the shared loop went in the VM crate rather than being
copied into each host. 1,365 lines are left.

**Lisp:** `eval.lisp`, about 80 lines. **The image:** 348 KB.

## What is not here

- The library read in Lisp. The build still reads `lisp/library/` with the Rust
  reader, which is why `packages.lisp` must stay last. Step E.
- Two processes compiling at once. Compiling is re-entrant, but its state is
  still global, and phase 3 decides how to share it (#34).
- `load`'s keyword arguments, `:verbose`, `:print` and `:if-does-not-exist`,
  and pathnames, so a file is just a name the host understands.
- `compile` and `compile-file` as CL names them. `compile-form` is the
  machinery under both.

*Later: `compile` arrived in chapter 53, as a thin layer over `compile-form`,
and `compile-file` has not, because nothing yet writes compiled code to a file.
`load`'s keywords and two processes compiling at once are both still open. The
twenty-two differential files are forty-five now, and every one of them agrees
with SBCL in a rebuilt world as well as a built one.*

---

**References.** CLHS `eval` and `load` (the "Evaluation and Compilation"
dictionary, chapter 3), and 3.2.2 on what compiling a form means. SBCL's
manual on `*evaluator-mode*`. Abelson and Sussman, *Structure and
Interpretation of Computer Programs*, chapter 4, for `eval` as the heart of a
Lisp, here replaced by a compiler.
