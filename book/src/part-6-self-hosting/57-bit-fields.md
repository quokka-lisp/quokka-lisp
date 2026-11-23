# Bit fields

> *Where this fits.* Chapter 47 gave Quokka integers of any size, and with
> them `logand`, `logior`, `logxor`, `lognot` and `ash`, the operations that
> treat an integer as a row of bits. Common Lisp builds more on those five:
> the other logical operations, `boole` with its sixteen named operations,
> and *byte specifiers*, which name a field of bits so that `ldb` can read it
> and `dpb` can replace it. This chapter, the eighth worked from ansi-test's
> list (chapter 49), adds them all, in Lisp, and shows how a `setf` place can
> sit inside another place.

*Status: done, issue #69. Step V of phase 2.*

## How should a negative number's bits behave?

Common Lisp treats an integer as if it were written in two's complement with
infinitely many bits, so a negative number has an endless row of ones to
its left: -1 is all ones, and -12 is ones followed by `0100`. That is why
`(lognot 0)` is -1, and why every logical operation is defined on integers
of any size and sign without saying how wide they are. Chapter 47 did the
hard part, carrying out `logand` and its two siblings on bignums by
converting them to two's complement limb by limb.

With those in place, the other eight operations are one line each, written
exactly as CLHS defines them: `lognand` is `(lognot (logand a b))`,
`logandc1` is `(logand (lognot a) b)`, `logtest` asks whether `(logand a b)`
is non-zero, and so on. Because they are built on the general operations,
they work alike on fixnums, negative numbers and bignums.

One of them was wrong in our first version. `logeqv`, the bitwise "both the
same", takes any number of arguments, and we wrote it as `lognot` of the
`logxor` of all of them. For two arguments that is right. For three it is
not: SBCL answered `(logeqv 1 2 3)` with 0 and ours with -1. The n-argument
version is the two-argument one applied from left to right, whose identity
is -1, and the shortcut through `logxor` loses that. It is a fold now.

## What are the sixteen boole operations?

Two input bits can be combined in sixteen ways, from "always 0" through
"and", "or", "exclusive or" and their complements to "always 1". `boole`
takes one of sixteen named constants, `boole-and`, `boole-xor`,
`boole-andc1` and the rest, and applies that operation to two integers. The
constants are numbers whose values the standard leaves open; ours are
SBCL's, 0 to 15, so that a program that prints one and reads it back in the
other gets the same operation.

## What is a byte specifier?

A *byte* in Common Lisp is not eight bits but any field of an integer: `(byte
4 8)` names the four bits starting at bit 8, counting from the least
significant. The standard leaves what `byte` returns to the implementation,
and we decided to use a cons of the size and the position, as SBCL does, so
`byte-size` is `car` and `byte-position` is `cdr`.

The operations on fields are a few lines each:

```lisp
(defun ldb (bytespec integer)
  (logand (ash integer (- (byte-position bytespec))) (- (ash 1 (byte-size bytespec)) 1)))

(defun dpb (newbyte bytespec integer)
  (let ((mask (%byte-mask bytespec)))
    (logior (logand integer (lognot mask))
            (logand (ash newbyte (byte-position bytespec)) mask))))
```

`ldb` shifts the field down to bit 0 and masks off the rest; `dpb` clears
the field in the integer and puts the low bits of the new value in its
place. `mask-field` and `deposit-field` are the same two without the
shifting, and `ldb-test` asks whether any bit of a field is set.

## How can a place sit inside a place?

`(setf (ldb (byte 4 4) x) 10)` changes four bits of `x`. It cannot be done
by changing the integer, because integers are immutable; it has to compute
a new integer with `dpb` and store that back into `x`. And `x` could as well
be `(aref v 1)` or `(car cell)`, any place at all. So `setf` of `ldb` is
written with `define-setf-expander` (chapter 43): it asks for the inner
place's own expansion, reads the old integer through it, and stores the
`dpb` of the new field through it.

```lisp
(defun %field-setf-expansion (reader writer bytespec place environment)
  (multiple-value-bind (temps values stores store-form access-form)
      (get-setf-expansion place environment)
    (let ((byte-temp (gensym "BYTE"))
          (new (gensym "NEW")))
      (values (cons byte-temp temps)
              (cons bytespec values)
              (list new)
              `(let ((,(car stores) (,writer ,new ,byte-temp ,access-form)))
                 ,store-form
                 ,new)
              `(,reader ,byte-temp ,access-form)))))
```

Because it is a full expansion with temporaries, everything chapter 43
built on places works through it: `(incf (ldb (byte 4 0) (car cell)) 3)`
evaluates the byte specifier and the inner place once each, and adds 3 to
the field.

## What did it cost?

This step needed no Rust. It took about 90 lines of Lisp at the end of
`integers.lisp`, and `tests/lisp/bit-fields.lisp` checks every operation
against SBCL, on small numbers, negative ones and bignums, and `setf` of a
field through a variable, a vector element and a list.

ansi-test went from 7,600 passing tests to 7,699, of 13,558, and the numbers
group from 432 to 528. The tests needing a tier-1 feature fell from 492 to
322, and none of those left in the numbers group is about bits.

## What is still missing?

Nothing of the standard's chapter 12 that deals with bits is missing now.
A byte specifier prints as the cons it is, which CLHS allows and a program
should not rely on.

---

**References.** The Common Lisp HyperSpec, 12.1.1.3 on logical operations
and two's complement, and the entries for `boole`, `byte`, `ldb` and `dpb`.
Guy Steele, *Common Lisp the Language*, 2nd edition, 1990, section 12.8, on
byte specifiers and their origin in the PDP-10's byte instructions.
