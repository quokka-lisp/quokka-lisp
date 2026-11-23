# Numbers with a point

> *Where this fits.* Chapter 47 gave Quokka integers of any size, held in
> the heap once they no longer fit in a tagged word, with the arithmetic
> written in Lisp and only the boundary in Rust. This chapter adds the other
> kind of number a program needs: the **floating-point number**, a fraction
> held to a fixed number of binary digits, which is what graphics, geometry
> and anything measured rather than counted are made of. Floats turn the
> bignum split around. Their arithmetic is a single machine instruction, so
> it goes in Rust, while the hard part is turning them into decimal text and
> back exactly. With this chapter Quokka reads, computes and prints floats as
> SBCL does, and the last numeric type tier 1 asks for is in place.

*Status: done, issue #17. Step M of phase 2.*

## What is a float?

Type `(+ 0.1 0.2)` into almost any programming language and the answer is
`0.30000000000000004`. Quokka now gives the same answer, and it is the right
one. A **float** stores a number as a whole number of binary digits, the
**significand** (also called the mantissa), scaled by a power of two, the
**exponent**, with a sign in front. The format nearly every computer uses is
**IEEE 754**, and its 64-bit form, the *double*, has one bit of sign, eleven
of exponent and 52 of significand, with a 53rd leading bit implied. So 1.5 is
3 × 2^-1, held exactly, but 0.1 has no finite binary expansion, just as 1/3
has no finite decimal one, and the double nearest to it is
0.1000000000000000055511151231257827.... Add that to the double nearest 0.2
and the sum lands one step above the double nearest 0.3. Nothing went wrong;
the decimal we typed was never what the machine held.

Issue #17 had said long ago that floats would be "a representation problem,
not an algorithm one". Bignums needed long division and Knuth's algorithm D,
and those took most of chapter 47. Floats need none of that: the processor
adds, multiplies, divides and takes square roots of doubles in one
instruction each, and IEEE 754 fixes the result of each to the last bit, so
every machine agrees. What is hard about floats is everything around that
instruction: where a double lives in Quokka's memory, what happens when a
result is too large, and how a double becomes text and back. We decided those
in #17 one question at a time, seven in all.

## Where does a double live?

Chapter 1 gave every Lisp value one 64-bit word, with the low three bits a
**tag** saying what the word is. A double needs all 64 bits for itself, so it
cannot sit inside a tagged word. There were two ways out. We could **box**
it: put the 64 bits in a small heap object and let the word refer to that,
as a bignum already is. Or we could turn the whole scheme around, as the
JavaScript engines in browsers do. **NaN boxing** stores every double as its
raw 64 bits, and hides every *other* kind of value inside the 2^51 or so bit
patterns that IEEE 754 spends on "not a number". Floats then cost nothing at
all.

We chose the box. NaN boxing would have replaced the tag scheme that chapter
1 is about, broken every saved image, and changed every function that reads a
word, for a gain only float-heavy programs would see. A double is now a heap
object of type `DoubleFloat` with one raw slot, which the collector copies
without looking inside, so nothing in the collector or the image format had
to change. Each float result allocates two words. The place where floats
will matter most is phase 4's graphics, and graphics mostly wants *arrays* of
coordinates, which can be stored as raw doubles side by side the way strings
store characters; we left those for phase 4, when `make-array` learns element
types.

## How many kinds of float?

Common Lisp names four float types, `short-float`, `single-float`,
`double-float` and `long-float`, and lets an implementation make some of them
the same type (CLHS 12.1.4). SBCL has 32-bit singles and 64-bit doubles. Its
reader turns `1.5` into a single, because the variable
`*read-default-float-format*` starts out as `single-float`, and `(sqrt 2)` is
a single too, printed as `1.4142135`.

We decided on doubles only. All four names denote the one 64-bit format,
which the standard permits, and `*read-default-float-format*` keeps its
standard initial value, `single-float`, which now names the same type, so
`1.5` reads as a double and prints as `1.5`. Having singles too, immediate
values in the tagged word as #17 had once suggested, would have meant every
primitive twice and rules for mixing the two formats, perhaps 150 more lines
of Rust, for a 32-bit format nobody in the project needs.

The price is a visible difference from SBCL: where SBCL makes a single, as
`(sqrt 2)` and `(float 1)` do, Quokka makes a double and prints more digits.
The differential tests, which compare Quokka's output with SBCL's form by
form (chapter 12), have to step around it. `tests/lisp/floats.lisp` begins
with `(setf *read-default-float-format* 'double-float)`, after which SBCL
reads every unmarked float as a double too, and the file never makes a float
from integers alone.

## Making float arithmetic fast

`+`, `-`, `*`, `=`, `<` and `>` are Rust primitives with a fast path for
fixnums. Chapter 47 added a **fallback**: when the fast path fails, because a
sum overflowed or an argument is a bignum, the virtual machine calls a Lisp
function such as `%generic-+` with the same arguments. Our first proposal was
to send floats down that same road. It needed the least Rust, but every float
addition would then have failed once in Rust, entered Lisp, built a list of
its arguments, and come back to Rust for the actual add. When we asked for
the fastest float arithmetic possible, that ruled it out.

So the primitives take doubles themselves. Each still tries its fixnum path
first, unchanged; only when that fails does it look for a double among the
arguments, in `crate::float`, and do the work there, two arguments at a time
from the left as SBCL does:

```rust
    let (x, y) = (a.float(), b.float());
    if matches!(op, Op::Div | Op::Rem) && y == 0.0 {
        return Err(division_by_zero());
    }
    let r = match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Div => x / y,
        // As SBCL computes it: the remainder after truncating the quotient.
        Op::Rem => x - (x / y).trunc() * y,
    };
```

`a.float()` is **float contagion** (CLHS 12.1.4.1): when a rational meets a
float, the rational is first converted to the nearest float, so `(+ 1 2.5)` is
`3.5`. Only a bignum meeting a double still goes through the Lisp fallback,
which converts the bignum and comes back here. Measured in a loop of 200,000
additions, adding doubles took as long as adding fixnums, about a second,
because the time goes into the loop around the addition and not the addition.
What is left of the cost of a float is its box.

Comparison is the one place where contagion does not apply. CLHS 12.1.4.1
says a float compared with a rational is compared as the exact rational it
is, so `(= (1+ (expt 2 70)) (float (expt 2 70)))` must be false even though
converting the integer to a double would make both sides equal. For a fixnum
against a double, Rust compares the integer with the double's whole part,
which is exact, and lets the fractional part settle a tie:

```rust
    let whole = x.trunc();
    n.cmp(&(whole as i64))
        .then_with(|| 0.0.partial_cmp(&(x - whole)).unwrap_or(Ordering::Equal))
```

The same idea in Lisp, `%compare-integer-float` in `lisp/library/floats.lisp`,
handles bignums.

`/` arrived with floats, because Quokka had none. Between two integers it
should give a **ratio**, `(/ 1 3)` being the exact fraction `1/3`, and ratios
are still undecided (#59). So `(/ 6 3)` is `2`, `(/ 1 3)` is an error saying
ratios do not exist yet, and anything with a float in it is a float.

## What happens when a float overflows?

IEEE 754 can represent infinity and "not a number" (**NaN**), and by default
hardware produces them quietly: dividing 1.0 by 0.0 gives infinity, and 0.0
by 0.0 gives NaN. SBCL turns that off. It enables the IEEE **traps** for
overflow, division by zero and invalid operations, so `(/ 1d0 0d0)` signals
`division-by-zero` and `(* 1d308 10)` signals `floating-point-overflow`. We
did the same. WebAssembly has no hardware traps and rule 2 wants every target
to behave alike, so the trap is a check in the one Rust function that boxes a
double:

```rust
pub fn make<'h>(mc: &mut Mutation<'h>, x: f64) -> Result<Value<'h>, EvalError> {
    if x.is_nan() {
        return Err(fault(
            "FLOATING-POINT-INVALID-OPERATION",
            "floating-point invalid operation",
        ));
    }
    if x.is_infinite() {
        return Err(fault("FLOATING-POINT-OVERFLOW", "floating-point overflow"));
    }
```

Every double Lisp can see is made here, so no infinity or NaN ever reaches a
Lisp program, and the reader, the printer and `eql` never have to decide what
to do with one. Underflow, a result too small for the normal range, is not
trapped, as in SBCL: it becomes a **subnormal**, a number that gives up
leading bits of precision to go lower, or zero.

Two zeros remain, `0.0` and `-0.0`, which IEEE 754 keeps apart so that a
result's sign survives underflow. They are `=`, but not `eql`, because `eql`
on floats compares their bits; `abs`, `signum` and the printer look at the
sign bit rather than asking `minusp`, which says no to both.

## Reading and printing a float

Reading `0.1` means finding the double nearest to the decimal 0.1, and
printing a double means finding the shortest decimal that reads back as that
same double, which for the double nearest 0.1 is `0.1`. Both are known hard
problems with known answers. The obvious way to read, multiplying by ten and
adding digit by digit, rounds at every step and can be wrong in the last
bit; William Clinger showed in 1990 how to read exactly with big integers.
Printing exactly is the problem Guy Steele and Jon White solved the same
year, which Robert Burger and Kent Dybvig restated in 1996 in about sixty
lines of Scheme, and which Grisu (2010) and Ryu (2018) later made fast.

Our first recommendation was to write Clinger's reader and Burger and
Dybvig's printer in Lisp over the bignums of chapter 47, which is partly why
#39 put floats after bignums. It would have made a good chapter, but it was
slow, tens of microseconds for an ordinary number and up to a millisecond for
an extreme one. We chose Rust instead. Rust's `str::parse::<f64>` rounds
correctly (with Eisel and Lemire's fast method and an exact fallback), and
its formatter writes the shortest round-tripping digits (Grisu, falling back
to Dragon4 where Grisu cannot be sure). Two primitives expose them, which
took about thirty lines, and each conversion takes about a microsecond.

The Lisp reader recognises a float token by CLHS 2.3.1's syntax, rewrites its
exponent marker as `e` (`1.5d3`, `1.5s3` and `1.5e3` are the same double
here), and hands the text to `%parse-double`. The Lisp printer gets the digits
and the exponent from `%double-digits` and lays them out as SBCL does, in
positional notation between 10^-3 and 10^7 and with an exponent outside it:

```lisp
(defun %print-float (float out)
  (if (%negative-float-p float) (write-char #\- out))
  (if (zerop float)
      (write-string "0.0" out)
      (multiple-value-bind (digits exponent) (%float-digits float)
        (if (and (>= exponent -3) (< exponent 7))
            (write-string (%fixed-shortest float) out)
            (progn
              (write-string (subseq digits 0 1) out)
              (write-char #\. out)
              (write-string (if (> (length digits) 1) (subseq digits 1) "0") out)
              (write-char #\e out)
              (%print-integer-in-decimal exponent out))))))
```

So `9999999.0` prints as itself and `1d7` as `1.0e7`, `0.001` as itself and
`9.99d-4` as `9.99e-4`. SBCL would add an exponent marker, `1.5d0`, to a
float that is not of the default format; with one format there is never one.

Since the reader and the printer are now two halves that must agree, a test
writes a few hundred random doubles, from every part of the range, as text in
the three exponent syntaxes, has the Lisp reader read them, checks the bits
it got through `integer-decode-float`, and checks that printing each one and
reading it again gives the same double back.

## Rounding the way SBCL's `format` rounds

`format`'s float directives, `~F`, `~E`, `~G` and `~$`, ask for a fixed
number of digits, and rounding a double to them is where the decimal surprise
of `0.1` comes back. `(format nil "~,2F" 2.675)` in SBCL is `"2.67"`, not
`"2.68"`, because the double nearest 2.675 is 2.67499999999999982236431605997495353221893310546875.
When we probed SBCL with a list of cases before writing anything, it turned
out to round the double's exact value, with an exact half going up: `~,1F` of
0.25 (which is exact in binary) is `"0.3"`, and `~,0F` of 2.5 is `"3."`. Rust's
formatter rounds a tie to even instead, so we could not use it here. The
digits come from the double's own bits, in Lisp, with bignums when they are
needed:

```lisp
(defun %scaled-integer (double places)
  (destructuring-bind (mantissa exponent sign) (%decode-double double)
    sign
    (let ((numerator (* mantissa (ash 1 (max exponent 0)) (expt 10 (max places 0))))
          (denominator (* (ash 1 (max (- exponent) 0)) (expt 10 (max (- places) 0)))))
      (multiple-value-bind (quotient remainder) (floor numerator denominator)
        (if (>= (* 2 remainder) denominator) (+ quotient 1) quotient)))))
```

The double is exactly *mantissa* × 2^*exponent*, so |x| × 10^*places* is a
fraction of two integers, and rounding it is one integer division. The probe
turned up one more surprise: `(format nil "~,3E" 9.9996)` in SBCL is
`"10.000e+0"`, two digits before the point, because SBCL takes the exponent
from the unrounded number and does not adjust it when rounding carries. We
copied that, since the differential tests hold us to SBCL's output and not to
what CLHS might prefer.

## Why is `sin` not the same everywhere?

IEEE 754 fixes `+`, `-`, `*`, `/` and the square root to the last bit. It does
not fix `sin`, `exp`, `log` and the other **transcendental functions**, and
maths libraries differ in their last bit. Rust's `f64::sin` calls a different
library on each of our targets: Apple's on a Mac, a port of musl's in the
browser, wasi-libc's under wasmtime. Quokka has a reason to care that most
languages do not: an image saved on a Mac can be loaded in a browser (chapter
30), and the same computation should give the same answer there.

So the transcendental primitives call the `libm` crate, a pure-Rust port of
musl's maths library (which goes back to Sun's fdlibm from 1993), on every
target. It is the project's first dependency in the virtual machine crate,
whose `Cargo.toml` allows only pure computation. SBCL uses yet another
library, and it shows: Quokka's `(exp 1d0)` is 2.7182818284590455 where
SBCL's is 2.718281828459045, one step apart in the last bit. The test file
therefore prints transcendental results through `~,10F` and checks exact
identities such as `(sin 0d0)` and `(exp 0d0)` as they are.

## What did it cost?

Rust grew by 515 lines: the new `float.rs` (321 lines, a third of them
comments), 24 primitives with their table rows and match arms (161), and a
handful of lines in the heap, the hash tables, the printer, the error type and
the condition that an error becomes. We had estimated about 185, counting the
box, the faster primitives, the traps and the two conversions; the
transcendental functions alone, fourteen of them each needing a table row,
were not in that estimate, and `rustfmt` spreads the rest wider than we
counted. The core is now 14,721 lines of the 15,000 guide, which leaves 279
for phases 2 to 6. The Lisp is `floats.lisp` and the float directives at the
end of `format.lisp`, about 300 lines between them, plus changes of a few
lines each to `truncate`, `floor`, `abs`, `zerop`, `expt`, `eql`, `equalp`,
`coerce`, the reader and the printer. The cold-built image grew from 688 KB to
726 KB.

The fixpoint, our check that the compiler compiles itself into exactly the
same code generation after generation (chapter 42), held without any change:
the compiler uses no floats, and its compiled code calls `+` exactly as
before. A warm rebuild now reads 839 forms and replaces 712 names, and it no
longer fitted in the step budget the `quokka rebuild` command gave it, a
hundred million steps; we doubled that to the figure the tests already used.

## What is still missing?

Ratios, and with them `rational`, `rationalize` and `/` on integers that do
not divide, wait for #59. Complex numbers are not planned, so `(sqrt -1)` is
an error where SBCL returns `#C(0.0 1.0)`. Single floats do not exist, so a
program that depends on 32-bit precision gets 64. Arrays of raw doubles are
phase 4's. And every float result still allocates a box; a compiler that
proved a variable always holds a double could keep it unboxed, but that needs
type inference, which belongs to a later phase.

---

**References.** IEEE Standard for Floating-Point Arithmetic, IEEE 754-2019.
David Goldberg, "What Every Computer Scientist Should Know About
Floating-Point Arithmetic", *ACM Computing Surveys* 23(1), 1991. William D.
Clinger, "How to Read Floating Point Numbers Accurately", PLDI 1990. Guy L.
Steele Jr. and Jon L. White, "How to Print Floating-Point Numbers
Accurately", PLDI 1990. Robert G. Burger and R. Kent Dybvig, "Printing
Floating-Point Numbers Quickly and Accurately", PLDI 1996. Florian Loitsch,
"Printing Floating-Point Numbers Quickly and Accurately with Integers"
(Grisu), PLDI 2010. Ulf Adams, "Ryū: Fast Float-to-String Conversion", PLDI
2018. Daniel Lemire, "Number Parsing at a Gigabyte per Second", *Software:
Practice and Experience*, 2021. The Common Lisp HyperSpec, chapter 12,
especially 12.1.4 on floats and contagion, 2.3.1 on the syntax of numbers,
and 22.3.3 on `format`'s float directives.
