# The special operators, one by one

> *Where this fits.* A reference chapter. Chapter 9 explained *why* special
> operators must exist; this one goes through all twenty-five, as the twelve
> tier 0 built and the thirteen that came later, and says where each of the
> later ones landed. Each entry answers the same four questions: what it does,
> why it cannot be an ordinary function, why you would want it, and what it
> makes possible. Two of the twenty-five turn out not to need to be special at
> all, which says something about how the list was assembled.
>
> It can be read straight through or dipped into.

## The twenty-five

Common Lisp has exactly **25 special operators**. That number is the
irreducible core of the language: everything else, `when`, `cond`, `dolist`,
`defun`, `loop`, all of it, is built out of these by macros.

Two things are worth clearing up before the list, because both trip people up.

`lambda` and `defun` are not on it. `lambda` is a macro expanding to
`(function (lambda ...))`, and `defun` is a macro that stores a function in a
symbol. Quokka implements both as special operators because it has no macros
yet, which is a shortcut rather than a tier 0 item.

The test for "must be special" is also narrower than it looks. An ordinary
function receives its arguments *already evaluated*, so a form must be special
exactly when it needs to control whether, when, or in what environment its
arguments are evaluated, or when its arguments are not expressions at all. Two
of the twelve below have a weaker claim than that, and the chapter says which.

---

# The twelve tier 0 built

## `quote`

```lisp
(quote foo)   ≡   'foo      ; => FOO, the symbol, not its value
'(1 2 3)                     ; => the list, not a call to the function 1
```

**What it does.** Returns its argument without evaluating it.

**Why it must be special.** It is the definition of not-evaluating. There is no
function that could do this, because by the time a function is called its
arguments are gone.

**Why you want it.** In a language where code and data are the same shape, you
need a way to say *"this time I mean the data"*. Without `quote` you could not
write a literal list: `(1 2 3)` would try to call the function `1`.

**What it unlocks.** Macros. A macro receives unevaluated forms and returns a
new form to be evaluated instead, which is only coherent because code is data,
and `quote` is how you hold a piece of code still long enough to look at it.

## `if`

```lisp
(if (> x 0) (report "positive") (crash-the-system))
```

**What it does.** Evaluates the test, then exactly one branch.

**Why it must be special.** This is the clearest case in the language. A
function's arguments are evaluated before it runs, so a functional `if` would
run *both* branches and then choose, by which time the damage is done.

**Why you want it.** All conditional execution. There is nothing else.

**What it unlocks.** `when`, `unless`, `cond`, `case`, `and`, `or` are every one
of them macros over `if`. The entire conditional vocabulary of Common Lisp is
this operator wearing different hats.

## `progn`

```lisp
(if ready
    (progn (log "starting") (start))
    (wait))
```

**What it does.** Evaluates forms in order and yields the last one's value.

**Why it must be special, and this one is weaker.** Look closely: a function
would evaluate its arguments left to right and could return the last, so for
*values* a functional `progn` would behave identically.

It is special for a reason that has nothing to do with evaluation control, which
is top-level processing. When a file is compiled, the forms inside a top-level
`progn` are themselves treated as top-level. That matters because of `eval-when`
below, since a macro defined inside a `progn` must take effect before later
forms in the same `progn` are compiled. A function call could not have that
property, because a function call is a single form.

So `progn` earns its place by compile-time behaviour rather than by runtime
semantics, which is worth knowing because it is the exception to the rule this
chapter opened with.

**Why you want it.** `if` takes one form per branch. Sometimes you have three.

**What it unlocks.** The *implicit* `progn`: the bodies of `lambda`, `defun`,
`let`, `when` and almost everything else are sequences, and they are sequences
because this exists to define what a sequence means.

## `setq`

```lisp
(setq x 1)
(setq a 1 b 2)
```

**What it does.** Assigns a value to a variable.

**Why it must be special.** The variable name must *not* be evaluated. If `setq`
were a function, `(setq x 1)` would evaluate `x` first, getting its current
value or an error if it has none, and then have no idea which place to assign
to.

**Why you want it.** Mutation. Loops that accumulate, state that changes.

**What it unlocks.** `setf`, which is the general assignment operator and a
macro: `(setf (car x) 1)` expands into code that eventually uses `setq` or a
primitive. `incf`, `push` and `pop` are all macros over `setf`.

## `function`

```lisp
#'car             ≡   (function car)
(funcall #'+ 1 2)
#'(lambda (x) x)
```

**What it does.** Yields the function a name denotes.

**Why it must be special.** The name must not be evaluated as a variable. In a
Lisp-2 `car` the variable and `car` the function are different things
(chapter 9), and `#'car` is how you say you mean the second.

**Why you want it.** It is the *only* way to get a function as a value in a
Lisp-2. Without it, higher-order programming is impossible: you could define
`mapcar` but never pass it anything.

**What it unlocks.** Everything higher-order, from `mapcar` and `sort` with a
predicate to callbacks and a function stored in a data structure. Chapter 9
called this the tax a Lisp-2 pays; `function` is the receipt.

## `let`

```lisp
(let ((x 1) (y 2))
  (+ x y))
```

**What it does.** Binds variables to values for the duration of a body. Every
init form is evaluated in the *enclosing* scope, before any binding is made.

**Why it must be special.** `((x 1) (y 2))` is not an expression. Evaluated as
one it would try to call the function `(x 1)`. The binding list is syntax that
`let` reads rather than a value it receives.

**Why you want it.** Local variables, which is to say naming things. Also scope:
a name that exists only where it is wanted, and is gone afterwards.

**What it unlocks.** Nearly every macro that needs a temporary expands into a
`let`. When `defmacro` arrives, `let` is where most expansions put the values
they must not evaluate twice.

## `let*`

```lisp
(let* ((x 1)
       (y (+ x 1)))     ; y can see x
  (* x y))
```

**What it does.** Like `let`, but each init can see the bindings before it.

**Why it must be special, and this is the second weak case.** `let*` is
*exactly* nested `let`s:

```lisp
(let* ((a 1) (b 2)) body)   ≡   (let ((a 1)) (let ((b 2)) body))
```

So it could be a macro, and in a sense it is one with the expansion written in
the implementation instead. It is in the standard's twenty-five for pragmatic
reasons: it is common enough to be worth implementing directly, and the
expansion creates a scope per binding either way.

Two of twenty-five being arguable is a useful thing to know about the list. It
is the core of the language, but it was assembled by people making judgement
calls rather than derived from first principles.

**Why you want it.** Sequences of computations where each step uses the last,
which is most of them. Writing that with nested `let` is possible and horrible.

**What it unlocks.** Readable code, mostly, which is not nothing.

## `flet`

```lisp
(flet ((double (x) (* x 2)))
  (double 21))
```

**What it does.** Binds functions locally, in the function namespace. The bound
functions *cannot* see each other or themselves.

**Why it must be special.** Same as `let`: the binding list is syntax. And the
bodies must not be evaluated at binding time, because they become closures.

**Why you want it.** A helper used twice in one function, without putting a name
in the global namespace where it will collide with someone else's.

The not-seeing-itself is a feature rather than a limitation. It is what lets you
wrap an existing function:

```lisp
(flet ((print (x) (print (annotate x))))   ; inner `print` is the outer one
  ...)
```

With `labels` that would be infinite recursion.

**What it unlocks.** Local abstraction. Macros use it to avoid evaluating a form
twice without naming the result.

## `labels`

```lisp
(labels ((even-p (n) (if (= n 0) t   (odd-p (- n 1))))
         (odd-p  (n) (if (= n 0) nil (even-p (- n 1)))))
  (even-p 10))
```

**What it does.** Like `flet`, except the functions *can* see themselves and
each other.

**Why it must be special.** As `flet`. The implementation difference is one
line, which scope the closures capture, and it is the whole of mutual recursion.
The scope has to exist before the closures that go in it, so it is built empty
and filled.

**Why you want it.** Local recursion. A helper that calls itself, without a
global name.

**Why both exist.** They are genuinely different tools rather than a default and
a variant: `labels` for recursion, `flet` for shadowing. Neither can do the
other's job.

**What it unlocks.** In a language where recursion *is* iteration, this is how
you write a loop locally. `do` and `loop` are macros that expand into `labels`
or `tagbody`.

## `the`

```lisp
(the integer (+ x y))
```

**What it does.** Declares that a form will produce a value of some type. The
value is returned unchanged.

**Why it must be special.** The type is not evaluated, since `integer` is a type
name rather than a variable.

**Why you want it.** It is a promise to the compiler: this is an integer, you
need not check. In Quokka today it does nothing at all.

**What it unlocks.** A compiler that knows a value is a fixnum can emit an
unchecked add instead of a dispatch. `the` is how the programmer hands the
compiler information the compiler could not derive, and it is the hook for every
type-driven optimisation later.

*Later: the compiler of phase 1 arrived and `the` still does nothing with the
type. It checks the form's shape, compiles the body and discards the
declaration; type-driven code generation is still unbuilt. It did gain one job
in phase 2, as a `setf` place, so `(setf (the integer x) 1)` works
([Places](../part-6-self-hosting/43-places.md)).*

## `locally`

```lisp
(locally (declare (optimize speed))
  (heavy-work))
```

**What it does.** Evaluates a body, with declarations in force for it.

**Why it must be special.** Declarations are not expressions.

**Why you want it.** Declarations normally attach to a binding form. `locally`
is for when you want them somewhere there is no binding.

**What it unlocks.** Nothing yet, because Quokka has no declarations, so it is
currently `progn` with a different name. It is implemented because tier 0 lists
it and because an empty slot now is cheaper than a retrofit later.

*Later: it has a job now. Phase 2 added `declare`, `proclaim` and `declaim`,
and the compiler reads `special` declarations in `locally`'s body, so
`(locally (declare (special x)) ...)` makes `x` dynamic for that body. Other
declarations are still discarded.*

## `eval-when`

```lisp
(eval-when (:compile-toplevel :load-toplevel :execute)
  (defmacro my-macro (x) ...))
```

**What it does.** Says *when* its body should run: while compiling, while
loading a compiled file, or when simply executed.

**Why it must be special.** The situations are not evaluated, and the body might
not be evaluated at all.

**Why you want it.** This looks like the most obscure operator in the list and
is one of the most important.

Consider a file that defines a macro and then uses it. Compiling that file, the
compiler reaches the use before anything has run, so unless the `defmacro` is
evaluated *during compilation*, the macro does not exist when it is needed.
`eval-when` is how a form says "run me at compile time too".

**What it unlocks.** The bootstrap. Phase 2's gate is the compiler compiling
itself, and the compiler is full of macros it uses itself. Without `eval-when`
there is no self-hosting.

In Quokka today the only situation is `:execute`, because an interpreter is
always executing. The other two start meaning something in phase 1.

*Later: they did not. The compiler decides `eval-when` at compile time and emits
either the body or a `NIL`, and it still honours only `:execute`. The bootstrap
got its compile-time effects another way, from the cold build's two passes,
which compile every form before installing any of them
([A world made of bytecode](../part-5-the-image/32-a-world-made-of-bytecode.md)).
The prediction was right about what was needed and wrong about which mechanism
would provide it.*

---

# The thirteen that came later

Grouped by what they need, which is also the order they arrived in. All thirteen
exist now, and each group says where.

## Non-local exits: `block`, `return-from`, `catch`, `throw`, `unwind-protect`

**What they do.** Leave a computation from the middle of it. `block` names a
region and `return-from` jumps out of it; `catch` and `throw` do the same
dynamically, finding the target at run time rather than lexically.
`unwind-protect` runs cleanup forms *as* the stack unwinds past it.

**Why they must be special.** Every one of them manipulates control flow itself,
which no function can.

**Why you want them.** Early exit from a loop. Giving up on a search. Closing a
file no matter how the body ends.

**What they unlock.** This is the machinery phase 3's **condition system** is
built from: `handler-case` is a `block` plus a dynamically found handler, and a
restart is `catch` with a protocol.

All five are built, and [Leaving early](14-leaving-early.md) is how, including
why they are one mechanism with three lookups rather than five features.

## Iteration: `tagbody`, `go`

**What they do.** `tagbody` is a body containing labels; `go` jumps to one.

**Why they must be special.** A jump is not a value.

**Why you want them.** Directly, almost never. It is assembly language with
parentheses.

**What they unlock.** Everything that iterates. `do`, `dotimes`, `dolist` and
`loop` are macros that expand into `tagbody` and `go`. It is the primitive
iteration construct, and the reason Lisp can have twenty looping constructs
without twenty implementations. Built; see
[Leaving early](14-leaving-early.md).

## Multiple values: `multiple-value-call`, `multiple-value-prog1`

**What they do.** Work with forms that return *several* values at once, which
Common Lisp allows and most languages do not.

**Why they must be special.** The single value register cannot express it.
Returning two values is not the same as returning a list of two.

**Why you want them.** `(floor 7 2)` returns 3 *and* 1, quotient and remainder,
without allocating. `gethash` returns the value and whether it was present. It
is how Common Lisp avoids the out-parameter and the tuple.

**What they unlock.** A great deal of the standard library's interface has this
shape, so the standard library cannot be finished without it. Built;
[More than one answer](23-more-than-one-answer.md) is the mechanism, including
why a second register was the wrong way to do it.

## Rarely needed: `progv`, `load-time-value`, `symbol-macrolet`, `macrolet`

`docs/design.md` leaves the first three out of tier 0 entirely: "The interpreter
can add them later at little cost." `macrolet`, which is local macro
definitions, arrives with macros.

As it turned out, the interpreter never added them. They arrived in tier 1,
compiled only, two of them as macros, which the standard allows, and
`symbol-macrolet` needed the compiler to hand macros their lexical environment.
[The last three, and what a macro can see](../part-6-self-hosting/44-the-last-three.md)
tells that story.

---

## The shape of the list

Reading the twenty-five in one go, a pattern is visible that is hard to see from
any one of them. The list is a basis. Every one is there because it cannot be
built from the others, with the two exceptions this chapter named, and naming
them is the point. Everything a Lisp programmer actually writes is one or two
macro-expansions away from this set.

That is why the Rust core does not grow in proportion to the language. `when`,
`unless`, `cond`, `case`, `dolist`, `dotimes`, `do`, `loop`, `push`, `pop`,
`incf`, `with-open-file`: none of them is in the VM, and none of them ever will
be. They are Lisp, written on top of twenty-five things that are not.
