//! The heap, and the rule that keeps it safe.
//!
//! The heap is an array of 64-bit words the VM owns. Objects are runs of
//! adjacent words; a cons cell is two. A [`Value`] referring into it holds a
//! word index, not an address, so an image relocates for free.
//!
//! # The problem this module solves
//!
//! The collector decides what is alive by tracing from its *roots* — locations
//! outside the heap holding values. It cannot see Rust local variables. A value
//! held in a Rust local across an allocation is therefore not stale, it is
//! **dead**: the collector concluded nobody wanted it and reclaimed it.
//!
//! ```text
//! let tail = heap.alloc_cons(..);      // offset 0x40, say
//! let head = heap.alloc_cons(.., tail); // may collect; `tail` now points at nothing
//! ```
//!
//! Because our references are offsets into a `Vec<u64>`, that is not a
//! memory-safety bug. There is no `unsafe`, no segfault, no sanitiser report —
//! you index a valid vector and get a valid word that means something else. A
//! **wrong answer, not a crash**, noticed several evaluations later.
//!
//! # The rule
//!
//! Work on the heap happens inside [`Heap::mutate`], and **collection cannot
//! happen inside a mutation**. Values obtained during one cannot leave it. So
//! at every point where the collector may run, no Rust local holds a value, and
//! the problem above is not merely avoided — it is unrepresentable.
//!
//! ```
//! # use quokka_vm::heap::Heap;
//! # use quokka_vm::value::Value;
//! let mut heap = Heap::new();
//! heap.mutate(|mc| {
//!     let tail = mc.cons(Value::fixnum(2).unwrap(), Value::NIL);
//!     let head = mc.cons(Value::fixnum(1).unwrap(), tail);  // holding `tail`: fine
//!     mc.set_root(head);
//! });
//! heap.collect();   // legal here, and only here
//! ```
//!
//! This costs no `unsafe` and no runtime check. It is thirteen lines of types.
//!
//! # How it works
//!
//! [`Heap::mutate`] takes `F: for<'h> FnOnce(&mut Mutation<'h>) -> R`. The
//! higher-ranked bound means the closure must work for *every* brand `'h`, and
//! `R` is fixed outside the closure, so `R` cannot mention `'h`. A
//! `Value<'h>` therefore has nowhere to go: not out through the return value,
//! and not into anything captured from outside, whose lifetimes are already
//! fixed. [`crate::value::Invariant`] explains why the brand must be invariant.
//!
//! Collection takes `&mut self` on the `Heap`, which a live `Mutation` borrows,
//! so the compiler refuses a collection during a mutation.
//!
//! # What it does not do
//!
//! It stops *callers* losing values. It does nothing about bugs inside the
//! collector — tracing the wrong field, mis-sizing a header, botching the
//! forwarding dance. Those produce the same silent wrong answers, and the
//! torture mode is what catches them.
//!
//! # What it costs
//!
//! Collection can only happen between mutations, so the heap must absorb
//! whatever one mutation allocates. That is cheap here because rule 4 already
//! forbids long mutations: the evaluator must return control to the browser
//! promptly, so one step per mutation gives fine-grained collection for free.

use crate::value::{Invariant, Value, SUBTAG_HEADER};
use core::fmt;
use core::marker::PhantomData;

/// Is the torture mode on? See [`Heap::collect`] and [`Heap::verify`].
pub const TORTURE: bool = cfg!(feature = "gc-torture");

/// Fills space that must never be read. Not a valid anything.
const POISON: u64 = 0xDEAD_BEEF_DEAD_BEEF;

/// Copy one word's referent into to-space if it has not been copied already,
/// and return the word updated to point at the copy.
///
/// Leaves a **forwarding pointer** behind in from-space: the object's first
/// word is overwritten with a reference tagged `111`, so a second reference to
/// the same object finds the copy instead of making another. That is what keeps
/// shared structure shared and stops cycles looping forever — and it is safe to
/// scribble on from-space precisely because it is about to be abandoned.
fn forward(from: &mut [u64], to: &mut Vec<u64>, static_end: usize, word: u64) -> u64 {
    let value = Value::from_bits(word);
    let Some(at) = value.word_index() else {
        return word; // a fixnum or a character: refers to nothing
    };

    // The static area does not move, so a reference into it is already correct
    // and must not be forwarded. This is the four lines that let `Value::NIL`
    // be a compile-time constant while `nil` is a real symbol.
    if at < static_end {
        return word;
    }

    // Already copied? Then from[at] is where it went.
    let first = Value::from_bits(from[at]);
    if first.is_forwarding() {
        return retag(value, first.word_index().expect("forwarding pointer"));
    }

    // Not yet. Work out how big it is, the same way the scan does.
    let size = match Header::decode(from[at]) {
        Some(header) => HEADER_WORDS + header.slots,
        None => CONS_WORDS,
    };

    let moved_to = to.len();
    to.extend_from_slice(&from[at..at + size]);
    from[at] = Value::forwarding(moved_to)
        .expect("heap index exceeds the 61-bit encoding")
        .bits();

    retag(value, moved_to)
}

/// Rebuild a reference at a new index, keeping whether it was a cons or an
/// object. The forwarding pointer records where, not what.
fn retag(original: Value<'_>, index: usize) -> u64 {
    let moved = if original.is_cons() {
        Value::cons(index)
    } else {
        Value::object(index)
    };
    moved
        .expect("heap index exceeds the 61-bit encoding")
        .bits()
}

/// How many words a cons cell occupies.
pub const CONS_WORDS: usize = 2;

/// How many words an object's header occupies.
pub const HEADER_WORDS: usize = 1;

/// A symbol's slots, in order.
///
/// Common Lisp gives a symbol a name, a value, a function and a property list;
/// these are those, in that order. The value and function cells hold
/// [`Value::UNBOUND`] until something binds them — reading an unbound variable
/// must signal rather than quietly return `nil`.
pub mod symbol {
    /// The symbol's name, a string object.
    pub const NAME: usize = 0;
    /// Its value as a variable.
    pub const VALUE: usize = 1;
    /// Its value as a function. Lisp-2: a symbol can be both at once.
    pub const FUNCTION: usize = 2;
    /// Its property list.
    pub const PLIST: usize = 3;
    /// Non-nil if this names a special operator, as a small integer the
    /// evaluator dispatches on.
    ///
    /// On the symbol rather than in a lookup table because that is what it is:
    /// `if` is special because the symbol `IF` is, and a slot read beats
    /// comparing names on every form evaluated.
    pub const SPECIAL: usize = 4;
    /// How many slots a symbol has.
    pub const SLOTS: usize = 5;
}

/// The static area: objects the collector never moves.
///
/// `nil` and `t` must be symbols — Common Lisp says so, and `(symbolp nil)` is
/// true in every conforming implementation. They must also be *constants* in
/// Rust, because `Value::NIL` appears at eighty-odd sites and `is_nil` at
/// twenty, none of which can afford to consult the heap.
///
/// Those two requirements look incompatible: a heap object moves when the
/// collector runs, so its index cannot be a constant. The way out is to have a
/// region that does not move. SBCL does this with a static space at a fixed
/// address; here it is simply the first words of the heap, which `collect`
/// copies verbatim and never relocates.
///
/// The static objects' *contents* are still traced — a property list set on
/// `nil` may point into the collected area — so they are roots, not islands.
///
/// **These indices are part of the image format.** A VM loading an image must
/// agree about where `nil` lives. Appending is a format addition; reordering
/// is a break. Same rule as the primitive table.
pub mod statics {
    use super::{symbol, HEADER_WORDS};

    /// Where the symbol `nil` lives. First, so the constant is zero.
    pub const NIL_INDEX: usize = 0;
    /// Where the symbol `t` lives: immediately after `nil`.
    pub const T_INDEX: usize = NIL_INDEX + HEADER_WORDS + symbol::SLOTS;
}

/// Default soft limit: 1 MiB of heap before a collection is wanted.
pub const DEFAULT_SOFT_LIMIT_WORDS: usize = 128 * 1024;

/// How far past the soft limit the heap may grow before allocation stops.
///
/// The soft limit is a request the driver answers between mutations. This is
/// the backstop for when it cannot be answered — a single mutation that
/// allocates without end, which collection cannot help with because everything
/// it allocated is still reachable.
///
/// Sixty-four times is generous enough that no reasonable program meets it and
/// small enough that an unreasonable one stops in under a second rather than
/// after gigabytes.
pub const HARD_LIMIT_MULTIPLE: usize = 64;

// An object header, which is not a value — see `value::SUBTAG_HEADER`.
//
//   [ length : 48 ][ type : 8 ][ subtag 31 : 5 ][ 101 ]
const HEADER_TYPE_SHIFT: u32 = 8;
const HEADER_LEN_SHIFT: u32 = 16;
const HEADER_TYPE_MASK: u64 = 0xFF << HEADER_TYPE_SHIFT;

/// The largest number of slots one object can have.
///
/// The header field is 48 bits wide, which no 64-bit heap will reach. On a
/// 32-bit host — wasm32 — `usize` runs out first, so the limit is whichever
/// binds. Writing the field width as a bare `usize` would be a compile error
/// there, and worse, would silently mean different things on different targets.
pub const MAX_OBJECT_SLOTS: usize = {
    let field = u64::MAX >> HEADER_LEN_SHIFT;
    if field > usize::MAX as u64 {
        usize::MAX
    } else {
        field as usize
    }
};

/// What a heap object is.
///
/// The header carries this so the collector knows two things: how to print or
/// dispatch on the object, and — load-bearing — **whether its slots are values
/// that must be traced**. A string's words are text and a code object's are
/// bytecode; tracing those would follow numbers as if they were references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ObjectType {
    /// Name, value cell, function cell, property list, package.
    Symbol = 1,
    /// A general vector. Slots are elements.
    Vector = 2,
    /// Code, plus the captured environment.
    Closure = 3,
    /// An instance of a class. Slots are its fields.
    Instance = 4,
    /// Buckets and metadata.
    HashTable = 5,
    /// One lexical scope: a parent link, then name/value pairs.
    ///
    /// A chain of these rather than an association list. A two-argument call
    /// costs one object of six words instead of four cons cells of eight, and
    /// — the point — this is the layout lexical addressing uses, so when the
    /// compiler arrives it replaces the name scan with an index and the data
    /// structure does not change. See `crate::eval::env`.
    Environment = 7,
    /// One pending step of the evaluator: what to do once a value arrives.
    ///
    /// Slot 0 is which kind, as a fixnum; the rest depend on it. One object
    /// type rather than one per kind, so adding a special operator does not
    /// mean adding a variant here. See `crate::eval`.
    Frame = 6,
    /// Characters, packed. **Not traced.**
    String = 64,
    /// Bytecode. **Not traced.**
    Code = 65,
    /// Magnitude limbs. **Not traced.**
    Bignum = 66,
}

impl ObjectType {
    /// Are this object's slots values the collector must trace?
    ///
    /// The discriminants are chosen so this is a comparison rather than a
    /// table: traced types are below 64, raw ones at or above it. Adding a type
    /// on the wrong side of that line is a silent heap-corrupting bug, which is
    /// why `raw_types_are_not_traced` exists.
    pub const fn slots_are_values(self) -> bool {
        (self as u8) < 64
    }

    fn from_u8(byte: u8) -> Option<ObjectType> {
        use ObjectType::*;
        Some(match byte {
            1 => Symbol,
            2 => Vector,
            3 => Closure,
            4 => Instance,
            5 => HashTable,
            6 => Frame,
            7 => Environment,
            64 => String,
            65 => Code,
            66 => Bignum,
            _ => return None,
        })
    }
}

/// Something the collector got wrong, as found by [`Heap::verify`].
///
/// `from` is the word index of the object holding the bad reference, or
/// `usize::MAX` for the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inconsistency {
    /// An object claims to extend past the end of the heap. Its header's length
    /// is wrong, or a linear walk lost the object boundaries.
    ObjectOverrunsHeap {
        /// Where the object begins.
        at: usize,
        /// How many words its header claims.
        size: usize,
    },
    /// A reference points outside the heap.
    ReferenceOutOfBounds {
        /// The object holding it.
        from: usize,
        /// Where it points.
        target: usize,
    },
    /// A reference points inside the heap but not at the start of an object —
    /// typically a slot the collector failed to update, now read against the
    /// new layout.
    ReferenceIntoNowhere {
        /// The object holding it.
        from: usize,
        /// Where it points.
        target: usize,
    },
    /// A forwarding pointer survived into the live heap. They belong only in
    /// the abandoned semispace, during a collection.
    ForwardingPointerLeft {
        /// The object holding it.
        from: usize,
    },
}

impl fmt::Display for Inconsistency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inconsistency::ObjectOverrunsHeap { at, size } => {
                write!(f, "object at {at} claims {size} words and runs off the end")
            }
            Inconsistency::ReferenceOutOfBounds { from, target } => {
                write!(f, "object at {from} refers to {target}, outside the heap")
            }
            Inconsistency::ReferenceIntoNowhere { from, target } => {
                write!(
                    f,
                    "object at {from} refers to {target}, not an object start"
                )
            }
            Inconsistency::ForwardingPointerLeft { from } => {
                write!(f, "object at {from} still holds a forwarding pointer")
            }
        }
    }
}

/// Which of the evaluator's four registers.
///
/// Named rather than numbered because every one of them is a root, and a
/// mistyped index would be a silent corruption rather than a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    /// The form being worked on.
    Control,
    /// Where names are looked up.
    Env,
    /// The result the last step produced.
    Value,
    /// Frames: what to do once that result arrives.
    Stack,
    /// What an unwind is searching for, or `nil`.
    Unwind,
}

/// An object header word, as stored in the heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// What kind of object this is.
    pub object_type: ObjectType,
    /// How many slots follow the header.
    pub slots: usize,
}

impl Header {
    fn encode(self) -> u64 {
        ((self.slots as u64) << HEADER_LEN_SHIFT)
            | ((self.object_type as u64) << HEADER_TYPE_SHIFT)
            | SUBTAG_HEADER
            | 0b101
    }

    fn decode(word: u64) -> Option<Header> {
        if !Value::from_bits(word).is_header() {
            return None;
        }
        Some(Header {
            object_type: ObjectType::from_u8(
                ((word & HEADER_TYPE_MASK) >> HEADER_TYPE_SHIFT) as u8,
            )?,
            slots: (word >> HEADER_LEN_SHIFT) as usize,
        })
    }
}

/// The heap's storage. Separated from [`Heap`] so a [`Mutation`] can borrow it
/// without borrowing the collection machinery.
#[derive(Debug, Default)]
struct Storage {
    /// The live semispace. Every `Value`'s word index points in here.
    words: Vec<u64>,
    /// The other semispace, empty between collections. Kept rather than
    /// reallocated so a collection does not also mean a large allocation.
    spare: Vec<u64>,
    /// The one persistent root: everything that must survive hangs off it.
    root: u64,
    /// Every interned symbol, as a list. A second root, and the collector must
    /// forward it — a root nobody forwards is a live object silently reclaimed.
    obarray: u64,
    /// The evaluator's registers. Roots, all four.
    ///
    /// They live here rather than in a Rust struct for two reasons: the
    /// collector has to forward them, and an image has to be able to save a
    /// computation half-finished. See `crate::eval`.
    control: u64,
    env: u64,
    value: u64,
    stack: u64,
    /// What an unwind is looking for: a `block`'s marker, a `catch`'s tag, or
    /// a `tagbody`'s marker. `nil` when not unwinding. A root like the rest.
    unwind: u64,
    /// Which half of the evaluator's cycle it is in. Not a `Value` — it is VM
    /// state rather than Lisp data, so the collector does not touch it.
    ///
    /// It does need to be here rather than in a Rust local, so that a slice can
    /// end between any two steps and resume correctly.
    mode: u8,
    /// Above this, the heap would like to be collected. Not a hard limit —
    /// see [`Heap::wants_collection`].
    soft_limit: usize,
    /// Above this, allocation stops being possible. See [`Heap::is_exhausted`].
    hard_limit: usize,
    /// Set when an allocation passed `hard_limit`. The evaluator checks it and
    /// halts; nothing else needs to know.
    exhausted: bool,
    /// How many collections have happened. Torture mode uses it to vary the
    /// padding, so no object can land where it was.
    collections: u64,
    /// Where the static area ends. Objects below this are never moved.
    static_end: usize,
    /// Where collected objects begin: `static_end`, plus torture padding.
    dynamic_start: usize,
}

impl Storage {
    fn alloc(&mut self, words: usize) -> usize {
        let at = self.words.len();
        // Measured against the collected part: the static area is permanent,
        // so counting it toward a limit that only collection can relieve would
        // make a small limit unreachable from the start.
        if at + words - self.dynamic_start > self.hard_limit {
            // The allocation still happens. Refusing it would mean every
            // allocator returning a Result, and every caller handling a case
            // that cannot be recovered from locally. Instead the flag is set
            // and the evaluator halts at its next step, so the overshoot is
            // one step's worth of words — a handful.
            self.exhausted = true;
        }
        self.words.resize(at + words, 0);
        at
    }
}

/// Access to the heap for the duration of one mutation.
///
/// Holding this proves collection cannot happen, which is what makes holding a
/// [`Value`] safe. It is the only way to obtain a value that refers into the
/// heap.
pub struct Mutation<'h> {
    storage: &'h mut Storage,
    _brand: Invariant<'h>,
}

impl<'h> Mutation<'h> {
    /// Allocate a cons cell.
    pub fn cons(&mut self, car: Value<'h>, cdr: Value<'h>) -> Value<'h> {
        let index = self.storage.words.len();
        self.storage.words.push(car.bits());
        self.storage.words.push(cdr.bits());
        Value::cons(index).expect("heap index exceeds the 61-bit encoding")
    }

    /// The car of a cons cell.
    ///
    /// # Panics
    /// If `v` is not a cons.
    pub fn car(&self, v: Value<'h>) -> Value<'h> {
        Value::from_bits(self.storage.words[self.cell(v)])
    }

    /// The cdr of a cons cell.
    ///
    /// # Panics
    /// If `v` is not a cons.
    pub fn cdr(&self, v: Value<'h>) -> Value<'h> {
        Value::from_bits(self.storage.words[self.cell(v) + 1])
    }

    /// Replace the car of a cons cell.
    pub fn set_car(&mut self, v: Value<'h>, new: Value<'h>) {
        let at = self.cell(v);
        self.storage.words[at] = new.bits();
    }

    /// Replace the cdr of a cons cell.
    pub fn set_cdr(&mut self, v: Value<'h>, new: Value<'h>) {
        let at = self.cell(v);
        self.storage.words[at + 1] = new.bits();
    }

    /// Allocate a heap object with `slots` slots, all initialised to `nil`.
    ///
    /// Slots are initialised rather than left as whatever was there, because
    /// the mutation may end before they are all filled, and the next collection
    /// would then trace garbage. One `resize` is cheaper than that class of bug.
    ///
    /// # Panics
    /// If `slots` exceeds [`MAX_OBJECT_SLOTS`].
    pub fn alloc(&mut self, object_type: ObjectType, slots: usize) -> Value<'h> {
        assert!(
            slots <= MAX_OBJECT_SLOTS,
            "object of {slots} slots exceeds the {MAX_OBJECT_SLOTS}-slot header field"
        );
        let at = self.storage.alloc(HEADER_WORDS + slots);
        self.storage.words[at] = Header { object_type, slots }.encode();
        for slot in 0..slots {
            self.storage.words[at + HEADER_WORDS + slot] = Value::NIL.bits();
        }
        Value::object(at).expect("heap index exceeds the 61-bit encoding")
    }

    /// The header of a heap object.
    ///
    /// # Panics
    /// If `v` is not a heap object.
    pub fn header(&self, v: Value<'h>) -> Header {
        let at = self.cell(v);
        Header::decode(self.storage.words[at]).expect("object has no valid header")
    }

    /// Read slot `index` of a heap object.
    ///
    /// # Panics
    /// If `v` is not a heap object, if `index` is out of range, or if this
    /// object's slots are raw words rather than values — use [`Self::raw_slot`]
    /// for those.
    pub fn slot(&self, v: Value<'h>, index: usize) -> Value<'h> {
        Value::from_bits(self.storage.words[self.slot_at(v, index, true)])
    }

    /// Write slot `index` of a heap object.
    ///
    /// # Panics
    /// As [`Self::slot`].
    pub fn set_slot(&mut self, v: Value<'h>, index: usize, new: Value<'h>) {
        let at = self.slot_at(v, index, true);
        self.storage.words[at] = new.bits();
    }

    /// Read slot `index` of an object whose slots are raw words — a string's
    /// characters, a code object's bytecode, a bignum's limbs.
    ///
    /// # Panics
    /// If `v` is not a heap object, if `index` is out of range, or if this
    /// object's slots *are* values.
    pub fn raw_slot(&self, v: Value<'h>, index: usize) -> u64 {
        self.storage.words[self.slot_at(v, index, false)]
    }

    /// Write a raw slot.
    ///
    /// # Panics
    /// As [`Self::raw_slot`].
    pub fn set_raw_slot(&mut self, v: Value<'h>, index: usize, word: u64) {
        let at = self.slot_at(v, index, false);
        self.storage.words[at] = word;
    }

    /// Allocate a string.
    ///
    /// One Unicode scalar per word. That wastes most of each word, and it is
    /// deliberate for now: it makes indexing a character O(1) and the code
    /// obvious. Packing comes when something measures that it matters.
    pub fn alloc_string(&mut self, text: &str) -> Value<'h> {
        let chars: Vec<char> = text.chars().collect();
        let v = self.alloc(ObjectType::String, chars.len());
        for (index, ch) in chars.iter().enumerate() {
            self.set_raw_slot(v, index, *ch as u64);
        }
        v
    }

    /// Read a string back out.
    ///
    /// # Panics
    /// If `v` is not a string.
    pub fn string_to_rust(&self, v: Value<'h>) -> String {
        assert_eq!(
            self.header(v).object_type,
            ObjectType::String,
            "not a string"
        );
        (0..self.header(v).slots)
            .map(|i| char::from_u32(self.raw_slot(v, i) as u32).expect("invalid character"))
            .collect()
    }

    /// Does this string have exactly these characters?
    ///
    /// Compares without allocating, which matters because interning calls it
    /// once per symbol already known.
    pub fn string_is(&self, v: Value<'h>, text: &str) -> bool {
        if !v.is_object() || self.header(v).object_type != ObjectType::String {
            return false;
        }
        let slots = self.header(v).slots;
        let mut chars = text.chars();
        for i in 0..slots {
            match chars.next() {
                Some(ch) if ch as u64 == self.raw_slot(v, i) => {}
                _ => return false,
            }
        }
        chars.next().is_none()
    }

    /// Find or create the symbol with this name.
    ///
    /// Two reads of `foo` must produce the *same* object, or `eq` on symbols
    /// stops working and every lookup becomes a string comparison. That is what
    /// interning means, and the table it uses is the **obarray**.
    ///
    /// The obarray here is a list, so this is O(n) in the number of symbols.
    /// A hash table belongs here eventually; a list is correct, obvious, and
    /// replaceable without anything outside noticing.
    ///
    /// `nil` and `t` need no special case: they are ordinary symbols living in
    /// the static area, interned at bootstrap like any other.
    pub fn intern(&mut self, name: &str) -> Value<'h> {
        let mut cursor = Value::from_bits(self.storage.obarray);
        while !cursor.is_nil() {
            let symbol = self.car(cursor);
            if self.string_is(self.slot(symbol, symbol::NAME), name) {
                return symbol;
            }
            cursor = self.cdr(cursor);
        }

        let text = self.alloc_string(name);
        let symbol = self.alloc(ObjectType::Symbol, symbol::SLOTS);
        self.set_slot(symbol, symbol::NAME, text);
        self.set_slot(symbol, symbol::VALUE, Value::UNBOUND);
        self.set_slot(symbol, symbol::FUNCTION, Value::UNBOUND);
        let entry = self.cons(symbol, Value::from_bits(self.storage.obarray));
        self.storage.obarray = entry.bits();
        symbol
    }

    /// Is this a symbol object?
    ///
    /// `nil` and `t` answer yes, as Common Lisp requires, because they are
    /// symbols — the header says so, and nothing here has to know their names.
    pub fn is_symbol(&self, v: Value<'h>) -> bool {
        v.is_object() && self.header(v).object_type == ObjectType::Symbol
    }

    /// Is this the symbol with exactly this name?
    ///
    /// Allocates nothing, unlike comparing [`Self::symbol_name`].
    pub fn symbol_is(&self, v: Value<'h>, name: &str) -> bool {
        self.is_symbol(v) && self.string_is(self.slot(v, symbol::NAME), name)
    }

    /// A symbol's name as a Rust string.
    ///
    /// # Panics
    /// If `v` is not a symbol.
    pub fn symbol_name(&self, v: Value<'h>) -> String {
        assert!(self.is_symbol(v), "not a symbol");
        self.string_to_rust(self.slot(v, symbol::NAME))
    }

    /// Read an evaluator register.
    pub fn register(&self, which: Register) -> Value<'h> {
        Value::from_bits(match which {
            Register::Control => self.storage.control,
            Register::Env => self.storage.env,
            Register::Value => self.storage.value,
            Register::Stack => self.storage.stack,
            Register::Unwind => self.storage.unwind,
        })
    }

    /// Write an evaluator register.
    pub fn set_register(&mut self, which: Register, v: Value<'h>) {
        let slot = match which {
            Register::Control => &mut self.storage.control,
            Register::Env => &mut self.storage.env,
            Register::Value => &mut self.storage.value,
            Register::Stack => &mut self.storage.stack,
            Register::Unwind => &mut self.storage.unwind,
        };
        *slot = v.bits();
    }

    /// The evaluator's half-cycle marker. Meaning is `crate::eval`'s business.
    pub fn mode(&self) -> u8 {
        self.storage.mode
    }

    /// Set the half-cycle marker.
    pub fn set_mode(&mut self, mode: u8) {
        self.storage.mode = mode;
    }

    /// Every interned symbol, as a list. A root.
    pub fn obarray(&self) -> Value<'h> {
        Value::from_bits(self.storage.obarray)
    }

    /// Replace the obarray. For bootstrap; `intern` manages it afterwards.
    pub fn set_obarray(&mut self, v: Value<'h>) {
        self.storage.obarray = v.bits();
    }

    /// The persistent root.
    pub fn root(&self) -> Value<'h> {
        Value::from_bits(self.storage.root)
    }

    /// Set the persistent root. Anything not reachable from it is garbage.
    pub fn set_root(&mut self, v: Value<'h>) {
        self.storage.root = v.bits();
    }

    /// How many words the collected part of the heap occupies.
    ///
    /// Excludes the static area and any torture padding, so the number means
    /// the same thing however the heap was built.
    pub fn used_words(&self) -> usize {
        self.storage.words.len() - self.storage.dynamic_start
    }

    /// Has an allocation passed the hard limit? See [`Heap::is_exhausted`].
    pub fn is_exhausted(&self) -> bool {
        self.storage.exhausted
    }

    fn cell(&self, v: Value<'h>) -> usize {
        v.word_index().expect("not a reference into the heap")
    }

    /// Locate a slot, checking the bound and that the caller is using the right
    /// accessor for this object's kind.
    ///
    /// Mixing them up is how a collector ends up tracing the bytes of a string
    /// as if they were references, so it is a panic rather than a shrug.
    fn slot_at(&self, v: Value<'h>, index: usize, want_values: bool) -> usize {
        let at = self.cell(v);
        let header = Header::decode(self.storage.words[at]).expect("object has no valid header");
        assert!(
            index < header.slots,
            "slot {index} is out of range for a {:?} of {} slots",
            header.object_type,
            header.slots
        );
        assert_eq!(
            header.object_type.slots_are_values(),
            want_values,
            "{:?} slots are {}; use the other accessor",
            header.object_type,
            if want_values { "raw words" } else { "values" }
        );
        at + HEADER_WORDS + index
    }
}

/// The heap.
///
/// Work on it through [`Heap::mutate`]; collect between mutations.
#[derive(Debug)]
pub struct Heap {
    storage: Storage,
}

impl Default for Heap {
    /// Not derived: a heap without its static area has no `nil`, and every
    /// constant in `crate::value` would name an object that is not there.
    fn default() -> Heap {
        Heap::new()
    }
}

impl Heap {
    /// A new, empty heap whose root is `nil`, with a default soft limit.
    pub fn new() -> Heap {
        Heap::with_soft_limit(DEFAULT_SOFT_LIMIT_WORDS)
    }

    /// A new heap that would like to be collected once it passes `words`.
    pub fn with_soft_limit(words: usize) -> Heap {
        let mut heap = Heap {
            storage: Storage {
                words: Vec::new(),
                spare: Vec::new(),
                root: Value::NIL.bits(),
                obarray: Value::NIL.bits(),
                control: Value::NIL.bits(),
                env: Value::NIL.bits(),
                value: Value::NIL.bits(),
                stack: Value::NIL.bits(),
                unwind: Value::NIL.bits(),
                mode: 0,
                soft_limit: words,
                hard_limit: words.saturating_mul(HARD_LIMIT_MULTIPLE),
                exhausted: false,
                collections: 0,
                static_end: 0,
                dynamic_start: 0,
            },
        };
        heap.build_static_area();
        heap
    }

    /// Lay out `nil` and `t` as real symbols, before anything else exists.
    ///
    /// Order matters and is asserted rather than assumed: `Value::NIL` and
    /// `Value::T` are compile-time constants naming indices, so if these land
    /// anywhere else every `nil` in the system is a different object.
    fn build_static_area(&mut self) {
        self.mutate(|mc| {
            let nil = mc.alloc(ObjectType::Symbol, symbol::SLOTS);
            assert_eq!(nil, Value::NIL, "nil must be the first static object");
            let t = mc.alloc(ObjectType::Symbol, symbol::SLOTS);
            assert_eq!(t, Value::T, "t must follow nil in the static area");

            // The names live in the static area too, so nothing here needs to
            // point into collected space on an empty heap.
            let nil_name = mc.alloc_string("NIL");
            let t_name = mc.alloc_string("T");

            for (s, name) in [(nil, nil_name), (t, t_name)] {
                mc.set_slot(s, symbol::NAME, name);
                mc.set_slot(s, symbol::FUNCTION, Value::UNBOUND);
                mc.set_slot(s, symbol::PLIST, Value::NIL);
                mc.set_slot(s, symbol::SPECIAL, Value::NIL);
            }
            // Both are constants that evaluate to themselves.
            mc.set_slot(nil, symbol::VALUE, Value::NIL);
            mc.set_slot(t, symbol::VALUE, Value::T);
        });
        // Interned before the boundary is fixed, so the obarray's first two
        // cells are static as well. They are permanent either way, and this
        // keeps a fresh heap at zero used words.
        self.mutate(|mc| {
            for s in [Value::NIL, Value::T] {
                let obarray = mc.obarray();
                let entry = mc.cons(s, obarray);
                mc.set_obarray(entry);
            }
        });

        self.storage.static_end = self.storage.words.len();
        self.storage.dynamic_start = self.storage.static_end;
    }

    /// Has an allocation passed the hard limit?
    ///
    /// The evaluator checks this each step and stops. Unlike
    /// [`Heap::wants_collection`] this is not a request: past here the heap
    /// will not grow further, and continuing would mean growing until the
    /// operating system intervenes — or, on wasm32, until linear memory runs
    /// out at 4 GiB and takes the tab with it.
    pub fn is_exhausted(&self) -> bool {
        self.storage.exhausted
    }

    /// Forget an exhaustion, so the REPL can carry on after reporting it.
    ///
    /// Only safe once whatever was allocating has stopped and a collection has
    /// reclaimed it; the REPL does both.
    pub fn clear_exhausted(&mut self) {
        self.storage.exhausted = false;
    }

    /// The hard ceiling, in words.
    pub fn hard_limit(&self) -> usize {
        self.storage.hard_limit
    }

    /// Is the static area laid out?
    ///
    /// False only while `build_static_area` is running. Nothing but the
    /// torture collection needs to ask.
    fn is_bootstrapped(&self) -> bool {
        self.storage.static_end > 0
    }

    /// Has the heap grown past its soft limit?
    ///
    /// The driver checks this **between** mutations and collects if it says
    /// yes. It is a request, not a limit: allocation inside a mutation can
    /// never fail for want of space, because collection cannot happen there, so
    /// a mutation that allocates heavily simply grows the heap past this line.
    /// That is the price of the rule in the module docs, and it is affordable
    /// because rule 4 already keeps mutations short.
    ///
    /// In the browser the ceiling is real: wasm32 linear memory stops at 4 GiB,
    /// and less in practice.
    pub fn wants_collection(&self) -> bool {
        self.used_words() > self.storage.soft_limit
    }

    /// The soft limit this heap was built with.
    pub fn soft_limit(&self) -> usize {
        self.storage.soft_limit
    }

    /// Work on the heap. Collection cannot happen inside `f`, and no value
    /// obtained inside it can escape — see the module docs.
    ///
    /// Trying to carry a value out does not compile:
    ///
    /// ```compile_fail
    /// # use quokka_vm::heap::Heap;
    /// # use quokka_vm::value::Value;
    /// let mut heap = Heap::new();
    /// let escaped = heap.mutate(|mc| mc.cons(Value::NIL, Value::NIL));
    /// ```
    ///
    /// Nor does stashing one in something from outside:
    ///
    /// ```compile_fail
    /// # use quokka_vm::heap::Heap;
    /// # use quokka_vm::value::Value;
    /// let mut heap = Heap::new();
    /// let mut stolen = Vec::new();
    /// heap.mutate(|mc| stolen.push(mc.cons(Value::NIL, Value::NIL)));
    /// ```
    ///
    /// Plain data may leave, because it refers to nothing:
    ///
    /// ```
    /// # use quokka_vm::heap::Heap;
    /// # use quokka_vm::value::Value;
    /// let mut heap = Heap::new();
    /// let words = heap.mutate(|mc| {
    ///     mc.cons(Value::NIL, Value::NIL);
    ///     mc.used_words()
    /// });
    /// assert_eq!(words, 2);
    /// ```
    pub fn mutate<F, R>(&mut self, f: F) -> R
    where
        F: for<'h> FnOnce(&mut Mutation<'h>) -> R,
    {
        let mut mutation = Mutation {
            storage: &mut self.storage,
            _brand: PhantomData,
        };
        let result = f(&mut mutation);

        // Torture mode: collect at every opportunity there is.
        //
        // The classical torture mode collects on every *allocation*. That is
        // impossible here, and for a good reason — collection cannot happen
        // inside a mutation, and allocation only happens inside one. The bug
        // that trick hunts, a value lost across an allocation, is the bug the
        // branding in this module made unrepresentable.
        //
        // What is left to hunt is the collector's own mistakes: a mis-sized
        // header, a slot nobody traced, a root nobody forwarded. Those do not
        // care how often you allocate, but they do care how often you collect
        // and whether anyone checks the result. So: collect at every mutation
        // boundary, which is the finest granularity that exists, pad to-space
        // so nothing can land where it was, and verify afterwards.
        // Not during bootstrap. The static area is being laid out, its end is
        // not yet known, and a collection would pad the front and move `nil`
        // away from the index `Value::NIL` names — after the assertion that
        // checks it has already passed.
        if TORTURE && self.is_bootstrapped() {
            self.collect();
        }
        result
    }

    /// How many collections have happened.
    pub fn collections(&self) -> u64 {
        self.storage.collections
    }

    /// Collect garbage.
    ///
    /// Takes `&mut self`, which a live [`Mutation`] borrows, so calling this
    /// during a mutation does not compile:
    ///
    /// ```compile_fail
    /// # use quokka_vm::heap::Heap;
    /// let mut heap = Heap::new();
    /// heap.mutate(|_mc| heap.collect());
    /// ```
    ///
    /// # The algorithm
    ///
    /// Cheney's. Two semispaces; survivors are copied from the live one into
    /// the spare, which then becomes live. Everything not reached is gone, and
    /// reclaiming it costs nothing — the space is simply abandoned, which is
    /// why allocation can be a bump.
    ///
    /// Its trick is that it needs **no auxiliary stack**. The naive copying
    /// collector recurses to trace references; Cheney uses to-space itself as
    /// the work queue. `scan` trails `free` through the objects already copied,
    /// and copying an object pushes its own references onto the queue by
    /// placing them ahead of `scan`. A loop, not a recursion, so it cannot
    /// overflow however deep or tangled the object graph is. That matters here
    /// more than usual: the wasm32 stack is about a megabyte and fixed at link
    /// time.
    pub fn collect(&mut self) {
        let mut to = core::mem::take(&mut self.storage.spare);
        to.clear();

        // The static area is copied verbatim and keeps its indices, so every
        // reference into it stays valid without being touched.
        //
        // The padding goes in *now*, before anything is forwarded. Resizing
        // after the scan truncates whatever the scan copied — which is exactly
        // what went wrong the first time, and showed up as a root pointing one
        // word past the end of the heap.
        let static_end = self.storage.static_end;
        let pad = self.torture_padding();
        to.extend_from_slice(&self.storage.words[..static_end]);
        to.resize(static_end + pad, POISON);

        let from = &mut self.storage.words;

        // Roots. The registers, the obarray, the image root — and each one
        // that is forgotten is a live object silently reclaimed.
        self.storage.root = forward(from, &mut to, static_end, self.storage.root);
        self.storage.obarray = forward(from, &mut to, static_end, self.storage.obarray);
        // Every register is a root. Forgetting one reclaims the computation
        // in progress, which is a bug with no symptom until much later.
        self.storage.control = forward(from, &mut to, static_end, self.storage.control);
        self.storage.env = forward(from, &mut to, static_end, self.storage.env);
        self.storage.value = forward(from, &mut to, static_end, self.storage.value);
        self.storage.stack = forward(from, &mut to, static_end, self.storage.stack);
        self.storage.unwind = forward(from, &mut to, static_end, self.storage.unwind);

        // The queue. `scan` trails the end of `to`, which grows as references
        // are forwarded ahead of it. When they meet, everything reachable has
        // been copied and had its own references updated.
        // One scan over both regions. The static objects are not copied — they
        // are already here at their own indices — but their slots are still
        // forwarded, because a property list set on `nil` can point into
        // collected space. They are roots that happen not to move.
        let mut scan = 0;
        while scan < to.len() {
            if scan == static_end {
                scan = static_end + pad;
                if scan >= to.len() {
                    break;
                }
            }
            let (size, slots) = match Header::decode(to[scan]) {
                Some(header) => {
                    let size = HEADER_WORDS + header.slots;
                    // A string's characters and a code object's bytecode are
                    // not references. Tracing them would follow numbers.
                    let slots = if header.object_type.slots_are_values() {
                        scan + HEADER_WORDS..scan + size
                    } else {
                        scan..scan
                    };
                    (size, slots)
                }
                // No header means a cons cell: the tag is its type.
                None => (CONS_WORDS, scan..scan + CONS_WORDS),
            };

            for at in slots {
                // Read, forward, then write: forwarding pushes onto `to`, so
                // the three cannot share a borrow. Indices stay valid across
                // the push because they are indices, not pointers.
                let word = to[at];
                let updated = forward(from, &mut to, static_end, word);
                to[at] = updated;
            }
            scan += size;
        }

        // Flip. The old live space becomes the spare, keeping its allocation.
        let abandoned = core::mem::replace(&mut self.storage.words, to);
        self.storage.spare = abandoned;
        self.storage.collections += 1;
        self.storage.dynamic_start = static_end + pad;

        if TORTURE {
            self.verify()
                .expect("the heap is inconsistent after a collection");
        }
    }

    /// Check that the heap is internally consistent.
    ///
    /// Two passes. The first walks the heap linearly, the way the collector
    /// does, recording where each object begins; an object whose header lies
    /// about its size makes this overrun. The second checks that every
    /// reference — in a cons, in an object's value slots, and the root —
    /// points at one of those recorded boundaries.
    ///
    /// Between them these catch what the branding in [`Heap::mutate`] cannot:
    /// a slot the collector failed to update, a header whose length is wrong,
    /// a type traced when it should not have been, a root nobody forwarded.
    /// Those are bugs *in* the collector, and they produce wrong answers rather
    /// than crashes, so something has to go looking.
    ///
    /// Torture mode runs this after every collection. It is linear in the size
    /// of the heap and allocates a bitmap, so it is not something to do in
    /// ordinary operation.
    pub fn verify(&self) -> Result<(), Inconsistency> {
        let words = &self.storage.words;
        let mut starts = vec![false; words.len()];

        // Two regions: the static area from zero, then collected objects after
        // any torture padding.
        let mut at = 0usize;
        while at < words.len() {
            if at == self.storage.static_end {
                at = self.storage.dynamic_start;
                if at >= words.len() {
                    break;
                }
            }
            starts[at] = true;
            let size = match Header::decode(words[at]) {
                Some(header) => HEADER_WORDS + header.slots,
                None => CONS_WORDS,
            };
            if at + size > words.len() {
                return Err(Inconsistency::ObjectOverrunsHeap { at, size });
            }
            at += size;
        }

        let check = |word: u64, from: usize| -> Result<(), Inconsistency> {
            let value = Value::from_bits(word);
            if value.is_forwarding() {
                return Err(Inconsistency::ForwardingPointerLeft { from });
            }
            let Some(target) = value.word_index() else {
                return Ok(()); // refers to nothing
            };
            if target >= words.len() {
                return Err(Inconsistency::ReferenceOutOfBounds { from, target });
            }
            if !starts[target] {
                return Err(Inconsistency::ReferenceIntoNowhere { from, target });
            }
            Ok(())
        };

        check(self.storage.root, usize::MAX)?;
        check(self.storage.obarray, usize::MAX)?;
        check(self.storage.control, usize::MAX)?;
        check(self.storage.env, usize::MAX)?;
        check(self.storage.value, usize::MAX)?;
        check(self.storage.stack, usize::MAX)?;
        check(self.storage.unwind, usize::MAX)?;

        let mut at = 0usize;
        while at < words.len() {
            if at == self.storage.static_end {
                at = self.storage.dynamic_start;
                if at >= words.len() {
                    break;
                }
            }
            match Header::decode(words[at]) {
                Some(header) => {
                    if header.object_type.slots_are_values() {
                        for slot in 0..header.slots {
                            check(words[at + HEADER_WORDS + slot], at)?;
                        }
                    }
                    at += HEADER_WORDS + header.slots;
                }
                None => {
                    check(words[at], at)?;
                    check(words[at + 1], at)?;
                    at += CONS_WORDS;
                }
            }
        }
        Ok(())
    }

    const fn torture_padding(&self) -> usize {
        if TORTURE {
            // Varying, so a stale index cannot accidentally still be right.
            (self.storage.collections % 7) as usize + 1
        } else {
            0
        }
    }

    /// How many words the heap's objects occupy.
    ///
    /// Excludes the torture padding, so the number means the same thing with
    /// the feature on or off.
    pub fn used_words(&self) -> usize {
        self.storage.words.len() - self.storage.dynamic_start
    }

    /// The header at `word_index`, or `None` if a cons cell starts there.
    ///
    /// This is how a linear scan of the heap steps from one object to the next,
    /// and therefore how the collector will walk to-space. It is the reason
    /// headers are encoded so that no value can be mistaken for one.
    /// Where the first object begins. Non-zero only under torture padding.
    #[cfg(test)]
    pub(crate) fn first_object_for_test(&self) -> usize {
        self.storage.dynamic_start
    }

    /// Corrupt a word, so [`Heap::verify`] can be shown to catch things.
    ///
    /// A verifier nobody has seen fail is not known to work.
    /// `offset` is counted from the first object, so the torture padding does
    /// not shift what a test is aiming at.
    #[cfg(test)]
    pub(crate) fn poke_for_test(&mut self, offset: usize, word: u64) {
        let at = self.storage.dynamic_start + offset;
        self.storage.words[at] = word;
    }

    // Not called from the library yet; the scan test below exercises it.
    #[allow(dead_code)]
    pub(crate) fn header_at(&self, word_index: usize) -> Option<Header> {
        Header::decode(self.storage.words[word_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cons_cell_holds_its_car_and_cdr() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let one = Value::fixnum(1).unwrap();
            let cell = mc.cons(one, Value::NIL);
            assert_eq!(mc.car(cell), one);
            assert!(mc.cdr(cell).is_nil());
            assert_eq!(mc.used_words(), CONS_WORDS);
        });
    }

    #[test]
    fn a_list_can_be_built_and_walked() {
        let mut heap = Heap::new();
        let total = heap.mutate(|mc| {
            // (1 2 3), built back to front, holding the tail throughout.
            let mut list = Value::NIL;
            for n in [3i64, 2, 1] {
                list = mc.cons(Value::fixnum(n).unwrap(), list);
            }
            let mut sum = 0;
            let mut cursor = list;
            while !cursor.is_nil() {
                sum += mc.car(cursor).as_fixnum().unwrap();
                cursor = mc.cdr(cursor);
            }
            mc.set_root(list); // or torture mode collects it the moment we leave
            sum
        });
        assert_eq!(total, 6);
        assert_eq!(heap.used_words(), 3 * CONS_WORDS);
    }

    #[test]
    fn mutation_is_cumulative_and_the_root_persists() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let cell = mc.cons(Value::fixnum(7).unwrap(), Value::NIL);
            mc.set_root(cell);
        });
        heap.collect();
        heap.mutate(|mc| {
            assert_eq!(mc.car(mc.root()).as_fixnum(), Some(7));
            let grown = mc.cons(Value::fixnum(8).unwrap(), mc.root());
            mc.set_root(grown);
        });
        heap.mutate(|mc| {
            assert_eq!(mc.car(mc.root()).as_fixnum(), Some(8));
            assert_eq!(mc.car(mc.cdr(mc.root())).as_fixnum(), Some(7));
        });
    }

    #[test]
    fn cells_can_be_mutated_in_place() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let cell = mc.cons(Value::NIL, Value::NIL);
            mc.set_car(cell, Value::T);
            mc.set_cdr(cell, Value::fixnum(-3).unwrap());
            assert_eq!(mc.car(cell), Value::T);
            assert_eq!(mc.cdr(cell).as_fixnum(), Some(-3));
        });
    }

    // ---- objects ---------------------------------------------------------

    #[test]
    fn an_object_starts_with_nil_slots_and_a_readable_header() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let v = mc.alloc(ObjectType::Vector, 3);
            assert_eq!(
                mc.header(v),
                Header {
                    object_type: ObjectType::Vector,
                    slots: 3
                }
            );
            for i in 0..3 {
                assert!(mc.slot(v, i).is_nil(), "slot {i} was not initialised");
            }
            assert_eq!(mc.used_words(), HEADER_WORDS + 3);
        });
    }

    #[test]
    fn slots_round_trip() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let sym = mc.alloc(ObjectType::Symbol, 2);
            let cell = mc.cons(Value::fixnum(1).unwrap(), Value::NIL);
            mc.set_slot(sym, 0, cell);
            mc.set_slot(sym, 1, Value::T);
            assert_eq!(mc.slot(sym, 0), cell);
            assert_eq!(mc.slot(sym, 1), Value::T);
            assert_eq!(mc.car(mc.slot(sym, 0)).as_fixnum(), Some(1));
        });
    }

    #[test]
    fn raw_slots_hold_arbitrary_words() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let s = mc.alloc(ObjectType::String, 2);
            mc.set_raw_slot(s, 0, u64::MAX);
            mc.set_raw_slot(s, 1, 0);
            assert_eq!(mc.raw_slot(s, 0), u64::MAX);
            assert_eq!(mc.raw_slot(s, 1), 0);
        });
    }

    #[test]
    fn headers_survive_every_size_and_type() {
        let types = [
            ObjectType::Symbol,
            ObjectType::Vector,
            ObjectType::Closure,
            ObjectType::Instance,
            ObjectType::HashTable,
            ObjectType::String,
            ObjectType::Code,
            ObjectType::Bignum,
        ];
        for object_type in types {
            // On wasm32 MAX_OBJECT_SLOTS is usize::MAX, which the 48-bit
            // field cannot hold; the encoding's own limit is what matters.
            let widest = (u64::MAX >> HEADER_LEN_SHIFT).min(usize::MAX as u64) as usize;
            for slots in [0usize, 1, 2, 7, 1_000, widest] {
                let header = Header { object_type, slots };
                assert_eq!(Header::decode(header.encode()), Some(header));
                assert!(Value::from_bits(header.encode()).is_header());
            }
        }
    }

    /// The discriminants encode "traced or not" as a comparison against 64.
    /// Putting a new type on the wrong side of that line would make the
    /// collector follow a string's characters as if they were references, and
    /// nothing else would notice.
    #[test]
    fn raw_types_are_not_traced() {
        for t in [ObjectType::String, ObjectType::Code, ObjectType::Bignum] {
            assert!(!t.slots_are_values(), "{t:?} must not be traced");
        }
        for t in [
            ObjectType::Symbol,
            ObjectType::Vector,
            ObjectType::Closure,
            ObjectType::Instance,
            ObjectType::HashTable,
        ] {
            assert!(t.slots_are_values(), "{t:?} must be traced");
        }
    }

    #[test]
    #[should_panic(expected = "use the other accessor")]
    fn reading_a_strings_characters_as_values_panics() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let s = mc.alloc(ObjectType::String, 1);
            let _ = mc.slot(s, 0);
        });
    }

    #[test]
    #[should_panic(expected = "use the other accessor")]
    fn reading_a_vectors_elements_as_raw_words_panics() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let v = mc.alloc(ObjectType::Vector, 1);
            let _ = mc.raw_slot(v, 0);
        });
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn reading_past_the_last_slot_panics() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let v = mc.alloc(ObjectType::Vector, 2);
            let _ = mc.slot(v, 2);
        });
    }

    // ---- the linear scan this header design exists for --------------------

    /// What the collector will do: walk the heap from the start, deciding at
    /// each position whether it is looking at a header or a cons cell, and
    /// stepping by the right amount. If this works on a mixed heap, the
    /// encoding does its job.
    #[test]
    fn a_linear_scan_can_walk_a_mixed_heap() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            // Everything must be reachable, or torture mode collects it before
            // the scan runs. A vector holds the lot.
            let all = mc.alloc(ObjectType::Vector, 4);
            let plain = mc.cons(Value::fixnum(1).unwrap(), Value::NIL);
            mc.set_slot(all, 0, plain);
            let inner = mc.alloc(ObjectType::Vector, 3);
            mc.set_slot(all, 1, inner);
            // A cons whose car is a character and whose cdr is a big fixnum:
            // the words most likely to be mistaken for a header.
            let awkward = mc.cons(
                Value::character('q'),
                Value::fixnum(i32::MAX as i64).unwrap(),
            );
            mc.set_slot(all, 2, awkward);
            let text = mc.alloc(ObjectType::String, 2);
            mc.set_slot(all, 3, text);
            mc.set_root(all);
        });

        let start = heap.first_object_for_test();
        let mut found = Vec::new();
        let mut at = start;
        while at < start + heap.used_words() {
            match heap.header_at(at) {
                Some(h) => {
                    found.push(format!("{:?}/{}", h.object_type, h.slots));
                    at += HEADER_WORDS + h.slots;
                }
                None => {
                    found.push("cons".to_string());
                    at += CONS_WORDS;
                }
            }
        }
        // Collection reorders objects, so compare the multiset rather than the
        // sequence. What is tested is that boundaries are recovered exactly.
        found.sort();
        assert_eq!(
            found,
            ["String/2", "Vector/3", "Vector/4", "cons", "cons"],
            "the scan lost track of object boundaries"
        );
        assert_eq!(at, start + heap.used_words(), "the scan overran the heap");
    }

    // ---- growth -----------------------------------------------------------

    #[test]
    fn the_heap_asks_to_be_collected_once_past_its_soft_limit() {
        let mut heap = Heap::with_soft_limit(8);
        assert!(!heap.wants_collection());
        // Rooted as it grows, so the measurement means the same thing whether
        // or not torture mode collects between the two mutations.
        heap.mutate(|mc| {
            let cell = mc.cons(Value::NIL, Value::NIL);
            mc.set_root(cell);
        });
        assert!(!heap.wants_collection(), "2 words is under the limit of 8");
        heap.mutate(|mc| {
            let mut list = mc.root();
            for _ in 0..4 {
                list = mc.cons(Value::NIL, list);
            }
            mc.set_root(list);
        });
        assert!(heap.wants_collection(), "10 words is over the limit of 8");
    }

    /// Allocation inside a mutation must never fail for want of space, because
    /// collection cannot happen there. The heap grows past its soft limit
    /// instead, and asks to be collected afterwards.
    #[test]
    fn a_mutation_can_overshoot_the_soft_limit() {
        let mut heap = Heap::with_soft_limit(4);
        let limit = heap.soft_limit();
        // Asserted inside the mutation: that is where the overshoot must be
        // allowed, and the only point torture mode has not yet collected.
        heap.mutate(|mc| {
            for n in 0..100 {
                mc.alloc(ObjectType::Vector, n % 5);
            }
            assert!(
                mc.used_words() > limit,
                "allocation inside a mutation must never fail for want of space"
            );
        });
    }

    // ---- collection ------------------------------------------------------

    /// Build a list of `n` fixnums and make it the root.
    fn list_of(heap: &mut Heap, n: i64) {
        heap.mutate(|mc| {
            let mut list = Value::NIL;
            for value in (1..=n).rev() {
                list = mc.cons(Value::fixnum(value).unwrap(), list);
            }
            mc.set_root(list);
        });
    }

    /// Read the root back as a list of fixnums.
    fn root_list(heap: &mut Heap) -> Vec<i64> {
        heap.mutate(|mc| {
            let mut out = Vec::new();
            let mut cursor = mc.root();
            while !cursor.is_nil() {
                out.push(mc.car(cursor).as_fixnum().unwrap());
                cursor = mc.cdr(cursor);
            }
            out
        })
    }

    #[test]
    fn what_is_reachable_survives_and_what_is_not_does_not() {
        let mut heap = Heap::new();
        let before = heap.mutate(|mc| {
            // Reachable: a three-element list hung off the root.
            let mut list = Value::NIL;
            for value in [3i64, 2, 1] {
                list = mc.cons(Value::fixnum(value).unwrap(), list);
            }
            mc.set_root(list);
            // Garbage: a vector and a long list nothing refers to.
            mc.alloc(ObjectType::Vector, 10);
            let mut waste = Value::NIL;
            for _ in 0..50 {
                waste = mc.cons(Value::NIL, waste);
            }
            mc.used_words()
        });

        heap.collect();

        assert_eq!(root_list(&mut heap), [1, 2, 3], "live data did not survive");
        assert_eq!(
            heap.used_words(),
            3 * CONS_WORDS,
            "only the three live conses should remain"
        );
        assert!(heap.used_words() < before);
        heap.verify().unwrap();
    }

    #[test]
    fn collecting_an_empty_heap_is_fine() {
        let mut heap = Heap::new();
        heap.collect();
        assert_eq!(heap.used_words(), 0);
        assert!(heap.mutate(|mc| mc.root().is_nil()));
        heap.verify().unwrap();
    }

    #[test]
    fn repeated_collection_is_idempotent() {
        let mut heap = Heap::new();
        list_of(&mut heap, 5);
        heap.collect();
        let settled = heap.used_words();
        for _ in 0..10 {
            heap.collect();
            assert_eq!(heap.used_words(), settled, "a collection changed live size");
            assert_eq!(root_list(&mut heap), [1, 2, 3, 4, 5]);
        }
        heap.verify().unwrap();
    }

    /// Two references to one object must still be two references to *one*
    /// object afterwards. This is what the forwarding pointer is for: without
    /// it the object would be copied twice and the sharing silently lost.
    #[test]
    fn shared_structure_stays_shared() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let shared = mc.cons(Value::fixnum(99).unwrap(), Value::NIL);
            let pair = mc.cons(shared, shared);
            mc.set_root(pair);
        });
        heap.collect();
        heap.mutate(|mc| {
            let pair = mc.root();
            assert_eq!(mc.car(pair), mc.cdr(pair), "sharing was lost");
            assert_eq!(mc.car(mc.car(pair)).as_fixnum(), Some(99));
        });
        assert_eq!(
            heap.used_words(),
            2 * CONS_WORDS,
            "the shared cell was copied twice"
        );
        heap.verify().unwrap();
    }

    /// A cycle must terminate. The forwarding pointer is what stops the trace
    /// going round forever.
    #[test]
    fn a_cycle_does_not_loop_forever() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let cell = mc.cons(Value::fixnum(1).unwrap(), Value::NIL);
            mc.set_cdr(cell, cell); // (1 . itself)
            mc.set_root(cell);
        });
        heap.collect();
        heap.mutate(|mc| {
            let cell = mc.root();
            assert_eq!(mc.car(cell).as_fixnum(), Some(1));
            assert_eq!(mc.cdr(cell), cell, "the cycle was not preserved");
        });
        heap.verify().unwrap();
    }

    /// Raw-slot objects must be copied verbatim and never traced. A string
    /// full of words that look like references is the case that would break.
    #[test]
    fn raw_slots_are_copied_but_not_followed() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let s = mc.alloc(ObjectType::String, 3);
            // Bit patterns that would be a cons at index 0, an object far out
            // of bounds, and a forwarding pointer, if anyone traced them.
            mc.set_raw_slot(s, 0, 0b001);
            mc.set_raw_slot(s, 1, (999_999u64 << 3) | 0b011);
            mc.set_raw_slot(s, 2, 0b111);
            mc.set_root(s);
        });
        heap.collect();
        heap.mutate(|mc| {
            let s = mc.root();
            assert_eq!(mc.header(s).object_type, ObjectType::String);
            assert_eq!(mc.raw_slot(s, 0), 0b001);
            assert_eq!(mc.raw_slot(s, 1), (999_999u64 << 3) | 0b011);
            assert_eq!(mc.raw_slot(s, 2), 0b111);
        });
        heap.verify().unwrap();
    }

    #[test]
    fn objects_and_conses_survive_together() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let vector = mc.alloc(ObjectType::Vector, 2);
            let cell = mc.cons(Value::fixnum(7).unwrap(), vector);
            mc.set_slot(vector, 0, cell); // back-reference: a cycle through an object
            mc.set_slot(vector, 1, Value::T);
            mc.set_root(cell);
        });
        heap.collect();
        heap.mutate(|mc| {
            let cell = mc.root();
            assert_eq!(mc.car(cell).as_fixnum(), Some(7));
            let vector = mc.cdr(cell);
            assert_eq!(mc.header(vector).slots, 2);
            assert_eq!(mc.slot(vector, 0), cell, "the back-reference broke");
            assert_eq!(mc.slot(vector, 1), Value::T);
        });
        heap.verify().unwrap();
    }

    /// Deep structure must not overflow the Rust stack. Cheney uses to-space
    /// as its work queue rather than recursing, so depth costs nothing — the
    /// property that matters most on wasm32, where the stack is about a
    /// megabyte and fixed at link time.
    #[test]
    fn a_very_deep_list_does_not_overflow_the_stack() {
        let mut heap = Heap::new();
        list_of(&mut heap, 100_000);
        heap.collect();
        assert_eq!(heap.used_words(), 100_000 * CONS_WORDS);
        assert_eq!(root_list(&mut heap).len(), 100_000);
        heap.verify().unwrap();
    }

    #[test]
    fn collecting_clears_the_wish_to_be_collected() {
        let mut heap = Heap::with_soft_limit(8);
        let before = heap.collections();
        heap.mutate(|mc| {
            for _ in 0..20 {
                mc.cons(Value::NIL, Value::NIL);
            }
            assert!(mc.used_words() > 8);
        });
        heap.collect();
        assert!(!heap.wants_collection(), "all of it was garbage");
        assert!(heap.collections() > before);
    }

    // ---- verify catches what branding cannot ------------------------------

    #[test]
    fn verify_passes_on_a_healthy_heap() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let vector = mc.alloc(ObjectType::Vector, 2);
            let cell = mc.cons(Value::fixnum(1).unwrap(), vector);
            mc.set_slot(vector, 0, cell);
            mc.set_root(cell);
            mc.alloc(ObjectType::String, 2);
        });
        heap.verify().unwrap();
        heap.collect();
        heap.verify().unwrap();
    }

    /// The failure a collector bug actually produces: a slot left pointing at
    /// the old layout. Simulated by corrupting a slot, since the collector is
    /// not currently wrong.
    #[test]
    fn verify_catches_a_reference_that_is_not_an_object_start() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let cell = mc.cons(Value::fixnum(1).unwrap(), Value::NIL);
            mc.set_root(cell);
        });
        // Point the cdr at its own second word: inside the heap, inside an
        // object, but not at an object start — what an untraced slot looks
        // like once everything has moved.
        let interior = heap.first_object_for_test() + 1;
        heap.poke_for_test(1, Value::cons(interior).unwrap().bits());
        assert!(matches!(
            heap.verify(),
            Err(Inconsistency::ReferenceIntoNowhere { .. })
        ));
    }

    #[test]
    fn verify_catches_a_reference_off_the_end() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let cell = mc.cons(Value::NIL, Value::NIL);
            mc.set_root(cell);
        });
        heap.poke_for_test(0, Value::cons(9_999).unwrap().bits());
        assert!(matches!(
            heap.verify(),
            Err(Inconsistency::ReferenceOutOfBounds { .. })
        ));
    }

    #[test]
    fn verify_catches_a_header_that_lies_about_its_length() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let v = mc.alloc(ObjectType::Vector, 1);
            mc.set_root(v);
        });
        let lying = Header {
            object_type: ObjectType::Vector,
            slots: 500,
        };
        heap.poke_for_test(0, lying.encode());
        assert!(matches!(
            heap.verify(),
            Err(Inconsistency::ObjectOverrunsHeap { .. })
        ));
    }

    #[test]
    fn verify_catches_a_stray_forwarding_pointer() {
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let cell = mc.cons(Value::NIL, Value::NIL);
            mc.set_root(cell);
        });
        heap.poke_for_test(0, Value::forwarding(0).unwrap().bits());
        assert!(matches!(
            heap.verify(),
            Err(Inconsistency::ForwardingPointerLeft { .. })
        ));
    }

    /// The variance claim the whole scheme rests on, from the other side.
    ///
    /// `Invariant<'h>` must reject shortening a brand. The matching
    /// `compile_fail` doctest on [`Value`] asserts that it does; this asserts
    /// that the *covariant* spelling would allow it, so that test is really
    /// testing variance rather than failing for some unrelated reason.
    #[test]
    fn the_covariant_spelling_would_allow_shortening() {
        use core::marker::PhantomData;

        #[derive(Clone, Copy)]
        struct Covariant<'h>(PhantomData<&'h ()>);

        fn shorten<'long: 'short, 'short>(v: Covariant<'long>) -> Covariant<'short> {
            v // compiles: covariance permits exactly the escape we must prevent
        }

        let long: Covariant<'static> = Covariant(PhantomData);
        let _short = shorten(long);
    }
}
