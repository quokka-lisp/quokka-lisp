# The compiler

Written in Lisp, read by the phase 0 interpreter, and tier 0 only (rule 5).

It is **not** part of the prelude. `lisp/prelude.lisp` is interpreted at every
boot because it defines `defun` and nothing works without it; the compiler is
interpreted once, to build an image, and after that it is compiled code inside
that image. Different lifecycles, different files. See issue #31.

Files appear here as they are written, rather than as empty structure:

| | |
| --- | --- |
| `compile.lisp` | forms to a list of instructions |

## The boundary

This side emits instructions as lists:

```lisp
((LREF 0 0) (FIX 0) (PRIM 3 2) (JMP-NIL else) (FIX 1) (RET) (:label else) ...)
```

and `%assemble` — a primitive, in `crates/quokka-vm/src/code.rs` — resolves
the labels, encodes the operands, verifies the result and computes its
`max-stack`.

The opcode numbers, the operand widths and the stack effect of each
instruction therefore exist in exactly one place. Assembling in Lisp would put
them in two, with nothing checking they agree, which is what issue #30 was
about.

What is left here is everything with a decision in it: what `let` compiles to,
which calls are in tail position, when a primitive may be inlined, where a
`block` needs a marker. That division is rule 1 applied to the compiler.
