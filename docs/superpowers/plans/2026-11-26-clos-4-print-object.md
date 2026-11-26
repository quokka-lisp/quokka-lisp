# CLOS sub-step 4 · `print-object`, `describe`, `with-slots` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The printer prints instances, structures and conditions through the generic function `print-object`; `describe` calls `describe-object`; `print-unreadable-object` works inside methods; `with-slots` and `with-accessors` exist; methods on structure classes dispatch and print.

**Architecture:** Lisp only. `print-object` is defined at the end of `printer.lisp` (which already loads after `generic-functions.lisp`), with methods on `t` (the printer's own code), `standard-object`, `structure-object`, and the metaobject classes. `%print-plain` calls it only for instances, structures and conditions, and for anything else only when the flag `%print-object-on-built-in` is true, which `add-method` sets for a `print-object` method on a built-in class. A method's nested `prin1`/`format` continues the printout it is part of: same depth for `*print-level*`, same `*print-circle*` table, and the circle pass runs the method into a discarded stream to see what it prints — SBCL's behaviour, measured. `describe` and its methods go in a new `describe.lisp` after `format.lisp`; `with-slots`/`with-accessors` at the end of `clos.lisp`.

**Tech Stack:** tier-1 Lisp; differential tests against SBCL 2.6.10; ansi-test.

**Spec:** `docs/clos.md` — *The printer, `describe`, and speed*, *Order of work* item 4; issue #124.

## Global Constraints

- No Rust. The compiler never prints; the fixpoint holds unchanged except for the rebuild's `(:FORMS … :REPLACED …)` counts.
- **Printing a cons, symbol, number, string, character, array, function or hash table costs what it costs now** while no program has a `print-object` method on a built-in class: one more global read in `%print-plain`, nothing else. Check with ansi-test's step budgets and the warm rebuild's.
- `printer.lisp` and `describe.lisp` load before `eval.lisp`: no `fdefinition` (use `%defined-function`), no `special-operator-p`, no `lambda-list-keywords`.
- `clos.lisp` defines no public generic-function name as a plain function.
- Every `%` name shares package `QUOKKA`: search `lisp/` before adding one. A `defvar` goes above the first function that binds it.
- Differential files never print an address (SBCL's `{…}`) and never bind a COMMON-LISP symbol. `describe`'s text is not tested against SBCL (CLHS leaves it to the implementation).
- `./scripts/check.sh` green before the commit; no attribution in commit messages.

## Measured in SBCL 2.6.10 (the behaviour to match)

- A method's nested `(format s "~S" x)` prints X at the depth the instance was printed at: with `*print-level*` 2, `(list foo)` whose method prints `'(1 (2 (3)))` gives `(#<FOO (1 #)>)`. The instance itself adds no level.
- `print-unreadable-object` is a level: with `*print-level*` 1, `(list foo)` whose method uses it gives `(#)`.
- Under `*print-circle*`, what a method prints is shared with the rest: `(#1=(1 2) #<FOO #1#>)`, and an instance holding itself prints `#1=#<FOO #1#>`.
- `print-unreadable-object` writes the type as `(write (type-of object))` (package prefix, `(INTEGER 0 4611686018427387903)` for 5) followed by a space whether or not a body follows (`#<PT >`); the identity is preceded by a space when there is a body or no type. Quokka has no address, so the identity's text is empty and only that space is written.
- `describe` returns no values; its text starts with the object, then `  [kind]`, then for an instance or structure `Slots with :INSTANCE allocation:` and one `  NAME = value` line per slot, `#<unbound slot>` for an unbound one.
- SBCL honours a `print-object` method on `cons` everywhere and one on `integer` inconsistently; CLHS 11.1.2.1.2 item 19 makes both undefined. Quokka honours them everywhere (the flag) and no differential file tests them.

## Review Focus

1. **A `print-object` method that calls `call-next-method`** on a standard object, a structure and a condition: the next method prints what the printer printed before this step.
2. **A structure with `:print-function` that `:include`s a structure with a user `print-object` method** (and the reverse): the most specific one wins, as in SBCL, because `defstruct`'s printer option becomes a method on its class.
3. **Printing readably** (`*print-readably*` true) an instance whose class has no method: `print-not-readable`, not `#<…>`.
4. **A method that prints its object again** (`(prin1 o s)` inside `print-object` for `o`'s own class) under `*print-circle*`: labelled, no endless loop; without `*print-circle*`: the program's own infinite recursion, as in SBCL (not tested).
5. **`with-slots` with `setq`, `setf` and `incf`** of a slot variable, and a `(var slot)` entry; `with-accessors` with `setf` through a writer.

---

## File structure

| File | Responsibility |
| --- | --- |
| `lisp/library/printer.lisp` (modify) | `%print-plain` routes to `print-object`; `%print-built-in` is what the `t` method runs; nesting (`%print-nesting`) and the circle pass through methods (`%noting`); `print-unreadable-object` by SBCL's rules with `type-of`; `defgeneric print-object` and its methods; `%note-structure-defined` redefined to make a structure's printer option a method; `%instance-text` removed |
| `lisp/library/generic-functions.lisp` (modify) | `defvar %print-object-on-built-in`; `add-method` sets it |
| `lisp/library/describe.lisp` (create) | `describe`, `defgeneric describe-object`, methods on `t`, `standard-object`, `structure-object`, `symbol`, `class`, `generic-function`, `array`, `hash-table` |
| `lisp/library/clos.lisp` (modify) | `with-slots`, `with-accessors` |
| `crates/quokka-bootstrap/src/boot.rs`, `lisp/library/rebuild.lisp` | `describe.lisp` after `format.lisp` |
| `tests/lisp/print-object.lisp`, `.expected` (create) | differential tests |
| `crates/quokka-bootstrap/tests/differential.rs` (modify) | `print_object => "print-object"`; Quokka-only tests of default `#<…>` text and `describe`; rebuild counts |
| `tests/ansi/deferred.lisp`, `baseline.txt`, `docs/ansi-test.md` | `print-object`, `describe-object`, `describe`, `with-slots`, `with-accessors` leave `tier-2:clos` |
| docs, `CLAUDE.md`, book chapter 75 | as before |

---

### Task 1: `print-object` in the printer

**Interfaces — produces:**
- `(defgeneric print-object (object stream))`; methods: `t` → `(%print-built-in object stream %print-nesting)`; `standard-object` → `#<NAME>` (no trailing space: Quokka writes no identity); `class` → `#<METACLASS NAME>`; `generic-function` → `#<STANDARD-GENERIC-FUNCTION NAME>`; `method` → `#<STANDARD-METHOD NAME (SPECIALIZER…)>` plus qualifiers; `slot-definition` → `#<CLASS NAME>`; `structure-object` → `#S(…)` (`%print-structure-slots`). The type names are written with `prin1`, as now.
- `%print-nesting`: a `defvar`, nil outside a method; while `%print-plain` runs a `print-object` method it is the depth the object was printed at. `%print-top` entered while it is an integer prints at that depth into the given stream with the current `%circle-table`, and binds it to nil underneath.
- `%noting`: a `defvar`, nil, or t while the circle pass runs a method. Pass 1 (`%note-shared`) runs `print-object` into a fresh string stream for every object `%print-plain` would send to it (except structures with no method of their own, whose slots it walks as now), with `%circle-table` bound to the table and `%noting` t; then `%print-object` notes instead of printing. The test sits inside the existing `%circle-table` branch, so it costs nothing with `*print-circle*` false.
- `(%print-object-p object)` → true when `%print-plain` sends OBJECT to `print-object`: `%print-object-on-built-in`, or an instance (`%instance-wrapper`), or a structure that is not one of the library's unreadable ones, or a condition (`%conditionp`).
- `print-unreadable-object` per *Measured*; `%type-name` deleted in favour of `type-of`. The body is a level: `#` when the depth is at `*print-level*`.
- `%note-structure-defined` (redefined here; the `structures.lisp` one stays for structures defined before the printer): when the layout has a printer of its own, add a `print-object` method on the structure class that calls it (`:print-object` with object and stream, `:print-function` with object, stream and depth), remembered under the name's `%printer-method` property; a redefinition without a printer removes it. The `structure-object` method no longer looks at `%structure-printer`.
- `add-method` sets `%print-object-on-built-in` when the generic function is named `print-object` and the method's first specializer is a class of metaclass `built-in-class` other than `t`, or an eql specializer whose object `%print-object-p` would not already route.

- [ ] **Step 1: Write the failing differential file** `tests/lisp/print-object.lisp`: a class with a `print-object` method using `print-unreadable-object` with `:type t` and a body (`#<POINT 1 2>`); `prin1-to-string`, `princ-to-string`, `format ~A`/`~S`, `write-to-string` of it, alone and in a list and a vector; a method using `format` directly; `call-next-method` from a method on a subclass wrapping the parent's text; `*print-level*` 1 and 2 with nested lists inside a method, with and without `print-unreadable-object` (the *Measured* outputs); `*print-circle*` with a list shared between a method's output and the outer list, and an instance holding itself (Review Focus 4); `*print-readably*` on an instance with a method that uses `print-unreadable-object` and on one with no method, in `handler-case` returning the condition type (Review Focus 3); a structure with a `print-object` method; a structure with `:print-function` and one with `:print-object`; a structure `:include`-ing one with a method, with and without its own `:print-function` (Review Focus 2); `call-next-method` from a structure method giving `#S(…)` (Review Focus 1); `print-unreadable-object` on a fixnum, a string and a structure with `:type` t/nil, `:identity` nil, body present/absent (the four outputs with no identity); `print-unreadable-object` returning nil; a method on a `defstruct` type found by `find-method`.
- [ ] **Step 2: SBCL baseline; see Quokka fail** — `sbcl --script scripts/oracle.lisp tests/lisp/print-object.lisp > tests/lisp/print-object.expected`, add `print_object => "print-object"` to `tier1_cases!`, run `cargo test -p quokka-bootstrap --test differential print_object`.
- [ ] **Step 3: Implement** as in *Interfaces*.
- [ ] **Step 4: The file passes; all tier-1 files pass** (`cargo test -p quokka-bootstrap --test differential`).
- [ ] **Step 5: Quokka-only test** in `differential.rs`, `print_object_prints_metaobjects`: `(prin1-to-string (make-instance 'point))` → `"#<POINT>"`, a class → `"#<STANDARD-CLASS POINT>"`, a generic function → `"#<STANDARD-GENERIC-FUNCTION AREA>"`, a method → `"#<STANDARD-METHOD AREA (CIRCLE)>"`, a condition prints as before, and after `(defmethod print-object ((x cons) s) (write-string "C" s))` `(prin1-to-string (list 1))` → `"C"` (the flag).

### Task 2: `describe` and `describe-object`

**Interfaces — produces:** `(describe object &optional stream)` → no values; STREAM a designator (nil → `*standard-output*`, t → `*terminal-io*`); `fresh-line`, `(describe-object object stream)`, `fresh-line`. `(defgeneric describe-object (object stream))`. Methods, each starting with the object `prin1`ed and `  [kind]` (kind lower case: `standard-object`, `structure-object`, `condition`, `list` for a cons, otherwise the name of `class-of`): `t` that alone; `standard-object` and `structure-object` add a blank line, `Slots with :INSTANCE allocation:` and `  NAME` padded to 33 columns, `= ` and the value (`#<unbound slot>`), and `:CLASS` slots likewise under their own heading; `symbol` adds what the name names (`a special variable` with its value, `a function`/`a macro`/`a generic function`, `the CLASS-KIND NAME` with `Class precedence-list:`, `Direct superclasses:`, `Direct subclasses:` or `No subclasses.`); `class` adds those class lines; `generic-function` adds `Lambda-list:` and `Methods:`; `array` adds `Element-type:` and `Length:` (vector) or `Dimensions:`; `hash-table` adds `Test:` and `Count:`.

- [ ] **Step 1: Write failing Quokka-only tests** in `differential.rs`, `describe_calls_describe_object`: `(with-output-to-string (s) (describe (make-instance 'point :x 1) s))` contains `"Slots with :INSTANCE allocation:"` and a line starting `"  X"` and ending `"= 1"`; an unbound slot shows `#<unbound slot>`; `(multiple-value-list (describe 5 s))` is nil; a user `describe-object` method's text replaces the default; a structure lists its slots; a symbol naming a class says `names the standard-class`. Append to `print-object.lisp`: `(multiple-value-list (describe …))` inside `with-output-to-string` and the user method's exact text (both agree with SBCL).
- [ ] **Step 2: See them fail.**
- [ ] **Step 3: Implement `lisp/library/describe.lisp`**, listed in `boot.rs` and `%library-files` after `format.lisp`.
- [ ] **Step 4: Pass; all tier-1 files pass.**

### Task 3: `with-slots`, `with-accessors`

**Interfaces — produces:** `(with-slots (entry*) instance-form declaration* form*)`, ENTRY `name` or `(variable name)` → `(let ((g instance-form)) (symbol-macrolet ((variable (slot-value g 'name))…) declaration* form*))`; `(with-accessors ((variable accessor)…) instance-form declaration* form*)` → `(accessor g)`. The instance form is evaluated once.

- [ ] **Step 1: Append failing forms** to `print-object.lisp`: reading slots; `setq`, `setf`, `incf` of a slot variable and the instance seeing it (Review Focus 5); `(var slot)` renaming; `with-accessors` reading and `setf` through an `:accessor`; the instance form evaluated once (a counter); a declaration in the body; nested `with-slots` on two instances; `with-slots` on a structure (SBCL allows `slot-value` on structures).
- [ ] **Step 2: Baseline; see them fail.**
- [ ] **Step 3: Implement at the end of `clos.lisp`.**
- [ ] **Step 4: Pass; all tier-1 files pass.**

### Task 4: ansi-test, the rebuild, documentation, commit

- [ ] **Step 1:** remove `print-object`, `describe-object`, `describe`, `with-slots`, `with-accessors` from `tier-2:clos` in `tests/ansi/deferred.lisp`; full run `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` in the background; fix every newly failing test or explain it by a deferred feature; repeat until no plain failure, escape or timeout. Compare the step-budget margin of the printer group before and after (the cost constraint).
- [ ] **Step 2:** extend `a_rebuild_keeps_classes_and_their_instances`: a user `print-object` method defined before the rebuild still prints after it; update the rebuild counts.
- [ ] **Step 3:** `docs/design.md` decision entry (who goes through `print-object` and why; the flag; nesting continues the printout; the circle pass runs methods; `defstruct` printer options as methods; no identity text), `docs/clos.md` if anything moved, `CLAUDE.md`, `docs/manual.md`, book chapter 75 `book/src/part-7-objects/printing-objects.md` (`book-voice`), `SUMMARY.md`, glossary (`print-object`, `describe`, symbol macro use in `with-slots`), `what-has-to-be-built.md`.
- [ ] **Step 4:** `./scripts/check.sh` green; commit on branch `clos-print-object`; fresh reviewer on the branch; fix its critical and important findings with tests; commit; tick sub-step 4 on #124 with a comment.
