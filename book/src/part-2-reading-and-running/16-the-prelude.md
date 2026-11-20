# The prelude

> **Where this fits.** Chapter 15 built macros. This chapter spends them. Up
> to now every word Quokka understood was a word some Rust function
> recognised; from here on there is a file of Lisp that the system reads at
> startup, and most of what a user types is defined in it. It is the first
> moment Quokka is a *Lisp system* rather than a Lisp interpreter — and the
> difference between those two things is the subject of the chapter.

## Two kinds of language

Here is a question that sounds pedantic and is not. When you write

```lisp
(when (ready-p x) (go-ahead x))
```

in Common Lisp, what *is* `when`?

In most languages the answer is "a keyword" — a word the compiler knows,
listed in a grammar, implemented in the compiler's own source. If you want a
different one you file an issue and wait a few years.

In Common Lisp the answer is "a macro, defined in a file, in Common Lisp". It
is not privileged. You could write it yourself in three lines, you could
redefine it, and the one the standard describes is written in the same
language as the one you would write. The *implementation* contains perhaps
twenty-five forms that cannot be defined this way (chapter 13). Everything
else — `when`, `unless`, `cond`, `and`, `or`, `dolist`, `case`, `defstruct`,
and in a real CL about a thousand more — is a library.

The file that holds the first of them is conventionally called the **prelude**
(or the "runtime", or "the Lisp side"). Quokka's is
[`lisp/prelude.lisp`](https://github.com/quokka-lisp/quokka-lisp/blob/main/lisp/prelude.lisp),
and this chapter is about getting it to load, what went in it, and the two
bugs that appeared the moment it did.

## Where the file lives

A prelude has to be *somewhere* when the system starts. The obvious answer —
read it off the disk next to the binary — is wrong here, twice:

- Rule 2 says the core VM crate has no host dependencies, so no `std::fs`.
- Rule 2 also says every commit builds for `wasm32`, where "the file next to
  the binary" is not a thing that exists. A browser has no directory to look
  in.

So the text is compiled into the binary:

```rust
/// The prelude source, compiled into the binary.
///
/// Changing `lisp/prelude.lisp` therefore forces a rebuild, which is correct:
/// it is part of the system, not configuration.
pub const PRELUDE: &str = include_str!("../../../lisp/prelude.lisp");
```

`include_str!` is a Rust macro that reads a file **at compile time** and
leaves its contents in the binary as a string constant. The VM carries its own
Lisp. Native and browser builds get the same bytes, which also means they
cannot drift apart — the same property the differential tests exist to check,
obtained here for free.

There is a pleasing symmetry in that: the Rust macro `include_str!` runs when
the compiler builds the VM; the Lisp macros in the file it includes run when
the VM builds its own vocabulary. Both are code arriving before the thing that
uses it starts.

### It is scaffolding, and it has a demolition date

Interpreting the prelude at every start is not the plan. Phase 1 saves an
**image** (chapter 7) — the whole heap written to a file — and an image
already contains everything the prelude *did*: the symbols, the macro objects,
the closures, all sitting in the heap where they were left. Booting from an
image skips this module entirely.

Which is the same shape as the bootstrap itself: do the slow thing once, keep
the answer, never do it again. The prelude loader is a stand-in for an image,
and when images exist it stops being called. It costs about 2,500 heap words
today — roughly 20 KB — and a few milliseconds. Small enough to ignore, and
worth measuring anyway, which is why there is a test that fails if it grows:

```rust
#[test]
fn booting_costs_a_known_amount_of_heap() {
    let mut heap = Heap::new();
    install(&mut heap).expect("the prelude boots");
    heap.collect();
    let words = heap.used_words();
    assert!(words < 4_000, "...");
}
```

A number nobody writes down is a number that doubles without anyone noticing.

## Loading it

The loader is twenty lines, and one detail in it is worth stopping on:

```rust
loop {
    form_number += 1;
    // Read and evaluate inside one `mutate`, because a `Value` cannot
    // leave one. The reader itself holds no values — only an offset into
    // the text — so it is free to live across the boundary.
    let outcome = heap.mutate(|mc| match reader.read(mc) {
        Err(e) => Some(Err(BootError::Read(e))),
        Ok(None) => None,
        Ok(Some(form)) => match eval_now(mc, form, BOOT_BUDGET) {
            Ok(_) => Some(Ok(())),
            Err(error) => Some(Err(BootError::Eval { form: form_number, error })),
        },
    });
    ...
}
```

Each form gets **its own `mutate`**. Chapter 4 established that a `Value`
cannot leave a mutation — that is what the lifetime brand enforces — so
reading a form and evaluating it must happen inside the same one. But
wrapping the *whole file* in a single `mutate` would be a mistake: no
collection can happen inside a mutation, so the entire prelude would allocate
with no collector allowed to run. Per-form mutations let the collector work
between them.

The `Reader` can live outside the loop because it holds no `Value`s — only an
offset into a slice of characters. That it compiles at all is the brand
telling the truth about what is safe.

Note also `form_number`. When a prelude form fails you want to know *which
one*, and "form 8" is a thing you can count to. This is not hypothetical: it
is how the first boot failure was diagnosed, below.

## What went in

### defun

The first entry in the prelude is the one this chapter exists for. Before it,
`defun` was a special operator written in Rust. In Common Lisp it is a macro,
and now it is one here:

```lisp
(defmacro defun (name args &body body)
  `(progn
     (%set-symbol-function
      ',name
      (lambda ,args
        ,@(if (%mentions-return-from name body)
              (list (cons 'block (cons name body)))
              body)))
     ',name))
```

One new primitive was needed. `%set-symbol-function` writes a symbol's
function cell, which is the one thing `defun` does that Lisp previously had no
way to say. Common Lisp spells this `(setf (symbol-function 'name) f)` — but
`setf` is itself a macro, and it has to expand into *something*. The writer
comes first and the sugar comes later. The `%` marks it as Quokka's own rather
than Common Lisp's, which is what rule 6 asks for while there are no packages
to put it in.

### The bug that moving it exposed

CLHS says the body of a `defun` is wrapped in a **block named after the
function**. That is what makes this work:

```lisp
(defun find-it (x)
  ...
  (return-from find-it hit)
  ...)
```

The Rust `defun` did not do it. Nobody noticed for the entire life of the
function, because an omission inside a Rust function is invisible — there is
no line that is missing, just behaviour that never happens. SBCL answered
`ZERO` and Quokka answered `error`, and no test asked.

Writing it as a macro made the omission a thing you could see, because the
expansion is a list and the block is a word in it. **This is the argument for
moving code above the line, and it is not about line counts.** It is that a
policy written in the language it governs can be read by the people who are
governed by it.

### The regression that followed immediately

Adding the block broke tail calls.

A `block` pushes a frame. A frame outstanding at a call is precisely what
tail-call elimination must *not* have (chapter 10). So

```lisp
(defun count-down (n) (if (= n 0) 'done (count-down (- n 1))))
```

went from constant frame-stack usage to one frame per call. The test said so
within a minute:

```text
the stack reached 20002 frames; a tail call grew it
```

Twenty thousand frames for twenty thousand calls. Note what did *not* happen:
nothing crashed. The frames live in Quokka's own heap, not Rust's call stack
(chapter 2), so the program still produced the right answer — it just used
memory proportional to the recursion depth instead of none. A design decision
quietly stopped being true, and only a test that measured the stack caught it.

Real Common Lisp compilers fix this by flow analysis: if nothing returns from
the block, the block is dead and is deleted. Quokka does the crude version of
the same thing — a tree walk at expansion time:

```lisp
(%set-symbol-function
 '%mentions-return-from
 (lambda (name form)
   (if (consp form)
       (cond ((eq (car form) 'return-from)
              (if (consp (cdr form)) (eq (car (cdr form)) name) nil))
             (t (or (%mentions-return-from name (car form))
                    (%mentions-return-from name (cdr form)))))
       nil)))
```

If the body mentions `(return-from name ...)` anywhere, emit the block;
otherwise do not. The errors it can make are asymmetric, which is what makes
it acceptable: a false positive (the body quotes the *symbol* `return-from`
in some data) costs one frame and nothing else. A false negative would be a
real bug — and there is exactly one way to get one, which the prelude says out
loud rather than hiding:

> **The known gap**: a *macro* in the body that expands into `(return-from
> name ...)` is not visible to this walk, and would fail with "there is no
> exit named NAME here". Nothing in Quokka's own sources does that. A real fix
> is to expand the body first, which needs `macroexpand-all`, which needs the
> full lambda-list machinery — later.

Writing the limitation down is not an apology. It is the difference between a
known gap and a surprise.

### Notice where that function is defined

```lisp
(%set-symbol-function '%mentions-return-from (lambda (name form) ...))
```

Not `defun`. It cannot be `defun`, because `defun` is what it is for — the
macro calls it while expanding, so it has to exist before `defun` does.

This is the bootstrap problem in miniature, and the solution is the same one
the whole project uses: write one thing by hand, in terms of what already
exists, and use it to get the convenient version. `defun` expands into
`%set-symbol-function` and `lambda`, so writing a function with
`%set-symbol-function` and `lambda` directly is simply doing by hand what
`defun` is about to automate. Two lines of inconvenience, and the circle is
broken.

### The conditionals and connectives

```lisp
(defmacro when (test &body body)
  `(if ,test (progn ,@body) nil))

(defmacro unless (test &body body)
  `(if ,test nil (progn ,@body)))
```

That is the entire implementation of `when` in Quokka, and essentially the
entire implementation of `when` in any Common Lisp.

`cond` is more interesting, because it expands into a use of *itself*:

```lisp
(defmacro cond (&rest clauses)
  (if (null clauses)
      nil
      (let ((clause (car clauses))
            (rest (cdr clauses)))
        (if (null (cdr clause))
            (let ((once (gensym)))
              `(let ((,once ,(car clause)))
                 (if ,once ,once (cond ,@rest))))
            `(if ,(car clause)
                 (progn ,@(cdr clause))
                 (cond ,@rest))))))
```

A macro that expands into itself is not a loop. Each expansion handles one
clause and hands the rest back to `cond`, so every round is one clause
shorter, and the empty case stops it. The result is a nest of `if`s exactly as
deep as the number of clauses — which is what you would have written by hand,
produced by a program instead.

The awkward case is a clause with no body. CLHS says `(cond (x))` returns *x's
value*, so the test cannot simply be evaluated twice: `(cond ((pop queue)))`
would pop twice. It is bound to a `gensym` instead, for exactly the reason
chapter 15 gave.

`and` and `or` are the same shape, and they illustrate why macros and not
functions:

```lisp
(defmacro and (&rest forms)
  (cond ((null forms) t)
        ((null (cdr forms)) (car forms))
        (t `(if ,(car forms) (and ,@(cdr forms)) nil))))
```

A function receives values, and `(or (safe-p x) (blow-up x))` must be able to
*not call* `blow-up`. Short-circuiting is control flow, and control flow is
the thing functions cannot do to their own arguments.

### And some ordinary functions

```lisp
(defun cadr (x) (car (cdr x)))
(defun not (x) (null x))

(defun length (list)
  (labels ((walk (rest n)
             (if (endp rest) n (walk (cdr rest) (+ n 1)))))
    (walk list 0)))
```

`cadr` in Rust would be a primitive nobody could change. Here it is nine
characters of Lisp and costs the 15k-line budget nothing.

`length` is written with the tail recursion from chapter 10 — the recursive
call is the last thing `walk` does, so it runs in constant frame stack however
long the list is. **No iteration construct was needed to write any of this.**
`tagbody` exists (chapter 13) and the prelude does not use it once, because
tail recursion is iteration when the evaluator is honest about tail calls.

The helper is a `labels` rather than a global, so the prelude does not leave
names lying about that nobody asked for.

## Order is not a style question

There is no forward reference and no second pass. Each form is evaluated as it
is read, so anything a form uses must already exist. `cond` comes before
`and`, because `and` is written with `cond`. `defun` comes after both, because
it uses them.

This stops being advice and becomes a boot failure very quickly. The helper
above was first written as `(not (consp form))`, and `not` is defined further
down the file:

```text
prelude form 8 failed: the function NOT is undefined
```

Which is a good error — it names the form by number, so the file can be opened
and counted. That is `form_number` earning its keep, from twenty lines up.

## Did anything actually move?

Honest accounting, since this chapter has been pleased with itself for a
while:

| Left Rust | Arrived in Rust |
| --- | --- |
| `eval_defun`, ~25 lines | `%set-symbol-function`, ~10 lines |
| one special operator | `symbol-function`, ~10 lines |
| | `boot.rs`, ~60 lines |

The Rust core got **bigger**. Anyone selling this as a line-count win is
selling something.

What actually changed is three things:

1. **A bug was fixed** that had been invisible for as long as the function
   existed, and fixed in a way that makes the next such bug visible too.
2. **The special operator count is honest.** `defun` is a macro in Common
   Lisp, and Quokka listing it as an operator was a lie about the language it
   claims to implement.
3. **The mechanism exists.** `boot.rs` is paid for once. Every macro from here
   — `dolist`, `push`, `case`, `defstruct`, and eventually most of the
   compiler — costs the Rust budget nothing at all. That is the whole argument
   for rule 1, and it only starts paying from the second customer onwards.

### What stayed, and why

`lambda` did not move, although it is also a macro in Common Lisp, expanding
to `(function (lambda ...))`.

The reason is that `function` has to turn a lambda expression into a closure
no matter what — capturing the environment is a primitive act, not something
Lisp can do to itself — so `eval_lambda` cannot leave the Rust core. Making
the *symbol* a macro as well would move no work, only a dispatch. The line is
in the right place already, and moving something across it for the sake of
tidiness is not an improvement.

`defmacro` stays too, and permanently. A macro that defines macros has to
bottom out somewhere, and that somewhere is the bootstrap floor.

## What this unlocks

The prelude is a file now, and files grow. The next entries in it are
`dolist`, `dotimes`, `push`, `pop` and `case` — all macros over `tagbody` and
`let`, all of them Lisp, none of them costing a line of Rust. After that
`defstruct`, which needs vectors first.

And eventually the compiler, which is the point of all of it: a Lisp program
that Quokka interprets once in order to never have to interpret anything
again.

---

**References.** CLHS 3.1.2.1.2.2 for what a macro call means, and the `defun`
entry for the implicit block. Peter Seibel, *Practical Common Lisp* (2005),
chapters 3 and 8, for what a prelude of macros feels like to use. Christian
Queinnec, *Lisp in Small Pieces* (1996), chapter 10, on bootstrapping a
library in the language it extends.
