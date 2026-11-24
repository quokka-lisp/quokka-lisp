# Reading, in Lisp

> **Where this fits.** Chapter 8 built the reader in Rust: text in, lists and
> symbols out. Chapter 36 gave Quokka packages, and with them a gap. The Rust
> reader knows nothing of `*package*`, so it cannot read `geometry::area`, and
> every symbol it makes lands in one flat table. This chapter writes the reader
> again, in Lisp, in the tier-1 library. It brings two pieces of Common Lisp
> with it, **streams** and **readtables**, and one test that only makes sense
> for a system rebuilding itself: the new reader must read Quokka's own source
> exactly as the old one did. What it unlocks is step D, `eval` and `load`, where
> the Lisp reader starts reading programs, and through that the fixpoint.

*Status: done, issue #45. Since step D (chapter 40) a built image reads
everything with it; the library is still read by the Rust reader during the
build, until step E.*

## Why write a reader twice

The Rust reader works. It has read every line of Quokka since chapter 8. But
three things are wrong with keeping it.

- **It cannot learn packages without the Rust side learning them.** Packages
  are Lisp objects (chapter 36). A Rust reader that resolved `cl:car` would
  have to call into Lisp halfway through reading a token, or Rust would need
  its own copy of the package tables.
- **It cannot be changed by a program.** Common Lisp lets a program teach the
  reader new syntax, as the next sections show. A reader in Rust is fixed
  when Quokka is compiled.
- **Rule 1.** Rust provides the machine. Anything a user could want to change
  belongs in Lisp, and the reader is the standard example of something users
  change.

The Rust reader is not deleted. The bootstrap still needs it, because something
has to read the prelude before any Lisp can run. What changes is who reads
*after* the bootstrap.

## Two decisions

Two choices had to be made before writing anything, and both were ours.

**Where does `read` get its characters?** In Common Lisp, from a **stream**: an
object that hands out characters one at a time and remembers how far it has
got. `read-from-string` makes a stream over a string and reads from that.
Streams were planned for tier 2, alongside files and CLOS, and #42 asked
whether tier 1 should have them at all. The options were:

- **No streams yet.** The reader takes a string and an index. That is less
  code now, but `read`'s argument list changes for every caller when streams
  arrive.
- **Minimal streams.** A string input stream is a Lisp structure holding a
  string, a position and an end, with the standard functions over it:
  `read-char`, `peek-char`, `unread-char`, `make-string-input-stream`,
  `with-input-from-string`.
- **Full streams.** This needs generic functions, which are CLOS, which is
  tier 2.

Minimal streams won. They cost about forty lines of Lisp, and every function
already has its final name and arguments. When tier 2 makes streams generic,
callers do not change. Output streams came with the printer (chapter 39).

**How does the reader decide what a character means?** The Rust reader is a
`match` on the next character: `(` reads a list, `"` a string, `'` a quotation.
Common Lisp instead puts that decision in a table, the **readtable**. It maps
characters to **reader macro** functions, and the reader calls the function it
finds. `(` is not built into the reader. It is a function in the readtable that
reads a list. A program can add its own:

```lisp
(set-macro-character #\[
  (lambda (stream char) (cons 'bracket (read-delimited-list #\] stream t))))
```

After that, `[1 2 3]` reads as `(BRACKET 1 2 3)`.

`#` is a **dispatching** macro character: it reads one more character and looks
*that* up in a second table. So `#\a` is a character, `#'f` a function, `#(1 2)`
a vector, and a program can add `#?` with `set-dispatch-macro-character`.

The choice was between building the readtable now and a fixed `cond` now, with
readtables later. The readtable cost about sixty more lines. Doing it later
would have meant rewriting the reader's dispatch and every macro character
with it. It went in now.

## The algorithm

CLHS section 2.2 describes reading as a loop over characters. With the
readtable in place it is short enough to quote nearly whole:

```lisp
(defun %read (stream eof-error-p eof-value)
  (let ((char (%skip-whitespace stream)))
    (if (null char)
        (%end-of-file eof-error-p eof-value)
        (let ((found (%read-maybe-nothing stream char)))
          (if found (car found) (%read stream eof-error-p eof-value))))))

(defun %read-maybe-nothing (stream char)
  (let ((entry (gethash char (%readtable-macros (%the-readtable *readtable*)))))
    (if entry
        (multiple-value-list (funcall (car entry) stream char))
        (list (%read-token stream char)))))
```

The interesting line is `multiple-value-list`. A reader macro may return **no
values at all**, and that is how a comment works. `;` is a reader macro that
skips to the end of the line and returns `(values)`, nothing. The reader sees
an empty list and goes round again. A comment is not a special case in the
reader. It is a macro character whose function produces nothing.

Everything that is not whitespace or a macro character begins a **token**: a
run of characters up to the next whitespace or *terminating* macro character.
`)` is terminating, so `foo)` is the token `foo` followed by a close. `#` is
not, so `a#b` is one token. A token then becomes a number if it looks like one
and a symbol otherwise.

## Tokens, case and packages

Collecting a token does three jobs at once, character by character:

- **Case.** Unescaped letters are upcased, so `foo`, `Foo` and `FOO` are one
  symbol. Common Lisp calls this the readtable case `:upcase`, the default.
- **Escapes.** `\` makes the next character literal, and `|...|` a whole run:
  `|Mixed Case|` keeps its case and its space. An escaped character is never
  upcased, never a package marker, and stops the token being a number, so
  `|12|` is a symbol.
- **Colons.** The position of every unescaped colon is recorded.

The colons decide where the symbol comes from:

| written | means | found by |
| --- | --- | --- |
| `foo` | in the current package | `(intern "FOO" *package*)` |
| `:foo` | a keyword | `(intern "FOO" "KEYWORD")` |
| `pkg:foo` | external in `PKG`, or an error | `find-symbol`, and its status must be `:external` |
| `pkg::foo` | in `PKG`, internal or not | `(intern "FOO" "PKG")` |
| `a:b:c` | an error | |

This is the whole reason the reader moved. Every case calls a package function
from chapter 36, and they are Lisp functions, so a Lisp reader calls them like
any other.

`reader.lisp` comes *before* `packages.lisp` in the library, because
`packages.lisp` must be the last file the Rust reader reads (its last form
adopts every system symbol into a package). That works because a compiled
call to `intern` looks the function up when it runs, not when it is compiled.

## Numbers it will not read

Quokka has no ratios or floats until steps L and M. The Rust reader reads `1.5`
as a *symbol* named `1.5`, which works today and would quietly mean something
else once floats exist. The Lisp reader refuses instead. It recognises the
syntax of a ratio (`1/2`) or a float (`1.5`, `.5`, `1e5`, `2d0`) from CLHS
2.3.1 and signals an error. Tokens that only *look* numeric, such as `1+`, `e5`
and `1.2.3`, are not numbers by the standard's rules and still read as
symbols. SBCL reads floats, so no SBCL baseline can check a refusal. A Quokka
test does it instead (`the_lisp_reader_refuses_ratios_and_floats`).

## Backquote, and the oracle that is not SBCL

Backquote has a constraint that nothing else in the reader has. The Rust reader
expands a template *while reading* (chapter 15):

```
`(a ,b ,@c)   reads as   (cons 'a (cons b (append c 'nil)))
```

Step F is the fixpoint (chapter 34). The compiler, read by the Lisp reader and
compiled, must produce exactly the bytes it produced when the Rust reader read
it. If the Lisp reader expanded backquote any other way, even into equivalent
code, the bytes would differ and the fixpoint would fail with nothing actually
wrong. So `%quasiquote` in `reader.lisp` is `reader.rs::quasiquote` rule for
rule.

SBCL cannot check this. It reads backquote into its own internal forms, so its
answer is right for SBCL and useless as a baseline. The oracle here is the Rust
reader. One test reads the prelude, the compiler and the whole library with
both readers, and compares every form, printed:

```rust
#[test]
fn the_lisp_reader_reads_the_system_as_the_rust_reader_does() {
```

That is every top-level form Quokka is built from, including every backquote,
string, character and comment in the system. It passed on its first run, and it stays as a
standing test, the first piece of the fixpoint to exist.

## What the tests found

The SBCL differential file, `reader.lisp`, has eighty-eight forms. On the first run
it found three things.

**An argument-order bug.** `a\bc` read as `AcB`. The escape case was written
as:

```lisp
(%collect-token stream (read-char stream nil nil)
                (cons (read-char stream) chars) ...)
```

Lisp evaluates arguments left to right, so the first `read-char`, meant to
fetch the character *after* the escaped one, ran first and took the escaped
character. The fix reads the escaped character into a variable before the
call. It is an easy bug to write in any language with side effects in
arguments, and a reader is mostly side effects.

**Where reading stops.** `(read-from-string "a b")` returns the symbol and a
position. SBCL says 2: a top-level `read` consumes one whitespace character
after the object. A `read` called by a reader macro in the middle of an object
does not, which is what its `recursive-p` argument is for. And SBCL reads
`::foo` as the keyword `:FOO`, which the first version refused.

**A difference that was the Rust reader's, not this one's.** The test
`(eq (read-from-string "cl-user::foo") 'foo)` was false in Quokka and true in
SBCL. The `'foo` in the test file was read by the *Rust* reader, which finds
the system symbol `FOO`, and the build had already put that one in the `QUOKKA`
package. The Lisp reader, asked for `CL-USER::FOO`, correctly made a new
symbol. Both readers were doing what they were written to do. Until step D,
the file is read by one reader and its strings by the other, so a test that
compares their symbols compares two packages. The test now reads both sides
from strings.

## What it cost

**Rust: 74 lines**, six primitives. Five are for characters: `char-code`,
`code-char`, `char-upcase`, `char-downcase` and `alpha-char-p`. The reader
cannot classify a character or upcase a token without them. Which characters
have case, and which are letters, are questions Unicode answers with tables.
Rust's standard library already carries those tables. In Lisp they would be
kilobytes of data in every image, undoing part of what chapter 37 saved.
`upper-case-p`, `digit-char-p` and the rest are Lisp, built from these.

`char-upcase` follows CLHS 13.1.4.3: a character has case only if it and its
partner map to each other one to one. Unicode upcases `ß` to "SS", two
characters, so in Common Lisp `ß` has no case.

The sixth primitive is `%constant-string`. A string literal read by the Rust
reader is a constant string, immutable (#26) and packed (#44). One read by the
Lisp reader has to be the same, or code read by the Lisp reader would have
literals a program could modify. The primitive makes a constant copy, and the
string reader macro calls it.

**Lisp: about 620 lines**, half of them comments, in `lisp/library/reader.lisp`. **The image** grew
from 254 KB to 306 KB. That is the reader's code, its readtable and its symbols.

## What is not here

- **The system does not read with it yet.** The REPL, the image and the test
  harness still read source with the Rust reader. Switching them is step D.
- **Readtable case** other than `:upcase`, `*read-base*`, `*read-suppress*`,
  and `set-syntax-from-char`. Whitespace and the two escapes are fixed rather
  than in the table.
- **Most of `#`**: `#.`, `#+`/`#-`, `#b`/`#o`/`#x`, `#c`, `#p`, `#*`, `#n=`.
- **Ratios and floats**, refused until steps L and M.
- **`read-from-string`'s `:preserve-whitespace`** and
  `with-input-from-string`'s `:index`.
- **Condition types.** Errors are signalled as simple errors, not as
  `reader-error` or `end-of-file`. The differential format compares only that
  an error happened.
- **Nested backquote**, which neither reader handles correctly, for the reason
  chapter 15 gives. The Lisp reader copies the Rust reader's behaviour here on
  purpose, for the fixpoint.

---

**References.** CLHS chapter 2, *Syntax*: 2.1.4 for character syntax types, 2.2
for the reader algorithm, 2.3 for tokens and potential numbers, 2.4 for the
standard macro characters. CLHS chapter 21, *Streams*, and chapter 23, *Reader*,
for the functions. Steele, *Common Lisp the Language*, 2nd edition, chapter 22,
for the reader with its history. Bawden, "Quasiquotation in Lisp" (1999), for
backquote.
