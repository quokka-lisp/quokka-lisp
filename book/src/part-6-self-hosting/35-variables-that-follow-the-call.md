# Variables that follow the call

> **Where this fits.** Chapter 34 planned phase 2 and found that Quokka had no
> special variables at all — every `let` lexical, no `defvar` — although
> packages, the reader and the printer all need them. This chapter adds them,
> by the shallow binding chapter 34 chose, and in doing so lays down two
> things the rest of the phase will use: the **tier-1 library**, and the
> **tier-1 leg** of the differential tests.

## What a special variable is

Every variable Quokka had until now was **lexical**: a name refers to the
nearest binding *written around it in the source*. Look at the text, and you
know which binding you get.

A **special variable** works differently. A binding of it is visible to
everything that runs *while the binding is in force* — including functions
written somewhere else entirely, which have no idea it exists:

```lisp
(defvar *depth* 0)
(defun show-depth () *depth*)

(show-depth)                     ; => 0
(let ((*depth* 5)) (show-depth)) ; => 5 — show-depth sees the binding
(show-depth)                     ; => 0 — and the binding has ended
```

`show-depth` is written outside the `let`. Lexically it could only ever see
the global `*depth*`. It sees 5 because the binding is in force while it runs.
Other languages call this **dynamic scope**. Common Lisp keeps it for a small
number of variables, marked with `defvar` or `defparameter` and named, by a
strong convention, with asterisks.

Why have them at all? Because some settings are naturally "for the duration of
this": print numbers in base 16 *while this runs*, read symbols into this
package *while this file loads*, send output here *while this function
writes*. Passing such a setting as an argument through every function in
between would be noise; a special variable lets the one place that cares set
it, and the one place that uses it read it.

## Shallow binding, and the record that makes it easy

Chapter 34 chose **shallow binding**: the current value lives in the symbol's
value cell, where a global variable's value already lived. So reading a
special variable needs nothing new at all. It was already `GREF`, one load from
the symbol; assigning it was already `GSET`.

Only *binding* is new, and it has to answer one question: when the binding
ends, how does the old value come back?

The answer was already in the machine. Chapter 27 built **unwind records** —
seven slots on the operand stack — for `block`, `catch`, `unwind-protect` and
`handler-case`. A record is pushed when the form begins and dropped when it
ends, and crucially, it is dropped whichever way the form ends: by `UNMARK` at
the bottom, or by the unwinder passing it on the way to a `throw`'s catch or an
error's handler.

That is exactly the guarantee a binding needs. So binding a special variable
pushes one more kind of record, holding the symbol and the value it had
before, and stores the new value. The instruction is `DBIND`, Quokka's
thirty-ninth:

```rust
Op::Dbind => {
    // ... constants[k] must be a symbol ...
    let value = self.pop();
    let old = self.mc.slot(symbol, crate::heap::symbol::VALUE);
    self.push_mark(mark::SPECIAL, symbol, 0);
    let at = self.mc.mp();
    let stack = self.mc.register(Register::Operands);
    self.mc.set_slot(stack, at + mark::SAVED, old);
    self.mc.set_slot(symbol, crate::heap::symbol::VALUE, value);
}
```

And every way of dropping a record now checks whether it is a binding:

```rust
fn restore_binding(&mut self, at: usize) {
    if self.mark_slot(at, mark::KIND).as_fixnum() == Some(mark::SPECIAL) {
        let symbol = self.mark_slot(at, mark::DATUM);
        let old = self.mark_slot(at, mark::SAVED);
        self.mc.set_slot(symbol, crate::heap::symbol::VALUE, old);
    }
}
```

`UNMARK` calls it. The unwinder calls it for every record it passes. That is
the whole of the restoring: a `throw` out of a binding, a `return-from` through
one, an error caught outside one, a `go` out of one — all of them were already
dropping records, and now they put a value back as they do.

### The ways out nobody takes

There is a fifth way for a binding to end, and it is the one that is easy to
miss: the computation simply *stops*. An error nothing catches halts the
machine; a computation that runs out of budget is abandoned by whoever was
driving it. Neither drops any records — they are just left there — and so a
special variable bound at that moment would stay rebound **for ever**, because
the only things that restore it are the ways out the machine never took.

So halting on an uncaught error now walks the remaining records and restores
every binding, and so does starting a new computation over an abandoned one.
Two tests pin it, one for each.

## What the compiler decides

The machine knows how to bind. The compiler has to know *when*: which
bindings in the source are dynamic.

The rule is Common Lisp's: once a symbol has been proclaimed special by
`defvar` or `defparameter`, **every** binding of it is dynamic, wherever it
appears — a `let`, a `let*`, a function's parameter. So the compiler keeps a
table of proclaimed symbols, and consults it each time it compiles a binding.
The table lives in the compiler, in Lisp, because the compiler is the only
thing that asks — which also means it cost no Rust.

`let*` is simple, because it binds one variable at a time anyway: a special
gets a `DBIND`, and an `UNMARK` after everything that follows it.

`let` is subtler, because of what `let` *means*: every initial value is
computed **before any variable is bound**. That matters as soon as a special
is involved:

```lisp
(let ((*depth* 50)
      (x *depth*))   ; this *depth* is the outer one — 0, not 50
  x)                 ; => 0
```

So the compiler binds all the values lexically first, exactly as a plain `let`
does — but gives each special a hidden name nothing can refer to — and only
then copies those values into their symbols with `DBIND`. References to the
special in the body find no lexical binding of that name, and reach its value
cell, as they must.

A function parameter that names a special — `(defun with-depth (*depth*) ...)`
— works the same way: the call binds the argument lexically, under a hidden
name, and the function's first instructions bind it dynamically.

Two consequences fall out. A body inside a dynamic binding is **not in tail
position**: the binding has to be undone after it, so a call there cannot
reuse the frame. And a `go` out of a dynamic binding cannot be chapter 28's
one-step `JMP-TAG`, because the innermost record is the binding, not the
tagbody — so it uses `GO` and the unwinder, which restores the binding on the
way. The verifier enforces the second: a `JMP-TAG` with a binding on top is
refused before anything runs.

## The first file of the library

`defvar` and `defparameter` are macros, and they are the first code in the
**tier-1 library** chapter 34 described: `lisp/library/`, a third body of Lisp,
loaded into the compiled world by the image's own compiler after the build.
Not the prelude, which is interpreted at boot and must stay tier 0. Not the
compiler. The interpreter never sees it.

```lisp
(defmacro defvar (name &optional (value nil value-p) documentation)
  (%proclaim-special name)
  (if value-p
      `(progn (%proclaim-special ',name)
              (if (boundp ',name) nil (setq ,name ,value))
              ',name)
      `(progn (%proclaim-special ',name) ',name)))
```

Two details deserve a sentence each. `defvar` assigns only if the variable has
no value yet, so that loading a file twice does not reset a variable a running
program is using — `defparameter` always assigns, because a parameter is
something the file means to set. And the proclamation happens twice: once when
the macro is *expanded*, which is during compilation, so a binding compiled
later in the same top-level form is already special; and once when the
expansion runs.

`boundp`, `symbol-value` and `set` came with it, as three small primitives.
They are the only Rust in this chapter besides the record and the instruction,
and they could not be Lisp: each reads or writes a symbol's value cell *named
at run time*, and no instruction can do that — `GREF` names its symbol in the
code. All three are Common Lisp functions in their own right.

The whole step cost 161 lines of Rust, out of the 1,915 phase 1 left.

## The tier-1 leg

The test is a new kind of differential file. `specials.lisp` can't run in the
interpreter, which knows nothing of special variables and never will, so it
has two legs rather than three: compiled Quokka, and SBCL. And its baseline
came from SBCL itself, through the same `oracle.lisp` the SBCL leg uses —
there was no interpreted run to take it from.

Every way out restores, nested bindings unwind innermost first, a closure
captures lexical variables but never a dynamic binding, `let` evaluates its
initial values before binding — forty-one forms, and compiled Quokka agreed
with SBCL on every one on the first run.

## What is not here

- **Local special declarations** — `(declare (special x))`, which makes one
  binding dynamic without proclaiming the symbol everywhere. Quokka has no
  declarations yet.
- **`progv`**, which binds a list of symbols computed at run time. It is step I,
  and will push the same records.
- **Threads.** With shallow binding, a thread switch must swap each thread's
  bindings out and in. That is phase 3's cost, chosen knowingly.

---

**References.** CLHS 3.1.2.1.1.2, *Dynamic Variables*, for the rules. Baker,
*Shallow Binding in Lisp 1.5* (CACM, 1978), for shallow and deep binding and
how a system can move between them.
