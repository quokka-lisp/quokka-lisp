# The image rebuilds itself

> **Where this fits.** Chapter 32 built the first compiled world: Rust boots
> an interpreter, the interpreted compiler compiles the system, and the result
> is saved. Chapters 38 to 40 moved reading, printing and the read-eval-print
> loop into Lisp. This chapter puts those together. A running world reads its
> own source, compiles it with its own compiler, and replaces itself with the
> result. What it unlocks is the phase's gate, step F: comparing what each
> generation of the compiler produces.

*Status: done, issue #48. The comparison of generations is step F.*

## Cold and warm

There are now two ways to make a Quokka world, and they need names.

The **cold build** is `boot::build_image`, from chapter 32. It starts from an
empty heap, so it can rely on nothing written in Lisp, and Rust does
everything: boot the interpreter, interpret the prelude and compiler, compile
them with the interpreted compiler, run the results, load the library. The word
comes from SBCL, whose build makes a **cold core**, an image assembled from
outside before any of its own code has run.

The **warm rebuild**, `%rebuild-world` in `rebuild.lisp`, starts from a running
world, and that world does all the work itself:

```bash
cargo run -p quokka -- build c1.qi
cargo run -p quokka -- rebuild c1.qi c2.qi
```

The second command loads `c1.qi`, calls `(quokka::%rebuild-world)`, and saves
what the world has become. Rust's part is loading the image, passing one form
to the loop from chapter 40, and saving the image again. Reading 163 KB of
source, compiling about 370 forms and installing them is Lisp.

The cold build stays. Something has to make the first world, and it cannot be a
world. What changes is that it now happens *once*. After that, the system can
remake itself from source: edit `lisp/compiler/compile.lisp`, rebuild, and the
new compiler is running, with no Rust recompiled.

Issue #39 names the generations: the cold build makes **C1**, a rebuild in C1
makes **C2**, and a rebuild in C2 makes **C3**.

## Two decisions

### Where the source comes from

The world could carry its source inside itself, as strings, which is the *image
host* `docs/design.md` describes. A rebuild would then need nothing from
outside, but 163 KB of text would grow the image from 348 KB to about 510 KB.
Or it could ask the host for the files by name, with the `%read-source` request
`load` already uses.

We chose the host. The native REPL answers with the files on disk, which is
what makes *edit and rebuild* work, and a test, or later the browser, answers
from the copies the binary already embeds for the cold build. The image pays
nothing.

### How the new code replaces the old

The simplest rebuild is a list of `load`s: prelude, compiler, library, each
form compiled and installed in turn. The trouble is the compiler file. Halfway
through loading `compile.lisp`, the functions near the top have already been
replaced, so the rest of the file is compiled by a compiler that is part new
and part old. If old and new agree exactly, that does no harm. If they do not,
the first symptom appears while the compiler is compiling itself, which is the
hardest place to debug.

GCC's bootstrap has the same problem and solves it in **stages**: each stage is
a complete compiler built entirely by the previous one, and a compiler is never
replaced while it runs. Quokka's cold build already works this way, with pass 1
compiling everything and pass 2 installing everything, and we chose the same
shape for the rebuild:

```lisp
;; Pass 1: compile the prelude and the compiler, installing nothing.
(let ((compiled nil))
  (%for-each-form (cons %prelude-file %compiler-files)
                  (lambda (form)
                    (setq defined (%note-definition form defined))
                    (setq compiled (cons (compile-form form) compiled))))
  ;; Pass 2: install them, in file order.
  (%run-in-order (reverse compiled))
  ...)
```

The library comes afterwards, form by form, compiled by the compiler just
installed. It defines macros (`defvar`, `with-output-to-string`) that its own
later forms use, so each form has to run before the next is compiled, which is
also exactly what the cold build does with it.

## Rebuilding *in* a world

A rebuild is not a fresh start. It runs inside a world that already has
packages, symbols, a readtable and data, and it replaces only the **code**.
That has three consequences.

The source is read in `QUOKKA`. The cold build's Rust reader put the system's
own symbols there (chapter 36), so reading the source in `QUOKKA` makes
`%compile` the very symbol the running system calls, and installing a new
definition replaces the old one instead of creating a lookalike in `CL-USER`.
`defun` and `car` come from `COMMON-LISP`, which `QUOKKA` uses.

`defvar` does not reassign. A rebuild re-runs every top-level form, but
`(defvar *readtable* (copy-readtable ...))` keeps its existing value, and that
value holds the *old* generation's reader functions, so a rebuilt world would
read with old code. The rebuild therefore resets `*readtable*` from the
standard readtable, which the reloaded `reader.lisp` has just filled with new
functions. Packages, on the other hand, *must* carry over: every symbol's home
points at one, and re-running `make-package "COMMON-LISP"` would fail. `defvar`
leaving them alone is exactly right.

And it ends with a proof. The cold build's is a scan of the heap for anything
still interpreted (chapter 32). The rebuild's is a comparison: before starting,
it records what every symbol in `COMMON-LISP` and `QUOKKA` holds, and
afterwards every name the source defines with `defun` or `defmacro` must hold
something different. A name still holding its old function was not rebuilt, and
the rebuild fails, naming it. Each rebuild reported `(:FORMS 373 :REPLACED
316)` when this was written; the library grows, and chapter 43 made it 421 and
348.

*Later: it reports `(:FORMS 1229 :REPLACED 983)` today, and the number is
asserted in the test, so every library form added since has had to change it on
purpose.*

## What the first rebuild found

The first rebuild worked, and the image grew: C1 was 359,688 bytes, and C2 was
367,952. Running it again gave C3 and C4 at *exactly* 367,952 bytes, so the
growth was not a leak. It was something every rebuild produced the same way.

Counting heap objects by type (the same census as chapter 37) showed where it
came from: 966 words of **environment objects**, three for each of about 316
functions. A function defined by the cold build had none of them.

The cause was chapter 40's change to the VM. To let `eval` call what
`compile-form` returns, the VM treated a bare compiled function as a closure
and bound its parameter list, which was empty, and binding an empty list still
makes a scope object. Every `defun` run inside that function then made a
closure that captured the empty scope, three words that could never be used.
The cold build does not do this, because it starts each compiled form with
`Vm::start`, which gives it no scope at all.

The fix makes a bare compiled function run the way `Vm::start` runs one, with
no scope. Afterwards C1 is 359,688 bytes and C2 and C3 are both 360,224. The
remaining 67 words are the REPL's own leftovers: the line it was given, and two
symbols (`%WAITING`, `%REPLIED`) that the rebuild's file requests created.

The waste was small, but it was a real difference between code installed by the
cold build and code installed by `eval`. Step F is about to compare the two, so
it was worth removing first.

## What step F will meet

The symbols in C1 and C2 differ in one systematic way. About 120 names like
`G16464` in C1 are `G18048` in C2. These are **gensyms**, symbols that macros
make with fresh names so that they cannot collide with anything the programmer
wrote. Their names come from a counter that only ever goes up, and C2 was
compiled later than C1, so its gensyms have higher numbers. The compiled *code*
can be identical while the *names* differ. #39 already listed this: the
fixpoint must compare functions with gensyms matched by position, not by name.

## What it cost

**Rust: 41 lines**, for the `--rebuild-image` flag (today `quokka rebuild`), a
`load_image` helper shared with `--image`, and the scope fix in the VM.
**Lisp:** `rebuild.lisp`, about 130 lines. **Tests:** a rebuilt world passes
all 22 differential files, then rebuilds itself again. **Time:** a rebuild
takes about eighteen seconds natively. A compiled compiler on a bytecode VM is
not fast, and nothing has asked it to be yet.

## What is not here

- The comparison. C1 = C2 = C3 is step F.
- A browser host to answer the rebuild's file requests from the embedded
  copies. Tests do it today.
- Rebuilding from an edited library in the browser, which would need the image
  host after all.
- Removing the cold build. It stays, and #29 asks whether it can at least stop
  needing the interpreter.

*Later: a rebuild still takes about eighteen seconds, with forty-five
differential files to pass afterwards rather than twenty-two, and the browser
host is still the one piece outstanding. The one thing that changed is the
budget: a rebuild runs for about 201 million steps now, so the limit it is
given is sixty times a single form's, and a rebuild that stops for want of
budget but finishes in normal time with more of it is not a loop (chapter 63).*

---

**References.** The GCC manual's *Building a native compiler* section on the
three-stage bootstrap and `make compare`. SBCL's `make.sh` and its
"genesis"/cold-core stage, for a Lisp built from outside before it can build
itself. Ken Thompson, *Reflections on Trusting Trust* (1984), for what a
compiler that compiles itself can carry invisibly from one generation to the
next. David A. Wheeler, *Countering Trusting Trust through Diverse
Double-Compiling* (2005), for comparing what two different compilers make of
the same compiler source, which is what comparing C1 (made by the
interpreted compiler) with C2 (made by the compiled one) amounts to.
