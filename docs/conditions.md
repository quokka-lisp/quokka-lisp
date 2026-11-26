# Conditions in Quokka: design

*Status: decided 27 Nov 2026 (#125), being built. Phase 3, step B.*

This document says how Quokka's condition system is built, and in what order,
closely enough that it can be built from. As for CLOS (`docs/clos.md`), a
part's decisions move into `docs/design.md` and its story into the book when
it is built; this file stays as the reference for the whole.

## What a condition system has to do

Today an error in Quokka can be caught, and nothing more. `handler-case`
leaves a record on the machine's stack, an error unwinds to the innermost
one, and the clause that wants the condition runs there, after everything
between the error and the record has been thrown away. That is enough for
`ignore-errors`, and for a compiler that wants to say which form it could
not compile.

Common Lisp asks for more (CLHS chapter 9). A handler established by
`handler-bind` is called *where the condition was signalled*, on top of the
frames that signalled it, and it may decline by returning. A warning is
signalled, printed and then the program carries on. A *restart* is a way out
that the code near the error offers and the code far from it chooses:
`(use-value 0)` from a handler three frames out tells the function that
failed to go on with 0. And an error nobody handles goes to the debugger,
which offers the restarts to a person. Phase 3's gate, fixing a function from
inside an error and retrying the frame, is built on exactly these.

## Decisions

The seven decisions argued on #125, taken as recommended:

1. **A condition is an instance of its condition class.** `define-condition`
   makes a class whose metaclass is `condition-class`; `make-condition`
   allocates an instance, a `StandardInstance` like any other (chapter 73),
   so `slot-value`, readers and `print-object` work on it. `condition` is not
   a subclass of `standard-object`, as in SBCL. The Rust `Condition` object
   stays for tier 0, which the interpreter and the cold build use, and a
   world with the library never makes one.
2. **Handlers live in a special variable**, `%handler-clusters`, bound by
   `handler-bind`, and `signal` is Lisp. The prelude's `handler-case` keeps
   its `HANDLER` records, because the compiler uses it and the fixpoint
   needs it to compile the same way; so `signal` walks the machine's record
   chain with one primitive, which yields the `HANDLER` records and the
   bindings of `%handler-clusters` in their true dynamic order.
3. **The machine's own errors are signalled through Lisp.** When a
   primitive or an instruction fails, the machine calls the library's
   `%signal-machine-error` with the condition's type and initargs, at the
   point of the error, the way an arithmetic primitive calls `%generic-+`.
4. **Restarts live in a special variable too**, `%restart-clusters`, and
   `restart-case` expands into `restart-bind` and a non-local exit, as
   SBCL's does.
5. **An error nobody handles** goes to `invoke-debugger`, which runs
   `*debugger-hook*`, and with no hook reports the condition on
   `*error-output*` and invokes the innermost `abort` restart; with none,
   the machine halts as it does today. The debugger itself is step F.
6. **Reports**: `define-condition`'s `:report` becomes what `print-object`
   prints when `*print-escape*` is false; `simple-condition`'s format
   control is formatted when the condition is reported.
7. **The interpreter is unchanged.** Tier 0 keeps its flat conditions, and
   the library replaces `error` and its kin in a compiled world, as it
   replaced `setf` and `gensym`.

## What the decisions left open, and how we settled it

**The library's own way of signalling.** About 240 calls in the library, and
every compiler error, are written `(error 'program-error "message" data...)`:
a type, a message that is not a format control, and data. SBCL refuses that
form (an odd-length initarg list). We keep it for the library: when the
datum is a symbol and the next argument is a string, the condition is made
of that type and the message and data go into two slots of class
`condition`, `%message` and `%data`, in package `QUOKKA`. SBCL's conditions
have internal slots of their own (`SB-KERNEL::CONTEXT` in a `type-error`),
so a program cannot rely on `condition` having none, and the names cannot
clash with a program's. Such a condition reports its message. The call sites
that ansi-test needs to see a `type-error`'s datum are converted to
`:datum`/`:expected-type` as the tests find them.

**The standard types' slots** are named in `QUOKKA` too (`datum`,
`expected-type`, `format-control`, ...), as SBCL's are in `SB-KERNEL`, so a
program reaches them only through the readers CLHS names.

**Condition types are classes in the type algebra.** `subtypep` (chapter 72)
kept condition types as a kind of object of their own, named by symbols.
Once a condition is an instance, a condition type is a class like any other,
and the `:condition` kind goes: its types become terms of the `:instance`
kind, and the machine's old condition objects, while they still exist, are
of their class by `class-of`.

**A `handler-case` clause tests its type at run time.** The compiler listed
a clause's subtypes from the prelude's lattice when it compiled the clause,
and took `error` to mean everything. Neither holds once a program can define
condition types and signal conditions that are not errors. A clause's test
becomes a call of `%handler-clause-p`, which the prelude defines over its
lattice and the library redefines as `typep`; `t` and `condition` still
compile to no test. Only `compile.lisp`'s code generation changes, so C1, C2
and C3 are still equal, and the library's macros expand the compiler's own
source exactly as the prelude's do.

**How `signal` reaches a `handler-case`.** The walk finds a `HANDLER`
record, `signal` asks which of its clauses wants the condition, and if one
does, calls the primitive `%unwind-to-handler` with the record's place and
the condition. The machine unwinds to that record only, running cleanups on
the way, and the compiled clause dispatch takes over as before.

**What a handler sees while it runs.** CLHS 9.1.4.1: while a handler runs,
only the handlers outside its cluster are active. A handler runs on top of
the stack, above everything between the signal and its cluster, so the walk
would find those again. `signal` binds `%handler-floor` around each handler
call to the place of the handler's cluster, and the walk, meeting that
binding, jumps below it. Handlers a handler establishes itself are above the
binding and stay visible.

**The REPL.** `%host-step` already catches every error with a
`handler-case` and returns it to the host, which prints it and reads the next
form. It keeps doing that in this step; an `abort` restart around each form
comes with the debugger (step F), where it has something to choose between.

## Order of work

1. **Condition objects.** Condition classes get CLHS's slots and readers;
   `define-condition`, `make-condition`; reports through `print-object`;
   condition types become classes in `typep` and `subtypep`. Conditions are
   still signalled the old way.
2. **`signal` in Lisp.** The walk primitive and `%unwind-to-handler`;
   `handler-bind`, `signal`, `error`, `%handler-clause-p`, and the library's
   `%condition-type` and `%condition-message` for compiled code that reads a
   condition.
3. **The machine's errors through Lisp** (Rust): `fail` calls
   `%signal-machine-error` with a type and initargs; host failures likewise.
4. **Restarts and the debugger's entry**: `restart-bind`, `restart-case`,
   the restart functions, `invoke-debugger`, `warn`, `cerror`, `assert`,
   `check-type`'s and `ccase`'s `store-value`, `break`,
   `*break-on-signals*`.
5. **ansi-test's `conditions` group** measured, and every test that does not
   pass explained by a deferred feature or by SBCL agreeing.

Each sub-step is a commit, reviewed and merged on its own, as CLOS's were.
