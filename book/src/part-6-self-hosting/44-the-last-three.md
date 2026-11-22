# The last three, and what a macro can see

> **Where this fits.** Chapter 13 went through Common Lisp's special
> operators one by one and left three out of tier 0 on purpose: `progv`,
> `load-time-value` and `symbol-macrolet`. Chapter 35 built dynamic
> binding, and chapter 43 built places. This chapter adds the three, which
> brings Quokka to all 25. One of them, `symbol-macrolet`, turns out to need
> something bigger than itself: macros that can see the code around them.
> That is the **lexical environment**, and it is what this chapter is really
> about. What it unlocks is the rest of tier 1, which is now all functions and
> macros, and no more special operators.

*Status: done, issue #53. Step I of phase 2.*

## A special operator can be a macro

A **special operator** is a form the evaluator handles itself, because it
can't be written as a function or a macro over the others. `if` is the usual
example. Common Lisp fixes the list at 25. But CLHS 3.1.2.1.2.2 adds a
permission: *an implementation is free to implement a Common Lisp special
operator as a macro*, as long as what the programmer sees is the same.

Quokka's interpreter knows 22 special operators, through a table in Rust
(`special.rs`). None of the three new ones is in that table, so to the
interpreter they are ordinary names. Each one could arrive as a library
macro, as a case in the compiler, or as a bit of both. Each ended up where
the knowledge it needs lives.

## progv: binding a name you only learn at run time

Chapter 35's `let` binds a special variable with one instruction:

```
DBIND k     ; bind the symbol in constants[k] to the value on the stack
```

The *name* is a constant. The compiler knows it when it compiles the `let`.
`progv` binds names that are computed while the program runs:

```lisp
(progv (list '*x* '*y*) (list 1 2)
  (list *x* *y*))        ; => (1 2), and afterwards both are back
```

There were three ways to do it (#53):

- **One `PROGV` instruction that binds a whole list.** This needs a new kind
  of record that holds many old values. Restoring it is a loop inside one
  instruction, and rule 9 says an instruction's work must stay bounded. A
  very long list would need a way to stop and resume partway through.
- **No Rust at all**: `set` each value, and put the old ones back with
  `unwind-protect`. This is wrong. A computation that is *abandoned* (for
  example, stopped from the REPL) runs no cleanup forms, so the global
  values would stay changed. It also can't make a variable unbound.
- **One small instruction, `DBIND-SYMBOL`**: `DBIND` with the symbol taken
  from the stack instead of the constants. This is the one chosen.

With `DBIND-SYMBOL`, `progv` is a macro that wraps its body in a function,
and the binding is a recursion in Lisp, one name per level:

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
then the value, then `DBIND-SYMBOL`, then the body, then `UNMARK`. Every
binding `progv` makes is therefore an ordinary binding record, exactly like
`let`'s, and everything that already undoes a `let` binding undoes these:
`UNMARK` on the way out, the unwinder on the way past for a `throw`, an
uncaught error, an abandoned computation. The new Rust is the instruction,
which shares its body with `DBIND`, and a `makunbound` primitive.
CLHS says a name given no value is "bound and then made to have no value",
and the **unbound marker** is a Rust value that Lisp must never hold, so only
Rust can put it into a symbol.

The body is a closure, so it still sees the lexical variables around the
`progv`. Its values come back through `funcall` unchanged, all of them.

## load-time-value: once, and when?

`(load-time-value form)` evaluates `form` once and uses the result as a
constant of the code:

```lisp
(defun cache () (load-time-value (make-hash-table)))
(eq (cache) (cache))   ; => T: one table, made once
```

"Load time" comes from compiling a *file*, where the code is compiled on one
day and loaded on another. Quokka has no compiled files. Every form is
compiled and then called straight away, and `eval` itself is
compile-then-call (chapter 40). CLHS covers this case directly: for code
processed by `compile`, the form "is evaluated at compile time", and the
result is "treated as a literal object at run time".

A macro expands while the compiler compiles, so a macro is exactly the right
place:

```lisp
(defmacro load-time-value (form &optional read-only-p)
  read-only-p
  (list 'quote (eval form)))
```

That's the whole of it. The alternative was a case in the compiler, which is
tier 0 code. The compiler never uses `load-time-value` itself, and in the
cold build it could not call `eval` anyway.

## symbol-macrolet: a name that stands for a form

```lisp
(symbol-macrolet ((head (car cell)))
  head          ; reads (car cell)
  (setq head 5) ; writes it: setq of a symbol macro is setf of its expansion
  (let ((head 1)) head))  ; a let shadows it: this is a plain variable
```

This one can't be a library macro. Whether `head` here means the symbol
macro or a variable depends on what is in scope *at that point in the
code*. Only the compiler keeps track of that. It does so in `env`, a list
of **scopes**, innermost first, that `%lookup` walks to turn a variable
name into a frame depth and a slot index (chapter 26).

So a symbol macro becomes one more kind of scope in that same list,
`(:symbol-macro (head . (car cell)))`. When the compiler meets a symbol, it
looks for a symbol macro and stops at the first scope that mentions the
name. A `let` of `head` *inside* the `symbol-macrolet` is nearer, so it wins.
That is shadowing, and it comes out of keeping both kinds of name in the one
chain.

There was one catch. `%lookup` counted *every* scope as a step outwards,
because every scope used to be something pushed at run time. A symbol-macro
scope exists only while compiling, so counting it would make every variable
outside it resolve one frame too far. `%lookup` now steps over these scopes
without counting them. A test in the differential file reaches past a symbol
macro, through a `let` and into a closure, to check this.

## What a macro can't see

Here is where it got bigger:

```lisp
(symbol-macrolet ((x (car (next-cell))))
  (incf x))
```

`incf` is a macro, and chapter 43's whole point was that it evaluates
`(next-cell)` **once**. It does that by calling `get-setf-expansion` on its
place. Here the place is `x`, and a macro sees only its argument forms. To
`incf`, `x` looks like an ordinary variable. It would expand to
`(setq x (+ x 1))`. The compiler would then turn that into
`(setf (car (next-cell)) (+ (car (next-cell)) 1))`, which calls `next-cell`
twice.

The fact that `x` stands for something is in the compiler's `env`, and the
macro has no access to it. Common Lisp's answer is to hand the macro its
**lexical environment**: an object describing the code around the call,
received through an `&environment` parameter, and accepted by
`macroexpand-1` and `get-setf-expansion` as an optional second argument:

```lisp
(defmacro expansion-here (form &environment env)
  (list 'quote (macroexpand-1 form env)))

(symbol-macrolet ((x (car cell))) (expansion-here x))   ; => (CAR CELL)
```

Step H had accepted `&environment` and bound it to `nil`, because nothing
could fill it. The alternative was `symbol-macrolet` alone: reading,
`setq` and `setf` right, and `incf` evaluating twice, which a test against
SBCL would catch. Carsten chose real environments.

The pieces:

- **The object.** While the compiler runs an expander, it records its current
  `env` in `%expansion-scopes`, next to `%local-macros`, which `macrolet`
  already kept. The environment is `(:environment scopes local-macros)`. Nil
  for both is the **null environment**, written `nil`.
- **`&environment`.** The Rust lambda-list rewrite has no `&environment`,
  and it doesn't need one. The compiler takes `&environment e` out of a
  `defmacro` or `macrolet` lambda list before the rewrite, and wraps the body
  in `(let ((e (%current-environment))) ...)`. No Rust.
- **Expanding in an environment.** The library redefines `macroexpand-1` and
  `macroexpand` to take one: they see its symbol macros and local macros, and
  run an expander *inside* it, so a nested macro that asks gets the right
  environment.
- **Places.** Every macro in `setf.lisp` now takes `&environment` and passes
  it to `get-setf-expansion`, which asks `macroexpand-1` whether a symbol is a
  symbol macro. `(incf x)` above now calls `next-cell` once. As a bonus, a
  `macrolet` macro is now a place too, which was the gap chapter 43 left.

### Keeping the fixpoint

Chapter 43's lesson applies again. The library now redefines
`macroexpand-1`, and the compiler used to call `macroexpand-1` to expand
every macro. The rebuild would then have compiled the compiler with the
library's version.

Instead of making the two versions agree, the compiler now **expands macros
itself**:

```lisp
(t (let ((expander (if (symbolp head) (%macro-function head) nil)))
     (if expander
         (%compile (%expand expander (cdr form) env %local-macros) env tail)
         (%compile-call form env tail))))
```

The compiler and `macroexpand-1` are now independent, so the library can
redefine `macroexpand-1` freely. It also lets the compiler set the
environment for the expander, which it had to do anyway. The expansion
itself is unchanged, since it is still `(apply expander arguments)`. The
fixpoint tests passed unchanged.

## What it cost

- **Rust: 42 lines**, comments included: `DBIND-SYMBOL`, sharing `DBIND`'s
  body; `makunbound`; and the new file's entry in `boot::LIBRARY`. That
  leaves 1,191 of the 15,000.
- **Lisp:** about 150 lines in the compiler and 90 in
  `lisp/library/operators.lisp`, and `setf.lisp` threads an environment
  through every macro.
- **Forty opcodes.**

## What is not here

- **`define-symbol-macro`**, the global kind of symbol macro. Nothing needs
  it yet.
- **A local function shadowing a global macro.** In
  `(flet ((when (x) x)) (when 1))`, CL calls the local function. Quokka's
  compiler checks local macros, then special operators, then *global* macros,
  before it looks at local functions. This was already true before this
  step, and it is now visible because the environment exists. A small fix
  for later.
- **Declarations in a `symbol-macrolet` body**, along with declarations
  generally, which Quokka mostly ignores.

---

**References.** The Common Lisp HyperSpec: 3.1.2.1.2.2 (special operators as
macros), 3.1.1.4 (environments), the dictionary entries for `progv`,
`load-time-value`, `symbol-macrolet`, `macroexpand-1` and `&environment`
in 3.4.4 (macro lambda lists). Guy Steele, *Common Lisp the Language*, 2nd
edition (1990), section 8.5 on environments, which describes the richer
interface (`variable-information` and friends) that ANSI left out. Christian
Queinnec, *Lisp in Small Pieces* (1996), chapter 9, on macros and the
environments they are expanded in.
