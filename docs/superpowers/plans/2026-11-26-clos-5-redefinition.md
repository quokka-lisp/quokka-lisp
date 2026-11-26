# CLOS sub-step 5 · Redefinition — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Instances survive their class changing under them: a redefined class's instances are updated through `update-instance-for-redefined-class` (CLHS 4.3.6), `make-instances-obsolete` forces that, `change-class` gives an instance another class through `update-instance-for-different-class` (CLHS 7.2), and shared slots are initialised when their class is defined, as SBCL does.

**Architecture:** Lisp only. A wrapper that a class stops using is marked obsolete; its state slot records the old class's shared slots, which the update needs (a slot shared in the old class and local in the new keeps its value). `%ensure-current-instance`, which already brings an instance up to date when a slot is touched, computes the added and discarded slots and the property list and calls the new generic function `update-instance-for-redefined-class`. `change-class` copies the instance into a "previous" instance of the old class, gives the instance the new class's wrapper and slots, and calls `update-instance-for-different-class` with both. The public names are generic functions in `generic-functions.lisp`; `clos.lisp` keeps `%` workers.

**Tech Stack:** tier-1 Lisp; differential tests against SBCL 2.6.10; ansi-test.

**Spec:** `docs/clos.md` — *Objects in the heap* (obsolete wrappers), *Redefinition*, *Order of work* item 5; issue #124.

## Global Constraints

- No Rust. The fixpoint holds unchanged except for the rebuild's `(:FORMS … :REPLACED …)` counts; a library class whose instance slots are unchanged keeps its wrapper across a warm rebuild, so nothing goes obsolete merely because the world rebuilt itself (`a_rebuild_keeps_classes_and_their_instances`).
- `clos.lisp` defines no public generic-function name as a plain function: `change-class`, `make-instances-obsolete`, `update-instance-for-redefined-class`, `update-instance-for-different-class` are generic functions in `generic-functions.lisp`.
- Every `%` name shares package `QUOKKA`: search `lisp/` before adding one.
- Differential files never print an address, never bind a COMMON-LISP symbol, and never depend on *when* an obsolete instance is updated before a slot is touched (CLHS 4.3.6 leaves that to the implementation: SBCL also updates on a generic function call, Quokka only when a slot is touched).
- `./scripts/check.sh` green before the commit; no attribution in commit messages.

## Measured in SBCL 2.6.10

- Redefining a class with the same local slots (an initform changed) makes nothing obsolete: no `update-instance-for-redefined-class` call.
- Removing `y` (bound to 2) and adding `z`: the update calls `(update-instance-for-redefined-class i '(z) '(y) '(y 2))` with no initargs, and `z` gets its initform.
- `make-instances-obsolete` returns its argument (the symbol when given a symbol); the next update has no added or discarded slots.
- A slot shared in the old definition and local in the new keeps its value; a slot local in the old and shared in the new takes the shared value (`:q2`), the local value going into the property list.
- A shared slot's initform is evaluated when its class is defined, not when the first instance is made; redefining keeps a shared slot's value; a subclass that defines the slot again has its own.
- `change-class` returns the instance (`eq`); slots local in both keep their values, a slot shared in the old class and local in the new keeps its value; in an `update-instance-for-different-class :before` method the previous instance still has the old class and values, and the new slots are unbound; then initargs fill slots (`:w 70`). An invalid initarg is a `program-error` (SBCL's `initarg-error`). `change-class` of a non-instance is `no-applicable-method`'s error; from a funcallable instance to a standard class is an `error`.

## Review Focus

1. **An instance not touched across two redefinitions**: the update compares its own old wrapper with the current one, once; discarded and added slots are those between the two.
2. **An obsolete instance passed to `change-class`**: brought up to date first, then changed.
3. **`update-instance-for-redefined-class` or `update-instance-for-different-class` methods that read slots of the instance** (or the previous one) while it is being updated: no endless update, no stale slot vector.
4. **A subclass of the redefined class**: its instances are updated too, with the slots it inherits added or removed.
5. **The warm rebuild**: library classes keep their wrappers; a program's class with live instances survives a rebuild unchanged.

---

## File structure

| File | Responsibility |
| --- | --- |
| `lisp/library/clos.lisp` (modify) | wrapper state `(:obsolete . shared-cells)`; `%obsolete-wrapper` marks one; `%finalize-class` marks the replaced wrapper and initialises shared slots whose cell is unbound; `%ensure-current-instance` computes added/discarded/plist and calls `update-instance-for-redefined-class`; `%make-instances-obsolete`; `%change-class` |
| `lisp/library/generic-functions.lisp` (modify) | `defgeneric`/`defmethod` for `update-instance-for-redefined-class`, `update-instance-for-different-class`, `make-instances-obsolete` (`standard-class`, `funcallable-standard-class`, `symbol`), `change-class` (`standard-object` × `standard-class`, `funcallable-standard-object` × `funcallable-standard-class`, `t` × `symbol`) |
| `tests/lisp/redefinition.lisp`, `.expected` (create) | differential tests |
| `crates/quokka-bootstrap/tests/differential.rs` (modify) | `redefinition => "redefinition"`; rebuild test extended; rebuild counts |
| `tests/ansi/deferred.lisp`, `baseline.txt`, `docs/ansi-test.md` | `make-instances-obsolete`, `change-class`, `update-instance-for-different-class`, `update-instance-for-redefined-class` leave `tier-2:clos` |
| docs, `CLAUDE.md`, book chapter 76 `book/src/part-7-objects/76-changing-classes.md` | as before |

---

### Task 1: Obsolete instances and `update-instance-for-redefined-class`

**Interfaces — produces:**
- A wrapper's state (slot 2): nil while current; `(:obsolete . cells)` once replaced, CELLS the old class's shared slots as `(name . cell)`.
- `(%obsolete-wrapper wrapper class)` marks WRAPPER obsolete with CLASS's current shared cells. Called by `%finalize-class` when it replaces a wrapper (names differ) and by `%make-instances-obsolete`.
- `%finalize-class`, after computing slots: every effective `:class` slot whose cell belongs to this class, is unbound and has an initfunction gets its initform's value.
- `%ensure-current-instance`: for each name in the current wrapper, its value from the old slot vector if the old wrapper had it, from the old shared cell if the old state lists it, otherwise unbound and the name is *added*; *discarded* are the old wrapper's names the new one lacks, in order; the *plist* holds each discarded name and value that was bound. The new wrapper and vector are installed first, then `(update-instance-for-redefined-class instance added discarded plist)`.
- `(defgeneric update-instance-for-redefined-class (instance added-slots discarded-slots property-list &rest initargs &key &allow-other-keys))`, standard method on `standard-object`: checks INITARGS against itself and `shared-initialize` (CLHS 7.1.2), then `(apply #'shared-initialize instance added-slots initargs)`.
- `(make-instances-obsolete class)` → its argument; methods on `standard-class`, `funcallable-standard-class` (`%make-instances-obsolete`: a new wrapper with the same names, the old one marked) and `symbol` (`find-class` first).

- [ ] **Step 1: Write the failing differential file** `tests/lisp/redefinition.lisp`: the *Measured* cases for redefinition — a redefinition with the same slots logging nothing; removing a bound slot and adding one with an initform, the log read after a `slot-value`; an unbound discarded slot not in the plist; `make-instances-obsolete` by symbol and by class, its return values, and the empty update after it; shared↔local transitions; shared slot initforms evaluated at `defclass` (`(slot-value (allocate-instance (find-class 'e)) 'a)`), kept on redefinition, a subclass redefining one; a subclass instance updated when its superclass loses a slot (Review Focus 4); two redefinitions before the first touch (Review Focus 1); a `:after` method on `update-instance-for-redefined-class` that reads a slot of the instance (Review Focus 3); an accessor still working after its slot moved place.
- [ ] **Step 2: SBCL baseline; add `redefinition => "redefinition"` to `tier1_cases!`; see Quokka fail.**
- [ ] **Step 3: Implement** as in *Interfaces*.
- [ ] **Step 4: The file passes; all tier-1 files pass.**

### Task 2: `change-class` and `update-instance-for-different-class`

**Interfaces — produces:**
- `(defgeneric change-class (instance new-class &rest initargs &key &allow-other-keys))`; methods: `standard-object` × `standard-class` and `funcallable-standard-object` × `funcallable-standard-class` → `(%change-class instance new-class initargs)`; `t` × `symbol` → with `(find-class new-class)`. A standard instance to a funcallable class or the reverse: `error` (different heap types, as SBCL refuses).
- `%change-class`: brings INSTANCE up to date; makes PREVIOUS, an instance of the old wrapper with a copy of the old slot vector; builds the new vector (local in both: copied; shared in the old class and local in the new: the shared value); installs the new wrapper and vector; calls `(apply #'update-instance-for-different-class previous instance initargs)`; returns INSTANCE.
- `(defgeneric update-instance-for-different-class (previous current &rest initargs &key &allow-other-keys))`, standard method on `standard-object` × `standard-object`: checks INITARGS against itself and `shared-initialize` on CURRENT's class, then `shared-initialize` CURRENT with the added slots (local in the new class, with no slot of that name in the old) and INITARGS.

- [ ] **Step 1: Append failing forms**: the *Measured* `change-class` cases (identity, retained, shared→local, the `:before` method's view of both instances, initargs filling slots); an invalid initarg (`program-error`) and the instance's class afterwards; `change-class` of 5, of a structure (`error`); to the same class; an obsolete instance changed (Review Focus 2); a method on the new class dispatching afterwards; `change-class` by class object.
- [ ] **Step 2: Baseline; see them fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Pass; all tier-1 files pass.**

### Task 3: ansi-test, the rebuild, documentation, commit

- [ ] **Step 1:** `tests/ansi/deferred.lisp`: the four names leave `tier-2:clos`; full run `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` in the background until no plain failure, escape or timeout.
- [ ] **Step 2:** extend `a_rebuild_keeps_classes_and_their_instances` (Review Focus 5): an instance whose class was redefined before the rebuild and not touched is updated correctly after it; update the rebuild counts.
- [ ] **Step 3:** `docs/design.md` decision entry, `docs/clos.md` if anything moved, `CLAUDE.md`, `docs/manual.md` (tier-1 file count), book chapter 76 "Changing classes" (`book-voice`), `SUMMARY.md`, glossary (obsolete instance, `change-class`), `what-has-to-be-built.md`.
- [ ] **Step 4:** `./scripts/check.sh` green; commit on branch `clos-redefinition`; fresh reviewer on the branch; fix its critical and important findings with tests; commit; tick sub-step 5 on #124 with a comment.
