# Glossary

Every term this book uses that is not ordinary English, in a sentence or two,
with the chapter that explains it properly.

---


**Header.** The first word of a heap object, saying what kind it is and how many
slots follow. Cons cells have none.
([5](part-1-substrate/05-objects-in-the-heap.md))

**Anaphoric macro.** A macro that deliberately introduces a binding the
caller can see — `(aif (lookup x) (greet it))`, where `it` is the macro's
doing. The useful case that [hygiene](#hygiene) makes awkward, and one reason
Common Lisp did not adopt it. ([17](part-2-reading-and-running/17-why-macros-matter.md))

**Allocation.** Setting aside space in the [heap](#heap) for a new value. In
Quokka it is a *bump*: take the next free words and move the mark along. No
searching, no free list — a gift from the [copying collector](#copying-collector).
([5](part-1-substrate/05-objects-in-the-heap.md))

**Backquote.** The template notation for building list structure: `` ` `` marks
a template, `,` a hole filled by evaluating something, `,@` a hole filled by
splicing a list's elements in. Handled entirely by the [reader](#reader), which
turns it into ordinary `cons`, `append` and `quote` calls — so it is not a
[special operator](#special-operator) and costs nothing at run time. Also
called *quasiquotation*, after Quine's corner quotes. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Bignum.** An integer too large to fit in one machine word, stored in the heap
as several. Contrast [fixnum](#fixnum). Not yet implemented.

**Bootstrapping.** Building a crude version of something, using it to build a better one, and then no longer needing the crude version. Quokka's compiler is written in Lisp, so a slow tree-walking interpreter in Rust runs it once — after which the compiler exists as bytecode and the slow path is never used again. ([The bootstrap](the-bootstrap.md))

**Brand.** A lifetime attached to a type purely so the compiler can refuse
certain programs — it points at nothing and exists at compile time only. Quokka
uses one to make it impossible to hold a value across a garbage collection.
([4](part-1-substrate/04-rooting.md))

**Bytecode.** A compact artificial instruction set that a [VM](#vm) executes, rather than the machine's own instructions. Smaller and more portable than compiling to real machine code, and much faster than re-reading the source each time. The VM that runs it is Rust and deliberately dumb; the compiler that produces it is Lisp. Not yet built. ([The bootstrap](the-bootstrap.md))

**Capture (variable capture).** A [macro](#macro) binding a name that the
caller also uses, so the caller's variable silently disappears inside the
expansion. The classic macro bug; Common Lisp's cure is to bind a
[gensym](#gensym) instead, and Scheme's is [hygiene](#hygiene). ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**car.** The first slot of a [cons cell](#cons-cell) — the item. The name is
an abbreviation of an IBM 704 instruction from 1958 and means nothing now. Read
it as "first". ([Preface](what-has-to-be-built.md))

**cdr.** The second slot of a [cons cell](#cons-cell) — the rest of the list.
Pronounced "could-er". Read it as "rest".
([Preface](what-has-to-be-built.md))

**Cheney's algorithm.** The [copying collector](#copying-collector) Quokka uses.
Its distinguishing trick is needing no extra memory to keep track of its work:
the objects it has already copied *are* the list of things still to examine.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Closure.** A function together with the scope it was written in. Capturing that scope is what lets `(lambda (x) (+ x n))` still know what `n` is after the call that bound `n` has returned. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Collection.** One run of the [garbage collector](#garbage-collection).

**Cons cell.** The two-slot box Lisp builds lists out of: a [car](#car) and a
[cdr](#cdr). Two words in the heap, with no header, because its type is known
from the reference that points at it. "Cons" is short for "construct".
([1](part-1-substrate/01-one-word-for-everything.md),
[5](part-1-substrate/05-objects-in-the-heap.md))

**Continuation.** What remains to be done after the current computation finishes. In Quokka they are [frames](#frame) on the evaluator's stack — ordinary heap objects rather than an abstraction, which is what will let a debugger show you one. ([9](part-2-reading-and-running/09-what-a-form-means.md))

**Copying collector.** A [garbage collector](#garbage-collection) that reclaims
memory by copying everything still wanted into a fresh region and abandoning the
old one. The garbage costs nothing to reclaim — you simply stop using the space
it was in. ([6](part-1-substrate/06-cheneys-algorithm.md))

**defun.** Makes a function and puts it in a [symbol](#symbol)'s *function* cell. In Common Lisp it is a macro over `setf symbol-function`; Quokka made it a special operator before it had [macros](#macro), and moving it up is now possible. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Differential testing.** Running the same test on several implementations and requiring them to agree. You need not know the right answer — only notice a disagreement, which takes your own understanding out of the loop. Quokka runs each Lisp file natively, under WebAssembly and on SBCL. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md))

**Dotted pair.** A [cons cell](#cons-cell) whose [cdr](#cdr) is not a list, written `(a . b)`. A list is the common case of a cons cell, not the only one; the dot is how you write the general case. Also called an *improper list*. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Dynamic extent.** Found by searching the stack at run time rather than by looking at the source. `catch` and `throw` work this way, so a function can abandon a computation on behalf of a caller it has never heard of. Contrast [lexical](#lexical-scope). ([14](part-2-reading-and-running/14-leaving-early.md))

**Environment.** The chain of scopes a name is looked up in: innermost first, then outward, then the symbol's own value cell for globals. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**eval-when.** Says whether a form runs at compile time, load time, or when executed. Looks obscure and is load-bearing: it is how a macro defined in a file exists by the time a later form in that file is compiled, and therefore how self-hosting is possible at all. ([13](part-2-reading-and-running/13-the-special-operators.md))

**Evaluator.** The part of a runtime that actually executes a program.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Explicit-control evaluator.** An evaluator built as a machine with visible
state — registers and a stack it owns — rather than as a function that calls
itself. The only kind that can stop halfway and resume. The name and the
technique are from *Structure and Interpretation of Computer Programs* §5.4.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Fixnum.** An integer small enough to be stored directly in one machine word,
with no heap space used. Quokka's are 63 bits. The traditional Lisp word;
contrast [bignum](#bignum).
([1](part-1-substrate/01-one-word-for-everything.md))

**Fixpoint.** The self-hosting test: the compiled compiler recompiles its own source, and the output must be identical to what the interpreted compiler produced. A mismatch means compilation changed its own meaning, which is a bug no ordinary test could find. ([The bootstrap](the-bootstrap.md))

**Flow analysis.** Working out, before a program runs, what it can possibly
do — which variables escape, which code is unreachable, whether a `block` is
ever returned from. A compiler's job; Quokka's interpreter does none of it,
which is why a `defun` that uses `return-from` loses its tail calls. ([16](part-2-reading-and-running/16-the-prelude.md))

**Forwarding pointer.** A note left behind in the abandoned space during a
[collection](#collection), saying where an object was moved to. It is what stops
a shared object being copied twice and a circular structure looping forever.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Frame.** One pending step of the evaluator — "I was in the middle of something, here is how to continue once a value arrives". See [continuation](#continuation). ([9](part-2-reading-and-running/09-what-a-form-means.md))

**funcall.** Applies whatever a variable *holds*, as opposed to whatever its name denotes as a function. A [Lisp-2](#lisp-2) cannot call a closure stored in a variable without it. ([9](part-2-reading-and-running/09-what-a-form-means.md))

**Garbage collection.** Working out automatically, while a program runs, which
values it can no longer reach, and reclaiming their memory. The program never
says "I am finished with this"; it is deduced from what is still reachable.
([3](part-1-substrate/03-what-is-alive.md))

**gensym.** A primitive returning a fresh [uninterned symbol](#uninterned-symbol),
named `G1`, `G2` and so on. Macros use it for temporary bindings, because a
symbol that is not in the [obarray](#obarray) cannot be named by anything the
caller writes — so it cannot be [captured](#capture-variable-capture). ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Generational collector.** A collector that exploits the fact that most values
die young, by examining recently created ones often and long-lived ones rarely.
Faster than copying everything each time, and more complicated. Planned, not
built. ([6](part-1-substrate/06-cheneys-algorithm.md))

**Homoiconicity.** A language whose programs are written in its own data
structures — Lisp code *is* a list. It sounds like trivia and is the reason
[macros](#macro) are a list-manipulating program rather than a text
preprocessor. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Heap.** The region of memory where values too big for a single word live. In
Quokka it is an array of 64-bit words the VM owns.
([3](part-1-substrate/03-what-is-alive.md))

**Hygiene.** The property, in Scheme's macro systems, that names introduced by
a macro are renamed automatically so [capture](#capture-variable-capture)
cannot happen. Quokka follows Common Lisp and does *not* have it: capture is an
error the language lets you make, and [gensym](#gensym) is the cure. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Hygienic macro expansion.** Scheme's answer to
[capture](#capture-variable-capture): the expander renames identifiers
introduced by a macro, so they cannot collide with the caller's, and names
*used* by a macro keep the meaning they had where it was written. Kohlbecker's
algorithm (1986). Quokka does not have it; see [gensym](#gensym). ([17](part-2-reading-and-running/17-why-macros-matter.md))

**Image.** The entire contents of a running system, written to a file and
loadable again — not a saved document but a saved *world*, resuming exactly
where it was. Booting means loading one rather than running a build.
([7](part-1-substrate/07-saving-a-world.md))

**Immediate.** A value stored entirely in its own word, referring to nothing:
a character, `nil`, `t`. Contrast a *reference*, which points into the heap.
([1](part-1-substrate/01-one-word-for-everything.md))

**Implicit progn.** A body that is a sequence of forms yielding the last — the bodies of `lambda`, `defun`, `let` and most else. Nobody writes the `progn`; it is what a body means. ([13](part-2-reading-and-running/13-the-special-operators.md))

**Interning.** Keeping one object per name, so that two reads of `foo` produce the *same* [symbol](#symbol) rather than two equal ones. Without it, `eq` on symbols fails and every variable lookup becomes a string comparison. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Invariance.** A property of a type's lifetime parameter meaning it cannot be
silently substituted for a shorter or longer one. Quokka's [brand](#brand) must
be invariant or the guarantee evaporates while everything still compiles.
([4](part-1-substrate/04-rooting.md))

**lambda.** Makes a function with no name. Its body is not evaluated when the `lambda` is — that happens when the function is called, which is why it must be a [special operator](#special-operator). ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Lexical addressing.** Resolving a variable to "scope 2, slot 0" ahead of time, so lookup is an index rather than a search. It requires a compiler to assign the numbers, which is why Quokka's interpreter cannot use it yet. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Lexical scope.** Determined by where something is written, not by what calls it. `block` and `return-from` are lexical, so a `return-from` can only reach a block it can see in the source — which makes it safe and makes it useless across a function boundary. ([14](part-2-reading-and-running/14-leaving-early.md))

**Lisp-1.** A Lisp where a symbol has one cell shared by variables and functions. Scheme is one; no `funcall` is needed, but functions and variables compete for names. Contrast [Lisp-2](#lisp-2). ([9](part-2-reading-and-running/09-what-a-form-means.md))

**Lisp-2.** A Lisp where a [symbol](#symbol) has separate value and function cells, so `(list list)` is legal — the first is the function, the second the variable. Common Lisp is one. Scheme, a *Lisp-1*, has a single cell. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Macro.** A function from code to code. It is called with its argument
*forms*, unevaluated, and the form it returns is evaluated in place of the
call — so it runs at *expansion time*, before anything else happens, and can
decide whether its arguments are evaluated at all. Macros are how a Lisp
program extends its own language without changing the [evaluator](#evaluator).
([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Macroexpansion.** Running a [macro](#macro) to get the form it produces.
Quokka does it inside the evaluator, with a frame that remembers the caller's
[environment](#environment) so the expansion is evaluated where the call was
written. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Marker.** A zero-slot object, equal to nothing but itself, used as the identity of a `block` or `tagbody`. The name cannot serve, because two recursive entries to the same block must be two different targets. ([14](part-2-reading-and-running/14-leaving-early.md))

**Mutation.** In Quokka, one bounded stretch of work on the heap. No
[collection](#collection) can happen inside one, which is what makes holding a
value safe. ([4](part-1-substrate/04-rooting.md))

**nil.** The empty list, false, and the symbol named "NIL" — one value doing three jobs, which is both the convenience and the problem. A real [symbol](#symbol) in the [static area](#static-area), not an immediate; it took four chapters and four special cases to get that right. ([11](part-2-reading-and-running/11-the-trouble-with-nil.md))

**Non-local exit.** Leaving a computation from the middle of it — `return-from`, `throw`. All of them are one mechanism differing only in how the target is found, and it is the same machinery a condition system needs. ([13](part-2-reading-and-running/13-the-special-operators.md))

**Obarray.** The table of every [interned](#interning) symbol. The name is from "object array" and is a 1960s MIT detail that stuck. Quokka's is a list for now, and a [root](#root) the collector must not forget. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Oracle.** A reference implementation trusted more than your own, used to check answers against. Available only when reimplementing something that already exists, which is why compilers and databases get a testing technique original software does not. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md))

**Parser.** What the [reader](#reader) is called in most languages, where it is far larger — it must know operator precedence, statements versus expressions, declarations. A Lisp reader needs none of that, because the text is already the data structure. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Prelude.** The Lisp that a Lisp system loads before any user code —
`lisp/prelude.lisp` here. Compiled into the binary with `include_str!`, since
the core VM has no filesystem, and interpreted at boot. `defun`, `when`,
`cond`, `and` and `or` live there rather than in Rust. ([16](part-2-reading-and-running/16-the-prelude.md))

**Primitive.** An operation the [VM](#vm) provides directly in Rust rather than
in Lisp — arithmetic, allocation, input and output. The *primitive table* is the
only door between the two languages.

**Printer.** Turns heap objects back into text. The *P* in [REPL](#repl), and how a [reader](#reader) is tested: text that survives read-then-print was almost certainly read correctly. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**progn.** Evaluates several forms in order and yields the last. It lets a sequence stand where one form is expected; the bodies of `lambda` and `defun` are implicit `progn`s. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Quote.** `'x` reads as the list `(quote x)` — ordinary structure, not syntax. The evaluator later treats `quote` as "give me this rather than running it", which is how a program says *data* rather than *code*. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Reader macro.** Syntax the [reader](#reader) expands into ordinary code
before the evaluator sees it. `'x` becoming `(quote x)` is the simplest one;
[backquote](#backquote) is the one that earns its keep. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Reachable.** Able to be found by following references from a [root](#root).
This is the definition of "still wanted" that garbage collection uses.
([3](part-1-substrate/03-what-is-alive.md))

**Reader.** The part of a runtime that turns text into structure. In Lisp it is
unusually small, because the structure it produces is ordinary lists.
([Preface](what-has-to-be-built.md))

**REPL.** Read, Eval, Print, Loop — a prompt that reads what you type, runs it,
prints the answer and comes back for more. Invented for Lisp in 1964; now in
every language. ([Preface](what-has-to-be-built.md))

**Root.** A location holding a value that is *not* inside the heap — a machine
register, a global table, a local variable in the implementation language.
Garbage collection starts its search from these. A root is a *place*; a
[value](#value) is a *thing*.
([3](part-1-substrate/03-what-is-alive.md))

**Rooting.** Telling the collector about a location it cannot find by itself.
([4](part-1-substrate/04-rooting.md))

**S-expression.** "Symbolic expression": Lisp's single notation, used for both
code and data. `(* 6 7)` is a list of three things, not special syntax.
([Preface](what-has-to-be-built.md))

**SBCL.** Steel Bank Common Lisp, a mature Common Lisp implementation, and Quokka's [oracle](#oracle). A reference to check answers against, never something Quokka needs in order to build. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md))

**Self-hosting.** When a compiler can compile its own source, so the system needs nothing outside it to rebuild. The point at which Quokka's Rust stops growing and every further feature is written in Lisp. ([The bootstrap](the-bootstrap.md))

**Semispace.** One of the two halves a [copying collector](#copying-collector)
works between. At any moment one holds everything and the other is empty.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Slice.** A bounded stretch of execution after which the VM returns control to
whoever called it, so a browser can repaint and respond. The caller says how
many steps. ([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Special operator.** A form that decides for itself whether, when, or in what environment its arguments are evaluated — or whose arguments are not expressions at all. `if` must be one, or both branches would run before it could choose. Common Lisp has exactly 25, they are a *basis* rather than a feature list, and everything else is macros over them. ([9](part-2-reading-and-running/09-what-a-form-means.md), [13](part-2-reading-and-running/13-the-special-operators.md))

**Static area.** The first words of the heap, which the collector copies verbatim and never relocates. A reference into it is therefore a compile-time constant, which is how `nil` can be an ordinary symbol *and* `Value::NIL` a Rust constant. `t` lives there too; the keyword package will. ([11](part-2-reading-and-running/11-the-trouble-with-nil.md))

**Symbol.** A named thing — `car`, `+`, `my-variable`. Lisp symbols are objects
in their own right, not just text, and carry a value and a function.

**Tag.** A few bits inside a word saying what kind of value it holds. Quokka
uses the low three bits, which are free because heap objects are aligned to
8-byte boundaries. A *tagged word* is a value plus its tag.
([1](part-1-substrate/01-one-word-for-everything.md))

**tagbody.** The primitive iteration construct: a body with labels that `go` jumps to. Almost never written directly; `do`, `dolist` and `loop` are macros that expand into it. ([13](part-2-reading-and-running/13-the-special-operators.md))

**Tail call.** A call whose result is immediately the result of the enclosing function, so the caller's [frame](#frame) is no longer needed. Eliminating it is what makes recursive loops run in constant space — which matters in Lisp, where recursion *is* iteration. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Torture mode.** A build setting that makes rare conditions happen constantly —
here, collecting garbage at every opportunity and checking the heap afterwards —
so latent bugs fail on the first test run rather than the thousandth.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Tracing.** Following references from the [roots](#root) to find everything
[reachable](#reachable). Also the thing you must *not* do to a string's
characters, which are not references.
([3](part-1-substrate/03-what-is-alive.md))

**Unbound.** The marker a [symbol](#symbol)'s value or function cell holds before anything is assigned. Distinct from `nil`: the difference between "this variable does not exist" and "this variable holds the empty list", which Lisp signals an error about. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Uninterned symbol.** A symbol that is not in the [obarray](#obarray), so
reading its printed name produces a *different* symbol. Nothing a program can
type refers to it; only whoever holds the object does. See [gensym](#gensym).
([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Unwinding.** Discarding frames until one matches, running any cleanups passed on the way. The third half-step of the machine, and what every non-local exit is made of. ([14](part-2-reading-and-running/14-leaving-early.md))

**Value.** One piece of Lisp data as the machine holds it: in Quokka, exactly one
64-bit word, either the data itself or a reference to somewhere in the heap.
([1](part-1-substrate/01-one-word-for-everything.md))

**VM.** Virtual machine: a program that behaves like a computer for the language
running on it, with its own memory and instructions. Quokka's is written in Rust
and does what a Lisp machine's hardware once did.

**WebAssembly (WASM).** A compact instruction format that runs inside a web
browser at close to native speed. Quokka compiles to it, which is why the VM
must be able to stop and hand control back.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Write barrier.** Machinery that notices when an old object is made to point at
a new one. A [generational](#generational-collector) collector needs it; the
simple copying collector here does not.
([6](part-1-substrate/06-cheneys-algorithm.md))
