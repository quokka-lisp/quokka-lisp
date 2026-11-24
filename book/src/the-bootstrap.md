# The thing that builds itself

There is one idea in this project that is more powerful than all the others,
and it is easy to walk past. It is worth stopping on, because it explains the
whole shape of the work: why a deliberately slow interpreter is built first,
why the compiler is written in the language it compiles, and why at a certain
point the Rust nearly stops growing.

The idea is **bootstrapping**.

## The problem

Quokka's compiler will be written in Lisp.

Read that again, because it should bother you. A compiler turns Lisp into
something a machine can run. It is written in Lisp. So to run the compiler, you
need something that can run Lisp, which is the thing the compiler was supposed
to provide.

```text
   to run Lisp fast   you need   a compiler
   to run the compiler you need  something that runs Lisp
```

This is the oldest circle in computing, and there is a standard way out of it.

## Why not write the compiler in Rust?

Rust is already there, it already runs, and a compiler is just a program. So
why not? There are four reasons, and the last is the one that matters.

It breaks rule 1. Rust provides what hardware and microcode once did, and
anything a user might want to change belongs in Lisp. A compiler is *exactly*
the thing people want to change: a new optimisation, a different code
generator, support for a new special operator.

It breaks the budget. The Rust core is meant to stay small enough that one
person can read all of it, and a serious compiler is several thousand lines on
its own.

You would write it twice. If the goal is ever a compiler in Lisp, and for a
Lisp machine it must be, then a Rust compiler is scaffolding you throw away.

And the real reason: a system with a Rust compiler cannot improve itself. Every
optimisation would need a Rust programmer, a rebuild and a restart. A system
whose compiler is Lisp can be handed a better compiler *while it is running*,
recompile itself with it, and be faster afterwards, without anything outside it
being involved. That is the difference between a program and a world.

## Three executors, and only one of them is clever

Here is the division that makes it work. Quokka ends up with three pieces that
deal in Lisp, and it matters enormously which is which.

| | What it does | Language | Clever? |
| --- | --- | --- | --- |
| **Tree-walking interpreter** | reads a form and does it, directly | Rust | no |
| **Bytecode VM** | executes a stream of simple instructions | Rust | no |
| **Compiler** | turns Lisp into those instructions | **Lisp** | **yes** |

Two of the three are in Rust, and both of them are dumb on purpose. They
execute, and they make no decisions worth arguing about. A tree-walker looks at
a form and does the obvious thing; a bytecode VM looks at an instruction and
does the obvious thing. Neither needs to be smart, and both need to be fast and
small and correct.

All the judgement lives in the compiler: which representation to choose, what
to inline, how to lay out a frame, when a type check can be skipped. Judgement
is exactly what changes over time, and exactly what someone might want to
change for their own reasons. So the mechanical parts are Rust and the part
that requires taste is Lisp.

## The way out of the circle

You break a circle by building something crude by hand, using it to build
something better, and then not needing the crude thing any more.

A blacksmith needs tools to make a forge and a forge to make tools, so the
first forge is built badly, by hand, with whatever is available. It is good
enough to make better tools. Those tools make a proper forge. The crude one
gets left in the corner rather than thrown away, because one day you may need
to start again from nothing.

Quokka's crude forge is the **tree-walking interpreter**, which is everything
Part I and Part II of this book build.

```text
  1.  tree-walking interpreter, in Rust          ← slow, simple, correct
            │  runs
            ▼
  2.  the compiler, as Lisp source               ← slowly. Once.
            │  compiles its own source
            ▼
  3.  the compiler, as bytecode                  ← now runs on the fast VM
            │  saved as an image
            ▼
  4.  the compiler compiles itself again         ← this time at full speed
            │
            ▼
  5.  is the output identical to step 3?         ← the self-hosting test
```

Step 2 is the strange one. The interpreter, which is perhaps a hundred times
slower than compiled code, reads the compiler's source and executes it one cons
cell at a time, while that compiler slowly emits bytecode for itself. It is
absurdly slow and it does not matter at all, because **it happens once**. After
step 3 the compiler exists as bytecode and the slow path is never needed again.

## The fixpoint

Step 5 deserves attention, because it is a subtle and beautiful check.

After step 3 there are two compilers that ought to be the same program: the one
running interpreted from source, and the one running as bytecode. Have the
second compile the compiler's source again, and compare its output to what the
first produced.

If they match byte for byte, the system is **self-hosting**. The compiler
produces a compiler that produces the same compiler, which is a fixed point.

What does a mismatch mean? Something about compiling changed its own meaning.
The compiler produced different code depending on which compiler compiled *it*,
which is a bug no ordinary test could find, because both outputs would run and
both might look correct. It is the one check only a self-hosting system can
perform, and it is phase 2's gate.

*Later: it passes. Chapter 42 tells how it was measured, and what "byte for
byte" had to mean for a file that records its own save count.*

## What changes once it passes

This is the part worth being clear about, because it is the payoff for all of
it.

The Rust nearly stops growing. Every new feature of the *language* is Lisp from
here: every library function, every macro, every optimisation. The VM is close
to its final shape.

The system can improve itself. Write a better compiler, in Lisp, inside the
running image. Recompile. Everything is faster now, *including the compiler
that did it*, so the next improvement is cheaper than the last.

Development moves inside. The editor, the debugger and the window system, all
the things in `docs/design.md`'s later phases, are Lisp, written in a system
that is running while you write them. You do not restart Quokka to change
Quokka. That is what "one live Lisp world" means, and it is only possible
because the compiler is on the inside.

And the crude forge is kept. The tree-walking interpreter stays, for two
reasons: it is a debugging fallback, and it means `cargo build` is always
enough to rebuild the whole world from source. Self-hosting systems that delete
their bootstrap can only be built by a previous copy of themselves, which is a
genuinely unpleasant position to be in.

*Later: this section said the Rust would stop growing, not slow, and that was
too strong. It did stop growing for new language features, exactly as promised:
`setf` expanders, the last special operators, sequences, `format`, the package
system, bit fields and the small functions together cost under a hundred lines
of Rust, and several cost none. What kept needing Rust was new ways of
**representing** things, which live below the primitive table by rule 1:
bignums, double floats, arrays and bit vectors, hash keys without contents,
and structures as objects of their own account for some fifteen hundred lines
between them. The budget went from fifteen thousand lines to 15,713, raised
three times on purpose, and became a guide rather than a ceiling.*

## This is not exotic

Almost every serious language does this, and it is worth knowing so the
technique does not look like a stunt.

GCC is written in C and compiles itself. SBCL, the Common Lisp used in this
book as a reference, is written in Common Lisp and compiles itself. The Rust
compiler is written in Rust. Every one of them needed a first version built by
something else, and every one of them stopped needing it.

The first Lisp interpreter was bootstrapped in 1960 by hand-translating a
function called `eval` into IBM 704 machine code, which is as crude a forge as
has ever been built.

## Trusting a compiler that compiled itself

Ken Thompson's Turing Award lecture, *Reflections on Trusting Trust*, points
out something uncomfortable about this arrangement. A compiler that compiles
itself can carry a change that reproduces itself in every later generation
while appearing nowhere in the source. Remove it from the source and the
compiled compiler will quietly put it back.

The fixpoint test in step 5 does not protect against that. A self-reproducing
change makes the outputs match perfectly, which is the whole trick. The known
defence is to compile the source with a *different* compiler and compare, which
is why a reference implementation like SBCL is worth having around for more
reasons than testing.

It is not a practical worry for a hobby Lisp, but it is the shadow of the
technique, and knowing it is there is part of understanding what self-hosting
means.

## Where this leaves the rest of the book

Everything in Parts I and II, meaning tagged values, a garbage collector, a
reader and a tree-walking evaluator, is the crude forge. None of it is the
final system and all of it has to be correct, because the compiler will be run
on it.

Phase 1 adds the bytecode VM, still in Rust, still deliberately dumb. Phase 2
is the sequence above. After that, this book changes subject: it stops being
about Rust.
