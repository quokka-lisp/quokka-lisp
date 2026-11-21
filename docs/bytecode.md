# Quokka bytecode — the instruction set

**Status: specification, nothing implemented.** Written before the VM and the
compiler, deliberately, so that it can be argued with (#25 is the record of
one such argument changing two answers). Where the implementation contradicts
this document, this document is wrong and gets corrected.

Decided already, in `docs/design.md` and #25:

- a **stack machine**, not a register machine;
- **packed bytes** with each opcode's operand widths fixed, so an
  instruction's size is a function of its opcode alone and no branch
  relaxation pass is needed;
- **one evaluator at a time** — the interpreter bootstraps and then goes cold,
  so nothing here has to interoperate with it;
- a **contiguous stack** in the heap;
- **lexical addressing over the environment chain**, not flat closures.

## 1 · What a compiled function is

Two objects, because the traced/raw line runs between them. `ObjectType::Code`
holds raw bytes the collector must not follow; constants are values it must.

```
Function  (traced)
  0  code        -> Code object: raw instruction bytes
  1  constants   -> Vector: symbols, lists, strings, nested Functions
  2  params      -> the simple lambda list, as the binder wants it
  3  max-stack   fixnum: deepest the operand stack gets, in words
  4  name        symbol, for the debugger and the disassembler
```

`params` is the lambda list **after** `lambda_list::rewrite` has run, so it
contains only bare symbols, `&optional`, `&rest`, `&key (keyword variable)`
and `&allow-other-keys` — everything that can be bound without evaluating
anything. Default forms became a `let*` in the body at macroexpansion time
(chapter 18), which is why the callee's prologue has no forms to run.

`max-stack` is checked once when a frame is pushed, so individual pushes need
no bounds check. The JVM class file format carries the same field for the same
reason.

A **closure** is the existing `ObjectType::Closure` with its `BODY` slot
holding a `Function` instead of a list of forms. `funcall`, `apply` and the
function namespace are untouched; the apply path grows one type test.

## 2 · The stack, and what a frame is

One growable `Vector` in the heap, with a stack pointer in a register. An
ordinary traced object, so slicing, image save and the collector need no
special knowledge of it.

**Popped slots are cleared.** Every slot of a traced object is traced, so a
dead slot above the stack pointer would keep whatever it last held alive. One
store per pop is cheaper than teaching the collector about a dynamic extent,
and keeps "trace every slot" true.

A call pushes four words, then the callee's operands grow above them:

```
   ...caller's operands...
   saved pc            <- frame pointer
   saved frame pointer
   saved function
   saved environment
   ...callee's operands...
```

Arguments are **not** in the frame. `CALL` pops them, and the callee's
environment is built from them before the frame is entered, because variables
live in the environment chain rather than in frame slots. That is one
allocation per call and it is the price of the chain; `docs/design.md` records
why it is being paid for now.

Markers for `block`, `catch`, `unwind-protect` and `handler-case` are records
pushed on the same stack with a kind word, and an unwind walks downward
looking at kinds. Exactly the interpreter's scheme (chapters 14 and 20) with
the cons list replaced by a region.

## 3 · Registers

The existing ones (`Control`, `Env`, `Value`, `Stack`, `Unwind`, `Values`)
plus what a bytecode machine needs:

| | |
| --- | --- |
| `Pc` | the offset of the next instruction, as a fixnum |
| `Function` | the `Function` being executed, which is where constants come from |
| `Sp` | the stack pointer, as a word index |
| `Fp` | the frame pointer |

`Env` and `Values` keep their meanings exactly. `Control` and `Stack` are the
interpreter's and are unused while compiled code runs.

## 4 · Encoding

One opcode byte, then operands whose widths the opcode fixes:

| | |
| --- | --- |
| `u8` | one byte: a depth, a slot, an argument count, a primitive index |
| `u16` | two bytes: an index into the constants vector |
| `s16` | two bytes, signed: a jump offset, relative to the byte after the operand |

No varints and no `EXTENDED_ARG`. That is what keeps the assembler to one pass
with backpatching: a jump's distance can be computed as soon as the
instructions between it and its target are chosen, because their sizes do not
depend on their operands.

## 5 · The instructions

Thirty-four. Grouped by what they are for, not by number.

### Pushing a value

| | bytes | |
| --- | --- | --- |
| `NIL` | 1 | push `nil` |
| `T` | 1 | push `t` |
| `FIX i:s16` | 3 | push a small fixnum without using a constant slot |
| `CONST k:u16` | 3 | push `constants[k]` |
| `POP` | 1 | discard the top |

`NIL`, `T` and `FIX` are not optimisations so much as an admission that
small integers and the two interesting symbols are most of what a program
mentions.

### Variables

| | bytes | |
| --- | --- | --- |
| `LREF d:u8 i:u8` | 3 | push the variable `d` scopes out, slot `i` |
| `LSET d:u8 i:u8` | 3 | pop into it |
| `GREF k:u16` | 3 | push the value cell of the symbol `constants[k]` |
| `GSET k:u16` | 3 | pop into it |
| `FREF d:u8 i:u8` | 3 | the **function** namespace — `flet` and `labels` |
| `GFREF k:u16` | 3 | the global function cell |

Two namespaces in the instruction set is what makes this a Lisp-2, and it is
the clearest place in the system to see that the choice is not cosmetic.

An `Environment` binds in exactly one namespace, so `d` counts scopes of
*every* namespace, not only variable ones. The compiler sees all of them and
counts correctly; it is a thing to get right rather than a reason to change
anything.

### Scopes

| | bytes | |
| --- | --- | --- |
| `BIND k:u16` | 3 | pop *n* values, push a variable scope binding them; `constants[k]` is the vector of names, and *n* is its length |
| `BINDF k:u16` | 3 | the same in the function namespace |
| `UNBIND n:u8` | 2 | drop *n* scopes |

The names are carried although nothing looks them up — a compiled reference is
an index. They are there for the debugger, and for a disassembler that can
print something a person can read. Dropping them would save a constant slot
per scope and make every backtrace anonymous.

### Calls

| | bytes | |
| --- | --- | --- |
| `CLOSURE k:u16` | 3 | make a closure of the `Function` `constants[k]` over the current environment |
| `CALL n:u8` | 2 | pop *n* arguments and the callee beneath them, call it |
| `TCALL n:u8` | 2 | the same, reusing the current frame |
| `PRIM p:u8 n:u8` | 3 | call primitive `p` with *n* arguments, with no lookup |
| `RET` | 1 | pop the frame, leave the value on the caller's stack |

`PRIM` is where most of the speed is. The compiler may emit it for a
`COMMON-LISP` symbol because CLHS 11.1.2.1.2 makes redefining one undefined
behaviour — so `(+ a b)` becomes three instructions with no function-cell read
at all. A **user's** function keeps its `GFREF`, because redefining it is
ordinary and must take effect.

That distinction is worth noticing: the standard's package locks, which look
like bureaucracy, are what makes inlining arithmetic legal.

The `%` names (chapter 22) are inlinable too, for a different reason — they
are Quokka's own and a program is told not to call them at all. When packages
arrive in phase 2 and they become `quokka-internal::` symbols, the reason
becomes the same one.

The table holds **50** primitives today, against `PRIM`'s ceiling of 256.

`TCALL` is an instruction rather than an optimisation. The compiler knows at
compile time which calls are in tail position, so it is a property of the
source written into the output where a disassembler can show it — unlike
chapter 10's version, which was an emergent property of popping a frame before
dispatch.

### Branching

| | bytes | |
| --- | --- | --- |
| `JMP o:s16` | 3 | |
| `JMP-NIL o:s16` | 3 | pop; jump if it is `nil` |
| `JMP-NOT-NIL o:s16` | 3 | pop; jump if it is not |

**`tagbody` and `go` mostly disappear into these.** The interpreter searches a
list of body forms for a tag at run time, every time round a loop; the
compiler resolves it to an offset once. Every loop in Lisp is a `tagbody`
underneath, so this is the single clearest case of a compiler doing something
an interpreter structurally cannot.

### Leaving early

| | bytes | |
| --- | --- | --- |
| `BLOCK k:u16 o:s16` | 5 | push a block record with a fresh marker, bound in the block namespace under `constants[k]`; a `RETFROM` to it resumes at `here + o` |
| `RETFROM d:u8 i:u8` | 3 | pop a value and unwind to the marker `d` scopes out, slot `i` |
| `CATCH o:s16` | 3 | pop a tag, push a catch record; a `THROW` to it resumes at `here + o` |
| `THROW` | 1 | pop a value and a tag, unwind |
| `PROTECT o:s16` | 3 | push an unwind-protect record whose cleanup is at `here + o` |
| `UNPROTECT` | 1 | pop it and run the cleanup |
| `HANDLER k:u16 o:s16` | 5 | push a handler record with the clauses `constants[k]` |
| `UNHANDLER` | 1 | pop it |
| `TAG k:u16 o:s16` | 5 | a `tagbody` marker, emitted **only** when a closure could `go` to it from outside |
| `GO d:u8 i:u8` | 3 | the non-local case |

`block` and `tagbody` go through the environment rather than through a known
stack depth because a `return-from` or a `go` can be inside a closure that has
escaped, where no depth is knowable. Treating both cases alike is slower and
is one mechanism; specialising the local case to a `JMP` plus some unwinding
is a later pass with a profile attached.

### Multiple values

| | bytes | |
| --- | --- | --- |
| `VALUES n:u8` | 2 | pop *n*, push the primary (or `nil`), set the `Values` register |
| `MVCALL n:u8` | 2 | `multiple-value-call` over *n* argument forms |
| `MVSAVE` | 1 | hold the current values across what follows |
| `MVRESTORE` | 1 | put them back — the two halves of `multiple-value-prog1` |

**The invariant, which is the whole of chapter 23 restated for a machine with
no single delivery point:**

> Every instruction that pushes a **newly computed** value clears `Values`.
> Only `VALUES`, `VALUES-LIST` and a `RET` passing a callee's values through
> may leave it set.

The interpreter had one funnel — `deliver` — and clearing there was free. A
bytecode VM has thirty-four funnels, so this is a rule each one must follow,
and forgetting it means a stale second value surfacing in a form a long way
from the one that set it. It belongs in a test, not only in a sentence.

## 6 · What the special operators compile to

| | |
| --- | --- |
| `quote` | `CONST` |
| `if` | `JMP-NIL` and `JMP` |
| `progn` | the forms, with `POP` between |
| `setq` | `LSET` or `GSET` |
| `let` | the inits, then `BIND`, then the body, then `UNBIND 1` |
| `let*` | a `BIND` of one name per binding |
| `flet` / `labels` | `CLOSURE` per function, then `BINDF`; `labels` binds the scope before making the closures so they can see themselves |
| `function` | `FREF`, `GFREF` or `CLOSURE` |
| `lambda` | `CLOSURE` |
| `block` / `return-from` | `BLOCK` / `RETFROM` |
| `catch` / `throw` | `CATCH` / `THROW` |
| `tagbody` / `go` | `JMP`, plus `TAG` / `GO` when a tag escapes |
| `unwind-protect` | `PROTECT` / `UNPROTECT` |
| `%handler-case` | `HANDLER` / `UNHANDLER` |
| `multiple-value-call` | `MVCALL` |
| `multiple-value-prog1` | `MVSAVE` / `MVRESTORE` |
| `the`, `locally` | **nothing** — they compile their body |
| `eval-when` | **nothing** — it decides, at compile time, whether the body is compiled at all |
| `macrolet` | **nothing whatsoever** — the expander runs in the compiler and never reaches the output |

The last three are the quiet proof that a compiler is a *different kind of
thing* from an interpreter rather than a faster one. `macrolet` is an
operator the interpreter needed a frame kind and a dispatch arm for (chapter
23), and which a compiler implements by having already finished with it.

## 7 · Worked: `fact`

```lisp
(defun fact (n) (if (= n 0) 1 (* n (fact (- n 1)))))
```

The `defun` is a macro over `%set-symbol-function`, so the top-level form is
three instructions, one of which builds the function:

```
  CONST 0            ; the symbol FACT
  CLOSURE 1          ; the Function below
  PRIM 17,2          ; %set-symbol-function
```

and the function itself, `max-stack` 4:

```
       LREF 0 0        ; n
       FIX 0
       PRIM 3,2        ; =
       JMP-NIL else
       FIX 1
       RET
else:  LREF 0 0        ; n, the left argument of *
       GFREF 2         ; FACT's function cell — read now, because a user
                       ;   function may have been redefined since
       LREF 0 0        ; n
       FIX 1
       PRIM 1,2        ; -
       CALL 1
       PRIM 2,2        ; *
       RET
```

Fourteen instructions, **thirty-seven bytes**.

Two things are visible that were invisible in the interpreter. The recursive
call is a `CALL` and not a `TCALL`, because `(fact ...)` is an argument to `*`
and therefore not in tail position — the compiler decided that by looking,
where chapter 10's interpreter would discover it by the frame still being
there. And `=`, `-` and `*` are `PRIM`, while `fact` is a `GFREF`: the
standard's own rule about redefining `COMMON-LISP` symbols is what licenses
the difference.

For contrast, the loop from the README:

```lisp
(defun down (n) (if (= n 0) 'done (down (- n 1))))
```

ends `... PRIM 1,2 / TCALL 1`, and runs in constant stack because the
instruction says so.

## 8 · How big is this, really

Thirty-seven bytes for fourteen instructions is **2.6 bytes each**.

That corrects a number given earlier in #25 and in book chapter 24, which
estimated "a little under two". Two bytes would be right for an instruction
set whose operands were mostly one byte; this one reaches for `u16` constant
indices and `s16` jump offsets often enough to average higher. The comparison
with a fixed 64-bit instruction word is therefore **about 3×, not 4×**, which
does not change the decision but should not be left overstated.

The obvious specialisations, listed so that they are a known option rather
than a later idea:

| | saves |
| --- | --- |
| `LREF0 i:u8` — depth zero, which most references are | 1 byte each |
| `CONST8 k:u8` — fewer than 256 constants, which most functions have | 1 byte each |
| `FIX8 i:s8` — small small integers | 1 byte each |
| `CALL0`…`CALL3` | 1 byte each |

Together they would bring the average under two. The JVM's `iload_0` and
CPython's `LOAD_FAST` are the same idea. **None is in the set above**, because
each one is a second spelling of an instruction that already exists, and rule
7 says the first version should be the one a person can read. They go in when
something measures that they matter.

## 9 · Ceilings

Every one is a **compiler error with a message**, never a truncation. A silent
wraparound here would be a bug with no symptom anywhere near its cause.

| | limit | |
| --- | --- | --- |
| constants per function | 65,536 | `u16` index |
| scope depth | 256 | `LREF`'s `d` |
| variables per scope | 256 | `LREF`'s `i` |
| arguments per call | 256 | `CALL`'s `n` |
| code per function | 32 KB forward, 32 KB back | `s16` jump offset |
| primitives in the table | **256** | `PRIM`'s `p` |

The last is the interesting one, because it is the only limit here that is
also a design rule. `primitives.rs` already says the table's size is part of
the comprehensibility budget and that every entry must justify itself. Making
the 257th primitive fail to assemble turns that from a sentence in a comment
into something the build enforces — which is the distinction chapter 22 draws
between a request and a guarantee, arriving by accident from an encoding
decision.

The code-size limit has a precedent worth knowing: a JVM method may not exceed
65,535 bytes, for the same reason and with the same consequence (generated
code occasionally hits it, and the generator has to split).

## 10 · Open

- **Where macroexpansion happens** once there are two evaluators that both
  need it (#23).
- **What the image format guarantees** across versions (#23). The primitive
  table's order is already part of it; the instruction encoding now is too.
- **Whether `BIND` should carry names.** Kept here for the debugger, at one
  constant slot per scope.
- **Specialised opcodes** (§8), deliberately not now.
