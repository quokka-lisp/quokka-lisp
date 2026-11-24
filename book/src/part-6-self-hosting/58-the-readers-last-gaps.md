# The reader's last gaps

> *Where this fits.* Chapter 38 wrote the reader in Lisp, the part of the
> system that turns text into objects, and chapter 39 wrote its partner, the
> printer, which turns objects back into text. Both covered what the
> library and the compiler needed. ansi-test, the conformance suite we
> measure ourselves against (chapter 49), asks for more: a readtable whose
> *case mode* can be changed, characters known by their names, a way to read
> without swallowing the space after a token, and a way to give one
> character the syntax of another. This chapter, the ninth worked from
> ansi-test's list, adds all four, in Lisp, and with them the reader is
> complete as far as tier 1 goes.

*Status: done, issue #70. Step W of phase 2.*

## Folding the case of a name

When the reader meets `car` it interns the symbol named `CAR`, in capitals.
Common Lisp's names are upper case inside, and the reader folds the letters
of every token you type so that you need not shout. That folding is a
setting of the readtable, the table that tells the reader what each
character means (chapter 38), called its **readtable case**. It has four
values. `:upcase`, the standard, raises the letters; `:downcase` lowers
them; `:preserve` leaves them alone; and `:invert` flips the case of a token
whose letters are all the same case and leaves a mixed one as it is, so that
`zebra` reads as `ZEBRA`, `ZEBRA` as `zebra`, and `Zebra` as `Zebra`. A
letter written with an escape, `\a` or inside `|...|`, is never folded.

Three of the modes fold one character at a time, and our reader already
upcased each character as it collected it. We kept that loop and made the
fold depend on the mode. `:invert` cannot work that way, because whether a
letter flips depends on every other letter in the token, including ones not
read yet. So for `:invert` the reader collects the letters as they are,
remembers where the escaped characters were, and decides once the token is
complete:

```lisp
(defun %invert-token (text escapes)
  (let ((upper nil) (lower nil))
    (dotimes (i (length text))
      (unless (member i escapes)
        (let ((char (aref text i)))
          (if (upper-case-p char) (setq upper t))
          (if (lower-case-p char) (setq lower t)))))
    (unless (and upper lower)
      (dotimes (i (length text))
        (unless (member i escapes)
          (let ((char (aref text i)))
            (setf (aref text i) (if upper (char-downcase char) (char-upcase char)))))))
    text))
```

The list of escape positions is kept in every mode, though only `:invert`
uses it. A token almost never contains an escape, so the list is nearly
always empty and keeping it costs nothing worth measuring.

Changing the mode showed us a bug that the standard mode had hidden. The
reader recognised a float like `1e5` by looking for the exponent marker `E`
in capitals, which worked only because the token had already been upcased.
Under `:preserve` the token stays `1e5`, and the reader would have read it
quietly as a symbol. Number syntax ignores case, and the check does too now.

## Printing in the reader's case

We had to decide whether the printer should follow the readtable case as
well, or keep printing as it did. The standard is clear that it should: the
printer's job, when it prints with escapes, is to write text that the reader
reads back as the same object, and that depends on how the reader will fold
it. CLHS 22.1.3.3.2 has a table of how the readtable case and
`*print-case*`, the variable that says whether names print in capitals,
small letters or capitalised, combine. We chose to follow it, partly
because 15 of the 24 tests that needed `readtable-case` were printer tests,
and partly because a printer that ignored the mode would write names the
reader then misread.

Under `:upcase` nothing changes: `*print-case*` applies to the upper-case
letters, the ones the reader folds to. `:downcase` is the mirror image, and
the interesting consequence is that an upper-case letter now needs an
escape, because the reader would lower it. SBCL prints the symbol `XYZ`
under `:downcase` as `|XYZ|`, and so do we. Under `:preserve` and `:invert`
case never needs escaping, since what is printed reads back unchanged, and
`*print-case*` is ignored; `:invert` prints an all-capitals name in small
letters, because the reader will flip it back.

## The names of characters

`#\Space` is a character written by its name, and `name-char` and
`char-name` translate between names and characters. Quokka knew only the
names of the ASCII control characters, such as `Nul`, `Tab` and `Rubout`,
which the printer needed. SBCL knows a name for every Unicode character,
taken from Unicode's own table, so `(char-name #\a)` is
`"LATIN_SMALL_LETTER_A"`.

We first proposed to stay with the control names, since Unicode's table
holds tens of thousands of names and would add hundreds of kilobytes to
every image. Then we noticed that this would be inconsistent with a
decision already made. When chapter 37 packed constant strings, it drew the
line at **Latin-1**, the first 256 characters of Unicode (ASCII and the
accented letters of Western European languages): a string of those is
stored eight characters to a word. So we drew the same line here. Quokka
knows SBCL's name for every one of the 256 codes and none above. Above 255,
`char-name` answers nil and the printer writes the character itself, where
SBCL writes its Unicode name; both read back as the same character.

The 256 names live in a data file, `lisp/library/char-names.lisp`,
generated once from SBCL by `scripts/generate-char-names.lisp`, in the same
way as the list of the standard's symbol names (chapter 36). A name is a
fact about a character, so this is data and not copied code. The table added
15 KB to the image, more than our first estimate of a few kilobytes, since
each name is a string object of its own.

`name-char` ignores case, as the standard requires. It also accepts SBCL's
other names for some controls (`Linefeed`, `Escape`, `Del`, `Lf` and so on),
and a code written as `U+E9` or `UE9`, which gives any character at all.
The reader's `#\` now looks names up with `name-char` itself, so the two
cannot disagree about what a name means.

Two old routines changed with the table. The printer writes a character from
128 to 255 by its name, as SBCL does. And `format`'s `~:C`, which spells out
the name of a character that does not print, started spelling out every
letter as well once letters had names; it now asks whether the character
prints first, as SBCL's does, so `(format nil "~:C" #\a)` is still `a`.

## Reading without consuming the delimiter

A token ends at the first character that cannot be part of it, usually a
space. When `read` is called from outside, it reads that space as well,
which is what you want when reading one form after another.
`read-preserving-whitespace` leaves it in the stream, for a program that
reads a token and then wants to see what came after it, a newline perhaps.
In our reader only the outermost call ever consumed the delimiter, so the
new function is `read` without that last step, and `read-from-string` gained
its `:preserve-whitespace` argument.

## Syntax types

The last function, `set-syntax-from-char`, needed a decision we had not
foreseen. It makes one character behave like another: `(set-syntax-from-char
#\! #\Space)` makes `!` whitespace, and `(set-syntax-from-char #\! #\\)`
makes it an escape. The standard gives every character a **syntax type**:
whitespace, constituent (an ordinary character of a token), single escape
`\`, multiple escape `|`, or macro character, terminating or not. Our
readtable held only the macro characters. Which characters were whitespace,
and the two escapes, were fixed in the reader's code.

We could have let `set-syntax-from-char` copy only macro characters and
refuse the rest. That would have been smaller, but it refuses what the
standard allows, and the tests do exactly the refused things: one makes each
standard character in turn a single escape. So we gave the readtable a
second table, `syntax`, holding only the characters whose type a program has
changed. Every question the reader asks about a character now goes through
one function:

```lisp
(defun %char-syntax (char)
  (let* ((readtable *readtable*)
         (entry (gethash char (%readtable-macros readtable))))
    (cond (entry (if (cdr entry) :non-terminating :terminating))
          ((%readtable-syntax readtable) (%table-syntax readtable char))
          (t (%standard-syntax char)))))
```

The syntax table is nil until something changes a character, and while it
is nil the reader takes the standard types directly. This matters because
the reader runs over every library file in every rebuild, and ansi-test's
runner loads its files close to the step budget it is given; a lookup in a
table for every character would have slowed both. It is the same kind of
fast path as the one `make-array` got in chapter 55.

The standard also gives characters *traits* that do not move with their
syntax type. One is called *invalid*: the whitespace characters, backspace
and rubout may not appear unescaped in a token. A space made a constituent
with `set-syntax-from-char` is still invalid, and reading `a b` with it then
signals a `reader-error`. Writing the test for that showed us that SBCL
applies the trait in the standard syntax too, to backspace and rubout, which
the standard counts as constituents; our reader had quietly put them into
names. It signals `reader-error` now, as SBCL does.

## What the measurement found

The first full ansi-test run after these changes passed 47 more tests and
lost none, but it also showed a new timeout. The runner gives each form it
loads a fixed number of steps, and one form in the structures tests, a
400-line function, no longer fitted. Nothing in it had changed; the reader
had become slower. Every character of every token now went through
`%char-syntax`, then through four tests of the type it returned before
reaching the common case, and then through a separate check for the
invalid trait.

Our first two attempts were guesses. We reordered the token loop so that
an ordinary constituent, by far the most common character, is the first
case it tests, and folded the invalid trait into the standard syntax as a
type of its own, `:invalid`, so that backspace and rubout are found by the
test the loop makes anyway. Then we moved the question "does this readtable
have a syntax table?" out of the loops that skip whitespace and read
strings, so that it is asked once rather than for every space of
indentation. Neither made the timeout go away, and a third guess, that some
test had left a changed readtable behind for the files after it, turned out
to be wrong when we printed the readtable's state just before the
structures files.

So we measured instead. The runner's budget can be lowered until a file no
longer loads, which gives the number of steps it needs. The structures file
needed 9.61 million steps of its 10 million before this step and 9.76
million after it, and a probe file holding only the 400-line function,
quoted so that loading it does nothing but read it, showed where they went:
reading that one form took 4.96 million steps before this step and 5.23
million after. Reading was half the cost of the form, and it was about 350
steps for every character of source.

That number pointed at `read-char`. Every character the reader took went
through it, and `read-char` is a standard function with a stream designator
to resolve and four optional arguments to sort out on every call. The
reader already knows its stream is a string input stream, so we gave it its
own `%next-char`, five lines that take the next character of the string
and nothing else, for the loops that run once per character: tokens,
whitespace, strings and comments.

```lisp
(defun %next-char (stream)
  (let ((at (%string-input-stream-position stream)))
    (if (< at (%string-input-stream-end stream))
        (progn (setf (%string-input-stream-position stream) (+ at 1))
               (aref (%string-input-stream-string stream) at))
        nil)))
```

Reading the probe dropped to 3.76 million steps, a quarter less than before
this step began, and the structures file to 8.46 million. We also turned
the token loop from a function that called itself once per character, with
the whole state of the token as eight arguments, into a `do` loop over
local variables; the measurement showed that this made little difference,
which taught us that a call is cheaper in our machine than we had assumed,
and that what a function does at its entry, like `read-char`'s argument
parsing, is what costs. The reader is in the path of everything, and its
cost per character is worth measuring whenever it changes.

The run also listed seven new failures, the error tests of
`read-preserving-whitespace`, which had been counted as "needs
`read-preserving-whitespace`" until it existed. They expect reading past
the end of the input to signal `end-of-file`, and ours signalled a plain
error. The reader signals `end-of-file` now, and `reader-error` for a `#`
followed by a character with no function, as SBCL does. The tests that
expect `stream-error`, which `end-of-file` is a kind of, still fail: our
conditions match by their exact type until tier 2 gives them a hierarchy.
One test read `#S(`, the syntax for a structure, which Quokka could not read
at all; `#S` now calls the structure's constructor. Quokka's structures are
still vectors and print as one, so this only matters for text written by
another Lisp. And a test of floats found that `(- 0.0)` was `0.0` rather than
`-0.0`, because negation was computed as `0 - x`, which loses the sign of
zero; negating a float flips its sign now, in two lines of Rust.

## What did it cost?

This step needed six lines of Rust: the entry for `char-names.lisp` in the
list of library files, and the two that negate a float. Everything else is
Lisp in `reader.lisp`, `printer.lisp` and `format.lisp`, and
`tests/lisp/reader-syntax.lisp` checks it all against SBCL: the four case
modes for reading and for printing, that what is printed under each mode
reads back, character names in both directions, `read-preserving-whitespace`,
`set-syntax-from-char` with whitespace, both escapes, macro characters and a
dispatching one, the invalid trait, the two conditions, `#S`, and the sign
of a negated zero.

ansi-test went from 7,699 passing tests to 7,757, of 13,558. The reader
group went from 75 to 111, and its four remaining failures all expect an
`end-of-file` to be caught as a `stream-error`. The printer group went from
208 to 223. The tests needing a tier-1 feature fell from 322 to 195; most of
those that no longer do still fail, because they also use `loop`, which is
tier 2. The whole run became 37 seconds faster, almost all of it from
`%next-char`.

## What is still missing?

Character names stop at Latin-1. A stream that is not a string, such as
the terminal, is tier 2, so the tests that read from `*terminal-io*` still
fail. The reader signals `reader-error` only for an invalid character; its
other complaints, an unbalanced parenthesis or an unknown package, are plain
errors still, since conditions with a hierarchy, in which a `reader-error` is
also a `parse-error`, come with tier 2.

---

**References.** The Common Lisp HyperSpec, 2.1.4 on syntax types and
constituent traits, 23.1.2 on the readtable case and the reader, 22.1.3.3.2
on how it affects the printer, and the entries for `name-char`,
`read-preserving-whitespace` and `set-syntax-from-char`. The Unicode
Standard, chapter 4, on character names.
