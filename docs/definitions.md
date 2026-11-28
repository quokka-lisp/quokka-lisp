# Definitions that keep their source: design

*Status: built, 27 Nov 2026 (#55), in three sub-steps, book chapters 84 and 85. Phase 3, step C.*

This document says how a Quokka world comes to know its own definitions,
closely enough to build from. It is the first concrete layer of what
`docs/design.md` calls *the world is a database of definitions* (#126): the
environment of phases 4 to 6 will browse, version, commit and write out
these definitions, and phase 3's debugger needs them to fix a function from
inside an error.

## What is missing today

A compiled function is bytecode and constants. When `defun` is evaluated,
its form is compiled, the function is installed, and the form is dropped.
Three things then cannot be done:

1. **Show or edit a definition typed into the world.** Its text is gone.
   `function-lambda-expression` always answers "unknown".
2. **Find a definition from a function.** A debugger stopped in a frame has
   the frame's function, and from it can reach nothing but the name the
   function was compiled under, which Lisp cannot even read.
3. **Recompile what a changed macro reaches.** A macro is expanded when its
   caller is compiled; redefine it and every caller keeps the old expansion,
   and nothing records which callers those are.

## Decisions

The four questions of #55, settled with the environment's decisions on #126
in hand.

1. **A definition keeps its form, its source text, and where it came from.**
   The form, so that the world can recompile it without reading anything
   again. The text, a stretch of the file or of what was typed, with the
   comments above the definition, because #126 decided that a world writes
   its definitions out byte for byte and the comments that explain why are
   part of them; the reader throws comments away, so the text is cut from
   the input around the form, not printed from it. And the file and
   position it came from, when it came from a file. Rejected: a location
   alone, as SBCL keeps, since it breaks when the file changes and a
   definition typed into the world has none; the form alone, since it has
   lost the comments.
2. **A definition is a Lisp object, in a table of the world keyed by kind
   and name**, and a function finds its definition through its name, which a
   new primitive, `%function-name`, reads from the function object. Kinds
   are function, macro, generic function, method, variable, constant,
   class, structure, condition, type, setf expander, compiler macro, symbol
   macro, method combination and package, from eighteen defining operators;
   a program's own `def...` macro is not recorded, since nothing says what
   it defines. Rejected: a slot on the
   `Function` object, which needs Rust, the image format and the verifier,
   and gives nothing a name does not, except for a lambda inside a
   definition, which step F (the debugger) can name after the definition
   that holds it.
3. **Cross-reference and recompilation come after, in their own
   sub-steps.** The compiler records, for each top-level form, the global
   functions it calls and the macros it expands, and the definition keeps
   them: `who-calls` and `who-macroexpands`, SBCL's `sb-introspect` names.
   Redefining a macro marks the definitions that expanded it stale, and
   recompiling a definition, or every stale one, compiles its kept form
   again. Staleness is marked, not acted on at once: Genera's *Compile
   Changed Definitions* is a command, and a world in the middle of changing
   several macros should not recompile on each.
4. **A redefinition keeps the definition it replaces, as a version.** #126
   gives definitions versions and a history; step C keeps the list, and
   phase 4 adds commits and rolling back.

Two boundaries follow from where definitions are recorded. They are
recorded where a top-level form is evaluated: by `load`, by the host's
read-eval-print loop, and by `eval` given a defining form, which knows the
form but has no text. And **the system's own definitions are not recorded
in this step**: the cold build loads the library with the Rust reader, not
`load`, and `%rebuild-world` reads files the system keeps; recording them
would put the 850 KB of the library's and the compiler's source in every
image, as writable strings of a word per character, several times the
image's own 1.9 MB, before anything uses it. Phase 4's file-in was to record them and did not; how the system's own definitions get their source is #151. The plan then was that it records them
when the environment needs to browse the system itself.

## Limits, known and accepted

- A definition is recorded when its top-level form returns. The
  definitions inside a top-level `progn`, or inside what a program's own
  macro expands into, are recorded one by one as each returns, with what
  each refers to (#184); they keep no text of their own. A macro used as a
  `setf` place is noted by `setf`, which expands it, since the compiler
  never sees it.
- The names are exported from `QUOKKA-DEBUG`: `find-definition`,
  `all-definitions`, the `definition-...` readers, `who-calls`,
  `who-macroexpands`, `stale-definitions`, `recompile-definition` and
  `recompile-stale-definitions`.
- A form evaluated again with the same form and text is no new version,
  unless the definition was stale (#185). Recompiling what is stale does
  macros first, until none is stale, then everything else.
- Text is cut by line comments. A `#|...|#` block comment, or a form a `#+`
  reader conditional skipped, before a definition becomes part of its text.
- The host loop is handed what a person types one line at a time, so a
  comment typed on the line above a definition is not part of its text.
- Definitions and their versions are kept for ever, as #126 decided (only
  pruning history, which asks first, drops old versions). A program that
  `eval`s defining forms in a loop, or a test runner that evaluates
  thousands, grows the table without bound.

## Order of work

1. **Definitions keep their source**: the definition object and the table;
   `load`, the host loop and `eval` record defining forms with their text,
   file and position; versions; `%function-name`; `function-lambda-expression`
   answers with a definition's lambda expression.
2. **Cross-reference**: the compiler records calls and macro expansions;
   `who-calls`, `who-macroexpands`; redefining a macro marks its users stale.
3. **Recompilation**: recompile a definition from its form, and every stale
   one.

Each sub-step is a commit, reviewed and merged on its own.
