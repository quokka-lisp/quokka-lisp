//! Tests for [`crate::heap`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

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
    for t in [
        ObjectType::String,
        ObjectType::Code,
        ObjectType::Bignum,
        ObjectType::ConstantString,
        ObjectType::PackedString,
    ] {
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

/// A constant of Latin-1 characters packs eight to a word, behind a count
/// word: twelve characters take two words of bytes plus the count, not
/// twelve. A character between 128 and 255 is still one byte. Issue #44.
#[test]
fn a_latin1_constant_is_packed() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let s = mc.alloc_constant_string("caf\u{e9} au lait");
        assert_eq!(mc.header(s).object_type, ObjectType::PackedString);
        assert_eq!(mc.header(s).slots, 1 + 2);
        assert_eq!(mc.string_length(s), 12);
        assert_eq!(mc.string_char(s, 3), '\u{e9}');
        assert_eq!(mc.string_to_rust(s), "caf\u{e9} au lait");
        assert!(mc.string_is(s, "caf\u{e9} au lait"));
        assert!(!mc.string_is(s, "caf\u{e9} au lai"));
        let empty = mc.alloc_constant_string("");
        assert_eq!(mc.string_length(empty), 0);
        assert!(mc.string_is(empty, ""));
    });
}

/// A constant with a character Latin-1 lacks stays one scalar per word, and
/// a writable string is always wide, so a write never has to widen it.
#[test]
fn only_latin1_constants_are_packed() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let wide = mc.alloc_constant_string("\u{3bb}x");
        assert_eq!(mc.header(wide).object_type, ObjectType::ConstantString);
        assert_eq!(mc.string_to_rust(wide), "\u{3bb}x");
        let mutable = mc.alloc_string("abc");
        assert_eq!(mc.header(mutable).object_type, ObjectType::String);
        assert_eq!(mc.string_length(mutable), 3);
    });
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
