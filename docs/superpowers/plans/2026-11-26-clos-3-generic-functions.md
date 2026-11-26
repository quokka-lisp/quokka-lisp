# CLOS sub-step 3 · Generic functions — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generic functions and methods: `defgeneric`, `defmethod`, dispatch on the classes (and `eql` identity) of the required arguments with a cache, standard method combination with `call-next-method` and `next-method-p`, and the CLHS functions around them; and `make-instance`, `initialize-instance`, `reinitialize-instance`, `shared-initialize`, `allocate-instance`, `slot-missing`, `slot-unbound` and every accessor become generic functions with standard methods.

**Architecture:** Lisp only, in a new file `lisp/library/generic-functions.lisp` after `clos.lisp`. A generic function is a funcallable instance of `standard-generic-function` (sub-step 2's heap type) whose function is a *discriminating function*, a closure that finds the classes of the required arguments, looks them up in the generic function's cache, and on a miss computes the applicable methods, sorts them (CLHS 7.6.6), builds the effective method from closures (standard combination, CLHS 7.6.6.2) and caches it. Generic function and method metaobjects have fixed slot places read by hand-written `%gf-…` and `%method-…` functions, as classes do, so dispatch never dispatches.

**Tech Stack:** tier-1 Lisp; differential tests against SBCL 2.6.10; ansi-test.

**Spec:** `docs/clos.md` — *Generic functions and dispatch*, *Redefinition* (its `defgeneric`/`defmethod` paragraph), *The warm rebuild*, *Order of work* item 3; issue #124.

## Global Constraints

- No Rust. The compiler never calls a generic function; the fixpoint holds unchanged except for the rebuild's `(:FORMS … :REPLACED …)` counts.
- Effective methods are built from closures; nothing compiles at dispatch time (`docs/clos.md`; only long-form `define-method-combination`, sub-step 6, compiles).
- The warm rebuild reloads both files while generic functions are in use: `defgeneric` keeps the generic function object, `defmethod` of the same specializers and qualifiers replaces the method, and every cache is emptied when a method is added or removed or a class is redefined.
- `slot-value`, `slot-boundp`, `slot-makunbound`, `slot-exists-p` stay ordinary functions (CLHS makes them functions); the dispatch machinery never calls a generic function.
- Every `%` name shares package `QUOKKA`: search `lisp/` before adding one. A `defvar` goes above the first function that binds it.
- Errors signal the condition type SBCL's does where a differential file can see it; behaviour matches SBCL (rule 6). `./scripts/check.sh` green before the commit; no attribution in commit messages.

## Review Focus

1. **A call with too few required arguments**, or an odd keyword list, to a generic function: a `program-error`, as for an ordinary function, never a fault inside the dispatch.
2. **Redefining a class after its instances were dispatched on**, so that the cached effective method for the old precedence list is stale: the next call uses the new precedence list.
3. **`call-next-method` with new arguments**, called twice in one method body, and called from inside a closure that outlives the method's dynamic extent (CLHS allows calling it later): each call uses the right next methods.
4. **A method whose lambda list is not congruent** with its generic function's (CLHS 7.6.4): an error at `defmethod`, leaving the generic function unchanged.
5. **Keyword arguments accepted by one applicable method but not another** (CLHS 7.6.5): the call is accepted; a keyword no applicable method accepts is a `program-error` unless `:allow-other-keys` is true.

---

## File structure

| File | Responsibility |
| --- | --- |
| `lisp/library/generic-functions.lisp` (create) | metaobject classes `generic-function`, `standard-generic-function`, `method`, `standard-method`, `standard-accessor-method`, `standard-reader-method`, `standard-writer-method`, `eql-specializer`, `method-combination`; `%gf-…`/`%method-…` accessors; `ensure-generic-function`, `defgeneric`, `defmethod`, `add-method`, `remove-method`, `find-method`, `method-qualifiers`, `compute-applicable-methods`, `no-applicable-method`, `no-next-method`, `intern-eql-specializer`; dispatch, cache, standard combination; the CLOS functions made generic |
| `lisp/library/clos.lisp` (modify) | the instance protocol functions renamed to `%` workers (`%make-instance`, `%allocate-instance`, `%initialize-instance`, `%reinitialize-instance`, `%shared-initialize`, `%slot-missing`, `%slot-unbound`) used until the generic functions exist; `%define-accessors` makes reader and writer methods once generic functions exist; class redefinition bumps the dispatch epoch; initarg checking learns the keywords of applicable methods (CLHS 7.1.2) |
| `lisp/library/types.lisp` (modify) | `type-of` of a funcallable instance is its class's name (the instance branch before the `functionp` branch) |
| `crates/quokka-bootstrap/src/boot.rs`, `lisp/library/rebuild.lisp` | list the new file after `clos.lisp` |
| `tests/lisp/generic-functions.lisp`, `.expected` (create) | differential tests |
| `crates/quokka-bootstrap/tests/differential.rs` (modify) | `generic_functions => "generic-functions"`; extend `a_rebuild_keeps_classes_and_their_instances` with a generic function and a method; rebuild counts |
| `tests/ansi/deferred.lisp`, `baseline.txt`, `docs/ansi-test.md` | `defgeneric`, `defmethod`, `call-next-method`, `next-method-p`, `add-method`, `remove-method`, `find-method`, `compute-applicable-methods`, `ensure-generic-function`, `method-qualifiers`, `no-applicable-method`, `no-next-method`, `function-keywords` leave `tier-2:clos` |
| docs, `CLAUDE.md`, book chapter 74 | as before |

---

### Task 1: Metaobjects, `ensure-generic-function`, `defmethod`, dispatch with standard combination

**Interfaces — produces:**
- Generic function slot places (each read by `%gf-NAME`, written by `%set-gf-NAME`): `0 name`, `1 lambda-list`, `2 methods`, `3 method-combination`, `4 argument-precedence-order`, `5 cache`, `6 epoch`, `7 declarations`, `8 documentation`, `9 method-class`, `10 initial-methods`, `11 eql-positions`, `12 plist`. A load-time assertion checks `standard-generic-function`'s instance slot names are these, in this order.
- Method slot places (`%method-NAME`): `0 generic-function`, `1 specializers`, `2 qualifiers`, `3 lambda-list`, `4 function`, `5 documentation`, `6 plist`; reader/writer methods add `7 slot-name`. Same assertion.
- A **method function** is `(lambda (args next) ...)`: ARGS the list of arguments, NEXT the effective method of the next methods (a function of one argument, the argument list) or nil.
- `(ensure-generic-function name &key lambda-list documentation method-combination argument-precedence-order generic-function-class method-class declare &allow-other-keys)` → the generic function; an ordinary function or nil in NAME's function cell is replaced; a macro or special operator is an error. An existing generic function keeps its identity; its lambda list may change only if every method stays congruent.
- `(defmethod name qualifier* specialized-lambda-list [[declaration* | documentation]] form*)` → the method; specializers are class names, `(eql form)` (FORM evaluated when the `defmethod` runs), or `t`; the body is in a block named after the function (`(setf f)` → `f`); `call-next-method` and `next-method-p` are local functions (made only when the body mentions them); a new method replaces one with the same specializers and qualifiers.
- `(intern-eql-specializer object)` → one `eql-specializer` per object (an `eql` hash table in a `defvar`).
- `add-method`, `remove-method`, `find-method gf qualifiers specializers &optional (errorp t)`, `method-qualifiers`, `compute-applicable-methods gf arguments`, `no-applicable-method gf &rest args` (default: `error`), `no-next-method gf method &rest args` (default: `error`), `function-keywords method` → two values.

**Dispatch** (`%dispatch gf args`): fewer arguments than required → `program-error`. The cache key is the list of the required arguments' classes, except that in a position listed in `eql-positions` an argument `eql` to some eql-specializer's object is keyed by that specializer. The cache is an `eq` hash table on the first key, holding an alist from the rest of the key (compared element-wise with `eq`) to the effective method; it is emptied when `epoch` differs from `%dispatch-epoch` (a `defvar`, incremented by `ensure-class`, `add-method`, `remove-method`). Applicable methods: every method whose specializers each accept its argument; sorted by comparing specializers in argument precedence order: an `eql` specializer before a class, and of two classes the one earlier in the argument's class precedence list. Standard combination: around methods first, each calling the next by `call-next-method`, the innermost calling the core; the core runs every `:before` method most specific first, the primary methods as a chain (each may `call-next-method`), and every `:after` method least specific first, returning the primary's values (`multiple-value-prog1`). No primary method → `error`. An effective method is `(lambda (args) ...)`.

- [ ] **Step 1: Write the failing differential file** `tests/lisp/generic-functions.lisp` covering: a generic function with methods on two classes and on `t`; a subclass picking the most specific method; `call-next-method` with and without new arguments; `next-method-p` true and false; `:before`, `:after` and `:around` methods with their order recorded in a list; multiple values through all of them; two required arguments dispatching on both (multiple dispatch), and a tie broken by the leftmost argument; an `eql` specializer winning over a class; a method on `integer`, `string`, `null`, `list`, `t` (built-in classes); a method on a structure type; no applicable method in `handler-case` (`error`); `call-next-method` with no next method (`error`); too few arguments (`program-error`, Review Focus 1); replacing a method with `defmethod` again; `find-method` and `remove-method`; `(typep #'gf 'generic-function)`, `(typep #'gf 'function)`, `(functionp #'gf)`, `(compiled-function-p #'gf)`, `(type-of #'gf)` name via `class-name`; a non-congruent `defmethod` in `handler-case` (Review Focus 4); redefining a class's superclasses after a call and calling again (Review Focus 2); `call-next-method` saved in a closure and called after the method returned (Review Focus 3); `defgeneric` with a documentation string returning the generic function.
- [ ] **Step 2: SBCL baseline; see Quokka fail** (`defgeneric` undefined).
- [ ] **Step 3: Implement** `generic-functions.lisp` up to dispatch and `defmethod`, listed in `boot.rs` and `%library-files` after `clos.lisp`; `ensure-class` increments `%dispatch-epoch`.
- [ ] **Step 4: All but the `defgeneric`-option lines pass.**

### Task 2: `defgeneric`, keyword arguments, argument precedence order

**Interfaces:** `defgeneric` with options `(:documentation s)`, `(:method qualifier* lambda-list . body)` (methods remembered in `initial-methods` and removed by the next `defgeneric` of the name), `(:argument-precedence-order …)`, `(:method-combination standard)` (any other → `error` naming sub-step 6), `(:generic-function-class standard-generic-function)`, `(:method-class standard-method)`, `(declare …)` (kept, not used). Lambda-list congruence (CLHS 7.6.4) checked by `add-method` and by `ensure-generic-function` against existing methods → `error`. Keyword arguments (CLHS 7.6.5): a method with `&key` accepts any keyword some applicable method accepts; the effective method checks that every keyword in the call is accepted by an applicable method or by the generic function's lambda list, unless one of them has `&allow-other-keys` or the call has a true `:allow-other-keys`; otherwise `program-error`.

- [ ] **Step 1: Append failing forms** — `defgeneric` with `:method` options and with `:documentation`; `defgeneric` again with fewer `:method`s removing the old one; `:argument-precedence-order` reversing which argument breaks a tie; `&optional` and `&key` methods; a keyword accepted only by a more specific method (accepted, Review Focus 5); a keyword no method accepts (`program-error`); `:allow-other-keys t`; `(:method-combination +)` in `handler-case` (`error` until sub-step 6 — leave out if SBCL accepts it, and say why in the file).
- [ ] **Step 2: Baseline; see the new forms fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Pass; all tier-1 files pass.**

### Task 3: The instance protocol and accessors become generic

**Interfaces:** `make-instance` (methods on `symbol` and `standard-class`/`funcallable-standard-class`), `allocate-instance`, `initialize-instance`, `reinitialize-instance`, `shared-initialize` (each one standard method on `standard-object`, plus `allocate-instance` on `funcallable-standard-class`), `slot-missing`, `slot-unbound` (methods on `t`), the readers and writers `defclass` names (methods of class `standard-reader-method`/`standard-writer-method`, `slot-name` remembered). The standard methods call the generic functions (`make-instance`'s calls `initialize-instance`, which calls `shared-initialize`), so a program's `:after` method on `initialize-instance` runs. `%check-initargs` adds the keywords of the applicable `initialize-instance`, `shared-initialize` and `allocate-instance` methods' lambda lists (CLHS 7.1.2), and any `&allow-other-keys` among them allows all. Until the generic functions exist (during `clos.lisp` and the first part of `generic-functions.lisp`) the `%` workers do the work; the generic functions' standard methods call the same workers' pieces.

- [ ] **Step 1: Append failing forms** — an `:after` method on `initialize-instance` that fills a slot computed from others; a `:before` method on `shared-initialize`; an `initialize-instance` method with `&key extra` making `(make-instance 'c :extra 1)` valid; `make-instance` with a class object; a `slot-missing` method returning a default; a `slot-unbound` method returning a default; an accessor called on an object of another class (`error` from `no-applicable-method`); an `:around` method on an accessor; `(typep #'point-x 'generic-function)`.
- [ ] **Step 2: Baseline; see them fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Pass; all tier-1 files pass; `tier1::classes` unchanged.**

### Task 4: ansi-test, the rebuild, documentation, commit

- [ ] **Step 1:** update `tests/ansi/deferred.lisp`; full run with `ANSI_TEST_UPDATE=1` (background); every newly failing test fixed with a line in `generic-functions.lisp` or explained by a deferred feature (`define-method-combination`, `print-object`, `with-slots`, `change-class`, conditions). Repeat until no plain failure, escape or timeout.
- [ ] **Step 2:** extend `a_rebuild_keeps_classes_and_their_instances`: before the rebuild, `(defgeneric area (s))`, a method, and `(defvar *gf* #'make-instance)`; after it, `(eq *gf* #'make-instance)`, the user method still applies, `make-instance` still works. Update the rebuild counts.
- [ ] **Step 3:** `docs/design.md` decision entry (method functions of args and next; closures not compilation; the cache and its epoch; ordinary `slot-value`; fixed metaobject places), `docs/clos.md` if anything moved, `CLAUDE.md`, `docs/manual.md`, book chapter 74 (`book-voice`), `SUMMARY.md`, glossary (generic function, method, specializer, dispatch, effective method, method combination, discriminating function), `what-has-to-be-built.md`.
- [ ] **Step 4:** `./scripts/check.sh` green; commit on branch `clos-generic-functions`; tick sub-step 3 on #124.
