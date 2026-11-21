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
    /// Which namespace the symbol was interned in — see [`crate::heap::home`].
    ///
    /// `nil` for a symbol that is interned nowhere, which is what
    /// [`crate::heap::Mutation::gensym`] makes.
    pub const HOME: usize = 5;
    /// Non-nil if this names a special operator, as a small integer the
    /// evaluator dispatches on.
    ///
    /// On the symbol rather than in a lookup table because that is what it is:
    /// `if` is special because the symbol `IF` is, and a slot read beats
    /// comparing names on every form evaluated.
    pub const SPECIAL: usize = 4;
    /// How many slots a symbol has.
    pub const SLOTS: usize = 6;
}

/// The namespaces a symbol can be interned in.
///
/// **This is deliberately two-sevenths of a package system.**
///
/// Common Lisp says a keyword is a symbol in the `KEYWORD` package, which
/// means `foo` and `:foo` have the *same name* and are *different symbols*:
///
/// ```lisp
/// (symbol-name :foo)   ; => "FOO", not ":FOO"
/// (eq :foo 'foo)       ; => NIL
/// ```
///
/// A symbol table keyed on the name alone cannot express that. The cheap
/// alternative — interning `:foo` under the name `":FOO"` — would make
/// `symbol-name` quietly wrong, and wrong in a way nothing detects until
/// `symbol-name` is exposed to Lisp, which is the worst kind of bug to plant.
///
/// So the table is keyed on *(namespace, name)*, with exactly two namespaces
/// and none of the machinery — no `use-package`, no `export`, no `::`, no
/// `*package*`. Packages proper are tier 1, phase 2 (`docs/design.md`), and
/// want a hash table per package, which is group C and not built. When they
/// arrive, [`symbol::HOME`] stops being a small integer and becomes a pointer
/// to a package object; nothing else here has to change.
pub mod home {
    /// The one ordinary namespace. Becomes `COMMON-LISP-USER` and friends.
    pub const CURRENT: i64 = 0;
    /// `KEYWORD`: self-evaluating constants, printed with a leading colon.
    pub const KEYWORD: i64 = 1;
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
    /// A compiled function: code, constants, parameters. See `crate::code`.
    Function = 10,
    /// A signalled condition: type, message, data. See `crate::condition`.
    ///
    /// Its own type rather than an `Instance`, which it was at first — and
    /// `Instance` is also what `fresh_marker` allocates for a `block` or
    /// `catch` to unwind to. Every marker therefore looked like a condition,
    /// so a `return-from` passing through a `handler-case` was caught by it.
    /// Reusing a type for two purposes is reusing a *name*, and the collector
    /// does not care but everything above it does.
    Condition = 9,
    /// A macro: the same shape as a closure, and deliberately a different
    /// type.
    ///
    /// A macro is applied to its arguments **unevaluated**, and its result is
    /// evaluated in place of the call. Calling one as if it were a function
    /// would quietly do the wrong thing, so the evaluator must be able to tell
    /// them apart — which a header can and a flag inside a closure could too,
    /// less obviously.
    Macro = 8,
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
    /// A string that cannot be written. **Not traced.**
    ///
    /// Source literals and symbol names. Common Lisp says the consequences of
    /// destructively modifying a literal object are undefined (CLHS 3.7.1),
    /// which is permission to share them and no help at all when a program
    /// does it anyway: before this type existed, `(setf (aref (greeting) 0)
    /// #\J)` quietly edited the source of `greeting`.
    ///
    /// A distinct type rather than a header flag, because the header's single
    /// subtag test is what makes the collector's linear scan one comparison,
    /// and because 67 is above the 64 line so `slots_are_values` still answers
    /// correctly without being told about it.
    ///
    /// It is also what makes sharing *sound* rather than merely licensed: an
    /// immutable string can be coalesced with an equal one and nothing can
    /// tell. See issue #26.
    ConstantString = 67,
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

    /// Is this a string, writable or not?
    ///
    /// Every reader — `aref`, `length`, `stringp`, the printer, the hasher —
    /// asks this. Only `%set-aref` cares which of the two it is, which is the
    /// whole point: immutability is one refusal in one place, not a condition
    /// scattered through everything that reads text.
    pub const fn is_string(self) -> bool {
        matches!(self, ObjectType::String | ObjectType::ConstantString)
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
            8 => Macro,
            9 => Condition,
            10 => Function,
            64 => String,
            65 => Code,
            66 => Bignum,
            67 => ConstantString,
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
    /// **All** the values the last step produced, or [`Value::UNBOUND`].
    ///
    /// Common Lisp forms may return any number of values, and almost nothing
    /// looks at more than the first. So the ordinary path keeps using
    /// [`Register::Value`], and this holds `UNBOUND` to mean "one value, in
    /// the Value register" — the common case, costing nothing.
    ///
    /// When a form does return some other number, this holds the whole list
    /// (empty for `(values)`), and only `multiple-value-call` and
    /// `multiple-value-prog1` look. Anything else takes the primary and the
    /// rest are dropped, which is what makes `(+ (values 1 2) 3)` equal 4.
    Values,
    /// The compiled [`crate::code`] function the bytecode VM is executing,
    /// which is where its constants come from. `nil` while the interpreter
    /// is the thing running.
    Function,
    /// The bytecode VM's operand stack: one vector, grown as needed.
    ///
    /// A heap object rather than a Rust `Vec`, for the same reason as every
    /// other register — the collector must trace it, and an image must be
    /// able to save a computation half-finished.
    Operands,
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
    /// All the values the last step produced, or `UNBOUND` for "just one".
    /// A root like the rest.
    values: u64,
    /// The compiled function the bytecode VM is running. A root like the rest.
    function: u64,
    /// Its operand stack, a vector. A root like the rest.
    operands: u64,
    /// The bytecode VM's program counter, stack pointer and frame pointer.
    ///
    /// Numbers rather than `Value`s, so the collector leaves them alone — the
    /// same arrangement as `mode`. They still live here rather than in a Rust
    /// local, because a slice may end between any two instructions and an
    /// image may be saved there.
    pc: usize,
    sp: usize,
    fp: usize,
    /// Where the innermost unwind record sits on the operand stack, or
    /// `usize::MAX` for none. See `crate::vm::mark`.
    mp: usize,
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
    /// Counts the uninterned symbols `gensym` has made, so each gets a
    /// distinct name. Not a `Value`, so the collector does not touch it.
    gensym_counter: u64,
    /// How many collections have happened. Torture mode uses it to vary the
    /// padding, so no object can land where it was.
    collections: u64,
    /// Which point in this image's history the heap is at.
    ///
    /// Saved in the image header and incremented by each save, so two images
    /// of the same world can be ordered without comparing their contents.
    /// Required by the design before the format was fixed, because a change
    /// log (#24) needs to say which snapshot it continues from and a header
    /// field cannot be added to images that already exist.
    sequence: u64,
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

    /// Read one byte of a raw object, counting from its first word.
    ///
    /// Bytes are packed eight to a word, least significant first. The packing
    /// is written as shifts rather than as a transmute, so it means the same
    /// thing on every target and an image written by one is readable by
    /// another — rule 3's concern, arriving a second time.
    pub fn raw_byte(&self, v: Value<'h>, index: usize) -> u8 {
        let word = self.raw_slot(v, index / 8);
        (word >> ((index % 8) * 8)) as u8
    }

    /// Write one byte of a raw object. See [`Self::raw_byte`].
    pub fn set_raw_byte(&mut self, v: Value<'h>, index: usize, byte: u8) {
        let at = index / 8;
        let shift = (index % 8) * 8;
        let word = self.raw_slot(v, at) & !(0xFFu64 << shift);
        self.set_raw_slot(v, at, word | ((byte as u64) << shift));
    }

    /// Allocate a raw object holding `bytes` bytes, rounded up to whole words.
    ///
    /// The header counts *words*, so the byte length is not recoverable from
    /// the object and whoever allocated it has to remember — for bytecode,
    /// that is the `Function` that owns the `Code`.
    pub fn alloc_bytes(&mut self, object_type: ObjectType, bytes: &[u8]) -> Value<'h> {
        let v = self.alloc(object_type, bytes.len().div_ceil(8));
        for (i, b) in bytes.iter().enumerate() {
            self.set_raw_byte(v, i, *b);
        }
        v
    }

    /// Allocate a string.
    ///
    /// One Unicode scalar per word. That wastes most of each word, and it is
    /// deliberate for now: it makes indexing a character O(1) and the code
    /// obvious. Packing comes when something measures that it matters.
    pub fn alloc_string(&mut self, text: &str) -> Value<'h> {
        self.alloc_string_of(text, ObjectType::String)
    }

    /// Allocate a string nothing may write to: a literal, or a symbol's name.
    ///
    /// See [`ObjectType::ConstantString`] for why the distinction is a type.
    pub fn alloc_constant_string(&mut self, text: &str) -> Value<'h> {
        self.alloc_string_of(text, ObjectType::ConstantString)
    }

    fn alloc_string_of(&mut self, text: &str, kind: ObjectType) -> Value<'h> {
        let chars: Vec<char> = text.chars().collect();
        let v = self.alloc(kind, chars.len());
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
        assert!(self.header(v).object_type.is_string(), "not a string");
        (0..self.header(v).slots)
            .map(|i| char::from_u32(self.raw_slot(v, i) as u32).expect("invalid character"))
            .collect()
    }

    /// Does this string have exactly these characters?
    ///
    /// Compares without allocating, which matters because interning calls it
    /// once per symbol already known.
    pub fn string_is(&self, v: Value<'h>, text: &str) -> bool {
        if !v.is_object() || !self.header(v).object_type.is_string() {
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
        self.intern_in(name, home::CURRENT)
    }

    /// Intern `name` as a keyword: `intern_keyword("FOO")` is `:FOO`.
    ///
    /// The name does **not** include the colon — that is punctuation the
    /// reader and printer deal in, not part of the symbol. The value cell is
    /// set to the symbol itself, because CLHS says a keyword is a constant
    /// variable bound to itself, and that one line is the whole reason `:foo`
    /// evaluates without the evaluator knowing keywords exist.
    pub fn intern_keyword(&mut self, name: &str) -> Value<'h> {
        let symbol = self.intern_in(name, home::KEYWORD);
        self.set_slot(symbol, symbol::VALUE, symbol);
        symbol
    }

    /// The shared path: a symbol is identified by its namespace *and* its
    /// name, so `FOO` and `:FOO` are two symbols with one name.
    fn intern_in(&mut self, name: &str, home: i64) -> Value<'h> {
        let wanted = Value::fixnum(home).expect("a namespace is a small number");
        let mut cursor = Value::from_bits(self.storage.obarray);
        while !cursor.is_nil() {
            let symbol = self.car(cursor);
            if self.slot(symbol, symbol::HOME) == wanted
                && self.string_is(self.slot(symbol, symbol::NAME), name)
            {
                return symbol;
            }
            cursor = self.cdr(cursor);
        }

        let text = self.alloc_constant_string(name);
        let symbol = self.alloc(ObjectType::Symbol, symbol::SLOTS);
        self.set_slot(symbol, symbol::NAME, text);
        self.set_slot(symbol, symbol::VALUE, Value::UNBOUND);
        self.set_slot(symbol, symbol::FUNCTION, Value::UNBOUND);
        self.set_slot(symbol, symbol::HOME, wanted);
        let entry = self.cons(symbol, Value::from_bits(self.storage.obarray));
        self.storage.obarray = entry.bits();
        symbol
    }

    /// Is this symbol a keyword?
    pub fn is_keyword(&self, v: Value<'h>) -> bool {
        self.is_symbol(v)
            && self.slot(v, symbol::HOME) == Value::fixnum(home::KEYWORD).expect("small")
    }

    /// A vector of `len` slots, all nil.
    ///
    /// Its slots hold `Value`s, so the collector traces them — which is what
    /// `ObjectType::Vector` being below 64 means (see
    /// [`ObjectType::slots_are_values`]). A string, whose slots are raw
    /// character codes, sits above that line instead.
    pub fn alloc_vector(&mut self, len: usize) -> Value<'h> {
        self.alloc(ObjectType::Vector, len)
    }

    /// A fresh symbol belonging to nobody.
    ///
    /// Deliberately **not interned**: reading `#:G1` would make a different
    /// symbol, and that is the point. A macro that binds a temporary must use
    /// a name the caller cannot have written, or
    ///
    /// ```lisp
    /// (defmacro or2 (a b) `(let ((v ,a)) (if v v ,b)))
    /// (let ((v 'outer)) (or2 nil v))   ; => NIL, and should be OUTER
    /// ```
    ///
    /// quietly computes the wrong thing: `,b` is substituted *inside* a
    /// binding the macro invented, so the caller's `v` is no longer in scope.
    /// That is *variable capture*, and an uninterned symbol is the simplest
    /// defence against it. `tests/lisp/macros.lisp` keeps both versions, so
    /// the difference stays visible.
    pub fn gensym(&mut self) -> Value<'h> {
        self.storage.gensym_counter += 1;
        let n = self.storage.gensym_counter;
        let text = self.alloc_constant_string(&format!("G{n}"));
        let s = self.alloc(ObjectType::Symbol, symbol::SLOTS);
        self.set_slot(s, symbol::NAME, text);
        self.set_slot(s, symbol::VALUE, Value::UNBOUND);
        self.set_slot(s, symbol::FUNCTION, Value::UNBOUND);
        // Interned nowhere, which is the point.
        self.set_slot(s, symbol::HOME, Value::NIL);
        s
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
            Register::Values => self.storage.values,
            Register::Function => self.storage.function,
            Register::Operands => self.storage.operands,
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
            Register::Values => &mut self.storage.values,
            Register::Function => &mut self.storage.function,
            Register::Operands => &mut self.storage.operands,
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

    /// The bytecode VM's program counter: a byte offset into its function's
    /// code. Meaning is `crate::vm`'s business.
    pub fn pc(&self) -> usize {
        self.storage.pc
    }

    /// Set it.
    pub fn set_pc(&mut self, pc: usize) {
        self.storage.pc = pc;
    }

    /// How deep the operand stack is, in slots.
    pub fn sp(&self) -> usize {
        self.storage.sp
    }

    /// Set it.
    pub fn set_sp(&mut self, sp: usize) {
        self.storage.sp = sp;
    }

    /// Where the current frame begins.
    pub fn fp(&self) -> usize {
        self.storage.fp
    }

    /// Set it.
    pub fn set_fp(&mut self, fp: usize) {
        self.storage.fp = fp;
    }

    /// Where the innermost unwind record sits, or `usize::MAX` for none.
    pub fn mp(&self) -> usize {
        self.storage.mp
    }

    /// Set it.
    pub fn set_mp(&mut self, mp: usize) {
        self.storage.mp = mp;
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
                values: Value::UNBOUND.bits(),
                function: Value::NIL.bits(),
                operands: Value::NIL.bits(),
                pc: 0,
                sp: 0,
                fp: 0,
                mp: usize::MAX,
                mode: 0,
                soft_limit: words,
                hard_limit: words.saturating_mul(HARD_LIMIT_MULTIPLE),
                exhausted: false,
                collections: 0,
                sequence: 0,
                gensym_counter: 0,
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
            let nil_name = mc.alloc_constant_string("NIL");
            let t_name = mc.alloc_constant_string("T");

            for (s, name) in [(nil, nil_name), (t, t_name)] {
                mc.set_slot(s, symbol::NAME, name);
                mc.set_slot(s, symbol::FUNCTION, Value::UNBOUND);
                mc.set_slot(s, symbol::PLIST, Value::NIL);
                mc.set_slot(s, symbol::SPECIAL, Value::NIL);
                mc.set_slot(
                    s,
                    symbol::HOME,
                    Value::fixnum(home::CURRENT).expect("small"),
                );
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
        } else if self.is_bootstrapped() && self.wants_collection() {
            // The end of a mutation is the *only* place a collection may
            // happen, so it is the only place that can answer the heap's
            // request. Leaving that to callers made it a rule in prose: the
            // REPL remembered, `install` did not, and three separate failures
            // this phase were a mutation that filled the heap because nobody
            // had collected since the last one.
            //
            // Sound for the same reason torture mode is: the mutation is
            // over, so by the branding in this module no `Value` is alive
            // outside it.
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
        self.collect_with_padding(self.torture_padding());
    }

    /// Collect, laying the survivors out with no padding before them.
    ///
    /// What [`crate::image::save`] uses. Torture mode pads the front of the
    /// dynamic area by a varying amount so that no object can land where it
    /// was, which is exactly the right thing to do to a *running* heap and
    /// exactly the wrong thing to write to a file: the padding would make two
    /// saves of one world differ, and the image would carry a few words of
    /// poison for no reason.
    ///
    /// This is not merely cosmetic. It is what lets a loaded heap put its
    /// `dynamic_start` back at `static_end` without having to be told where
    /// the objects begin — and the first version, which did not do this,
    /// produced images that loaded fine in ordinary builds and failed
    /// verification under torture, with a forwarding pointer apparently left
    /// behind at word 24. There was no such pointer; the verifier was reading
    /// padding as an object because the two halves disagreed about where the
    /// heap started.
    pub(crate) fn collect_for_image(&mut self) {
        self.collect_with_padding(0);
    }

    fn collect_with_padding(&mut self, pad: usize) {
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
        self.storage.values = forward(from, &mut to, static_end, self.storage.values);
        self.storage.function = forward(from, &mut to, static_end, self.storage.function);
        self.storage.operands = forward(from, &mut to, static_end, self.storage.operands);

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

    /// The live words, for [`crate::image`] to write out.
    pub(crate) fn words(&self) -> &[u64] {
        &self.storage.words
    }

    /// Everything about the heap that is *not* in its words.
    ///
    /// The boundary that keeps the image format in one module: `heap` knows
    /// which fields exist, `image` knows what order they go on disk in, and
    /// neither knows the other's business. The same arrangement as the
    /// instruction encoding, where only the assembler and the VM's fetch know
    /// the byte layout.
    pub(crate) fn scalars(&self) -> Scalars {
        let s = &self.storage;
        Scalars {
            sequence: s.sequence,
            root: s.root,
            obarray: s.obarray,
            control: s.control,
            env: s.env,
            value: s.value,
            stack: s.stack,
            unwind: s.unwind,
            values: s.values,
            function: s.function,
            operands: s.operands,
            pc: s.pc as u64,
            sp: s.sp as u64,
            fp: s.fp as u64,
            mp: s.mp as u64,
            mode: s.mode as u64,
            gensym_counter: s.gensym_counter,
            static_end: s.static_end as u64,
        }
    }

    /// Rebuild a heap from an image's words and scalars.
    ///
    /// Note what is *not* restored. `soft_limit` and `hard_limit` are the
    /// host's policy rather than the image's property — an image moved from a
    /// desktop to a phone should obey the phone's limits, not carry the
    /// desktop's — so they come from the caller. `collections` starts again at
    /// zero, because it counts this process's collections. `exhausted` is not
    /// saved at all: a heap that could not allocate is not a heap worth
    /// preserving, and restoring the flag would halt the evaluator for a
    /// reason that no longer applies.
    ///
    /// `dynamic_start` becomes `static_end`, deliberately dropping any torture
    /// padding the saving process had. The padding exists to move objects
    /// about between collections; it is not part of the world.
    pub(crate) fn from_image(words: Vec<u64>, s: Scalars, soft_limit: usize) -> Heap {
        Heap {
            storage: Storage {
                words,
                spare: Vec::new(),
                root: s.root,
                obarray: s.obarray,
                control: s.control,
                env: s.env,
                value: s.value,
                stack: s.stack,
                unwind: s.unwind,
                values: s.values,
                function: s.function,
                operands: s.operands,
                pc: s.pc as usize,
                sp: s.sp as usize,
                fp: s.fp as usize,
                mp: s.mp as usize,
                mode: s.mode as u8,
                soft_limit,
                hard_limit: soft_limit.saturating_mul(HARD_LIMIT_MULTIPLE),
                exhausted: false,
                collections: 0,
                sequence: s.sequence,
                gensym_counter: s.gensym_counter,
                static_end: s.static_end as usize,
                dynamic_start: s.static_end as usize,
            },
        }
    }

    /// Which point in the world's history this heap is at.
    ///
    /// Incremented by each [`crate::image::save`], so two images of the same
    /// world can be ordered without comparing their contents.
    pub fn sequence(&self) -> u64 {
        self.storage.sequence
    }

    /// Move to the next point in history. Called by [`crate::image::save`].
    pub(crate) fn advance_sequence(&mut self) {
        self.storage.sequence += 1;
    }
}

/// Everything about a heap that is not one of its words.
///
/// One `u64` per field, in the order [`crate::image`] writes them. A
/// deliberately dumb struct: adding a field here is adding a field to the
/// image format, which is exactly the coupling that should be impossible to
/// do by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scalars {
    pub sequence: u64,
    pub root: u64,
    pub obarray: u64,
    pub control: u64,
    pub env: u64,
    pub value: u64,
    pub stack: u64,
    pub unwind: u64,
    pub values: u64,
    pub function: u64,
    pub operands: u64,
    pub pc: u64,
    pub sp: u64,
    pub fp: u64,
    pub mp: u64,
    pub mode: u64,
    pub gensym_counter: u64,
    pub static_end: u64,
}

impl Scalars {
    /// How many `u64`s a `Scalars` occupies in an image header.
    pub const WORDS: usize = 18;

    /// In the order they are written. Changing this order changes the format.
    pub fn to_words(self) -> [u64; Scalars::WORDS] {
        [
            self.sequence,
            self.root,
            self.obarray,
            self.control,
            self.env,
            self.value,
            self.stack,
            self.unwind,
            self.values,
            self.function,
            self.operands,
            self.pc,
            self.sp,
            self.fp,
            self.mp,
            self.mode,
            self.gensym_counter,
            self.static_end,
        ]
    }

    /// The inverse, and the only other place that knows the order.
    pub fn from_words(w: &[u64]) -> Scalars {
        Scalars {
            sequence: w[0],
            root: w[1],
            obarray: w[2],
            control: w[3],
            env: w[4],
            value: w[5],
            stack: w[6],
            unwind: w[7],
            values: w[8],
            function: w[9],
            operands: w[10],
            pc: w[11],
            sp: w[12],
            fp: w[13],
            mp: w[14],
            mode: w[15],
            gensym_counter: w[16],
            static_end: w[17],
        }
    }
}

#[cfg(test)]
#[path = "unit/heap.rs"]
mod tests;
