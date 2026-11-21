# Where is the variable?

> **Where this fits.** Chapter 25 left one instruction unexplained. `LREF 0 0`
> pushes a variable, and the two numbers are where it is — but *where* in what,
> counted how, and why is that faster than looking it up by name? This chapter
> is that question and its consequences, and it ends with the honest accounting
> of where a compiled Lisp's speed actually comes from.
>
> **Status: built**, like chapter 25 — with the exception named at the end:
> flat closures and stack-allocated locals are still the faster design, still
> need escape analysis, and are still deferred.

## The question every instruction has to answer

An instruction like "add the top two values" needs no help: it knows where its
inputs are, on the stack. But an instruction like "push `n`" has to say
*which* `n`, and there are several possible answers — a value written into the
instruction itself, an entry in a table, a variable in some scope, a global.

How an instruction names the thing it works on is called its **addressing
mode**. The phrase comes from processor design, where the modes are things
like "register 3" or "the memory at this address", but the idea transfers
exactly: an operand is not a value, it is *directions for finding* a value.

Quokka's instructions use five.

| mode | example | how the value is found |
| --- | --- | --- |
| implicit | `PRIM +, 2` | it is on the stack |
| literal | `FIX 7` | the operand **is** the value |
| constant table | `CONST 3` | look it up in this function's constants vector |
| lexical | `LREF 1 0` | count scopes outward, then index |
| global | `GREF 3` | read the value cell of a symbol |

The interesting one is lexical, because it is the one that replaces work the
interpreter was doing over and over.

## What the interpreter does today

Take this, which is the `adder` from the README:

```lisp
(defun adder (n)
  (lambda (x) (+ x n)))

(funcall (adder 5) 37)   ; => 42
```

When that inner `lambda` runs, the environment it is running in is a chain of
scopes (chapter 10) — a linked list where each link holds some names and their
values, and a pointer to the scope outside it:

```
  [ x = 37 ]  ──►  [ n = 5 ]  ──►  (global)
```

To evaluate `n`, the interpreter walks that chain. It looks in the first link,
compares the symbol `n` against `x`, fails, follows the pointer, compares `n`
against `n`, succeeds. To evaluate `x`, it compares once and succeeds.

This happens **every time the expression runs**. The comparisons are cheap —
symbols are interned, so comparing them is comparing two machine words
(chapter 8) — but there are a variable number of them, and the number depends
on how deeply nested the code is and how many variables are in scope.

## What the compiler knows instead

It knows the shape of that chain *without running anything*, because the shape
is determined by the source text. Reading `(lambda (x) (+ x n))` inside
`(defun adder (n) ...)`, the compiler can see that when this body runs there
will be a scope holding `x`, and outside it a scope holding `n`.

So it writes down coordinates:

| | meaning | instruction |
| --- | --- | --- |
| `x` | innermost scope, first slot | `LREF 0 0` |
| `n` | one scope out, first slot | `LREF 1 0` |

and the body of the lambda compiles to:

```
  LREF 0 0        ; x
  LREF 1 0        ; n
  PRIM +, 2
  RET
```

At run time, `LREF 1 0` follows one pointer and reads slot 0. No symbol is
compared. No name is even *present* — the instruction contains two small
numbers where the interpreter had a symbol to search for.

This is called **lexical addressing**, and it is the single largest difference
between interpreted and compiled code in a Lisp. The glossary has had an entry
for it since chapter 10, ending "which is why Quokka's interpreter cannot use
it yet". This is the *yet*.

### One more, to make the counting concrete

```lisp
(let ((a 1))
  (let ((b 2) (c 3))
    (+ a c)))
```

The chain inside the inner body is `[b c] ──► [a]`, so:

- `a` is one scope out, slot 0 → `LREF 1 0`
- `c` is the innermost scope, slot 1 → `LREF 0 1`

The first number counts *links followed*; the second counts *slots along*.
That is the whole of it.

## Why globals are different

`GREF` does not get coordinates. It carries a constant-table index, and that
constant is a **symbol**, and the instruction reads that symbol's value cell at
run time, every time.

It has to, because globals can change:

```lisp
quokka> (setq *limit* 10)
10
quokka> (defun check (x) (< x *limit*))
CHECK
quokka> (check 15)
NIL
quokka> (setq *limit* 20)
20
quokka> (check 15)
T                              ; CHECK noticed, without being recompiled
```

(Quokka spells this `setq` because `defvar` and `defparameter` are tier 1 and
not here yet. A global is a symbol's value cell, which is all those macros
set up anyway.)

The same reasoning as chapter 25's `GFREF` versus `PRIM`. A lexical variable's
binding is created and destroyed by code the compiler can see in full, so its
position is a fact. A global's value is reachable by anything at any time, so
only the symbol itself is a fact and the value must be fetched.

**Local variables are fast because they are private.** That sentence is most
of the practical argument for lexical scope, arrived at from the implementer's
side rather than the programmer's.

## Two namespaces, and a trap in the counting

Quokka is a Lisp-2 (chapter 10): `(list 1 2)` and the variable `list` are
unrelated. So there are two instructions where another language would have
one — `LREF` reads a variable, `FREF` reads a function — and `flet` binds in
the function namespace while `let` binds in the variable one.

Which produces a wrinkle that must be got exactly right, and is worth seeing
because it is the kind of thing that silently returns the wrong answer rather
than crashing.

**An `Environment` object binds in exactly one namespace**, and the `d` in
`LREF d i` counts *links in the chain* — all of them, whatever namespace they
belong to. So:

```lisp
(let ((x 1))            ; a variable scope
  (flet ((f () x))      ; a function scope
    (funcall #'f)))
```

When `f` runs, the chain is:

```
  [ f's parameters: empty ] ──► [ f ] ──► [ x = 1 ] ──► (global)
       variable scope          function      variable
```

`x` is **two** links out, not one — even though only two *variable* scopes
exist. The function scope is stepped over when searching, but it is still a
link, and the compiler must count it.

And note the first link: calling a function with no parameters still pushes an
empty scope, because `apply_closure` builds one unconditionally. That is a
decision rather than an accident, and it now has to be *kept*:

> The compiler's idea of the chain and the machine's actual chain must agree
> link for link. If the VM were later changed to skip empty scopes as an
> optimisation, every compiled depth would be off by one — and an off-by-one
> depth does not crash, it reads a different variable.

Neither half can check this alone. What catches it is chapter 24's fourth test
leg: run a file interpreted and compiled and demand the same output. This is
the first concrete example of a bug that *only* that test can find, and it is
a good argument for building the test before the compiler is finished.

## So where does the speed actually come from?

Time to be specific rather than enthusiastic. Take `(+ a b)` with both
variables in scope, and count what each evaluator does.

**The interpreter:**

1. look at the form — it is a cons, so a call or a special form;
2. check whether `+` is a special operator;
3. look up `+` in the function namespace: walk the chain, then read the
   symbol's function cell;
4. check whether what it found is a macro;
5. **allocate** a frame object to remember where it is, and a cons to push it
   on the frame stack;
6. evaluate `a`: dispatch on its type, walk the chain comparing symbols;
7. **allocate** a cons to collect the value;
8. evaluate `b`: the same again;
9. **allocate** another cons;
10. reverse the collected list — **two more conses**;
11. call the primitive.

**The compiled version:**

```
  LREF 0 0
  LREF 0 1
  PRIM 0, 2
```

Three instructions, nine bytes. Two array reads and a primitive call. **No
allocation at all.**

That last point is worth more than it looks. Every allocation in the left-hand
column is also a step towards the next garbage collection, so the interpreter
is not merely doing more work per form — it is manufacturing work for the
collector to do later.

The usual figure quoted for a tree-walker against a bytecode VM is **ten to
fifty times**, and the honest position is that Quokka will measure rather than
quote. But the *shape* of the win is not in doubt, and it decomposes into
exactly three things.

### The three wins, and which ones phase 1 takes

| | what it removes | taken now? |
| --- | --- | --- |
| Analysis once | asking what kind of form this is, every time | **yes** — this is what compiling *is* |
| Lexical addressing | searching a chain by name | **yes** — `LREF d i` |
| No allocation per call | frame objects and argument lists | **partly** |

The third is the one to be careful about, because it is easy to claim in full
and only half true here.

Frames stop being allocated: chapter 24's contiguous stack means a call pushes
four words into a vector rather than building a heap object. But **binding
parameters still allocates**, because variables live in `Environment` objects
on a chain, and a call makes one. So a call costs one allocation instead of
several, which is a real improvement and not the end of the story.

Removing the rest means **stack-allocated locals**: keeping a function's
variables in its stack frame, and only promoting to the heap the ones that a
closure captures and therefore outlive the call. Which requires the compiler
to work out which variables escape — *escape analysis* — and that is deferred,
with its cost written down in `docs/design.md`: the instruction set changes
when it arrives, because a stack slot and a chain coordinate are not the same
operand.

## What "performant at the end" actually means

Worth stating plainly, because it is the goal the whole of Part III serves and
it is not "phase 1 makes Lisp fast".

A tree-walking interpreter has a **ceiling**. Its costs are structural — the
dispatch, the name lookups, the allocation — and the only way past them is to
stop being a tree-walking interpreter. There is no sequence of small
improvements that gets you there.

A compiler plus a dumb VM has a **floor**, and floors can be lowered, one
component at a time, without redesigning anything:

- **better instruction selection** — the compiler emitting smarter sequences,
  which is a change to a Lisp file;
- **specialised opcodes** — `LREF0`, `CONST8`, `CALL1`, listed in
  `docs/bytecode.md` and deliberately not built yet;
- **threaded dispatch** — instead of returning to the top of the fetch loop
  after each instruction and deciding again, each instruction jumps straight
  into the next one's code. Saves the processor a branch it keeps guessing
  wrong;
- **inline caches** — remembering at a call site what was called last time and
  skipping the lookup when it is the same again. Named by Deutsch and
  Schiffman for Smalltalk in 1984, and the ancestor of every fast dynamic
  language since;
- **a generational collector**, which `docs/design.md` already plans for when
  the environment starts allocating heavily.

`docs/design.md` names the first four as where speed comes from — explicitly
*not* a JIT, because a JIT is a second compiler, written per processor, and
rule 7 would not survive it.

### The part that compounds

And then the thing that makes this more than a list of optimisations.

**The compiler is written in Lisp.** After phase 2, the compiled compiler
compiles itself. So an improvement to the compiler improves the compiler that
compiles the compiler, and the gain arrives in everything built afterwards
without a line of Rust changing.

That is the real answer to "how does this end up performant". Not that the
first bytecode VM is fast — it will not be, particularly. It is that after
phase 1 the system's speed is a property of a Lisp file that the system itself
can improve, rather than a property of a Rust program that has to be rewritten.

Which is the same claim chapter 24 made about *correctness* — the language
stops being defined in Rust — arriving a second time, about speed.

---

**References.** Queinnec, *Lisp in Small Pieces*, chapter 5, for lexical
addressing worked through properly, and chapter 7 for the machine that
executes it. Dybvig, *Three Implementation Models for Scheme* (1987), for
what stack-allocated locals and flat closures buy and what they cost. Deutsch
& Schiffman, "Efficient Implementation of the Smalltalk-80 System" (1984), for
inline caches. Ertl & Gregg, "The Structure and Performance of Efficient
Interpreters" (2003), for where a bytecode interpreter's time really goes —
and it is not where most people guess.
