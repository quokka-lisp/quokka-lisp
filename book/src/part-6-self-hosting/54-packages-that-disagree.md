# When two packages disagree

> *Where this fits.* Chapter 36 gave Quokka packages: named tables of
> symbols, so that two programs can each have a `FOO`, with external and
> internal symbols and packages that use others. It built what it called the
> useful core and left the rest for when something asked. ansi-test (chapter
> 49) asked: after chapters 52 and 53, the package functions were the
> biggest group of gaps left. This chapter adds the rest of chapter 11 of the
> standard: what happens when two symbols of one name meet, how a package
> says in advance which one wins, how a package is deleted or renamed, and
> how a program walks the symbols in one.

*Status: done, issue #66. Step S of phase 2.*

## The missing operators

While running the tests we found that fifteen operators were still missing
from the library, especially `delete-package` (202 tests),
`package-used-by-list` (171) and `unuse-package` (170). Those three arrive together for a reason
that has nothing to do with what the tests check. Almost every package test
starts by cleaning up after the last one, with a helper from ansi-test's
auxiliary files:

```lisp
(defun safely-delete-package (package-designator)
  (let ((package (find-package package-designator)))
    (when package
      (let ((used-by (package-used-by-list package)))
        (dolist (using-package used-by)
          (unuse-package package using-package)))
      (delete-package package))))
```

So 85 tests needed exactly those three and nothing else, and none of them
could even begin before all three existed. Behind them were the operators
that remove things from packages (`unintern`, `unexport`), the ones that
deal with conflicts (`shadow`, `shadowing-import`, `import`), renaming, the
iteration macros (`do-symbols`, `do-external-symbols`, `do-all-symbols`,
`with-package-iterator`, `find-all-symbols`), and four options of
`defpackage`.

## What is a name conflict?

A symbol is **present** in a package when the package's own tables hold it,
and **accessible** when it is present or inherited from a package it uses.
The one rule the package system exists to keep is that a package never has
two different symbols of the same name accessible at once, since a name read
in that package would then mean two things.

Every operator that makes a symbol accessible can break the rule. Suppose
package `PS-A` exports `SHARED`, package `PS-B` exports a different
`SHARED`, and `PS-C` uses `PS-A`. If `PS-C` now uses `PS-B` as well, the
name `SHARED` in `PS-C` has two meanings. `import` can do the same, and so
can `export`, by making a symbol visible in every package that uses its own,
and even `unintern`, which we come to below. The standard asks each of
these to check first, and on a conflict to signal a **correctable** error, a
`package-error` with restarts that let the person at the keyboard choose
which symbol wins.

Quokka has no restarts until tier 2 brings the rest of the condition
system. We could have skipped the check, as chapter 36 did, and let the last
symbol in win silently. We chose to check, and to signal a plain
`package-error` with the package left exactly as it was. Each operator looks
at every symbol it would make accessible before it changes anything:

```lisp
(dolist (symbol (%external-symbols used))
  (multiple-value-bind (found status) (find-symbol (symbol-name symbol) user)
    (if (and status (not (eq found symbol))
             (not (member found (%package-shadowing user))))
        (%package-error user "using that package would make two symbols of one name accessible"))))
```

This is `use-package`'s check: for each external symbol of the package being
used, is a different symbol of the same name already accessible here?

## How does a package choose in advance?

The `(not (member found (%package-shadowing user)))` in that check is the
other half. Each package keeps a list of **shadowing symbols**, symbols it
has declared will win any conflict over their name. `(shadow "SHARED"
"PS-C")` makes a new `SHARED` present in `PS-C` and puts it on the list, and
after that `PS-C` can use `PS-B` without complaint: its own `SHARED` hides
both inherited ones. `shadowing-import` does the same with an existing
symbol from elsewhere, displacing whatever symbol of that name was present.
`defpackage` has options for both, `:shadow` and `:shadowing-import-from`,
and applies them before `:use`, whatever order they are written in, so that
the conflicts they are there to prevent never happen.

`unintern` is the surprising case. Taking a symbol away seems unable to make
two symbols accessible. But if the symbol was a shadowing symbol, it was
hiding something, and if it was hiding two different inherited symbols of
its name, removing it exposes the conflict it was put there to prevent. So
`unintern` checks too.

## Which packages use this one?

`package-used-by-list` asks the opposite question to `package-use-list`. We
proposed computing it by scanning every package's use list, which keeps
nothing extra in step and costs little when there are a handful of packages.
We chose to keep a second slot, `used-by`, as most implementations do, so
that the answer is a slot read. `use-package` and `unuse-package` now change
both lists, the user's `uses` and the used package's `used-by`, and nothing
else writes either.

## What happens to a deleted package?

Here the standard leaves room, and we followed SBCL. `delete-package` refuses a package another still uses, with
a `package-error`. Otherwise the package stops using its own, leaves the list
that `find-package` searches, and its name and nicknames are cleared, so
`package-name` of the object answers nil and deleting it again answers nil.
Each symbol whose home it was loses its home, and from then on prints with
`#:`, like a symbol that was never interned. The standard lets an
implementation do this and does not require it. We refuse to delete
`COMMON-LISP`, `KEYWORD` and `QUOKKA`, the packages the system itself lives
in; SBCL refuses the first two through a package lock, a mechanism Quokka
does not have.

## How do you walk the symbols in a package?

Every iteration operator first makes a list of the symbols it will visit and
then walks the list. `do-symbols` is a `dolist`:

```lisp
(defmacro do-symbols ((var &optional (package '*package*) result) &body body)
  `(dolist (,var (%accessible-symbols ,package) ,result) ,@body))
```

Walking the hash tables directly would cons less, but a body that interned
or uninterned a symbol would then be changing a table in the middle of being
walked. The standard leaves that undefined; with a list made first, it is
harmless.

`with-package-iterator` is the odd one. Inside its body, `(next)` returns
four values for each symbol in turn, true, the symbol, its status and its
package, and nil at the end. The body may call `(next)` anywhere, as often
as it likes, so it cannot be a loop around the body. It is a local macro
(chapter 44's `macrolet`) whose expansion takes the next entry from a list
made at the start:

```lisp
(let ((,entries (%iterator-entries ,packages ',types)))
  (macrolet ((,name () (list '%iterator-step ',entries)))
    ,@body))
```

The keyword package needed care everywhere. Its symbols live in Rust's
system table and are keywords by their home (chapter 36), not in the
package's own hash tables, so every operator that lists a package's symbols
lists `KEYWORD`'s from that table instead.

## What did the measurement find?

The first ansi-test run after this step showed five package tests running
out of their step budget and two returning `:BAD`, all in the iteration
macros. The timeouts were ours. A package that uses `COMMON-LISP` inherits
978 symbols, and `%inherited-symbols` made sure each appeared once by
searching the list built so far with `member`, which is about half a million
comparisons for one `do-symbols`. An `eq` hash table, which chapter 52 had
just made able to hold any key, does the same in one lookup per symbol.

The two `:BAD`s were older than this step. Both tests put `(declare (special
x))` at the start of the loop body and expect the result form `x` to read
the special variable:

```lisp
(let ((x :good))
  (declare (special x))
  (let ((x :bad))
    (do-symbols (s "CL-TEST" x)
      (declare (special x)))))
```

`do-symbols` is a `dolist`, and `dolist`, `dotimes`, `do` and `do*`
(chapter 45) all put their body inside a `tagbody`, declarations included.
There a `declare` is only a call to the macro that expands to nil, so the
declaration said nothing, and the result form read the lexical `x`. The four
macros now lift a body's leading declarations onto the bindings they make,
where the compiler reads them and where they cover the result form too.

## What did it cost?

This step needed no Rust at all, only about 400 lines of Lisp in
`packages.lisp`, which also gained
`gentemp`, character names as string designators, and `defpackage`'s
`:import-from`, `:documentation` and `:size`. `tests/lisp/package-system.lisp`
checks the conflicts, shadowing, importing, uninterning, deleting, renaming
and iterating against SBCL, comparing names and statuses rather than
printing package objects, whose printed form the standard leaves to each
implementation.

Measured with steps Q and R, ansi-test went from 5,382 passing tests to
5,701 of 12,324. The packages group went from 55 passing to 225, and the
iteration group, after the fix to the loop macros above, from 81 to 95 with
none failing. Most of the 44 package tests that still fail make strings with
`make-array` and `:fill-pointer` or `:element-type`, which Quokka's vectors
do not have yet.

## What is still missing?

Several things are still missing. A conflict should offer restarts, which
need tier 2's conditions. There are no package locks, and no nicknames local
to a package, an extension SBCL and others share but the standard does not
have. A `package-error` does not yet say which package
it is about in a way a program can ask, because `package-error-package`
needs conditions with slots, also tier 2.

---

**References.** The Common Lisp HyperSpec, chapter 11 (*Packages*), and
11.1.1.2.5 (*Prevention of Name Conflicts in Packages*) in particular. Guy
Steele, *Common Lisp the Language*, 2nd edition, 1990, chapter 11, whose
account of the package system the standard follows.
