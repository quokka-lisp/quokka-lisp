# What a form means

> *Where this fits.* Chapter 8 turned text into lists, and nothing yet knows
> that `(* 6 7)` means anything. This chapter is the **evaluator**, the part
> that works out what a list means and does it. The whole of Lisp evaluation
> turns out to be four rules, and the machine that runs them can be stopped
> between any two steps and resumed tomorrow, which is what chapter 2 promised
> and could not yet show.

## The rules of evaluation

Here is the whole of it:

| What you are evaluating | What happens |
| --- | --- |
| a number, a string, a character | **itself**. `42` evaluates to `42` |
| a symbol | look up its **value** |
| a list `(f a b)` | evaluate `a` and `b`, then **apply** `f` to the results |

Plus one exception:

| | |
| --- | --- |
| a list whose head is a **special operator** | that operator's own rule |

Four lines, and every Lisp program is covered.

Compare that to the evaluation rules of any other language, with their
precedence tables, statements against expressions, declaration scope and
implicit conversions, and you can see why Lisp people get evangelical. The
smallness is not an accident of age. It is what you get when the syntax and the
data structure are the same thing, so there is nothing to translate between.

## Why special operators have to exist

The exception is where everything interesting lives, and it is worth
understanding why there has to be one.

Suppose `if` were an ordinary function. Then this:

```lisp
(if (> x 0) (report "positive") (crash-the-system))
```

would evaluate *both* branches before `if` ever ran, because that is what "apply
`f` to the results" means: the results have to exist first. By the time `if` got
to choose, the damage would be done.

So `if` cannot be a function. It needs to control whether its arguments are
evaluated at all, and that is precisely what a special operator is: a form that
decides for itself whether, and when, to evaluate its arguments. Every one of
them exists for that reason.

- `quote` evaluates *none* of them, which is its entire job.
- `if` evaluates the test, then exactly one branch.
- `lambda` evaluates the body later, possibly many times, possibly never.
- `setq` evaluates the value but not the variable name.

Common Lisp has exactly 25, and that number is the irreducible core of the
language. Everything else, `when`, `unless`, `cond`, `dolist`, is built out of
them by macros.
[The special operators, one by one](13-the-special-operators.md) goes through
them individually: what each does, why it cannot be a function, and what it
makes possible. It is a reference chapter and can wait.

## Where "special" is recorded

The obvious approach is a lookup table, a list of special operator names that
the evaluator checks against. Quokka instead puts a mark on the symbol itself:

```rust
pub const SPECIAL: usize = 4;   // a symbol's fifth slot
```

That is where the fact lives. `if` is not special because it appears on a list
somewhere, it is special *because the symbol `IF` is*, and recording it on the
symbol says what is true. It is also faster, since dispatch becomes a slot read
and an integer comparison rather than a string comparison, on every single form
the evaluator looks at.

## The machine

Chapter 2 established that the evaluator cannot be a recursive function. Its
state has to live somewhere it can be stopped and resumed, so it lives in four
registers and a stack, all in the heap:

```rust
control  // the form being worked on
env      // where names are looked up
value    // the result the last step produced
stack    // frames: what to do once that result arrives
```

The machine alternates between two half-steps. In an eval step it looks at
`control`, and either produces a value directly or pushes a frame and starts on
a sub-form. In a return step a value is sitting in `value`, and it hands that to
the top frame, which decides what happens next.

A frame answers one question: "I was in the middle of something; when you give
me a value, how do I continue?" That has a name, a **continuation**, and here it
is not an abstraction but a heap object with slots you can print. Which is
exactly what will make the debugger possible in phase 3, because showing someone
their call stack means showing them these.

## A computation, step by step

`(+ 1 (* 2 3))`. Every row is one `step()`:

| # | mode | `control` | `value` | `stack`, top first |
| --- | --- | --- | --- | --- |
| 0 | Eval | `(+ 1 (* 2 3))` | — | — |
| 1 | Eval | `1` | — | `Args{done:(+), left:(1 (* 2 3))}` |
| 2 | Return | — | `1` | `Args{done:(+), left:(1 (* 2 3))}` |
| 3 | Eval | `(* 2 3)` | `1` | `Args{done:(1 +), left:((* 2 3))}` |
| 4 | Eval | `2` | — | `Args{done:(*), left:(2 3)}`, `Args{…}` |
| 5 | Return | — | `2` | … |
| 6 | Eval | `3` | `2` | `Args{done:(2 *), left:(3)}`, `Args{…}` |
| 7 | Return | — | `3` | … |
| 8 | Return | — | **`6`** | `Args{done:(1 +), left:()}` — `*` applied |
| 9 | Return | — | **`7`** | — — `+` applied, stack empty |
| 10 | Halt | | `7` | |

What is not there is any Rust recursion. Each row is one small transition,
reading and writing four registers, so you can stop between any two rows and the
whole state is four heap values: save it to a file, resume it tomorrow, show it
to a user.

Notice also the `done` lists running backwards. Pushing onto the front of a list
is the cheap direction, so values accumulate reversed and the list is reversed
once at the end, rather than being walked to its tail on every argument.

## Lisp-1 and Lisp-2, learned the hard way

Here is a bug that a test found and no amount of reading would have.

```
quokka> (cons 1 2)
; the variable CONS is unbound
```

We had implemented the rules exactly as written above: evaluate `f`, `a`, `b`,
then apply the first to the rest. Evaluating `f` means evaluating the symbol
`CONS`, which means looking up its value. And `CONS` has no value. It has a
*function*.

### Two cells, not one

In Common Lisp a symbol carries a value and a function separately:

```text
   symbol CONS
   ├── name      "CONS"
   ├── value     <unbound>      ← what `cons` means as a variable
   ├── function  #<primitive>   ← what `(cons ...)` means as a call
   └── plist     nil
```

So the rule for a list is not quite what the table said. The head of a list is
looked up in the **function** cell, and everything else is looked up in the
value cell. A language that works this way is called a **Lisp-2**, and one with
a single shared cell is a **Lisp-1**, of which Scheme is the famous example.

The consequence you can see immediately:

```lisp
(defun thing (x) (* x 2))   ; the function THING
(setq thing 5)              ; the variable THING, unrelated
(thing thing)               ; => 10
```

Both are legal and they do not collide. That is a real convenience, because you
can call a variable `list` or `count` without destroying the function of that
name, which in a Lisp-1 you cannot.

### What it costs

Two things, and both are visible in any Common Lisp program.

The first is that you need `funcall`. If a closure is sitting in a variable,
`(f 1)` will not call it, because that looks in `f`'s function cell, which is
empty. You have to say `(funcall f 1)`, which means "apply whatever this
*holds*".

```lisp
(setq add5 (adder 5))
(add5 1)            ; error: the function ADD5 is undefined
(funcall add5 1)    ; => 6
```

The second is that you need `#'`. To pass a function as a value you have to say
which cell you mean, so `#'car` is "the function `car`", as opposed to `car` the
variable.

Scheme does without both, at the cost of functions and variables competing for
the same names. Neither choice is wrong. They trade different inconveniences,
and the argument between the two camps has been running since the 1980s with no
sign of resolution.

### The failure mode is quiet

What makes this worth a section is not the fact but the shape of the mistake.
Implementing a Lisp-1 by accident does not produce a crash or a type error. It
produces "the variable CONS is unbound", a message about a variable when the
problem is a missing distinction. Every function call in the system goes looking
in the wrong place, and the error names a symptom three steps from the cause.

Nothing but a test that called a function would have found it. The specification
is in the Hyperspec, the behaviour is well known, and we wrote a Lisp-1 anyway,
because the rules as summarised say "evaluate `f`", and summaries lose exactly
the distinction that matters.

## Errors are values, not crashes

Every failure above, whether an unbound variable, an undefined function, a wrong
argument count, bad syntax or integer overflow, is a value the machine returns
rather than a panic.

```
quokka> (nope 1)
; the function NOPE is undefined
quokka> (* 2432902008176640000 21)
; integer overflow, and bignums do not exist yet
```

That matters more than it sounds. A REPL that dies on a typo is not a REPL, and
"errors are conversations" is one of the project's stated principles. In phase 3
these become Lisp **conditions**, which do not unwind the stack but stop and ask,
at which point the frames the machine is holding become the thing the debugger
shows you, and the whole design pays off a fourth time.

## Where this leaves us

`(+ 1 (* 2 3))` evaluates to 7, in a machine that can be stopped between any two
steps.

What is missing is the ability to define anything. There are no variables beyond
globals, no functions of your own, and no way to write `fact`. Those need
`lambda`, and `lambda` needs somewhere for its parameters to live, which is the
next chapter.
