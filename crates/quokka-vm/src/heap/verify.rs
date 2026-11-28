//! Checking a heap: is it one, and what is in it?
//!
//! Part of [`crate::heap`], and a child module of it so that it sees the
//! storage. Nothing here changes a heap. [`Heap::verify`] is what torture
//! mode runs after every collection and what stands between an image file
//! and the machine; the rest are walks over every object that answer one
//! question each.

use super::*;

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
    /// A header word whose type byte names no object type. Decoded leniently
    /// it was taken for a cons, by the verifier and the collector both (#99).
    UnknownType {
        /// Where the header is.
        at: usize,
    },
    /// A reference whose tag disagrees with what it points at: a cons tag on
    /// an object with a header, or the reverse. `car` of one reads a header
    /// as a value.
    WrongTag {
        /// The object holding the reference, or `usize::MAX` for a root.
        from: usize,
        /// Where it points.
        target: usize,
    },
    /// An object with fewer slots than the code that reads its type relies
    /// on, or a count inside it that its words cannot hold.
    TooSmall {
        /// Where the object begins.
        at: usize,
    },
    /// A register that does not fit the heap it came with: a stack pointer
    /// past the stack, a program counter past the code.
    BadRegister {
        /// Which one.
        which: &'static str,
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
            Inconsistency::UnknownType { at } => {
                write!(f, "object at {at} has a type this machine does not know")
            }
            Inconsistency::WrongTag { from, target } => {
                write!(f, "object at {from} refers to {target} with the wrong tag")
            }
            Inconsistency::TooSmall { at } => {
                write!(f, "object at {at} is too small for its type")
            }
            Inconsistency::BadRegister { which } => {
                write!(f, "the {which} register does not fit the heap")
            }
        }
    }
}

/// What [`Heap::interpreted_code`] found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InterpretedCode {
    /// Closures and macros with a body that is not compiled code.
    pub count: usize,
    /// The symbols whose function cell holds one.
    pub names: Vec<String>,
}

impl Heap {
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
    ///
    /// It is also what stands between an image file and the machine (#99).
    /// A checksum says the bytes are the bytes that were written; only this
    /// says they were a heap. So it checks what the rest of the code takes
    /// on trust: that every header names a type, that a reference's tag
    /// matches what it points at, that an object has the slots its type's
    /// readers index, and that the registers fit the stack and the code they
    /// index. Anything less, and a damaged or crafted image loads and then
    /// panics at its first `intern`.
    pub fn verify(&self) -> Result<(), Inconsistency> {
        let words = &self.storage.words;
        // What starts at each word: nothing, a cons, or an object.
        const CONS: u8 = 1;
        const OBJECT: u8 = 2;
        let mut starts = vec![0u8; words.len()];
        if self.storage.static_end > words.len() || self.storage.dynamic_start > words.len() {
            return Err(Inconsistency::BadRegister {
                which: "static-area",
            });
        }

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
            let size = match Header::decode(words[at]) {
                Some(header) => {
                    starts[at] = OBJECT;
                    HEADER_WORDS.saturating_add(header.slots)
                }
                None if Value::from_bits(words[at]).is_header() => {
                    return Err(Inconsistency::UnknownType { at });
                }
                None => {
                    starts[at] = CONS;
                    CONS_WORDS
                }
            };
            if size > words.len() - at {
                return Err(Inconsistency::ObjectOverrunsHeap { at, size });
            }
            if !self.big_enough(at) {
                return Err(Inconsistency::TooSmall { at });
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
            if starts[target] == 0 {
                return Err(Inconsistency::ReferenceIntoNowhere { from, target });
            }
            if value.is_cons() != (starts[target] == CONS) {
                return Err(Inconsistency::WrongTag { from, target });
            }
            Ok(())
        };

        // Every root, which until #99 was seven of the eleven: the collector
        // could have failed to forward one of the other four and torture
        // mode would not have noticed.
        let s = &self.storage;
        for root in [
            s.root, s.obarray, s.control, s.env, s.value, s.stack, s.unwind, s.values, s.function,
            s.operands, s.request, s.process,
        ] {
            check(root, usize::MAX)?;
        }
        self.registers_fit()?;

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

    /// Every closure and macro whose body is **not** compiled code: what a
    /// built image must not contain (#33).
    ///
    /// A linear walk of the whole heap, the way [`Heap::verify`] walks it.
    /// Run after a collection, everything it finds is live — a copying
    /// collector leaves nothing else behind — so a count of zero means the
    /// world has no interpreted code left in it, reachable or otherwise.
    ///
    /// The names are the symbols whose *function cell* holds one, which is
    /// where the bootstrap would have left it; an interpreted closure held
    /// somewhere else counts but has no name to give.
    pub fn interpreted_code(&self) -> InterpretedCode {
        let words = &self.storage.words;
        let type_of = |word: u64| -> Option<ObjectType> {
            let target = Value::from_bits(word).word_index()?;
            Header::decode(*words.get(target)?).map(|h| h.object_type)
        };
        let interpreted = |at: usize| -> bool {
            match Header::decode(words[at]) {
                Some(h) if matches!(h.object_type, ObjectType::Closure | ObjectType::Macro) => {
                    let body = words[at + HEADER_WORDS + crate::closure::BODY];
                    type_of(body) != Some(ObjectType::Function)
                }
                _ => false,
            }
        };
        let mut found = InterpretedCode::default();
        let mut at = 0usize;
        while at < words.len() {
            if at == self.storage.static_end {
                at = self.storage.dynamic_start;
                if at >= words.len() {
                    break;
                }
            }
            let size = match Header::decode(words[at]) {
                Some(header) => {
                    if interpreted(at) {
                        found.count += 1;
                    }
                    if header.object_type == ObjectType::Symbol {
                        let cell = Value::from_bits(words[at + HEADER_WORDS + symbol::FUNCTION]);
                        if let Some(target) = cell.word_index() {
                            if target < words.len() && interpreted(target) {
                                found.names.push(self.symbol_name_at(at));
                            }
                        }
                    }
                    HEADER_WORDS + header.slots
                }
                None => CONS_WORDS,
            };
            at += size;
        }
        found.names.sort();
        found
    }

    /// A symbol's name, read straight out of the words — for reports made
    /// outside a mutation, where there is no `Value` to ask.
    fn symbol_name_at(&self, at: usize) -> String {
        let words = &self.storage.words;
        let Some(name) = Value::from_bits(words[at + HEADER_WORDS + symbol::NAME]).word_index()
        else {
            return String::from("?");
        };
        let Some(header) = Header::decode(words[name]) else {
            return String::from("?");
        };
        if header.object_type == ObjectType::PackedString {
            let first = name + HEADER_WORDS + 1;
            return (0..words[first - 1] as usize)
                .map(|i| char::from((words[first + i / 8] >> ((i % 8) * 8)) as u8))
                .collect();
        }
        (0..header.slots)
            .filter_map(|i| char::from_u32(words[name + HEADER_WORDS + i] as u32))
            .collect()
    }

    /// Has the object at `at` the slots its type's readers rely on? They
    /// index by constant — a symbol's function cell, a table's stamp, which
    /// the collector writes without asking — so too few is a read or a write
    /// outside the object.
    fn big_enough(&self, at: usize) -> bool {
        let words = &self.storage.words;
        let Some(header) = Header::decode(words[at]) else {
            return true;
        };
        // A count kept in the first raw word, and how many it may be at most.
        let counted = |per_word: usize| {
            header.slots >= 1
                && words[at + HEADER_WORDS] <= ((header.slots - 1) as u64) * per_word as u64
        };
        match header.object_type {
            ObjectType::Symbol => header.slots > symbol::HOME,
            ObjectType::HashTable => header.slots >= crate::hash::SLOTS,
            ObjectType::Function => header.slots >= crate::code::function::SLOTS,
            ObjectType::ArrayHeader => header.slots >= crate::array::SLOTS,
            ObjectType::Condition => header.slots >= crate::condition::SLOTS,
            ObjectType::Closure | ObjectType::Macro => header.slots > crate::closure::ENV,
            ObjectType::Environment => header.slots >= crate::env::FIRST_PAIR,
            ObjectType::Ratio => header.slots == 2,
            ObjectType::StandardInstance => header.slots == crate::instance::STANDARD_SLOTS,
            ObjectType::FuncallableInstance => header.slots == crate::instance::FUNCALLABLE_SLOTS,
            ObjectType::Process => header.slots == crate::process::SLOTS,
            ObjectType::DoubleFloat | ObjectType::NilVector => header.slots >= 1,
            ObjectType::PackedString => counted(8),
            ObjectType::BitVector => counted(64),
            _ => true,
        }
    }

    /// Do the machine's registers fit what they index? The stack, frame and
    /// mark pointers are positions in the operand stack, and the program
    /// counter one in the running function's code.
    fn registers_fit(&self) -> Result<(), Inconsistency> {
        let s = &self.storage;
        // The live set is a background process's when one was running as
        // the image was saved, and is then resumed as it stands, as every
        // saved set is: those are held to what resuming needs.
        let resumed = {
            let p = Value::from_bits(s.process);
            p.is_object()
                && p.word_index().is_some_and(|at| {
                    Header::decode(s.words[at]).map(|h| h.object_type) == Some(ObjectType::Process)
                        && Value::from_bits(s.words[at + HEADER_WORDS + crate::process::MAIN])
                            .is_nil()
                })
        };
        let live = RegisterSet {
            operands: s.operands,
            sp: s.sp,
            fp: s.fp,
            mp: s.mp,
            function: s.function,
            pc: s.pc,
            request: s.request,
        };
        self.register_set_fits(&live, resumed, true)?;
        // Every process's saved registers are loaded into these when it
        // runs (#34), so each set must fit as the live one must; and the
        // ring must be processes all the way round.
        let bad = |which| Err(Inconsistency::BadRegister { which });
        let words = &s.words;
        let header_at = |word: u64| -> Option<Header> {
            let v = Value::from_bits(word);
            let at = v.is_object().then(|| v.word_index()).flatten()?;
            Header::decode(words[at])
        };
        let running = Value::from_bits(s.process);
        if !running.is_nil()
            && header_at(s.process).map(|h| h.object_type) != Some(ObjectType::Process)
        {
            return bad("process");
        }
        for at in self.objects_of(ObjectType::Process) {
            let slot = |n: usize| words[at + HEADER_WORDS + n];
            let place = |n: usize| -> Option<usize> {
                let v = Value::from_bits(slot(n)).as_fixnum()?;
                if v == -1 && n == crate::process::MP {
                    return Some(usize::MAX);
                }
                usize::try_from(v).ok()
            };
            if header_at(slot(crate::process::NEXT)).map(|h| h.object_type)
                != Some(ObjectType::Process)
                || Value::from_bits(slot(crate::process::STATE))
                    .as_fixnum()
                    .is_none()
            {
                return bad("process");
            }
            let (Some(sp), Some(fp), Some(mp), Some(pc)) = (
                place(crate::process::SP),
                place(crate::process::FP),
                place(crate::process::MP),
                place(crate::process::PC),
            ) else {
                return bad("process-registers");
            };
            let saved = RegisterSet {
                operands: slot(crate::process::OPERANDS),
                sp,
                fp,
                mp,
                function: slot(crate::process::FUNCTION),
                pc,
                request: slot(crate::process::REQUEST),
            };
            // A process that is alive and not the running one will be
            // switched to and run on its stack: it must have one (#171).
            // The running process's saved slots are stale, and the main
            // one may never have been switched out.
            let alive = Value::from_bits(slot(crate::process::STATE)).as_fixnum()
                == Some(crate::process::ALIVE);
            let main = !Value::from_bits(slot(crate::process::MAIN)).is_nil();
            let is_running = running.is_object() && running.word_index() == Some(at);
            if alive && !main && !is_running && Value::from_bits(saved.operands).is_nil() {
                return bad("process-stack");
            }
            self.register_set_fits(&saved, true, !is_running)?;
        }
        Ok(())
    }

    /// Do a set of registers fit what they index: the stack, frame and
    /// mark pointers in the operand stack, the program counter in the
    /// function's code?
    ///
    /// CHAINS says whether to follow the records and frames on the stack
    /// too (`chains_fit`): not for the saved registers of the process that
    /// is running, which are what it had when it was last switched out and
    /// describe a stack that has moved on since. They are written over
    /// before they are ever loaded.
    fn register_set_fits(
        &self,
        set: &RegisterSet,
        resumed: bool,
        chains: bool,
    ) -> Result<(), Inconsistency> {
        let RegisterSet {
            operands,
            sp,
            fp,
            mp,
            function,
            pc,
            request,
        } = *set;
        let s = &self.storage;
        let object = |word: u64| -> Option<(usize, Header)> {
            let v = Value::from_bits(word);
            let at = v.is_object().then(|| v.word_index()).flatten()?;
            Some((at, Header::decode(s.words[at])?))
        };
        let bad = |which| Err(Inconsistency::BadRegister { which });
        // No stack at all is `nil`: a world in which nothing compiled has
        // run yet. `Vm::start` makes one before anything is popped.
        // Then nothing is on it, so the machine never has a stack pointer
        // or a frame outside a stack vector (#130: a return trusts that).
        if Value::from_bits(operands).is_nil() {
            if sp != 0 || fp != 0 || mp != usize::MAX {
                return bad("stack-pointer");
            }
        } else if let Some((_, stack)) = object(operands) {
            if stack.object_type != ObjectType::Vector {
                return bad("operand-stack");
            }
            if sp > stack.slots || fp > sp {
                return bad("stack-pointer");
            }
            // A record is read whole, from its first slot to its last.
            if mp != usize::MAX && mp + crate::vm::mark::SIZE > sp {
                return bad("mark-pointer");
            }
            if chains {
                self.chains_fit(operands, sp, fp, mp)?;
            }
        } else {
            // Neither nil nor an object: nothing could push onto it.
            return bad("operand-stack");
        }
        // A set that will be resumed as it stands (#34) runs its function
        // and looks at its request at once, before anything resets them: a
        // stack means a compiled function, and a request is a record.
        if resumed {
            let compiled =
                object(function).map(|(_, f)| f.object_type) == Some(ObjectType::Function);
            if !Value::from_bits(operands).is_nil() && !compiled {
                return bad("function");
            }
            if !Value::from_bits(request).is_nil() {
                let record = object(request).filter(|(_, h)| {
                    h.object_type == ObjectType::Vector && h.slots == crate::host::request::SLOTS
                });
                let Some((at, _)) = record else {
                    return bad("request");
                };
                let state = s.words[at + HEADER_WORDS + crate::host::request::STATE];
                let symbol = object(state).map(|(_, h)| h.object_type) == Some(ObjectType::Symbol);
                if !symbol {
                    return bad("request");
                }
            }
        }
        if let Some((at, f)) = object(function) {
            if f.object_type == ObjectType::Function {
                let length = Value::from_bits(
                    s.words[at + HEADER_WORDS + crate::code::function::CODE_LENGTH],
                )
                .as_fixnum();
                if !length.is_some_and(|n| n >= 0 && pc as u64 <= n as u64) {
                    return bad("program-counter");
                }
            }
        }
        Ok(())
    }

    /// Do the unwind records and the call frames on a stack lead where the
    /// machine will follow them (#171)?
    ///
    /// The machine walks both chains trusting each link: `unbind_all` when a
    /// host call begins, every `signal` and `throw` looking for its record,
    /// every return. An image made by hand could point a record's link past
    /// the end of the stack (a panic), at itself (a loop no step budget
    /// covers), or put a number where a return expects a function. So each
    /// record must lie whole below the one before it and be of a kind the
    /// machine knows, and each frame must hold a count of arguments that
    /// fits under it and a caller's frame below it; where the machine will
    /// carry on *in* a function, that must be a function.
    fn chains_fit(
        &self,
        operands: u64,
        sp: usize,
        fp: usize,
        mp: usize,
    ) -> Result<(), Inconsistency> {
        use crate::vm::{frame, mark};
        let s = &self.storage;
        let bad = |which| Err(Inconsistency::BadRegister { which });
        let stack = Value::from_bits(operands);
        let Some(base) = stack.is_object().then(|| stack.word_index()).flatten() else {
            return bad("operand-stack");
        };
        let slot = |i: usize| Value::from_bits(s.words[base + HEADER_WORDS + i]);
        let index = |v: Value<'_>| v.as_fixnum().and_then(|n| usize::try_from(n).ok());
        let is_function = |v: Value<'_>| {
            v.is_object()
                && v.word_index()
                    .and_then(|at| Header::decode(s.words[at]))
                    .map(|h| h.object_type)
                    == Some(ObjectType::Function)
        };

        let mut top = sp;
        let mut at = mp;
        while at != usize::MAX {
            if at + mark::SIZE > top {
                return bad("mark-chain");
            }
            let Some(kind) = slot(at + mark::KIND).as_fixnum() else {
                return bad("mark-chain");
            };
            match kind {
                // These carry on in a function at a frame: both are read.
                mark::BLOCK | mark::CATCH | mark::TAG | mark::PROTECT | mark::HANDLER => {
                    let frame_fits = index(slot(at + mark::FP)).is_some_and(|f| f <= sp);
                    let resumes = index(slot(at + mark::PC)).is_some();
                    if !frame_fits || !resumes || !is_function(slot(at + mark::FUNCTION)) {
                        return bad("mark-chain");
                    }
                }
                // A pause and a binding keep other things in those slots.
                mark::PAUSED | mark::SPECIAL => {}
                _ => return bad("mark-chain"),
            }
            let prev = slot(at + mark::PREV);
            top = at;
            at = match prev.as_fixnum() {
                Some(-1) => usize::MAX,
                // Strictly below: the chain ends, and cannot come round.
                Some(n) if usize::try_from(n).is_ok_and(|p| p + mark::SIZE <= top) => n as usize,
                _ => return bad("mark-chain"),
            };
        }

        let mut at = fp;
        while at != 0 {
            if at < frame::SIZE || at > sp {
                return bad("frame-chain");
            }
            let header = at - frame::SIZE;
            let arguments = index(slot(header + frame::ARGUMENT_COUNT));
            let caller = index(slot(header + frame::FP));
            let function = slot(header + frame::FUNCTION);
            let (Some(arguments), Some(caller)) = (arguments, caller) else {
                return bad("frame-chain");
            };
            if arguments > header
                || caller >= at
                || index(slot(header + frame::PC)).is_none()
                || !(function.is_nil() || is_function(function))
            {
                return bad("frame-chain");
            }
            at = caller;
        }
        Ok(())
    }

    /// Where every object of type `wanted` begins. For a pass over all of
    /// one kind — the image loader verifying every compiled function.
    pub(crate) fn objects_of(&self, wanted: ObjectType) -> Vec<usize> {
        let words = &self.storage.words;
        let mut found = Vec::new();
        let mut at = 0usize;
        while at < words.len() {
            if at == self.storage.static_end {
                at = self.storage.dynamic_start;
                if at >= words.len() {
                    break;
                }
            }
            at += match Header::decode(words[at]) {
                Some(header) => {
                    if header.object_type == wanted {
                        found.push(at);
                    }
                    HEADER_WORDS + header.slots
                }
                None => CONS_WORDS,
            };
        }
        found
    }
}

/// One computation's registers, as `register_set_fits` checks them: the
/// live set, or one a process saved (#34).
#[derive(Clone, Copy)]
struct RegisterSet {
    operands: u64,
    sp: usize,
    fp: usize,
    mp: usize,
    function: u64,
    pc: usize,
    request: u64,
}
