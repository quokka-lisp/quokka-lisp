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

mod scalars;
mod verify;

pub(crate) use scalars::Scalars;
pub use verify::{Inconsistency, InterpretedCode};

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
    /// Where the symbol belongs. Since packages (#43), one of four things:
    ///
    /// | | |
    /// | --- | --- |
    /// | `nil` | interned nowhere — `gensym`, `make-symbol` |
    /// | `0` ([`crate::heap::home::CURRENT`]) | in the system table, not yet adopted into a package |
    /// | `1` ([`crate::heap::home::KEYWORD`]) | a keyword; the `KEYWORD` package is implied |
    /// | a package object | adopted into that package by the tier-1 library |
    ///
    /// Keywords keep the integer because Rust must recognise them — for
    /// `keywordp`, the printer's colon, a value cell bound to itself — without
    /// knowing what a Lisp package object is. Everything else is the library's
    /// business.
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

/// The slots of the symbol cache ([`Mutation::cached_symbol`]).
pub mod symbol_cache {
    /// `UNSUPPLIED MARKER`, which `lambda_list::bind` gives a parameter that
    /// got no argument.
    pub const UNSUPPLIED: usize = 0;
    /// The first of the arithmetic fallbacks, one slot each from here.
    pub const FALLBACKS: usize = 1;
    /// `%SIGNAL-MACHINE-ERROR`, which the machine calls with an error of
    /// its own once the library defines it (#125). After the twelve
    /// fallbacks.
    pub const MACHINE_ERROR: usize = 13;
    /// How many slots the cache has.
    pub const SLOTS: usize = 16;
}

/// How many buckets the obarray's index starts with
/// ([`Mutation::find_system_symbol`]).
const INDEX_BUCKETS: usize = 1024;

/// A hash of a symbol's name, for the obarray's index: FNV-1a over the
/// characters (Fowler, Noll and Vo), as `hash.rs` uses.
fn name_hash(chars: impl Iterator<Item = char>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in chars {
        h ^= c as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Which of COUNT buckets a hash belongs in. The high half is folded in
/// first: a multiplication only carries upwards, so the low bits alone
/// depend on little more than the last character.
fn bucket_of(hash: u64, count: usize) -> usize {
    ((hash ^ (hash >> 32)) % count as u64) as usize
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
    /// A structure (#73): its layout, the Lisp object describing its type,
    /// in slot 0, then its slots. Tier 2's class instances will be the same.
    Instance = 4,
    /// Buckets and metadata.
    HashTable = 5,
    /// A compiled function: code, constants, parameters. See `crate::code`.
    Function = 10,
    /// An array that is not simple: the array holding its elements, the
    /// offset into it, the fill pointer, the dimensions, and whether it is
    /// displaced and adjustable (#67). See `crate::array`.
    ArrayHeader = 11,
    /// A signalled condition: type, message, data. See `crate::condition`.
    ///
    /// Its own type rather than an `Instance`, which it was at first — and
    /// `Instance` is also what `fresh_marker` allocates for a `block` or
    /// `catch` to unwind to. Every marker therefore looked like a condition,
    /// so a `return-from` passing through a `handler-case` was caught by it.
    /// Reusing a type for two purposes is reusing a *name*, and the collector
    /// does not care but everything above it does.
    Condition = 9,
    /// What a `block`, `catch` or `tagbody` unwinds to: no slots, equal only
    /// to itself. An `Instance` until structures needed that type (#73).
    Marker = 12,
    /// An exact ratio of two integers, in lowest terms (#59): the numerator
    /// and the denominator, both traced. See `crate::ratio`.
    Ratio = 13,
    /// An instance of a class (#124): its wrapper and its slot vector. See
    /// `crate::instance`.
    StandardInstance = 14,
    /// An instance that can be called, as a generic function is: its
    /// function, its wrapper and its slot vector. See `crate::instance`.
    FuncallableInstance = 15,
    /// A process (#34): a computation's saved registers, its state and its
    /// place in the ring of processes. See `crate::process`.
    Process = 16,
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
    /// structure does not change. See `crate::env`.
    Environment = 7,
    /// One pending step of the evaluator: what to do once a value arrives.
    ///
    /// Slot 0 is which kind, as a fixnum; the rest depend on it. One object
    /// type rather than one per kind, so adding a special operator does not
    /// mean adding a variant here. See `crate::eval`.
    Frame = 6,
    /// A string that can be written: one Unicode scalar per word. **Not
    /// traced.**
    ///
    /// Wide on purpose. Any character can be stored at any index without the
    /// string changing size, which a packed string cannot promise: putting a
    /// `λ` into eight-to-a-word Latin-1 would need more words, and an object
    /// cannot grow where it stands. See [`ObjectType::PackedString`].
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
    ///
    /// Since #44 only a constant string with a character above Latin-1 is
    /// stored this way; the rest are [`ObjectType::PackedString`].
    ConstantString = 67,
    /// A constant string whose characters are all Latin-1 (codes below 256),
    /// eight to a word. **Not traced.**
    ///
    /// Raw slot 0 is the character count, because the header counts words and
    /// five characters and eight both fill one. The bytes follow from slot 1,
    /// least significant first, as [`Mutation::raw_byte`] reads them. A byte
    /// *is* the character's code — Latin-1 is Unicode's first 256 scalars — so
    /// reading character `i` is one shift, and stays O(1). UTF-8 would be as
    /// small for ASCII but would make `char` a scan from the start.
    ///
    /// Only constants are packed, because only a constant's characters are
    /// all known when it is made and can never change. That is every symbol
    /// name and literal, which is what fills an image (#44). Lisp cannot tell
    /// the two constant layouts apart: both are `string`, both refuse writes.
    PackedString = 68,
    /// A simple bit vector: the count in raw slot 0, then the bits, 64 to a
    /// word (#67). **Not traced.** See `crate::array`.
    BitVector = 70,
    /// A simple array of element type `nil`: the count in raw slot 0 and
    /// nothing else, since no object is of type `nil` (#77). **Not traced.**
    /// CLHS 15.1.2.2 makes it a string; see `crate::array`.
    NilVector = 71,
    /// An IEEE 754 double, its 64 bits in raw slot 0. **Not traced.**
    ///
    /// Boxed because a double needs all 64 bits of a word and three are the
    /// tag's (#17, decision 1). See `crate::float`.
    DoubleFloat = 69,
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
        matches!(
            self,
            ObjectType::String | ObjectType::ConstantString | ObjectType::PackedString
        )
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
            11 => ArrayHeader,
            12 => Marker,
            13 => Ratio,
            14 => StandardInstance,
            15 => FuncallableInstance,
            16 => Process,
            64 => String,
            65 => Code,
            66 => Bignum,
            67 => ConstantString,
            68 => PackedString,
            69 => DoubleFloat,
            70 => BitVector,
            71 => NilVector,
            _ => return None,
        })
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
    /// What the running computation has asked the host for, or `nil`.
    ///
    /// A request record (see `crate::host::request`) while the machine is
    /// blocked or holding an answer it has not yet taken. A register because
    /// the running computation owns it; in phase 3 each process holds its own
    /// and this becomes the running one's, cached (#34). And in the heap
    /// rather than on the host's side so that an image saved while waiting
    /// reloads still waiting (#35).
    Request,
    /// The running process (#34), or `nil` in a world that has never made
    /// one: all the registers above are that process's, cached, and a
    /// switch stores them into it and loads another's (`crate::process`).
    Process,
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
    /// The pending host request, or `nil`. A root like the rest.
    request: u64,
    /// The running process, or `nil`. A root like the rest, and the way to
    /// every other process, round the ring.
    process: u64,
    /// System symbols the machine needs by name, remembered once found
    /// (see [`Mutation::cached_symbol`]): a vector, or `nil` until the first
    /// lookup. A root, so the collector forwards it; but only a cache, so an
    /// image does not keep it — `collect_for_image` drops it first, and a
    /// loaded world fills it again as it goes.
    symbol_cache: u64,
    /// The index of the obarray ([`Mutation::find_system_symbol`]): a hash
    /// from a name to the system symbols with that name, so that interning
    /// does not walk the whole table. A vector, or `nil` until the first
    /// lookup. A root and a cache exactly as `symbol_cache` is: everything
    /// in it is in the obarray, which an image does keep.
    symbol_index: u64,
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
    /// Above this many words a collection is wanted: the soft limit, or twice
    /// what the last collection left alive if that is more. See
    /// [`Heap::wants_collection`].
    trigger: usize,
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
        // The allocation still happens. Refusing it would mean every
        // allocator returning a Result, and every caller handling a case
        // that cannot be recovered from locally. Instead the flag is set
        // and the evaluator halts at its next step, so the overshoot is one
        // step's worth of words — a handful, *provided* whoever takes a size
        // from a program asked `Mutation::fits` first (#91).
        self.note_growth(words);
        self.words.resize(at + words, 0);
        at
    }

    /// The **store barrier**: called before a reference is stored into the
    /// object or cons at word `holder`, and empty.
    ///
    /// It is here for the collector this heap will have and does not have
    /// yet (#86, decided 25 Nov 2026). A *generational* collector keeps new
    /// objects in a small nursery and collects only that, most of the time,
    /// on the observation that most objects die young. To do so it must find
    /// every reference *into* the nursery without scanning the old objects,
    /// and the only way an old object comes to hold one is by being written
    /// to after it was made. So every such write is noted, in a *remembered
    /// set*, and a nursery collection treats the noted objects as roots (the
    /// Garbage Collection Handbook, chapter 9; Ungar 1984). Noting the write
    /// is this function's whole job.
    ///
    /// Empty now, because Cheney's collector copies everything each time and
    /// needs nothing remembered, and placed now, because the alternative is
    /// to find every store in the machine again on the day the collector
    /// changes. There are exactly three: `set_car`, `set_cdr` and
    /// `set_slot`. A raw slot holds no reference, a fresh object's first
    /// contents are written to the youngest object there is, and the
    /// registers are roots, scanned whole at every collection.
    #[inline(always)]
    #[allow(clippy::unused_self)]
    fn note_store(&mut self, _holder: usize) {}

    /// Set the flag if `words` more would pass the hard limit.
    fn note_growth(&mut self, words: usize) {
        if self.words.len() + words - self.dynamic_start > self.hard_limit {
            self.exhausted = true;
        }
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

/// How much of the heap is in use, and not the heap: a `Mutation` borrows
/// every word there is, and printing them would be the whole world.
impl fmt::Debug for Mutation<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mutation")
            .field("used_words", &self.used_words())
            .field("exhausted", &self.storage.exhausted)
            .finish_non_exhaustive()
    }
}

impl<'h> Mutation<'h> {
    /// Allocate a cons cell.
    pub fn cons(&mut self, car: Value<'h>, cdr: Value<'h>) -> Value<'h> {
        // A cons is not allocated through `Storage::alloc`, and until #85 was
        // not counted either: a program that only consed never met the limit.
        self.storage.note_growth(CONS_WORDS);
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
        self.storage.note_store(at);
        self.storage.words[at] = new.bits();
    }

    /// Replace the cdr of a cons cell.
    pub fn set_cdr(&mut self, v: Value<'h>, new: Value<'h>) {
        let at = self.cell(v);
        self.storage.note_store(at);
        self.storage.words[at + 1] = new.bits();
    }

    /// The elements of a list and its tail (`nil` if proper), or `None` if
    /// the list is circular (#92).
    ///
    /// Any Rust loop that follows cdrs runs inside one step, where the budget
    /// cannot stop it — so it must stop itself. This is Brent's cycle
    /// detection (1980): a checkpoint that jumps ahead to the cursor at every
    /// power of two, and a cursor that meets it only if the list loops. One
    /// comparison a cell and no memory, where Floyd's hare reads every cell
    /// twice.
    pub fn list_elements(&self, list: Value<'h>) -> Option<(Vec<Value<'h>>, Value<'h>)> {
        let (mut out, mut cursor, mut checkpoint) = (Vec::new(), list, list);
        let (mut since, mut power) = (0usize, 1usize);
        while cursor.is_cons() {
            out.push(self.car(cursor));
            cursor = self.cdr(cursor);
            if cursor == checkpoint {
                return None;
            }
            since += 1;
            if since == power {
                (checkpoint, since, power) = (cursor, 0, power.saturating_mul(2));
            }
        }
        Some((out, cursor))
    }

    /// Would an object of `slots` slots fit under the hard limit (#91)?
    ///
    /// For a size that comes from Lisp: [`alloc`](Self::alloc) never refuses,
    /// so whoever takes a count from a program asks this first.
    pub fn fits(&self, slots: usize) -> bool {
        let used = self.storage.words.len() - self.storage.dynamic_start;
        slots <= MAX_OBJECT_SLOTS
            && slots
                .checked_add(HEADER_WORDS + used)
                .is_some_and(|words| words <= self.storage.hard_limit)
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
        let at = self.index(v);
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
        self.storage.note_store(at);
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

    /// Allocate a string that can be written.
    ///
    /// One Unicode scalar per word, which wastes most of each word. Constants
    /// are packed instead (#44); this stays wide so that a write never has to
    /// widen it. See [`ObjectType::String`].
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
        if kind == ObjectType::ConstantString && chars.iter().all(|&ch| (ch as u32) < 256) {
            let v = self.alloc(ObjectType::PackedString, 1 + chars.len().div_ceil(8));
            self.set_raw_slot(v, 0, chars.len() as u64);
            for (index, ch) in chars.iter().enumerate() {
                self.set_raw_byte(v, 8 + index, *ch as u8);
            }
            return v;
        }
        let v = self.alloc(kind, chars.len());
        for (index, ch) in chars.iter().enumerate() {
            self.set_raw_slot(v, index, *ch as u64);
        }
        v
    }

    /// How many characters a string holds — not how many words.
    ///
    /// # Panics
    /// If `v` is not a string.
    pub fn string_length(&self, v: Value<'h>) -> usize {
        let header = self.header(v);
        assert!(header.object_type.is_string(), "not a string");
        if header.object_type == ObjectType::PackedString {
            self.raw_slot(v, 0) as usize
        } else {
            header.slots
        }
    }

    /// Character `index` of a string. This and [`Self::string_length`] are
    /// the only two places that know there are two layouts.
    ///
    /// # Panics
    /// If `v` is not a string or `index` is past its end.
    pub fn string_char(&self, v: Value<'h>, index: usize) -> char {
        assert!(
            index < self.string_length(v),
            "index past the end of a string"
        );
        if self.header(v).object_type == ObjectType::PackedString {
            char::from(self.raw_byte(v, 8 + index))
        } else {
            char::from_u32(self.raw_slot(v, index) as u32).expect("a string holds characters")
        }
    }

    /// Read a string back out.
    ///
    /// # Panics
    /// If `v` is not a string.
    pub fn string_to_rust(&self, v: Value<'h>) -> String {
        (0..self.string_length(v))
            .map(|i| self.string_char(v, i))
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
        let mut chars = text.chars();
        (0..self.string_length(v)).all(|i| chars.next() == Some(self.string_char(v, i)))
            && chars.next().is_none()
    }

    /// The system symbol called `name`, found once and then remembered in
    /// slot `slot` of the symbol cache — for the names the machine itself
    /// needs on a hot path, where [`Self::intern`]'s walk of the whole symbol
    /// table on every call is far too slow: the unsupplied marker on each call
    /// with `&optional` or `&key` (#16 found it costing 50 µs a call), and
    /// the arithmetic fallbacks. The slots are fixed in [`symbol_cache`].
    pub fn cached_symbol(&mut self, slot: usize, name: &str) -> Value<'h> {
        // `nil` is a heap object too (a symbol in the static area), so the
        // empty cache is recognised as nil, not as "not an object".
        if Value::from_bits(self.storage.symbol_cache).is_nil() {
            let cache = self.alloc_vector(symbol_cache::SLOTS);
            self.storage.symbol_cache = cache.bits();
        }
        let cache = Value::from_bits(self.storage.symbol_cache);
        let known = self.slot(cache, slot);
        if !known.is_nil() {
            return known;
        }
        let symbol = self.intern(name);
        self.set_slot(cache, slot, symbol);
        symbol
    }

    /// Find or create the symbol with this name.
    ///
    /// Two reads of `foo` must produce the *same* object, or `eq` on symbols
    /// stops working and every lookup becomes a string comparison. That is what
    /// interning means, and the table it uses is the **obarray**.
    ///
    /// The obarray is a list, and was searched as one until #102: every
    /// keyword the reader met and every new symbol walked all of it. The
    /// list is still the table — what an image keeps and what the library
    /// adopts into packages — and [`Self::find_system_symbol`] now looks in
    /// a hash index beside it.
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
        if let Some(symbol) = self.find_system_symbol(name, home == home::KEYWORD) {
            return symbol;
        }
        let symbol = self.new_symbol(name);
        let wanted = Value::fixnum(home).expect("a namespace is a small number");
        self.set_slot(symbol, symbol::HOME, wanted);
        let entry = self.cons(symbol, Value::from_bits(self.storage.obarray));
        self.storage.obarray = entry.bits();
        self.index_symbol(symbol);
        symbol
    }

    /// The symbol called `name` in the system table, if there is one — the
    /// keyword if `keyword`, the ordinary symbol otherwise. Never creates.
    ///
    /// Since packages (#43) an ordinary symbol's home may be `0`, meaning not
    /// yet adopted, or the package the library adopted it into; both are in
    /// this table and both match. Only symbols the *Lisp* `intern` makes are
    /// outside it, which is what lets two packages each have a `FOO`.
    /// Keywords keep the small integer, because Rust has to recognise them
    /// without knowing what a package object is.
    ///
    /// Found through the index: the bucket NAME hashes to holds the few
    /// symbols whose names share that hash, newest first, which is the order
    /// the list had — a symbol that lost its home (`unintern`) stays in the
    /// table beside the one made to replace it, and the newer must win.
    pub fn find_system_symbol(&mut self, name: &str, keyword: bool) -> Option<Value<'h>> {
        let keyword_home = Value::fixnum(home::KEYWORD).expect("small");
        let unadopted = Value::fixnum(home::CURRENT).expect("small");
        let index = self.symbol_index();
        let buckets = self.header(index).slots - 1;
        let mut cursor = self.slot(index, 1 + bucket_of(name_hash(name.chars()), buckets));
        while !cursor.is_nil() {
            let symbol = self.car(cursor);
            let home = self.slot(symbol, symbol::HOME);
            let right_namespace = if keyword {
                home == keyword_home
            } else {
                home == unadopted || (home.is_object() && home != Value::NIL)
            };
            if right_namespace && self.string_is(self.slot(symbol, symbol::NAME), name) {
                return Some(symbol);
            }
            cursor = self.cdr(cursor);
        }
        None
    }

    /// The obarray's index, built from the list if this world has none yet:
    /// a fresh heap, or one just loaded from an image.
    ///
    /// A vector: the number of symbols in slot 0, then the buckets, each a
    /// list of symbols. This is *separate chaining*, the same shape as
    /// `hash.rs`'s tables; it is not one of those because they are keyed by
    /// a Lisp object and this is asked with a Rust `&str`, before any string
    /// object exists.
    ///
    /// Rule 9: building it reads every symbol once, in one step, when a
    /// world starts — what a single failed lookup used to cost every time.
    /// Growing it does the same, at each doubling, as a hash table does.
    fn symbol_index(&mut self) -> Value<'h> {
        let known = Value::from_bits(self.storage.symbol_index);
        if !known.is_nil() {
            return known;
        }
        let (symbols, _) = self
            .list_elements(Value::from_bits(self.storage.obarray))
            .expect("the obarray is a list that ends");
        self.build_symbol_index(&symbols)
    }

    /// An index of SYMBOLS, which are newest first, with twice the buckets
    /// they need: room to grow into before the next doubling.
    fn build_symbol_index(&mut self, symbols: &[Value<'h>]) -> Value<'h> {
        let mut buckets = INDEX_BUCKETS;
        while buckets < symbols.len() * 2 {
            buckets *= 2;
        }
        let index = self.alloc_vector(1 + buckets);
        self.storage.symbol_index = index.bits();
        self.set_slot(index, 0, Value::fixnum(0).expect("small"));
        // Oldest first, since each goes on the front of its bucket.
        for &symbol in symbols.iter().rev() {
            self.index_symbol(symbol);
        }
        Value::from_bits(self.storage.symbol_index)
    }

    /// Add SYMBOL, just put on the obarray, to the index, doubling the index
    /// when it holds more symbols than buckets.
    fn index_symbol(&mut self, symbol: Value<'h>) {
        let index = self.symbol_index();
        let buckets = self.header(index).slots - 1;
        let count = self.slot(index, 0).as_fixnum().expect("a count") as usize;
        if count >= buckets {
            // Full: build a bigger one from the list, which already holds
            // SYMBOL, so the new index does too.
            let (symbols, _) = self
                .list_elements(Value::from_bits(self.storage.obarray))
                .expect("the obarray is a list that ends");
            self.build_symbol_index(&symbols);
            return;
        }
        let name = self.slot(symbol, symbol::NAME);
        let hash = name_hash((0..self.string_length(name)).map(|i| self.string_char(name, i)));
        let at = 1 + bucket_of(hash, buckets);
        let chain = self.slot(index, at);
        let chain = self.cons(symbol, chain);
        self.set_slot(index, at, chain);
        self.set_slot(index, 0, Value::fixnum(count as i64 + 1).expect("small"));
    }

    /// Every symbol in the system table, as the list it is kept in. For the
    /// library's adoption of them into packages (#43); not to be modified.
    pub fn system_symbols(&self) -> Value<'h> {
        Value::from_bits(self.storage.obarray)
    }

    /// A symbol called `name`, interned nowhere: no value, no function, home
    /// `nil`. What `gensym` and `make-symbol` both make.
    pub fn new_symbol(&mut self, name: &str) -> Value<'h> {
        let text = self.alloc_constant_string(name);
        let s = self.alloc(ObjectType::Symbol, symbol::SLOTS);
        self.set_slot(s, symbol::NAME, text);
        self.set_slot(s, symbol::VALUE, Value::UNBOUND);
        self.set_slot(s, symbol::FUNCTION, Value::UNBOUND);
        self.set_slot(s, symbol::HOME, Value::NIL);
        s
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
        // Interned nowhere, which is the point.
        self.new_symbol(&format!("G{n}"))
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
            Register::Request => self.storage.request,
            Register::Process => self.storage.process,
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
            Register::Request => &mut self.storage.request,
            Register::Process => &mut self.storage.process,
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

    /// Where a heap object's header is.
    fn index(&self, v: Value<'h>) -> usize {
        v.word_index().expect("not a reference into the heap")
    }

    /// Where a cons cell is. The tag is checked, not only that `v` points
    /// into the heap: any reference has a word index, and `car` of a vector
    /// read its header word as a value, which the collector then followed
    /// (#100). One comparison, on the accessors everything else trusts.
    fn cell(&self, v: Value<'h>) -> usize {
        assert!(v.is_cons(), "not a cons");
        v.word_index().expect("a cons has a word index")
    }

    /// Locate a slot, checking the bound and that the caller is using the right
    /// accessor for this object's kind.
    ///
    /// Mixing them up is how a collector ends up tracing the bytes of a string
    /// as if they were references, so it is a panic rather than a shrug.
    fn slot_at(&self, v: Value<'h>, index: usize, want_values: bool) -> usize {
        let at = self.index(v);
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
                request: Value::NIL.bits(),
                process: Value::NIL.bits(),
                symbol_cache: Value::NIL.bits(),
                symbol_index: Value::NIL.bits(),
                pc: 0,
                sp: 0,
                fp: 0,
                mp: usize::MAX,
                mode: 0,
                soft_limit: words,
                trigger: words,
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
    ///
    /// # The line moves (#85)
    ///
    /// "Past the soft limit" was the whole test once, and it has a cliff. A
    /// collection frees garbage; it cannot free what is alive. So a world
    /// whose *live* data is larger than the limit is past it immediately after
    /// collecting, and asks again at the end of the very next mutation — a
    /// full copy of the heap per slice, each freeing almost nothing. Measured:
    /// the same loop took 0.5 s with 3.5M words alive and 23.7 s with 4.5M,
    /// either side of a 4M-word limit.
    ///
    /// The cure is the ordinary one for a copying collector (Jones, Hosking
    /// and Moss, *The Garbage Collection Handbook*, 2012, on sizing the
    /// heap): after each collection, ask again only when the heap has grown
    /// to twice what survived. The work a collection does is proportional to
    /// what is alive, and it now buys at least that much allocation before
    /// the next one, so collecting costs a constant per word allocated
    /// however large the world grows. The soft limit stays as the floor,
    /// which is what a small world wants.
    pub fn wants_collection(&self) -> bool {
        self.used_words() > self.storage.trigger
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

    const fn torture_padding(&self) -> usize {
        if TORTURE {
            // Varying, so a stale index cannot accidentally still be right.
            (self.storage.collections % 7) as usize + 1
        } else {
            0
        }
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
        // The symbol cache is not part of the world, and what it happens to
        // hold must not make two images of the same world differ.
        self.storage.symbol_cache = Value::NIL.bits();
        self.storage.symbol_index = Value::NIL.bits();
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
        self.storage.request = forward(from, &mut to, static_end, self.storage.request);
        self.storage.process = forward(from, &mut to, static_end, self.storage.process);
        self.storage.symbol_cache = forward(from, &mut to, static_end, self.storage.symbol_cache);
        self.storage.symbol_index = forward(from, &mut to, static_end, self.storage.symbol_index);

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
                    // Everything is moving, so a table that hashed a key by
                    // its address must rehash before its next use (hash.rs).
                    if header.object_type == ObjectType::HashTable {
                        let stamp = scan + HEADER_WORDS + crate::hash::STAMP;
                        to[stamp] = crate::hash::moved(to[stamp]);
                    }
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

        // Where the next collection is wanted: see `wants_collection`.
        let live = self.storage.words.len() - self.storage.dynamic_start;
        self.storage.trigger = self.storage.soft_limit.max(live.saturating_mul(2));
        // And give back what a spike left behind. Both spaces keep their
        // allocation between collections, which is right while the heap is
        // steady and wrong after one huge object: without this, a single
        // `(make-array 200000000)` held 3 GB for the life of the process.
        let keep = self.storage.words.len() + self.storage.trigger.saturating_mul(2);
        self.storage.words.shrink_to(keep);
        self.storage.spare.clear();
        self.storage.spare.shrink_to(keep);

        if TORTURE {
            self.verify()
                .expect("the heap is inconsistent after a collection");
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

    /// The header at `word_index`, for the test that walks a heap.
    #[cfg(test)]
    pub(crate) fn header_at(&self, word_index: usize) -> Option<Header> {
        Header::decode(self.storage.words[word_index])
    }

    /// The live words, for [`crate::image`] to write out.
    pub(crate) fn words(&self) -> &[u64] {
        &self.storage.words
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

#[cfg(test)]
#[path = "unit/heap.rs"]
mod tests;
