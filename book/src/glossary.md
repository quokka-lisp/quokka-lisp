# Glossary

Every term this book uses that is not ordinary English, in a sentence or two,
with the chapter that explains it properly.

---


**Hash table.** A collection that finds a value from a key in roughly one
step rather than by searching. Quokka's hashes the key's *contents*, never its
address, because the collector moves objects and an address-based hash would
be correct until the next collection and silently wrong afterwards. ([19](part-2-reading-and-running/19-things-with-an-index.md))

**Header.** The first word of a heap object, saying what kind it is and how many
slots follow. Cons cells have none.
([5](part-1-substrate/05-objects-in-the-heap.md))

**External symbol.** One of a [package](#package)'s public symbols, written with one colon from outside it (`geometry:area`). The rest are *internal*, reachable with two colons (`geometry::helper`); and a symbol visible because the package *uses* another is *inherited*. `find-symbol` reports which. ([36](part-6-self-hosting/36-whose-symbol-is-it.md))

**Fetch–decode–execute.** The loop a [VM](#vm) runs: read the next
[opcode](#opcode), read however many operand bytes it takes, do the thing,
repeat. Real processors are built around the same three steps. Quokka's loop
also counts down a step budget, so it can stop between instructions (rule 4).
([25](part-3-compiling/25-a-machine-made-of-instructions.md))

**Hash consing.** Canonicalising structures so that two structurally equal
objects are also the *same* object, and can therefore be compared by pointer.
[Interning](#interning) is this applied to names. Unsound for anything mutable,
which is why Quokka's literals had to become read-only before sharing them
could even be considered. ([24](part-3-compiling/24-why-compile-at-all.md))

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

**apply.** Calls a function with a list of arguments — `(apply #'+ 1 '(2 3))`
is `(+ 1 2 3)`. Intercepted by the evaluator rather than implemented as a
primitive, because making a call is the machine's business. The last argument
is shared, not copied. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**aref.** Reads the element of a vector or string at an index, in one step
rather than by walking. Common Lisp calls both of them vectors, so one
function serves both, although one holds traced references and the other raw
character codes. ([19](part-2-reading-and-running/19-things-with-an-index.md))

**Bignum.** An integer too large to fit in one machine word, stored in the heap
as several. Contrast [fixnum](#fixnum). Not yet implemented.

**Blocking.** Stopping until something outside answers — a file is read, a
packet arrives. A program that blocks is fine; a *VM* that blocks inside a step
freezes everything above it. Quokka's computations block by leaving a
[request](#host) in the heap and ending the [slice](#slice); the VM itself
never waits. ([31](part-5-the-image/31-asking-the-outside-world.md))

**Bootstrapping.** Building a crude version of something, using it to build a better one, and then no longer needing the crude version. Quokka's compiler is written in Lisp, so a slow tree-walking interpreter in Rust runs it once — after which the compiler exists as bytecode and the slow path is never used again. ([The bootstrap](the-bootstrap.md))

**Brand.** A lifetime attached to a type purely so the compiler can refuse
certain programs — it points at nothing and exists at compile time only. Quokka
uses one to make it impossible to hold a value across a garbage collection.
([4](part-1-substrate/04-rooting.md))

**Branch relaxation.** The pass an assembler needs when instructions vary in
size: a jump's distance depends on the sizes of the instructions between it and
its target, and those sizes depend on the jump distances, so it iterates until
the two agree. Quokka avoids it by fixing each opcode's operand widths, which
makes an instruction's size a function of its opcode alone. ([24](part-3-compiling/24-why-compile-at-all.md))

**Bytecode.** A compact artificial instruction set that a [VM](#vm) executes, rather than the machine's own instructions. Smaller and more portable than compiling to real machine code, and much faster than re-reading the source each time. The VM that runs it is Rust and deliberately dumb; the compiler that produces it is Lisp. Chapter 24 is the plan and chapter 25 the machine. ([24](part-3-compiling/24-why-compile-at-all.md))

**Code generation.** The part of a compiler that turns an analysed form into
instructions — the dispatch on the shape of a form that most of a compiler
turns out to be. ([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

**Addressing mode.** How an instruction says *where* its operand is, rather
than what it is. Quokka's are: on the stack, written into the instruction, an
index into the function's constants, a `(depth, index)` pair into the scope
chain, or a symbol whose cell is read at run time. The term is from processor
design and carries over exactly. ([26](part-3-compiling/26-where-is-the-variable.md))

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

**Character encoding.** A rule for writing characters' numbers as bytes. *UTF-32* uses four bytes for every character; *UTF-8* uses one for ASCII and two to four for the rest, which is compact but means finding the *n*th character takes a scan; [Latin-1](#latin-1) uses one byte and covers only 256 characters. ([37](part-6-self-hosting/37-eight-characters-to-a-word.md))

**Cheney's algorithm.** The [copying collector](#copying-collector) Quokka uses.
Its distinguishing trick is needing no extra memory to keep track of its work:
the objects it has already copied *are* the list of things still to examine.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Closure.** A function together with the scope it was written in. Capturing that scope is what lets `(lambda (x) (+ x n))` still know what `n` is after the call that bound `n` has returned. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Collection.** One run of the [garbage collector](#garbage-collection).

**Completion queue.** A place where a program leaves requests and the system
leaves answers, so that neither has to wait for the other. Linux's io_uring and
Windows' I/O completion ports are built this way; so, with one request at a
time, is Quokka's [host](#host) interface. Also called the *proactor* pattern.
([31](part-5-the-image/31-asking-the-outside-world.md))

**Condition.** An error, or any other noteworthy situation, as an *object* a
program can catch rather than something that ends the computation. Quokka's
has a type symbol, a message and some data; Common Lisp's are class instances
matched by subtype. The word is not "exception" on purpose: a condition is
signalled before the stack unwinds, so a handler can in principle repair
things and resume. ([20](part-2-reading-and-running/20-errors-you-can-catch.md))

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

**defstruct.** Defines a record type from one line: a constructor, a
predicate, an accessor per slot that `setf` can write, and a copier. A macro
that writes ten functions, and the clearest demonstration of what macros are
for. Quokka's structures are vectors with the type name in the first slot,
since real types need a class system. ([21](part-2-reading-and-running/21-records.md))

**Differential testing.** Running the same test on several implementations and requiring them to agree. You need not know the right answer — only notice a disagreement, which takes your own understanding out of the loop. Quokka runs each Lisp file interpreted and compiled, natively and under WebAssembly, and on SBCL. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md), [33](part-5-the-image/33-two-evaluators-one-answer.md))

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

**Flat closure.** A [closure](#closure) that copies the values it captured into
itself when it is made, so each one is at a known index rather than found by
walking a chain of scopes. Cheaper to use and more expensive to create — the
opposite trade from the linked environments Quokka's interpreter uses, and an
open question for the compiler. ([24](part-3-compiling/24-why-compile-at-all.md))

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

**handler-case.** Runs a form, and if a condition is signalled inside it,
hands it to the first clause whose type matches. A [catch](#non-local-exit)
that matches on type instead of identity — the same unwinding, asking a
different question of each frame. ([20](part-2-reading-and-running/20-errors-you-can-catch.md))

**Host.** Whatever is running Quokka: the native terminal program, or the
browser tab. The VM may not touch files, clocks or screens itself (rule 2), so
it gets everything from the outside world through its host — by a **host
request**: a record left in the heap that the host reads, answers, and writes
back. ([31](part-5-the-image/31-asking-the-outside-world.md))

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

**Instruction selection.** Choosing *which* instructions express a form, when
several would work. `(+ 1 2)` can be a function-cell read and a call, or a
single `PRIM`; both are correct and one is faster. Where most of a compiler's
judgement lives, and most of what changes as one improves. ([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

**Literal operand.** An operand that *is* the value rather than directions for
finding it — the `7` in `FIX 7`. Machine-code writing usually calls this an
*immediate*, which this book avoids here because
[immediate](#immediate) already means a tagged value needing no heap object.
Two unrelated meanings, one word. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

**Implicit progn.** A body that is a sequence of forms yielding the last — the bodies of `lambda`, `defun`, `let` and most else. Nobody writes the `progn`; it is what a body means. ([13](part-2-reading-and-running/13-the-special-operators.md))

**Interning.** Keeping one object per name, so that two reads of `foo` produce the *same* [symbol](#symbol) rather than two equal ones. Without it, `eq` on symbols fails and every variable lookup becomes a string comparison. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Label.** A name standing for a position in a function's instructions, so
that the compiler can emit a jump before knowing how far it goes. The
assembler resolves them; the compiler uses `gensym` for them, so that a nested
`if` does not collide with itself. ([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

**Opcode.** Short for *operation code*: the number at the start of an
instruction saying which operation it is. `CONST`, `LREF` and `JMP` are names
this book gives to opcodes so they can be read; the machine sees bytes.
([25](part-3-compiling/25-a-machine-made-of-instructions.md))

**Literal.** An object written directly in the source — `"hello"`, `#(1 2 3)`,
a quoted list — as opposed to one a program builds. Common Lisp leaves the
consequences of modifying one undefined (CLHS 3.7.1); Quokka makes source
strings and symbol names a separate heap type that refuses to be written, so
the undefined case cannot arise. ([24](part-3-compiling/24-why-compile-at-all.md))

**Invariance.** A property of a type's lifetime parameter meaning it cannot be
silently substituted for a shorter or longer one. Quokka's [brand](#brand) must
be invariant or the guarantee evaporates while everything still compiles.
([4](part-1-substrate/04-rooting.md))

**lambda.** Makes a function with no name. Its body is not evaluated when the `lambda` is — that happens when the function is called, which is why it must be a [special operator](#special-operator). ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Jump table.** A row of jumps, all the same size, one per destination, so
that "go to the *i*th place" becomes "jump to the *i*th row" — arithmetic
instead of a search. The classic way to compile a computed goto, or a `switch`
on small integers. A `tagbody`'s `go` lands on one.
([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

**Keyword.** A symbol written with a leading colon — `:foo` — that evaluates
to itself. The colon is punctuation, not part of the name: `(symbol-name :foo)`
is `"FOO"`, and `:foo` is a *different symbol* from `foo` with the same name.
In Common Lisp that is because it lives in the `KEYWORD`
[package](#package); Quokka has no packages, so symbols carry a namespace
marker instead. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Lambda list.** A function's parameter list, which in Common Lisp says more
than a list of names: `&optional` parameters with default *forms*, `&key`
parameters supplied by name, supplied-p variables, and `&aux` locals. Quokka
rewrites the whole thing into a simple list plus a `let*` when the closure is
made, so nothing has to be evaluated while arguments are being bound. ([18](part-2-reading-and-running/18-lambda-lists.md))

**Lexical addressing.** Resolving a variable to "scope 2, slot 0" ahead of time, so lookup is an index rather than a search. It requires a compiler to assign the numbers, which is why Quokka's interpreter cannot use it yet. Phase 1 is the *yet*. ([10](part-2-reading-and-running/10-functions-and-scope.md), [24](part-3-compiling/24-why-compile-at-all.md))

**Lexical scope.** Determined by where something is written, not by what calls it. `block` and `return-from` are lexical, so a `return-from` can only reach a block it can see in the source — which makes it safe and makes it useless across a function boundary. ([14](part-2-reading-and-running/14-leaving-early.md))

**Latin-1.** The [character encoding](#character-encoding) ISO 8859-1: one byte per character, covering Western European text. Its 256 codes are exactly Unicode's first 256, so a Latin-1 byte *is* the character's Unicode number. Quokka packs constant strings in it, eight to a word. ([37](part-6-self-hosting/37-eight-characters-to-a-word.md))

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

**Keyword argument.** An argument supplied by name — `(draw 'square :colour
'red)` — rather than by position, so a function can gain an option without
breaking its callers. Matched by [keyword](#keyword), leftmost occurrence
first, and an unrecognised one is an error unless `&allow-other-keys` says
otherwise. ([18](part-2-reading-and-running/18-lambda-lists.md))

**Supplied-p parameter.** The third element of an `&optional` or `&key` entry,
bound to true when an argument was actually given. It exists because `nil` is
a perfectly good argument, so "absent" and "nil" have to be different things.
([18](part-2-reading-and-running/18-lambda-lists.md))

**macroexpand.** Runs a [macro](#macro) and returns the form it produced,
without evaluating it — the first debugging tool a macro system needs, since
what a macro produces is otherwise invisible. `macroexpand-1` does one step;
`macroexpand` repeats until the head is no longer a macro. Neither descends
into subforms. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Marker.** A zero-slot object, equal to nothing but itself, used as the identity of a `block` or `tagbody`. The name cannot serve, because two recursive entries to the same block must be two different targets. ([14](part-2-reading-and-running/14-leaving-early.md))

**Multiple values.** A form may return any number of results, and a caller
that wants one writes nothing special — the extras evaporate. Different from
returning a list, which every caller would have to take apart. `gethash`
returns the value *and* whether the key was there, which is the only way to
tell a stored nil from a missing key. ([23](part-2-reading-and-running/23-more-than-one-answer.md))

**macrolet.** Binds a macro with a lexical scope, as `flet` binds a function.
Needed no new machinery here, because the evaluator already looked operators
up through the environment and already asked whether what it found was a
macro. Its expander runs in a null lexical environment — it cannot see the
surrounding variables, because at expansion time they do not have values yet.
([23](part-2-reading-and-running/23-more-than-one-answer.md))

**Mutation.** In Quokka, one bounded stretch of work on the heap. No
[collection](#collection) can happen inside one, which is what makes holding a
value safe. ([4](part-1-substrate/04-rooting.md))

**nil.** The empty list, false, and the symbol named "NIL" — one value doing three jobs, which is both the convenience and the problem. A real [symbol](#symbol) in the [static area](#static-area), not an immediate; it took four chapters and four special cases to get that right. ([11](part-2-reading-and-running/11-the-trouble-with-nil.md))

**Non-local exit.** Leaving a computation from the middle of it — `return-from`, `throw`. All of them are one mechanism differing only in how the target is found, and it is the same machinery a condition system needs. ([13](part-2-reading-and-running/13-the-special-operators.md))

**Obarray.** The table of every [interned](#interning) symbol. The name is from "object array" and is a 1960s MIT detail that stuck. Quokka's is a list for now, and a [root](#root) the collector must not forget. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Operand.** The part of an instruction after the [opcode](#opcode), saying
what to operate on. Quokka's instructions take nought to two, and how many —
and how wide — is fixed by the opcode, which is what spares the assembler a
[branch relaxation](#branch-relaxation) pass. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

**Oracle.** A reference implementation trusted more than your own, used to check answers against. Available only when reimplementing something that already exists, which is why compilers and databases get a testing technique original software does not. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md))

**Package.** Common Lisp's namespace for symbols: a symbol table plus rules
about which names are visible where. Quokka does not have them — tier 1, phase
2 — but symbols do carry a *home namespace*, with two possible values, because
[keywords](#keyword) cannot be represented without one. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Parser.** What the [reader](#reader) is called in most languages, where it is far larger — it must know operator precedence, statements versus expressions, declarations. A Lisp reader needs none of that, because the text is already the data structure. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Promise.** In a browser, an object standing for an answer that has not
arrived yet. JavaScript that asks for something slow gets a Promise back at
once and finishes; when the answer comes, the browser runs a callback with it.
The reason a browser host cannot answer a [host request](#host) on the spot.
([31](part-5-the-image/31-asking-the-outside-world.md))

**Prelude.** The Lisp that a Lisp system loads before any user code —
`lisp/prelude.lisp` here. Compiled into the binary with `include_str!`, since
the core VM has no filesystem, and interpreted at boot. `defun`, `when`,
`cond`, `and` and `or` live there rather than in Rust. ([16](part-2-reading-and-running/16-the-prelude.md))

**Percent name.** A name beginning with `%` — `%set-aref`, `%macro-function`
— marking something as Quokka's own rather than Common Lisp's, and not to be
written in a program. It says nothing about *what* the thing is: most are
ordinary functions, some written in Rust and some in Lisp, and exactly one is
a special operator. The convention stands in for a [package](#package), which
is phase 2, and is enforced by nothing until then. ([22](part-2-reading-and-running/22-the-percent-names.md))

**Primitive.** An operation the [VM](#vm) provides directly in Rust rather than
in Lisp — arithmetic, allocation, input and output. The *primitive table* is the
only door between the two languages.

**Prevalence.** Keeping a system's data as live objects in memory rather than
in a separate database, and making it survive a restart with a log of every
change plus periodic snapshots. Named after the Prevayler project; Common Lisp
has `bknr.datastore`. Quokka's image save is the snapshot half, and it has no
log — [#24](https://github.com/quokka-lisp/quokka-lisp/issues/24) is the open question. ([Further reading](further-reading.md))

**Printer.** Turns heap objects back into text. The *P* in [REPL](#repl), and how a [reader](#reader) is tested: text that survives read-then-print was almost certainly read correctly. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**progn.** Evaluates several forms in order and yields the last. It lets a sequence stand where one form is expected; the bodies of `lambda` and `defun` are implicit `progn`s. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Quote.** `'x` reads as the list `(quote x)` — ordinary structure, not syntax. The evaluator later treats `quote` as "give me this rather than running it", which is how a program says *data* rather than *code*. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

**Quicklisp.** Common Lisp's library manager: an index of a few thousand
libraries and a downloader, so `(ql:quickload "mito")` fetches a library and
its dependencies and loads them into the Lisp you are running. Each release is
a *curated snapshot* — versions known to build together — which is the choice
that made it trusted. It builds on ASDF (Another System Definition Facility),
the build system that compiles and loads a set of files in dependency order.
Quokka has no equivalent and will eventually need one. ([Further reading](further-reading.md))

**Reader macro.** Syntax the [reader](#reader) expands into ordinary code
before the evaluator sees it. `'x` becoming `(quote x)` is the simplest one;
[backquote](#backquote) is the one that earns its keep. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

**Raw slots.** An object whose words are *not* references — a string's
character codes, a bignum's digits — and which the collector must copy rather
than follow. Quokka draws the line at type number 64: below it slots are
traced, at or above they are raw. A type on the wrong side corrupts the heap
silently, so a test guards it. ([19](part-2-reading-and-running/19-things-with-an-index.md))

**Reachable.** Able to be found by following references from a [root](#root).
This is the definition of "still wanted" that garbage collection uses.
([3](part-1-substrate/03-what-is-alive.md))

**Reader.** The part of a runtime that turns text into structure. In Lisp it is
unusually small, because the structure it produces is ordinary lists.
([Preface](what-has-to-be-built.md))

**Program counter (`Pc`).** The offset of the next instruction. Keeping it in
a heap register rather than a Rust local is what lets the VM stop between
instructions and resume later — and what lets an image be saved mid-run.
([25](part-3-compiling/25-a-machine-made-of-instructions.md))

**Register machine.** An instruction set whose instructions name where their
operands are — `ADD r3, r1, r2` — like real hardware. Fewer, larger
instructions than a [stack machine](#stack-machine), and a harder compiler,
because something must decide which value lives in which register. ([24](part-3-compiling/24-why-compile-at-all.md))

**REPL.** Read, Eval, Print, Loop — a prompt that reads what you type, runs it,
prints the answer and comes back for more. Invented for Lisp in 1964; now in
every language. ([Preface](what-has-to-be-built.md))

**Root.** A location holding a value that is *not* inside the heap — a machine
register, a global table, a local variable in the implementation language.
Garbage collection starts its search from these. A root is a *place*; a
[value](#value) is a *thing*.
([3](part-1-substrate/03-what-is-alive.md))

**Operand stack.** The pile of values that instructions push to and pop from.
`FIX 2`, `FIX 3`, `PRIM *` leaves `6` where the two numbers were. Expressions
nest and a stack nests, which is why compiling to a stack machine is simpler
than compiling to anything else. In Quokka it is a vector in the heap, not a
Rust local, because of rule 4. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

**Rooting.** Telling the collector about a location it cannot find by itself.
([4](part-1-substrate/04-rooting.md))

**S-expression.** "Symbolic expression": Lisp's single notation, used for both
code and data. `(* 6 7)` is a list of three things, not special syntax.
([Preface](what-has-to-be-built.md))

**SBCL.** Steel Bank Common Lisp, a mature Common Lisp implementation, and Quokka's [oracle](#oracle). A reference to check answers against, never something Quokka needs in order to build. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md))

**SLIME.** Superior Lisp Interaction Mode for Emacs: the usual way to develop
Common Lisp, where the editor is one program and Lisp is another, and the two
talk over a socket using a protocol called Swank. Its feature list — evaluate
the form at point, jump to a definition, show arguments while you type — is a
good specification for Quokka's own environment. Its architecture is the one
Quokka does not have, because the editor will live inside the image. ([Further reading](further-reading.md))

**Self-hosting.** When a compiler can compile its own source, so the system needs nothing outside it to rebuild. The point at which Quokka's Rust stops growing and every further feature is written in Lisp. ([The bootstrap](the-bootstrap.md))

**Sequence.** Common Lisp's name for anything `length` and `elt` work on:
lists, vectors and strings. The functions over them dispatch on which kind
they were given. ([19](part-2-reading-and-running/19-things-with-an-index.md))

**setf.** The general way to write a place in Common Lisp: `(setf (aref v i)
x)`, `(setf (gethash k h) v)`, `(setf x 1)`. A macro that turns a *reader* —
`aref`, `gethash` — into the corresponding writer, so a program names what it
wants to change in the same words it would use to read it. Quokka's knows a
fixed list of places; the real one can be taught new ones. ([19](part-2-reading-and-running/19-things-with-an-index.md))

**setf expander.** How `setf` is taught a place it has never seen.
Quokka's is a table from a reader's name to a writer's name, which is what
`defstruct` needs. Common Lisp's `define-setf-expander` hands back five
values instead, so that a place's subforms can be evaluated exactly once even
when something reads *and* writes it — which is why Quokka has no `incf`.
([21](part-2-reading-and-running/21-records.md))

**Semispace.** One of the two halves a [copying collector](#copying-collector)
works between. At any moment one holds everything and the other is empty.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Shallow binding.** One way to implement [special variables](#special-variable): the current value lives in the symbol, binding saves the old value and stores the new one, and ending the binding puts the old one back. Reading is one load; switching threads has to swap bindings. Its alternative, *deep binding*, keeps bindings in a per-thread list that every read searches. Quokka uses shallow binding. ([34](part-6-self-hosting/34-standing-on-its-own.md))

**Slice.** A bounded stretch of execution after which the VM returns control to
whoever called it, so a browser can repaint and respond. The caller says how
many steps. ([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

**Stack pointer.** How deep the [operand stack](#operand-stack) currently is.
Popped slots are cleared, because every slot of a traced object is traced and
a dead one would keep what it last held alive. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

**Special variable.** A variable whose binding is visible to everything *called* while the binding is in force, not only to the code written inside it — what other languages call dynamic scope. Common Lisp names them with asterisks, like `*package*`, and makes them with `defvar` and `defparameter`. ([34](part-6-self-hosting/34-standing-on-its-own.md))

**Special operator.** A form that decides for itself whether, when, or in what environment its arguments are evaluated — or whose arguments are not expressions at all. `if` must be one, or both branches would run before it could choose. Common Lisp has exactly 25, they are a *basis* rather than a feature list, and everything else is macros over them. ([9](part-2-reading-and-running/09-what-a-form-means.md), [13](part-2-reading-and-running/13-the-special-operators.md))

**Static area.** The first words of the heap, which the collector copies verbatim and never relocates. A reference into it is therefore a compile-time constant, which is how `nil` can be an ordinary symbol *and* `Value::NIL` a Rust constant. `t` lives there too; the keyword package will. ([11](part-2-reading-and-running/11-the-trouble-with-nil.md))

**Stack machine.** An instruction set whose instructions take no operands and
work on an implicit stack: `CONST 3`, `CONST 4`, `ADD`. Tiny instructions and a
simple compiler, at the cost of more of them. What Quokka's bytecode will be,
chosen for size rather than speed. ([24](part-3-compiling/24-why-compile-at-all.md))

**String.** A heap object whose slots hold character codes rather than
[values](#value), which is why it sits on the raw side of the collector's line
(see [raw slots](#raw-slots)). Common Lisp counts it as a vector, so `length`
and [aref](#aref) work on one. A writable string holds one character per word;
a constant one whose characters are all [Latin-1](#latin-1) holds eight. ([8](part-2-reading-and-running/08-text-becomes-structure.md), [19](part-2-reading-and-running/19-things-with-an-index.md), [37](part-6-self-hosting/37-eight-characters-to-a-word.md))

**System call.** How a program asks an operating system's kernel to do
something it may not do itself, such as read a disk. The program hands over a
request, is put to sleep if the answer is not ready, and wakes with the result.
Quokka's [host requests](#host) are the same shape: the computation is the
program and the host is the kernel.
([31](part-5-the-image/31-asking-the-outside-world.md))

**Symbol.** A named thing — `car`, `+`, `my-variable`. Lisp symbols are objects
in their own right, not just text, and carry a value and a function.

**Simple vector.** A fixed-length run of slots holding any values, indexed
by [aref](#aref). The only kind of array Quokka has: no fill pointers, no
adjustable arrays, no dimensions beyond one. ([19](part-2-reading-and-running/19-things-with-an-index.md))

**Tag.** A few bits inside a word saying what kind of value it holds. Quokka
uses the low three bits, which are free because heap objects are aligned to
8-byte boundaries. A *tagged word* is a value plus its tag.
([1](part-1-substrate/01-one-word-for-everything.md))

**tagbody.** The primitive iteration construct: a body with labels that `go` jumps to. Almost never written directly; `do`, `dolist` and `loop` are macros that expand into it. ([13](part-2-reading-and-running/13-the-special-operators.md))

**Tier.** One of the three stages in which Quokka's Common Lisp subset grows, each the minimum needed to unlock a phase. Tier 0 is enough to run the compiler; tier 1 is the self-hosting core — packages, the reader and printer in Lisp, bignums, floats; tier 2 is what the environment needs, such as CLOS and streams. ([34](part-6-self-hosting/34-standing-on-its-own.md))

**Transparent to values.** Said of a form that returns however many values
its last form returned — `block`, `let`, `progn`, `if`, `unwind-protect` and
most others. Mostly free here, because a form in [tail position](#tail-call)
never re-delivers; the three that were not free each read the value register
and delivered it again, which was *almost* right. ([23](part-2-reading-and-running/23-more-than-one-answer.md))

**Tail call.** A call whose result is immediately the result of the enclosing function, so the caller's [frame](#frame) is no longer needed. Eliminating it is what makes recursive loops run in constant space — which matters in Lisp, where recursion *is* iteration. ([10](part-2-reading-and-running/10-functions-and-scope.md))

**Torture mode.** A build setting that makes rare conditions happen constantly —
here, collecting garbage at every opportunity and checking the heap afterwards —
so latent bugs fail on the first test run rather than the thousandth.
([6](part-1-substrate/06-cheneys-algorithm.md))

**Tracing.** Following references from the [roots](#root) to find everything
[reachable](#reachable). Also the thing you must *not* do to a string's
characters, which are not references.
([3](part-1-substrate/03-what-is-alive.md))

**Trampoline.** A small piece of code whose only job is to jump into
something else. Quokka's host uses a four-instruction one to call a compiled
Lisp function on the VM, so that a compiled image never needs the interpreter.
([32](part-5-the-image/32-a-world-made-of-bytecode.md))

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
