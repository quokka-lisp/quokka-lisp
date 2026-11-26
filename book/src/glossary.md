# Glossary

Every term this book uses that is not ordinary English, in a sentence or two,
with the chapter that explains it properly.

---

<a id="addressing-mode"></a>
**Addressing mode.** How an instruction says *where* its operand is, rather
than what it is. Quokka's are: on the stack, written into the instruction, an
index into the function's constants, a `(depth, index)` pair into the scope
chain, or a symbol whose cell is read at run time. The term is from processor
design and carries over exactly. ([26](part-3-compiling/26-where-is-the-variable.md))

<a id="adjustable-array"></a>
**Adjustable array.** An array that `adjust-array` can resize while it stays the same object, so every reference to it sees the new size. In Quokka its header gets new data. ([55](part-6-self-hosting/55-arrays.md))

<a id="allocation"></a>
**Allocation.** Setting aside space in the [heap](#heap) for a new value. In
Quokka it is a *bump*: take the next free words and move the mark along. No
searching, no free list — a gift from the [copying collector](#copying-collector).
([5](part-1-substrate/05-objects-in-the-heap.md))

<a id="anaphoric-macro"></a>
**Anaphoric macro.** A macro that deliberately introduces a binding the
caller can see — `(aif (lookup x) (greet it))`, where `it` is the macro's
doing. The useful case that [hygiene](#hygiene) makes awkward, and one reason
Common Lisp did not adopt it. ([17](part-2-reading-and-running/17-why-macros-matter.md))

<a id="apply"></a>
**apply.** Calls a function with a list of arguments — `(apply #'+ 1 '(2 3))`
is `(+ 1 2 3)`. Intercepted by the evaluator rather than implemented as a
primitive, because making a call is the machine's business. The last argument
is shared, not copied. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="aref"></a>
**aref.** Reads the element of a vector or string at an index, in one step
rather than by walking. Common Lisp calls both of them vectors, so one
function serves both, although one holds traced references and the other raw
character codes. ([19](part-2-reading-and-running/19-things-with-an-index.md))

<a id="array-header"></a>
**Array header.** A small heap object in front of a [simple array](#simple-array) that holds an array's elements, recording the array's dimensions, its fill pointer, where in the simple array its elements start, and whether it is displaced or adjustable. Rust's `aref` and `length` look through it. ([55](part-6-self-hosting/55-arrays.md))

<a id="backquote"></a>
**Backquote.** The template notation for building list structure: `` ` `` marks
a template, `,` a hole filled by evaluating something, `,@` a hole filled by
splicing a list's elements in. Handled entirely by the [reader](#reader), which
turns it into ordinary `cons`, `append` and `quote` calls — so it is not a
[special operator](#special-operator) and costs nothing at run time. Also
called *quasiquotation*, after Quine's corner quotes. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="bignum"></a>
**Bignum.** An integer too large to fit in one machine word, stored in the heap
as several. Contrast [fixnum](#fixnum). ([47](part-6-self-hosting/47-bignums.md))

<a id="bit-vector"></a>
**Bit vector.** A vector whose elements are 0 or 1, written `#*1011`. Quokka packs 64 to a word. ([55](part-6-self-hosting/55-arrays.md))

<a id="blocking"></a>
**Blocking.** Stopping until something outside answers — a file is read, a
packet arrives. A program that blocks is fine; a *VM* that blocks inside a step
freezes everything above it. Quokka's computations block by leaving a
[request](#host) in the heap and ending the [slice](#slice); the VM itself
never waits. ([31](part-5-the-image/31-asking-the-outside-world.md))

<a id="boa-constructor"></a>
**BOA constructor.** A [defstruct](#defstruct) constructor whose parameters are slot names taken "by order of arguments" rather than as keywords: `(:constructor make-point (x y))` makes `(make-point 1 2)`. A parameter that gives no default takes the slot's own initial value (CLHS 3.4.6). ([61](part-6-self-hosting/61-structures.md))

<a id="bootstrapping"></a>
**Bootstrapping.** Building a crude version of something, using it to build a better one, and then no longer needing the crude version. Quokka's compiler is written in Lisp, so a slow tree-walking interpreter in Rust runs it once — after which the compiler exists as bytecode and the slow path is never used again. ([The bootstrap](the-bootstrap.md))

<a id="boxing"></a>
**Boxing.** Storing a value as a separate heap object rather than inside
the word that refers to it. A fixnum or a character is *unboxed* — the word
is the value — while a bignum or a float is *boxed*, so two equal ones can be
two objects. Why `eql` exists beside `eq`. ([45](part-6-self-hosting/45-sequences.md), [48](part-6-self-hosting/48-floats.md))

<a id="branch-relaxation"></a>
**Branch relaxation.** The pass an assembler needs when instructions vary in
size: a jump's distance depends on the sizes of the instructions between it and
its target, and those sizes depend on the jump distances, so it iterates until
the two agree. Quokka avoids it by fixing each opcode's operand widths, which
makes an instruction's size a function of its opcode alone. ([24](part-3-compiling/24-why-compile-at-all.md))

<a id="brand"></a>
**Brand.** A lifetime attached to a type purely so the compiler can refuse
certain programs — it points at nothing and exists at compile time only. Quokka
uses one to make it impossible to hold a value across a garbage collection.
([4](part-1-substrate/04-rooting.md))

<a id="bytecode"></a>
**Bytecode.** A compact artificial instruction set that a [VM](#vm) executes, rather than the machine's own instructions. Smaller and more portable than compiling to real machine code, and much faster than re-reading the source each time. The VM that runs it is Rust and deliberately dumb; the compiler that produces it is Lisp. Chapter 24 is the plan and chapter 25 the machine. ([24](part-3-compiling/24-why-compile-at-all.md))

<a id="byte-specifier"></a>
**Byte specifier.** What `(byte size position)` returns: the name of a field of SIZE bits in an integer, starting POSITION bits from the least significant. Not eight bits, despite the name. `ldb` reads such a field and `dpb` replaces it; in Quokka, as in SBCL, a byte specifier is a cons of the two numbers. ([57](part-6-self-hosting/57-bit-fields.md))

<a id="call-method"></a>
**Call-method.** An operator that exists only inside an [effective method form](#effective-method-form): `(call-method m next-methods)` calls the method `m`, whose `call-next-method` then reaches the first of NEXT-METHODS. Its companion `(make-method form)` stands for a method made of FORM. In Quokka both are local macros of the compiled effective method. ([77](part-7-objects/77-method-combination.md))

<a id="capture-variable-capture"></a>
**Capture (variable capture).** A [macro](#macro) binding a name that the
caller also uses, so the caller's variable silently disappears inside the
expansion. The classic macro bug; Common Lisp's cure is to bind a
[gensym](#gensym) instead, and Scheme's is [hygiene](#hygiene). ([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="canonical-type"></a>
**Canonical type.** A [type specifier](#type-specifier) rewritten into a standard form, so that two specifiers naming the same set of objects are written the same way. Quokka's, like SBCL's *ctypes*, divide the world into disjoint [kinds](#kind) and say for each kind which of its objects the type holds; `subtypep` then asks whether *A and not B* is empty. ([72](part-7-objects/72-what-is-a-type.md))

<a id="car"></a>
**car.** The first slot of a [cons cell](#cons-cell) — the item. The name is
an abbreviation of an IBM 704 instruction from 1958 and means nothing now. Read
it as "first". ([Preface](what-has-to-be-built.md))

<a id="cdr"></a>
**cdr.** The second slot of a [cons cell](#cons-cell) — the rest of the list.
Pronounced "could-er". Read it as "rest".
([Preface](what-has-to-be-built.md))

<a id="character-encoding"></a>
**Character encoding.** A rule for writing characters' numbers as bytes. *UTF-32* uses four bytes for every character; *UTF-8* uses one for ASCII and two to four for the rest, which is compact but means finding the *n*th character takes a scan; [Latin-1](#latin-1) uses one byte and covers only 256 characters. ([37](part-6-self-hosting/37-eight-characters-to-a-word.md))

<a id="class"></a>
**Class.** In CLOS, an object that names a kind of object and says what slots its instances have; every Lisp object has one, which `class-of` returns. A class is itself an instance of a [metaclass](#metaclass). ([73](part-7-objects/73-classes.md))

<a id="class-precedence-list"></a>
**Class precedence list.** One total order of a class and all its superclasses, keeping every class's request to come before its superclasses and those in the order it lists them: a topological sort (CLHS 4.3.5). It decides which superclass wins when two define the same slot or method. ([73](part-7-objects/73-classes.md))

<a id="cheney-s-algorithm"></a>
**`change-class`.** The Common Lisp function that gives an existing instance another class, keeping it the same object: slots both classes have keep their values, and the generic function `update-instance-for-different-class` fills the rest. ([76](part-7-objects/76-changing-classes.md))

**Cheney's algorithm.** The [copying collector](#copying-collector) Quokka uses.
Its distinguishing trick is needing no extra memory to keep track of its work:
the objects it has already copied *are* the list of things still to examine.
([6](part-1-substrate/06-cheneys-algorithm.md))

<a id="closure"></a>
**Closure.** A function together with the scope it was written in. Capturing that scope is what lets `(lambda (x) (+ x n))` still know what `n` is after the call that bound `n` has returned. ([10](part-2-reading-and-running/10-functions-and-scope.md))

<a id="code-generation"></a>
**Code generation.** The part of a compiler that turns an analysed form into
instructions — the dispatch on the shape of a form that most of a compiler
turns out to be. ([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

<a id="cold-build"></a>
**Cold build.** Making a world from nothing, from outside it: Quokka's `boot::build_image`, in Rust, with the interpreter. Contrast the *warm rebuild*, in which a running world remakes itself from its own source. The term is SBCL's, whose build makes a *cold core*. ([41](part-6-self-hosting/41-the-image-rebuilds-itself.md))

<a id="collection"></a>
**Collection.** One run of the [garbage collector](#garbage-collection).

<a id="compiler-macro"></a>
**Compiler macro.** An optional rewrite of calls to a global function, applied by the compiler before it compiles the call: `(define-compiler-macro square (&whole form x) (if (numberp x) (* x x) form))` turns `(square 5)` into `25` and leaves `(square y)` alone by returning its own form. The function itself is unchanged and still callable. Quokka's compiler applies them, except where a local function of the same name is in scope. ([53](part-6-self-hosting/53-compiling-on-request.md))

<a id="completion-queue"></a>
**Completion queue.** A place where a program leaves requests and the system
leaves answers, so that neither has to wait for the other. Linux's io_uring and
Windows' I/O completion ports are built this way; so, with one request at a
time, is Quokka's [host](#host) interface. Also called the *proactor* pattern.
([31](part-5-the-image/31-asking-the-outside-world.md))

<a id="condition"></a>
**Condition.** An error, or any other noteworthy situation, as an *object* a
program can catch rather than something that ends the computation. Quokka's
has a type symbol, a message and some data; Common Lisp's are class instances
matched by subtype. The word is not "exception" on purpose: a condition is
signalled before the stack unwinds, so a handler can in principle repair
things and resume. ([20](part-2-reading-and-running/20-errors-you-can-catch.md))

<a id="conformance-test-suite"></a>
**Conformance test suite.** A large collection of tests that checks an implementation against a standard rather than against its own intentions. Common Lisp's is *ansi-test*, some seventeen thousand tests written by Paul Dietz from 2002 on, organised by the chapters of the standard. Quokka runs fourteen of its groups. ([49](part-6-self-hosting/49-ansi-test.md))

<a id="cons-cell"></a>
**Cons cell.** The two-slot box Lisp builds lists out of: a [car](#car) and a
[cdr](#cdr). Two words in the heap, with no header, because its type is known
from the reference that points at it. "Cons" is short for "construct".
([1](part-1-substrate/01-one-word-for-everything.md),
[5](part-1-substrate/05-objects-in-the-heap.md))

<a id="constant-variable"></a>
**Constant variable.** A variable whose value never changes: `t`, `nil`, every keyword, and any name `defconstant` defines, such as `pi`. Binding or assigning one is refused; Quokka's compiler refuses it with a `program-error`. ([51](part-6-self-hosting/51-symbol-cells.md))

<a id="constituent-trait"></a>
**Constituent trait.** A property a character keeps whatever its [syntax type](#syntax-type): whether it is a letter, a digit, a package marker, or *invalid*. An invalid character (whitespace, backspace, rubout) may not appear unescaped in a token, and reading one there signals `reader-error`. ([58](part-6-self-hosting/58-the-readers-last-gaps.md))

<a id="content-hash"></a>
**Content hash.** A hash computed from what a key contains rather than where it lives, so that two equal keys that are different objects meet in one bucket. Quokka's looks a bounded way into conses and vectors, and gives an object with nothing but its identity a number for its type, so the collector moving it never changes the hash. It is also `sxhash`. ([59](part-6-self-hosting/59-hash-tables-finished.md))

<a id="continuation"></a>
**Continuation.** What remains to be done after the current computation finishes. In Quokka they are [frames](#frame) on the evaluator's stack — ordinary heap objects rather than an abstraction, which is what will let a debugger show you one. ([9](part-2-reading-and-running/09-what-a-form-means.md))

<a id="control-string"></a>
**Control string.** The template `format` fills in: text with directives in
it, each starting with `~`. `"~A has ~D item~:P"` is one. It is a small
program, with loops (`~{`) and conditionals (`~[`), which Quokka parses into
a tree and then runs. ([46](part-6-self-hosting/46-format.md))

<a id="copying-collector"></a>
**Copying collector.** A [garbage collector](#garbage-collection) that reclaims
memory by copying everything still wanted into a fresh region and abandoning the
old one. The garbage costs nothing to reclaim — you simply stop using the space
it was in. ([6](part-1-substrate/06-cheneys-algorithm.md))

<a id="declaration"></a>
**Declaration.** Information about a program written at the start of a body, `(declare ...)`: that a variable is unused, holds a fixnum, or should be fast. An implementation may ignore every kind except `special`, which makes a variable dynamic for one binding or one region of code. ([50](part-6-self-hosting/50-first-gaps.md))

<a id="defstruct"></a>
**defstruct.** Defines a record type from one line: a constructor, a
predicate, an accessor per slot that `setf` can write, and a copier. A macro
that writes ten functions, and the clearest demonstration of what macros are
for. In tier 0 a structure was a vector with the type name in the first
slot; since step Z it is a heap object of its own whose first slot is its
[layout](#layout), and `defstruct` has all its options, `:include` among
them. ([21](part-2-reading-and-running/21-records.md), [61](part-6-self-hosting/61-structures.md))

<a id="defun"></a>
**defun.** Makes a function and puts it in a [symbol](#symbol)'s *function* cell. In Common Lisp it is a macro over `setf symbol-function`; Quokka made it a special operator before it had [macros](#macro), and moving it up is now possible. ([10](part-2-reading-and-running/10-functions-and-scope.md))

<a id="designator"></a>
**Designator.** An object that stands for another of a given kind, so a
function can accept either. A *string designator* is a string, a symbol (its
name) or a character (a string of one); `string` turns any into a string.
([45](part-6-self-hosting/45-sequences.md))

<a id="differential-testing"></a>
**Differential testing.** Running the same test on several implementations and requiring them to agree. You need not know the right answer — only notice a disagreement, which takes your own understanding out of the loop. Quokka runs each Lisp file interpreted and compiled, natively and under WebAssembly, and on SBCL. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md), [33](part-5-the-image/33-two-evaluators-one-answer.md))

<a id="discriminating-function"></a>
**`describe`.** The function that writes a description of an object for a person to read, by calling the generic function `describe-object`, whose methods a program can add for its own classes. The text is left to each implementation. ([75](part-7-objects/75-printing-objects.md))

**Discriminating function.** The function a generic function runs when called: it finds the classes of the required arguments, looks up the effective method for them in a cache, and computes it on a miss. ([74](part-7-objects/74-generic-functions.md))

<a id="displaced-array"></a>
**Displaced array.** An array with no elements of its own, which shares another array's, starting at an offset: writing through either is seen through both. ([55](part-6-self-hosting/55-arrays.md))

<a id="diverse-double-compiling"></a>
**Diverse double-compiling.** Compiling a compiler's source with two *different* compilers and comparing the results, as a defence against a compiler that hides something in itself (Wheeler, 2005). Quokka's C1 (compiled by the interpreted compiler) against C2 (by the compiled one) is a small instance. ([42](part-6-self-hosting/42-the-fixpoint.md))

<a id="dotted-pair"></a>
**Dotted pair.** A [cons cell](#cons-cell) whose [cdr](#cdr) is not a list, written `(a . b)`. A list is the common case of a cons cell, not the only one; the dot is how you write the general case. Also called an *improper list*. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="dynamic-extent"></a>
**Dynamic extent.** Found by searching the stack at run time rather than by looking at the source. `catch` and `throw` work this way, so a function can abandon a computation on behalf of a caller it has never heard of. Contrast [lexical](#lexical-scope). ([14](part-2-reading-and-running/14-leaving-early.md))

<a id="effective-method"></a>
**Effective method.** The applicable methods of one call, ordered and combined into a single function, under standard method combination the around, before, primary and after methods. ([74](part-7-objects/74-generic-functions.md))

<a id="effective-method-form"></a>
**Effective method form.** The Lisp form that the body of a long-form `define-method-combination` returns, saying with [call-method](#call-method) how one call's methods are to be run. Quokka compiles it, once per cache miss, into the [effective method](#effective-method). ([77](part-7-objects/77-method-combination.md))

<a id="environment"></a>
**Environment.** The chain of scopes a name is looked up in: innermost first, then outward, then the symbol's own value cell for globals. ([10](part-2-reading-and-running/10-functions-and-scope.md))

<a id="eql-equal-equalp"></a>
**eql, equal, equalp.** Common Lisp's equalities, from strictest to
loosest after `eq` (identity): `eql` also compares numbers and characters by
value; `equal` compares conses and strings by content; `equalp` also ignores
case and compares vectors element by element.
([45](part-6-self-hosting/45-sequences.md))

<a id="evaluator"></a>
**Evaluator.** The part of a runtime that actually executes a program.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

<a id="eval-when"></a>
**eval-when.** Says whether a form runs at compile time, load time, or when executed. Looks obscure and is load-bearing: it is how a macro defined in a file exists by the time a later form in that file is compiled, and therefore how self-hosting is possible at all. ([13](part-2-reading-and-running/13-the-special-operators.md))

<a id="expander"></a>
**Expander.** The function behind a [macro](#macro): given the macro form and the environment it is expanded in, it returns the form to use instead. Common Lisp defines it as a function of exactly those two arguments, which `macro-function` returns; the macro's lambda list takes the form apart inside it. Quokka's expanders took only the argument forms until step AC, which is why `&whole` was missing. ([64](part-6-self-hosting/64-what-a-macro-is-given.md))

<a id="explicit-control-evaluator"></a>
**Explicit-control evaluator.** An evaluator built as a machine with visible
state — registers and a stack it owns — rather than as a function that calls
itself. The only kind that can stop halfway and resume. The name and the
technique are from *Structure and Interpretation of Computer Programs* §5.4.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

<a id="external-symbol"></a>
**External symbol.** One of a [package](#package)'s public symbols, written with one colon from outside it (`geometry:area`). The rest are *internal*, reachable with two colons (`geometry::helper`); and a symbol visible because the package *uses* another is *inherited*. `find-symbol` reports which. ([36](part-6-self-hosting/36-whose-symbol-is-it.md))

<a id="fallback"></a>
**Fallback.** What the machine does when a primitive cannot answer: instead
of an error, it calls the matching Lisp function with the same arguments. A
fixnum primitive does this on overflow or given a bignum (`%generic-+` and so
on), and a hash table primitive given a key that `equal` or `equalp` must
compare (`%generic-gethash` and its siblings). The fast path stays in Rust and
the slow path is Lisp. ([47](part-6-self-hosting/47-bignums.md), [59](part-6-self-hosting/59-hash-tables-finished.md))

<a id="fast-path"></a>
**Fast path.** A short route through code for the common case, taken after a cheap test, with the general mechanism kept for the rest. The reader skips its syntax table while no character's type has been changed; `make-array` skips keyword parsing when given only a size. ([55](part-6-self-hosting/55-arrays.md), [58](part-6-self-hosting/58-the-readers-last-gaps.md))

<a id="feature-expression"></a>
**Feature expression.** What `#+` and `#-` test: a keyword, or `and`, `or` and `not` of them, true when the keywords are in `*features*`. It lets one file carry code for several Lisps: `#+sbcl (...)` is read only on SBCL. ([50](part-6-self-hosting/50-first-gaps.md))

<a id="fetch-decode-execute"></a>
**Fetch–decode–execute.** The loop a [VM](#vm) runs: read the next
[opcode](#opcode), read however many operand bytes it takes, do the thing,
repeat. Real processors are built around the same three steps. Quokka's loop
also counts down a step budget, so it can stop between instructions (rule 4).
([25](part-3-compiling/25-a-machine-made-of-instructions.md))

<a id="fill-pointer"></a>
**Fill pointer.** An index into a vector saying how many of its elements are in use: `length` answers it, `vector-push` stores at it and moves it on, and the elements after it stay in the vector unseen. ([55](part-6-self-hosting/55-arrays.md))

<a id="fixnum"></a>
**Fixnum.** An integer small enough to be stored directly in one machine word,
with no heap space used. Quokka's are 63 bits. The traditional Lisp word;
contrast [bignum](#bignum).
([1](part-1-substrate/01-one-word-for-everything.md))

<a id="fixpoint"></a>
**Fixpoint.** The self-hosting test: the compiled compiler recompiles its own source, and the output must be identical to what the interpreted compiler produced. A mismatch means compilation changed its own meaning, which is a bug no ordinary test could find. Quokka's holds since step F: every named compiled function is the same in C1, C2 and C3, and C2 and C3 are the same world word for word apart from gensym names. ([The bootstrap](the-bootstrap.md), [42](part-6-self-hosting/42-the-fixpoint.md))

<a id="flat-closure"></a>
**Flat closure.** A [closure](#closure) that copies the values it captured into
itself when it is made, so each one is at a known index rather than found by
walking a chain of scopes. Cheaper to use and more expensive to create — the
opposite trade from the linked environments Quokka's interpreter uses, and an
open question for the compiler. ([24](part-3-compiling/24-why-compile-at-all.md))

<a id="float-contagion"></a>
**Float contagion.** Common Lisp's rule for mixing a rational with a float in arithmetic: the rational is converted to the nearest float first, so `(+ 1 2.5)` is `3.5`. Comparison is the exception and is exact: `(= (1+ (expt 2 70)) (float (expt 2 70)))` is false. CLHS 12.1.4.1. ([48](part-6-self-hosting/48-floats.md))

<a id="float-floating-point-number"></a>
**Float (floating-point number).** A number held as a whole number of binary digits, the [significand](#significand), times a power of two, with a sign. Quokka's only float format is the IEEE 754 *double*: 64 bits, with 53 significant binary digits (about 16 decimal ones). Most decimals, 0.1 among them, have no exact binary form, which is why `(+ 0.1 0.2)` is `0.30000000000000004`. ([48](part-6-self-hosting/48-floats.md))

<a id="flow-analysis"></a>
**Flow analysis.** Working out, before a program runs, what it can possibly
do — which variables escape, which code is unreachable, whether a `block` is
ever returned from. A compiler's job; Quokka's interpreter does none of it,
which is why a `defun` that uses `return-from` loses its tail calls. ([16](part-2-reading-and-running/16-the-prelude.md))

<a id="forwarding-pointer"></a>
**Forwarding pointer.** A note left behind in the abandoned space during a
[collection](#collection), saying where an object was moved to. It is what stops
a shared object being copied twice and a circular structure looping forever.
([6](part-1-substrate/06-cheneys-algorithm.md))

<a id="frame"></a>
**Frame.** One pending step of the evaluator — "I was in the middle of something, here is how to continue once a value arrives". See [continuation](#continuation). ([9](part-2-reading-and-running/09-what-a-form-means.md))

<a id="funcall"></a>
**funcall.** Applies whatever a variable *holds*, as opposed to whatever its name denotes as a function. A [Lisp-2](#lisp-2) cannot call a closure stored in a variable without it. ([9](part-2-reading-and-running/09-what-a-form-means.md))

<a id="funcallable-instance"></a>
**Funcallable instance.** An [instance](#instance) that can also be called as a function, which is what a generic function is: it has slots (its name, its methods) and the machine calls the function it holds. In Quokka a heap type of its own, and the one part of CLOS in Rust. ([73](part-7-objects/73-classes.md))

<a id="generic-function"></a>
**Generic function.** A function whose code is chosen at each call by the classes (or identity) of its arguments, from the methods defined for it. ([74](part-7-objects/74-generic-functions.md))

<a id="function-designator"></a>
**Function designator.** An object that stands for a function wherever one is wanted: the function itself, or a symbol, meaning its global function. Why `(funcall 'car x)` and `:test 'eql` work. ([50](part-6-self-hosting/50-first-gaps.md))

<a id="garbage-collection"></a>
**Garbage collection.** Working out automatically, while a program runs, which
values it can no longer reach, and reclaiming their memory. The program never
says "I am finished with this"; it is deduced from what is still reachable.
([3](part-1-substrate/03-what-is-alive.md))

<a id="generalised-boolean"></a>
**Generalised boolean.** Any value used as a truth value: nil is false and
everything else true. `(string< "ab" "abc")` answers 2, the position where the
strings differ — true, and informative. ([45](part-6-self-hosting/45-sequences.md))

<a id="generational-collector"></a>
**Generational collector.** A collector that exploits the fact that most values
die young, by examining recently created ones often and long-lived ones rarely.
Faster than copying everything each time, and more complicated. Planned, not
built. ([6](part-1-substrate/06-cheneys-algorithm.md))

<a id="generation-of-a-compiler"></a>
**Generation (of a compiler).** One complete build of a compiler by the previous one. Quokka's cold build makes C1; C1 rebuilding itself makes C2; C2 makes C3. A *bootstrap stage*, in GCC's term. Comparing generations is how a self-compiling compiler is checked. ([41](part-6-self-hosting/41-the-image-rebuilds-itself.md))

<a id="gensym"></a>
**gensym.** A function returning a fresh [uninterned symbol](#uninterned-symbol),
named `G1`, `G2` and so on, or with a prefix of your choosing (`FOO12`).
Macros use it for temporary bindings, because a
symbol that is not in the [obarray](#obarray) cannot be named by anything the
caller writes — so it cannot be [captured](#capture-variable-capture). The
number comes from the special variable `*gensym-counter*`, which a program may
bind; until step Y it was a primitive with its counter in Rust. ([15](part-2-reading-and-running/15-code-that-writes-code.md), [60](part-6-self-hosting/60-labels-and-limits.md))

<a id="handler-case"></a>
**handler-case.** Runs a form, and if a condition is signalled inside it,
hands it to the first clause whose type matches. A [catch](#non-local-exit)
that matches on type instead of identity — the same unwinding, asking a
different question of each frame. ([20](part-2-reading-and-running/20-errors-you-can-catch.md))

<a id="hash-consing"></a>
**Hash consing.** Canonicalising structures so that two structurally equal
objects are also the *same* object, and can therefore be compared by pointer.
[Interning](#interning) is this applied to names. Unsound for anything mutable,
which is why Quokka's literals had to become read-only before sharing them
could even be considered. ([24](part-3-compiling/24-why-compile-at-all.md))

<a id="hash-table"></a>
**Hash table.** A collection that finds a value from a key in roughly one
step rather than by searching. Quokka's hashes a key's *contents* where it has
any (numbers, characters, symbols, strings in an `equal` table), and any other
key by its *address*. The collector moves objects, so an address-based hash is
right only until the next collection: the collector marks a table holding such
a key stale as it moves it, and the table rehashes on its next use. ([19](part-2-reading-and-running/19-things-with-an-index.md), [52](part-6-self-hosting/52-printing-for-another-lisp.md))

<a id="header"></a>
**Header.** The first word of a heap object, saying what kind it is and how many
slots follow. Cons cells have none.
([5](part-1-substrate/05-objects-in-the-heap.md))

<a id="heap"></a>
**Heap.** The region of memory where values too big for a single word live. In
Quokka it is an array of 64-bit words the VM owns.
([3](part-1-substrate/03-what-is-alive.md))

<a id="homoiconicity"></a>
**Homoiconicity.** A language whose programs are written in its own data
structures — Lisp code *is* a list. It sounds like trivia and is the reason
[macros](#macro) are a list-manipulating program rather than a text
preprocessor. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="host"></a>
**Host.** Whatever is running Quokka: the native terminal program, or the
browser tab. The VM may not touch files, clocks or screens itself (rule 2), so
it gets everything from the outside world through its host — by a **host
request**: a record left in the heap that the host reads, answers, and writes
back. ([31](part-5-the-image/31-asking-the-outside-world.md))

<a id="hygiene"></a>
**Hygiene.** The property, in Scheme's macro systems, that names introduced by
a macro are renamed automatically so [capture](#capture-variable-capture)
cannot happen. Quokka follows Common Lisp and does *not* have it: capture is an
error the language lets you make, and [gensym](#gensym) is the cure. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="hygienic-macro-expansion"></a>
**Hygienic macro expansion.** Scheme's answer to
[capture](#capture-variable-capture): the expander renames identifiers
introduced by a macro, so they cannot collide with the caller's, and names
*used* by a macro keep the meaning they had where it was written. Kohlbecker's
algorithm (1986). Quokka does not have it; see [gensym](#gensym). ([17](part-2-reading-and-running/17-why-macros-matter.md))

<a id="ieee-754"></a>
**IEEE 754.** The standard for floating-point arithmetic that nearly every processor follows. It fixes the formats (the 64-bit double among them), the result of `+ - * /` and the square root to the last bit, rounding to nearest with ties to even, and the special values infinity and [NaN](#nan). It does not fix `sin`, `exp` or `log` to the last bit. ([48](part-6-self-hosting/48-floats.md))

<a id="image"></a>
**Image.** The entire contents of a running system, written to a file and
loadable again — not a saved document but a saved *world*, resuming exactly
where it was. Booting means loading one rather than running a build.
([7](part-1-substrate/07-saving-a-world.md))

<a id="immediate"></a>
**Immediate.** A value stored entirely in its own word, referring to nothing:
a character, `nil`, `t`. Contrast a *reference*, which points into the heap.
([1](part-1-substrate/01-one-word-for-everything.md))

<a id="implementation-limit"></a>
**Implementation limit.** A constant with which a Common Lisp tells a program what it may rely on, such as `call-arguments-limit`: an exclusive upper bound, so one more than the most that is guaranteed to work. CLHS sets only minimums. Quokka's are its bytecode's real ceilings, 256 each. ([60](part-6-self-hosting/60-labels-and-limits.md))

<a id="implicit-progn"></a>
**Implicit progn.** A body that is a sequence of forms yielding the last — the bodies of `lambda`, `defun`, `let` and most else. Nobody writes the `progn`; it is what a body means. ([13](part-2-reading-and-running/13-the-special-operators.md))

<a id="instruction-selection"></a>
**Instruction selection.** Choosing *which* instructions express a form, when
several would work. `(+ 1 2)` can be a function-cell read and a call, or a
single `PRIM`; both are correct and one is faster. Where most of a compiler's
judgement lives, and most of what changes as one improves. ([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

<a id="interning"></a>
**Interning.** Keeping one object per name, so that two reads of `foo` produce the *same* [symbol](#symbol) rather than two equal ones. Without it, `eq` on symbols fails and every variable lookup becomes a string comparison. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="invariance"></a>
**Invariance.** A property of a type's lifetime parameter meaning it cannot be
silently substituted for a shorter or longer one. Quokka's [brand](#brand) must
be invariant or the guarantee evaporates while everything still compiles.
([4](part-1-substrate/04-rooting.md))

<a id="jump-table"></a>
**Jump table.** A row of jumps, all the same size, one per destination, so
that "go to the *i*th place" becomes "jump to the *i*th row" — arithmetic
instead of a search. The classic way to compile a computed goto, or a `switch`
on small integers. A `tagbody`'s `go` lands on one.
([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

<a id="keyword"></a>
**Keyword.** A symbol written with a leading colon — `:foo` — that evaluates
to itself. The colon is punctuation, not part of the name: `(symbol-name :foo)`
is `"FOO"`, and `:foo` is a *different symbol* from `foo` with the same name.
In Common Lisp that is because it lives in the `KEYWORD`
[package](#package); Quokka has no packages, so symbols carry a namespace
marker instead. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="keyword-argument"></a>
**Keyword argument.** An argument supplied by name — `(draw 'square :colour
'red)` — rather than by position, so a function can gain an option without
breaking its callers. Matched by [keyword](#keyword), leftmost occurrence
first, and an unrecognised one is an error unless `&allow-other-keys` says
otherwise. ([18](part-2-reading-and-running/18-lambda-lists.md))

<a id="kind"></a>
**Kind (of object).** In Quokka's [canonical types](#canonical-type), one of the disjoint groups every object falls into: integers, ratios, floats, characters, symbols, keywords, conses, arrays, structures, conditions, functions and a few more. Because no object is of two kinds, `and`, `or` and `not` on types can be worked out one kind at a time. ([72](part-7-objects/72-what-is-a-type.md))

<a id="instance"></a>
**Instance.** An object of a class `defclass` made, created by `make-instance`. In Quokka a heap object of its own type, holding a [wrapper](#wrapper) and a vector of slot values. ([73](part-7-objects/73-classes.md))

<a id="label"></a>
**Label.** A name standing for a position in a function's instructions, so
that the compiler can emit a jump before knowing how far it goes. The
assembler resolves them; the compiler uses `gensym` for them, so that a nested
`if` does not collide with itself. ([28](part-4-the-compiler/28-a-compiler-in-the-language-it-compiles.md))

<a id="lambda"></a>
**lambda.** Makes a function with no name. Its body is not evaluated when the `lambda` is — that happens when the function is called, which is why it must be a [special operator](#special-operator). ([10](part-2-reading-and-running/10-functions-and-scope.md))

<a id="lambda-list"></a>
**Lambda list.** A function's parameter list, which in Common Lisp says more
than a list of names: `&optional` parameters with default *forms*, `&key`
parameters supplied by name, supplied-p variables, and `&aux` locals. Quokka
rewrites the whole thing into a simple list plus a `let*` when the closure is
made, so nothing has to be evaluated while arguments are being bound. ([18](part-2-reading-and-running/18-lambda-lists.md))

<a id="latin-1"></a>
**Latin-1.** The [character encoding](#character-encoding) ISO 8859-1: one byte per character, covering Western European text. Its 256 codes are exactly Unicode's first 256, so a Latin-1 byte *is* the character's Unicode number. Quokka packs constant strings in it, eight to a word. ([37](part-6-self-hosting/37-eight-characters-to-a-word.md))

<a id="layout"></a>
**Layout.** The object that describes a structure's type: its name, its slots, the type it includes, how it prints. Every structure holds its layout in its first slot, so asking an object its type is one read, and a type that includes another is found by following the layouts' chain. ([61](part-6-self-hosting/61-structures.md))

<a id="lexical-addressing"></a>
**Lexical addressing.** Resolving a variable to "scope 2, slot 0" ahead of time, so lookup is an index rather than a search. It requires a compiler to assign the numbers, which is why Quokka's interpreter cannot use it yet. Phase 1 is the *yet*. ([10](part-2-reading-and-running/10-functions-and-scope.md), [24](part-3-compiling/24-why-compile-at-all.md))

<a id="lexical-environment"></a>
**Lexical environment.** What the code around a point in a program means:
which names are variables, local functions, local macros or symbol macros
there. The compiler keeps it as a list of scopes; a macro that asks for it
with `&environment` receives it as an object it can hand to `macroexpand-1`
and `get-setf-expansion`. Empty, it is the *null environment*.
([44](part-6-self-hosting/44-the-last-three.md))

<a id="lexical-scope"></a>
**Lexical scope.** Determined by where something is written, not by what calls it. `block` and `return-from` are lexical, so a `return-from` can only reach a block it can see in the source — which makes it safe and makes it useless across a function boundary. ([14](part-2-reading-and-running/14-leaving-early.md))

<a id="limb"></a>
**Limb.** One fixed-size piece of a bignum, like one digit in a very large
base. Quokka's limbs are 30 bits, least significant first, so that a limb
times a limb plus a carry still fits in a fixnum.
([47](part-6-self-hosting/47-bignums.md))

<a id="lisp-1"></a>
**Lisp-1.** A Lisp where a symbol has one cell shared by variables and functions. Scheme is one; no `funcall` is needed, but functions and variables compete for names. Contrast [Lisp-2](#lisp-2). ([9](part-2-reading-and-running/09-what-a-form-means.md))

<a id="lisp-2"></a>
**Lisp-2.** A Lisp where a [symbol](#symbol) has separate value and function cells, so `(list list)` is legal — the first is the function, the second the variable. Common Lisp is one. Scheme, a *Lisp-1*, has a single cell. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="literal"></a>
**Literal.** An object written directly in the source — `"hello"`, `#(1 2 3)`,
a quoted list — as opposed to one a program builds. Common Lisp leaves the
consequences of modifying one undefined (CLHS 3.7.1); Quokka makes source
strings and symbol names a separate heap type that refuses to be written, so
the undefined case cannot arise. ([24](part-3-compiling/24-why-compile-at-all.md))

<a id="literal-operand"></a>
**Literal operand.** An operand that *is* the value rather than directions for
finding it — the `7` in `FIX 7`. Machine-code writing usually calls this an
*immediate*, which this book avoids here because
[immediate](#immediate) already means a tagged value needing no heap object.
Two unrelated meanings, one word. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

<a id="macro"></a>
**Macro.** A function from code to code. Its [expander](#expander) is
called with the macro form, unevaluated, and the form it returns is evaluated
in place of the call — so it runs at *expansion time*, before anything else happens, and can
decide whether its arguments are evaluated at all. Macros are how a Lisp
program extends its own language without changing the [evaluator](#evaluator).
([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="macroexpand"></a>
**macroexpand.** Runs a [macro](#macro) and returns the form it produced,
without evaluating it — the first debugging tool a macro system needs, since
what a macro produces is otherwise invisible. `macroexpand-1` does one step;
`macroexpand` repeats until the head is no longer a macro. Neither descends
into subforms. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="macroexpansion"></a>
**Macroexpansion.** Running a [macro](#macro) to get the form it produces.
Quokka does it inside the evaluator, with a frame that remembers the caller's
[environment](#environment) so the expansion is evaluated where the call was
written. ([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="macrolet"></a>
**macrolet.** Binds a macro with a lexical scope, as `flet` binds a function.
Needed no new machinery here, because the evaluator already looked operators
up through the environment and already asked whether what it found was a
macro. Its expander runs in a null lexical environment — it cannot see the
surrounding variables, because at expansion time they do not have values yet.
([23](part-2-reading-and-running/23-more-than-one-answer.md))

<a id="marker"></a>
**Marker.** A zero-slot object, equal to nothing but itself, used as the identity of a `block` or `tagbody`. The name cannot serve, because two recursive entries to the same block must be two different targets. Markers have a header type of their own, so that nothing else can be mistaken for one. ([14](part-2-reading-and-running/14-leaving-early.md), [61](part-6-self-hosting/61-structures.md))

<a id="method"></a>
**Method.** One piece of a generic function's code, with a specializer for each required argument saying when it applies, and qualifiers such as `:before` saying how it combines with the others. ([74](part-7-objects/74-generic-functions.md))

<a id="method-combination"></a>
**Method combination.** The rule that turns a call's applicable methods into one effective method. *Standard method combination* runs the most specific primary method, which may call the next with `call-next-method`, inside any `:around` methods, between the `:before` and `:after` methods; the [operator method combinations](#operator-method-combination) and those a program defines with `define-method-combination` are the others. ([74](part-7-objects/74-generic-functions.md), [77](part-7-objects/77-method-combination.md))

<a id="multiple-dispatch"></a>
**Multiple dispatch.** Choosing a method by the classes of several arguments rather than the first alone. CLOS does; Smalltalk, Java and Python dispatch on the receiver only. ([74](part-7-objects/74-generic-functions.md))

<a id="metaclass"></a>
**Metaclass.** A class whose instances are classes. `standard-class` is the metaclass of classes `defclass` makes, and is an instance of itself. ([73](part-7-objects/73-classes.md))

<a id="metaobject-protocol"></a>
**Metaobject protocol (MOP).** The design in which an object system's own parts (classes, slot definitions, generic functions, methods) are themselves objects of classes, so a program can ask them about themselves and change how they behave. Described in *The Art of the Metaobject Protocol* (1991). ([73](part-7-objects/73-classes.md))

<a id="multiple-values"></a>
**Multiple values.** A form may return any number of results, and a caller
that wants one writes nothing special — the extras evaporate. Different from
returning a list, which every caller would have to take apart. `gethash`
returns the value *and* whether the key was there, which is the only way to
tell a stored nil from a missing key. ([23](part-2-reading-and-running/23-more-than-one-answer.md))

<a id="mutation"></a>
**Mutation.** In Quokka, one bounded stretch of work on the heap. No
[collection](#collection) can happen inside one, which is what makes holding a
value safe. ([4](part-1-substrate/04-rooting.md))

<a id="name-conflict"></a>
**Name conflict.** Two different symbols of one name both accessible in a [package](#package), so that the name read there would mean two things. `use-package`, `import`, `export` and `unintern` check for one before they change anything and signal a `package-error` if they would create it. ([54](part-6-self-hosting/54-packages-that-disagree.md))

<a id="nan"></a>
**NaN.** "Not a number": the IEEE 754 value for a result with no sensible answer, such as 0.0 / 0.0. *NaN boxing* is a representation trick, used by JavaScript engines, that stores every double raw in a 64-bit word and hides all other values inside the unused NaN bit patterns; Quokka [boxes](#boxing) its doubles instead. Quokka, like SBCL, signals an error rather than make a NaN or an infinity. ([48](part-6-self-hosting/48-floats.md))

<a id="nil"></a>
**nil.** The empty list, false, and the symbol named "NIL" — one value doing three jobs, which is both the convenience and the problem. A real [symbol](#symbol) in the [static area](#static-area), not an immediate; it took four chapters and four special cases to get that right. ([11](part-2-reading-and-running/11-the-trouble-with-nil.md))

<a id="non-local-exit"></a>
**Non-local exit.** Leaving a computation from the middle of it — `return-from`, `throw`. All of them are one mechanism differing only in how the target is found, and it is the same machinery a condition system needs. ([13](part-2-reading-and-running/13-the-special-operators.md))

<a id="not-applicable-a-test-result"></a>
**Not applicable (a test result).** A test that cannot say anything yet because it uses a feature that does not exist, as opposed to one that ran and got the wrong answer. Quokka's ansi-test runner reports these as *needs X*, so that a failure always means a wrong answer. pytest calls the same idea *skip*, JUnit an *assumption*. ([49](part-6-self-hosting/49-ansi-test.md))

<a id="obarray"></a>
**Obsolete instance.** An instance whose class has been redefined with other slots since it was made, so that its slot vector is still laid out by the old wrapper. It is updated, through `update-instance-for-redefined-class`, the next time one of its slots is touched. ([76](part-7-objects/76-changing-classes.md))

**Obarray.** The table of every [interned](#interning) symbol. The name is from "object array" and is a 1960s MIT detail that stuck. Quokka's is a list for now, and a [root](#root) the collector must not forget. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="opcode"></a>
**Opcode.** Short for *operation code*: the number at the start of an
instruction saying which operation it is. `CONST`, `LREF` and `JMP` are names
this book gives to opcodes so they can be read; the machine sees bytes.
([25](part-3-compiling/25-a-machine-made-of-instructions.md))

<a id="operand"></a>
**Operand.** The part of an instruction after the [opcode](#opcode), saying
what to operate on. Quokka's instructions take nought to two, and how many —
and how wide — is fixed by the opcode, which is what spares the assembler a
[branch relaxation](#branch-relaxation) pass. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

<a id="operand-stack"></a>
**Operand stack.** The pile of values that instructions push to and pop from.
`FIX 2`, `FIX 3`, `PRIM *` leaves `6` where the two numbers were. Expressions
nest and a stack nests, which is why compiling to a stack machine is simpler
than compiling to anything else. In Quokka it is a vector in the heap, not a
Rust local, because of rule 4. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

<a id="operator-method-combination"></a>
**Operator method combination.** A [method combination](#method-combination) that applies one operator to the values of every applicable primary method, such as `+`, `list`, `and` or `progn`; each primary method is qualified with the combination's name. Common Lisp defines nine, and the short form of `define-method-combination` defines more. ([77](part-7-objects/77-method-combination.md))

<a id="oracle"></a>
**Oracle.** A reference implementation trusted more than your own, used to check answers against. Available only when reimplementing something that already exists, which is why compilers and databases get a testing technique original software does not. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md))

<a id="package"></a>
**Package.** Common Lisp's namespace for symbols: a symbol table plus rules
about which names are visible where. Quokka does not have them — tier 1, phase
2 — but symbols do carry a *home namespace*, with two possible values, because
[keywords](#keyword) cannot be represented without one. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="package-prefix"></a>
**Package prefix.** The `pkg:` or `pkg::` before a symbol's name, saying which package it comes from: one colon for an external symbol, two for an internal one. The printer writes one only when the reader would not find the symbol without it. ([36](part-6-self-hosting/36-whose-symbol-is-it.md), [39](part-6-self-hosting/39-printing-in-lisp.md))

<a id="parser"></a>
**Parser.** What the [reader](#reader) is called in most languages, where it is far larger — it must know operator precedence, statements versus expressions, declarations. A Lisp reader needs none of that, because the text is already the data structure. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="percent-name"></a>
**Percent name.** A name beginning with `%` — `%set-aref`, `%macro-function`
— marking something as Quokka's own rather than Common Lisp's, and not to be
written in a program. It says nothing about *what* the thing is: most are
ordinary functions, some written in Rust and some in Lisp, and a few are forms
the compiler knows. The convention stood in for a [package](#package) until
phase 2 brought one, and when every system symbol was adopted into `QUOKKA`
the prefix was kept, because it still says *implementation, not API* at a
glance. ([22](part-2-reading-and-running/22-the-percent-names.md), [36](part-6-self-hosting/36-whose-symbol-is-it.md))

<a id="place"></a>
**Place.** Anything `setf` can write, written as the form that reads it: a
variable, `(aref v i)`, `(gethash k h)`, a structure slot, `(getf plist :k)`.
The standard also calls it a *generalized reference*. ([43](part-6-self-hosting/43-places.md))

<a id="placeholder-and-patch"></a>
**Placeholder and patch.** How a reader returns an object that refers to itself before it is finished, as in `#1=(a . #1#)`. `#1=` records a fresh placeholder; a `#1#` read while the object is incomplete returns it; once the object is complete, a walk replaces every placeholder with the object. Also called *backpatching*. ([60](part-6-self-hosting/60-labels-and-limits.md))

<a id="prelude"></a>
**Prelude.** The Lisp that a Lisp system loads before any user code —
`lisp/prelude.lisp` here. Compiled into the binary with `include_str!`, since
the core VM has no filesystem, and interpreted at boot. `defun`, `when`,
`cond`, `and` and `or` live there rather than in Rust. ([16](part-2-reading-and-running/16-the-prelude.md))

<a id="present-and-accessible"></a>
**Present and accessible.** A symbol is *present* in a [package](#package) when the package's own tables hold it, and *accessible* when it is present or inherited from a package the package uses. ([54](part-6-self-hosting/54-packages-that-disagree.md))

<a id="pretty-printer"></a>
**Pretty printer.** The part of a Common Lisp printer that lays a long form out over several lines with indentation, controlled by `*print-pretty*`, `*print-right-margin*` and their kin. Tier 2 in Quokka: the variables exist and are not yet consulted. ([52](part-6-self-hosting/52-printing-for-another-lisp.md))

<a id="prevalence"></a>
**Prevalence.** Keeping a system's data as live objects in memory rather than
in a separate database, and making it survive a restart with a log of every
change plus periodic snapshots. Named after the Prevayler project; Common Lisp
has `bknr.datastore`. Quokka's image save is the snapshot half, and it has no
log — [#24](https://github.com/quokka-lisp/quokka-lisp/issues/24) is the open question. ([Further reading](further-reading.md))

<a id="primitive"></a>
**Primitive.** An operation the [VM](#vm) provides directly in Rust rather than
in Lisp — arithmetic, allocation, input and output. The *primitive table* is the
only door between the two languages.

<a id="printer"></a>
**Printer.** Turns heap objects back into text. The *P* in [REPL](#repl), and how a [reader](#reader) is tested: text that survives read-then-print was almost certainly read correctly. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="printing-readably"></a>
**Printing readably.** Printing with `*print-readably*` true: the printer must write each object so that `read` gives back an equal one, or signal a `print-not-readable` error. A package, a stream or a function cannot be printed readably. `with-standard-io-syntax` turns it on, along with every other printer and reader variable's standard value. ([52](part-6-self-hosting/52-printing-for-another-lisp.md))

<a id="print-read-consistency"></a>
**`print-object`.** The generic function a program specialises to say how its objects print. Quokka's printer calls it for instances, structures and conditions, and for any other object only once a program has defined a method on a built-in class; its method on `t` is the printer's own code. ([75](part-7-objects/75-printing-objects.md))

**Print-read consistency.** The printer's contract in Common Lisp: what `prin1` writes, `read` reads back as the same object (or an equal one, for a list). For symbols it means a package prefix when the current package would not find the symbol alone, and `|...|` around a name the reader would change. ([39](part-6-self-hosting/39-printing-in-lisp.md))

<a id="proclamation"></a>
**Proclamation.** A declaration that holds everywhere rather than in one scope. `defvar` *proclaims* a variable special, and it must do so before any code that binds the variable is compiled, because the compiler decides at compile time whether a binding is lexical or dynamic. ([40](part-6-self-hosting/40-the-loop-closes.md))

<a id="progn"></a>
**progn.** Evaluates several forms in order and yields the last. It lets a sequence stand where one form is expected; the bodies of `lambda` and `defun` are implicit `progn`s. ([10](part-2-reading-and-running/10-functions-and-scope.md))

<a id="program-counter-pc"></a>
**Program counter (`Pc`).** The offset of the next instruction. Keeping it in
a heap register rather than a Rust local is what lets the VM stop between
instructions and resume later — and what lets an image be saved mid-run.
([25](part-3-compiling/25-a-machine-made-of-instructions.md))

<a id="promise"></a>
**Promise.** In a browser, an object standing for an answer that has not
arrived yet. JavaScript that asks for something slow gets a Promise back at
once and finishes; when the answer comes, the browser runs a callback with it.
The reason a browser host cannot answer a [host request](#host) on the spot.
([31](part-5-the-image/31-asking-the-outside-world.md))

<a id="property-list-plist"></a>
**Property list (plist).** A list of alternating keys and values,
`(:colour red :size 3)`, read with `getf`. Adding a key puts it on the front,
which makes a new list — why `(setf (getf …))` must write the place that holds
the list as well. ([43](part-6-self-hosting/43-places.md)) ([51](part-6-self-hosting/51-symbol-cells.md))

<a id="quicklisp"></a>
**Quicklisp.** Common Lisp's library manager: an index of a few thousand
libraries and a downloader, so `(ql:quickload "mito")` fetches a library and
its dependencies and loads them into the Lisp you are running. Each release is
a *curated snapshot* — versions known to build together — which is the choice
that made it trusted. It builds on ASDF (Another System Definition Facility),
the build system that compiles and loads a set of files in dependency order.
Quokka has no equivalent and will eventually need one. ([Further reading](further-reading.md))

<a id="quote"></a>
**Quote.** `'x` reads as the list `(quote x)` — ordinary structure, not syntax. The evaluator later treats `quote` as "give me this rather than running it", which is how a program says *data* rather than *code*. ([8](part-2-reading-and-running/08-text-becomes-structure.md))

<a id="ratchet-in-testing"></a>
**Ratchet (in testing).** A baseline that may only improve: the list of tests that pass is committed, and a change that makes one of them fail is refused, while a change that makes more pass updates the list. Quokka's ansi-test run is one. ([49](part-6-self-hosting/49-ansi-test.md))

<a id="raw-slots"></a>
**Ratio.** An exact quotient of two integers, such as `1/3`, kept as a numerator and a denominator in lowest terms with a positive denominator, so that one number has one representation. With the integers they are the *rationals*. ([70](part-6-self-hosting/70-ratios.md))

**Rational canonicalisation.** The rule that a rational is always in its one standard form: a ratio in lowest terms with a denominator above 1, and an integer whenever the denominator would be 1, so `(/ 4 2)` is the integer 2. (CLHS 12.1.3.2; [70](part-6-self-hosting/70-ratios.md))

**Raw slots.** An object whose words are *not* references — a string's
character codes, a bignum's digits — and which the collector must copy rather
than follow. Quokka draws the line at type number 64: below it slots are
traced, at or above they are raw. A type on the wrong side corrupts the heap
silently, so a test guards it. ([19](part-2-reading-and-running/19-things-with-an-index.md))

<a id="reachable"></a>
**Reachable.** Able to be found by following references from a [root](#root).
This is the definition of "still wanted" that garbage collection uses.
([3](part-1-substrate/03-what-is-alive.md))

<a id="reader"></a>
**Reader.** The part of a runtime that turns text into structure. In Lisp it is
unusually small, because the structure it produces is ordinary lists.
([Preface](what-has-to-be-built.md))

<a id="reader-macro"></a>
**Reader macro.** A function the [readtable](#readtable) holds for a character, called by the reader when it meets that character, which expands syntax into ordinary code before the evaluator sees it: `'x` becomes `(quote x)`, and [backquote](#backquote) is the one that earns its keep. `(`, `'`, `"` and `;` are all reader macros in Common Lisp; a comment is one that returns no value at all. A *dispatching* macro character, such as `#`, reads one more character and looks that one up in a second table. ([15](part-2-reading-and-running/15-code-that-writes-code.md), [38](part-6-self-hosting/38-reading-in-lisp.md))

<a id="readtable"></a>
**Readtable.** The table that tells Common Lisp's [reader](#reader) what each character means: whitespace, part of a [token](#token), or a [reader macro](#reader-macro). `*readtable*` is the current one, and a program changes syntax by changing a copy. ([38](part-6-self-hosting/38-reading-in-lisp.md))

<a id="readtable-case"></a>
**Readtable case.** A setting of the [readtable](#readtable) saying what the reader does to the unescaped letters of a [token](#token): `:upcase` (the standard) raises them, `:downcase` lowers them, `:preserve` leaves them, and `:invert` flips a token whose letters are all one case. The printer follows it, so that what it writes reads back. ([58](part-6-self-hosting/58-the-readers-last-gaps.md))

<a id="re-entrant"></a>
**Re-entrant.** Safe to call again while a call is still in progress. Quokka's `compile-form` keeps its work in global variables, so it became re-entrant by saving and restoring them around each call. That is not the same as safe for two processes at once, whose calls interleave rather than nest. ([40](part-6-self-hosting/40-the-loop-closes.md))

<a id="register-machine"></a>
**Register machine.** An instruction set whose instructions name where their
operands are — `ADD r3, r1, r2` — like real hardware. Fewer, larger
instructions than a [stack machine](#stack-machine), and a harder compiler,
because something must decide which value lives in which register. ([24](part-3-compiling/24-why-compile-at-all.md))

<a id="repl"></a>
**REPL.** Read, Eval, Print, Loop — a prompt that reads what you type, runs it,
prints the answer and comes back for more. Invented for Lisp in 1964; now in
every language. ([Preface](what-has-to-be-built.md))

<a id="restart"></a>
**Restart.** A way of carrying on from an error that the code signalling it offers, chosen by a handler or by the person at the keyboard: `ccase`'s `store-value` stores a new value and tries again. Part of Common Lisp's full condition system, which is tier 2 in Quokka; until then `ccase` signals its error like `ecase`. ([56](part-6-self-hosting/56-small-functions.md))

<a id="root"></a>
**Root.** A location holding a value that is *not* inside the heap — a machine
register, a global table, a local variable in the implementation language.
Garbage collection starts its search from these. A root is a *place*; a
[value](#value) is a *thing*.
([3](part-1-substrate/03-what-is-alive.md))

<a id="rooting"></a>
**Rooting.** Telling the collector about a location it cannot find by itself.
([4](part-1-substrate/04-rooting.md))

<a id="row-major-order"></a>
**Row-major order.** Laying a grid out as a line, one row after another: element (i, j) of an array with three columns is element 3i + j of the line. How an array of several dimensions keeps its elements, and what `row-major-aref` indexes. ([55](part-6-self-hosting/55-arrays.md))

<a id="sbcl"></a>
**SBCL.** Steel Bank Common Lisp, a mature Common Lisp implementation, and Quokka's [oracle](#oracle). A reference to check answers against, never something Quokka needs in order to build. ([12](part-2-reading-and-running/12-agreeing-with-something-else.md))

<a id="self-hosting"></a>
**Self-hosting.** When a compiler can compile its own source, so the system needs nothing outside it to rebuild. The point at which Quokka's Rust stops growing and every further feature is written in Lisp. ([The bootstrap](the-bootstrap.md))

<a id="semispace"></a>
**Semispace.** One of the two halves a [copying collector](#copying-collector)
works between. At any moment one holds everything and the other is empty.
([6](part-1-substrate/06-cheneys-algorithm.md))

<a id="sequence"></a>
**Sequence.** Common Lisp's name for anything `length` and `elt` work on:
lists, vectors and strings. The functions over them dispatch on which kind
they were given. ([19](part-2-reading-and-running/19-things-with-an-index.md))

<a id="setf"></a>
**setf.** The general way to write a place in Common Lisp: `(setf (aref v i)
x)`, `(setf (gethash k h) v)`, `(setf x 1)`. A macro that turns a *reader* —
`aref`, `gethash` — into the corresponding writer, so a program names what it
wants to change in the same words it would use to read it. Tier 0's knows a
fixed list of places; the tier-1 library's can be taught new ones, and expands
a place tier 0 knows exactly as tier 0 did. ([19](part-2-reading-and-running/19-things-with-an-index.md), [43](part-6-self-hosting/43-places.md))

<a id="setf-expander"></a>
**setf expander.** How `setf` is taught a place it has never seen. Tier 0's
is a table from a reader's name to a writer's name, which is what `defstruct`
needs and what `defsetf`'s short form still is. `define-setf-expander` is the
general kind: a function from the place's subforms to its five-value
[setf expansion](#setf-expansion), so that a place inside a place, such as
`(getf plist :k)`, can be written. ([21](part-2-reading-and-running/21-records.md), [43](part-6-self-hosting/43-places.md))

<a id="setf-expansion"></a>
**setf expansion.** The five values `get-setf-expansion` returns for a
[place](#place): temporaries, the subforms they hold, store variables, a form
that writes and a form that reads. A macro that reads *and* writes a place —
`incf`, `push` — binds the temporaries once and uses both forms, so each
subform is evaluated exactly once, left to right. ([43](part-6-self-hosting/43-places.md))

<a id="s-expression"></a>
**S-expression.** "Symbolic expression": Lisp's single notation, used for both
code and data. `(* 6 7)` is a list of three things, not special syntax.
([Preface](what-has-to-be-built.md))

<a id="shadowing-symbol"></a>
**Shadowing symbol.** A symbol a [package](#package) has declared will win any [name conflict](#name-conflict) over its name, made with `shadow` or `shadowing-import`. It hides the inherited symbols of the same name. ([54](part-6-self-hosting/54-packages-that-disagree.md))

<a id="shallow-binding"></a>
**Shallow binding.** One way to implement [special variables](#special-variable): the current value lives in the symbol, binding saves the old value and stores the new one, and ending the binding puts the old one back. Reading is one load; switching threads has to swap bindings. Its alternative, *deep binding*, keeps bindings in a per-thread list that every read searches. Quokka uses shallow binding. ([34](part-6-self-hosting/34-standing-on-its-own.md))

<a id="shared-structure"></a>
**Shared structure.** Parts of a data structure reached by more than one path, such as one list that is two elements of another, or a list whose last cons points back at its first (a *circular* list). With `*print-circle*` true the printer labels such a part `#1=` where it first appears and writes `#1#` for it afterwards, and the reader rebuilds the sharing from the labels. ([52](part-6-self-hosting/52-printing-for-another-lisp.md))

<a id="shortest-round-trip-printing"></a>
**Shortest round-trip printing.** Printing a float as the fewest decimal digits that read back as exactly the same float: `0.1`, though the double nearest 0.1 is 0.1000000000000000055511151231257827.... Solved by Steele and White (1990); made fast by Grisu (2010) and Ryu (2018). The reverse problem, reading a decimal into the nearest float, is Clinger's (1990). ([48](part-6-self-hosting/48-floats.md))

<a id="significand"></a>
**Significand.** The digits of a [float](#float-floating-point-number), as a whole number, before the power of two scales them; also called the *mantissa*. `integer-decode-float` returns it with the exponent and the sign. ([48](part-6-self-hosting/48-floats.md))

<a id="simple-array"></a>
**Simple array.** An array with one dimension and nothing else to it: no fill pointer, not adjustable, not displaced. In Quokka a general vector, a string or a [bit vector](#bit-vector), each the object it always was; every other array is an [array header](#array-header). ([55](part-6-self-hosting/55-arrays.md))

<a id="simple-vector"></a>
**Simple vector.** A fixed-length run of slots holding any values, indexed
by [aref](#aref). The only kind of array Quokka has: no fill pointers, no
adjustable arrays, no dimensions beyond one. ([19](part-2-reading-and-running/19-things-with-an-index.md))

<a id="slice"></a>
**Slice.** A bounded stretch of execution after which the VM returns control to
whoever called it, so a browser can repaint and respond. The caller says how
many steps. ([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

<a id="slime"></a>
**SLIME.** Superior Lisp Interaction Mode for Emacs: the usual way to develop
Common Lisp, where the editor is one program and Lisp is another, and the two
talk over a socket using a protocol called Swank. Its feature list — evaluate
the form at point, jump to a definition, show arguments while you type — is a
good specification for Quokka's own environment. Its architecture is the one
Quokka does not have, because the editor will live inside the image. ([Further reading](further-reading.md))

<a id="special-operator"></a>
**Special operator.** A form that decides for itself whether, when, or in what environment its arguments are evaluated — or whose arguments are not expressions at all. `if` must be one, or both branches would run before it could choose. Common Lisp has exactly 25, they are a *basis* rather than a feature list, and everything else is macros over them. ([9](part-2-reading-and-running/09-what-a-form-means.md), [13](part-2-reading-and-running/13-the-special-operators.md))

<a id="special-variable"></a>
**Special variable.** A variable whose binding is visible to everything *called* while the binding is in force, not only to the code written inside it — what other languages call dynamic scope. Common Lisp names them with asterisks, like `*package*`, and makes them with `defvar` and `defparameter`. ([34](part-6-self-hosting/34-standing-on-its-own.md))

<a id="stack-machine"></a>
**Stack machine.** An instruction set whose instructions take no operands and
work on an implicit stack: `CONST 3`, `CONST 4`, `ADD`. Tiny instructions and a
simple compiler, at the cost of more of them. What Quokka's bytecode will be,
chosen for size rather than speed. ([24](part-3-compiling/24-why-compile-at-all.md))

<a id="stack-pointer"></a>
**Stack pointer.** How deep the [operand stack](#operand-stack) currently is.
Popped slots are cleared, because every slot of a traced object is traced and
a dead one would keep what it last held alive. ([25](part-3-compiling/25-a-machine-made-of-instructions.md))

<a id="static-area"></a>
**Static area.** The first words of the heap, which the collector copies verbatim and never relocates. A reference into it is therefore a compile-time constant, which is how `nil` can be an ordinary symbol *and* `Value::NIL` a Rust constant. `t` lives there too; the keyword package will. ([11](part-2-reading-and-running/11-the-trouble-with-nil.md))

<a id="stream"></a>
**Stream.** An object that hands out characters one at a time (an *input* stream) or accepts them (an *output* stream), remembering where it is. Quokka's tier 1 has string input streams; output streams come with the printer. ([38](part-6-self-hosting/38-reading-in-lisp.md))

<a id="string"></a>
**String.** A heap object whose slots hold character codes rather than
[values](#value), which is why it sits on the raw side of the collector's line
(see [raw slots](#raw-slots)). Common Lisp counts it as a vector, so `length`
and [aref](#aref) work on one. A writable string holds one character per word;
a constant one whose characters are all [Latin-1](#latin-1) holds eight. ([8](part-2-reading-and-running/08-text-becomes-structure.md), [19](part-2-reading-and-running/19-things-with-an-index.md), [37](part-6-self-hosting/37-eight-characters-to-a-word.md))

<a id="subnormal"></a>
**Subnormal.** A float too small for the normal range, which gives up leading bits of its [significand](#significand) to reach closer to zero. The smallest double, about 4.9 × 10^-324, has one significant bit. IEEE 754 calls producing one *underflow*. ([48](part-6-self-hosting/48-floats.md))

<a id="supplied-p-parameter"></a>
**Supplied-p parameter.** The third element of an `&optional` or `&key` entry,
bound to true when an argument was actually given. It exists because `nil` is
a perfectly good argument, so "absent" and "nil" have to be different things.
([18](part-2-reading-and-running/18-lambda-lists.md))

<a id="symbol"></a>
**Symbol.** A named thing — `car`, `+`, `my-variable`. Lisp symbols are objects
in their own right, not just text, and carry a value and a function.

<a id="symbol-macro"></a>
**Symbol macro.** A symbol that stands for a form within a region of code,
made by `symbol-macrolet`: reading it reads the form, `setq` of it is `setf`
of the form, and a variable binding of the same name inside the region
shadows it. `define-symbol-macro` makes a global one, which any lexical binding of the name hides. ([44](part-6-self-hosting/44-the-last-three.md), [56](part-6-self-hosting/56-small-functions.md))

<a id="syntax-type"></a>
**Syntax type.** What a character is to the reader: whitespace, a *constituent* (an ordinary character of a [token](#token)), a single escape `\`, a multiple escape `|`, or a [reader macro](#reader-macro) character. `set-syntax-from-char` gives one character another's type. ([58](part-6-self-hosting/58-the-readers-last-gaps.md))

<a id="system-call"></a>
**System call.** How a program asks an operating system's kernel to do
something it may not do itself, such as read a disk. The program hands over a
request, is put to sleep if the answer is not ready, and wakes with the result.
Quokka's [host requests](#host) are the same shape: the computation is the
program and the host is the kernel.
([31](part-5-the-image/31-asking-the-outside-world.md))

<a id="tag"></a>
**Tag.** A few bits inside a word saying what kind of value it holds. Quokka
uses the low three bits, which are free because heap objects are aligned to
8-byte boundaries. A *tagged word* is a value plus its tag.
([1](part-1-substrate/01-one-word-for-everything.md))

<a id="tagbody"></a>
**tagbody.** The primitive iteration construct: a body with labels that `go` jumps to. Almost never written directly; `do`, `dolist` and `loop` are macros that expand into it. ([13](part-2-reading-and-running/13-the-special-operators.md))

<a id="tail-call"></a>
**Tail call.** A call whose result is immediately the result of the enclosing function, so the caller's [frame](#frame) is no longer needed. Eliminating it is what makes recursive loops run in constant space — which matters in Lisp, where recursion *is* iteration. ([10](part-2-reading-and-running/10-functions-and-scope.md))

<a id="tier"></a>
**Tier.** One of the three stages in which Quokka's Common Lisp subset grows, each the minimum needed to unlock a phase. Tier 0 is enough to run the compiler; tier 1 is the self-hosting core — packages, the reader and printer in Lisp, bignums, floats; tier 2 is what the environment needs, such as CLOS and streams. ([34](part-6-self-hosting/34-standing-on-its-own.md))

<a id="token"></a>
**Token.** A run of characters the reader collects up to whitespace or a terminating [reader macro](#reader-macro) character, then turns into a number or a symbol. `\` and `|...|` escape characters inside one. ([38](part-6-self-hosting/38-reading-in-lisp.md))

<a id="torture-mode"></a>
**Torture mode.** A build setting that makes rare conditions happen constantly —
here, collecting garbage at every opportunity and checking the heap afterwards —
so latent bugs fail on the first test run rather than the thousandth.
([6](part-1-substrate/06-cheneys-algorithm.md))

<a id="tracing"></a>
**Tracing.** Following references from the [roots](#root) to find everything
[reachable](#reachable). Also the thing you must *not* do to a string's
characters, which are not references.
([3](part-1-substrate/03-what-is-alive.md))

<a id="trampoline"></a>
**Trampoline.** A small piece of code whose only job is to jump into
something else. Quokka's host uses a four-instruction one to call a compiled
Lisp function on the VM, so that a compiled image never needs the interpreter.
([32](part-5-the-image/32-a-world-made-of-bytecode.md))

<a id="transcendental-function"></a>
**Transcendental function.** A function such as `sin`, `exp` or `log` that no finite sequence of additions and multiplications computes exactly. Maths libraries compute them to within a last-bit error, and different libraries differ in that bit, which is why Quokka uses one library, `libm`, on every target. ([48](part-6-self-hosting/48-floats.md))

<a id="transparent-to-values"></a>
**Transparent to values.** Said of a form that returns however many values
its last form returned — `block`, `let`, `progn`, `if`, `unwind-protect` and
most others. Mostly free here, because a form in [tail position](#tail-call)
never re-delivers; the three that were not free each read the value register
and delivered it again, which was *almost* right. ([23](part-2-reading-and-running/23-more-than-one-answer.md))

<a id="trap-floating-point"></a>
**Trap (floating-point).** Stopping with an error when a float operation overflows, divides by zero or has no answer, instead of quietly producing an infinity or a [NaN](#nan). SBCL traps those three, and so does Quokka, by checking each result in Rust. ([48](part-6-self-hosting/48-floats.md))

<a id="two-s-complement"></a>
**Two's complement.** How machines store signed integers: the top bit counts as a large negative value. A fixed width therefore holds one more negative number than positive, so negating the most negative one overflows. Common Lisp's bit operations behave as if every integer were stored this way, infinitely wide; Quokka stores bignums as sign and magnitude and converts for `logand` and its relatives. ([39](part-6-self-hosting/39-printing-in-lisp.md), [47](part-6-self-hosting/47-bignums.md))

<a id="type-specifier"></a>
**Type specifier.** A Lisp object naming a set of objects: a symbol such as `integer`, a list such as `(integer 0 9)` or `(or string symbol)`, or a name a program gave one with `deftype`. `typep` asks whether one object is in the set, `subtypep` whether one set lies inside another. ([72](part-7-objects/72-what-is-a-type.md))

<a id="unbound"></a>
**Unbound.** A variable or function cell with no value at all, which is not `nil`, since `nil` is a value. Reading one is an error. Inside, the cell holds a marker only Rust can see, so that the difference between "this variable does not exist" and "this variable holds the empty list" is one the system can tell; `boundp` asks, and `makunbound` puts the marker back. ([8](part-2-reading-and-running/08-text-becomes-structure.md), [44](part-6-self-hosting/44-the-last-three.md))

<a id="uninterned-symbol"></a>
**Uninterned symbol.** A symbol that is not in the [obarray](#obarray), so
reading its printed name produces a *different* symbol. Nothing a program can
type refers to it; only whoever holds the object does. See [gensym](#gensym).
([15](part-2-reading-and-running/15-code-that-writes-code.md))

<a id="unit-in-the-last-place-ulp"></a>
**Unit in the last place (ulp).** The gap between a floating-point number and the next one, and so the smallest error a result can have. Most C libraries promise their logarithms and hyperbolic functions to within one ulp, not exactly, which is why two of them can disagree in the last digit. ([56](part-6-self-hosting/56-small-functions.md))

<a id="unwinding"></a>
**Unwinding.** Discarding frames until one matches, running any cleanups passed on the way. The third half-step of the machine, and what every non-local exit is made of. ([14](part-2-reading-and-running/14-leaving-early.md))

<a id="upgrading"></a>
**Upgrading (an element type).** Replacing the element type a program asks an array for with the nearest one the implementation can store: CLHS lets an implementation keep only a few specialised kinds of array. Quokka's are `t`, `character`, `bit` and `nil`, so `(make-array 3 :element-type 'fixnum)` makes an array of `t`, and `base-char` upgrades to `character`. `upgraded-array-element-type` says what a type upgrades to. ([55](part-6-self-hosting/55-arrays.md), [72](part-7-objects/72-what-is-a-type.md))

<a id="value"></a>
**Value.** One piece of Lisp data as the machine holds it: in Quokka, exactly one
64-bit word, either the data itself or a reference to somewhere in the heap.
([1](part-1-substrate/01-one-word-for-everything.md))

<a id="vm"></a>
**VM.** Virtual machine: a program that behaves like a computer for the language
running on it, with its own memory and instructions. Quokka's is written in Rust
and does what a Lisp machine's hardware once did.

<a id="webassembly-wasm"></a>
**WebAssembly (WASM).** A compact instruction format that runs inside a web
browser at close to native speed. Quokka compiles to it, which is why the VM
must be able to stop and hand control back.
([2](part-1-substrate/02-why-the-interpreter-cannot-call-itself.md))

<a id="whole-form-whole"></a>
**Whole form (`&whole`).** In a macro lambda list, `&whole w` binds `w` to the entire macro form, head included: in `(defmacro show (&whole w arg) ...)`, the call `(show x)` binds `w` to `(show x)`. Inside a nested pattern, or in `destructuring-bind`, it binds the list that pattern matches. ([64](part-6-self-hosting/64-what-a-macro-is-given.md))

<a id="wrapper"></a>
**Wrapper.** The object an [instance](#instance) points to that belongs to its class and lists the names of its slots in the order its slot vector keeps them. Replacing an instance's wrapper and slot vector lets a redefined class give its old instances new slots while they stay the same objects; this is PCL's representation. ([73](part-7-objects/73-classes.md))

<a id="write-barrier"></a>
**Write barrier.** Machinery that notices when an old object is made to point at
a new one. A [generational](#generational-collector) collector needs it; the
simple copying collector here does not.
([6](part-1-substrate/06-cheneys-algorithm.md))
