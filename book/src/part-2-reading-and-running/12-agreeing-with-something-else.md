# Agreeing with something else

> *Where this fits.* The last chapter of phase 0, and the only one not about
> making Quokka work. It is about finding out whether Quokka is *right*, which
> is a different question, and one the previous eleven chapters cannot answer
> on their own. The answer is a test harness that needs nobody to know the
> expected result, and by the end of the chapter it has caught a rule being
> broken an hour after it was written down.

## What a hundred passing tests prove

By the end of the last chapter there were ninety-nine tests and all of them
passed. It is worth being precise about what that establishes. Every assertion
in them came out of our heads, so they prove that Quokka does what *we believed*
Common Lisp does.

That is a weaker claim than it looks, and there is a worked example two chapters
back. We had read the evaluation rules, written a chapter explaining them, and
implemented a **Lisp-1**, because the summary we were working from said
"evaluate `f`" and that phrasing loses the one distinction that mattered. A test
caught it only because we happened to write `(cons 1 2)` and happened to know
what it should produce.

Here are three things the test suite asserts where we are genuinely unsure:

```lisp
(+)           ; => 0     guessed from "identity element"
(progn)       ; => NIL   guessed
(eq 1 1)      ; => T     ...and this one is a trap
```

The last is worth dwelling on. Common Lisp says `eq` on *numbers* is
implementation-dependent, and `eql` is the one that compares numbers by value.
SBCL answers `T` because small integers are immediates there, exactly as in
Quokka, so the test passes and it is testing a coincidence rather than a rule. A
test you wrote tests your understanding; something else has to test reality.

## An oracle

An **oracle** is a reference implementation you trust more than your own. You
run the same input through both and compare, and the useful property is that you
do not have to know the right answer. You only have to notice a disagreement,
and then go and find out which of you is wrong. That removes your own
understanding from the loop, which is exactly the thing that failed.

Quokka's oracle is **SBCL**, Steel Bank Common Lisp, a mature implementation
that has been getting this right since 1999. `docs/design.md` chose it before
any code existed:

> Code written for it should also run in SBCL, which gives us a reference
> implementation to test against.

## Three legs, two comparisons

Here is the part that makes this worth building rather than just also running
the tests in SBCL. The same file runs three ways, and the two comparisons catch
entirely different kinds of bug:

```text
                    ┌──────────────────┐
     test file ────▶ │ Quokka, native   │──┐
          │          └──────────────────┘  │  agree?  ← portability
          │          ┌──────────────────┐  │
          ├────────▶ │ Quokka, wasm32   │──┘
          │          └──────────────────┘
          │          ┌──────────────────┐
          └────────▶ │ SBCL             │─── agree?  ← correctness
                     └──────────────────┘
```

Quokka against SBCL catches *we implemented Lisp wrong*: semantics, whether
`(car nil)` is `NIL` or an error, whether the operator position reads a function
cell. Quokka native against Quokka wasm32 catches *the VM behaves differently
depending on where it runs*. That is a separate hazard and not a theoretical
one, since chapters 1 and 5 each describe a constant typed as `usize` meaning
different things on a 64-bit machine and a 32-bit one. Both were compile errors;
the next one might not be.

Rule 2 demands that both targets build and pass. This is what makes "and pass"
mean something at the level of the language rather than the level of a unit
test.

*Later: there are four legs now. Phase 1 added a compiled world, so every file
also runs through the bytecode compiler and the VM, which catches the compiler
and the interpreter disagreeing;
[Two evaluators, one answer](../part-5-the-image/33-two-evaluators-one-answer.md)
is that story, including the bugs it found on its first run.*

## What a test file is

Ordinary Common Lisp. It has to be, since SBCL must read it:

```lisp
(+ 1 2)
(* 6 7)
(car nil)
(if nil (undefined-function) 2)
```

Notice what is *not* in the file: the expected answers. The assertion is
agreement rather than a recorded value, so writing a test requires no knowledge
of the right answer at all. You write the interesting form and let the
implementations argue.

The output format is one line per top-level form:

```text
=> 42          the value, printed as the reader would want it back
!! error       the form signalled
```

Errors are flattened to a single token deliberately. Quokka says "the function
NOPE is undefined" and SBCL says something else entirely, with a condition class
and a restart list, so the text will never agree. Comparing condition *classes*
would be stronger and waits for the condition system of phase 3; this still
catches the case that matters, which is one implementation signalling where the
other does not.

This is also why a printer was written back in chapter 8, alongside the reader
rather than later. Without one there is nothing to compare.

## What may not go in a test file

Three kinds of form disagree for reasons that are nobody's fault, and knowing
them is most of the skill in writing these.

1. Anything needing a bignum. SBCL has them and Quokka does not yet, so
   `(* 2432902008176640000 21)` is an answer there and an error here. That is a
   real difference, and not one a test should assert about.
2. Anything that prints a function. SBCL renders one as
   `#<FUNCTION (LAMBDA (X) :IN ADDER) {1200E36C9B}>`, with an address in it.
   Nothing will ever match that.
3. Anything the standard leaves unspecified, such as `(eq 1 1)` again. A test
   asserting it would pass for the wrong reason.

## The first run

The baselines had to be generated from Quokka, because there is no SBCL on the
machine this was written on. So the first time the tests ran in CI was the first
time Quokka's semantics met a real Common Lisp.

SBCL disputed exactly one line of four files:

```text
quokka  => #<function>
sbcl    => #<FUNCTION (LAMBDA (X) :IN ADDER) {1200E36C9B}>
```

Read the three rules above again. The second one is *do not print a function*,
and it had been written in the README beside those test files an hour earlier.
And then `functions.lisp` contained:

```lisp
(setq add5 (adder 5))
```

whose value is a closure. The rule was written down and broken three files later
by the person who wrote it, and the only thing that noticed was the oracle, on
its first run.

## The same lesson, a third time

Chapter 5 has a sentence written about an unrelated mistake:

> A bound that depends on how callers choose to use an API is not a bound. It is
> a hope with a justification attached.

A rule in a README is the same shape. So the fix was not only to wrap the form,
it was to make the rule fail the build:

```bash
if grep -n '#<' "$baseline" >/dev/null 2>&1; then
  echo "unprintable: $baseline contains #<...>" >&2
  status=1
fi
```

`#<...>` is Common Lisp's own mark for "this cannot be read back", so a baseline
containing one can never match any oracle. The check is three lines and there is
a test confirming it fires.

That is three times in this book that a rule we knew, had written down, and had
explained at length was broken shortly afterwards: the `rebrand` helper in
chapter 8, the mutation bound in chapter 5, and this. The pattern is consistent
enough to be worth stating as a conclusion rather than an anecdote.

> At the moment you break a rule, the situation does not look like the rule. It
> looks like a signature problem, or a convenience, or a test file. Knowing the
> rule provides no protection at the moment protection is needed. Only something
> that fails the build does.

## What agreed

Everything else. Over a hundred forms across arithmetic, lists, `quote`, `if`,
`progn`, `setq`, scope, closures, recursion and errors, matching SBCL exactly,
on native and under WebAssembly.

That is the first evidence from outside that Quokka implements Common Lisp
rather than something that resembles it. Everything before this chapter was our
own tests agreeing with our own understanding.

## Why this had to exist before phase 1

Phase 1 replaces the tree-walking evaluator with **bytecode**, and that is two
separate things which are easy to run together. One is a bytecode VM, in Rust,
beside the heap and the collector; it executes instructions, and it is the part
that plays the role of hardware. The other is a compiler, in Lisp, which
translates source into those instructions. Every form in the language will then
execute by a different mechanism.

The only reason to believe such a rewrite preserved behaviour is a test suite
that compares against something *outside* the rewrite. Unit tests written
against the interpreter would be rewritten alongside it, and would agree with
whatever it did.

These files will not change. SBCL will not change. That is what makes the next
phase safe, and it is why the last thing built in phase 0 is the thing that
makes phase 0 trustworthy.
