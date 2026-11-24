# Two evaluators, one answer

> *Where this fits.* Chapter 32 built a world in which everything is
> compiled. Chapter 28 had already shown, the hard way, that two evaluators
> can each pass their own tests for weeks and still disagree: the
> interpreter's `error` and the VM's differed in two ways and nothing compared
> them. This chapter adds the comparison, which is every differential test
> file run in the compiled world against the same baseline the interpreter and
> SBCL already agree on, and with it phase 1 is finished.

## The fourth leg

Chapter 12 introduced the differential tests: ordinary Common Lisp files,
each with a **baseline**, a file of what every top-level form should print,
and the same file run several ways, all of which must print the baseline.
Quokka natively against SBCL catches *we implemented Lisp wrong*. Quokka
natively against Quokka under WebAssembly catches *the machine behaves
differently depending on where it runs*.

The fourth leg runs each file in a **compiled world**: load the image chapter
32 builds, then for each form, compile it with the image's own compiler, run
it on the VM, and print what it gave. It catches *the compiler and the
interpreter disagree*.

What makes it worth having is that the baseline is shared. It was written
against the interpreter and checked against SBCL long before anything was
compiled, so the compiled leg cannot be made to pass by adjusting the
expectations to fit. If the compiled world prints something else, one of the
two evaluators is wrong, and the baseline says which.

Two details make the comparison fair. Forms are compiled and run one at a
time, in order, because a file means its forms in sequence: a `defstruct` has
run, and defined how `setf` works on its accessors, before the form that uses
them is compiled. And a compile error counts as the form's error, because some
forms in the files are wrong on purpose, such as `(return-from nowhere 1)` and
`(let* 1)`, and their baseline line is `!! error`. The interpreter finds these
when it runs them; a compiler finds them when it compiles them. Either way the
form is an error, and either way the line matches.

## What the first run found

Fourteen of the seventeen files agreed straight away. The other three
disagreed on six forms, which turned out to be two problems.

### `setq` with more than one pair

```lisp
(setq a 1 b 2)
```

should set both variables and return 2. The compiled version returned 1 and
never set `b`: the compiler's `setq` compiled the first pair and ignored the
rest. Every unit test of compiled `setq` had used one pair. The only test with
two lived in a differential file that had never been run compiled.

The fix compiles each pair in turn, the value, then `LSET` or `GSET`, then a
`POP` before the next pair, so that what is left on the stack at the end is
the last value, which is what `setq` returns.

### A compiler that accepted nonsense

The other five disagreements were all the same thing:

```lisp
(let)            ; expected an error, compiled to nil
(block)          ; expected an error, compiled to nil
(catch)          ; expected an error, compiled to nil
(flet ((f)) 1)   ; expected an error, compiled to 1
```

Each is malformed, being a `let` with no binding list, a `block` with no name,
a local function with no parameter list, and the interpreter refuses each, as
SBCL does. The compiler took them apart as best it could: no binding list
became no bindings, no name became `nil`, no parameter list became none.

That sounds like leniency. It is worse than that. A compiler that accepts
programs the language rejects is implementing a *different language*, and
anyone who writes `(let)` by mistake gets a running program from one
evaluator and an error from the other.

The fix is a function, `%check-syntax`, that applies the interpreter's checks
to every special form before compiling it. The conditions were **read from
the interpreter's source**, one by one, rather than guessed from its error
messages: `if` wants at least a test and a consequent; `quote` exactly one
argument; a binding is a symbol or a list starting with one; and so on. The
point of the exercise is that the two agree, and agreement copied from a
summary is how they came to disagree in the first place.

## What it did not find

Nothing else. Every other form of every file, meaning conditions, multiple
values through every construct that must pass them on, `unwind-protect` and
the order cleanups run in, keyword arguments, `defstruct`, hash tables and
macros with destructuring, printed exactly what the interpreter prints.

It would be wrong to read that as luck, and wrong to read it as proof. It is
not luck because most of the ground had already been covered piece by piece:
the verifier checked every function's shape, chapter 28's sections made the
special operators behave like the interpreter's, and chapter 32's tests ran
the compiled compiler before this leg existed. It is not proof because
seventeen files are seventeen files. They are what the project has learned to
test so far, and the next bug will be in something none of them says.

Every test also passes under **GC torture**, meaning the build and all
seventeen files in the compiled world, with a garbage collection at every
single allocation. A collector bug in compiled code would surface there as a
wrong answer.

## A test that was retired

Chapter 28 had added a *compile leg*: every file compiled, form by form, by
the bootstrap compiler, with each failure checked against the baseline. It
was the best that could be done before a compiled world existed, since it
proved the compiler could handle each form without running anything.

It is gone. The compiled leg compiles every form with the compiler that
actually runs in images, runs it, and compares the answer, so it checks
strictly more. The bootstrap compiler's real job, compiling the prelude and
itself, is exercised on 103 forms by every build. And the old leg's two
standing exceptions, `values.lisp` and `structures.lisp`, were not bugs at
all, only consequences of compiling without running, so keeping the leg meant
maintaining a list of things that were not wrong.

The cost is honest and small: the bootstrap compiler is no longer pointed at
the test files. If it ever regresses on something the prelude and the compiler
do not use, nothing will notice until the build does.

## Phase 1, finished

The design set phase 1 one gate, *an image saves, reloads and resumes where
it left off*, and a test plan: image round-trips, and interpreted and
compiled results must match. Both hold. What the phase built:

| | | |
| --- | --- | --- |
| the instruction set | `docs/bytecode.md` | 38 opcodes, each with fixed operand widths |
| the machine | `vm.rs`, `code.rs` | a bytecode VM and a verifier that checks every function before it runs |
| the compiler | `lisp/compiler/compile.lisp` | every special operator, written in tier-0 Lisp |
| the image | `image.rs` | a heap to a file and back, mid-computation included |
| the compiled world | `boot::build_image` | prelude, macros and compiler all bytecode, and a scan proving it |
| host requests | `host.rs` | the outside world reached by asking, never by waiting |

And, along the way, things that were cheaper to fix now than later: compiled
code that could not catch an error (#36), a `block` that left its scope behind,
a `tagbody` that could not loop, a host interface that could not have worked in
a browser.

The language is no longer defined by the Rust in `eval.rs`. It is defined by a
Lisp file, compiled by itself, and the interpreter is what it was always meant
to be: the thing that builds the first image, and is then cold.

Phase 2 is self-hosting: the compiled compiler, running in the image, rebuilds
itself and the core library, and the gate is that the rebuilt compiler matches
the first build byte for byte. The design notes that packages belong at the
*start* of phase 2, before the compiler's sources would have to be
re-packaged mid-bootstrap.

*Later: phase 2 grew. Its gate held, in chapter 42, and we chose to extend
the phase to all of tier 1 rather than stop there, so Part VI runs from
packages to arrays. The instruction set is at forty opcodes now, and the
seventeen files are forty-four, which is the measure of how much of Common
Lisp arrived after this page.*

## Status

Built. All seventeen files agree interpreted, compiled, and with SBCL; the
compiled leg passes under GC torture; and it passes under WebAssembly too.

That last one is worth a sentence on how it was known. The machine this
chapter was written on has no `wasmtime`, so the compiled leg was first run
under wasm32 by CI, after the commit, and until that run came back the
chapter said "expected rather than observed", which is the honest wording for
something not yet seen. It came back green: the compiled world gives the same
answers on 64-bit native and on wasm32's 32-bit address space, which is rule
3's word indices earning their keep once more.

Not here: the browser host for requests (chapter 31), which still waits for a
compiled world in a browser tab.

---

**References.** McKeeman, *Differential Testing for Software*, Digital
Technical Journal 10(1), 1998, the paper that named the technique this book
has leaned on since chapter 12. Yang, Chen, Eide and Regehr, *Finding and
Understanding Bugs in C Compilers*, PLDI 2011, for differential testing aimed
at compilers, at a scale that found hundreds of bugs in GCC and LLVM.
