# CLOS sub-step 6 · Method combination — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every method combination of CLHS 7.6.6: the nine built-in operator combinations (`+ and append list max min nconc or progn`) with `:most-specific-first`/`:most-specific-last`, `define-method-combination` in its short form and in its long form (method groups, `:order`, `:required`, `:arguments`, `:generic-function`, `call-method`, `make-method`), and `invalid-method-error` and `method-combination-error`.

**Architecture:** Lisp only, in `generic-functions.lisp`. A generic function's `method-combination` slot holds a method combination object (class `standard-method-combination`: name, options) instead of the symbol `standard`. A table maps each combination name to a *combiner*, a function of the generic function, the sorted applicable methods and the options that returns an effective method `(lambda (args) ...)`. `standard` and every operator combination build closures, as standard combination does today; an operator that names a macro or special operator in a user's short form, and every long form, builds a form with `call-method`/`make-method` and compiles it with `compile`, once per cache miss (`docs/clos.md`: the long form is the one place dispatch compiles).

**Tech Stack:** tier-1 Lisp; differential tests against SBCL 2.6.10; ansi-test.

**Spec:** `docs/clos.md` — *Generic functions and dispatch* (method combination paragraphs), *Order of work* item 6; issue #124; CLHS 7.6.6, `define-method-combination`.

## Global Constraints

- No Rust. The compiler never calls a generic function, and the library defines no method combination the compiler uses, so the fixpoint holds except for the rebuild's counts.
- Effective methods of `standard` and the built-in operator combinations are closures; only a long form, or a short form whose operator is a macro or special operator, compiles.
- The warm rebuild reloads `generic-functions.lisp` while generic functions exist: a generic function keeps its combination object, the built-in combinations are defined again in place, and a user's `define-method-combination` survives as a name in the table.
- Every `%` name shares package `QUOKKA`: search `lisp/` before adding one. The symbol the effective method binds its argument list to is a `QUOKKA` symbol, so no form in `COMMON-LISP-USER` can see it (CLHS `define-method-combination`).
- Differential files never print an address and never bind a COMMON-LISP symbol. `./scripts/check.sh` green before the commit; no attribution in commit messages.

## Measured in SBCL 2.6.10 (`*print-pretty*` nil)

- `+` over methods 1, 10, 100 gives 111; `(:method-combination list :most-specific-last)` gives `(:A :B)`; `list` with an `:around` method that calls `call-next-method` gives `(:AROUND :B :A)`.
- A single primary method's values pass through when the combination has `:identity-with-one-argument` true, as every built-in one but `list` has: `or` and `progn` and `+` return `(NIL 1)`, `(1 2)`, `(3 4)` from one method; with two methods, `or` returns the first true value alone and `and` the last method's values (`(1 2)`).
- A method whose qualifiers the combination does not accept (none, or `:before`, on a `progn` generic function) is accepted by `defmethod` (SBCL warns) and makes the call signal `invalid-method-error`'s error; no primary method is an error.
- A short form `(define-method-combination my-list :operator list :identity-with-one-argument nil)` behaves as `list`; with no `:operator`, the operator is the combination's own name.
- The long form reproduces standard combination (CLHS's own example); `:order` reverses a group; a group pattern `*` takes every method; two methods with the same specializers in one group is an error; `:required t` with an empty group is an error; `method-combination-error` signals from the body.

## Review Focus

1. **`call-next-method` inside a method called through `call-method` with an explicit next-method list**, including `(make-method form)` elements: each call reaches the next method of that list, and `next-method-p` says whether there is one.
2. **A generic function whose combination is redefined** (`define-method-combination` again, or `defgeneric` with another `:method-combination`) after it was called: the cached effective methods are dropped.
3. **Keyword checking (CLHS 7.6.5) for a non-standard combination**: the same as for standard combination.
4. **A long-form body that signals an error**, or returns a malformed form: the error reaches the caller; the cache does not keep a broken effective method.
5. **The warm rebuild** with a user combination and a generic function using it: both still work after it.

---

## File structure

| File | Responsibility |
| --- | --- |
| `lisp/library/generic-functions.lisp` (modify) | combination objects and the name table; `%standard-effective-method` registered as `standard`; `%operator-combiner`; `define-method-combination` (both forms), `call-method`/`make-method` expansion, `%long-form-effective-method`; `invalid-method-error`, `method-combination-error`; `defgeneric`/`ensure-generic-function` accept any defined combination with its options |
| `tests/lisp/method-combination.lisp`, `.expected` (create) | differential tests |
| `crates/quokka-bootstrap/tests/differential.rs` (modify) | `method_combination => "method-combination"`; rebuild test extended; rebuild counts |
| `tests/ansi/deferred.lisp`, `baseline.txt`, `docs/ansi-test.md` | `define-method-combination`, `call-method`, `make-method`, `invalid-method-error`, `method-combination-error` leave `tier-2:clos` |
| docs, `CLAUDE.md`, book chapter 77 `book/src/part-7-objects/77-method-combination.md` | as before |

---

### Task 1: Combination objects and the built-in operator combinations

**Interfaces — produces:**
- `(%method-combination name options)` → a `standard-method-combination` instance (slots `name`, `options`); `%gf-method-combination` holds one; `ensure-generic-function`'s `:method-combination` takes `name` or `(name . options)`, and an undefined name is an error.
- `%combinations`: an `eq` hash table, name → combiner `(lambda (gf methods options) effective-method)`, METHODS sorted most specific first. `standard` is the present `%standard-effective-method`.
- `(%define-operator-combination name operator identity-with-one-argument)` registers a combiner: methods with qualifiers `(name)` are primary and `(:around)` around, anything else `invalid-method-error`; the option `:most-specific-first` (default) or `:most-specific-last` orders the primaries; no primary method is an error; one primary with IDENTITY-WITH-ONE-ARGUMENT is called alone, values passing through; otherwise the operator is applied to each primary's value, with `and`, `or` and `progn` evaluated in order and stopping as those operators do, as closures; around methods chain with `call-next-method` as in standard combination. The nine built-in ones are registered with identity true except `list`.
- `(invalid-method-error method format-control &rest args)` and `(method-combination-error format-control &rest args)` signal a `program-error` (SBCL's types are subtypes of it — check with the oracle).
- The cache is emptied when a combination is (re)defined (`%invalidate-dispatch`).

- [ ] **Step 1: Write the failing differential file** `tests/lisp/method-combination.lisp`: the *Measured* operator cases for all nine, `:most-specific-last`, an `:around` method with `call-next-method`, values through one method, `or`/`and` with several methods, an invalid qualifier and no primary method in `handler-case`, a method with a keyword parameter on a `+` generic function called with and without the keyword (Review Focus 3), `defgeneric` with `(:method-combination +)` redefined to `list` after a call (Review Focus 2).
- [ ] **Step 2: SBCL baseline; register `method_combination`; see Quokka fail.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: The file passes; all tier-1 files pass.**

### Task 2: `define-method-combination`, short form

**Interfaces — produces:** `(define-method-combination name [[:documentation string | :identity-with-one-argument flag | :operator operator]])` → NAME; registers `%define-operator-combination`; an OPERATOR that names a macro or special operator (other than `and`, `or`, `progn`, handled as closures) builds `(operator (call-method m1) ...)` and compiles it through Task 3's machinery.

- [ ] **Step 1: Append failing forms**: the *Measured* short forms; `:operator` defaulting to the name; a short form over a macro operator (e.g. `when`-like user macro) — only if SBCL accepts it; redefinition of a short form after a call (Review Focus 2).
- [ ] **Step 2: Baseline; see them fail. Step 3: Implement. Step 4: Pass.**

### Task 3: `define-method-combination`, long form

**Interfaces — produces:**
- `(define-method-combination name lambda-list (method-group-specifier*) [(:arguments . lambda-list)] [(:generic-function symbol)] [[declaration* | documentation]] form*)` → NAME. A method group specifier is `(name {qualifier-pattern+ | predicate} [[:description string | :order form | :required form]])`; a pattern is `()`, `*`, or a list whose tail may be `*`; a predicate is a function name.
- The combiner binds LAMBDA-LIST to the options, the `:generic-function` symbol to the generic function, each group name to its methods (in group order, reversed under `:order :most-specific-last`), signals for a `:required` empty group and for two methods with the same specializers in one group, calls `invalid-method-error` for a method no group takes, evaluates FORMS to an effective method form, and compiles `(lambda (%emf-args) (let (:arguments bindings) form))` with `call-method` and `make-method` as local macros: `(call-method m nexts)` → `(funcall (%method-function 'm) %emf-args next)` where NEXT is a chain of closures built from NEXTS (methods or `(make-method form)`), nil when empty; `(make-method form)` as the method called → FORM. `:arguments` variables are bound to the argument list's elements by position (required, `&optional`, `&rest`, `&whole`), CLHS 7.6.6.4.
- The compiled effective method is cached as any other; an error while computing or compiling it leaves nothing in the cache.

- [ ] **Step 1: Append failing forms**: CLHS's long-form standard combination (`standard2`) with before/around/primary/after and `call-next-method`; `:order` from an option; a predicate group; `:arguments` and `:generic-function`; `:required` empty; two methods same specializers in a group; `method-combination-error` from the body; a `make-method` in a next-method list and `next-method-p` in a method called through `call-method` (Review Focus 1); a body that errors, then works after the methods change (Review Focus 4).
- [ ] **Step 2: Baseline; see them fail. Step 3: Implement. Step 4: Pass; all tier-1 files pass.**

### Task 4: ansi-test, the rebuild, documentation, commit

- [ ] **Step 1:** the five names leave `tier-2:clos` in `tests/ansi/deferred.lisp`; full ansi-test with `ANSI_TEST_UPDATE=1` until no plain failure, escape or timeout.
- [ ] **Step 2:** extend `a_rebuild_keeps_classes_and_their_instances` with a user `define-method-combination` and a generic function using it, called after the rebuild (Review Focus 5); update the rebuild counts.
- [ ] **Step 3:** `docs/design.md` decision entry, `docs/clos.md` if anything moved, `CLAUDE.md`, `docs/manual.md` (tier-1 file count), book chapter 77 "Method combination" (`book-voice`), `SUMMARY.md`, glossary (method combination, effective method form, `call-method`), `what-has-to-be-built.md`.
- [ ] **Step 4:** `./scripts/check.sh` green; commit on branch `clos-method-combination`; fresh reviewer; fix critical and important findings with tests; commit; tick sub-step 6 on #124.
