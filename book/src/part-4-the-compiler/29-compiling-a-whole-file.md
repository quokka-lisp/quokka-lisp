# Compiling a whole file

> **Where this fits.** Chapter 28 built a compiler that turns *one* form into
> a function the machine can run. This chapter turns it loose on a file — on
> `prelude.lisp`, six hundred lines of Lisp that nobody wrote to flatter a
> compiler, and then on the compiler's own source. It ends somewhere
> surprising: everything compiles, and **none of it is allowed to be
> installed**. Understanding why not is understanding what the image in
> chapter 30 is actually for.

Compiling one form at a time is a demonstration. A language is a file.

So the obvious next step is a *file compiler*: read a file, compile every form
in it, and leave the results behind — so that after compiling `prelude.lisp`
the system's `length` is compiled code rather than a tree being walked. That
is what every compiler you have used does. `javac` writes a class file;
`rustc` writes an object file; SBCL's `compile-file` writes a fasl and `load`
installs it.

Quokka's file compiler does the first half and refuses the second. It is worth
being precise about why, because "we did not get round to it" and "doing it
would break the system" look identical from outside, and only one of them is
true.

## The shape of the problem

The compiler is written in Lisp (chapter 28). Here is a fragment of it, the
part that turns a list of instructions into a function:

```lisp
(defun compile-form (form)
  (setq %code nil)
  (%reset-constants)
  (%compile form nil t)
  (%emit (list 'RET))
  (%assemble (reverse %code) (%constants-vector) nil 'compiled-form))
```

Count the prelude functions in five lines: `setq` is a special operator, but
`list` and `reverse` are ordinary functions defined in `prelude.lisp`.
Elsewhere the compiler uses `length`, `second`, `cddr`, `member`, `append` and
`apply` — all of them prelude functions, all of them interpreted.

Now suppose the file compiler installs what it compiles. It reaches the
prelude's definition of `length`, compiles it, and puts the compiled function
in `length`'s function cell. The compiler carries on to the next form, needs
to know how long a list is, calls `length` —

and the interpreter meets a compiled function.

This is the two-evaluators problem of chapter 24, arriving from the direction
nobody was watching. The rule is *one evaluator at a time*; installing
compiled code into a running interpreter breaks it on the very next call.

## The bug that was waiting there

The VM already refused the mirror image of this. Call an interpreted closure
from compiled code and it says so:

```
; bad CALL: reached an interpreted closure — compiled code can only call
  compiled code, and the prelude is not compiled yet
```

The interpreter had no such refusal. Here is what it did instead, in
`crates/quokka-bootstrap/src/eval.rs`:

```rust
fn apply_closure(&mut self, function: Value<'h>, arguments: Value<'h>) -> Transition {
    let params = self.mc.slot(function, closure::PARAMS);
    let body = self.mc.slot(function, closure::BODY);
    // ... bind the arguments ...
    self.begin(body, scope)
}
```

`begin` evaluates `body` as a sequence of forms. For an interpreted closure
the body is a list of forms, which is right. For a *compiled* one the body
would be a `Function` object — a heap object full of bytes — and a heap object
that is not a list **evaluates to itself**.

So `(length xs)` would not have crashed. It would have returned
`#<compiled LENGTH>`.

That is much worse than a crash. A crash stops at the mistake; a wrong value
travels. It would have been passed to arithmetic, stored in a vector, compared
against a number, and the error would have surfaced four functions away from
its cause with nothing pointing back.

The fix is three lines and it is the whole lesson:

```rust
if body.is_object() && self.mc.header(body).object_type == ObjectType::Function {
    return self.fail(EvalError::BadSyntax {
        operator: "a call",
        detail: "reached a compiled closure — the interpreter cannot run \
                 compiled code, and after the image is built it does not have to",
    });
}
```

**A rule enforced in one direction only is enforced where it happened to be
tested.** The VM's refusal existed because compiled code calling the prelude
was the first thing anyone tried. The interpreter's absence of one existed
because nothing had yet handed the interpreter a compiled function — not
because anyone had decided it was safe.

This is a general shape and it is worth naming, because it recurs in every
system with two halves that must not mix: *symmetry of enforcement*. If
crossing a boundary is forbidden, both sides check, and the side nobody
crosses yet is the side that will be crossed by accident.

## What the file compiler does instead

`boot::compile_source` compiles every form and throws the result away.

That sounds pointless, and the trick is what it does in between: it hands each
compiled function to the **verifier** of chapter 25 before discarding it.

```rust
crate::code::verify::function(mc, f).err().map(|e| format!("{e:?}"))
```

Now the exercise has content. "It compiled" only means the compiler did not
signal an error, and a compiler is perfectly capable of emitting a jump that
lands in the middle of an instruction while feeling fine about it — chapter 25
exists because exactly that happened. "It compiled *and verified*" means the
bytes are a program the machine can actually run: every path through it agrees
about how deep the stack is, every jump lands on an instruction boundary,
every constant index is one the function has, and the stack it reserved is
deep enough.

So the file compiler is a test of the compiler's **output**, across every form
of a real file, rather than a test of its temper.

## Macros are the exception, for the same reason

One kind of form is evaluated rather than compiled:

```rust
if form.is_cons() && mc.symbol_is(mc.car(form), "DEFMACRO") {
    // evaluate it in the interpreter
}
```

A *macro* (chapter 11) is a function that runs at compile time and returns a
form to compile instead. So when the compiler meets `(when x y)` it **calls**
`when`'s expander — from the compiler, which is interpreted code.

An expander compiled here could not be called by the interpreted compiler. The
very next form that used the macro would fail. So the expander has to exist,
in the interpreter, *before* the forms that use it are compiled.

This is the **bootstrap rule**, and it is not a special case so much as the
same rule from a third angle: compile time and run time are different times,
and during the bootstrap they are served by different evaluators. The REPL had
been applying the rule by hand since chapter 28; now there is one copy of it,
in the file compiler, where it belongs.

## Trying it

```bash
cargo run -p quokka-repl -- bootstrap --compile-file lisp/prelude.lisp
```

```
lisp/prelude.lisp: 45 forms compiled and verified, 13 macro definitions evaluated
```

Fifty-eight top-level forms, which is what six hundred lines of Lisp comes to
when a `defun` is twenty lines. All of them: `defstruct` with the ten
functions it generates (chapter 21), `handler-case` and its unwinding
(chapter 20), `setf` and its expanders, the sequence functions, multiple
values (chapter 23).

And then the one that matters:

```bash
cargo run -p quokka-repl -- bootstrap --compile-file lisp/compiler/compile.lisp
```

```
lisp/compiler/compile.lisp: 40 forms compiled and verified, 0 macro definitions evaluated
```

**The compiler compiles itself.**

Be precise about what that does and does not mean, because this is the point
where it is easy to claim too much. This is the compiler *interpreted*,
compiling the *text* of the compiler, and throwing the answer away. It is not
self-hosting: nothing is yet running compiled. What it is, is the
precondition. An image containing a compiled compiler can only be built if the
compiler can compile the compiler, and now that is a fact with a test behind
it rather than a hope.

Zero macro definitions, incidentally, is not an accident either: the compiler
is written in tier 0 (rule 5) and defines no macros of its own, which is one
less circularity to think about.

## Where the second half went

So where does installing happen, if not here?

**In the image, in chapter 30.** The sequence is:

1. Start an empty heap and interpret the prelude, as every boot does today.
2. Interpret the compiler on top of it.
3. Compile every prelude form and every compiler form — and this time, keep
   the results, writing them into the function cells.
4. Save the whole heap to a file.
5. Throw that process away. It has done its job.

Step 3 is the dangerous one, and it is safe there and not here for a reason
worth stating plainly: **that heap is never used again**. Nothing calls
`length` after it has been replaced, because nothing runs in that heap after
step 3 — it is written to disk and the process exits. The next session starts
by loading the image, and in *that* session everything is compiled from the
first instruction. There is no mixed state because there is no moment when
half the system is compiled and something is still running.

This is the same manoeuvre as the bootstrap itself (`the-bootstrap.md`): do
the slow, fragile thing exactly once, keep the answer, and never be in that
position again. The interpreter does not have to be able to call compiled code
— it has to not exist by the time compiled code is being called.

## Status

Built and tested. `--compile-file` (today `quokka bootstrap --compile-file`) works on any tier-0 file; the prelude and
the compiler both compile clean and verify clean.

Steps 3, 4 and 5 are built too. Chapter 30 is the image; chapter 32 is
step 3, compiling the prelude and the compiler *and keeping the results*, in
exactly the heap this chapter said it belonged in — one that is saved and
abandoned straight afterwards.

And the check that the compiled world means what the interpreted one meant —
every differential file, run compiled, against the same baseline — is chapter
33's, and it found two things the verifier could never have seen, because a
program can be perfectly well-formed and still compute the wrong thing.

---

**References.** Queinnec, *Lisp in Small Pieces*, chapter 9, on macros
and the separation between the world a compiler expands in and the world its
output runs in — the root of the bootstrap rule. Hart and Levin, *The New
Compiler*, MIT AI Memo 39 (1962), for the first compiler brought up by
running it in the interpreter of the language it compiles. Steele and Gabriel, *The
Evolution of Lisp* (1993), §2.9, for how real Lisps have drawn the compile
time / run time line and what `eval-when` exists to say about it. The CLHS
entry for `compile-file`, §3.2.3, on *literal objects in compiled code* and
the similarity rules — the standard's own treatment of the fact that a
compiled file is built in one world and loaded into another.
