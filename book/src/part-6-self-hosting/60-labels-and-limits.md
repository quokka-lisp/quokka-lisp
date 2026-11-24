# Labels, limits and a counter

> *Where this fits.* Chapter 52 taught the printer to write shared and
> circular structure with labels, `#1=(a . #1#)`, so that another Lisp could
> read it back. Our own reader, rewritten in Lisp in chapter 38, could not.
> After chapter 59 ansi-test (chapter 49) listed 22 tests that need a
> feature tier 1 should have, and six of them were exactly this. This
> chapter adds the labels to the reader, explains why the other sixteen were
> partly our measuring tool's mistake, and fills in a handful of constants
> and one variable the standard defines and we had never bound. One of
> those, `*gensym-counter*`, turned out to reach into the fixpoint.

*Status: done, issue #72. Step Y of phase 2.*

## Reading an object that is not finished yet

The syntax is small. `#1=` in front of an object gives it the label 1, and
`#1#` later in the same text stands for that object. In

```lisp
(#1=(a b) #1# #1#)
```

the three elements of the outer list are one and the same list `(a b)`,
`eq` to each other, not three copies. That case is easy for a reader: when
it meets `#1#`, the object labelled 1 is complete, and it returns it.

The hard case is the one the printer produces for a circular list:

```lisp
#1=(a b . #1#)
```

Here `#1#` comes *inside* the object it names. When the reader reaches it,
it has read `a` and `b` and is still in the middle of the list; there is no
finished object to return, and the list cannot be finished until it knows
what to put at its end.

The usual answer, the one SBCL uses and CMUCL before it, is a
**placeholder** and a **patch**. When `#1=` is read, we make a fresh symbol
that nothing else can contain, an uninterned symbol (chapter 15), and record
it under the label. Any `#1#` read before the object is complete returns the
placeholder, so the reader builds an ordinary list with the placeholder at
its end. When the object is complete, we walk it and replace every
placeholder with the object itself:

```lisp
(defun %read-sharp-equal (stream sub-char argument)
  (cond ((null argument) (error 'reader-error "#= wants a label: #1="))
        ((assoc argument %read-labels) (error 'reader-error "that #= label is already defined" argument)))
  (let* ((placeholder (make-symbol "LABEL"))
         (entry (list argument placeholder placeholder)))
    (push entry %read-labels)
    (let ((object (%read-required stream)))
      (if (eq object placeholder) (error 'reader-error "#n= labels only itself" argument))
      (setf (third entry) object)
      (%patch-label object placeholder object))))
```

The entry holds the label, the placeholder and the object, which starts out
as the placeholder too, so `#1#` needs no separate test for "not finished
yet": it returns whatever the third element is at that moment.

The other way to do it would be to have every reader macro cooperate, each
`(` and `#(` filling in a slot later when it learns what the label stands
for. That puts knowledge of labels into every piece of the reader. The patch
keeps it in one function, at the cost of walking the object once more after
it is read, and only objects that have a label are walked.

The walk itself has a trap. By the time it runs, the object is circular,
which is the whole point, so a walk that simply followed every car and cdr
would never end. `%patch-label` remembers every cons and array it has
visited in an `eq` hash table, the kind chapter 52 made able to take any
object as a key, and does not enter one twice. Down the spine of a list it
loops, so a list of a thousand elements does not cost a thousand levels of
recursion. It looks into conses and arrays and nowhere else, because those
are all the reader can build.

Labels belong to the outermost call to `read` (CLHS 2.4.8.15). `#1=` in one
form and `#1=` in the next are unrelated, and a reader macro that calls
`read` again in the middle of a form, with the argument that says it is a
recursive read, sees the labels of the form around it. We keep them in a
special variable, `%read-labels`, which `read` binds afresh unless the read
is recursive; the binding goes when the outer `read` returns. That is the
same shape as the printer's table for `*print-circle*`, bound by the
outermost `print`.

With this, what the printer writes reads back:

```lisp
(let* ((*print-circle* t)
       (x (list 1 2 3)))
  (setf (cdr (last x)) x)
  (prin1-to-string (read-from-string (prin1-to-string x))))
;; => "#1=(1 2 3 . #1#)"
```

## When the measuring tool is wrong

The other sixteen tests on the list were not what they seemed. Ten were
reported as needing `#.`, `#*` or `#A`, syntax the reader has had since
chapters 50 and 55. The runner (chapter 49) cannot ask the reader *why* it
gave up on a piece of text, so it searches the text for kinds of syntax it
knows Quokka lacks, and its list of probes had been written before those
chapters. A test whose text contains both `#C(1 2)`, a complex number we
defer, and `#*101` was blamed on both. We cut the probes down to what is
still missing: `#C`, `#P` and ratios.

The `#.` cases needed a different fix. `#.` evaluates a form while reading,
so when that evaluation fails, the reader fails, and the text looks
unreadable:

```lisp
(typecase 'a
  (number 'bad)
  (#.(find-class 'symbol nil) 'good))
```

The real need is `find-class`, which is part of CLOS (tier 2), and `typecase`,
which waits for types (phase 3). The runner now reads such text a second
time with a copy of the readtable in which `#.` returns its form instead of
evaluating it, and explains the test by the operators that form uses, the
same way it explains any other test. If the second reading also fails, the
reason is syntax after all.

One of the `#.` tests, `APPLY.4`, failed for a plainer reason: it evaluates
`call-arguments-limit`, and we had never defined it. Eight more were
`defstruct`'s `:include`, which belongs to the next step, and one was a
small mistake in `defsetf`. Its long form lets the body `return-from` the
name of the place being defined, as CLHS says, and ours did not wrap the
body in a block of that name.

## The limits a program may rely on

`call-arguments-limit` is one of a family of constants with which a Common
Lisp tells a program what it can rely on. Each is an *exclusive* upper
bound: `call-arguments-limit` is one more than the largest number of
arguments any call is guaranteed to accept. CLHS only sets minimums, 50 for
this one, and SBCL's are all 2<sup>62</sup>−1, as large as its fixnums go.

Ours had to be measured, and the measurement turned up two different
numbers. A call written in the source compiles to a `CALL` instruction whose
argument count is one byte (the bytecode's ceilings are in chapter 24), so
it can pass 255 arguments and no more. `apply`, which passes the elements of
a list, has no such ceiling; we applied `list` to ten thousand arguments
without trouble. A lambda list may name 256 variables, the most one scope
can hold. A limit is a promise about what always works, so we took the
smaller numbers: `call-arguments-limit` and `lambda-parameters-limit` are
both 256, and so is `multiple-values-limit`, since `values` is called like
any other function.

The float limits were simpler. Chapter 48 decided that all four of the
standard's float formats, short, single, double and long, are the one IEEE
double, which CLHS permits. But a Lisp that merges formats still has to
define every format's constants, `most-positive-single-float` among them,
and we had only defined the double's. They are now the double's values
under the other names, and all of them are constants, made with
`defconstant` (chapter 51) rather than `defvar` as the double's had been.
`lambda-list-keywords`, the list of the eight `&` words, joined them.

## The counter that reached the fixpoint

`gensym` makes the uninterned symbols that macros use for their temporary
variables (chapter 15). Until now it was a primitive, written in Rust, which
kept its counter inside the machine, ignored its argument and always made a
name like `G48058`. CLHS says more. `(gensym "FOO")` makes `FOO` followed by
the counter, `(gensym 123)` makes `G123` without advancing anything, and the
counter is a special variable, `*gensym-counter*`, that a program may read
and bind:

```lisp
(let ((*gensym-counter* 1327))
  (symbol-name (gensym "FOO")))
;; => "FOO1327"
```

A variable the program binds cannot live in Rust, so `gensym` moved into
the library, into `packages.lisp`, the file that loads last, because by then
everything it uses to turn the counter into digits exists. Macros in earlier
library files are expanded with the primitive, which is harmless: both make
uninterned symbols.

The compiler is full of macros and calls `gensym` itself, so from now on the
compiler, when it compiles itself in a rebuild (chapter 41), calls the
library's `gensym`. Its names now carry their prefixes, `SETF-EXPANDER17`
where they used to be `G17`. We expected no trouble, since the fixpoint,
our check that the compiler compiles itself into exactly the same code from
one generation to the next (chapter 42), already allows a gensym's name to
differ between generations. The check that compares two rebuilt worlds, C2
and C3, word for word failed anyway, and not on a name: the two heaps were
125 words apart in size.

The cause was in the names after all. A constant string is packed eight
characters to a word (chapter 37). C2's rebuild had numbered its gensyms
from 110 upwards and C3's from where C2 stopped, about 8,200, so C3's names
had a digit more. A name such as `SETF-EXPANDER999` is sixteen characters,
two words; `SETF-EXPANDER1000` is seventeen, three. With the old names, `G`
and five digits, the extra digit had always fitted in the same word, and
the comparison had been lucky.

We could have taught the comparison to allow strings of different lengths,
but every object after the first longer name moves, and comparing word for
word stops making sense. Instead `%rebuild-world` binds `*gensym-counter*`
to 0 while it rebuilds, so every rebuild numbers its gensyms the same way.
The names the library's `gensym` makes in C2 and C3 are now identical; the
few the machine still makes itself, while rewriting lambda lists, keep
rising as before and differ only in their digits. The binding ends with the
rebuild, so the world's own counter is as it was. The
variable had to be special before `rebuild.lisp` binds it, so its `defvar`
sits in `symbols.lisp`, earlier in the library than `gensym` itself.

## Two smaller corrections

While writing the test file we found that `read` with `*read-suppress*` true
returned a list of nils for `(a b)`, where CLHS says `read` returns nil. The
reader macros below `read` may build what they like while suppressed, so
the fix is in `read`, `read-preserving-whitespace` and `read-delimited-list`
themselves. And `defconstant` used to refuse a second definition whose value
is not `eql` to the first, as SBCL does. A quoted list is a new list each
time its file is compiled, and the warm rebuild compiles every library file
again, so `lambda-list-keywords` would have stopped the rebuild. A value
`equal` to the old one is now accepted, and the old one kept; CLHS leaves
that case undefined.

## What did it cost?

The step cost no Rust. Its Lisp is about 150 lines, half of it the reader's
labels and the runner's new explanation, and a test file,
`tests/lisp/labels-and-limits.lisp`, agrees with SBCL. ansi-test went from
7,807 to 7,851 tests passing, with none of the old ones lost; the plain
failures went from 347 to 311, and the tests needing a tier-1 feature from
22 to 8, all of them `defstruct`'s `:include`.

## What is still missing?

`defstruct` is still tier 0's: a name and slots, and no options at all, so
`:include`, `:conc-name`, `:constructor` and the rest are refused. That is
step Z, the next chapter. The other plain failures ansi-test lists are
grouped by message in `docs/ansi-test.md`.
