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
settled — thirty-nine instructions, and changing one is a format change. The
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

## `handler-case`, and four faults behind one symptom

The compiler as first written could not compile `handler-case` at all, and
nobody noticed for a week, because nothing it was asked to compile used one.
The prelude's functions do not. It came to light from the side, while building
chapter 31: `(handler-case (car 5) (type-error () 'caught))` worked when
interpreted and stopped the machine when compiled.

It looked like one bug. It was four, and the instructive part is that each
would have hidden the next.

**1 · The compiler had no case for it.** In Quokka `handler-case` is a macro
in the prelude, expanding to the special operator `%handler-case`, and
`compile.lisp` had no clause for `%handler-case`. So the compiler did what it
does with any unknown head: it compiled a *call* to a function of that name.
There is no such function, and the compiling REPL said so: *"the function
%HANDLER-CASE is undefined"*.

**2 · The machine handed the handler `nil`.** When an unwind reaches a
`HANDLER` record (chapter 27), the VM jumps to the handler's code and pushes a
value for it. It pushed the `Value` register, which is right for `block` and
`catch` — they exist to carry a value out — and wrong here, because `error`
starts its unwind with `nil` in that register. A compiled handler would never
have seen the condition it is meant to look at. The VM's own test had not
noticed: its handler threw away whatever arrived.

**3 · The VM could not turn an error into a condition.** When a primitive
fails — `(car 5)` is a primitive being handed a number — the interpreter asks
whether any `handler-case` is open, and if so builds a condition and unwinds
to it. The VM simply stopped. So even with 1 and 2 fixed, an error from inside
a primitive was uncatchable in compiled code.

**4 · `(error c)` lost the condition's type.** Given a condition object, the
standard says `error` signals *that* condition. The VM's `error` wrapped it in
a fresh `simple-error`. That matters exactly where 1 to 3 lead: re-signalling a
condition that a clause did not want.

### The clause dispatch is compiled code

`docs/bytecode.md` had already decided the shape, back when the VM was
specified: a `HANDLER` record stops **every** condition, and deciding whether
this one was wanted is ordinary compiled code at the handler's label. The VM
knows *that* a handler is there and nothing about which conditions it likes.
The alternative, teaching the VM what a clause looks like, would put a
language decision below the primitive table, and rule 1 says those live above
it.

So the compiler turns

```lisp
(handler-case (car 5)
  (type-error (e) (%condition-type e)))
```

into, in effect,

```text
    HANDLER clauses   →handler
    <(car 5)>
    UNMARK
    JMP               →done
handler:                          ; the condition is on the stack
    BIND #(c)
    <(cond ((eq (%condition-type c) 'type-error)
            (let ((e c)) (%condition-type e)))
           (t (error c)))>
    UNBIND 1
done:
```

and the `cond` is built as a Lisp form and handed back to the compiler, which
compiles it like any other. Here is the piece that writes each test:

```lisp
(defun %handler-test (condition type)
  (if (or (eq type t) (eq type 'error) (eq type 'condition))
      t
      (list 'eq (list '%condition-type condition) (list 'quote type))))
```

The matching is the same flat matching the interpreter does in Rust, but it is
decided **at compile time** where it can be: the clause's type is right there
in the source, so a catch-all clause compiles to no test at all. And a
catch-all ends the dispatch. The first version went on to emit the re-signal
after it — dead code the verifier would have accepted — and a test counting
tail calls found two where the source had one.

That last clause, `(t (error c))`, is the re-signal. By the time it runs the
`HANDLER` record has been popped, so the search for a handler carries on
*further out*, to whatever encloses this `handler-case`. That is fault 4's
fix earning its keep: without it, the outer handler would have received a
`simple-error` instead of the `type-error` it was written for.

The protected form is not in tail position — the record must outlive anything
it calls, which is the rule `block` already follows. A clause body *is* in
tail position, because by then the record is gone.

### Which errors are the program's to catch

Fault 3's fix copies the interpreter's rule — *if a handler is open and the
error is the program's fault, signal it* — with one distinction the interpreter
never needed.

In compiled code, a `BadSyntax` error always means the **bytecode** is wrong.
The compiler checked the program's syntax before any of it ran, so what is
left for the VM to complain about is an opcode that is not one, a jump before
the start of the code, a constant that is not there, or compiled code calling
an interpreted function. None of those is a mistake in the program. They are
mistakes in the compiler, or in the bootstrap, and a catch-all `handler-case`
quietly swallowing one would hide the bug behind whatever the program does
about errors. So they halt, as `Malformed` already did:

```rust
fn fail(&mut self, e: EvalError) -> Transition {
    let the_programs = !matches!(e, EvalError::BadSyntax { .. });
    if the_programs && self.has_handler() {
        if let Some(condition) = crate::condition::from_eval_error(self.mc, &e) {
            return self.start_unwind(condition, Value::NIL);
        }
    }
    self.error = Some(e);
    self.halted = true;
    Transition::Halt
}
```

The plan written in the issue was to change the eleven places that raise such
an error so that each raised `Malformed` instead. Deciding by variant in one
place won: it keeps every message as it was, and it is one rule a reader can
check rather than eleven edits a reader has to trust.

### What it says about having two evaluators

Fault 4 is the one to remember. The interpreter's `error` was right; the VM's
was subtly different in two ways, and both evaluators had passed their own
tests for weeks. Nothing compared them. That is precisely the gap the fourth
leg of the differential tests (step E of #23, and chapter 33) exists to close: the same file,
interpreted and compiled, must print the same thing. Until it exists, every
feature implemented twice is a feature that can quietly disagree with itself.

## The rest of the special operators

The section above ended by saying the compiler lacked three more operators.
It lacked ten. The three had come from a comment in the compiler listing what
"must cancel tail position, when they are implemented" — a list of the forms
that leave a record, not a list of what was missing. Checked against the
interpreter's own table of special operators, `eval::special::ALL`, the
missing ones were `catch`, `throw`, `unwind-protect`, `multiple-value-prog1`,
`the`, `locally`, `eval-when`, `tagbody`, `go` and `macrolet`.

The lesson is the one this chapter keeps arriving at: **count against the
source of truth, not against a description of it.**

### A guard, so it cannot happen again

How had ten operators gone missing without a single failure? Because of what
the compiler did with a head it did not recognise. It assumed a function call.
`(tagbody top)` compiled to "call the function `TAGBODY` with the argument
`top`", which assembled, passed the verifier, and failed only when run — and
nothing had run it.

The interpreter already knows which symbols are special operators: each one
carries its code in its `SPECIAL` slot. A new primitive, `%special-operator-p`,
reads that slot, and the compiler asks it before falling back to a call:

```lisp
((%special-operator-p head)
 (error (%string-append "the compiler cannot compile the special operator "
                        (symbol-name head))))
```

Now an operator the compiler has no case for is a compile error that names it.
The name has a `%` because Common Lisp's `special-operator-p` gives SBCL's
answer, and Quokka's is different — `lambda` and `defmacro` are operators here
and macros there.

### `unwind-protect`, compiled twice

`(unwind-protect protected cleanup...)` must run its cleanup however the
protected form is left: by finishing, or by an unwind passing through on its
way to a `catch`, a `block` or a handler. Two ways out, and the cleanup is
compiled once for each:

```lisp
(defun %compile-unwind-protect (form env)
  (let ((cleanup (gensym))
        (done (gensym)))
    (%emit (list 'PROTECT cleanup))
    (%compile (second form) env nil)
    (%emit (list 'UNMARK))
    (%emit (list 'MVSAVE))
    (%compile-for-effect (cddr form) env)
    (%emit (list 'MVRESTORE))
    (%emit (list 'JMP done))
    (%emit-label cleanup)
    (%compile-for-effect (cddr form) env)
    (%emit (list 'UNPROTECT))
    (%emit-label done)))
```

The first copy is the normal path. `MVSAVE` and `MVRESTORE` are wrapped round
it because `unwind-protect` returns *every* value of its protected form
(chapter 23), and the cleanup forms in between would otherwise leave their own
values behind. The second copy is the fragment the unwinder jumps to: it runs
the cleanup and `UNPROTECT` picks the interrupted unwind back up.

Compiling something twice sounds wasteful, and it is a deliberate choice made
in `docs/bytecode.md`. The alternative is one shared copy that both paths
call, which needs somewhere to remember where to come back to — a return
address, which means a frame, which is exactly what the record design was
built to avoid allocating. Code size was the cheaper thing to spend.

`catch` and `throw` are `block` and `return-from` with the identity found at
run time instead of in the source. `multiple-value-prog1` is the `MVSAVE` /
`MVRESTORE` pair on its own. `the`, `locally` and `eval-when` compile to
nothing of their own: the first two are their bodies, and `eval-when` is
decided while compiling, so the output holds either the body or a `NIL`, never
a test. All three follow the interpreter's rules exactly, down to the three
spellings of `:execute` it accepts until there are packages — because two
evaluators that disagree is the bug the section above was about.

### A bug that had been there since the start

The test that `return-from` runs an `unwind-protect` cleanup failed, with an
error about a variable reference that "names a scope that is not there". The
fault was not in `unwind-protect`. It was in `block`, and it had been there
since the VM was built.

`BLOCK` binds the block's name in a new scope and pushes a record. It did them
in that order, so the record saved the environment *including* the block's
scope. Both ways of leaving a block — `UNMARK` at the end and the unwinder
landing after a `return-from` — restore the record's environment, so both put
the block's scope back on. After any compiled `block`, every variable was
looked up one link too deep.

No test had noticed because none read a variable *after* a block. The fix is
to push the record first. The lesson is about tests rather than blocks: a
test that checks a value is returned does not check that everything around it
was left as it was found.

### `tagbody`: where the specification broke, and what replaced it

`docs/bytecode.md` said a `go` to a tag in the same function becomes a plain
`JMP`, and the `TAG`/`GO` instructions are for when a closure could `go` from
outside. Neither half survived being implemented.

A bare `JMP` is not safe, because `go` can appear anywhere. `(f a (go top))`
has values on the operand stack when it runs. `(let ((x 1)) (go top))` has a
scope bound. `(block b (go top))` has a record open. A jump leaves all of
them behind.

And `TAG`/`GO`, as first built, could not loop. They shared `BLOCK`'s code,
so when a `go` landed, the record was popped — and the next `go` to the same
tag found nothing. Pushing it again made a new marker, so a closure made on
an earlier trip round the loop would be holding a marker with no record. No
test had ever run them.

There were three ways forward, and the choice mattered more than most,
because **every loop in Lisp is a `tagbody` underneath** — `dotimes`,
`dolist`, `do` and `loop` all expand into one — so whatever is chosen here is
chosen for every loop. Carsten's question, when the choice was first laid out,
was the right one: *why not the fastest, just because it is more work?* Work
is not a reason. The real reason turned out to be that the fastest option
could not stand alone.

**A `go` from inside a closure can only be done by unwinding.**

```lisp
(tagbody
 retry
   (mapc (lambda (x) (when (bad-p x) (go retry))) items))
```

The `lambda` runs inside `mapc`, several frames deeper than the `tagbody`.
No jump reaches back there. So a general mechanism is needed whatever else is
built, and a fast path can only ever sit on top of it. Both were built.

#### The general mechanism: a record that survives

The interpreter already had the shape. Its `tagbody` binds **every tag to one
marker**, keeps its frame on the stack across `go`s, and carries *which* tag
in a register. The VM now does the same:

```text
    TAG    #(tag0 tag1) ->table   bind both tags to one marker, push a record
  tag0:
    ...statements...
  tag1:
    ...statements...
    NIL                           fell off the end: a tagbody yields nil
    UNMARK
    UNBIND 1
    JMP    ->done
  table:                          a go lands at table + 3 × its tag's index
    JMP    ->tag0
    JMP    ->tag1
  done:
```

`(go tag1)` is `GO depth index`, found through the environment like a
variable. The unwinder carries the index to the `TAG` record, and there —
unlike every other landing — **the record stays**. Everything above it is
dropped, the environment it saved is put back, and control resumes at
`table + 3 × index`.

That last part is a **jump table**: a row of jumps, one per tag, all the same
size, so that "go to the *i*th tag" becomes "go to the *i*th row". It is the
classic way to compile a computed goto, and the same thing a C compiler does
with a `switch` on small integers. The verifier had to learn about it,
because every row after the first is reached *only* by a `go` landing, never
by falling through — without being told, it would have checked the first row
and never looked at the rest. Now it checks that every row is a `JMP`, and
follows each one.

One detail went the opposite way from `block`. A `block`'s record saves the
environment *outside* the block, because a `return-from` resumes after it. A
`tagbody`'s record saves the environment *inside*, because a `go` resumes
among the statements, which can see the tags. So the normal way out is
`UNMARK` and then `UNBIND 1` to drop the tags' scope.

A `go` costs three steps this way: the `GO`, the landing, and the table's
`JMP`.

#### The fast path: `JMP-TAG`

When the `go` is in the same function as its tagbody and nothing with a
record of its own lies in between, the tagbody's record is the innermost one
on the stack. And that record already knows the two things a `go` must
restore: where the stack was when the tagbody began, and the environment
inside it. So the thirty-eighth instruction, `JMP-TAG`, simply drops the stack
to the top of the innermost record, puts back the environment saved in it,
and jumps. One step.

The insight that made it small: **the compiler does not count anything.**
The obvious way to do option C was for the compiler to track how many values
are on the stack and how many scopes are bound at every `go`, and emit the
right number of `POP`s and an `UNBIND`. The compiler does not track stack
depth, and teaching it to would have meant a second copy of what every
instruction does to the stack, which must agree with the verifier's — the
exact duplication issue #30 removed. Using the record makes the question
disappear: whatever was pushed or bound since the tagbody began, the record
undoes.

What the compiler does decide is *when* `JMP-TAG` is safe. It keeps a list of
what is open around the form it is compiling — a nested function, a form
whose record is on the stack, a tagbody — and a `go` uses `JMP-TAG` only when
its own tagbody is the innermost thing on that list. Otherwise, `GO`.

```lisp
(defun %innermost-tagbody (scope)
  (let ((innermost (car %open)))
    (if (and (consp innermost) (eq (second innermost) scope))
        innermost
        nil)))
```

And the verifier checks it statically. Until now it tracked *how many*
records were open at each instruction. `JMP-TAG` needs to know *where* the
innermost one starts and *what kind* it is, so its state became a list of
records rather than a count — the Rust side of the boundary this chapter has
been drawing all along: decisions in Lisp, stack accounting in Rust. The
machine checks again at run time, because a wrong record would leave it in a
state nothing else would notice.

| | steps per local `go` | |
| --- | --- | --- |
| built from `catch`/`throw` (rejected) | about 10+ | and more with every tag |
| `TAG`/`GO` | 3 | works from anywhere |
| `JMP-TAG` | 1 | only when its tagbody's record is innermost |

### `macrolet` waits

A local macro's expander is a function the compiler must *call* while it is
compiling. The compiler is interpreted until #33, so it can only call
interpreted functions — and it cannot make one out of a list, because there
is no `eval`. Once #33 has compiled the compiler, it can compile the expander
with `compile-form` and call it like anything else. `defmacro` waits for the
same reason. Building either now would mean building it twice.

### The differential files, compiled

For a while the differential tests had a second half here: every file compiled
form by form by the bootstrap compiler, everything it emitted verified, and a
form allowed to fail only where its baseline said it was an error. It was the
best check possible before a compiled world existed — it proved the compiler
could handle each form, without running any of them — and it found the
`tagbody` gaps and the deliberate-error forms that a first version, stopping at
the first failure, had silently skipped.

It has since been retired in favour of something strictly stronger: chapter
33 runs every file in a compiled world and compares each answer with the
baseline. Chapter 33 says what that found, and what retiring this cost.


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

*Since chapter 32 they are not errors any more.* In a built image the prelude
and the compiler are compiled, and both forms above work — from compiled code,
with the interpreter nowhere in the loop.

---

**References.** Queinnec, *Lisp in Small Pieces*, chapters 6 and 7, for a
compiler to a bytecode machine written in the language it compiles, which is
this chapter's exact shape done properly. SICP §5.5 for a compiler for an
explicit-control machine, and §4.1.7 for the idea it rests on. Appel, *Modern
Compiler Implementation* (1998), chapter 1, for how much of a compiler is a
dispatch on the shape of a form — a thing that looks accidental in one
compiler and obviously structural across three.
