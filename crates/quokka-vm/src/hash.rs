//! Hash tables, keyed by what a value *is* rather than where it lives.
//!
//! # The problem this module is shaped around
//!
//! A hash table needs a number for each key, and the obvious number for an
//! object is its address. Chapter 6's collector **moves objects**, so an
//! address-based hash is correct until the next collection and silently wrong
//! afterwards — and "silently" is the word that matters. Nothing signals; a
//! lookup simply stops finding something that is still there.
//!
//! Four answers were weighed (see `docs/design.md` and issue #22): hash
//! contents only; store a stable hash word in every hashable object; have the
//! collector mark live tables stale and rehash them lazily, which is what SBCL
//! does; or pin hashed objects where the collector cannot move them.
//!
//! Quokka hashes **contents** wherever an object has contents that stand for
//! it: numbers, characters, symbols, and strings in an `equal` table. What
//! was blocked without hash tables was content-keyed: the obarray is keyed by
//! string, and the compiler's tables are keyed by symbol.
//!
//! # Identity keys (step Q, #64)
//!
//! Every other object — a cons, a vector, a function — is the same key only as
//! itself, and has nothing to hash but its address. Until step Q such keys were
//! refused, a divergence from SBCL under rule 6; `*print-circle*`, which must
//! notice a cons it has met before, made them necessary. They are hashed by
//! address, and the third answer of #22 keeps that honest: a table that holds
//! one says so in its [`STAMP`] slot, the collector marks such a table stale
//! as it moves it (`moved`), and the next use rehashes it. An `equal` table
//! still refuses a cons, which `equal` compares by contents.
//!
//! | Test | Keys hashed by content | Keys hashed by address |
//! | --- | --- | --- |
//! | `eq`, `eql` | numbers, characters, symbols | everything else |
//! | `equal` | the above, and strings | everything else but conses |
//!
//! Note that symbols are safe to hash by name even though they are heap
//! objects, because interning (chapter 8) guarantees one symbol per name per
//! namespace. The name is the identity.
//!
//! # Shape
//!
//! A table is four slots: a vector of buckets, a count, the test, and the stamp. Each
//! bucket is an association list of `(key . value)` conses — the simplest
//! thing that works, and a chain short enough not to matter once the table
//! grows. All of it is ordinary traced storage, so the collector needs to know
//! nothing about hash tables beyond following their slots.
//!
//! # Rule 9
//!
//! Hashing a string reads every character, so the cost grows with the *data*.
//! A table that grows rehashes every entry in one step. Both are bounded by
//! the heap and neither is short; this is the same debt as `append` and
//! `%make-vector`, written down in the same place rather than discovered
//! later.

use crate::error::EvalError;
use crate::heap::{Mutation, ObjectType};
use crate::value::Value;

/// The buckets: a vector whose elements are association lists.
pub const BUCKETS: usize = 0;
/// How many entries the table holds, as a fixnum.
pub const COUNT: usize = 1;
/// The test, as the symbol `EQ`, `EQL` or `EQUAL`.
pub const TEST: usize = 2;
/// How the keys were hashed: a fixnum, `BY_CONTENT`, `BY_ADDRESS` or
/// `STALE` (step Q, #64).
pub const STAMP: usize = 3;
/// How many slots a hash table has.
pub const SLOTS: usize = 4;

/// Every key was hashed by its content, which no collection changes.
const BY_CONTENT: u64 = stamp(0);
/// Some key was hashed by its address, and nothing has moved since.
const BY_ADDRESS: u64 = stamp(1);
/// Some key was hashed by its address, and the collector has moved it since:
/// the table must be rehashed before it is next used.
const STALE: u64 = stamp(2);

/// The word for a small fixnum, at compile time (a `match`, since the
/// minimum Rust version cannot call `Option::expect` in a constant).
const fn stamp(n: i64) -> u64 {
    match Value::fixnum(n) {
        Some(v) => v.bits(),
        None => panic!("a stamp is a small fixnum"),
    }
}

/// The collector calls this for every hash table it scans, with the word in
/// its [`STAMP`] slot, and stores what it returns. A copying collector moves
/// every live object, so a table that hashed any key by address is wrong from
/// now on — and saying so here, where the move happens, is the one place that
/// cannot be forgotten: images (saved after a collection), torture mode and
/// ordinary running all pass through it. This is what SBCL does, and the
/// third answer #22 weighed.
pub(crate) fn moved(stamp: u64) -> u64 {
    if stamp == BY_ADDRESS {
        STALE
    } else {
        stamp
    }
}

/// How many buckets a new table starts with.
///
/// Small, because most tables are small and growing is cheap. A compiler's
/// symbol table will outgrow it immediately and that is fine.
const INITIAL_BUCKETS: usize = 8;

/// Grow when there is more than this many entries per bucket on average.
///
/// One is the usual choice for chaining: at that point the average successful
/// lookup walks about half a cell.
const LOAD_FACTOR: usize = 1;

/// Make an empty table with the given test.
pub fn make<'h>(mc: &mut Mutation<'h>, test: Value<'h>) -> Result<Value<'h>, EvalError> {
    if !(mc.symbol_is(test, "EQ") || mc.symbol_is(test, "EQL") || mc.symbol_is(test, "EQUAL")) {
        return Err(EvalError::WrongType {
            name: "MAKE-HASH-TABLE",
            wanted: "a test of EQ, EQL or EQUAL",
        });
    }
    let buckets = mc.alloc_vector(INITIAL_BUCKETS);
    let table = mc.alloc(ObjectType::HashTable, SLOTS);
    mc.set_slot(table, BUCKETS, buckets);
    mc.set_slot(table, COUNT, Value::fixnum(0).expect("small"));
    mc.set_slot(table, TEST, test);
    mc.set_slot(table, STAMP, Value::from_bits(BY_CONTENT));
    Ok(table)
}

/// Is this a hash table?
pub fn is_table<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::HashTable
}

fn check_table<'h>(mc: &Mutation<'h>, v: Value<'h>, name: &'static str) -> Result<(), EvalError> {
    if is_table(mc, v) {
        Ok(())
    } else {
        Err(EvalError::WrongType {
            name,
            wanted: "a hash table",
        })
    }
}

/// A number for `key`, or an error saying this key cannot be hashed.
///
/// Fixnums and characters are immediate values, so the word *is* the content.
/// A symbol is hashed by its name, which interning makes as good as identity.
/// A string is hashed by its characters, and only in an `equal` table —
/// putting a string in an `eql` table would mean comparing string objects by
/// identity, which is what this module cannot do.
fn hash_of<'h>(mc: &Mutation<'h>, key: Value<'h>, equal: bool) -> Result<u64, EvalError> {
    // FNV-1a, which is short, has no magic beyond two constants, and is
    // perfectly adequate for a table that is not under attack. Nothing here
    // needs a cryptographic or collision-resistant hash.
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x1000_0000_01b3;
    fn mix(mut h: u64, byte: u64) -> u64 {
        h ^= byte;
        h = h.wrapping_mul(PRIME);
        h
    }

    if key.as_fixnum().is_some() || key.as_character().is_some() {
        return Ok(mix(OFFSET, key.bits()));
    }
    if mc.is_symbol(key) {
        let mut h = mix(OFFSET, 1);
        for ch in mc.symbol_name(key).chars() {
            h = mix(h, ch as u64);
        }
        // A keyword and an ordinary symbol share a name and are different
        // symbols (chapter 8), so the namespace has to go into the hash.
        return Ok(mix(h, if mc.is_keyword(key) { 2 } else { 3 }));
    }
    // A bignum by its value, so two equal ones (different objects) meet.
    if crate::bignum::is(mc, key) {
        return Ok(crate::bignum::hash(mc, key, mix(OFFSET, 5), mix));
    }
    // A double by its bits, which is `eql`'s test: 0.0 and -0.0 differ.
    if crate::float::is(mc, key) {
        return Ok(mix(mix(OFFSET, 6), mc.raw_slot(key, 0)));
    }
    // `equal` compares strings and bit vectors element by element, a fill
    // pointer's worth of them (#67), so that is what is hashed.
    if let Some((bits, text)) = equal.then(|| crate::array::equal_text(mc, key)).flatten() {
        let mut h = mix(OFFSET, if bits { 7 } else { 4 });
        for ch in text.chars() {
            h = mix(h, ch as u64);
        }
        return Ok(h);
    }
    // `equal` compares conses by their contents, which an address cannot
    // stand for, so an `equal` table still refuses them.
    if equal && key.is_cons() {
        return Err(EvalError::WrongType {
            name: "GETHASH",
            wanted: "a key other than a cons in an EQUAL table",
        });
    }
    // Anything else is the same key only as itself, so its address will do,
    // for as long as the collector leaves it where it is (see `moved`).
    Ok(mix(OFFSET, key.bits()))
}

/// Was this key hashed by its address? A string is, in an `eq` or `eql`
/// table, where two strings are the same key only if they are one object.
fn by_address<'h>(mc: &Mutation<'h>, key: Value<'h>, equal: bool) -> bool {
    !(key.as_fixnum().is_some()
        || key.as_character().is_some()
        || mc.is_symbol(key)
        || crate::bignum::is(mc, key)
        || crate::float::is(mc, key)
        || (equal && crate::array::equal_text(mc, key).is_some()))
}

/// Make the table right before using it: rehashed if the collector has moved
/// a key hashed by address since it was last hashed.
fn refresh<'h>(mc: &mut Mutation<'h>, table: Value<'h>) -> Result<(), EvalError> {
    if mc.slot(table, STAMP).bits() == STALE {
        let size = mc.header(mc.slot(table, BUCKETS)).slots;
        rehash(mc, table, size)?;
        mc.set_slot(table, STAMP, Value::from_bits(BY_ADDRESS));
    }
    Ok(())
}

/// Are these the same key, under this table's test?
fn same_key<'h>(mc: &Mutation<'h>, a: Value<'h>, b: Value<'h>, equal: bool) -> bool {
    if a == b {
        return true;
    }
    if crate::bignum::is(mc, a) && crate::bignum::is(mc, b) {
        return crate::bignum::compare(mc, a, b) == Some(std::cmp::Ordering::Equal);
    }
    if crate::float::is(mc, a) && crate::float::is(mc, b) {
        return mc.raw_slot(a, 0) == mc.raw_slot(b, 0);
    }
    if equal {
        if let (Some(x), Some(y)) = (
            crate::array::equal_text(mc, a),
            crate::array::equal_text(mc, b),
        ) {
            return x == y;
        }
    }
    false
}

fn uses_equal<'h>(mc: &Mutation<'h>, table: Value<'h>) -> bool {
    mc.symbol_is(mc.slot(table, TEST), "EQUAL")
}

/// The `(key . value)` cell for `key`, or nil.
fn find<'h>(mc: &Mutation<'h>, table: Value<'h>, key: Value<'h>) -> Result<Value<'h>, EvalError> {
    let equal = uses_equal(mc, table);
    let hash = hash_of(mc, key, equal)?;
    let buckets = mc.slot(table, BUCKETS);
    let count = mc.header(buckets).slots;
    let mut chain = mc.slot(buckets, hash as usize % count);
    while chain.is_cons() {
        let entry = mc.car(chain);
        if same_key(mc, mc.car(entry), key, equal) {
            return Ok(entry);
        }
        chain = mc.cdr(chain);
    }
    Ok(Value::NIL)
}

/// The value stored for `key`, or `missing` if there is none.
///
/// Common Lisp's `gethash` returns a second value saying whether the key was
/// present. Quokka has no multiple values yet, so the caller supplies what
/// absence should look like — which is `gethash`'s optional third argument,
/// and covers everything except telling a stored nil from a missing key.
pub fn get<'h>(
    mc: &mut Mutation<'h>,
    table: Value<'h>,
    key: Value<'h>,
    missing: Value<'h>,
) -> Result<Value<'h>, EvalError> {
    check_table(mc, table, "GETHASH")?;
    refresh(mc, table)?;
    let entry = find(mc, table, key)?;
    if entry.is_cons() {
        Ok(mc.cdr(entry))
    } else {
        Ok(missing)
    }
}

/// Store `value` under `key`, replacing any entry already there.
pub fn put<'h>(
    mc: &mut Mutation<'h>,
    table: Value<'h>,
    key: Value<'h>,
    value: Value<'h>,
) -> Result<Value<'h>, EvalError> {
    check_table(mc, table, "SET-GETHASH")?;
    refresh(mc, table)?;
    let entry = find(mc, table, key)?;
    if entry.is_cons() {
        mc.set_cdr(entry, value);
        return Ok(value);
    }

    let equal = uses_equal(mc, table);
    let hash = hash_of(mc, key, equal)?;
    let buckets = mc.slot(table, BUCKETS);
    let bucket_count = mc.header(buckets).slots;
    let index = hash as usize % bucket_count;

    if by_address(mc, key, equal) {
        mc.set_slot(table, STAMP, Value::from_bits(BY_ADDRESS));
    }
    let cell = mc.cons(key, value);
    let chain = mc.slot(buckets, index);
    let chain = mc.cons(cell, chain);
    mc.set_slot(buckets, index, chain);

    let count = mc.slot(table, COUNT).as_fixnum().expect("a count") + 1;
    mc.set_slot(table, COUNT, Value::fixnum(count).expect("small"));

    if count as usize > bucket_count * LOAD_FACTOR {
        rehash(mc, table, bucket_count * 2)?;
    }
    Ok(value)
}

/// Remove `key`, and say whether it was there.
pub fn remove<'h>(
    mc: &mut Mutation<'h>,
    table: Value<'h>,
    key: Value<'h>,
) -> Result<bool, EvalError> {
    check_table(mc, table, "REMHASH")?;
    refresh(mc, table)?;
    let equal = uses_equal(mc, table);
    let hash = hash_of(mc, key, equal)?;
    let buckets = mc.slot(table, BUCKETS);
    let index = hash as usize % mc.header(buckets).slots;

    // Rebuilt rather than spliced: the chain is short by construction, and a
    // rebuild has no case where a pointer is left pointing at a removed cell.
    let mut kept: Vec<Value<'h>> = Vec::new();
    let mut found = false;
    let mut chain = mc.slot(buckets, index);
    while chain.is_cons() {
        let entry = mc.car(chain);
        if !found && same_key(mc, mc.car(entry), key, equal) {
            found = true;
        } else {
            kept.push(entry);
        }
        chain = mc.cdr(chain);
    }
    if !found {
        return Ok(false);
    }
    let mut rebuilt = Value::NIL;
    for entry in kept.into_iter().rev() {
        rebuilt = mc.cons(entry, rebuilt);
    }
    mc.set_slot(buckets, index, rebuilt);
    let count = mc.slot(table, COUNT).as_fixnum().expect("a count") - 1;
    mc.set_slot(table, COUNT, Value::fixnum(count).expect("small"));
    Ok(true)
}

/// How many entries the table holds.
pub fn count<'h>(mc: &Mutation<'h>, table: Value<'h>) -> Result<Value<'h>, EvalError> {
    check_table(mc, table, "HASH-TABLE-COUNT")?;
    Ok(mc.slot(table, COUNT))
}

/// Every `(key . value)` cell, as a list.
///
/// Common Lisp iterates with `maphash`, which takes a function — and calling a
/// function is the evaluator's job, not a primitive's (chapter 15). So the
/// primitive hands back the entries and `maphash` is written in the prelude,
/// where calling things is ordinary.
pub fn entries<'h>(mc: &mut Mutation<'h>, table: Value<'h>) -> Result<Value<'h>, EvalError> {
    check_table(mc, table, "HASH-TABLE-ENTRIES")?;
    let buckets = mc.slot(table, BUCKETS);
    let bucket_count = mc.header(buckets).slots;
    let mut all: Vec<Value<'h>> = Vec::new();
    for i in 0..bucket_count {
        let mut chain = mc.slot(buckets, i);
        while chain.is_cons() {
            all.push(mc.car(chain));
            chain = mc.cdr(chain);
        }
    }
    let mut out = Value::NIL;
    for entry in all.into_iter().rev() {
        out = mc.cons(entry, out);
    }
    Ok(out)
}

/// Redistribute every entry into NEW_COUNT buckets: twice as many when the
/// table grows, as many as before when a collection has moved its keys.
///
/// Every entry is rehashed, so this is one long step — the rule 9 note in the
/// module documentation. Growing happens on an insertion that crosses the
/// load factor, so the cost is amortised, which is a different claim from
/// bounded; rehashing after a move happens at most once per collection.
fn rehash<'h>(mc: &mut Mutation<'h>, table: Value<'h>, new_count: usize) -> Result<(), EvalError> {
    let equal = uses_equal(mc, table);
    let old = mc.slot(table, BUCKETS);
    let old_count = mc.header(old).slots;

    let mut moving: Vec<Value<'h>> = Vec::new();
    for i in 0..old_count {
        let mut chain = mc.slot(old, i);
        while chain.is_cons() {
            moving.push(mc.car(chain));
            chain = mc.cdr(chain);
        }
    }

    let fresh = mc.alloc_vector(new_count);
    for entry in moving {
        let hash = hash_of(mc, mc.car(entry), equal)?;
        let index = hash as usize % new_count;
        let chain = mc.slot(fresh, index);
        let chain = mc.cons(entry, chain);
        mc.set_slot(fresh, index, chain);
    }
    mc.set_slot(table, BUCKETS, fresh);
    Ok(())
}
