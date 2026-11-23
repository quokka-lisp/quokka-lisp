# Numbers without a ceiling

> *Where this fits.* Chapter 1 made an integer a **fixnum**: a number held
> inside the tagged word itself, 63 bits of it, with nothing in the heap.
> Arithmetic that left that range was an error, "integer overflow, and
> bignums do not exist yet", and issue #16 recorded long ago how bignums
> would eventually be split between Rust and Lisp. This chapter builds them.
> It is the first tier-1 step since the fixpoint that needed real Rust, and
> the most arithmetic the book has done: school addition and multiplication,
> long division as Knuth wrote it down, and the bit operations on numbers
> that are stored one way and behave as if they were stored another. With
> it, an integer in Quokka is as large as memory allows, and the reader, the
> printer and `format` handle such numbers without having been told about
> them.

*Status: done, issue #16. Step L of phase 2.*

## What is a bignum?

`(* 2432902008176640000 21)` is 20 factorial times 21. The first number fits
in a fixnum, but the product does not, and until this step Quokka refused
it. A **bignum** is an integer too large for one word, kept in the heap as a
row of **limbs**: fixed-size pieces of the number, like digits in a very
large base. In base 2^30, the number 2^40 is the two limbs `0` and `1024`,
because 2^40 = 0 + 1024 × 2^30. Arithmetic on bignums is the arithmetic
taught at school, done one limb at a time with a carry, instead of one
decimal digit at a time.

We settled the overall split in #16 before any of this was needed. Rust owns
the representation and the boundary with fixnums, because the collector has
to know what a bignum looks like and the fixnum fast path has to stay fast.
Lisp owns the algorithms: adding, multiplying, dividing and printing are
loops over limbs, and a loop in Lisp is sliced by the machine like any other
(rule 4). A multiplication of two ten-thousand-digit numbers in Rust would
run with nothing able to interrupt it, which in a browser is a frozen tab;
in Lisp it is just a long computation. Rule 9 names big-number arithmetic as
exactly the kind of primitive that needs this thought before it is written.

## Where does the switch happen?

Every call to `+`, `-`, `*`, `=`, `<` and `>` in compiled code goes through
one function in the machine, `Vm::call_value`, which calls the primitive. The
question was how an overflow there reaches the Lisp that knows what to do
(decision 1 in #16).

We could have redefined `+` and its relatives in Lisp, over fixnum-only
primitives that report an overflow instead of failing. That needs no change
to the machine, but it turns every addition in the system, including every
addition the compiler makes while compiling, into a Lisp call that conses a
list for its `&rest` arguments. We could also have let the overflow signal a
condition and retried the operation in a handler, which puts the condition
system on the arithmetic path. We chose a **fallback** in the machine:

```rust
Err(e) => match self.arithmetic_fallback(index, args, &e) {
    Some(generic) => self.call_value(generic, args, tail),
    None => self.fail(e),
},
```

When an arithmetic primitive fails with an overflow, or fails because one of
its arguments is a bignum, the machine looks up the matching Lisp function,
`%generic-+` and so on, and calls it with the same arguments. The fast path
does not change at all: the fallback runs only once the primitive has
already failed. Most Lisp systems split their arithmetic this way, with a
fast path in the machine and a slow, general path in the library.

The fallback finds its function by name, in the system's symbol table, and
then remembers it (more on that below). Before the library has defined the
function, the lookup finds no function and the old error stands, which is
what the interpreter and a world without the library still see.

## Why thirty bits?

The limb loops run in Lisp, on fixnums. If the product of two limbs, plus a
carry, did not fit in a fixnum, the code that implements bignums would
itself overflow and need bignums. A fixnum has 62 bits of magnitude, so a
limb can have at most 30: a product is then at most 60 bits, and the carry
and the partial sums of a multiplication fit in the bits left over (decision
2). It is a pleasing coincidence that CPython chose exactly the same: on a
64-bit machine its integers are stored as 30-bit "digits", with the sign kept
apart from the magnitude.

We store the sign apart too, which is called **sign and magnitude**: −5 is
"negative" together with the limbs of 5. The loops then only ever deal with
non-negative numbers, and the sign is settled once at the end. Inside the
library the working form of a number is a *magnitude*, a plain vector of
limbs with no leading zero, and every operation takes its arguments apart
into sign and magnitude, works, and puts the result back together:

```lisp
(defun %mul2 (a b)
  (multiple-value-bind (an am) (%magnitude a)
    (multiple-value-bind (bn bm) (%magnitude b)
      (%make-integer (not (eq an bn)) (%mag-mul am bm)))))
```

`%make-integer` ends by **normalising** the result: leading zero limbs are
dropped, and a value that fits in a fixnum becomes a fixnum. So a bignum is
never equal to a fixnum, and `(- (+ most-positive-fixnum 1) 1)` comes back
as an ordinary fixnum that `eq` can compare.

## What stays in Rust?

Rust makes a bignum, reads and writes its limbs, and does three operations
whose time grows with the length of the number: normalising, comparing and
hashing. Rule 9 asks for a plan for exactly these. Ours (decision 4) is an
argument rather than a mechanism. Each of the three is a single pass at
memory speed over an object that already exists, and allocating that object
was one step of the machine that touched every limb just as much, since a
new object's slots are all filled. None of them costs more than an
allocation the machine already accepts as one step. The long-running work,
multiplication and division, is in Lisp.

## Doing long division

Addition, subtraction and multiplication are the school methods, written
over limbs. Division by a single limb is also the school method, from the
top limb down, carrying the remainder. Division by a number of several limbs
is harder, because each limb of the quotient has to be *guessed*. We used
**Knuth's algorithm D** (*The Art of Computer Programming*, volume 2, 4.3.1),
the standard method.

At each step it estimates the next quotient limb from the top two limbs of
what is left of the dividend and the top limb of the divisor, then checks the
estimate against the divisor's second limb and corrects it. The trick that
makes the estimate good is to shift both numbers first, so that the
divisor's top limb has its highest bit set; after that the guess is never
more than two too large, and the check usually catches even that. When it
does not, subtracting the guess times the divisor goes negative, and the
algorithm adds the divisor back once:

```lisp
(let ((d (- (aref un (+ j n)) carry borrow)))
  (if (>= d 0)
      (setf (aref un (+ j n)) d)
      ;; QHAT was one too many, which happens rarely: add the
      ;; divisor back once.
      ...))
```

That last branch runs so rarely that ordinary tests may never reach it. So
besides the differential file, whose divisions SBCL agrees with, we checked
400 random divisions of numbers up to twelve limbs long, with limbs chosen to
sit at the edges (zero, one, all ones, the top bit alone), against Python's
integers, which are exact. All 400 agree. Whether any of them took the
add-back branch we cannot say without instrumenting the code.

`truncate`, `floor`, `mod` and `rem` were already written in Lisp (chapter
39) over two primitives, `%quotient` and `rem`. Those primitives now fall
back too, so the division family worked for bignums as soon as `%generic-quotient`
existed. `ceiling` and `round` were new, both written as a correction to
`truncate`.

## Working with bits

Common Lisp defines `logand`, `logior` and the other bit operations as if
every integer were stored in **two's complement**, the way the machine
stores a negative number: −1 is ones all the way to the left, forever. A
number stored as sign and magnitude has no such bits. So, following CPython
again, the bit operations convert their arguments to two's complement in
enough limbs, combine them limb by limb, and convert the result back. `ash`
is a multiplication or a `floor` by a power of two, `lognot` is −n−1, and
`logbitp`, `logcount` and `integer-length` follow from those.

The limb-by-limb step needed something Lisp did not have at all: `logand`
on two fixnums. Bit operations on a single word are what the hardware does in
one instruction and Lisp cannot build from arithmetic in any reasonable way,
so they are three more primitives, `%fixnum-logand`, `%fixnum-logior` and
`%fixnum-logxor`.

## What came for free, and what did not

The reader builds a number by multiplying and adding,
`(+ (* value 10) digit)`, and the printer takes one apart with `%quotient`
and `rem`. Both went through the fallback the first time they met a bignum,
so `123456789012345678901234567890` read and printed correctly without a
change to either. So did `format`'s `~:D` with its commas. `eql` gained one
case, comparing two bignums by value, and every function from chapter 45
whose `:test` defaults to `eql` became right for bignums with it. The hash
tables needed a few lines of Rust to hash a bignum by its limbs and compare
two by value, so that two equal bignums, which are two different objects,
find the same entry.

One thing did not come for free. The differential file read
`#x123456789ABCDEF0123456789`, and the Lisp reader turned out to have no
`#x`, `#b`, `#o` or `#r` syntax at all, although the printer has written `#x`
prefixes when `*print-radix*` is set since chapter 39. They are a dozen
lines of reader macros, added here.

## Finding a slow call that was always there

The first version worked and was slow: the differential file took eighteen
seconds, and a single bignum addition about three milliseconds. Timing the
pieces showed that the bignum code was not the problem. A bare `dotimes` of
100,000 iterations took five seconds, where the same loop written as a
`tagbody` took a tenth of a second. The difference was `>=`, which `dotimes`
calls on every pass and which takes `&rest` arguments, and in the end it came
down to one line in Rust. Every call to a function with `&optional`, `&rest`
or `&key` parameters looked up the *unsupplied marker*, the symbol a missing
argument is bound to (chapter 18), by name:

```rust
let marker = mc.intern(crate::lambda_list::UNSUPPLIED);
```

`intern` walks the whole system symbol table, comparing names, and the table
had grown to nearly two thousand symbols as the library grew. So every such
call cost about fifty microseconds, against two for a plain call. The bug
was as old as the lambda lists, and it had been getting slower with every
step; the bignum code, which calls `make-array` and its other `&key`
functions constantly, made it impossible to miss.

The fix is a small cache in the heap. A system symbol the machine needs on a
hot path is found by name once and then kept in a vector the collector knows
about. The unsupplied marker and the arithmetic fallback functions both use
it. The cache is not part of the world: saving an image drops it, and a
loaded world fills it again as it goes, so two images of the same world
still compare equal. With it, the twelve tier-1 differential files run in
five seconds instead of eighteen.

## What did it cost?

The Lisp is about 400 lines in `lisp/library/integers.lisp`. The Rust was
381 lines, comments included: the `bignum` module (164), twelve primitives
(about 125), the fallback (33), the symbol cache (about 50, including its
use in `lambda_list.rs`), hashing (7), and the new file's entry in
`boot::LIBRARY`. That is far more than the 40 lines we first thought of when
discussing the fallback alone, and it leaves 794 lines of the 15,000 for the
rest of phases 2 to 6, floats included. The
cold-built image grew from 639 KB to 688 KB. The fixpoint, our check that the
compiler compiles itself into exactly the same code generation after
generation (chapter 42), was not touched: the compiler only ever adds small
numbers, and its compiled code calls `+` exactly as before.

## What is still missing?

`/` does not exist, because dividing two integers in Common Lisp gives an
exact **ratio**, `(/ 1 3)` being `1/3`, and ratios are a numeric type of
their own whose tier is not yet decided (#59). `expt` with a negative power
is an error for the same reason. Floats are step M.

The algorithms are the plain ones. Multiplication is the schoolbook method,
quadratic in the length; Karatsuba's method and its successors are faster for
large numbers. Printing divides once per digit, which is quadratic too.
Neither matters until something measures a need.

---

**References.** The Common Lisp HyperSpec, chapter 12 (*Numbers*), especially
12.1.1 on the integer type and 12.1.3.2 on rational canonicalisation, and the
dictionary entries for `ash`, `logand`, `integer-length` and `isqrt`. Donald
Knuth, *The Art of Computer Programming*, volume 2, *Seminumerical
Algorithms*, section 4.3.1, for algorithm D and the analysis of its estimate.
CPython's `Objects/longobject.c`, for 30-bit digits, sign and magnitude, and
bit operations by conversion to two's complement.
