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

use crate::value::{Invariant, Value};
use core::marker::PhantomData;

/// How many words a cons cell occupies.
pub const CONS_WORDS: usize = 2;

/// The heap's storage. Separated from [`Heap`] so a [`Mutation`] can borrow it
/// without borrowing the collection machinery.
#[derive(Debug, Default)]
struct Storage {
    words: Vec<u64>,
    /// The one persistent root: everything that must survive hangs off it.
    root: u64,
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
}

/// The heap.
///
/// Work on it through [`Heap::mutate`]; collect between mutations.
#[derive(Debug, Default)]
pub struct Heap {
    storage: Storage,
}

impl Heap {
    /// A new, empty heap whose root is `nil`.
    pub fn new() -> Heap {
        Heap {
            storage: Storage {
                words: Vec::new(),
                root: Value::NIL.bits(),
            },
        }
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
