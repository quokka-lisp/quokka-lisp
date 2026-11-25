//! Tagged words: how every Lisp value is represented.
//!
//! Every Lisp object in Quokka is exactly one 64-bit word. A word either *is*
//! the data — a small integer, a character, `nil` — or it *refers* to an object
//! in the heap. Which of the two, and what kind, is recorded in the low bits.
//! This is the scheme Symbolics' Ivory used, and `docs/design.md` specifies it:
//!
//! | Low bits | Meaning | Payload |
//! |---|---|---|
//! | `xx0` | Fixnum | a 63-bit signed integer, in the upper 63 bits |
//! | `001` | Cons | where the cons cell is |
//! | `011` | Heap object | where the object is; its header says what type |
//! | `101` | Immediate | characters, `nil`, `t`, the unbound marker |
//! | `111` | Forwarding | where an object moved to, during a collection |
//!
//! # Why the fixnum tag is a single zero bit
//!
//! Fixnums get the largest share of the encoding space — half of it — and the
//! cheapest test, because they are the most common value in any running Lisp and
//! the only one whose operations are on the critical path.
//!
//! A fixnum `n` is stored as `n << 1`, so the low bit is always clear. That
//! looks like a cost (one bit of range lost, a shift to read the value) and is
//! actually the opposite:
//!
//! ```text
//! tagged(a) + tagged(b)  ==  2a + 2b  ==  2(a + b)  ==  tagged(a + b)
//! ```
//!
//! Addition and subtraction need no untagging at all — add the words, the
//! result is already a correctly tagged fixnum. Comparison likewise: doubling
//! preserves order, so comparing the raw words compares the numbers.
//!
//! The better part is overflow. A fixnum's payload spans [`MIN_FIXNUM`] to
//! [`MAX_FIXNUM`], and doubling maps that range onto *exactly* the range of
//! `i64`. So `i64::checked_add` on the tagged words overflows precisely when the
//! mathematical sum leaves fixnum range — not approximately, exactly. The
//! machine's own overflow detection becomes the range check, at no cost, and
//! `None` is the signal to promote to a bignum. See [`Value::checked_add`].
//!
//! # Why heap references have three bits spare
//!
//! Every heap object is 8-byte aligned, so its offset from the heap base is a
//! multiple of 8 and its low three bits are always zero. That is where the tag
//! goes. The payload is therefore the object's byte offset with the tag ORed
//! into the space alignment already guaranteed to be empty — which, since a word
//! is 8 bytes, is the same number as the object's index into the heap's word
//! array. [`Value::word_index`] returns that index.
//!
//! These are **offsets, not addresses** (rule 3). An offset means the same thing
//! on 64-bit native and on wasm32, and in an image file saved by one and loaded
//! by the other, so images need no relocation. See the image discussion in
//! `docs/design.md`.
//!
//! # What this module is not
//!
//! Representation only. There is no heap here, nothing allocates, and nothing
//! can move. Bignum promotion, type dispatch and everything else live above.

use core::fmt;
use core::marker::PhantomData;

/// The lifetime brand that keeps values inside a mutation.
///
/// **Invariant in `'h`, and that is the whole point.** The obvious spelling,
/// `PhantomData<&'h ()>`, is *covariant*: it would let a long brand be shortened
/// to a short one, which is exactly the escape the scheme is meant to prevent.
/// The guarantee would silently evaporate and everything would still compile.
///
/// `fn(&'h ()) -> &'h ()` puts `'h` in both argument and return position, which
/// forces invariance. The doctests on [`Value`] prove it: one asserts that
/// shortening the brand does not compile, and the matching test in
/// `crate::heap` asserts that the *covariant* spelling would.
pub type Invariant<'h> = PhantomData<fn(&'h ()) -> &'h ()>;

/// Number of tag bits in the low end of a word.
pub const TAG_BITS: u32 = 3;

/// Mask selecting the tag bits.
pub const TAG_MASK: u64 = (1 << TAG_BITS) - 1;

/// Tag `001`: a reference to a cons cell.
pub const TAG_CONS: u64 = 0b001;

/// Tag `011`: a reference to a heap object whose header gives its type.
pub const TAG_OBJECT: u64 = 0b011;

/// Tag `101`: an immediate — a character, `nil`, `t`, the unbound marker.
pub const TAG_IMMEDIATE: u64 = 0b101;

/// Tag `111`: a forwarding pointer, left behind by a moved object during a
/// collection. Never visible outside the collector.
pub const TAG_FORWARDING: u64 = 0b111;

/// The largest integer representable as a fixnum.
pub const MAX_FIXNUM: i64 = (1 << 62) - 1;

/// The smallest integer representable as a fixnum.
pub const MIN_FIXNUM: i64 = -(1 << 62);

/// The largest word index a heap reference can hold.
///
/// A `u64`, not a `usize`, because this is a property of the *encoding* — 61
/// bits of index — and must mean the same thing on 64-bit native and on wasm32,
/// where `usize` is narrower. On wasm32 the real limit is lower still: linear
/// memory tops out at 4 GiB, so a heap cannot approach this.
pub const MAX_WORD_INDEX: u64 = u64::MAX >> TAG_BITS;

// Immediates carry a 5-bit subtag above the main tag, then a payload.
//
//   [ payload : 56 ][ subtag : 5 ][ 101 ]
const SUBTAG_SHIFT: u32 = TAG_BITS;
const SUBTAG_MASK: u64 = 0b11111 << SUBTAG_SHIFT;
const IMM_PAYLOAD_SHIFT: u32 = 8;

const SUBTAG_SINGLETON: u64 = 0 << SUBTAG_SHIFT;
const SUBTAG_CHARACTER: u64 = 1 << SUBTAG_SHIFT;

/// A primitive: an operation the VM performs in Rust rather than in Lisp.
///
/// The payload is an index into the primitive table, not a heap reference, so a
/// primitive is an immediate and costs no allocation. That index is part of the
/// VM, like bytecode will be: an image carries primitive references, so the VM
/// that loads it must have the same table. Adding primitives is therefore an
/// image-format change, and reordering them is a breaking one.
pub(crate) const SUBTAG_PRIMITIVE: u64 = 2 << SUBTAG_SHIFT;

/// Reserved: marks an object header, which is **not a value**.
///
/// The collector scans to-space linearly, so at every position it must know how
/// big the object there is. A headed object's header says; a cons cell has no
/// header, because the tag is its type. Something has to tell the two apart.
///
/// Giving cons cells a header too would cost a third more memory on the most
/// common object in a Lisp. Instead a header is encoded in the immediate space
/// with a subtag that is never handed out, so it cannot be mistaken for a value
/// — in particular, a cons cell's car can never impersonate one. The scan
/// becomes a single test, and conses stay two words.
///
/// The layout above the subtag is [`crate::heap`]'s business.
pub(crate) const SUBTAG_HEADER: u64 = 31 << SUBTAG_SHIFT;

// `nil` and `t` were singletons until they became static symbols; only the
// unbound marker is left, and it is not a Lisp object at all.
const SINGLETON_UNBOUND: u64 = 2;

const fn singleton<'h>(which: u64) -> Value<'h> {
    Value(
        (which << IMM_PAYLOAD_SHIFT) | SUBTAG_SINGLETON | TAG_IMMEDIATE,
        PhantomData,
    )
}

/// What kind of thing a word is.
///
/// Returned by [`Value::kind`] so that code which must handle every case — the
/// collector, the printer — can match exhaustively and be told by the compiler
/// when a case is missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A small integer, stored in the word itself.
    Fixnum,
    /// A reference to a cons cell.
    Cons,
    /// A reference to a heap object; its header word gives the type.
    Object,
    /// A character, `nil`, `t` or the unbound marker.
    Immediate,
    /// A forwarding pointer. Only during a collection.
    Forwarding,
}

/// One Lisp value: a single tagged 64-bit word.
///
/// `Value` is `Copy` and comparing two with `==` is Common Lisp's `eq` — it
/// compares the words, so two fixnums with the same value are `eq`, and two
/// distinct cons cells holding equal contents are not.
///
/// # The brand
///
/// `'h` ties a value to one mutation of one heap. It refers to nothing — it is
/// a [`PhantomData`] marker — and exists only so the compiler can refuse to let
/// a value outlive the mutation that produced it. See [`crate::heap`].
///
/// The brand must be **invariant**, and this doctest is the proof. If `'h` were
/// covariant a long brand could be shortened at will, which is exactly the
/// escape the scheme prevents, and everything would still compile:
///
/// ```compile_fail
/// # use quokka_vm::value::Value;
/// fn shorten<'long: 'short, 'short>(v: Value<'long>) -> Value<'short> {
///     v
/// }
/// ```
///
/// A test in `crate::heap` asserts that the covariant spelling *would* compile,
/// so the above is really testing variance rather than failing for some
/// unrelated reason.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value<'h>(u64, Invariant<'h>);

impl<'h> Value<'h> {
    /// The empty list, false, and the symbol named `"NIL"`.
    ///
    /// A reference to a **real symbol**, not an immediate — one living at a
    /// fixed index in the heap's static area, which the collector never moves.
    /// That is what lets this be a compile-time constant while `nil` still has
    /// a name, a value cell and a property list, as Common Lisp requires.
    ///
    /// It took four special cases elsewhere to learn that an immediate was the
    /// wrong answer. See `crate::heap::statics`.
    pub const NIL: Value<'h> = Value(
        ((crate::heap::statics::NIL_INDEX as u64) << TAG_BITS) | TAG_OBJECT,
        PhantomData,
    );

    /// Canonical true, and the symbol named `"T"`. Also a static symbol.
    pub const T: Value<'h> = Value(
        ((crate::heap::statics::T_INDEX as u64) << TAG_BITS) | TAG_OBJECT,
        PhantomData,
    );

    /// Marks a variable or slot that has no value.
    ///
    /// Never visible to Lisp code: reading an unbound variable signals rather
    /// than returning this.
    pub const UNBOUND: Value<'h> = singleton(SINGLETON_UNBOUND);

    /// Reinterpret a raw word as a value.
    ///
    /// For the heap, the collector and the image loader, which read words back
    /// out of storage. Not every `u64` is a meaningful value — an immediate with
    /// an unassigned subtag, for instance — so an image loader should check what
    /// it reads rather than trusting the file.
    pub(crate) const fn from_bits(bits: u64) -> Value<'h> {
        Value(bits, PhantomData)
    }

    /// The raw word, for storing into the heap or an image.
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Which kind of value this is.
    pub const fn kind(self) -> Kind {
        if self.0 & 1 == 0 {
            return Kind::Fixnum;
        }
        match self.0 & TAG_MASK {
            TAG_CONS => Kind::Cons,
            TAG_OBJECT => Kind::Object,
            TAG_IMMEDIATE => Kind::Immediate,
            _ => Kind::Forwarding,
        }
    }

    // ---- fixnums ---------------------------------------------------------

    /// Is this a fixnum?
    pub const fn is_fixnum(self) -> bool {
        self.0 & 1 == 0
    }

    /// Make a fixnum, or `None` if `n` is out of range.
    ///
    /// `None` means "this needs a bignum", which is a job for the layer above.
    pub const fn fixnum(n: i64) -> Option<Value<'h>> {
        if n < MIN_FIXNUM || n > MAX_FIXNUM {
            None
        } else {
            Some(Value((n << 1) as u64, PhantomData))
        }
    }

    /// The integer in a fixnum, or `None` if this is not one.
    pub const fn as_fixnum(self) -> Option<i64> {
        if self.is_fixnum() {
            // Arithmetic shift: sign-extends, so negative fixnums come back
            // negative.
            Some((self.0 as i64) >> 1)
        } else {
            None
        }
    }

    // ---- heap references -------------------------------------------------

    /// Is this a reference to a cons cell?
    pub const fn is_cons(self) -> bool {
        self.0 & TAG_MASK == TAG_CONS
    }

    /// Is this a reference to a tagged heap object?
    pub const fn is_object(self) -> bool {
        self.0 & TAG_MASK == TAG_OBJECT
    }

    /// Is this a forwarding pointer left by the collector?
    pub const fn is_forwarding(self) -> bool {
        self.0 & TAG_MASK == TAG_FORWARDING
    }

    /// Does this refer to somewhere in the heap?
    pub const fn is_heap_ref(self) -> bool {
        self.is_cons() || self.is_object() || self.is_forwarding()
    }

    /// Refer to the cons cell at `word_index`.
    ///
    /// `pub(crate)`: only [`crate::heap::Mutation`] may hand out a reference
    /// into the heap. If anyone could construct one at an arbitrary brand the
    /// lifetime guarantee would be worthless — it stops a value escaping a
    /// mutation, not being invented outside one.
    pub(crate) const fn cons(word_index: usize) -> Option<Value<'h>> {
        Value::heap_ref(word_index, TAG_CONS)
    }

    /// Refer to the heap object at `word_index`.
    ///
    /// `pub(crate)`: only [`crate::heap::Mutation`] may hand out a reference
    /// into the heap. If anyone could construct one at an arbitrary brand the
    /// lifetime guarantee would be worthless — it stops a value escaping a
    /// mutation, not being invented outside one.
    // Not called from the library yet: #2 gives heap objects their headers. The tests below do
    // exercise it, but test-only use does not satisfy dead-code analysis.
    #[allow(dead_code)]
    pub(crate) const fn object(word_index: usize) -> Option<Value<'h>> {
        Value::heap_ref(word_index, TAG_OBJECT)
    }

    /// A forwarding pointer to `word_index`. For the collector only.
    ///
    /// `pub(crate)`: only [`crate::heap::Mutation`] may hand out a reference
    /// into the heap. If anyone could construct one at an arbitrary brand the
    /// lifetime guarantee would be worthless — it stops a value escaping a
    /// mutation, not being invented outside one.
    // Not called from the library yet: #3 writes the collector. The tests below do
    // exercise it, but test-only use does not satisfy dead-code analysis.
    #[allow(dead_code)]
    pub(crate) const fn forwarding(word_index: usize) -> Option<Value<'h>> {
        Value::heap_ref(word_index, TAG_FORWARDING)
    }

    const fn heap_ref(word_index: usize, tag: u64) -> Option<Value<'h>> {
        if word_index as u64 > MAX_WORD_INDEX {
            None
        } else {
            Some(Value(((word_index as u64) << TAG_BITS) | tag, PhantomData))
        }
    }

    /// Where in the heap this refers to, or `None` if it refers nowhere.
    ///
    /// The index is into the heap's array of words. It is the same number as the
    /// object's byte offset from the heap base divided by eight, which is why
    /// 8-byte alignment is what frees the tag bits.
    ///
    /// On a 32-bit host the encoded index can in principle exceed `usize` — an
    /// image saved from a very large 64-bit heap — and this would truncate. Such
    /// an image cannot fit in wasm32's 4 GiB of linear memory anyway, so the
    /// image loader rejects it on the way in; the assertion records the
    /// invariant rather than defending against it here.
    pub fn word_index(self) -> Option<usize> {
        if self.is_heap_ref() {
            let index = self.0 >> TAG_BITS;
            debug_assert!(index <= usize::MAX as u64, "heap index wider than usize");
            Some(index as usize)
        } else {
            None
        }
    }

    // ---- immediates ------------------------------------------------------

    /// Is this an immediate — a character, `nil`, `t` or the unbound marker?
    pub const fn is_immediate(self) -> bool {
        self.0 & TAG_MASK == TAG_IMMEDIATE
    }

    /// Is this `nil`?
    ///
    /// Every test for the empty list and for falsity goes through here, so that
    /// how `nil` is represented stays a detail of this module. See the note on
    /// `nil` and symbols in `docs/design.md`.
    pub const fn is_nil(self) -> bool {
        self.0 == Value::NIL.0
    }

    /// Is this anything other than `nil`? Lisp's notion of truth.
    pub const fn is_true(self) -> bool {
        self.0 != Value::NIL.0
    }

    /// Make a primitive referring to entry `index` of the primitive table.
    pub(crate) const fn primitive(index: u32) -> Value<'h> {
        Value(
            ((index as u64) << IMM_PAYLOAD_SHIFT) | SUBTAG_PRIMITIVE | TAG_IMMEDIATE,
            PhantomData,
        )
    }

    /// Which primitive this is, or `None` if it is not one.
    pub fn as_primitive(self) -> Option<u32> {
        if self.is_immediate() && (self.0 & SUBTAG_MASK) == SUBTAG_PRIMITIVE {
            Some((self.0 >> IMM_PAYLOAD_SHIFT) as u32)
        } else {
            None
        }
    }

    /// Is this word an object header rather than a value?
    ///
    /// Only the heap and the collector ask. Headers are never handed to Lisp,
    /// and no value can answer yes — see [`SUBTAG_HEADER`].
    pub(crate) const fn is_header(self) -> bool {
        self.is_immediate() && (self.0 & SUBTAG_MASK) == SUBTAG_HEADER
    }

    /// Is this a character?
    pub const fn is_character(self) -> bool {
        self.is_immediate() && (self.0 & SUBTAG_MASK) == SUBTAG_CHARACTER
    }

    /// Make a character.
    pub const fn character(c: char) -> Value<'h> {
        Value(
            ((c as u64) << IMM_PAYLOAD_SHIFT) | SUBTAG_CHARACTER | TAG_IMMEDIATE,
            PhantomData,
        )
    }

    /// The character in this value, or `None` if it is not one.
    pub fn as_character(self) -> Option<char> {
        if self.is_character() {
            char::from_u32((self.0 >> IMM_PAYLOAD_SHIFT) as u32)
        } else {
            None
        }
    }

    // ---- fixnum arithmetic -----------------------------------------------
    //
    // The fast path only. `None` means either "not a fixnum" or "the result
    // left fixnum range"; the numeric tower above decides which and promotes to
    // a bignum if it must.

    /// Add two fixnums. `None` if either is not a fixnum, or on overflow.
    ///
    /// No untagging: the tagged words are added directly, because doubling
    /// distributes over addition. `checked_add` catches exactly the cases that
    /// leave fixnum range.
    pub fn checked_add(self, other: Value<'h>) -> Option<Value<'h>> {
        if !self.is_fixnum() || !other.is_fixnum() {
            return None;
        }
        let sum = (self.0 as i64).checked_add(other.0 as i64)?;
        Some(Value(sum as u64, PhantomData))
    }

    /// Subtract two fixnums. `None` if either is not a fixnum, or on overflow.
    pub fn checked_sub(self, other: Value<'h>) -> Option<Value<'h>> {
        if !self.is_fixnum() || !other.is_fixnum() {
            return None;
        }
        let diff = (self.0 as i64).checked_sub(other.0 as i64)?;
        Some(Value(diff as u64, PhantomData))
    }

    /// Multiply two fixnums. `None` if either is not a fixnum, or on overflow.
    ///
    /// Multiplication is the one case that needs a half-untag: `2a * 2b` is
    /// `4ab`, twice what it should be, so exactly one operand is shifted down
    /// first.
    pub fn checked_mul(self, other: Value<'h>) -> Option<Value<'h>> {
        let a = self.as_fixnum()?;
        if !other.is_fixnum() {
            return None;
        }
        let product = (other.0 as i64).checked_mul(a)?;
        Some(Value(product as u64, PhantomData))
    }

    /// Negate a fixnum. `None` if not a fixnum, or if the result is out of range
    /// (which happens only for [`MIN_FIXNUM`]).
    pub fn checked_neg(self) -> Option<Value<'h>> {
        if !self.is_fixnum() {
            return None;
        }
        let negated = (self.0 as i64).checked_neg()?;
        Some(Value(negated as u64, PhantomData))
    }
}

impl fmt::Debug for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            Kind::Fixnum => write!(f, "{}", (self.0 as i64) >> 1),
            Kind::Cons => write!(f, "#<cons @{}>", self.0 >> TAG_BITS),
            // `nil` and `t` are heap objects now, and this impl has no heap to
            // look their names up in — but it does not need one, because their
            // indices are constants. A developer convenience, not the Lisp
            // printer: `crate::printer` goes through `symbol-name` like any
            // other symbol, which is the whole point of the change.
            Kind::Object if *self == Value::NIL => write!(f, "nil"),
            Kind::Object if *self == Value::T => write!(f, "t"),
            Kind::Object => write!(f, "#<object @{}>", self.0 >> TAG_BITS),
            Kind::Forwarding => write!(f, "#<forwarded to {}>", self.0 >> TAG_BITS),
            Kind::Immediate => {
                if *self == Value::UNBOUND {
                    write!(f, "#<unbound>")
                } else if let Some(c) = self.as_character() {
                    write!(f, "#\\{c}")
                } else {
                    write!(f, "#<immediate {:#x}>", self.0)
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "unit/value.rs"]
mod tests;
