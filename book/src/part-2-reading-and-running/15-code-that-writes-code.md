# Code that writes code

> **Where this fits.** Chapter 13 listed twenty-five special operators and
> admitted the number was arbitrary. This chapter is how the list stops
> growing. A macro lets a program add to the language without touching the
> evaluator, which means every convenience from here on — `when`, `cond`,
> `and`, eventually `defun` itself — can be written in Lisp instead of Rust.
> It is the first time rule 1 ("Rust below the primitive table, Lisp above
> it") earns its keep rather than merely being obeyed.

## The thing a function cannot do

Here is something that looks like it should be a function:

```lisp
(my-when (file-exists-p "/tmp/x")
  (delete-file "/tmp/x")
  'deleted)
```

Run the body only if the test is true. Try writing it as a function:

```lisp
(defun my-when (test &rest body) ...)
```

By the time the body of `my-when` starts running, `(delete-file "/tmp/x")` has
**already happened**. A function receives values, and getting a value out of
`(delete-file "/tmp/x")` means deleting the file. The test has not even been
looked at yet.

This is the same wall chapter 13 ran into with `if`, and the answer there was
to make `if` a special operator — a form the evaluator recognises by name and
treats differently. That answer does not scale. Every convenience anyone ever
wants would have to be built into the evaluator, in Rust, by whoever maintains
the evaluator. A language whose users cannot extend it is a language whose
users file feature requests.

## A function from code to code

A **macro** is a function that runs at a different time, on different data.

An ordinary function is called with *values* and returns a *value*. A macro is
called with the argument **forms**, unevaluated — the actual list structure the
reader produced — and returns another form. That returned form is then
evaluated in place of the original call. The macro itself has disappeared by
the time anything runs.

```lisp
(defmacro my-when (test &body body)
  (cons 'if (cons test (cons (cons 'progn body) nil))))
```

Called as `(my-when t 1 2 3)`, the macro receives `test` = the symbol `T` and
`body` = the list `(1 2 3)`. Nothing has been evaluated. It builds the list
`(IF T (PROGN 1 2 3) NIL)` and hands it back, and *that* is what the evaluator
runs.

Three different times are now in play, and keeping them apart is most of
understanding macros:

| Time | What happens | Chapter |
| --- | --- | --- |
| **Read time** | text becomes list structure | [8](08-text-becomes-structure.md) |
| **Expansion time** | macros run, producing more list structure | this one |
| **Run time** | the resulting structure is evaluated | [9](09-what-a-form-means.md) |

The reason this is possible at all is the one from chapter 8: Lisp programs
*are* lists. A macro is an ordinary list-manipulating program, and the thing it
manipulates happens to be the program. In a language whose source is text, the
equivalent tool is a preprocessor working on strings, which is why C's macros
are a byword for pain. The technical name for the property is **homoiconicity**
— code and data have the same representation — and macros are the entire reason
anybody cares about it.

## Expansion in the machine

Chapter 9's machine alternates **Eval** and **Return**, with a stack of frames.
Adding macros to it is three lines in the apply path and one new frame kind:

```rust
// A macro takes the argument *forms*, unevaluated, and its result is
// evaluated in place of the call. Nothing is evaluated on the way in,
// so there is no argument-collecting frame — just the expansion.
if closure::is_macro(self.mc, function) {
    let f = frame::make_marked(self.mc, frame::EXPAND, env);
    self.push(f);
    return self.apply_closure(function, argument_forms);
}
```

Compare this to the ordinary call just below it in the same function. An
ordinary call pushes an `ARGS` frame and starts evaluating the first argument.
A macro call pushes an `EXPAND` frame and goes straight to applying the
closure, handing it `argument_forms` — the forms themselves, untouched. The
*absence* of the argument-evaluating step is the whole of what makes a macro a
macro.

When the macro body finishes, its value arrives at the `EXPAND` frame:

```rust
frame::EXPAND => {
    let expansion = self.mc.register(Register::Value);
    // Back to where the macro was *called*, not where it was
    // written. `(let ((v 10)) (twice v))` depends on exactly this.
    let caller_env = self.mc.slot(f, frame::EXPAND_ENV);
    self.mc.set_register(Register::Env, caller_env);
    self.mc.set_register(Register::Control, expansion);
    self.mc.set_mode(Mode::Eval.as_u8());
    Transition::Continue
}
```

Put the expansion in `Control`, switch back to Eval, carry on. A value that
arrived is treated as a form to evaluate — which is exactly the sentence "the
expansion is evaluated in place of the call", written in registers.

### The bug that proves the point

Those two lines restoring `Env` were not there the first time. Without them,
the tests said this:

```lisp
(let ((v 10)) (twice 1))   ; => (1 1)    correct
(let ((v 10)) (twice v))   ; => error: the variable V is unbound
```

Constants worked; variable references did not. The expansion was being
evaluated in the *macro's* environment — the scope the macro body ran in —
rather than in the scope where the macro was called.

This is worth sitting with, because it is a mistake that only exists in a
machine of this shape. In a recursive `eval`, the caller's environment is a
local variable in a Rust stack frame, and it comes back by itself when the
recursive call returns. Chapter 2 gave that up: the environment is a
**register**, and a register that gets clobbered has to be put back by whoever
clobbered it. Applying the macro closure sets `Env` to the macro's own scope,
so something has to remember the old one. That is all the `EXPAND` frame does;
it carries no other information.

The symptom is also instructive. A test using only literals would have passed.
It took `(let ((v 10)) (twice v))` — a macro called somewhere that a variable
is in scope — to notice, which is a good argument for tests that look like the
code people actually write.

## Writing the expansion down

`my-when` above is correct and nearly unreadable:

```lisp
(cons 'if (cons test (cons (cons 'progn body) nil)))
```

Five `cons` calls to describe a four-element list. The output is buried in the
plumbing, and a macro of any size becomes write-only. What is wanted is to draw
the result and mark the holes:

```lisp
(defmacro my-when (test &body body)
  `(if ,test (progn ,@body) nil))
```

The backquote `` ` `` means *this is a template*. A comma `,` means
*this hole is filled by evaluating what follows*. A comma-at `,@` means *this
hole is filled by splicing in the elements of a list*. The template is the
shape of the output, at the size it will be.

This is **quasiquotation**, and it was not invented for Lisp — it comes from
Willard Quine's 1940 *Mathematical Logic*, where the corner quotes ⌜ ⌝ do the
same job for logical formulae: quote this expression, except the parts I am
leaving as variables.

### It is not a special operator

Backquote is handled in the **reader**, before the evaluator ever sees the
form. `` `(if ,test (progn ,@body) nil) `` is read as, and is
indistinguishable from, this hand-written code:

```lisp
(cons (quote if)
      (cons test
            (cons (cons (quote progn) (append body (quote nil)))
                  (quote nil))))
```

A reader construct that expands into ordinary code is a **reader macro**.
Quokka's is the one from [Steele's *Common Lisp the Language*][cltl], three
rules and a recursion:

| Template | Expands to |
| --- | --- |
| `` `atom `` | `(quote atom)` |
| `` `,x `` | `x` |
| `` `(a . rest) `` | ``(cons `a `rest)`` |
| `` `(,@x . rest) `` | ``(append x `rest)`` |

Read the third rule carefully: a backquoted list is a `cons` of the backquoted
head onto the backquoted tail, so the rules apply again to each part, all the
way down. The whole implementation is about thirty lines.

Doing it in the reader rather than the evaluator is the choice CLHS 2.4.6
describes and that every implementation makes. It costs nothing at run time —
the expansion happens once, when the text is read — and it keeps the special
operator count at twenty-five. A `quasiquote` special operator would have been
the twenty-sixth, and the evaluator would have had to grow a recursive template
walker.

### A symbol nobody can type

Transforming a template needs somewhere to record "a comma was here" between
reading the comma and handling the enclosing list. Quokka marks it with a
symbol:

```rust
// The names contain a space on purpose. This reader has no
// `|bar quoting|`, so a space ends a token and *no input can
// name these symbols*. A marker a user could write by hand —
// `%UNQUOTE`, say — would make `(a (%unquote b)) expand as
// though a comma had been typed. Unlikely, but wrong, and the
// fix costs one character.
let tag = mc.intern(if splicing {
    "SPLICE MARKER"
} else {
    "UNQUOTE MARKER"
});
```

The first version used `%UNQUOTE`, which a sufficiently unlucky program could
have written by hand. The repair is the cheapest possible one: pick a name the
reader cannot produce. It is the same instinct as chapter 12's — *a rule in
prose is a hope; put it where it fails* — applied to a rule about what input
can exist.

## The leak

Now a macro that is nearly right. This is `or` for two arguments, binding the
first value so it is evaluated only once:

```lisp
(defmacro or2-badly (a b)
  `(let ((v ,a)) (if v v ,b)))
```

Watch it with a caller whose own variable happens to be called `v`:

```lisp
(let ((v 'outer)) (or2-badly nil v))   ; => NIL
(let ((w 'outer)) (or2-badly nil w))   ; => OUTER
```

The same program, one variable renamed, two different answers. `,b` is
substituted *inside* the `let` the macro invented, so the caller's `v` is no
longer the `v` that gets evaluated. The macro reached out and took a name that
was not its to take.

This is **variable capture**, and it is the classic macro bug. Note that it is
invisible at the call site: nothing about `(or2-badly nil v)` suggests that the
name `v` is dangerous, and the macro's author never meant to publish it.

The cure is to bind a name the caller could not possibly have written:

```lisp
(defmacro or2 (a b)
  (let ((tmp (gensym)))
    `(let ((,tmp ,a)) (if ,tmp ,tmp ,b))))
```

`gensym` makes a fresh symbol that is **uninterned** — it is not in the
obarray, the table chapter 8 uses to make sure that reading `FOO` twice yields
the same symbol. Reading the text `G17` would produce a *different* symbol from
the `G17` that `gensym` returned, because interning is what makes two readings
of a name agree. A name that cannot be written cannot collide.

```rust
pub fn gensym(&mut self) -> Value<'h> {
    self.storage.gensym_counter += 1;
    let n = self.storage.gensym_counter;
    let text = self.alloc_string(&format!("G{n}"));
    let s = self.alloc(ObjectType::Symbol, symbol::SLOTS);
    self.set_slot(s, symbol::NAME, text);
    self.set_slot(s, symbol::VALUE, Value::UNBOUND);
    self.set_slot(s, symbol::FUNCTION, Value::UNBOUND);
    s
}
```

Eight lines, and the counter is a plain `u64` rather than a `Value`, so the
collector never has to think about it. The name is only for printing; the
identity is the object.

### Hygiene, and why Quokka does not have it

Scheme solved this differently. `syntax-rules` and `syntax-case` macros are
**hygienic**: the expander renames introduced bindings automatically, so
capture cannot happen at all, and the macro writer never calls anything like
`gensym`. The technique is Kohlbecker's algorithm (1986), and it is genuinely
better at the thing it does.

Common Lisp did not adopt it, and Quokka follows Common Lisp (rule 6). Three
reasons, in descending order of how much they matter here:

1. **Compatibility is the whole point.** A CL subset whose macros are hygienic
   is not a CL subset. Real code calls `gensym`.
2. **Capture is sometimes wanted.** *Anaphoric* macros deliberately introduce a
   binding the caller can see — `(aif (lookup x) (use it))`, where `it` is the
   macro's doing. Hygiene has to provide an escape hatch for this; unhygienic
   macros are the escape hatch.
3. **The machinery is large.** Renaming correctly means tracking which
   identifiers came from where through arbitrarily nested expansions. Against a
   15k-line budget (rule 7) that is a lot of lines for a problem one call to
   `gensym` closes.

The cost is honest: it is an error the language lets you make. The defence is
that it is a *known* error with a *known* cure, written down here and
demonstrated in `tests/lisp/macros.lisp`, which keeps both versions of `or2` so
the difference stays visible.

## append, and the first brush with rule 9

Splicing needs `append`, so `append` became a primitive — and it is the first
one for which rule 9 ("primitives are bounded") is a live question rather than
a formality.

Every primitive so far has been O(1): `car`, `cons`, `+`. `append` copies all
but its last argument, so its cost grows with the *data*, not with the call.
Chapter 2's slicing promise — the VM always returns control to the browser
within a bounded time — assumes a single step is short, and a step that is
inside a Rust primitive cannot be interrupted by anything.

The decision for now, recorded in the code:

```rust
// Copies all but its last argument, so its cost grows with the *data*
// rather than the call — the first primitive here for which rule 9 is a
// live question. It is bounded by what is already in the heap, which is
// bounded by the heap, so a single call cannot run away; but a long append
// is a long step, and when slices start mattering this is the one to chunk.
```

Bounded, because a list long enough to matter would not fit; not *short*,
which is a different promise. The honest statement is that this is a debt, not
a solved problem, and it is written next to the code that owes it.

The last argument is shared rather than copied, which is why `(append '(1) 'x)`
is `(1 . X)` and not an error. That is specified behaviour, it matches SBCL,
and the test file checks it.

## Seeing what happened

A macro turns code you wrote into code you did not, and then the evaluator
runs the second one. When it goes wrong, the thing that is wrong is invisible
— which is why the first tool anyone wants is a way to look:

```lisp
(defmacro my-when (test &body body)
  `(if ,test (progn ,@body) nil))

(macroexpand-1 '(my-when t 1 2))     ; => (IF T (PROGN 1 2) NIL)
```

`macroexpand-1` runs one expansion and hands back the form. `macroexpand`
keeps going until the head is no longer a macro, which matters when macros
expand into macros — as they usually do, once a prelude exists:

```lisp
(defmacro outer (x) (list 'my-when t x))

(macroexpand-1 '(outer 5))   ; => (MY-WHEN T 5)
(macroexpand   '(outer 5))   ; => (IF T (PROGN 5) NIL)
```

Both are written in Lisp, in the prelude, and the reason they *can* be is the
chapter's own point turned around. Expanding a macro means **running** it, and
running it means running Lisp. A Rust primitive that wanted to do that would
have to call the evaluator from inside itself, putting Lisp back on the Rust
stack — the same wall chapter 18's default forms ran into, and refused for the
same reason (rule 4).

So two small things went into Rust and the rest went into Lisp.
`%macro-function` fetches a symbol's expander, and `apply` calls a function
with a list of arguments. `apply`, like `funcall` before it, is *intercepted by
the evaluator* rather than implemented as a primitive, precisely because making
a call is the machine's business. With those two, the whole of `macroexpand-1`
is:

```lisp
(defun macroexpand-1 (form)
  (if (consp form)
      (let ((head (car form)))
        (if (symbolp head)
            (let ((expander (%macro-function head)))
              (if expander
                  (apply expander (cdr form))
                  form))
            form))
      form))
```

The `%` on `%macro-function` is doing real work. Common Lisp has a
`macro-function`, and it returns something else — a function of two arguments,
the whole form and an environment. Giving the standard's name to something
with a different contract would be worse than not having the name, so the
Quokka-specific one is marked as Quokka's.

**One divergence, on purpose.** The standard's `macroexpand` returns two
values — the form, and whether anything was expanded — and loops on the
second. Quokka has no multiple values yet, so it loops while the form keeps
*changing*, compared with `eq`. The two differ only for a macro that returns
the identical form object it was handed: the standard says that counts as an
expansion, so a conforming implementation loops forever, and this one stops.
Diverging towards terminating seemed the better way to be wrong.

## What is still missing

Status, as of this chapter:

- **Nested backquote is wrong.** `` `(a `(b ,,c)) `` expands the inner
  backquote first, which is not what the standard says. Real nested
  quasiquotation needs the depth-aware algorithm in [CLtL2][cltl] §2.4.6, and
  nothing in Quokka's own Lisp sources needs it yet.
- **No `macrolet`**, so a macro cannot be local to a form.
- **Lambda lists are still simple.** `&optional` with default values,
  `&key`, and destructuring (`(defmacro m ((a b) c) ...)`) are all absent.
  `&optional` binds `nil`, and that is the whole of it.

## What this unlocks

Chapter 13 said the special operator list was arbitrary and that most of Lisp
is not in it. This is the chapter where that becomes true rather than
aspirational. `when`, `unless`, `cond`, `and`, `or`, `dolist`, `push` — none of
them needs the evaluator to change. They are macros, they will be written in
Lisp, and they are the beginning of a core library that lives above the
primitive table.

`defun` is currently a special operator, which is slightly embarrassing: it is
a macro in every real Common Lisp, expanding roughly into `(setf (symbol-function
'name) (lambda ...))`. Moving it up is now possible, and is the first test of
whether the line in rule 1 is in the right place.

---

**References.** Paul Graham, *On Lisp* (1993), chapters 7–10 — still the best
account of what macros are for, and chapter 9 is entirely about capture.
Christian Queinnec, *Lisp in Small Pieces* (1996), chapter 9, for macro
expansion as an interpreter design problem. Guy Steele, *[Common Lisp the
Language, 2nd edition][cltl]*, §2.4.6 for the backquote rules. Eugene
Kohlbecker et al., "Hygienic Macro Expansion" (LFP 1986), for what Scheme does
instead. CLHS 3.1.2.1.2.2 for what the standard says a macro call means.

[cltl]: https://www.cs.cmu.edu/Groups/AI/html/cltl/clm/node1.html
