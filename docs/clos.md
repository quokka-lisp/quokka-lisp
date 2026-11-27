# CLOS in Quokka: design

*Status: proposed, 26 Nov 2026. Phase 3's first step. Not built.*

This document says what Quokka's object system is, how it is built, and in
what order, closely enough that it can be planned and built from. When a part
is built, its decisions move into `docs/design.md` and its story into the
book, as for every other step; this file stays as the reference for the whole,
the way `bytecode.md` is for the instruction set.

## Why it comes first

Phase 3's gate is *fix a function from inside an error and retry the frame*.
Its parts are conditions with restarts, frames that keep what a retry needs,
definitions that keep their source (#55), processes (#34) and a terminal
debugger. None of them strictly needs CLOS, and we could have built conditions
on a class layer of their own, as SBCL does. We chose to build CLOS first
instead. The tier table already puts CLOS in tier 2, which runs phases 3 to 5,
and phase 5's presentations, commands and inspector all dispatch on the type
of an object. With CLOS in place, `define-condition` makes real classes, a
condition's report is a method, `print-object` and `describe-object` are
generic functions from the start, and nothing has to be fitted onto CLOS
later. The cost is that the gate is further away.

## What we are building

A **small CLOS with a metaobject protocol (MOP)**, written new, in the design
of Part I of *The Art of the Metaobject Protocol* (Kiczales, des Rivières and
Bobrow, MIT Press 1991), whose "Closette" is a complete CLOS in about a
thousand lines of Lisp. *Metacircular* means CLOS is described in CLOS:
classes, generic functions, methods and slot definitions are themselves
instances of classes, and `standard-class` is an instance of itself. We follow
Closette's structure and add what it leaves out for brevity: a dispatch cache,
`eql` specializers done properly, class redefinition, the built-in method
combinations, and the CLHS details it skips. We do not copy its text, which
uses `loop` and would break the tier rule anyway.

Two other ways were considered and rejected. CLOS without a MOP, with classes
and generic functions as hidden structures, is smaller and quicker, but phase
5's inspector asks a class for its slots and a generic function for its
methods, and class redefinition is far easier with a protocol to hang it on;
adding a MOP later would be a rewrite. Porting PCL, SBCL's CLOS, brings proven
code but is large, uses `loop` and full `format`, and would teach us nothing
about how it works.

### Goals

1. Chapter 7 of the CLHS (Objects) and the class half of chapter 4 (Types and
   Classes) behave as SBCL's do, measured by differential files against SBCL
   and by ansi-test's `objects` and `types-and-classes` groups.
2. Every object has a class. `class-of` works on fixnums, characters, conses,
   strings, arrays, hash tables, functions, symbols, numbers of every kind,
   structures and standard objects.
3. The **introspective** half of the MOP exists, so a program can ask a class
   for its superclasses, precedence list and slots, and a generic function for
   its methods and lambda list.
4. Redefining a class with `defclass` updates its existing instances, lazily,
   as CLHS 4.3.6 describes; `change-class` works.
5. `print-object` and `describe-object` are generic functions that the printer
   and `describe` call.
6. The fixpoint still holds, and a warm rebuild still works with CLOS objects
   alive in the world.

### Non-goals

- The **intercessory** half of the MOP, beyond what the CLHS itself requires.
  A program can call `allocate-instance` and define methods on
  `initialize-instance`, `shared-initialize`, `print-object`, `slot-missing`
  and `slot-unbound`, because the standard says so. It cannot define its own
  metaclass or change how slots are accessed (`slot-value-using-class`) or how
  a discriminating function is computed. The design table already defers "the
  full MOP".
- `define-condition`, `handler-bind`, restarts, and conditions as instances.
  That is the next step, and it uses this one. What this step does for
  conditions is in *Conditions in this step* below.
- Streams as classes (Gray streams). Later in tier 2.
- `make-load-form` and `make-load-form-saving-slots` are defined, but nothing
  calls them, because Quokka has no `compile-file`.

### Anti-goals

The work has failed, even if every test passes, if any of these is true:

- The compiler calls a generic function, or compiles itself differently
  because CLOS is loaded. The fixpoint would then hold only by luck. This is
  the rule that already governs compiler macros and symbol macros.
- A program that never uses CLOS gets noticeably slower. The printer calls
  `print-object` for every object it prints, and a reader or printer that pays
  a full dispatch per object would push ansi-test's runner and the warm
  rebuild over their step budgets.
- Rust grows anything that could be Lisp. The one piece of Rust (below) is the
  machine's business, and nothing else is.

## Constraints

- **Tier 1 rules apply.** CLOS is library code, compiled only. It goes in new
  files under `lisp/library/`, listed in `boot::LIBRARY` and in
  `%library-files`, after `structures.lisp` and before `packages.lisp` (which
  must stay last). The interpreter never learns CLOS.
- **No `defstruct` inside the bootstrap of CLOS** where the cold build and a
  rebuild would expand it differently. `structures.lisp` already follows this
  rule and says why.
- **The warm rebuild reloads every file while the old objects are in use.**
  `defclass` of a class that exists, with the same definition, must keep the
  class object's identity, as `%install-layout` keeps a structure's layout.
  Every `defgeneric` and `defmethod` the library itself performs must be safe
  to perform twice.
- **Step budgets.** The rebuild runs under 60 times a form's budget, and
  ansi-test's runner gives each file a budget. Anything the reader or printer
  does per character or per object has to stay cheap.
- **Every `%` name shares package `QUOKKA` with the compiler.** Search `lisp/`
  before adding a `%` helper.
- **Rule 9.** A primitive's cost must not grow with its argument. None of the
  primitives below does.
- **Rule 6.** CLOS's standard names are in `COMMON-LISP`. The MOP's names
  (`class-slots`, `class-precedence-list`, `generic-function-methods`,
  `slot-definition-name`, …) are not, and go in a package of their own,
  `QUOKKA-MOP`, as SBCL puts them in `SB-MOP`.

## The design

### Types come first

Quokka has no `typep`, `type-of` or `subtypep` today; ansi-test's runner lists
them as deferred to phase 3. CLOS needs them: `defclass` makes a type that
`typep` must recognise, and `class-of` must agree with `type-of`. So the step
opens with the type system:

- `type-of`, `typep` and `subtypep` over the standard type specifiers: the
  atomic types, `and`, `or`, `not`, `member`, `eql`, `satisfies`, numeric
  ranges (`(integer 0 *)`), array and vector types, `cons` types and
  `function` types (which `typep` refuses, as CLHS says).
- `deftype`, `typecase`, `etypecase`, `ctypecase`, `check-type`. `ctypecase`
  and `check-type` signal a `type-error` until restarts exist, as `ccase` does
  today.
- `subtypep` is a real type algebra, in the manner of SBCL's `ctype`
  system: a specifier is parsed into a canonical type object (numeric ranges,
  `member` sets, `cons` and array types, unions, intersections and
  negations), and `subtypep` compares those. It must give the definite
  answers CLHS 4.2.2.1 requires, and it may say "don't know" (`nil nil`) only
  where SBCL does, chiefly around `satisfies`. A conservative `subtypep` that
  gives up on hard cases would be a few hundred lines and leave hundreds of
  ansi-test failures; the standing rule for this phase is to do it the right
  way now, and the algebra is what the right way is. This makes the types
  sub-step about as large as the next two together.
- A class name is a type specifier. Before CLOS is loaded, the type system
  knows the built-in types and structure names; when it is loaded, `typep`
  of a class name asks the class.

The existing `%of-type` in `sequences.lisp` becomes a use of `typep`.

### Objects in the heap

A **standard object** is an `ObjectType::StandardInstance`, a heap type of
its own, with two slots: a **wrapper** and a **slot vector**. (This document
first said it would be an `Instance`, the type a structure is. Sub-step 2
gave it a type of its own instead, because every structure path reads an
`Instance`'s slot 0 as a structure layout, and a separate header type keeps
those paths exactly as they were at no cost.) The wrapper
belongs to the instance's class and says which slots the vector holds and in
what order. This is PCL's representation, and the indirection is there for
class redefinition and `change-class`: both give an existing instance a
different set of slots, which may be larger, and the instance must stay the
same object (`eq` to what it was). Replacing its slot vector and wrapper does
that. A structure keeps its present shape, layout and slots inline, because
CLHS does not allow a structure class to be redefined compatibly.

A redefined class gets a new wrapper, and the old one is marked **obsolete**.
An instance is brought up to date the next time any slot of it is touched
(CLHS 4.3.6): `update-instance-for-redefined-class` runs then, with the added
and discarded slots and the old values.

A structure's layout and a standard object's wrapper are told apart by the
object's header type, so a structure accessor's fast path, an `eq` test of
the layout, is unchanged.

### The one piece of Rust: funcallable instances

A generic function is called like any function, `(area shape)`, and is also an
instance with slots: its name, lambda list, methods, method class and
discriminating function. The machine has to be able to call it. So Rust gets a
new heap type, `ObjectType::FuncallableInstance`, whose first slot is a
function and whose other slots are a wrapper and a slot vector as above
(`crates/quokka-vm/src/instance.rs`, with `StandardInstance`). The
VM's `call_value` (through `closure::designated`) calls a funcallable
instance's function with the arguments unchanged. A primitive sets that
function (`%set-funcallable-instance-function`, AMOP's
`set-funcallable-instance-function`); others allocate one and read its parts.
`functionp` is true of one, and `compiled-function-p` is false, as in SBCL.

This cannot be Lisp because calling is the machine's job. The alternatives
were a closure with the metaobject hidden in its captured variables, found
through an `eq` table from closure to metaobject: no Rust, but every generic
function would be two objects that must be kept in step, the table would hold
every generic function alive, and an image would need both. The image format
version goes up by one, and the collector and `Heap::verify` learn the new
type.

### Classes and the bootstrap

The class hierarchy at the top is the standard one: `t`; `standard-object`
and `structure-object` under it; `standard-class`, `built-in-class`,
`structure-class` and `funcallable-standard-class` as metaclasses;
`standard-generic-function`, `standard-method`,
`standard-direct-slot-definition` and `standard-effective-slot-definition`.
The built-in classes are the ones CLHS figure 4-8 lists (`integer`, `fixnum`,
`cons`, `symbol`, `string`, `character`, …); `class-of` finds one for any
object from its heap type and tag.

The circularity is broken the way Closette breaks it: the first classes,
`standard-class` above all, are built by hand as instances whose class slot is
patched afterwards, and only then does `defclass` exist to define the rest.
Every structure type gets a `structure-class` when it is defined, and the
library's own structures (packages, streams, the readtable) get theirs as
CLOS loads, so that methods can specialise on them.

The **class precedence list** is computed by CLHS 4.3.5's algorithm, a
topological sort of the superclass relations in which ties are broken by the
order of direct superclasses. It is not C3, which Dylan and Python use and
which refuses some hierarchies that CLOS accepts.

Slots support `:initarg`, `:initform`, `:reader`, `:writer`, `:accessor`,
`:allocation` (`:instance` and `:class`), `:type` and `:documentation`;
classes support `:default-initargs`, `:metaclass` (one of the four above)
and `:documentation`. Initialisation follows CLHS 7.1: `make-instance` calls
`allocate-instance`, then `initialize-instance`, which calls
`shared-initialize`; initargs are checked for validity (7.1.2).
`make-instance` of a structure class, which CLHS leaves open, does what
SBCL's does: a structure holding its slots' initforms, then
`initialize-instance`; a structure's slots take no initargs.

### Generic functions and dispatch

`defgeneric`, `defmethod`, `ensure-generic-function`, `add-method`,
`remove-method`, `find-method`, `compute-applicable-methods`,
`method-qualifiers`, `call-next-method`, `next-method-p`,
`no-applicable-method`, `no-next-method`, and congruent lambda lists as CLHS
7.6.4 requires. Specializers are classes and `(eql object)`.

A call goes to the generic function's **discriminating function**, set with
`%set-funcallable-instance-function`. It takes the classes of the required
arguments, looks them up in a cache, and on a miss computes the applicable
methods, sorts them by precedence (CLHS 7.6.6), builds the **effective
method** and stores it in the cache. The cache is emptied when a method is
added or removed, or when a class that might affect it is redefined. A generic
function with any `eql` specializer checks those first. The effective method
is built from closures, not by compiling a form at call time: building costs a
few allocations, and calling `compile-form` from inside a dispatch would bring
the compiler's re-entrancy into every first call.

The cache is a list of `(key . effective-method)`, the key a list of one class
or `eql` specializer per required argument, searched from the front with a
bare `eq` loop (`%cached-method`, `%same-key-p`). It was first searched with
`(assoc key cache :test ...)` over `(every #'eq a b)`, which is the same
search and cost 1,000 of a call's 1,446 steps: the general sequence functions
parse keywords and take their slow path for two sequences (#161, found by the
review after phase 4). A call now costs about 340 steps against 4 for a plain
one. What is left is the `&rest` list, the key consed per call, and the linear
search at a call site that sees many classes; #161 keeps the list.

Method combination: `standard` (primary, `:before`, `:after`, `:around`),
the nine built-in operator combinations (`+`, `and`, `append`, `list`, `max`,
`min`, `nconc`, `or`, `progn`), and `define-method-combination` in both its
short and long forms (sub-step 6, done).

The long form is the one exception to building effective methods from
closures. Its body returns an arbitrary Lisp form containing `call-method`
and `make-method`, so the effective method for a long-form combination is
compiled with `compile`, once per cache miss, as SBCL compiles its effective
methods. `compile-form` is already re-entrant, so a compilation started from
inside another (a dispatch during macro expansion) is safe today. It is not
safe for two processes at once, but neither is any other compilation, and
the processes step already makes the compiler's state per-process (decided
in #111), which covers this case with the rest. Standard and the built-in operator
combinations never compile; a short form whose operator is a macro or a
special operator other than `and`, `or` and `progn` does, the same way.

### Conditions in this step

A condition is still the Rust `ObjectType::Condition` (a type symbol, a
message and data) during this step, and `handler-case` still matches through
`%condition-supertypes`. But goal 2 says every object has a class, and a
condition is the object every error makes, so the standard condition types
become classes now, with the shape the next step will keep:

- Their metaclass is `condition-class`, as in SBCL, where conditions are not
  standard objects: `condition` is not a subclass of `standard-object`, and
  `make-instance` refuses a condition class (`make-condition` is the next
  step's).
- The hierarchy is the one `%condition-supertypes` lists, defined once, as
  classes; `%condition-supertypes` is then computed from the classes, or
  replaced by `subtypep`, so that the lattice exists in one place.
- `class-of` of a Rust condition object is the class its type symbol names;
  `typep` and `subtypep` work on condition types; a method can specialise on
  `error` and is selected for a Rust condition.

The next step replaces the Rust object with instances of those same classes
and removes `ObjectType::Condition`. The classes do not change, so nothing is
designed twice.

### The printer, `describe`, and speed

`print-object` is a generic function with a method for `t` that does what the
printer does today, and methods for `standard-object` (`#<POINT {id}>`, as
`print-unreadable-object` writes it) and `structure-object` (the `#S` syntax,
or the `:print-function`/`:print-object` a `defstruct` named). The printer
calls `print-object` only for objects whose class may have a method the
printer does not already know: standard objects, funcallable instances,
structures and conditions. For a cons, a symbol, a number or a string it goes
straight to its own code unless a program has defined a `print-object` method
specialised on a built-in class, which CLHS allows, so the printer keeps a
flag that `defmethod` sets. This is the measure that keeps printing as cheap
as it is now.

`describe` calls `describe-object`, a generic function whose methods print
what SBCL's print, in outline.

### Redefinition

`defclass` on an existing class keeps the class object. If the slots changed,
the class gets a new wrapper and the old one becomes obsolete, as above;
subclasses are finalised again and their wrappers replaced too.
`make-instances-obsolete`, `update-instance-for-redefined-class`,
`change-class`, `update-instance-for-different-class` and
`reinitialize-instance` are all in.

`defmethod` of a method with the same specializers and qualifiers replaces it.
`defgeneric` on an existing generic function keeps the object and removes the
methods the previous `defgeneric` form defined, as CLHS says.

### The warm rebuild

A warm rebuild (`%rebuild-world`, chapter 41) promises what it promises
today: the library is recompiled from its files and replaces itself in a
world that holds the library. What user code defined is not covered now and
is not covered with CLOS either; a user class or method that survives does so
by accident. Within the library:

- The hand-built bootstrap classes are rebuilt in place, as
  `%install-layout` rebuilds a structure's layout: the objects keep their
  identity, their slots are refilled.
- Every library `defclass`, `defgeneric` and `defmethod` runs again, keeping
  class and generic function objects and replacing method objects.
- The rebuild's check, that no `defun` or `defmacro` name still holds its old
  function, would flag every generic function, which keeps its identity on
  purpose. It becomes: a generic function is the same object, and every
  method the library defines on it is a new method object. The test that
  asserts the rebuild's `(:FORMS … :REPLACED …)` counts changes with it.
- A library class whose slots are unchanged keeps its wrapper, so no
  instance in the world goes obsolete merely because the world rebuilt
  itself.

### MOP introspection

In package `QUOKKA-MOP`: `class-name`, `class-direct-superclasses`,
`class-direct-subclasses`, `class-precedence-list`, `class-direct-slots`,
`class-slots`, `class-default-initargs`, `class-finalized-p`,
`finalize-inheritance`, `slot-definition-name`, `-initargs`, `-initform`,
`-initfunction`, `-allocation`, `-type`, `-readers`, `-writers`,
`-location`, `generic-function-name`, `-methods`, `-lambda-list`,
`-method-combination`, `method-specializers`, `method-function`,
`method-generic-function`, `eql-specializer-object`, `intern-eql-specializer`,
`set-funcallable-instance-function`. The standard names among them
(`class-name`, `find-class`) are in `COMMON-LISP` as CLHS lists them.

Built in sub-step 7 (`lisp/library/mop.lisp`), with the metaobject classes
and a few more of SBCL's `SB-MOP` names: `class-direct-default-initargs`,
`class-prototype`, `generic-function-argument-precedence-order`,
`-declarations`, `-method-class`, `method-lambda-list`,
`accessor-method-slot-definition`, `specializer-direct-methods`,
`specializer-direct-generic-functions`, `find-method-combination`,
`extract-lambda-list`, `extract-specializer-names`, `ensure-class`,
`standard-instance-access`. The readers are ordinary functions, since a
program may look but not intercede.

## Testing

- **Differential files** against SBCL, compiled only, in `tier1_cases!`
  (the mechanism is the same though CLOS is tier 2), one per sub-step at
  least: classes and initialisation, dispatch and method combination,
  redefinition, printing, types. They must avoid what SBCL leaves
  unspecified, such as the text of `#<...>` (SBCL prints an address).
- **ansi-test**: `tests/ansi/deferred.lisp` loses the `tier-2:clos` and
  `phase-3:types` lists as each part lands; the `objects` and
  `types-and-classes` groups join the runner; the baseline ratchets up.
  Tests that also need conditions or restarts stay explained by
  `tier-2:conditions`.
- **When the step is done** the bar is phase 2's: in the `objects` and
  `types-and-classes` groups, and in the fifteen groups already measured,
  every test that does not pass is explained by a deferred feature
  (conditions, restarts, streams, complex numbers, a single-float format) or
  by SBCL agreeing, with no plain failure, escape or timeout. The types
  sub-step meets that bar for `types-and-classes` before the CLOS sub-steps
  start; `objects` meets it at the end.
- **Rust**: unit tests for the new heap type (allocation, collection, image
  round trip, `Heap::verify`), and `no_primitive_panics_on_any_argument`
  covers the new primitives without being told.
- **The fixpoint** and `a_rebuilt_world_is_the_same_world` keep passing,
  with the rebuild's form count updated.

## Order of work

Each sub-step is a commit, after `./scripts/check.sh` is green, and the whole
is one GitHub issue under phase 3's tracking issue.

1. Types: `type-of`, `typep`, `subtypep`, `deftype`, the `typecase` family,
   `check-type`.
2. Funcallable instances in Rust; the bootstrap of the class hierarchy;
   `defclass`, `make-instance`, initialisation, slot access, `class-of` for
   every object, `find-class`.
3. Generic functions: `defgeneric`, `defmethod`, dispatch with its cache,
   standard method combination, `call-next-method`, `eql` specializers.
4. The printer and `describe` through `print-object` and `describe-object`;
   structure classes for every `defstruct`; `with-slots`, `with-accessors`.
5. Redefinition: obsolete instances, `change-class`, `reinitialize-instance`.
6. Built-in method combinations and `define-method-combination`, short form
   then long.
7. MOP introspection in `QUOKKA-MOP`; ansi-test's `objects` group measured
   and its non-passing tests explained.

## The rest of phase 3, for context

After CLOS, in this order (each its own issue, with options written out
before it starts): conditions as classes, with `handler-bind`, `signal` that
does not unwind, restarts, and the machine's errors signalled through Lisp;
definitions that keep their source (#55); frames that keep their arguments
under a debug setting (decided in #111); processes and per-process bindings
(#34), with the compiler's state as per-process special variables (decided in
#111); the terminal debugger; then the gate.

## Left to the builder

- The cache's shape (an `eq` hash table keyed on a list of classes, or one
  table per argument position), measured against the step budgets.
- Whether `:type` on a slot is checked. Follow SBCL at its default policy, as
  measured, and keep it out of differential files if SBCL's behaviour
  depends on policy.
- The exact output of `describe`, which CLHS leaves to the implementation and
  which differential files therefore must not test.
