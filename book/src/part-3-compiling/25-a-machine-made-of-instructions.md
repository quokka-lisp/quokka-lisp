# A machine made of instructions

> **Where this fits.** Chapter 24 argued *why* to compile. It did not say what
> the result is. This chapter is the answer in full: what a virtual machine
> actually is, what an instruction is made of, why the instructions push and
> pop a stack, and how a loop of about twenty lines runs them.
>
> Nothing here assumes you have seen a machine instruction before. It assumes
> only Part II — the reader, the evaluator, environments and frames.
>
> **Status: specified, not built.** `docs/bytecode.md` is the specification
> this chapter explains. The VM is step B of
> [#23](https://github.com/quokka-lisp/quokka-lisp/issues/23) and does not
> exist yet.

## Start from what already works

Here is the thing the interpreter does, written out. When it meets this:

```lisp
(+ (* 2 3) 4)
```

the reader has already turned it into a tree of cons cells (chapter 8):

```
     (+ · )
      │   └── 4
      └── (* · )
           │   └── 3
           └── 2
```

and the evaluator walks that tree. At every node it asks a series of
questions — *is this a list? is the head a special operator? is it a macro? no?
then evaluate each argument and call it* — and those questions are asked again
every single time the expression runs.

Chapter 24's observation was that the answers never change. `*` is not going
to stop being a function call between one execution and the next. So the
answers can be worked out once and **written down**.

What they get written down as is a list of instructions. That is the entire
idea, and everything below is detail.

## What an instruction is

An instruction is a very small, very specific command. Not "evaluate this
expression" but "push the number 2", or "add the top two things together".

It has two parts:

- the **opcode** — short for *operation code* — which says *what to do*. It is
  just a number, with a name we give it for the sake of reading: `47` might
  mean "push a constant", and we write it `CONST`.
- the **operands** — zero or more — which say *what to do it to*. `CONST 3`
  means "push constant number 3".

That is all. An instruction set is a list of the operations you are allowed to
name, and a program is a sequence of them.

> **A word of warning about the word "immediate".** This book already uses it
> for a *tagged value* that needs no heap object — `nil`, `t`, a character
> (chapter 1). In machine-code writing it also means *an operand that is the
> value itself rather than a reference to it*. Two unrelated meanings, one
> word. This chapter says "a literal operand" for the second to keep them
> apart, and the [glossary](../glossary.md) lists both.

## Why the instructions push and pop

Here is the part that is genuinely clever, and which looks strange until you
have watched it work once.

Our instructions do not say where to put their results. There is a shared pile
of values — the **operand stack** — and every instruction takes its inputs off
the top and puts its output back on the top.

A *stack* here means exactly what it means in ordinary speech: a pile where
you add and remove at one end. Put three plates down, and the first one you
pick up is the third one you put down. Adding is called **pushing**; removing
is **popping**.

So `(+ (* 2 3) 4)` becomes five instructions, and the pile looks like this as
they run:

```
                      the stack, bottom to top
                      ────────────────────────
  FIX 2               2
  FIX 3               2  3
  PRIM *, 2           6                 ← popped 2 and 3, pushed 6
  FIX 4               6  4
  PRIM +, 2           10                ← popped 6 and 4, pushed 10
```

`FIX` pushes a small whole number written directly in the instruction.
`PRIM p, n` calls primitive number `p` with the top `n` values.

Two things to notice, because they are the whole reason for the design.

**Nothing had to say where the result of `(* 2 3)` goes.** It went on the
stack, and the next instruction that wanted a value found it there. No
temporary variable, no register to allocate, no bookkeeping.

**The shape of the instructions matches the shape of the expression.** An
expression nests, and a stack nests. `(* 2 3)` finished and left exactly one
value behind, which is precisely what `(+ ... 4)` needed. That correspondence
is not a coincidence — it is why compiling to a stack machine is so much
simpler than compiling to anything else, and chapter 24's comparison with
register machines is the longer version of this paragraph.

### Try the nesting one level deeper

```lisp
(* (+ 1 2) (- 10 4))
```

```
  FIX 1               1
  FIX 2               1  2
  PRIM +, 2           3
  FIX 10              3  10
  FIX 4               3  10  4
  PRIM -, 2           3  6
  PRIM *, 2           18
```

The left-hand answer, `3`, simply waits at the bottom of the pile while the
right-hand side is worked out on top of it. Nobody wrote code to make it wait.
That falling-out is what people mean when they say a stack machine is a
natural fit for expressions.

## Where the stack lives

In Quokka it is **one growable vector in the heap**, with a number in a
register saying how deep it currently is — the **stack pointer**.

That it is in the heap is not an implementation detail, it is rule 4 again.
Chapter 2 established that no execution state may live on Rust's own call
stack, because the VM has to be able to stop between instructions and hand
control back to the browser. A pile of values in a Rust `Vec` local to the
interpreter loop would be exactly the state we are forbidden to keep there.
Putting it in the heap means it is traced by the collector, saved by an image,
and survives being stopped.

One detail earns a sentence, because it is a bug that would not have
announced itself. The collector traces *every* slot of a traced object. The
stack vector has slots above the stack pointer holding whatever was last
popped from them, so those dead values would be kept alive forever. The fix
is to clear a slot when it is popped — one store, and "trace every slot" stays
true rather than the collector learning about a moving boundary.

## How the machine runs

A **program counter** — the `Pc` register — holds the position of the next
instruction, as an offset in bytes from the start of the code. Running the
program is a loop:

```
  loop {
      fetch the opcode byte at Pc
      read however many operand bytes that opcode takes
      advance Pc past them
      do the thing
  }
```

This is called **fetch–decode–execute**, and real processors are built around
the same three steps. It is worth saying plainly how small it is: the whole of
"running compiled Lisp" is that loop plus one arm per opcode, and the arms are
mostly two or three lines.

### Where the loop stops

It does not run to the end of the program. Rule 4 says the VM executes a
bounded number of steps and returns to its caller, so the loop has a budget
and returns when it runs out.

That works only because `Pc` is a register in the heap rather than a variable
in the Rust loop. Stopping is then just *returning* — and starting again is
reading `Pc` back. The same property makes an image resumable: everything
needed to carry on is in the heap, so it can be written to a file.

This is the fourth or fifth time in this book that chapter 2's constraint has
paid for something unrelated. At this point it is less a run of luck than a
property: **state you are forced to make explicit is state you can transport.**

## Why the instructions are bytes

The instructions are stored as a run of bytes in a `Code` object — which is a
heap object whose contents the collector does *not* trace, because instruction
bytes are not references and following them would be a disaster (chapter 19
built that distinction for strings).

Each instruction is one opcode byte followed by however many operand bytes
that opcode takes. Fixed per opcode: `CONST` always takes two, `CALL` always
takes one. That matters for a reason worth knowing, because it is a trap the
design walked around rather than through:

> If operand sizes could vary — if small numbers took one byte and large ones
> took four — then the distance a jump has to travel would depend on the sizes
> of the instructions in between, and *those* sizes would depend on the jump
> distances. An assembler then has to go round and round until the two agree.
> It has a name, **branch relaxation**, and fixing the widths per opcode means
> it never arises.

Chapter 24 has the argument for bytes over one fat word per instruction, and
the honest arithmetic: about 2.6 bytes an instruction, roughly three times
smaller.

## A real function, start to finish

```lisp
(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
```

```
       LREF 0 0        ; push n
       FIX 0           ; push 0
       PRIM =, 2       ; pop both, push whether they are equal
       JMP-NIL else    ; pop it; if nil, jump
       FIX 1
       RET             ; the answer for n = 0
else:  LREF 0 0        ; push n again — the left argument of *
       GFREF 2         ; push FACT's current function
       LREF 0 0
       FIX 1
       PRIM -, 2       ; (- n 1)
       CALL 1          ; call FACT with one argument
       PRIM *, 2       ; multiply by the n we pushed first
       RET
```

Fourteen instructions. Read it slowly once: every line is one of the things
the interpreter was deciding at run time, decided instead by the compiler and
written down.

`LREF 0 0` is the one that has not been explained, and it is the subject of
the next chapter. For now: it means *the variable in the innermost scope,
first slot* — the compiler worked out where `n` lives and put coordinates in
the instruction, instead of leaving the name `n` to be searched for.

### Jumps are how `if` survives

There is no `if` instruction. An `if` becomes a **jump**: an instruction that
changes `Pc` instead of computing anything.

`JMP-NIL else` pops a value and, if it is `nil`, sets `Pc` to the position
labelled `else`. Otherwise it does nothing and the next instruction runs
normally.

That is the whole of control flow. `when`, `unless`, `cond`, `and`, `or`,
`dolist`, `do` are macros that expand into `if` and `tagbody` (chapter 15), and
both of those become jumps. The structure visible in the source — nesting,
indentation, blocks — does not exist down here at all. There is one straight
run of instructions and a `Pc` that sometimes moves somewhere other than
forward.

## What the compiler does and does not know

Two instructions in `fact` differ in a way that is easy to miss and is a real
decision.

`=`, `-` and `*` compiled to `PRIM`, which calls a primitive **directly**. The
compiler did not look them up; it baked in which one to call.

`fact` compiled to `GFREF` — *go and read the function cell of the symbol
`FACT` right now*. Every recursive call reads it again.

Why the difference? Because somebody might redefine `fact`:

```lisp
(defun fact (n) 'not-today)
```

and the next call must get the new one. That is ordinary Lisp and it must keep
working. But redefining `*` is **not** ordinary — CLHS 11.1.2.1.2 says the
consequences of redefining a symbol in the `COMMON-LISP` package are
undefined, which is the standard giving an implementation permission to assume
it will not happen.

So the standard's rule about not redefining `car`, which reads like
bureaucracy, is precisely what makes it legal for arithmetic to compile to
three instructions instead of a function-cell read and a call.

## Three operators that compile to nothing

The last thing worth seeing before the next chapter, because it is the
clearest sign that a compiler is a *different kind of thing* rather than a
faster interpreter.

| | |
| --- | --- |
| `the`, `locally` | compile their body and contribute no instructions of their own |
| `eval-when` | decides, at compile time, whether there is a body to compile at all |
| `macrolet` | **vanishes entirely** |

`macrolet` cost the interpreter a new frame kind, an arm in the dispatch and a
section of chapter 23. In the compiler it is not implemented so much as
*finished with*: the expander runs while compiling, the call it expands is
compiled in its place, and no instruction is ever emitted that knows a
`macrolet` was there.

An interpreter has to carry every feature of the language with it at run time.
A compiler gets to spend features at compile time and hand the machine
something smaller.

---

**References.** `docs/bytecode.md` in this repository is the specification —
all thirty-six opcodes with their operand widths, which this chapter
explains rather than repeats. Queinnec, *Lisp in Small Pieces*, chapter 7,
builds a bytecode machine for Scheme in the same order used here. Smith's
*Smalltalk-80: The Language and its Implementation* (the "Blue Book"), part
four, is the classic description of a stack-based bytecode VM and is still the
most readable. For the fetch–decode–execute loop as hardware does it,
Patterson & Hennessy, *Computer Organization and Design*, chapter 4.
