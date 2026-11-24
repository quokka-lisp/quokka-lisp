# An array that holds nothing

> *Where this fits.* Chapter 55 gave Quokka every kind of array in CLHS
> 15: simple vectors, strings, bit vectors, and the headers that give an
> array several dimensions, a fill pointer or a displacement. Chapter 62
> sorted ansi-test's remaining failures by cause, and one cause was an
> array we had not made at all, the one whose element type is `nil`. This
> chapter adds it, and shows what we did when the standard and SBCL, our
> oracle, disagreed about it.

*Status: done, issue #77. Step AD of phase 2.*

## Element type nil

When you make an array you can say what its elements will be:

```lisp
(make-array 10 :element-type 'bit)
(make-array 10 :element-type 'character)
```

A Lisp may keep such an array more compactly than a general vector, 64
bits to a word for the first and a character per word for the second, as
chapter 55 described. Any type at all can be given, and an implementation
*upgrades* the ones it has no special array for to a type it does have,
usually `t`, the type of every object. So `(make-array 10 :element-type
'symbol)` is an ordinary vector that happens to be told it will hold
symbols.

The type `nil` is the one type that has no objects in it. An array of
element type `nil` therefore cannot hold anything: it has a length, it
has dimensions, and every attempt to read or store an element is an
error. That sounds useless, and for most programs it is, but it falls out
of the type system, so CLHS has to say what it is, and ansi-test checks
that an implementation agrees. Until this step, `%element-kind` in
`arrays.lisp` upgraded `nil` to `t` along with everything else, and
`(array-element-type (make-array 3 :element-type nil))` answered `t`.

## Two answers

CLHS 15.1.2.2 says that `(vector nil)` is a string. Its argument is
short: a string is a vector whose element type is a subtype of
`character`, and `nil` is a subtype of every type. ansi-test has thirty
tests that rely on this, each marked with the note
`:nil-vectors-are-strings`, and they expect `stringp` to be true of an
empty nil array, `string-upcase` to take one, and `prin1` to print it as
`""`.

Before writing any code we asked SBCL, because rule 6 says that where
Quokka implements a CL feature it must behave as SBCL does. SBCL 2.6
answers `nil` to `(stringp (make-array 0 :element-type nil))`, refuses it
as a string designator, and prints it as `#<(SIMPLE-ARRAY NIL (0))
{...}>`. Its array is real, with element type `nil`, but it is not a
string. ansi-test anticipates this: its notes file disables
`:nil-vectors-are-strings` for OpenMCL and ECL, which made the same
choice.

So there were two decisions. The first was whether the array should be
real at all, or whether `make-array` should quietly hand back an empty
string. A string would have made `array-element-type` answer `character`
and let a program store characters into an array declared to hold none,
so we made it real, as SBCL does. The second was whether it is a string,
and there we followed SBCL rather than the text of CLHS. The tests that
assume otherwise now count as needing a disabled note, the same way
ansi-test's own configuration treats them on other Lisps, and the runner's
list of disabled notes says why.

## The object

The array is a new kind of heap object, `ObjectType::NilVector`. Like a
bit vector it is *raw*: the collector does not look inside it for
references, which here is easy, because there is only a count.

```rust
/// `(%make-nil-vector n)`: a simple array of N elements of type nil.
pub fn make_nil_vector<'h>(mc: &mut Mutation<'h>, n: Value<'h>) -> Result<Value<'h>, EvalError> {
    let n = match n.as_fixnum() {
        Some(n) if n >= 0 => n as u64,
        _ => {
            return Err(EvalError::WrongType {
                name: "MAKE-ARRAY",
                wanted: "a non-negative fixnum size",
            })
        }
    };
    let v = mc.alloc(ObjectType::NilVector, 1);
    mc.set_raw_slot(v, 0, n);
    Ok(v)
}
```

`array.rs` already sorted every simple array into a `Kind` (a general
vector, a string, or bits) so that `aref` and `length` know how to reach
an element. We added a fourth kind, `Nothing`, whose length is the count
and whose `read` and write both refuse. Everything built on top of a
simple array works unchanged: a nil array with a fill pointer, or with
two dimensions, is an ordinary header in front of a `NilVector`.

The Lisp side is a line in each of the functions that turn an element
type into a kind and back:

```lisp
(defun %element-kind (type)
  (cond ((null type) :nil)
        ((member type '(character base-char standard-char extended-char)) :character)
```

`upgraded-array-element-type` and `array-element-type` answer `nil` for
it, and `make-array` refuses an `:initial-element` for a nil array that
is not empty, since there is nowhere to put one. The printer prints one
the way SBCL does, by its type, as `#<(ARRAY NIL (3))>`; until we taught
it that, printing a non-empty nil array tried to read its first element
and failed.

## What did it cost?

The Rust is about thirty lines: the object type, the primitive, and the
new kind in `array.rs`. One differential file, `nil-arrays.lisp`, holds
Quokka to SBCL on all of it, including the refusals: a nil array is not a
string designator, so `string-upcase` of one is an error in both.

The thirty ansi-test failures did not turn into passes. They moved to
*needs later*, explained by the note we disabled, which is what phase 2's
gate asks of every remaining failure: that it is explained by something
we chose not to have, not left as a plain bug. Eleven tests that had
passed went the same way, `string-trim` of an empty nil array among
them, because the runner does not run a test whose note is disabled. The
count of passing tests fell from 8,018 to 8,007, and plain failures from
121 to 91.
