# CLOS sub-step 2 · Classes and instances — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every object has a class, and a program can define its own with `defclass` and make instances with `make-instance`: funcallable instances in Rust, the hand-built bootstrap of `standard-class`, `defclass`, initialisation, slot access, `class-of`, `find-class`, and class names as types.

**Architecture:** Two new heap types in Rust (a deviation from `docs/clos.md`, which made a standard object an `Instance`: every structure path reads an `Instance`'s slot 0 as a layout, and a header type of its own keeps those paths unchanged at no cost), `StandardInstance` (wrapper, slot vector) and `FuncallableInstance` (function, wrapper, slot vector), reached by eight constant-time primitives; the machine calls a funcallable instance's function. Everything else is Lisp in a new file `lisp/library/clos.lisp`, after `type-algebra.lisp`: class metaobjects are standard instances with a fixed slot layout, bootstrapped by hand in stages as AMOP's Closette does; `make-instance`, `initialize-instance`, `shared-initialize` and the slot functions are ordinary functions in this sub-step (sub-step 3 makes them generic). The type system learns classes as one more kind, `:instance`.

**Tech Stack:** Rust (`crates/quokka-vm`), tier-1 Lisp, differential tests against SBCL 2.6.10, ansi-test runner.

**Spec:** `docs/clos.md` — sections *Objects in the heap*, *The one piece of Rust*, *Classes and the bootstrap*, *Conditions in this step*, *The warm rebuild*, *Order of work* item 2; issue #124.

## Global Constraints

- Rust only where it must be (rule 1): allocation of the two heap types, their accessors, and calling a funcallable instance. Every primitive is O(1) (rule 9) and no argument may panic it (rule 10; `no_primitive_panics_on_any_argument` covers new primitives automatically).
- Heap references are offsets; the image format version goes from 7 to 8 (`crates/quokka-vm/src/image.rs`, `FORMAT_VERSION`); `Heap::verify` (`crates/quokka-vm/src/heap/verify.rs`) checks the new types' slot counts.
- WASM on every commit: `./scripts/check.sh` green (native, GC torture, wasip1, browser build) before the one commit.
- The compiler never calls anything in `clos.lisp`; the fixpoint must hold unchanged except for the rebuild's `(:FORMS … :REPLACED …)` counts.
- The warm rebuild reloads `clos.lisp` while its objects are alive: every class keeps its identity, a class whose slots did not change keeps its wrapper, and the bootstrap is safe to run twice (`docs/clos.md`, *The warm rebuild*).
- `clos.lisp` must not use `defstruct` for its own metaobjects (classes are standard instances), and must not define a compiler macro.
- Every `%` name shares package `QUOKKA` with the compiler: search `lisp/` before adding one.
- A `defvar` goes above the first function that binds the variable.
- Errors signal the condition type SBCL's does, where the differential file can see it.
- Behaviour matches SBCL (rule 6). No Claude attribution in the commit message.

## Review Focus

1. **Calling a funcallable instance with no function set**, or whose function is itself a funcallable instance: an error (`NotAFunction`), never a panic or an unbounded Rust recursion.
2. **An inconsistent class precedence list** — `(defclass a () ()) (defclass b (a) ()) (defclass c (a b) ())` — signals an error as SBCL does, and leaves no half-defined class behind.
3. **A forward-referenced superclass** — `(defclass d (not-yet) ())` then `(make-instance 'd)` — the `defclass` succeeds (CLHS allows it) and `make-instance` signals an error until `not-yet` is defined; defining it afterwards makes `d` usable.
4. **Redefining a class with identical slots during a warm rebuild** keeps the class object and its wrapper (`eq` before and after), so instances made before the rebuild still work.
5. **A slot named by a symbol from another package**, and two slots with the same name in a class and its superclass, merge into one effective slot (CLHS 7.5.3), with the most specific `:initform` and the union of `:initarg`s.

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/quokka-vm/src/heap.rs` (modify) | `ObjectType::StandardInstance = 14`, `ObjectType::FuncallableInstance = 15`, `from_u8` |
| `crates/quokka-vm/src/instance.rs` (create) | slot indices, constructors, predicates for the two types — one place that knows their layout |
| `crates/quokka-vm/src/primitives.rs` (modify) | eight primitives; `functionp` true of a funcallable instance |
| `crates/quokka-vm/src/vm.rs` (modify) | `call_value` replaces a funcallable instance by its function |
| `crates/quokka-vm/src/heap/verify.rs`, `image.rs`, `printer.rs`, `hash.rs` (modify) | slot counts, format 8, `#<instance>`/`#<funcallable-instance>`, identity hashing |
| `crates/quokka-vm/src/unit/instance.rs` (create) | Rust unit tests |
| `lisp/library/clos.lisp` (create) | metaobject layout, bootstrap, built-in/structure/condition classes, CPL, `defclass`, instances, slots, `class-of`, `find-class` |
| `lisp/library/types.lisp`, `type-algebra.lisp` (modify) | class names and class objects as type specifiers; the `:instance` kind; `type-of` of a standard object |
| `lisp/library/printer.lisp`, `eval.lisp` (modify) | printing a standard object and a class; `compiled-function-p` false of a funcallable instance |
| `crates/quokka-bootstrap/src/boot.rs`, `lisp/library/rebuild.lisp` (modify) | list `clos.lisp` after `type-algebra.lisp` |
| `tests/lisp/classes.lisp`, `.expected` (create) | differential tests |
| `crates/quokka-bootstrap/tests/differential.rs` (modify) | `classes => "classes"`; a Quokka-only test of the primitives; rebuild counts |
| `tests/ansi/deferred.lisp`, `baseline.txt`, `docs/ansi-test.md` (modify) | the operators built here leave `tier-2:clos` |
| docs, `CLAUDE.md`, book chapter 73 (modify/create) | as for sub-step 1 |

---

### Task 1: Funcallable and standard instances in Rust

**Files:** `crates/quokka-vm/src/instance.rs` (create; `pub mod instance;` in `lib.rs`), `heap.rs`, `primitives.rs`, `vm.rs`, `heap/verify.rs`, `image.rs`, `printer.rs`, `hash.rs`; test `crates/quokka-vm/src/unit/instance.rs` (pulled in by `#[cfg(test)] #[path = "unit/instance.rs"] mod tests;` in `instance.rs`), and one Quokka-only test in `crates/quokka-bootstrap/tests/differential.rs`.

**Interfaces — produces (primitive names, all in package QUOKKA):**
- `(%allocate-standard-instance wrapper slots)` → a `StandardInstance` with slot 0 `wrapper`, slot 1 `slots`.
- `(%allocate-funcallable-instance wrapper slots)` → a `FuncallableInstance`: slot 0 the function (nil), slot 1 `wrapper`, slot 2 `slots`.
- `(%instance-wrapper object)` → the wrapper of either type, or nil for any other object (so it is also the predicate).
- `(%instance-slots object)`, `(%set-instance-wrapper object wrapper)`, `(%set-instance-slots object slots)` → for either type; `WrongType` for anything else.
- `(%funcallable-instance-p object)` → t or nil.
- `(%set-funcallable-instance-function instance function)` → `function`; `WrongType` unless `instance` is a funcallable instance and `function` is a function that is not itself a funcallable instance (Review Focus 1).

Stores go through `set_slot`, so they call `Storage::note_store` (the store barrier, #86).

- [ ] **Step 1: Write failing Rust unit tests** in `unit/instance.rs`: allocate each type, read back wrapper and slots, set them; `%instance-wrapper` of a cons, a structure and a fixnum is nil; `%set-funcallable-instance-function` refuses a fixnum and a funcallable instance; a heap holding both types survives `collect` and `Heap::verify`; an image holding both round-trips (`image::save`/`image::load`) with format 8.
- [ ] **Step 2: Write the failing Quokka-only test** `a_funcallable_instance_is_called_through_its_function` in `differential.rs`, using `run_compiled`:

```lisp
(defvar *f* (quokka::%allocate-funcallable-instance :w (vector)))
(handler-case (funcall *f* 1) (error () :no-function))
(quokka::%set-funcallable-instance-function *f* (lambda (x) (* x 2)))
(funcall *f* 21)
(apply *f* '(4))
(mapcar *f* '(1 2))
(list (functionp *f*) (compiled-function-p *f*) (quokka::%funcallable-instance-p *f*))
(quokka::%instance-wrapper *f*)
(handler-case (quokka::%set-funcallable-instance-function *f* *f*) (error () :refused))
```

expected `=> *F*`, `=> :NO-FUNCTION`, `=> #<…>` (match the function line loosely or bind it), `=> 42`, `=> 8`, `=> (2 4)`, `=> (T NIL T)`, `=> :W`, `=> :REFUSED`.

- [ ] **Step 3: Run both, see them fail** — `command cargo test -p quokka-vm instance` and `command cargo test -p quokka-bootstrap --test differential a_funcallable_instance` → FAIL (types and primitives missing).
- [ ] **Step 4: Implement.** In `call_value`, after `closure::designated`: a `FuncallableInstance` is replaced by its slot 0; if that is not a function (nil included), `self.fail(EvalError::NotAFunction(..))`. No loop is needed, because the setter refuses a funcallable instance as the function. `hash::content` hashes both types by type only (identity objects, as functions are). `printer.rs` writes `#<instance>` and `#<funcallable-instance>`. `compiled-function-p` in `eval.lisp` becomes `(and (functionp object) (not (%funcallable-instance-p object)))`, as SBCL answers nil for a generic function.
- [ ] **Step 5: Both pass; the whole VM crate passes** — `command cargo test -p quokka-vm` and the differential test → PASS.

### Task 2: The class metaobjects and the bootstrap

**Files:** create `lisp/library/clos.lisp`; modify `boot.rs` and `rebuild.lisp` (after `type-algebra.lisp`); test `tests/lisp/classes.lisp`, `tier1_cases!` entry `classes => "classes"`.

**Interfaces — produces:**
- A **wrapper** is a simple vector `#(class instance-slot-names state)`: `state` nil now (sub-step 5 adds `:obsolete`). Accessors `%wrapper-class`, `%wrapper-slot-names`, `%wrapper-state`.
- Every class metaobject is a `StandardInstance` whose slot vector has these locations, in this order, for every metaclass: `0 name`, `1 direct-superclasses`, `2 direct-subclasses`, `3 precedence-list`, `4 direct-slots`, `5 slots`, `6 direct-default-initargs`, `7 default-initargs`, `8 finalized-p`, `9 wrapper`, `10 documentation`, `11 plist`. Hand-written readers and writers `%class-name` … `%class-plist`, `(%set-class-… class value)`. They are the slots of class `class`, which every metaclass inherits unchanged, so the locations are fixed.
- A **slot definition** is a `StandardInstance` of `standard-direct-slot-definition` or `standard-effective-slot-definition`, with locations `0 name`, `1 initargs`, `2 initform`, `3 initfunction`, `4 allocation`, `5 type`, `6 readers`, `7 writers`, `8 documentation`, `9 location`; accessors `%slot-definition-…`.
- `(find-class symbol &optional (errorp t) environment)`, `(setf find-class)` (through `%set-find-class`, since `setf.lisp` comes earlier), kept in `%class-table`, an `eq` hash table made by `defvar` so a rebuild keeps it.
- `(class-of object)`, `(class-name class)`.
- The classes of CLHS figure 4-8 with the precedence lists CLHS 4.3.7 and figure 4-8 give, plus SBCL's `fixnum`, `bignum` and `double-float` under `integer` and `float`; `standard-object`, `structure-object`, `condition`; metaclasses `class`, `standard-class`, `built-in-class`, `structure-class`, `funcallable-standard-class`, `condition-class`; `metaobject`, `specializer`, `funcallable-standard-object` (superclasses `function`, `standard-object`), and the slot definition classes. MOP-only names (`metaobject`, `specializer`, `condition-class`, `funcallable-standard-class`, `funcallable-standard-object`, `slot-definition`, `direct-slot-definition`, `effective-slot-definition`, `standard-slot-definition`, `standard-direct-slot-definition`, `standard-effective-slot-definition`) are QUOKKA symbols until sub-step 7 exports them from `QUOKKA-MOP`.

The bootstrap, in this order (each stage idempotent: an existing class is reused and refilled, never replaced):
1. Make the class objects of `t`, `standard-object`, `metaobject`, `specializer`, `class`, the six metaclasses, and the slot definition classes as standard instances with an empty slot vector of the right length and a placeholder wrapper, and enter them in `%class-table`.
2. Give each metaclass its wrapper (its class field the metaclass, its slot names the twelve above) and set every stage-1 class's wrapper to its metaclass's.
3. Make the slot definition objects for the stage-1 classes' direct slots, now that the slot definition classes have wrappers.
4. Compute each stage-1 class's precedence list and effective slots (`%finalize-class`), set subclasses.
5. Define the built-in classes with `%ensure-built-in-class name supers`.

- [ ] **Step 1: Write the failing differential file** `tests/lisp/classes.lisp` (header as in `tests/lisp/types.lisp`), covering at least:

```lisp
(class-name (find-class 'standard-class))
(class-name (class-of (find-class 'standard-class)))
(eq (class-of (find-class 'standard-class)) (find-class 'standard-class))
(class-name (class-of (find-class 'integer)))
(class-name (class-of (find-class 'standard-object)))
(class-name (class-of 5)) (class-name (class-of (expt 2 70))) (class-name (class-of 1/2))
(class-name (class-of #\a)) (class-name (class-of 'a)) (class-name (class-of nil))
(class-name (class-of '(1))) (class-name (class-of (make-hash-table))) (class-name (class-of #'car))
(subtypep (class-of "abc") 'string) (subtypep (class-of #(1)) 'vector)
(find-class 'no-such-class nil)
(handler-case (find-class 'no-such-class) (error () :error))
(typep nil (find-class 'symbol)) (typep nil (find-class 'list)) (typep "a" (find-class 'vector))
```

Precedence lists are tested through `typep` and `subtypep`, since the MOP's reader has a different name in SBCL (`sb-mop:class-precedence-list`) and the file must stay portable. The exact lists of `string`, `null` and `standard-class` are asserted in a Quokka-only test beside Task 1's, with `quokka::%class-precedence-list`.

- [ ] **Step 2: SBCL baseline; see Quokka fail** — `sbcl --script scripts/oracle.lisp tests/lisp/classes.lisp > tests/lisp/classes.expected`; `command cargo test -p quokka-bootstrap --test differential tier1::classes` → FAIL.
- [ ] **Step 3: Implement the bootstrap, `find-class`, `class-of`, `class-name`, and `%compute-class-precedence-list`** by CLHS 4.3.5: a topological sort of the local precedence orders, breaking each tie by choosing the candidate with a direct subclass rightmost in the list built so far; an inconsistent order signals `error` (Review Focus 2). `class-of` dispatches as `type-of` does; a structure's class is made the first time it is asked for, from its layout (`structure-class`, superclass the included structure's class or `structure-object`), and kept in `%class-table`; a condition's class is the class its type symbol names (`condition-class`, hierarchy from `%condition-supertypes`).
- [ ] **Step 4: The file passes** — `command cargo test -p quokka-bootstrap --test differential tier1::classes`.

### Task 3: `defclass` and instances

**Files:** `lisp/library/clos.lisp`; append to `tests/lisp/classes.lisp`.

**Interfaces — produces:** `defclass` (macro over `(ensure-class name &key direct-superclasses direct-slots direct-default-initargs metaclass documentation)`, which returns the class); `allocate-instance`, `make-instance`, `initialize-instance`, `reinitialize-instance`, `shared-initialize` (ordinary functions, keyword-taking as CLHS gives them); `slot-value`, `(setf slot-value)` (a setf function), `slot-boundp`, `slot-makunbound`, `slot-exists-p`, `slot-missing`, `slot-unbound`; readers, writers and accessors as ordinary functions and setf functions; `%slot-unbound-marker`, a `defvar` holding an uninterned symbol that fills unbound slots.

Rules (CLHS 7.1, 7.5): effective slots merge same-named direct slots, most specific first — `:initform` and `:allocation` from the most specific that has one, `:initarg`s unioned, `:type` the `and` of all; an `:instance` slot has a location in the slot vector, a `:class` slot a cons shared by the class and its subclasses that do not redefine it; `make-instance` appends the class's default initargs not already supplied, checks every initarg is valid (a slot initarg or `:allow-other-keys`; otherwise `program-error`), then calls `allocate-instance` and `initialize-instance`; `shared-initialize` fills a slot from the first matching initarg, else, if the slot is unbound and named by the slot-names argument (or that is `t`), from its initfunction; `slot-value` of an unbound slot calls `slot-unbound`, which signals `unbound-slot`; of a missing slot calls `slot-missing`, which signals `error`. A `defclass` whose superclass is not yet defined makes a forward-referenced placeholder (Review Focus 3): `make-instance` of the class, or of a subclass, signals `error` until the superclass is defined.

- [ ] **Step 1: Append failing forms** — at least: a class with `:initarg`/`:initform`/`:accessor`/`:reader`/`:writer`; `make-instance` with and without initargs; `slot-value`, its `setf`, `slot-boundp`, `slot-makunbound`, `slot-exists-p`; inheritance with a redefined slot and merged initargs (Review Focus 5); `:allocation :class` shared between two instances and a subclass; `:default-initargs` overridden by a supplied initarg; an invalid initarg in `handler-case` with `program-error`; an unbound slot with `unbound-slot`; a missing slot with `error`; `(typep obj 'point)`, `(typep obj 'standard-object)`, `(subtypep 'point3 'point)`, `(type-of obj)`; the inconsistent precedence list of Review Focus 2 in `handler-case`; the forward reference of Review Focus 3; `defclass` returning the class (`(class-name (defclass q () ()))`).
- [ ] **Step 2: Baseline; see the new forms fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Pass; all tier-1 files pass** — `command cargo test -p quokka-bootstrap --test differential tier1`.

### Task 4: Classes in the type system

**Files:** `lisp/library/types.lisp`, `lisp/library/type-algebra.lisp`; append to `tests/lisp/classes.lisp` and `tests/lisp/subtypep.lisp`.

**Interfaces:** `typep` and `subtypep` accept a class name defined by `defclass` and a class object as type specifiers; `type-of` of a standard object is its class's name (or the class, when the class has no proper name).

- `%typep-atom` asks `find-class` (no error) after its other cases: a standard or funcallable class means `(member class (%class-precedence-list (class-of object)))`. A class object as a specifier is handled before `%expand-type` treats it as a symbol.
- A new algebra kind `(:instance . :tree)`, for standard and funcallable instances, whose `%tree-subtype-p` is membership in the class precedence list; `%object-kind` puts any object with an `%instance-wrapper` there. `standard-object` → `:instance` root `standard-object`; `function` gains an `:instance` term rooted at `function` (funcallable instances are functions); a defined class name → `:instance` root its name. A built-in, structure or condition class object maps to its name's existing ctype.
- Two `:instance` roots neither of which precedes the other may share instances (multiple inheritance): "don't know", as for conditions.

- [ ] **Step 1: Append failing forms** — `(subtypep 'point 'standard-object)`, `(subtypep 'point3 'point)`, `(subtypep 'point 'point3)`, `(subtypep 'point 'structure-object)`, `(subtypep (find-class 'point) 'point)`, `(typep (make-instance 'point) (find-class 'point))`, `(subtypep 'standard-class 'class)`, `(typep (find-class 'point) 'standard-class)`, `(subtypep 'point '(or point integer))`, and the TAC-3 multiple-inheritance pairs from ansi-test's `types-and-class-2.lsp` written out.
- [ ] **Step 2: Baselines; see them fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Pass; all tier-1 files pass.**

### Task 5: Printing, ansi-test, and the rebuild

**Files:** `lisp/library/printer.lisp`; `tests/ansi/deferred.lisp` (remove from `tier-2:clos` the operators now built: `defclass`, `make-instance`, `allocate-instance`, `initialize-instance`, `reinitialize-instance`, `shared-initialize`, `slot-value`, `slot-boundp`, `slot-makunbound`, `slot-exists-p`, `slot-missing`, `slot-unbound`, `class-of`, `find-class`, `class-name`), `tests/ansi/baseline.txt`, `docs/ansi-test.md`, `crates/quokka-bootstrap/tests/differential.rs` (rebuild counts).

- The printer writes a standard object as `#<NAME {n}>` and a class as `#<STANDARD-CLASS NAME>` (`print-unreadable-object` style, `*print-readably*` an error); the number is the object's identity number if one exists, else omitted. Not tested against SBCL (addresses differ); a Rust-side or Quokka-only check that printing does not fail is enough. `print-object` is sub-step 4.

- [ ] **Step 1: Run the full ansi-test with the update** (background) and classify every non-passing test that newly runs: a bug in Tasks 2–4 is fixed with a line in `classes.lisp`; a test needing `defgeneric`/`defmethod` stays explained by `tier-2:clos`. Repeat until no plain failure, escape or timeout.
- [ ] **Step 2: Update the rebuild counts** (`a_rebuilt_world_is_the_same_world` reports them) and check Review Focus 4 with a Quokka-only assertion in `differential.rs`: in the rebuilt world, `(eq (find-class 'standard-class) <the class before>)` cannot be asked across worlds, so assert instead that a class defined before `(quokka::%rebuild-world)` in one world still makes working instances after it, and that `(%class-wrapper (find-class 'standard-object))` is the same object before and after.

### Task 6: Documentation, the book, and the commit

**Files:** `docs/design.md` (a decision entry: two heap types instead of `Instance`, the bootstrap stages, ordinary functions until sub-step 3, MOP names in QUOKKA until sub-step 7, structure classes made lazily), `docs/clos.md` (correct *Objects in the heap* to the two new heap types and say why), `CLAUDE.md`, `docs/manual.md`, `book/src/part-7-objects/73-…md` (with `book-voice`), `book/src/SUMMARY.md`, `book/src/glossary.md` (class, instance, metaclass, metaobject, wrapper, class precedence list, funcallable instance, bootstrap of a metacircular system), `book/src/what-has-to-be-built.md`; this plan.

- [ ] **Step 1:** write the documents.
- [ ] **Step 2:** `if ./scripts/check.sh > … 2>&1; then echo ok; else tail …; fi` (background) → ok.
- [ ] **Step 3:** commit on a branch `clos-classes`; tick sub-step 2 on #124 with a comment.
