# The names with a percent sign

> *Where this fits.* Reading the prelude, you keep meeting names like
> `%set-aref` and `%macro-function`, and nothing has explained them. This
> chapter is that explanation. By the end you will know what the prefix means,
> what each of the thirty-one names is doing there, and why a back door that
> size is a sign of something missing rather than something clever.

## What the prefix does and does not mean

They are not special forms. That is the natural guess, since they look
reserved and they appear in the prelude and not in anybody's program, so they
must be something the evaluator knows about. Almost none of them is. The
prefix says nothing at all about *what kind of thing* a name is. It says one
thing only, which is that this is Quokka's rather than Common Lisp's, and you
should not write it in a program.

Underneath that single meaning sit four quite different kinds of thing.

| Kind | How many |
| --- | --- |
| Primitives, written in Rust | 15 |
| Functions and macros written in Lisp, in the prelude | 15 |
| Special operators | **1** |
| Symbols with names nothing can type | 3 |

So one of the thirty-one `%` names is a special operator. The rest are
ordinary functions; it is just that half of them happen to be implemented in
Rust and half in Lisp, and from a program's point of view there is no
difference, since `(aref v 0)` and `(elt v 0)` are both just calls and only
one of them is a primitive.

## Why the convention exists at all

Rule 6 says:

> Quokka-specific extensions go in their own packages, never in
> `COMMON-LISP`.

That is the right rule and Quokka cannot follow it, because it has no
packages. Packages are tier 1, phase 2, and chapter 8's keyword namespace is
deliberately two-sevenths of one (see issue #20 for why that was enough).

In a real Common Lisp, every one of these names would be a package-internal
symbol. SBCL writes `sb-impl::%make-array`, and the double colon is the
language itself saying *you are reaching inside*. Quokka has one namespace for
everything, so the warning has to live in the name. `%` is a convention where
there should be a mechanism, and it is enforced by nothing: a program that
calls `%set-aref` works, it will break when packages arrive, and nothing will
have warned it.

## The fifteen primitives

Grouped by *why* they exist, which is more useful than listing them
alphabetically. None of them is here because somebody preferred Rust.

### Writers that `setf` expands into

| | |
| --- | --- |
| `%set-symbol-function` | puts a function in a symbol's function cell |
| `%set-aref` | writes a vector or string element |
| `%set-gethash` | writes a hash table entry |

Common Lisp has no name for any of these. Writing a vector element is
`(setf (aref v i) x)`, and `(setf aref)` is not a symbol you can define a
function on. It is a *place*, and `setf` is a macro that has to expand into
something. These are the something.

Chapter 19 tells the story of what that cost: until `setf` existed, these were
the only way to write anything, and they are invisible to SBCL, so two whole
features were untestable against the standard. That is the single strongest
reason this list should be short.

### Things Common Lisp spells with arguments Quokka cannot express

| | Common Lisp | What is missing |
| --- | --- | --- |
| `%make-vector` | `make-array` | dimension lists, `:element-type`, fill pointers |
| `%make-hash-table` | `make-hash-table` | a primitive cannot take `&key` |
| `%intern` | `intern` | takes a **package** |
| `%string-append` | `concatenate` | takes a **sequence type** |
| `%vector-to-list` | `coerce` | takes a **type** |

Three of those are waiting on the same thing: Common Lisp's name takes a
*type* or a *package* as an argument, and Quokka has neither a type system nor
packages. `(concatenate 'string a b)` cannot be written when `'string` means
nothing. So the narrow version gets a `%` and the general one waits.

The first two are a different problem. A primitive's arguments are positional,
and `make-array` and `make-hash-table` are defined in terms of keywords, so
the prelude wraps each in a real function with a real `&key` lambda list,
which is how `(make-array 3 :initial-element 'x)` works.

### Things whose Common Lisp counterpart has a *different contract*

| | Why not the standard name |
| --- | --- |
| `%macro-function` | CL's returns a function of two arguments — the form and an environment. This returns the macro's own closure. |
| `%gethash` | CL's returns **two values**: the value, and whether the key was present. Quokka has no multiple values. |
| `%sequence-length` | Only handles vectors and strings; the list case is in Lisp, and `length` dispatches. |

This is the group where taking the standard name would have been worse than
not having it. A `macro-function` that returns the wrong shape of thing is a
trap, because code written against Common Lisp would compile, run, and do
something subtly wrong. A name with a `%` on it cannot be mistaken for the
standard's.

### Things that need the class system

| | |
| --- | --- |
| `%condition-type` | CL uses `type-of` and the class hierarchy |
| `%condition-message` | CL uses readers like `simple-condition-format-control` |
| `%conditionp` | CL uses `typep` against the `condition` class |

All three are placeholders for phase 3. And one of them has no caller at all,
which writing this chapter is what found out, and which is a decent argument
for writing it.

### One that exists because calling is the machine's job

`%hash-table-entries` hands back a list of `(key . value)` cells, and the
prelude's `maphash` does the calling. A primitive cannot call a function
(chapter 15), so the primitive returns data and Lisp iterates over it. The
same division as `macroexpand`.

## The one special operator

`%handler-case` (chapter 20).

`handler-case` **is a macro** in Common Lisp, over `handler-bind` and `block`.
Quokka has no `handler-bind`, so the machinery of pushing a frame that a
signal can unwind to has to be an operator. But calling that operator
`handler-case` would have made chapter 13's "twenty-five special operators" a
lie, so the operator is `%handler-case` and the name people write is a
one-line prelude macro over it. The count of special operators is a claim
about the language, and a language that claims twenty-five and has
twenty-six is describing itself wrongly.

## The fifteen in the prelude

These are not Rust at all. They are ordinary Lisp functions that happen to be
the insides of something:

- `setf`'s expander: `%setf-place`, `%setf-through`, `%setf-unknown-place`,
  `%define-setf-writer`, and the table `%setf-setters`.
- `defstruct`'s code generator: `%slot-name`, `%slot-default`,
  `%map-slot-names`, `%symbol-concat`, `%defstruct-constructor`,
  `%defstruct-predicate`, `%defstruct-copier`, `%defstruct-accessors`.
- Two error reporters, `%destructuring-too-short` and
  `%destructuring-too-long`, called from code the lambda-list rewriter
  generates (chapter 18).
- One tree walk, `%mentions-return-from`, which decides whether `defun` needs
  an implicit block (chapter 16).

Every one is a helper for a macro. In a real Common Lisp they would be
internal symbols in the implementation's package and a program would never see
them; here they sit in the same namespace as `car`, distinguished only by
spelling.

## The three you cannot type

A fourth category, and the most interesting one, because these are hidden by a
mechanism rather than a convention:

| | Where | Chapter |
| --- | --- | --- |
| `UNQUOTE MARKER` | the reader, between reading `,` and handling the list | 15 |
| `SPLICE MARKER` | the same, for `,@` | 15 |
| `UNSUPPLIED MARKER` | lambda lists, meaning "no argument was given" | 18 |

Each name **contains a space**. Quokka's reader has no `|bar quoting|`, so a
space ends a token, and therefore *no input can produce these symbols*. They
are not hidden by a naming convention anybody could ignore. They cannot be
written.

That difference matters. `%set-aref` is a request; `UNSUPPLIED MARKER` is a
guarantee. The first relies on a programmer reading a prefix and deciding to
behave; the second relies on the reader being unable to make the symbol at
all. Where correctness depends on the distinction, as it does when an
unsupplied argument must be told apart from `nil` and a comma marker must not
be forgeable, the convention was not good enough and the guarantee was used
instead. A rule that can be followed needs discipline, and a rule that cannot
be broken needs none.

## What the convention costs, and when it goes

Three costs, in order of how much they matter.

1. Each one is a hole in the differential tests. SBCL has no `%set-aref`, so a
   behaviour reachable only through one is not *different* from the oracle's,
   it is inexpressible to it, and has to fall back on Rust tests that check
   Quokka against its author's expectations. Chapter 19 describes pulling
   `setf` forward from tier 1 for exactly this reason.
2. It is enforced by nothing. A program that calls these works today and
   breaks later.
3. It is a flat namespace pretending to be a structured one. Three genuinely
   different kinds of thing wear the same prefix, and the prefix cannot tell
   you which.

All three go away at the same moment, in phase 2, when packages arrive (#20).
`%set-aref` becomes `quokka-internal::set-aref`, the double colon is the
language saying what the percent sign is only hinting, and the list in this
chapter becomes a package export list, which is a thing a machine can check.

Until then, a name in Quokka that starts with `%` is scaffolding. It is honest
scaffolding, it is written down here, and none of it is meant to survive
contact with a package system.

*Later: packages arrived and the prefix stayed. The symbols moved, as
predicted: there are four packages now, and everything that is not one of the
standard's 978 names goes into a single internal one called `QUOKKA`, so the
flat namespace and the lack of enforcement are both gone.
[Whose symbol is it](../part-6-self-hosting/36-whose-symbol-is-it.md) is that
work. But `%set-aref` is still called `%set-aref`, rather than becoming
`set-aref` inside its package, because the prefix turned out to be doing a
second job this chapter did not notice: inside a package it still says
*implementation, not API* at a glance, which is exactly why SBCL writes
`sb-impl::%make-array` with both the package and the percent sign. The third
cost was real and the convention was not only a stand-in for it. The first
cost has not gone and will not: a name SBCL does not have stays inexpressible
to the oracle whatever package it lives in.*

---

**References.** CLHS 11 for packages, and 11.1.2.1.2 for why binding
`COMMON-LISP` symbols is undefined behaviour, which is the rule `%` exists to
keep. The SBCL source, for how a real implementation does this with
`sb-impl::` and `sb-int::`; the convention there is exactly the same, with a
mechanism behind it.
