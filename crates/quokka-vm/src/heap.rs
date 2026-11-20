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
use core::marker::PhantomData;

/// How many words a cons cell occupies.
pub const CONS_WORDS: usize = 2;

/// How many words an object's header occupies.
pub const HEADER_WORDS: usize = 1;

/// Default soft limit: 1 MiB of heap before a collection is wanted.
pub const DEFAULT_SOFT_LIMIT_WORDS: usize = 128 * 1024;

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
            64 => String,
            65 => Code,
            66 => Bignum,
            _ => return None,
        })
    }
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
    words: Vec<u64>,
    /// The one persistent root: everything that must survive hangs off it.
    root: u64,
    /// Above this, the heap would like to be collected. Not a hard limit —
    /// see [`Heap::wants_collection`].
    soft_limit: usize,
}

impl Storage {
    fn alloc(&mut self, words: usize) -> usize {
        let at = self.words.len();
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

    /// The persistent root.
    pub fn root(&self) -> Value<'h> {
        Value::from_bits(self.storage.root)
    }

    /// Set the persistent root. Anything not reachable from it is garbage.
    pub fn set_root(&mut self, v: Value<'h>) {
        self.storage.root = v.bits();
    }

    /// How many words are in use.
    pub fn used_words(&self) -> usize {
        self.storage.words.len()
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
#[derive(Debug, Default)]
pub struct Heap {
    storage: Storage,
}

impl Heap {
    /// A new, empty heap whose root is `nil`, with a default soft limit.
    pub fn new() -> Heap {
        Heap::with_soft_limit(DEFAULT_SOFT_LIMIT_WORDS)
    }

    /// A new heap that would like to be collected once it passes `words`.
    pub fn with_soft_limit(words: usize) -> Heap {
        Heap {
            storage: Storage {
                words: Vec::new(),
                root: Value::NIL.bits(),
                soft_limit: words,
            },
        }
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
        self.storage.words.len() > self.storage.soft_limit
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
        f(&mut mutation)
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
    /// Not implemented yet — see the Cheney collector issue. The signature is
    /// here because it is what makes the rule above true.
    pub fn collect(&mut self) {
        // Cheney's algorithm goes here. Tracing starts at `self.storage.root`.
    }

    /// How many words are in use.
    pub fn used_words(&self) -> usize {
        self.storage.words.len()
    }

    /// The header at `word_index`, or `None` if a cons cell starts there.
    ///
    /// This is how a linear scan of the heap steps from one object to the next,
    /// and therefore how the collector will walk to-space. It is the reason
    /// headers are encoded so that no value can be mistaken for one.
    // Not called from the library yet: #3 writes the collector that walks with
    // it. The scan test below does exercise it.
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
            mc.cons(Value::fixnum(1).unwrap(), Value::NIL);
            mc.alloc(ObjectType::Vector, 3);
            // A cons whose car is a character and whose cdr is a big fixnum:
            // the words most likely to be mistaken for a header.
            mc.cons(
                Value::character('q'),
                Value::fixnum(i32::MAX as i64).unwrap(),
            );
            mc.alloc(ObjectType::String, 2);
            mc.cons(Value::NIL, Value::NIL);
        });

        let mut found = Vec::new();
        let mut at = 0;
        while at < heap.used_words() {
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
        assert_eq!(
            found,
            ["cons", "Vector/3", "cons", "String/2", "cons"],
            "the scan lost track of object boundaries"
        );
        assert_eq!(at, heap.used_words(), "the scan overran the heap");
    }

    // ---- growth -----------------------------------------------------------

    #[test]
    fn the_heap_asks_to_be_collected_once_past_its_soft_limit() {
        let mut heap = Heap::with_soft_limit(8);
        assert!(!heap.wants_collection());
        heap.mutate(|mc| {
            mc.cons(Value::NIL, Value::NIL);
        });
        assert!(!heap.wants_collection(), "2 words is under the limit of 8");
        heap.mutate(|mc| {
            for _ in 0..4 {
                mc.cons(Value::NIL, Value::NIL);
            }
        });
        assert!(heap.wants_collection(), "10 words is over the limit of 8");
    }

    /// Allocation inside a mutation must never fail for want of space, because
    /// collection cannot happen there. The heap grows past its soft limit
    /// instead, and asks to be collected afterwards.
    #[test]
    fn a_mutation_can_overshoot_the_soft_limit() {
        let mut heap = Heap::with_soft_limit(4);
        heap.mutate(|mc| {
            for n in 0..100 {
                mc.alloc(ObjectType::Vector, n % 5);
            }
        });
        assert!(heap.used_words() > heap.soft_limit());
        assert!(heap.wants_collection());
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
