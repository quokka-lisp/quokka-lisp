# What a parameter list can say

> *Where this fits.* Chapter 10 gave functions parameters: a list of names,
> matched to arguments one for one. Common Lisp's parameter lists do rather
> more than that, and three of the things they do need *evaluation at call
> time*, which the part of the machine that binds arguments cannot do. By the
> end you will know how `&optional`, `&key` and a macro's destructuring
> patterns are built out of machinery that already exists, and why that was
> worth more than building the obvious thing.

## What people actually write

```lisp
(defun draw (shape &optional (colour 'black) (width 1 width-given)
                   &key (fill nil) origin
                   &aux (area (* width width)))
  ...)
```

Five things are going on, and only the first was built in chapter 10:

| | |
| --- | --- |
| `shape` | required — must be supplied |
| `&optional (colour 'black)` | may be omitted; then `colour` is `BLACK` |
| `(width 1 width-given)` | and `width-given` says whether it was supplied |
| `&key (fill nil) origin` | supplied by name, in any order |
| `&aux (area (* width width))` | not a parameter at all — a local binding |

`&key` is how a function grows an option without breaking its callers, and it
is everywhere in Common Lisp's own library, so none of this is decoration.

## Default values are forms, not values

`'black`, `1` and `(* width width)` are not values. They are **forms**,
evaluated when the function is called, and CLHS is specific about where:

> each is evaluated in a scope containing every parameter to its left.

So this must work, and must see `a`:

```lisp
((lambda (a &optional (b (* a 2))) (list a b)) 5)   ; => (5 10)
```

And a default must not be evaluated at all when an argument *was* supplied.
That is observable, because a default can have an effect:

```lisp
(let ((n 0))
  ((lambda (&optional (x (setq n 1))) x) 'given)
  n)                                                ; => 0, not 1
```

Now look at where argument binding happens. `bind_lambda_list` is an ordinary
Rust function that walks two lists and returns names and values. It has no
evaluator. It could not evaluate `(* a 2)` if it wanted to.

The obvious repair is to let it call the evaluator. That is exactly what
chapter 2 forbids: Lisp evaluation would be back on the Rust call stack, the
machine could no longer stop between steps, and rule 4, that the VM must be
able to yield, would be broken for every function call with a default.

## Two ways out, and why the second one

The first is to build a new machine state. Add a `BIND` frame: the machine
sets up a call, evaluates each default in turn, extends the environment, then
starts the body. It is faithful to "the machine does everything in explicit
frames", and it works.

It is also a second implementation of `let*`. Look at what `let*` already
does: evaluate each init form in a scope containing the bindings before it,
one at a time, with the state in a frame. That is the rule CLHS states for
default forms, word for word.

The second is to rewrite the lambda list instead. Once, when the closure is
made, turn

```lisp
(lambda (a &optional (b (* a 2)) (c 0 c-given)) BODY)
```

into a lambda list with no forms in it, and a body that computes the rest:

```lisp
(lambda (a &optional b c)
  (let* ((b       (if (eq b '<unsupplied>) (* a 2) b))
         (c-given (if (eq c '<unsupplied>) nil t))
         (c       (if c-given c 0)))
    BODY))
```

Every feature becomes `let*` and `if`. No new frame kinds, no new states,
nothing to test that was not already tested, and the scoping rule is obeyed
because `let*` obeys it rather than because this code remembered to.

This is a **source transformation**, the same technique as chapter 15's
macros, applied by the implementation to itself. It lives in Rust rather than
in the prelude only because `lambda` is a special operator, and `lambda` is a
special operator because `function` has to build a closure out of a lambda
expression (chapter 16).

### What it costs

The rewrite runs when a closure is *made*, not when it is called, so once per
`defun`, and once per evaluation of a `lambda` form. A lambda expression
inside a loop pays each time round, which is a real cost, and the honest
answer is that an interpreter has many larger ones. When the compiler arrives
it will do this at compile time and the cost disappears.

A lambda list of plain required parameters is returned untouched, body and
all. That is almost all of them, and there is a test saying so:

```rust
#[test]
fn a_simple_lambda_list_is_left_alone() {
    assert_eq!(rewritten("((a b) (+ a b))"), "((A B) (+ A B))");
}
```

## Telling "not supplied" from nil

The rewrite needs to ask "was an argument supplied?", and the answer cannot be
"is it nil", because these two calls must differ:

```lisp
((lambda (&optional (b 0 b-p)) (list b b-p)))        ; => (0 NIL)
((lambda (&optional (b 0 b-p)) (list b b-p)) nil)    ; => (NIL T)
```

The first supplied nothing and got the default. The second supplied `nil`,
which is a perfectly good argument, and the default was not used. A function
that cannot tell those apart cannot implement `&key` correctly, because
`(f :colour nil)` has to mean something different from `(f)`.

So `bind_lambda_list` binds a missing parameter to a **marker** rather than to
nil, and the generated `let*` turns the marker into the default. The marker is
a symbol interned under a name with a space in it:

```text
UNSUPPLIED MARKER
```

The reader has no `|bar quoting|`, so a space ends a token and no input can
name this symbol. It is the trick from chapter 15's backquote markers, used
again for the same reason: a value the user cannot forge, obtained by making
it *unwriteable* rather than by hiding it.

## &key, and the details that only a standard has

Keyword arguments are matched by name out of whatever is left of the argument
list. Three details are easy to get wrong and all three are specified.

The keyword is not the variable. `&key b` means the keyword `:b` and the
variable `b`, two different symbols with the same name, which chapter 8's
keyword namespace is what makes expressible. The rewrite makes the pairing
explicit, emitting `(:B B)`, so the binder never has to turn a name into a
keyword while a call is in progress.

Leftmost wins. CLHS says that when a keyword appears more than once, the
first occurrence is the one that counts:

```lisp
((lambda (&key a) a) :a 'first :a 'second)   ; => FIRST
```

Which looks arbitrary until you notice it is what makes
`(apply #'f :colour 'red defaults)` work: supply your overrides first, let the
defaults trail behind, and the overrides win.

And unknown keywords are an error, unless `&allow-other-keys` says otherwise.
This catches typos in a language where a misspelled keyword would otherwise be
silently ignored.

## &aux is not a parameter

```lisp
(lambda (a &aux (d (* a a))) (list a d))
```

`&aux` binds a local variable. It takes no argument, it is not part of the
function's interface, and it is `let*` written in an odd place. The rewrite
treats it as exactly that, so `&aux` entries leave the lambda list entirely
and become ordinary `let*` bindings:

```lisp
((a) (LET* ((D (* A A))) (list a d)))
```

Common Lisp has a few features like this, which exist because somebody found
them convenient in 1984. Knowing which ones the language leans on and which
are historical is most of knowing it, and `&aux` is historical.

## A macro's parameter list describes a shape

Everything so far applies to functions and macros alike. One thing applies to
macros only:

```lisp
(defmacro with-thing ((var name) &body body)
  `(let ((,var ,name))
     (unwind-protect (progn ,@body)
       (close-thing ,var))))

(with-thing (handle "x")
  (use handle))
```

The first parameter is not a name. It is a **pattern**: `(var name)` says the
first argument will be a two-element list, and binds `var` and `name` to its
parts. That is what lets a `with-` macro take its arguments in the shape
people want to write them, and it is why chapter 17 could claim macros
abstract over *syntax* rather than values.

Only macros can do this, and the reason is worth stating. A function receives
**values**, and a value has no shape for a lambda list to take apart, since by
the time `f` runs, `(list 1 2)` has become one object. A macro receives
**forms**, which are lists, which have parts. Destructuring is not a
convenience bolted onto macro parameters; it is the only place it could
possibly mean anything. `(defun f ((a b)) ...)` is an error in Common Lisp,
and an error here.

The implementation is the same trick again, one level down. A pattern leaves
a gensym in the lambda list, and becomes `car` and `cdr` in the `let*`:

```lisp
(c     <the argument>)
(var   (car c))   (c (cdr c))
(name  (car c))   (c (cdr c))
```

The cursor is *rebound* rather than advanced, which `let*` allows, so the
whole of destructuring is one more list of bindings beside the ones defaults
produced. Patterns nest by recursion, each level taking a fresh cursor, and a
pattern may contain `&optional` and `&rest` of its own.

`(defmacro m (a . b) ...)` works too, a dotted macro lambda list where the
tail is `&rest` by another spelling. That one needs the list rebuilt as a
proper one even when nothing else does, because the binder walks conses and
would never reach a dotted tail.

## What this does not do

- Pattern arity is not checked. `(with-thing (h) ...)` against the pattern
  `(var name)` binds `name` to nil, where a real Common Lisp signals an error.
  Checking means emitting a test per position, which is easy; signalling means
  having something to signal *into*, which is the condition system and is not
  built. Doing the first without the second would mean inventing an error
  mechanism to throw away later.
- `&key` and `&aux` inside a nested pattern are refused. They would need
  keyword scanning in generated Lisp; the top level of a macro lambda list
  supports both.
- `&whole` and `&environment` are absent. `&whole` binds the entire macro
  call, which Quokka's macro application does not currently pass along: it
  hands over the argument forms and not the call itself.
- `:allow-other-keys t` passed as an argument is not honoured; only the
  `&allow-other-keys` declaration in the lambda list is.

*Later: three of those four are closed. Once the condition system existed
(chapter 20) the arity check went in, and a required position now emits a
`consp` test that signals instead of binding nil. `:allow-other-keys` with a
true value among the arguments switches the check off for that call, as the
standard says. And `&environment` arrived with `symbol-macrolet` in phase 2:
the compiler strips it from a macro's lambda list and binds it to
`(%current-environment)`. The last, `&whole`, came with step AC
(chapter 64), when an expander became a function of the whole form and the
environment, as in Common Lisp. Since then every macro lambda list is taken
apart by the generated code this section describes, `&key` and `&aux` in
nested patterns included, and the dotted list no longer needs rebuilding.*

## What this unlocks

`&key` is how Common Lisp's library is shaped, so a great deal of it becomes
writeable that was not before. Nearer to hand, the prelude can grow the macros
that were waiting on this, and `defstruct`, when vectors exist, is a macro
whose entire job is to generate functions with lambda lists like these.

---

**References.** CLHS 3.4.1 for the ordinary lambda list in full and 3.4.4 for
the macro one, including the destructuring this chapter does not implement.
Guy Steele, *Common Lisp the Language, 2nd edition*, §5.2.2, for the same
material with more explanation and fewer cross-references.
