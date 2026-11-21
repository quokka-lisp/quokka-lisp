# A compiler in the language it compiles

> **Where this fits.** Part III built a machine: thirty-six instructions, a
> stack, a verifier, and no way to produce a program for it except by hand.
> This part is the program that produces them, and it is a different kind of
> thing from everything before it — not a piece of the runtime but a **Lisp
> program**, which happens to take Lisp as input.
>
> It opens its own part for a reason worth stating. The machine is close to
> finished and will change rarely; the compiler is where the judgement lives,
> and most of what it decides today is provisional. The last section of this
> chapter is a list of those decisions and how long each is expected to last.
>
> **Status: it runs.** `(+ 1 2)` compiles and evaluates to 3, with nothing in
> Rust deciding what the instructions should be.

## Trying it

```
cargo run -p quokka-repl -- --compile
```

The ordinary REPL interprets and does not load the compiler at all. This one
loads it, and then evaluates every form by **compiling it and running the
result on the VM**:

```lisp
quokka> (+ 1 2)
3
quokka> ((lambda (n) ((lambda (x) (+ x n)) 5)) 37)
42
quokka> (labels ((down (n) (if (= n 0) 'done (down (- n 1))))) (down 20000))
DONE
quokka> (when t 'macros-work)
MACROS-WORK
```

Nothing above was interpreted. The last one is worth a second look: `when` is
a macro in the prelude, expanded by the compiler, and the instructions that
run are two jumps.

### Looking at the output

`--show-code` prints the instructions before running them, which is the only
way to check that a compiler is doing what you think:

```
quokka> ((lambda (x) (+ x 1)) 41)
COMPILED-FORM NIL  ; stack 2
     0  CLOSURE 0  ; #<compiled LAMBDA>
     3  FIX 41
     6  TCALL 1
     8  RET

    LAMBDA (X)  ; stack 3
         0  GFREF 0  ; +
         3  LREF 0 0
         6  FIX 1
         9  TCALL 2
        11  RET
42
```

Four things in that listing are worth reading rather than skimming.

**The constants are resolved.** `GFREF 0` on its own is unreadable; `GFREF 0
; +` is a sentence. A disassembler that prints only numbers is a disassembler
nobody uses twice.

**The nested function is indented beneath the `CLOSURE` that makes it**, which
is where it lives: in the outer function's constants vector. The structure of
the source survives into the structure of the listing.

**`LREF 0 0`** is chapter 26's lexical address, in the wild: `x` is the
innermost scope, slot zero, and the name is gone.

**Both calls are `TCALL`.** The inner `(+ x 1)` is the lambda's whole body, so
it is in tail position; the outer call is the whole form. Chapter 25 said the
compiler decides tail position from the shape of the source and writes it
down — this is it written down.

## The smallest real thing

```lisp
quokka> (compile-form '(+ 1 2))
#<compiled COMPILED-FORM>
```

That function's instructions are:

```
  GFREF 0        ; the function cell of +
  FIX 1
  FIX 2
  CALL 2
  RET
```

A Lisp function read the form, chose those five instructions, and handed the
list to the assembler. The chain is worth following once, because it is the
whole of what this part is about:

| | |
| --- | --- |
| a reader, in Rust | text to cons cells (chapter 8) |
| **a compiler, in Lisp** | cons cells to a list of instructions |
| an assembler, in Rust | instructions to bytes, verified (chapter 25) |
| a VM, in Rust | bytes to an answer |

Only the second one has an opinion.

## Why it is written in Lisp, and what that costs

It could be written in Rust. It would be faster to run and would work from the
first day, since the interpreter would not have to be good enough to run it.

It is in Lisp because of the sentence this whole project is arranged around:

> **Rust below the primitive table, Lisp above it.**

A compiler is the most opinionated program in a language implementation. It
decides what `let` costs, whether a call is a jump, which functions may be
inlined and when a variable can live somewhere cheaper. Those are exactly the
decisions a person working *inside* the system would want to change — and a
compiler in Rust is one they cannot reach without rebuilding the compiler that
built their image.

The cost is paid at the bootstrap: the interpreter has to be good enough to run
it, which is what all of tier 0 was for. This is the bill arriving for
[The thing that builds itself](../the-bootstrap.md).

## Where it lives, and why not in the prelude

`lisp/compiler/`, beside `lisp/prelude.lisp` rather than inside it. The reason
is **lifecycle**, not size:

| | when it runs |
| --- | --- |
| `prelude.lisp` | **every boot.** It defines `defun`; nothing works without it |
| `compiler/` | **once**, to build an image. After that it is compiled code inside that image |

Every REPL start would otherwise interpret six hundred lines of compiler it is
not going to use. They also fail differently: a broken prelude form means
nothing works at all, and a broken compiler form means an image cannot be
built from source — which is bad in a quite different way.

## The boundary, which was decided before there was a compiler

Chapter 24 wrote down a rule without knowing what it would be for:

> **The encoding is a boundary.** The compiler emits instructions as lists,
> and an assembler encodes them.

So the Lisp side produces this:

```lisp
((GFREF 0) (FIX 1) (FIX 2) (CALL 2) (RET))
```

and one primitive, `%assemble`, turns it into a function: resolve the labels,
encode the operands, verify the result, work out how deep its stack gets.

**The alternative is to assemble in Lisp, and it is a mistake this project had
already made once elsewhere.** Opcode numbers and operand widths would then
live in `code.rs` *and* in a Lisp file, with nothing checking that they agree.
Adding an instruction would mean editing both; a disagreement would produce a
program that assembles cleanly and executes nonsense. The stack-effect table
would be in a third place.

So the compiler never learns that `JMP-NIL` is byte 20. It names instructions
and the assembler knows what they are.

There is a second benefit that was not the reason but is the better argument.
`%assemble` builds the function with a deliberately huge stack reservation,
runs the verifier to find out what it actually needs, and rebuilds with that.
**The compiler never computes how deep its own output gets** — so it cannot
disagree with the verifier about it, which is a whole category of bug that
simply does not arise.

## What a compiler function looks like

One form in, a list of instructions out, and the instructions leave exactly
one value on the stack. That last clause is the contract everything else rests
on:

```lisp
(defun %compile (form)
  (cond ((null form) (list (list 'NIL)))
        ((eq form t) (list (list 'T)))
        ((symbolp form) (list (list 'GREF (%constant-index form))))
        ((consp form) (%compile-cons form))
        (t (%compile-value form))))
```

Five cases, and four of them are one instruction. A compiler is mostly this:
a dispatch on the shape of a form, and a lot of appending.

### Constants, and a distinction worth getting right

A small integer goes in the instruction itself:

```lisp
(defun %small-integer-p (n)
  (and (> n -32769) (< n 32768)))
```

because `FIX` has a signed 16-bit operand, and small integers are most of what
a program mentions. Anything else — a symbol, a string, a list, a large
number — goes in the function's constants vector, and the compiler remembers
what it has put there so that two mentions share a slot.

Shared by `eq`, not by `equal`, and that is a decision rather than laziness.
Two occurrences of the symbol `foo` are the same object, because symbols are
interned (chapter 8), so sharing them is free and invisible. Two occurrences
of `"text"` are **not** the same object, and a program can tell:

```lisp
(eq "text" "text")   ; => NIL, and must stay NIL
```

Chapter 26's literals are read-only, which would make sharing them *sound* —
but soundness is not the question here. The question is whether a program can
observe the difference, and for strings it can.

### `if` is a pair of jumps

```lisp
(defun %compile-if (form)
  (let ((else (gensym))
        (done (gensym)))
    (append (%compile (second form))
            (list (list 'JMP-NIL else))
            (%compile (third form))
            (list (list 'JMP done))
            (list (list ':label else))
            (%compile (nth 3 form))
            (list (list ':label done)))))
```

`gensym` for the labels, because a nested `if` would otherwise collide with
itself — the same reason macros need it (chapter 15), arriving in a program
that is not a macro.

And a thing that went right the first time because of work done earlier: both
arms **must** leave exactly one value, or the verifier refuses the whole
function. The two paths meet at `done` and have to agree about the stack. That
check was written two days before there was a compiler to catch out, and it is
now the thing standing between a code-generation bug and a wrong answer
somewhere far away.

### A call reads the function cell

```lisp
(defun %compile-call (form)
  (append (list (list 'GFREF (%constant-index (car form))))
          (%compile-arguments (cdr form))
          (list (list 'CALL (length (cdr form))))))
```

Callee, then arguments, then `CALL`. `GFREF` reads the symbol's function cell
at run time, so redefining the function takes effect — which is ordinary Lisp
and has to keep working.

For `+` it is waste. Chapter 25 explained why `+` could be a single `PRIM`
instruction instead: CLHS 11.1.2.1.2 makes redefining a `COMMON-LISP` symbol
undefined, which is the standard giving an implementation permission to assume
it will not happen.

The compiler does not do that yet, on purpose. The uniform version is the one
to get working first, and an optimisation that changes what a program *means*
when somebody breaks a rule deserves its own change and its own paragraph.

## The decisions that are provisional

Which is the real reason this is a part of its own. The machine is nearly
settled — thirty-seven instructions, and changing one is a format change. The
compiler is a pile of judgements, and most of today's will not survive.

| decision | why it is that way now | what changes it |
| --- | --- | --- |
| Calls go through `GFREF` | uniform, correct, obviously right | inlining `COMMON-LISP` functions to `PRIM`, which is a measurable win on every arithmetic expression |
| ~~No tail calls~~ | **done.** The compiler threads a tail flag and emits `TCALL`, so a loop written as recursion runs in constant space | — |
| ~~Instructions accumulated with `append`~~ | **done.** They accumulate reversed in a global and are turned round once; `append` copies its first argument, so joining n of them copied n, then n−1, then n−2 | — |
| Constants found by scanning a list | fifty constants, scanned linearly | a hash table, when a function has enough constants for it to matter |
| One global for the constants being built | threading it through every function would be most of the code | special variables, when there are any |
| `if` emits a `JMP` even when the then-branch cannot fall through | simple and always correct | noticing that a branch ending in `RET` needs no jump after it |
| Lambda lists are rewritten by a primitive | the interpreter already has that rewrite, and a second one in Lisp is a second `&key` to keep in step | nothing — the rewrite has no judgement in it, which is why it is on the Rust side of #28's boundary |
| `labels` makes a scope of `nil`s and fills it in | the closures must capture a scope that exists | nothing — this is what `labels` *is*, and the instruction it needed (`FSET`) is the thirty-seventh opcode |

One is already struck out, which is the table doing its job: tail calls were
on it when this chapter was written and were done two days later, and the row
is left in place rather than deleted so that the list reads as a history
rather than a wish.

None of the rest is a bug and each is a thing to come back to. The point of
writing them down here is that a reader of this chapter in a year should be
able to tell **which parts of the compiler were decisions and which were
accidents** — and in a compiler, that distinction is harder to recover
afterwards than anywhere else in the system.

## Two things that do not work, and should not

```lisp
quokka> (length '(1 2 3))
; bad CALL: reached an interpreted closure — compiled code can only call
  compiled code, and the prelude is not compiled yet
```

`length` is in the prelude, which is still *interpreted* — so a compiled
program calling it is exactly the mixing of the two evaluators that chapter 24
ruled out. The VM refuses by design rather than by accident.

The second is funnier and the same rule:

```lisp
quokka> (compile-form '(+ 1 2))
; bad CALL: reached an interpreted closure — ...
```

**The compiler cannot be called from compiled code either**, because the
compiler is interpreted. Typed into the interpreting REPL it works; typed into
the compiling one it is refused, by the same machinery and for the same
reason.

Both stop being errors when the prelude and the compiler are themselves
compiled, which is the next piece of work and most of what phase 1 has left.
Chapter 29 compiles both of them, and finds that the second half — installing
what was compiled — cannot be done here at all, and belongs to the image.
Until then they are the right failure: a rule enforced from the day it is
made, rather than one observed until something needs breaking it.

---

**References.** Queinnec, *Lisp in Small Pieces*, chapters 6 and 7, for a
compiler to a bytecode machine written in the language it compiles, which is
this chapter's exact shape done properly. SICP §5.5 for a compiler for an
explicit-control machine, and §4.1.7 for the idea it rests on. Appel, *Modern
Compiler Implementation* (1998), chapter 1, for how much of a compiler is a
dispatch on the shape of a form — a thing that looks accidental in one
compiler and obviously structural across three.
