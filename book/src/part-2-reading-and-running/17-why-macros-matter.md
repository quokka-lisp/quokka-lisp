# Why macros matter, and what hygiene is

> **Where this fits.** Chapter 15 built macros and chapter 16 spent them.
> Neither said why a sane person should care, or explained the one genuine
> schism in the subject: Common Lisp and Scheme disagree about macros, have
> disagreed for forty years, and both are right about something. This chapter
> is the argument and the disagreement. It builds nothing — it is here because
> the decision in chapter 15 was made in one paragraph and deserves more.

## The claim

A macro system lets a program change the language it is written in.

That sentence gets nodded at and not believed, so here is the test. Below are
five things from five languages. In most languages each requires a compiler
change. In Common Lisp each is a library, written by a user, in an afternoon.

| Feature | Language that needed a compiler change | Lisp |
| --- | --- | --- |
| `for (x : xs)` | Java 5, 2004 | a macro |
| `async`/`await` | C# 5, 2012 | a macro |
| pattern matching | Java 21, 2023 | a macro |
| `using`/`with` resource cleanup | C# 1 / Python 2.5 | a macro |
| a class system with multiple dispatch | — | a macro |

The last one is not a joke. CLOS — the Common Lisp Object System, with
multiple inheritance, multiple dispatch and a metaobject protocol — was added
to Common Lisp **as a library**, specified before it was built into any
compiler, and `defclass` and `defmethod` are macros. A language feature of
that size arrived without the implementors' permission.

That is the claim in its strongest form: in most languages the set of
abstractions available to you is chosen by someone else and shipped on their
schedule. In Lisp it is not.

## Three things macros buy

### 1. Control flow that functions cannot express

This is the floor, and chapter 15 covered it: a function receives values, so
`(or (cached-p x) (expensive x))` cannot be a function without calling
`expensive` every time. Anything that chooses whether, when, or how often to
evaluate something must be a macro.

Every iteration construct, every conditional, every "do this with a timeout",
every "retry three times", every "only in debug builds" is this case.

### 2. Abstraction over syntax, not just over values

A function abstracts over *values*: you had two bits of code differing in a
number, so you pass the number. A macro abstracts over *code*: you had two
bits of code differing in a statement, and no function can take a statement.

The standard example is resource cleanup. Here is a pattern:

```lisp
(let ((handle (open-thing "x")))
  (unwind-protect
       (use handle)
    (close-thing handle)))
```

Written out, that is four lines of ceremony and one line of work, and the
ceremony is where the bug lives — someone forgets the `unwind-protect` and a
handle leaks on an error. In a language without macros the answer is a style
guide and a code review. With macros:

```lisp
(defmacro with-thing ((var name) &body body)
  `(let ((,var (open-thing ,name)))
     (unwind-protect (progn ,@body)
       (close-thing ,var))))

(with-thing (handle "x")
  (use handle))
```


The pattern becomes a thing with a name, so forgetting it is a compile error
rather than a slow leak. Common Lisp's standard library is full of these:
`with-open-file`, `with-output-to-string`, `with-slots`. C# eventually grew
`using` and Python grew `with`; both took a language version and a committee.

**This is the one to internalise.** The first use of macros is clever control
flow, and the thousandth is removing repetition that no function can reach.

### 3. Paying at compile time

A macro runs before the program does, so work it does is work the program does
not do. A `format`-like macro can parse its format string once, at compile
time, and expand into direct output calls. A state machine can be computed
during expansion and emitted as a jump table. A unit-conversion macro can do
the arithmetic on constants and leave a literal.

Quokka does not exploit this yet — there is nothing to exploit it with until
the compiler exists — but it is the reason macros are not merely
syntactic sugar. They are the user's access to compile time.

## The bill

Macros are not free, and a chapter that only sold them would be dishonest.

**They are harder to debug.** An error inside an expansion points at code the
user never wrote. Without `macroexpand` — which Quokka does not have yet, and
which is the first thing to build next — you are guessing at a program you
cannot see.

**They do not compose like functions.** You cannot pass `when` to `mapcar`,
store it in a variable, or partially apply it. A macro is not a value. Every
macro is therefore a small hole in the functional fabric of the language, and
a macro that should have been a function is a mistake rather than a style
choice. The rule of thumb is old and good: *if it can be a function, it must
be a function.*

**They are a dialect risk.** A codebase where every third form is a local
macro is a codebase written in a language only its authors speak. Common Lisp
folklore has a name for the failure — "everyone builds their own language and
nobody can read anyone else's".

**And they can capture.** Which is the rest of this chapter.

## The disagreement

Recall the bug from chapter 15:

```lisp
(defmacro or2 (a b)
  `(let ((v ,a)) (if v v ,b)))

(let ((v 'outer)) (or2 nil v))   ; => NIL
(let ((w 'outer)) (or2 nil w))   ; => OUTER
```

The macro introduced a binding called `v`, and the caller's `,b` was
substituted *inside* it. The same program, with one variable renamed, gives
two answers. This is **variable capture**.

Two families of solution exist, and the split between them is the
longest-running argument in the subject.

### Common Lisp: name it something nobody can write

```lisp
(defmacro or2 (a b)
  (let ((tmp (gensym)))
    `(let ((,tmp ,a)) (if ,tmp ,tmp ,b))))
```

`gensym` makes a fresh **uninterned** symbol. Interning (chapter 8) is what
makes reading `FOO` twice produce the same symbol; a symbol that is not in the
obarray can be *held* but never *named*. Reading the text `G17` produces a
different symbol from the `G17` that `gensym` returned.

So capture becomes impossible for that binding — by making the name
unwritable, rather than by making the system clever.

The cost: it is manual. The macro writer must notice. A macro that binds a
temporary and forgets `gensym` is a latent bug that fires only when a caller
picks the wrong variable name, which may be years later and in someone else's
file.

### Scheme: rename automatically

Scheme's answer is **hygienic macro expansion**, introduced by Kohlbecker,
Friedman, Felleisen and Duba in 1986 and standard in Scheme since R4RS. The
same macro in `syntax-rules`:

```scheme
(define-syntax or2
  (syntax-rules ()
    ((_ a b) (let ((v a)) (if v v b)))))

(let ((v 'outer)) (or2 #f v))    ; => outer
```

The macro writer wrote `v`, the caller wrote `v`, and it works. The expander
renames the macro's `v` to something unique automatically.

The property is defined in two halves, and both are needed:

- **No capture outwards**: a binding introduced by the macro cannot capture a
  name used by the caller. That is the `v` above.
- **Referential transparency inwards**: a name *used* by the macro refers to
  whatever it meant where the macro was written, not where it is called. So a
  macro that calls `list` keeps working for a caller who has locally rebound
  `list` to something else.

The second half is the one Common Lisp's `gensym` does not even attempt. In
CL, a macro that expands into `(list ...)` is at the mercy of whatever `list`
means at the call site, and the defence is social: do not rebind standard
names. That is why Quokka has `scripts/check-cl-names.lisp` (chapter 12) — the
language cannot enforce it, so a script does.

The mechanism is not simple. The expander tracks, for every identifier, which
expansion introduced it, through arbitrarily nested macro uses. Early
implementations did this by renaming and then unrenaming; modern ones
(`syntax-case`, Flatt's *sets of scopes*, 2016) carry scope sets on syntax
objects and resolve bindings against them. It is a genuine piece of theory
with its own literature.

### Why hygiene is not simply better

The obvious reaction is that Scheme won: it removed a class of bug with no
work from the user. For the bug it addresses, that is right.

What hygiene makes hard is the case where you *want* to introduce a binding
the caller can see. The classic is the **anaphoric** macro — named after the
grammatical anaphor, a word like "it" that refers back:

```lisp
(defmacro aif (test then &optional else)
  `(let ((it ,test))
     (if it ,then ,else)))

(aif (lookup-user name)
     (greet it)
     (complain))
```

`it` is the macro's doing, and the caller uses it on purpose. Under hygiene
this does not work: the macro's `it` and the caller's `it` are different
identifiers, which is exactly what hygiene promises. `syntax-case` provides an
escape hatch (`datum->syntax`) to deliberately break hygiene, and using it is
noticeably more work than writing the unhygienic version.

So the trade is real in both directions:

| | Common Lisp | Scheme |
| --- | --- | --- |
| Accidental capture | possible; `gensym` is the cure | impossible by construction |
| Deliberate capture | the normal case | needs an escape hatch |
| What the macro writer must know | `gensym`, and when | the hygiene model, when fighting it |
| Expander complexity | essentially none | substantial |
| Macros are | ordinary functions on lists | a separate sublanguage (`syntax-rules`) |

That last row matters more than it looks. A Common Lisp macro is a *function*,
written in Common Lisp, that takes lists and returns lists. You debug it with
the debugger you already have, and anything you can compute, you can compute
in it. `syntax-rules` is a pattern-matching sublanguage that is not Scheme —
`syntax-case` restores the full power, at the cost of more machinery to learn.

Neither design is a mistake. They optimise for different fears: Scheme fears a
subtle bug the user cannot see, Common Lisp fears a wall between the user and
the expander.

## What Quokka does, and why

Quokka is unhygienic, like Common Lisp. Three reasons, in descending order of
weight:

1. **Rule 6.** Quokka is a Common Lisp subset, and a CL subset whose macros
   are hygienic is not a CL subset. Real Common Lisp code calls `gensym` and
   expects it to be necessary. Matching SBCL is the whole point of the
   differential tests, and macro expansion is not exempt.

2. **Rule 7, the 15k-line budget.** Tracking identifier provenance through
   nested expansions is a substantial piece of machinery for a problem one
   call to `gensym` closes. There are things in Quokka's future — a condition
   system, a bytecode compiler, a GUI — with a better claim on those lines.

3. **Comprehensibility, which is rule 7 again and also the point of this
   book.** `mc.gensym()` is eight lines of Rust and can be explained in a
   paragraph, as chapter 15 did. The sets-of-scopes algorithm cannot. For a
   system whose purpose is partly to be read, an explainable defence that the
   user must remember beats an unexplainable one that is automatic.

The cost is stated plainly: **Quokka lets you write this bug.** The defence is
that it is a known bug with a known cure, demonstrated in
`tests/lisp/macros.lisp`, which keeps both versions of `or2` side by side so
the difference stays visible rather than becoming folklore. The prelude's own
`cond` and `or` call `gensym`, and the test file checks that a caller with a
variable named `once`, `value` or `tmp` is unharmed.

## What to take away

The three-line version:

- **Macros are how a language stops being somebody else's.** The features
  other languages wait years for are a library here, and the clearest evidence
  is that CLOS is one.
- **The discipline is "function if possible, macro if necessary."** Macros do
  not compose, do not pass as values, and are harder to debug. They earn their
  place when evaluation order or syntax is the thing being abstracted.
- **Hygiene is a real answer to a real bug, with a real cost.** Scheme removed
  accidental capture by making deliberate capture hard. Common Lisp kept both
  and handed you `gensym`. Quokka follows Common Lisp, on purpose, and the
  reason is written down here so the decision can be argued with rather than
  inherited.

---

**References.** Eugene Kohlbecker, Daniel Friedman, Matthias Felleisen and
Bruce Duba, "Hygienic Macro Expansion", *LFP* 1986 — the original. Matthew
Flatt, "Binding as Sets of Scopes", *POPL* 2016, for how it is done now. Paul
Graham, *On Lisp* (1993), chapter 9 on capture and chapter 14 on anaphoric
macros, which is the best case anyone has made for the unhygienic side.
Richard Gabriel and Kent Pitman, "Technical Issues of Separation in Function
Cells and Value Cells" (1988), for the related Lisp-1/Lisp-2 argument, which
turns out to be entangled with this one. Sonya Keene, *Object-Oriented
Programming in Common Lisp* (1989), if the claim about CLOS seemed too
convenient.
