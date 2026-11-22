# A world made of bytecode

> **Where this fits.** Chapter 30 made the heap a file, and it worked — but the
> world inside that file was interpreted, because the only heap there was to
> save was the one built by interpreting the prelude. Chapter 29 had said why
> installing compiled code into a running interpreter cannot work, and where
> it can: in a heap that is about to be saved and abandoned. This chapter does
> that. At the end of it, Quokka starts from an image in which the prelude,
> every macro, and the compiler itself are bytecode, the interpreter never
> runs, and a scan of the whole heap proves there is nothing interpreted left
> to run.

## The question this answers

When chapter 30's image first worked, the obvious question came straight
back: *why is the image interpreted? Wasn't the whole point of the compiler
to create this image and hand it over to the VM?*

It was. Chapter 30 built the mechanism — a heap becomes a file and comes back
— and saved the only heap that existed, which was full of tree-walked
closures. `image::save` does not know or care what is in the heap. This
chapter changes *what is in the heap* before it is saved.

## Five steps, two of them passes

The build is `boot::build_image`, and it does five things.

1. **Boot as always.** Interpret the prelude, interpret the compiler. Every
   function and macro now exists, interpreted.
2. **Pass 1 — compile everything, install nothing.** Each form of the prelude,
   then each form of the compiler, goes through the (interpreted) compiler.
   Every function that comes out is verified and kept on a list.
3. **Pass 2 — run everything, in order, on the VM.** That is what installs.
   The compiled form of `(defun length ...)` is code that stores a compiled
   closure in `length`'s function cell; running it does exactly that.
4. **Prove it.** Collect, then scan the heap for any closure or macro whose
   body is not compiled code.
5. **Save.**

Why two passes and not one — compile a form, run it, compile the next? Because
the compiler is interpreted Lisp that calls prelude functions. Compile and
install `length` in one go, and the very next form's compilation calls
`length`, gets a compiled closure, and the interpreter refuses it (chapter 29).
So pass 1 finishes *all* the compiling while everything is still interpreted,
and only then does pass 2 replace things. Once pass 2 begins, nothing
interpreted runs at all.

And why is file order safe in pass 2? A top-level form that *calls* a
function at load time can only call one defined earlier in the file — that
was already true when the file was interpreted — and by then pass 2 has
replaced it.

This is not a new idea. It is, almost exactly, how the first compiler written
in the language it compiles was brought up: Tim Hart and Mike Levin's Lisp
compiler at MIT in 1962, written in Lisp, run by the Lisp interpreter to
compile itself, after which the compiled version took over. Quokka's
bootstrap is that, sixty-odd years on, with a heap that can be saved.

## Macros had to be compilable first

Until now the compiler refused `defmacro`, for a reason chapter 29 explained:
while the compiler is interpreted, a compiled expander is one it could never
*call*. So the interpreter evaluated every `defmacro` — the bootstrap rule.

A built image needs compiled macros, so that rule had to have an end.

A **macro**, underneath, is a `Macro` object: the same three slots as a
closure — parameters, body, captured environment — and a different type, so
that the evaluator can tell "call this" from "expand this". The prelude's
`macroexpand-1` fetches one with `%macro-function` and calls it with `apply`
on the argument forms. So an expander is just a function, and a compiled
macro is a `Macro` whose body is compiled code.

A compiled `defmacro` therefore does by instructions what the interpreter did
by hand:

```lisp
(progn (%set-symbol-function 'name (%make-macro <the expander, compiled>))
       'name)
```

Two small primitives made it possible. `%rewrite-macro-lambda-list` takes a
macro's parameter list apart — macros may destructure, `((a b) c)`, and may be
dotted — using the same Rust the interpreter's `defmacro` uses, so the two
take a call apart identically. `%make-macro` turns a compiled closure into a
`Macro`.

And the VM had to learn to *call* one. `macroexpand-1` calls an expander with
`apply`, and the interpreter has always allowed that, because its test for
"callable" accepts closures and macros alike. The VM's only accepted
closures. Once a compiled `macroexpand-1` met a compiled macro, the two would
have disagreed — so now they do not.

## `macrolet`: the compiler calling what it just made

`macrolet` defines macros that exist only while its body is being
*compiled*:

```lisp
(macrolet ((cube (x) (list '* x x x)))
  (cube 3))                              ; compiles as (* 3 3 3)
```

The compiler has to compile `cube`'s expander into a function *right now*,
at compile time, and then call it on `(3)` to find out what to compile. That
is a compiled function being called by the compiler. An interpreted compiler
cannot do it — the interpreter refuses compiled code — and a compiled one can.

So `macrolet` is the one feature that *only* works in the world this chapter
builds. Run by the bootstrap compiler it is refused, with exactly the error the
one-evaluator rule should give; run by the compiled compiler in an image, it
works. It is the cleanest demonstration there is that the compiler in the image
really is compiled.

## Calling Lisp without the interpreter

There was one more place the interpreter was hiding. The REPL compiled each
form by asking the *interpreter* to evaluate `(compile-form 'FORM)`. In a
compiled world, `compile-form` is a compiled closure, and the interpreter —
correctly — refuses to call it.

So the host needed a way to call a Lisp function using only the bytecode VM.
`vm::call_global` builds one, by hand, four instructions long:

```text
    GFREF  0      the function cell of COMPILE-FORM
    CONST  1      the form
    CALL   1
    RET
```

and runs it. A tiny piece of code whose only job is to jump into something
else is called a **trampoline**. With it, `vm::compile_and_run_forms` reads a
form, calls the image's `compile-form` through the trampoline, runs what comes
back, and hands over the answer — and at no point is there an interpreter in
the loop. That is the evaluator of a compiled image, and the REPL's `--image` (today `quokka WORLD.qi`)
mode uses nothing else.

## Proving nothing interpreted survived

Pass 2 replaces everything the prelude and the compiler define. But "replaces
everything" is a claim, and claims about what is in a heap are exactly the
kind this project has learned to check rather than trust.

The check is a scan. After pass 2 the build collects, and then walks the
whole heap linearly — the way the verifier and the collector walk it — looking
at every closure and every macro, and asking whether its body is a compiled
function.

The collection first is what makes the scan meaningful. Chapter 7's
collector is a *copying* collector: it copies what is reachable and leaves
everything else behind. After a collection, then, **everything in the heap
is live**. A scan that finds no interpreted closure is not saying "none that
I happened to look at" but "none, anywhere, that anything could reach".

If it does find one, the build fails and names the symbols whose function
cells hold one — which is where a bootstrap mistake would leave it.

It found none, first time.

## It worked first time, and why that is not luck

The first run of `build_image` built a world of 103 compiled forms, the scan
came back empty, and the image, loaded into a new process, did everything
asked of it:

```
quokka> (length '(1 2 3))
3
quokka> (defmacro twice (f) (list 'progn f f))
TWICE
quokka> (macrolet ((cube (x) (list '* x x x))) (cube 3))
27
quokka> (handler-case (car 5) (type-error () 'caught))
CAUGHT
quokka> (defstruct point x y)
POINT
quokka> (let ((p (make-point :x 1 :y 2))) (setf (point-x p) 42) (point-x p))
42
quokka> (compile-form '(+ 1 2))
#<compiled COMPILED-FORM>
```

That is the compiled compiler running for the first time ever, and it was
right. The reason is the pile of checks built before it. Chapter 29 compiled
the prelude and the compiler and verified every function — so pass 1 was
already known to work. The verifier guaranteed every function pass 2 ran was
well-formed. Chapter 28's last sections had filled in the special operators
and fixed `block`, so compiled code behaved like interpreted code on every
form that had a test. The work of this chapter was mostly connecting pieces
that had each been checked on their own.

The compiled image is also *smaller*: 96 KB, against 107 KB for the
interpreted one. Bytecode is more compact than the list structure it
replaced.

## What this chapter did not check

**Whether the compiled world gives the same answers as the interpreted one
across the whole test suite.** The tests here check what they check; the
seventeen differential files had only ever run interpreted. Chapter 33 runs
them compiled, and it found two disagreements.

**`mapcar` does not exist** — not a compiled-world problem; the prelude never
had it. Tier 1 will.

## Trying it

```bash
cargo run -p quokka-repl -- build world.qi
cargo run -p quokka-repl -- world.qi
```

The first builds the compiled world and saves it. The second loads it and
compiles every form you type with the image's own compiler — `defmacro`
included, and with no bootstrap rule, because there is nothing left to
bootstrap.

---

**References.** Hart and Levin, *The New Compiler*, MIT AI Memo 39 (1962), for
the original compiler-compiles-itself bootstrap through an interpreter.
Queinnec, *Lisp in Small Pieces* (1996), chapter 9, on macros and the
separation between the world a compiler expands in and the world its output
runs in — which is the bootstrap rule, and why it could end here. Baker, *CONS
Should Not CONS Its Arguments, Part II: Cheney on the M.T.A.* (1995), for
trampolines and what they are used to get around.
