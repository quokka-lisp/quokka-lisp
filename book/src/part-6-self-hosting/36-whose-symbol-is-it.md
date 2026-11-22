# Whose symbol is it?

> **Where this fits.** Until now Quokka had one namespace: a symbol called `FOO`
> was *the* symbol called `FOO`, everywhere. Chapter 35 gave it special
> variables, and with them `*package*`. This chapter gives it **packages** —
> Common Lisp's way of letting two programs each have a `FOO` — built as
> chapter 34's plan said, in Lisp, in the tier-1 library. It comes before the
> reader and printer move into Lisp, because reading and printing `pkg::foo`
> both depend on it.

## What a package is

A **package** is a named table of symbols. Two packages can each contain a
symbol called `FOO`, and they are two different symbols: `geometry::area` and
`cl-user::area` have the same name and nothing else in common.

Each package divides its symbols in two:

- **external** symbols are its public interface, written `geometry:area` with
  one colon;
- **internal** symbols are the rest, reachable with two colons,
  `geometry::helper`, which is the programmer saying "I know this is not
  meant for me".

A package can **use** other packages, which makes their external symbols
visible in it without any prefix — a symbol found that way is **inherited**.
`COMMON-LISP-USER`, where code starts out, uses `COMMON-LISP`, which is why
`car` means Common Lisp's `car` without anyone writing `cl:car`.

And `*package*` — a special variable, which is why chapter 35 came first —
says which package is *current*: the one the reader puts new symbols into, and
the one the printer leaves prefixes off for.

Most functions that want a package accept a **designator** instead: a string
is its own name, a symbol stands for its name, so `:cl`, `cl` and `"CL"` all
name the package whose nickname is `CL`.

## The decision, and what it avoided

Chapter 34 left packages as an open question, and four answers settled it
(#41). The interesting one was **where packages live**.

Quokka's symbols have always been interned by Rust: the Rust reader reads
`car`, asks Rust's `intern` for the symbol named `CAR`, and gets it from one
flat list of every symbol there is — the *system table*. The obvious way to add
packages is to make that Rust code package-aware: a package structure in Rust,
`intern` taking a package, the reader understanding `in-package`, the printer
deciding prefixes.

It would have cost several hundred of the 1,754 Rust lines left for five phases
of work, and it would have put a structure users routinely change — packages
are created, extended and inspected by ordinary programs — below the primitive
table, against rule 1.

The alternative, and the decision: **packages are Lisp objects, in the
library.** Rust keeps its flat list exactly as it was, and with it the whole
bootstrap: the interpreter, which is tier 0, never learns that packages exist.
Then, as the library's very last act, every symbol in the Rust table is
**adopted** into a package.

## Adoption

`lisp/library/packages.lisp` builds the four standard packages —
`COMMON-LISP` (nickname `CL`), `KEYWORD`, `QUOKKA`, and `COMMON-LISP-USER`
(nickname `CL-USER`) — and then sorts the system table into them:

- every name among the **978 symbols the standard puts in `COMMON-LISP`** goes
  there, as an external symbol. The list is a data file generated once from
  SBCL — a name is a fact, and SBCL is already the oracle. Quokka has not
  needed all 978; the ones it has not, such as `LOOP`, are created now, so that
  `(find-symbol "LOOP" "CL")` finds a symbol as it does in every Common Lisp;
- everything else — the 89 `%`-names, the compiler's functions, the library's
  helpers — goes into **`QUOKKA`**, as internal symbols. One internal package,
  not one per subsystem: simpler, and one prefix to know when debugging. The
  `%` prefixes stay, because they still say *implementation, not API* at a
  glance, as they do inside SBCL's own packages;
- keywords are already `KEYWORD`'s.

Adoption sets each symbol's **home** — the slot that says where it belongs.
The design said long ago that, when packages arrived, the home would stop
being a small integer and become a pointer to a package. That is true of every
symbol but one kind:

| home | meaning |
| --- | --- |
| `nil` | interned nowhere — `gensym`, `make-symbol` |
| `0` | in the system table, not yet adopted |
| `1` | a keyword |
| a package | adopted into that package |

**Keywords keep their small integer**, and the reason is worth a sentence,
because #41 had said otherwise. Rust has to recognise a keyword — to answer
`keywordp`, to print the colon, because a keyword's value is itself — and it
would have to do that without knowing what a Lisp package object looks like.
The integer lets it. The `KEYWORD` package is implied by it, and the Lisp side
maps one to the other.

## The core of the package system

The library implements what #41 called the useful core: `make-package`,
`find-package`, `package-name`, `package-nicknames`, `package-use-list`,
`list-all-packages`, `intern`, `find-symbol`, `export`, `use-package`,
`symbol-package`, `in-package` and `defpackage`.

`find-symbol` shows the structure most clearly — look among the externals,
then the internals, then the externals of every used package:

```lisp
(defun %find-in (package name)
  (multiple-value-bind (symbol present) (gethash name (%package-externals package))
    (if present
        (values symbol :external)
        (multiple-value-bind (symbol present) (gethash name (%package-internals package))
          (if present
              (values symbol :internal)
              (multiple-value-bind (symbol present) (%find-inherited name (%package-uses package))
                (if present
                    (values symbol :inherited)
                    (%find-unadopted package name))))))))
```

The second value — `:external`, `:internal`, `:inherited`, or `nil` — is part
of the standard's interface, and the tests check it against SBCL as carefully
as the symbol itself.

A keyword made by either side has to be one object, so the `KEYWORD` package
has no tables of its own: `intern` and `find-symbol` there go straight to
Rust's keyword namespace.

Everything the standard also has — `shadow`, `import`, `unexport`,
`rename-package`, `delete-package`, the `do-symbols` family, name-conflict
checking, package locks — arrives when a test or the library needs it.

## The gap until the Lisp reader

There is one awkward fortnight in this design, and it is better to name it than
to hide it. The compiled world still *reads* with the Rust reader until step C
replaces it, and the Rust reader knows nothing about `*package*`. A symbol it
reads after adoption lands in the system table with home `0`, in no package at
all.

So `%find-unadopted`, at the bottom of `%find-in`, closes the gap from the
other side: a symbol like that, looked for in `COMMON-LISP-USER`, is adopted
there — which is exactly what the Lisp reader will do when it reads in
`CL-USER`. `symbol-package` does the same. And `in-package` sets `*package*`
correctly but does not yet change where the Rust reader puts things. When the
Lisp reader arrives, the gap closes for good.

## Two things found on the way

**Quokka had no `eql`.** The library reached for it, and the build stopped:
*the function EQL is undefined*. Every comparison in the system had used `eq`.
Until there are bignums and floats, every number is an immediate fixnum and the
two give the same answer, so the library uses `eq` for now; `eql` comes with
bignums, which is where it starts to mean something different.

**The image grew fivefold.** Adopting 978 standard names created about seven
hundred symbols Quokka had never needed, each with its name stored as a string
— and Quokka stores a string at **one word per character**, eight bytes for
every letter, a simplification `heap.rs` made on purpose "until something
measures that it matters". The image went from 114 KB to 592 KB. Dropping the
name list once adoption was done brought it back to 478 KB, and the rest is
mostly names. That is the measurement the comment was waiting for, so it has
its own decision issue, #44: pack the characters, compress the image file,
or decide it does not matter yet. Chapter 37 is how that went; the image
ended at 254 KB.

## The test

`packages.lisp` is the second tier-1 differential file: compiled Quokka against
SBCL, with SBCL's output as the baseline. It finds packages by name and
nickname, asks where symbols live, checks `find-symbol` and `intern` with their
second values, makes packages, exports, uses, nicknames, `in-package` there and
back — and two errors. What it never does is *print* a symbol from another
package, or a package itself: that needs to know about prefixes, which is the
printer's job, and the printer is not in Lisp yet.

All forty-six forms agreed with SBCL on the first run. The step cost 108 lines
of Rust — the split of `intern` into finding and making, the home's documented
union, and six small primitives — leaving 1,646.

## What is not here

- **Reading** `pkg:sym` and `pkg::sym` — the Lisp reader (chapter 38) reads
  them; the system reads its own source with it from step D.
- **Printing** them — step G, the Lisp printer.
- **A `package-error` condition type.** Package errors are signalled, as simple
  errors; the differential format compares only that an error happened.
- **Everything on demand**, listed above.

---

**References.** CLHS chapter 11, *Packages*, and 11.1.1 on how a symbol is
looked up. Steele, *Common Lisp the Language*, 2nd edition, chapter 11, for the
same with the history of why packages work the way they do.
