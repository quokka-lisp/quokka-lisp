# The last three, and what a macro can see

> *Where this fits.* In chapter 13 we went through Common Lisp's special
> operators one by one, and left three of them out of tier 0 on purpose:
> `progv`, `load-time-value` and `symbol-macrolet`. Chapter 35 built dynamic
> binding, and chapter 43 built places. This chapter adds the three, which
> brings Quokka to all 25. One of them, `symbol-macrolet`, turned out to need
> something larger than itself: macros that can see the code around them.
> That is the **lexical environment**, and most of the chapter is about it.
> Once it is in place, the rest of tier 1 is functions and macros, with no
> special operators left to add.

*Status: done, issue #53. Step I of phase 2.*

## Can a special operator be a macro?

A **special operator** is a form the evaluator handles itself, because it
cannot be written as a function or as a macro over the others; `if` is the
usual example. Common Lisp fixes the list at 25, but CLHS 3.1.2.1.2.2 adds a
permission: *an implementation is free to implement a Common Lisp special
operator as a macro*, as long as what the programmer sees is the same.

Quokka's interpreter knows 22 special operators, through a table in Rust
(`special.rs`). None of the three new ones is in that table, so to the
interpreter they are ordinary names, and each of them could arrive as a macro
in the library, as a case in the compiler, or as a bit of both. We put each
one where the knowledge it needs is kept.

## How does `progv` bind a name it only learns at run time?

A `let` of a special variable compiles to one instruction (chapter 35):

```
DBIND k     ; bind the symbol in constants[k] to the value on the stack
```

The *name* is a constant, which the compiler knows when it compiles the
`let`. `progv` binds names that are computed while the program runs:

```lisp
(progv (list '*x* '*y*) (list 1 2)
  (list *x* *y*))        ; => (1 2), and afterwards both are back
```

We looked at three ways of doing this in #53. A single `PROGV` instruction
that binds a whole list would need a new kind of binding record holding many
old values, and restoring it would be a loop inside one instruction. Rule 9
says the work of one instruction must stay bounded, so a very long list would
need a way to stop and resume halfway through. The second way used no Rust at
all: `set` each value, and put the old ones back with `unwind-protect`. That
is simply wrong. A computation that is *abandoned*, for example stopped from
the REPL, runs no cleanup forms, so the global values would stay changed, and
there is no way to make a variable unbound with `set`.

We chose the third: one small instruction, `DBIND-SYMBOL`, which is `DBIND`
with the symbol taken from the stack instead of from the constants. With it,
`progv` becomes a macro that wraps its body in a function, and the binding is
a recursion in Lisp, one name per level:

```lisp
(defmacro progv (symbols values &body body)
  `(%progv ,symbols ,values (lambda () ,@body)))

(defun %progv (symbols values body)
  (if (null symbols)
      (funcall body)
      (%dynamic-bind (car symbols) (car values)
        (if (null values) (makunbound (car symbols)))
        (%progv (cdr symbols) (cdr values) body))))
```

`%dynamic-bind` is a form only the compiler knows. It compiles the symbol,
then the value, then `DBIND-SYMBOL`, then the body, then `UNMARK`. So every
binding `progv` makes is an ordinary binding record, like those of `let`, and
whatever already undoes a `let` binding undoes these too: `UNMARK` on the way
out, the unwinder on the way past for a `throw`, an uncaught error, or an
abandoned computation. The new Rust is the instruction, which shares its body
with `DBIND`, and a `makunbound` primitive. CLHS says that a name given no
value is "bound and then made to have no value", and the **unbound marker**
is a Rust value that Lisp must never hold, so only Rust can put it into a
symbol.

The body is a closure, so it still sees the lexical variables around the
`progv`, and all of its values come back through `funcall` unchanged.

## When is "load time" in Quokka?

`(load-time-value form)` evaluates `form` once and uses the result as a
constant of the code:

```lisp
(defun cache () (load-time-value (make-hash-table)))
(eq (cache) (cache))   ; => T: one table, made once
```

"Load time" comes from compiling a *file*, where the code is compiled on one
day and loaded on another. Quokka has no compiled files: every form is
compiled and then called straight away, and `eval` itself compiles and then
calls (chapter 40). CLHS covers this case directly. For code processed by
`compile`, the form "is evaluated at compile time", and the result is
"treated as a literal object at run time".

A macro expands while the compiler compiles, so a macro is the right place
for it:

```lisp
(defmacro load-time-value (form &optional read-only-p)
  read-only-p
  (list 'quote (eval form)))
```

That is all of it. The other way would have been a case in the compiler,
which is tier 0 code, for something the compiler never uses itself and
could not have run during the cold build anyway, since the interpreted
compiler cannot call `eval`.

## Introducing `symbol-macrolet`

```lisp
(symbol-macrolet ((head (car cell)))
  head          ; reads (car cell)
  (setq head 5) ; writes it: setq of a symbol macro is setf of its expansion
  (let ((head 1)) head))  ; a let shadows it: this is a plain variable
```

This one cannot be a library macro. Whether `head` means the symbol macro or
a variable depends on what is in scope *at that point in the code*, and only
the compiler keeps track of that. It does so in `env`, a list of **scopes**,
innermost first, which `%lookup` walks to turn a variable's name into a frame
depth and a slot index (chapter 26).

So a symbol macro became one more kind of scope in the same list,
`(:symbol-macro (head . (car cell)))`. When the compiler meets a symbol, it
looks for a symbol macro and stops at the first scope that mentions the name.
A `let` of `head` *inside* the `symbol-macrolet` is nearer, so the variable
wins. That is shadowing, and we get it for free by keeping both kinds of name
in one chain.

There was one catch. `%lookup` counted *every* scope as a step outwards,
because until then every scope had been something pushed at run time. A
symbol-macro scope exists only while compiling, so counting it would make
every variable outside it resolve one frame too far. `%lookup` now steps over
these scopes without counting them, and a test in the differential file
reaches past a symbol macro, through a `let` and into a closure, to check it.

## What can't a macro see?

With `symbol-macrolet` working, a slightly harder case showed the limit:

```lisp
(symbol-macrolet ((x (car (next-cell))))
  (incf x))
```

`incf` is a macro, and the point of chapter 43 was that it evaluates
`(next-cell)` only once, which it does by calling `get-setf-expansion` on its
place. Here the place is `x`, and a macro sees nothing but its argument forms,
so to `incf`, `x` looks like an ordinary variable. It would expand to
`(setq x (+ x 1))`, and the compiler would then turn that into
`(setf (car (next-cell)) (+ (car (next-cell)) 1))`, which calls `next-cell`
twice.

The fact that `x` stands for something is in the compiler's `env`, and the
macro cannot reach it. Common Lisp's answer is to hand the macro its
**lexical environment**: an object describing the code around the call,
which the macro receives through an `&environment` parameter, and which
`macroexpand-1` and `get-setf-expansion` accept as an optional second
argument:

```lisp
(defmacro expansion-here (form &environment env)
  (list 'quote (macroexpand-1 form env)))

(symbol-macrolet ((x (car cell))) (expansion-here x))   ; => (CAR CELL)
```

Step H had accepted `&environment` and bound it to `nil`, because nothing
could fill it yet. We could have added `symbol-macrolet` on its own, with
reading, `setq` and `setf` all correct and only `incf` evaluating twice, but
a test against SBCL would have caught that, so we chose to build real
environments.

While the compiler runs a macro's expander, it now keeps its current
environment in a variable, `%expansion-scopes`, next to `%local-macros` where
`macrolet` already kept the local macros. The two together are what we hand
the macro as its environment, `(:environment scopes local-macros)`; when both
are empty it is the **null environment**, written `nil`. The lambda-list
rewrite in Rust has no `&environment`, and it does not need one: the compiler
takes `&environment e` out of a `defmacro` or `macrolet` lambda list before
the rewrite, and wraps the body in `(let ((e (%current-environment))) ...)`.

(Both halves of that have since been replaced. Step AC made the environment
the expander's second argument, as it is in Common Lisp; and #111 put the
local macros into the compiler's environment itself, as `(:macro ...)`
entries beside the scopes, so that what we hand a macro is now that one list
and nothing is kept in a variable while it runs.)

The library then redefines `macroexpand-1` and `macroexpand` to take an
environment. They see its symbol macros and local macros, and run an expander
*inside* it, so a nested macro that asks for its environment gets the right
one. Every macro in `setf.lisp` now takes `&environment` and passes it on to
`get-setf-expansion`, which asks `macroexpand-1` whether a symbol is a symbol
macro, and `(incf x)` above calls `next-cell` once. It also closed the gap
chapter 43 left open: a place defined with `macrolet` now works too.

## How did the fixpoint survive?

The fixpoint, our check that the compiler compiles itself into exactly the
same code generation after generation (chapter 42), was at risk again for the
reason chapter 43 described. The library now redefines `macroexpand-1`, and
the compiler used to call `macroexpand-1` to expand every macro, so the warm
rebuild would have compiled the compiler with the library's version.

Rather than make the two versions agree, we had the compiler expand macros
itself:

```lisp
(t (let ((expander (if (symbolp head) (%macro-function head) nil)))
     (if expander
         (%compile (%expand expander (cdr form) env %local-macros) env tail)
         (%compile-call form env tail))))
```

The compiler and `macroexpand-1` are now independent, so the library can
redefine `macroexpand-1` as it likes. It also gives the compiler the place to
set up the expander's environment, which it needed anyway. The expansion
itself is still `(apply expander arguments)`, and the fixpoint tests passed
unchanged.

## What did it cost?

The Rust was 42 lines, comments included: `DBIND-SYMBOL`, which shares its
body with `DBIND`, the `makunbound` primitive, and the new file's entry in
`boot::LIBRARY`. That left 1,191 lines of the 15,000, and brought the
instruction set to forty opcodes. The Lisp was about 150 lines in the
compiler and 90 in `lisp/library/operators.lisp`, and `setf.lisp` now passes
an environment through every macro.

## What is still missing?

`define-symbol-macro`, the global kind of symbol macro, is not there yet;
nothing has needed it. Declarations at the head of a `symbol-macrolet` body
are not understood either, along with declarations in general, which Quokka
mostly ignores.

When this chapter was written, a local function did not yet shadow a global
macro of the same name: in `(flet ((when (x) x)) (when 1))` Common Lisp calls
the local function, but Quokka's compiler looked for global macros before
local functions. The environment made the gap visible, and step J1 fixed it
(chapter 45).

---

**References.** The Common Lisp HyperSpec: 3.1.2.1.2.2 (special operators as
macros), 3.1.1.4 (environments), the dictionary entries for `progv`,
`load-time-value`, `symbol-macrolet`, `macroexpand-1` and `&environment`
in 3.4.4 (macro lambda lists). Guy Steele, *Common Lisp the Language*, 2nd
edition (1990), section 8.5 on environments, which describes the richer
interface (`variable-information` and friends) that ANSI left out. Christian
Queinnec, *Lisp in Small Pieces* (1996), chapter 9, on macros and the
environments they are expanded in.
