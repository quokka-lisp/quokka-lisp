# A little language for output

> *Where this fits.* Chapter 39 gave Quokka a printer in Lisp, with output
> streams and a console, and chapter 45 gave it the string and sequence
> functions. What it still lacked was the way Common Lisp programs actually
> produce text: `format`, which takes a template full of directives and fills
> it in. This chapter builds the basic part of it. A control string turns out
> to be a small program in a language of its own, so the chapter is also a
> second, smaller run at something the book has done once already: reading a
> language into a tree and then walking the tree. With `format` in place,
> programs and tests can write output the way Common Lisp code is usually
> written, and step N, the ansi-test harness, can report its results.

*Status: done, issue #57. Step K of phase 2.*

## What does `format` do?

`format` takes a destination, a **control string**, and some arguments. The
control string is text with **directives** in it, each starting with a tilde,
and each directive either uses up the next argument or shapes the output:

```lisp
(format nil "~A has ~D item~:P" "cart" 3)   ; => "cart has 3 items"
```

`~A` prints its argument the way a person reads it, `~D` prints an integer,
and `~:P` writes an "s" unless the argument before it was 1. The destination
`nil` asks for the result as a string; `t` writes to `*standard-output*`; a
stream is written to directly.

There is a good deal more to it than this. Directives take parameters
(`~10A` pads to ten columns, `~8,'0X` prints hexadecimal padded with zeros),
modifiers (`~:D` puts commas between groups of digits, `~@D` always shows the
sign), and some of them contain other directives:

```lisp
(format nil "~{~A~^, ~}" '(1 2 3))   ; => "1, 2, 3"
```

`~{…~}` runs its contents once for each element of a list, and `~^` stops
early when nothing is left, which is how the comma after the last element is
left out. `~[…~;…~]` chooses one of several clauses, and `~(…~)` changes the
case of whatever its contents print. Taken together, a control string is a
small program, with loops and conditionals, in a language that has no
spaces between its words.

## How much of it is "basic"?

The tier table in `docs/design.md` puts *basic* `format` in tier 1 and full
`format` in tier 2, so we had to decide where to draw the line (decision 1 in
#57). CLHS 22.3 lists about 35 directives. We took the ones ordinary code
uses most, and which nothing stops us from testing against SBCL today:
`~A` and `~S` with padding; integers in any radix with `~D`, `~B`, `~O`, `~X`
and `~nR`; characters with `~C`; newlines and tildes; plurals with `~P`;
moving among the arguments with `~*`; the conditional `~[`, the iteration
`~{` with its escape `~^`, case conversion `~(`, and indirection `~?`.

What we left out waits for something we have not built. The float directives
(`~F`, `~E`, `~G`, `~$`) need floats, which come with step M. `~R` without a
radix prints English words or Roman numerals, each a table of its own.
`~T` and `~<…~>` need to track columns and justify text, and `~W`, `~_` and
`~I` belong to the pretty printer, all of them tier 2.

## Should a control string be compiled?

A control string is a program, so the old question from chapter 24 comes
back: interpret it every time, or compile it once? SBCL compiles. When a
control string is written literally in the source, which is nearly always, a
*compiler macro* on `format` turns it into ordinary Lisp code while the
surrounding code is compiled, and nothing is parsed at run time.

We chose to interpret (decision 2). Compiling is an optimisation, which no
program can observe except by its speed, and it would need a second version
of every directive, one that writes code instead of output, as well as
compiler macros, which Quokka does not have yet. We opened #58 to come back
to it in a later phase, when a profile shows that `format` matters. The
`formatter` macro, which turns a control string into a function, is here a
thin wrapper that returns a closure around the interpreter.

## Parsing first, then running

We could have written `format` the way a small `printf` is written: read a
character, act on it, read the next. That works for flat strings, but the
nesting makes it awkward. `~[` has to skip the clauses it does not choose,
and `~^` has to jump to the end of its enclosing `~{`, so each would have to
scan ahead for its matching close directive, parsing without acting, and do
it again on every pass of a loop.

So we split the work in two (decision 3), the same split Quokka uses for
Lisp itself, where the reader turns text into lists and the evaluator walks
them (chapters 8 and 9). The parser turns the control string into a list of
literal strings and **directive records**:

```lisp
(defstruct %format-directive char params colon at clauses default once)
```

Each record holds the directive's character, its parameters, its two
modifiers, and, for `~[`, `~{` and `~(`, its clauses, each of which is again
a list of strings and records. `"~A: ~{~D~^, ~}"` becomes a string-and-record
list whose `~{` record holds one clause, `(~D ~^ ", ")`. A second function,
`%run-items`, walks the list with the arguments, and nesting becomes ordinary
recursion. A malformed control string, such as one with an unclosed `~{`, is
reported before anything has been written.

The arguments are kept in a small record of their own, with the whole list
and the part not yet used, because some directives need both: `~:*` backs up
one argument, `~3@*` jumps to the fourth, and the parameter `#` stands for
the number of arguments left. A `~{` makes a fresh record for the list it
iterates over, so the directives inside it consume that list rather than the
outer arguments.

## How does `~^` know where to jump?

`~^` is an escape: "if nothing is left, stop". What it stops depends on
where it is. At the top level it ends the whole `format`. Inside `~{` it ends
the iteration. Inside `~:{`, where each element is a sublist of arguments, it
ends only the current step, and `~:^` ends the whole iteration once the
current step is the last.

A non-local exit is what `catch` and `throw` are for (chapter 14). Each of
the three constructs opens a `catch` with its own tag, and binds a special
variable, `%format-escape-tag`, to the tag a `~^` inside it should throw to:

```lisp
(catch 'format-iteration
  (let ((%format-escape-tag (if colon 'format-step 'format-iteration))
        (%format-steps state))
```

Because the binding is dynamic (chapter 35), a `~^` always throws to the
innermost construct that encloses it, however deeply it is nested inside
`~[` and `~(` clauses, and the binding is undone when the construct ends.
`handler-case` finds its way out of a computation in much the same way.

## What did SBCL teach us?

We wrote the differential file from the standard and let SBCL check it, and
four of the answers corrected our first draft.

The first was `~:^`. We had it test whether the current step had arguments
left, which ends a `~:{` after its first step. CLHS says it tests whether the
current step is the *last* one, which needs the list of steps rather than the
step itself, so `~:{` now also binds that list, `%format-steps`.

The second was `~@C`, which writes a character the way the reader would read
it. We used `prin1`, which writes a space as `#\Space`; SBCL writes `#\` and
then the character itself when the character is graphic, so a space comes
out as `#\ ` with a trailing blank. The third was `~D` given something that
is not an integer: the standard says to print it as `~A` would, and SBCL
prints it without the padding the directive asked for, which we now do too.

The fourth was an ordering mistake of our own. In `(format nil "~vA" 6 "x")`
the `V` parameter takes its value from the arguments, and it takes it
*before* the directive takes its own argument. Our `~A` took the argument
first, so it tried to print `6` padded to `"x"` columns.

We would not have found any of the four by reading the code; the
differential tests against SBCL (chapter 12) found all of them on the first
run.

## What did it cost?

No Rust beyond the new file's entry in `boot::LIBRARY`. The Lisp is about 400
lines in `lisp/library/format.lisp`, comments included, most of it one
function per directive.
The image grew from 592 KB to 639 KB. The fixpoint, our check that the
compiler compiles itself into exactly the same code generation after
generation (chapter 42), was never at risk, because nothing in the system's
own source uses `format`.

## What is still missing?

Everything on the far side of the line from decision 1: the float
directives, English and Roman numerals, column tracking and justification,
and the pretty printer's directives. A string with a fill pointer is not
accepted as a destination either, because Quokka has no fill pointers.

Error messages are not formatted yet. In Common Lisp, `(error "~A is bad" x)`
reports "3 is bad", because the message is a control string. Quokka's
`error` keeps the message and its arguments apart, and the REPL prints them
apart. Making conditions report through `format` belongs with the condition
system's own work in phase 3, alongside `define-condition` and restarts.

And control strings are interpreted every time they are used, until #58.

---

**References.** The Common Lisp HyperSpec, section 22.3, *Formatted Output*,
and the dictionary entries for `format` and `formatter`. Guy Steele, *Common
Lisp the Language*, 2nd edition (1990), chapter 22.3.3, whose examples are
still the clearest introduction to `~{` and `~^`. SBCL's
`src/code/late-format.lisp` and `target-format.lisp`, for how a production
implementation compiles control strings.
