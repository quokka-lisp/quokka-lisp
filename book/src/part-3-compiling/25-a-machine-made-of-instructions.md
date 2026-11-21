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

## The two tables

A question that comes up on first reading the source, and the answer is not
obvious from either file: **where, exactly, does a compiled program meet the
Rust?**

There are two numbered tables in the system, and they are unrelated.

| | what it is | how many | where |
| --- | --- | --- | --- |
| `Op` | the instructions the VM executes | 36 | `code.rs` |
| the primitive table | Rust functions Lisp can call | 50 | `primitives.rs` |

Nothing in the first table is callable from Lisp. Nothing in the second is
executable by the VM. They are different layers, numbered separately, and the
only thing joining them is **one instruction**:

```
  PRIM p, n
```

whose first operand is an index into the *second* table. That instruction is
the entire door. In the VM it is nine lines:

```rust
Op::Prim => {
    let args = self.pop_n(decoded.b as usize);
    let index = decoded.a as u32;
    ...
    match primitives::call_slice(self.mc, index, &args) {
        Ok(v) => self.push_fresh(v),
        Err(e) => return self.fail(e),
    }
}
```

Pop *n* values, hand them and an index to `primitives::call_slice`, push what
comes back. `(+ 2 3)` is `PRIM 0, 2`, because `+` is entry 0.

### The second way in, which is the one that is easy to miss

A primitive is also a **value**:

```rust
pub(crate) const fn primitive(index: u32) -> Value<'h> {
    Value(((index as u64) << IMM_PAYLOAD_SHIFT) | SUBTAG_PRIMITIVE | TAG_IMMEDIATE, ...)
}
```

An immediate (chapter 1) carrying the table index in its payload. Which is why
a symbol's function cell can hold one, why `#'car` is a thing you can pass
around, and why this works with no `PRIM` instruction anywhere in it:

```
  GFREF 0        ; push the function cell of +
  FIX 1
  FIX 2
  CALL 2
```

`CALL` finds that the callee is not a compiled closure but a primitive value,
pulls the index back out with `as_primitive()`, and ends up in the same
`call_slice`. Same destination, two routes: **by name at compile time**, or
**as a value at run time**.

The compiler uses the first whenever it can, which is whenever the name is a
`COMMON-LISP` symbol — the package-lock argument above. The second is what
`(apply #'+ xs)` has to use, because there the function is a value nobody knew
at compile time.

### The third layer, which is most of the language

Neither table holds `length`, `member`, `reverse` or `defstruct`. Those are in
`lisp/prelude.lisp` — **55 definitions**, written in Lisp, compiled to
instructions like anything else. So `(length v)` on a vector goes:

```
      (length v)         Lisp         a compiled function
           │
           ▼
  (%sequence-length v)   primitive    PRIM 27, 1
           │
           ▼
      header.slots       Rust
```

and `(length xs)` on a *list* does not reach Rust at all — the prelude walks
the conses itself, in Lisp, and only the vector case needs the primitive:

```lisp
(defun length (sequence)
  (cond ((null sequence) 0)
        ((consp sequence)
         (labels ((walk (rest n)
                    (if (endp rest) n (walk (cdr rest) (+ n 1)))))
           (walk sequence 0)))
        (t (%sequence-length sequence))))
```

One function, one of whose two branches crosses into Rust and the other of
which never leaves Lisp. That is rule 1 at the granularity of a single
`cond`.

Three layers, and only the middle one is a table of Rust functions. Rule 1 is
the shape of this diagram: the primitive table is "the only door from Lisp
into the host", and it has fifty entries because everything else was built
above it instead.

And **special operators are in neither table**. `if` is two jumps, `let` is a
`BIND`, `block` is a record on the stack — the VM implements them as
instructions and `primitives.rs` has never heard of them. Which is why
"twenty-two special operators" and "fifty primitives" are not comparable
numbers: they count different kinds of thing, and the first is a claim about
the language where the second is a claim about the implementation.

### One wart, since you have the file open

`call_slice` uses the index to find the entry, checks the arity from it, and
then dispatches like this:

```rust
match p.name {
    "+" => fold(p.name, args, 0, |a, b| a.checked_add(b)),
    "*" => fold(p.name, args, 1, |a, b| a.checked_mul(b)),
    ...
```

**On the name.** A string comparison, after an index lookup that had already
identified the entry uniquely. The table carries a name and an arity and no
function pointer, so the index gets you the metadata and then the dispatch
starts again from scratch.

It has not mattered — the whole thing was reached from a tree-walking
interpreter where this was lost in the noise. With a bytecode VM in front of
it, it is the most obviously wasteful thing on the path, and the fix is to put
a function pointer in the table beside the name. Written down here rather than
done, because the honest reason to do it is a measurement nobody has taken
yet.

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

## And what the two machines share

That section, and most of this chapter, has been about difference. Which
invites a conclusion that is mostly wrong, and worth correcting before it
hardens:

> *The bytecode VM is a separate thing that works in a completely different
> way from the tree-walking interpreter.*

It is a different **machine**. It is not a different **world**. Almost
everything underneath both of them is the same code, and the line between what
is shared and what is not turns out to be sharp and worth naming.

| shared, one copy | each has its own |
| --- | --- |
| the heap, and the collector that moves things in it | the stack of frames |
| tagged values — fixnums, conses, symbols | the operand stack |
| symbols and the obarray | the program counter, or the `Control` register standing in for one |
| the reader and the printer | how the next thing to do is decided |
| the primitive table — *both* call `+` the same way | |
| the layout of an environment | |
| **matching arguments to a lambda list** | |

The pattern: they share **mechanism** and keep their own **state**.

Which is also exactly what chapter 24's decision was about, although it did not
say so in these words. "One evaluator at a time" forbids an interpreted frame
underneath a compiled one, an unwind crossing from one machine's frames into
the other's, a scope held in two representations at once. All of those are
*state*. None of them is a Rust function that takes a heap and some values and
returns an answer.

If sharing mechanism were the problem, the two would need separate heaps.

### The one that looks like a contradiction

The last row of that table is the one that raises an eyebrow, so it is worth
following.

Chapter 18 showed that a lambda list is rewritten when a closure is made:
default forms become a `let*` around the body, so nothing has to be *evaluated*
while arguments are being bound. What survives the rewrite still looks like
this, though:

```lisp
(kind &optional (colour) &key (:sides sides))
```

and matching arguments against *that* is unavoidably a run-time job. Which
keyword actually turns up is not something a compiler can know:

```lisp
(describe-shape 'square :sides 4)
(describe-shape 'square)
```

Two calls, the same compiled function, different work to do on arrival. So the
callee has to decide at call time which name gets which value — and that
decision is **identical** for a function the interpreter is applying and one
the VM is calling. Same lambda lists, same rules.

Common Lisp's rules here are fiddly in ways the standard is specific about:
the *leftmost* occurrence of a keyword wins, an unknown keyword is an error
unless `&allow-other-keys` was declared, and a parameter with no argument gets
the unsupplied marker rather than `nil`, so that `(f)` and `(f nil)` can
differ. Writing that twice is two chances to get one of those subtly
different, and rule 6 says both have to match SBCL.

So there is one copy, in `lambda_list.rs`, and its signature is the guarantee
that it is not a back door:

```rust
pub fn bind<'h>(
    mc: &mut Mutation<'h>,
    params: Value<'h>,
    arguments: Value<'h>,
) -> Result<(Vec<Value<'h>>, Vec<Value<'h>>), EvalError>
```

No machine. No stack, no frames, no program counter. A heap, a lambda list, and
some arguments. Whichever machine is running calls it, at a time when the other
is not running at all.

### What sharing costs, and how it was paid

Sharing is not free, and the shape of the cost here is a good one to recognise
because it recurs.

The shared binder takes its arguments as a **list**, because that is what the
interpreter has: it collected them one at a time into a chain of cons cells. A
compiled call does not have a list. It has the arguments in a row on the
operand stack. Using the shared binder would mean consing them up only for the
binder to walk back down — **an allocation per argument, in every call**, which
is one of the costs compiling was supposed to remove.

The answer is not a second binder. It is to take the common case directly and
fall through:

```rust
if let Some(names) = self.all_required(params) {
    // the overwhelmingly common shape: no list, no allocation beyond the scope
    return Ok(env::make(self.mc, parent, env::VARIABLE, &names, args));
}
// &optional, &rest or &key — build the list and use the shared binder
```

The same move had already been made one layer down: `primitives::call` took a
list, so it was split into a thin wrapper over `call_slice`, which takes a row.
Both times the question was the same — *the shared code speaks the
interpreter's language; does the VM translate, or duplicate?* — and both times
the answer was to narrow the shared part rather than clone it.

### Could we ship only one of them?

A fair question once the sharing is clear, and it has a real answer, although
not the one you would expect.

After phase 1 the browser does not need the interpreter. It loads an image — a
heap with everything already compiled in it — and runs the VM. The interpreter
exists to *build* an image from source with nothing but `cargo`, which is a
thing you do on a development machine, not in a tab.

So: build the browser without it, and ship less code.

The honest arithmetic, measured rather than guessed. `eval.rs` is **2,340
lines** of the **8,939** non-test lines in the VM crate — a little over a
quarter of the source. The whole compiled WebAssembly module is **190,976
bytes**, which is 186 KB, and compresses to a fraction of that over the wire.
Even deleting the interpreter entirely would save perhaps a few tens of
kilobytes after compression, against an *image* that will be measured in
megabytes.

(Both numbers are as of writing and will drift, which is the hazard of putting
figures in prose. The wasm one is `cargo build -p quokka-wasm --target
wasm32-unknown-unknown --release` and the size of what lands in `target`; the
line counts are each file up to its `#[cfg(test)]`. Re-measure rather than
trust them — an earlier draft of this paragraph said *a third of seven
thousand*, which was a guess that read like a measurement.)

So the size argument, which is the obvious one, is the weak one.

The strong argument is different, and it is this book's recurring theme
wearing yet another costume. "One evaluator at a time" is currently **a rule in
prose**. A build that physically does not contain the interpreter cannot break
it — any dependency on interpretation that crept in would show up as a *build
error*, at the moment it was introduced, rather than as a silent fallback that
works perfectly until the day it matters.

*A rule that can be followed needs discipline; a rule that cannot be broken
needs none.*

There is a precondition, and it is the kind that is cheap now and expensive
later. Today `vm.rs` begins:

```rust
use crate::eval::{closure, env, EvalError};
```

Those three are not the interpreter. `closure` is what a function object looks
like, `env` is what a scope looks like, `EvalError` is what going wrong looks
like — shared mechanism, every one of them, living in `eval.rs` because that is
where they were first needed. While they live there, "build without the
interpreter" is a refactor. Once they live somewhere of their own, it is a line
in a manifest.

Which is the same shape as the image-format question in chapter 24: the payoff
is later, the structure that allows it is now.

---

**References.** `docs/bytecode.md` in this repository is the specification —
all thirty-six opcodes with their operand widths, which this chapter
explains rather than repeats. Queinnec, *Lisp in Small Pieces*, chapter 7,
builds a bytecode machine for Scheme in the same order used here. Goldberg
and Robson, *Smalltalk-80: The Language and Its Implementation* (1983) — the
"Blue Book" — part four, is the classic description of a stack-based bytecode
VM and is still the most readable. For the fetch–decode–execute loop as hardware does it,
Patterson & Hennessy, *Computer Organization and Design*, chapter 4.
