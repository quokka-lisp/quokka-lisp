# Printing, in Lisp

> **Where this fits.** Chapter 38 moved the reader into Lisp, so text becomes
> objects through packages and a readtable. This chapter runs the same road the
> other way: objects become text, in Lisp, with package prefixes and escapes
> that the Rust printer never had to know. It also gives Quokka its first
> output from a program, through the host-request protocol of chapter 31, and
> its first division. And it records a decision taken, measured, and taken
> again in a different order. What it unlocks is step D, where the system reads
> and prints its own work in Lisp.

*Status: done, issue #46. The differential files still print with the Rust
printer until step D; the reason is below.*

## What a printer promises

The printer's job is the reader's job backwards, and Common Lisp states the
contract precisely: what `prin1` writes, `read` reads back as the same object,
or as an equal one when "the same" cannot apply, as for a list. CLHS calls this
**print-read consistency**.

For numbers, strings and lists that is easy. For symbols it has two hard parts:

- **Which package.** `CL-USER::A` and `QUOKKA::A` are different symbols with
  the same name. If the printer writes plain `A`, the reader in `CL-USER` reads
  back the first. So when the printer writes the second, it must write
  `QUOKKA::A`.
- **Which characters.** A symbol can be named `lower`, `1`, `a b` or `a:b`.
  Written plainly, the reader would upcase the first, read the second as a
  number, split the third, and treat the colon in the fourth as a package
  marker. So the printer writes `|lower|`, `|1|`, `|a b|`, `|a:b|`. Bars are the
  multiple escape from chapter 38: everything inside is literal.

The Rust printer knew neither, because until packages existed there was only one
`A`, and the Rust reader made no symbols with odd names.

## The package prefix

CLHS 22.1.3.3.1 gives the rule, and the code is a direct reading of it:

```lisp
(defun %print-prefix (symbol name out)
  (let ((home (symbol-package symbol)))
    (cond ((null home) (if *print-gensym* (write-string "#:" out)))
          ((eq home %keyword-package) (write-char #\: out))
          ((%accessible-as-itself-p symbol name) nil)
          (t (%print-name (package-name home) out)
             (write-string (if (eq (nth-value 1 (find-symbol name home)) :external) ":" "::")
                           out)))))

(defun %accessible-as-itself-p (symbol name)
  (multiple-value-bind (found status) (find-symbol name *package*)
    (and status (eq found symbol))))
```

The key question is in `%accessible-as-itself-p`. It doesn't ask "is this
symbol's home `*package*`?". It asks "if the reader saw this name in
`*package*`, would it find *this* symbol?". `CAR` lives in `COMMON-LISP`, but
`CL-USER` uses `COMMON-LISP`, so `car` finds it and no prefix is written. Bind
`*package*` to `KEYWORD` and the same symbol prints as `COMMON-LISP:CAR`.

## Which names need bars

The standard is permissive here. CLHS lets a printer put bars around a name
that does not strictly need them, so implementations differ, and rule 6 says
SBCL is the one to match. So the rule was measured, not guessed: about thirty
names were printed in SBCL, and the results are in #46. In short, a name needs
bars if it is:

- **Empty:** `||`.
- **All dots:** `|.|`.
- **A number** in the reader's syntax, or in `*print-base*`: `|1|`, `|+1|`,
  `|1.5|`. In base 16 the symbol `A1` needs bars, because it is a number there.
- **Contains a lower-case letter, whitespace, a macro character, a colon, `|`
  or `\`:** `|foo|`, `|a b|`, `|x(|`, `|a:b|`.

Some names look as if they need bars and don't. `1+` is not a number by CLHS
2.3.1, because a potential number cannot end in a sign. `E5` is not either. Both
print plainly. And SBCL puts bars around `A#` even though `#` only means
something at the start of a token. That is permitted, and Quokka follows it,
because matching the oracle wins over being clever.

## Numbers, and the division Quokka did not have

Printing `255` in base 16 means dividing by 16 until nothing is left, keeping
the remainders. Quokka had `+`, `-`, `*` and comparisons, and **no division at
all**. Nothing had needed it.

The printer did, so step G added two primitives, `%quotient` and `rem`. Rust's
`/` and `%` truncate towards zero, which is exactly Common Lisp's `truncate`.
`truncate`, `floor` and `mod` are then Lisp, in `numbers.lisp`. `floor` differs
from `truncate` only when the remainder and divisor have different signs:
-7/2 truncates to -3 remainder -1, and floors to -4 remainder 1.

One detail in the digit loop is worth knowing about:

```lisp
;;; A negative number is divided as it is, with each remainder's sign dropped:
;;; negating it first would overflow for the most negative fixnum, which has
;;; no positive twin.
```

This is **two's complement** showing through. A fixed-width signed integer has
one more negative value than positive ones. So `(- most-negative-fixnum)` does
not fit, and the obvious "negate, then print the digits" fails on exactly one
number. Bignums (step L) make the problem go away; until then the loop avoids
it.

## Output, at last

Until this step, no Lisp program in Quokka could print anything. The REPL
printed *results*, from Rust, but `(print 42)` had nowhere to go. #42 had
asked whether tier 1 should have streams at all. Chapter 38 built input
streams; this step built output streams:

- a **string output stream** collects characters until
  `get-output-stream-string` takes them. `with-output-to-string` and
  `prin1-to-string` are built on it.
- the **console stream** is `*standard-output*`.

The question was what a console write *does*, and Carsten had three options:

- **A host request per write.** The write leaves a request in the heap and the
  machine stops until the host has printed it.
- **A buffer the host drains** after each form.
- **No console yet.**

He chose the first. It is the protocol chapter 31 built for reading files,
pointed the other way: `%write-console` is intercepted by the VM exactly as
`%read-source` is, and the host answers with its existing `write_stdout`. Output
appears as it is written, and a browser host uses the same protocol around a
Promise. The cost was about thirty lines of Rust.

To keep that to one request per print rather than one per character, every
entry point builds the whole text first:

```lisp
(defun prin1 (object &optional stream)
  (write-string (prin1-to-string object) stream)
  object)
```

## A decision taken twice

The second decision was how to test the printer beyond its own file. Carsten
chose the broadest option: make the compiled leg of the differential harness
print every result with the Lisp printer, so all twenty files check it against
SBCL.

That was built and run. **66 forms in 15 files disagreed with SBCL**, and every
one disagreed the same way:

```
expected => (A B C)
got      => (QUOKKA::A QUOKKA::B C)
```

The printer was right. The test files are still read by the *Rust* reader,
which has one flat table. When a test says `a`, it gets the only `A` there is:
the symbol the prelude used as a parameter name, which adoption had put in
`QUOKKA` (chapter 36). A correct printer in `CL-USER` asks whether writing `A`
would read back as that symbol. It would not, because the Lisp reader would
make `CL-USER::A`. So it writes `QUOKKA::A`. The disagreement was the Rust
reader's flat namespace, reported accurately.

Two ways forward, and the difference between them is the lesson of the chapter:

- **Make `CL-USER` use `QUOKKA`**, with all its symbols exported, as SBCL's
  `CL-USER` uses `SB-EXT`. The tests pass at once. But it reverses #41's
  "`QUOKKA` is internal" for good. After step D, about three hundred system
  names, and every helper added later, would stay visible to user code, where
  a user's `defun` could quietly replace the system's. SBCL's extra packages
  export a deliberate public API; `A`, `X` and `OUTER` are not one.
- **Switch the harness at step D**, when test files are read by the Lisp
  reader and a test's `a` becomes `CL-USER::A`. Nothing about the design
  changes; the switch waits for its precondition.

Carsten chose the second. The original choice was right; the *order* was wrong.
A package-aware printer can only be checked against a package-aware reader,
and that pairing exists at step D, not before. The harness carries both
printers now, with a comment saying when to flip it.

Meanwhile the Lisp printer is tested two ways. Its own SBCL file,
`printer.lisp`, has 116 forms; it found one bug, a missing space before `...`
when `*print-length*` cuts a vector short. And it is checked against the whole
system:

```rust
#[test]
fn the_lisp_printer_prints_the_system_as_the_rust_printer_does() {
```

That test reads every form of the prelude, compiler and library with the Lisp
reader, prints it with the Lisp printer, and compares the result with the Rust
reader and Rust printer. Read in `CL-USER` by the Lisp reader, those symbols are
`CL-USER`'s own, so no prefixes appear. It passed first time.

To let the two printers agree at all, the Rust printer changed one thing: a
space character now prints as `#\ `, as SBCL prints it, not `#\Space`.

## What it cost

**Rust: 59 lines.**
- The console request: a primitive, a `Request` variant, the host's arm, and
  sharing `%read-source`'s interception in the VM.
- `%quotient` and `rem`.
- `%printed`: the Rust printer's text for objects Lisp cannot describe, such as
  functions and conditions. Both printers then write them identically.

**Lisp:** `printer.lisp` (about 420 lines) and `numbers.lisp` (about 30).

**The image** grew from 306 KB to 344 KB.

## What is not here

- **The differential files printing with it**: step D.
- **`*print-pretty*`**, so everything is on one line; **`*print-circle*`**, so a
  circular list prints until the step budget runs out; **`*print-readably*`**,
  **`*print-lines*`**, **`*print-right-margin*`**. None of these variables
  exist, because a variable that existed and was ignored would be worse than
  none.
- **Unicode character names.** SBCL writes `#\λ` as
  `#\GREEK_SMALL_LETTER_LAMDA` and answers `char-name` for every character.
  Quokka writes the character itself, which SBCL reads back, and has names only
  for the ASCII controls.
- **`#S(...)` for structures**, which are still vectors underneath and print
  as vectors, until there are structure types.
- **`with-output-to-string` into an existing string**, which needs fill
  pointers; and buffering, so every console write is one request.
- **`format`**, step K.

---

**References.** CLHS chapter 22, *Printer*: 22.1.1 for the print variables,
22.1.3.3 for printing symbols and 22.1.3.3.1 for package prefixes. CLHS 2.3.1
for potential numbers, which decide what needs bars. CLHS chapter 21,
*Streams*. Steele, *Common Lisp the Language*, 2nd edition, chapter 22.
