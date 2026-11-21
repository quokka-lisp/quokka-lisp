//! The instruction set: opcodes, their encoding, and the objects that hold
//! them.
//!
//! `docs/bytecode.md` is the specification and this is its implementation;
//! book chapters 25 and 26 are the version that explains it. Where they
//! disagree, this file is the truth and the others are stale.
//!
//! # Why bytes rather than one word per instruction
//!
//! A `Code` object is a raw heap object, so a fixed 64-bit instruction word
//! would have cost nothing to decode. Packed bytes are about three times
//! smaller, and wasm32 is a 32-bit address space whose heap is halved again
//! by the collector's two semispaces. See #25 for the argument and
//! `docs/bytecode.md` for the arithmetic.
//!
//! # Why operand widths are fixed per opcode
//!
//! Because then an instruction's size depends only on its opcode. A jump's
//! distance can be computed as soon as the instructions between it and its
//! target are *chosen*, rather than after their sizes have settled — and with
//! variable-width operands those two things chase each other round a loop
//! until they agree, which assemblers call **branch relaxation**. Avoided
//! here by construction rather than by implementing it.

use crate::heap::{Mutation, ObjectType};
use crate::value::Value;

/// What an opcode's operands look like, and therefore how wide it is.
///
/// The instruction is one opcode byte plus this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operands {
    /// No operands: the instruction works on the stack alone.
    None,
    /// One unsigned byte: a count, a depth, a primitive index.
    U8,
    /// Two unsigned bytes: a scope depth and a slot within it.
    U8U8,
    /// Two bytes, unsigned: an index into the constants vector.
    U16,
    /// Two bytes, signed: a jump offset from the end of this instruction.
    S16,
    /// A constant index and a jump offset — what the block-like operators
    /// need, since each carries both a name and a place to resume.
    U16S16,
}

impl Operands {
    /// How many bytes follow the opcode.
    pub const fn width(self) -> usize {
        match self {
            Operands::None => 0,
            Operands::U8 => 1,
            Operands::U8U8 | Operands::U16 | Operands::S16 => 2,
            Operands::U16S16 => 4,
        }
    }
}

/// One operation.
///
/// The discriminants are the bytes an image contains, so **appending is a
/// format addition and renumbering is a breaking change** — the same contract
/// as the primitive table, and for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    // ---- pushing a value ----
    /// Push `nil`.
    Nil = 0,
    /// Push `t`.
    T = 1,
    /// Push a small fixnum written into the instruction.
    Fix = 2,
    /// Push `constants[k]`.
    Const = 3,
    /// Discard the top of the stack.
    Pop = 4,

    // ---- variables ----
    /// Push the variable `d` scopes out, slot `i`.
    Lref = 5,
    /// Pop into it.
    Lset = 6,
    /// Push the value cell of the symbol `constants[k]`.
    Gref = 7,
    /// Pop into it.
    Gset = 8,
    /// The function namespace: what `flet` and `labels` bind.
    Fref = 9,
    /// The global function cell.
    Gfref = 10,

    // ---- scopes ----
    /// Pop *n* values and push a variable scope binding them, where
    /// `constants[k]` is the vector of names and *n* is its length.
    Bind = 11,
    /// The same in the function namespace.
    Bindf = 12,
    /// Drop *n* scopes.
    Unbind = 13,

    // ---- calls ----
    /// Make a closure of the `Function` `constants[k]` over the current scope.
    Closure = 14,
    /// Pop *n* arguments and the callee beneath them, and call it.
    Call = 15,
    /// The same, reusing this frame.
    Tcall = 16,
    /// Call primitive `p` with *n* arguments, with no lookup.
    Prim = 17,
    /// Pop the frame; the value is left on the caller's stack.
    Ret = 18,

    // ---- branching ----
    /// Jump.
    Jmp = 19,
    /// Pop; jump if it is `nil`.
    JmpNil = 20,
    /// Pop; jump if it is not.
    JmpNotNil = 21,

    // ---- leaving early ----
    /// Push a block record with a fresh marker, bound under `constants[k]`.
    Block = 22,
    /// Pop a value and unwind to the marker `d` scopes out, slot `i`.
    Retfrom = 23,
    /// Pop a tag and push a catch record.
    Catch = 24,
    /// Pop a value and a tag, and unwind.
    Throw = 25,
    /// Push an unwind-protect record whose cleanup is at the offset.
    Protect = 26,
    /// Pop it and run the cleanup.
    Unprotect = 27,
    /// Push a handler record with the clauses `constants[k]`.
    Handler = 28,
    /// Drop the innermost unwind record: what a `block`, `catch`,
    /// `tagbody` or `handler-case` that was left normally does.
    ///
    /// Specified as `UNHANDLER`, for `handler-case` alone. Writing the VM
    /// showed that block, catch and tagbody need exactly the same
    /// instruction and nothing else, so there is one rather than four.
    Unmark = 29,
    /// A `tagbody` marker, emitted only when a tag can be reached from a
    /// closure that outlived it.
    Tag = 30,
    /// The non-local `go`.
    Go = 31,

    // ---- multiple values ----
    /// Pop *n*, push the primary, set the `Values` register.
    Values = 32,
    /// `multiple-value-call` over *n* argument forms.
    Mvcall = 33,
    /// Hold the current values across what follows.
    Mvsave = 34,
    /// Put them back.
    Mvrestore = 35,
}

/// Every opcode, in order, which is also the order of their byte values.
///
/// One table rather than three parallel `match` arms, so that adding an
/// opcode is one line and cannot be half-done. `every_opcode_is_in_the_table`
/// checks the correspondence the compiler cannot.
const TABLE: &[(Op, &str, Operands)] = &[
    (Op::Nil, "NIL", Operands::None),
    (Op::T, "T", Operands::None),
    (Op::Fix, "FIX", Operands::S16),
    (Op::Const, "CONST", Operands::U16),
    (Op::Pop, "POP", Operands::None),
    (Op::Lref, "LREF", Operands::U8U8),
    (Op::Lset, "LSET", Operands::U8U8),
    (Op::Gref, "GREF", Operands::U16),
    (Op::Gset, "GSET", Operands::U16),
    (Op::Fref, "FREF", Operands::U8U8),
    (Op::Gfref, "GFREF", Operands::U16),
    (Op::Bind, "BIND", Operands::U16),
    (Op::Bindf, "BINDF", Operands::U16),
    (Op::Unbind, "UNBIND", Operands::U8),
    (Op::Closure, "CLOSURE", Operands::U16),
    (Op::Call, "CALL", Operands::U8),
    (Op::Tcall, "TCALL", Operands::U8),
    (Op::Prim, "PRIM", Operands::U8U8),
    (Op::Ret, "RET", Operands::None),
    (Op::Jmp, "JMP", Operands::S16),
    (Op::JmpNil, "JMP-NIL", Operands::S16),
    (Op::JmpNotNil, "JMP-NOT-NIL", Operands::S16),
    (Op::Block, "BLOCK", Operands::U16S16),
    (Op::Retfrom, "RETFROM", Operands::U8U8),
    (Op::Catch, "CATCH", Operands::S16),
    (Op::Throw, "THROW", Operands::None),
    (Op::Protect, "PROTECT", Operands::S16),
    (Op::Unprotect, "UNPROTECT", Operands::None),
    (Op::Handler, "HANDLER", Operands::U16S16),
    (Op::Unmark, "UNMARK", Operands::None),
    (Op::Tag, "TAG", Operands::U16S16),
    (Op::Go, "GO", Operands::U8U8),
    (Op::Values, "VALUES", Operands::U8),
    (Op::Mvcall, "MVCALL", Operands::U8),
    (Op::Mvsave, "MVSAVE", Operands::None),
    (Op::Mvrestore, "MVRESTORE", Operands::None),
];

/// How many opcodes there are. `docs/bytecode.md` quotes this number.
pub const OPCODE_COUNT: usize = TABLE.len();

impl Op {
    /// The opcode with this byte value, or `None`.
    pub fn from_u8(byte: u8) -> Option<Op> {
        TABLE.get(byte as usize).map(|(op, _, _)| *op)
    }

    /// Its name, for a disassembly.
    pub fn name(self) -> &'static str {
        TABLE[self as usize].1
    }

    /// The opcode with this name, or `None`.
    ///
    /// How the Lisp compiler names an instruction. It emits `(JMP-NIL else)`
    /// and never learns that `JMP-NIL` is byte 20 — the encoding stays in
    /// this file, which is the whole of chapter 24's "the encoding is a
    /// boundary" and the reason the compiler cannot drift from the VM.
    pub fn from_name(name: &str) -> Option<Op> {
        TABLE
            .iter()
            .find(|(_, n, _)| *n == name)
            .map(|(op, _, _)| *op)
    }

    /// What its operands look like.
    pub fn operands(self) -> Operands {
        TABLE[self as usize].2
    }

    /// Total bytes, opcode included.
    pub fn width(self) -> usize {
        1 + self.operands().width()
    }
}

/// One decoded instruction.
///
/// `a` and `b` are the operands in the order they were written, widened to
/// `i32` so a signed jump offset and an unsigned constant index can share a
/// field without either being lied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    /// Which operation.
    pub op: Op,
    /// The first operand, or zero.
    pub a: i32,
    /// The second, or zero.
    pub b: i32,
    /// Where the next instruction starts.
    pub next: usize,
}

/// What a malformed instruction stream looks like.
///
/// Both cases mean a bug in whatever produced the bytes, not in the program
/// being run, which is why they are their own type rather than an `EvalError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeError {
    /// No opcode has this byte value.
    UnknownOpcode {
        /// The byte.
        byte: u8,
        /// Where it was.
        at: usize,
    },
    /// The instruction's operands run past the end of the code.
    Truncated {
        /// Where the instruction started.
        at: usize,
    },
}

/// Decode the instruction at `at` in `bytes`.
pub fn decode(bytes: &[u8], at: usize) -> Result<Decoded, CodeError> {
    let byte = *bytes.get(at).ok_or(CodeError::Truncated { at })?;
    let op = Op::from_u8(byte).ok_or(CodeError::UnknownOpcode { byte, at })?;
    let operands = op.operands();
    let end = at + 1 + operands.width();
    if end > bytes.len() {
        return Err(CodeError::Truncated { at });
    }
    let u8_at = |i: usize| bytes[at + 1 + i] as i32;
    let u16_at = |i: usize| (bytes[at + 1 + i] as i32) | ((bytes[at + 2 + i] as i32) << 8);
    let s16_at = |i: usize| u16_at(i) as i16 as i32;
    let (a, b) = match operands {
        Operands::None => (0, 0),
        Operands::U8 => (u8_at(0), 0),
        Operands::U8U8 => (u8_at(0), u8_at(1)),
        Operands::U16 => (u16_at(0), 0),
        Operands::S16 => (s16_at(0), 0),
        Operands::U16S16 => (u16_at(0), s16_at(2)),
    };
    Ok(Decoded {
        op,
        a,
        b,
        next: end,
    })
}

/// Decode the instruction at `at` in a `Code` object, without copying the
/// rest of it.
///
/// The VM does this once per step, so reading the whole function out as a
/// `Vec` would make every instruction cost the length of its function. At
/// most five bytes are ever needed, which is the widest instruction.
pub fn decode_in_heap<'h>(
    mc: &Mutation<'h>,
    code: Value<'h>,
    length: usize,
    at: usize,
) -> Result<Decoded, CodeError> {
    if at >= length {
        return Err(CodeError::Truncated { at });
    }
    let mut buffer = [0u8; MAX_INSTRUCTION];
    let available = (length - at).min(MAX_INSTRUCTION);
    for (i, slot) in buffer.iter_mut().enumerate().take(available) {
        *slot = mc.raw_byte(code, at + i);
    }
    match decode(&buffer[..available], 0) {
        Ok(d) => Ok(Decoded {
            next: at + d.next,
            ..d
        }),
        Err(CodeError::Truncated { .. }) => Err(CodeError::Truncated { at }),
        Err(CodeError::UnknownOpcode { byte, .. }) => Err(CodeError::UnknownOpcode { byte, at }),
    }
}

/// The widest an instruction can be: an opcode and four operand bytes.
pub const MAX_INSTRUCTION: usize = 5;

/// Append one instruction to `out`.
///
/// # Panics
/// If an operand does not fit the width the opcode fixes. That is a ceiling
/// the compiler must report rather than wrap — `docs/bytecode.md` lists them
/// all — and a panic here means something skipped the check.
pub fn encode(out: &mut Vec<u8>, op: Op, a: i32, b: i32) {
    out.push(op as u8);
    let mut u8_operand = |v: i32| {
        assert!(
            (0..=255).contains(&v),
            "{} operand {v} exceeds a byte",
            op.name()
        );
        out.push(v as u8);
    };
    match op.operands() {
        Operands::None => {
            assert_eq!((a, b), (0, 0), "{} takes no operands", op.name());
        }
        Operands::U8 => u8_operand(a),
        Operands::U8U8 => {
            u8_operand(a);
            u8_operand(b);
        }
        Operands::U16 => push_u16(out, op, a, false),
        Operands::S16 => push_u16(out, op, a, true),
        Operands::U16S16 => {
            push_u16(out, op, a, false);
            push_u16(out, op, b, true);
        }
    }
}

fn push_u16(out: &mut Vec<u8>, op: Op, v: i32, signed: bool) {
    if signed {
        assert!(
            (-32_768..=32_767).contains(&v),
            "{} operand {v} exceeds a signed 16-bit field",
            op.name()
        );
    } else {
        assert!(
            (0..=65_535).contains(&v),
            "{} operand {v} exceeds an unsigned 16-bit field",
            op.name()
        );
    }
    out.push(v as u8);
    out.push((v >> 8) as u8);
}

/// Render a whole instruction stream, one line per instruction.
///
/// Offsets are printed because jump operands are relative to the end of the
/// jump, which is unreadable without them.
pub fn disassemble(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let line = match decode(bytes, at) {
            Ok(d) => {
                let text = match d.op.operands() {
                    Operands::None => d.op.name().to_string(),
                    Operands::U8 | Operands::U16 => format!("{} {}", d.op.name(), d.a),
                    Operands::U8U8 => format!("{} {} {}", d.op.name(), d.a, d.b),
                    // A jump prints where it lands as well as how far it goes.
                    Operands::S16 => {
                        format!("{} {:+} -> {}", d.op.name(), d.a, d.next as i32 + d.a)
                    }
                    Operands::U16S16 => {
                        format!(
                            "{} {} {:+} -> {}",
                            d.op.name(),
                            d.a,
                            d.b,
                            d.next as i32 + d.b
                        )
                    }
                };
                at = d.next;
                text
            }
            Err(e) => {
                at = bytes.len();
                format!("<{e:?}>")
            }
        };
        out.push_str(&format!("{start:>4}  {line}\n"));
    }
    out
}

/// A compiled function: where the instructions, the constants and the shape
/// of the parameter list all live.
///
/// Two objects rather than one, because the traced/raw line (chapter 19) runs
/// between them: instruction bytes must **not** be traced, and constants must
/// be. A constant stored inside the `Code` object would be a symbol the
/// collector never forwarded.
pub mod function {
    /// The `Code` object: raw instruction bytes.
    pub const CODE: usize = 0;
    /// A vector of constants: symbols, lists, strings, nested functions.
    pub const CONSTANTS: usize = 1;
    /// The simple lambda list, as `lambda_list::rewrite` leaves it.
    pub const PARAMS: usize = 2;
    /// How deep this function's operand stack gets, in words.
    pub const MAX_STACK: usize = 3;
    /// The name, for the debugger and the disassembler.
    pub const NAME: usize = 4;
    /// How many of the `Code` object's bytes are instructions.
    ///
    /// The header counts whole words, so the last word is padded and the byte
    /// length has to be written down somewhere. Here.
    pub const CODE_LENGTH: usize = 5;
    /// How many slots a function has.
    pub const SLOTS: usize = 6;
}

/// Build a `Function` object.
pub fn make_function<'h>(
    mc: &mut Mutation<'h>,
    bytes: &[u8],
    constants: Value<'h>,
    params: Value<'h>,
    max_stack: usize,
    name: Value<'h>,
) -> Value<'h> {
    let code = mc.alloc_bytes(ObjectType::Code, bytes);
    let f = mc.alloc(ObjectType::Function, function::SLOTS);
    mc.set_slot(f, function::CODE, code);
    mc.set_slot(f, function::CONSTANTS, constants);
    mc.set_slot(f, function::PARAMS, params);
    mc.set_slot(
        f,
        function::MAX_STACK,
        Value::fixnum(max_stack as i64).expect("a stack depth is a small number"),
    );
    mc.set_slot(f, function::NAME, name);
    mc.set_slot(
        f,
        function::CODE_LENGTH,
        Value::fixnum(bytes.len() as i64).expect("a code length is a small number"),
    );
    f
}

/// Read a function's instructions back out.
pub fn code_bytes<'h>(mc: &Mutation<'h>, f: Value<'h>) -> Vec<u8> {
    let code = mc.slot(f, function::CODE);
    let len = mc
        .slot(f, function::CODE_LENGTH)
        .as_fixnum()
        .expect("a code length is a fixnum") as usize;
    (0..len).map(|i| mc.raw_byte(code, i)).collect()
}

/// What a function has to satisfy before the VM will run it.
///
/// # Why this exists
///
/// A stack machine is an agreement about what is where, and at run time
/// nothing enforces it. An instruction that leaves one item too many does not
/// fail — it makes every instruction after it quietly wrong, and the symptom
/// surfaces somewhere else entirely. Twice while building the VM a hand-
/// written program was wrong in exactly this way, and both times the error
/// named something several instructions from the cause:
///
/// - a jump of four bytes over a two-byte branch landed in the *middle* of a
///   later instruction, where the machine read the high half of an operand as
///   an opcode and ran on through nonsense;
/// - `MVSAVE` pushed where it should have replaced, so `multiple-value-call`
///   popped a fixed number of things off a stack with one too many on it and
///   reported that `(1 2)` is not a function.
///
/// Both are decidable **before the program runs**, by walking it once and
/// tracking how deep the stack would be. That is what this does, and it is
/// the JVM's answer to the same problem — one pass when a class is loaded
/// rather than a check on every pop. See the specification's §4.10.
///
/// # What it does not check
///
/// Types. A verified function will not corrupt the machine, but it may still
/// add a symbol to a string. Chapter 26's miscounted scope depth is also out
/// of reach: both the depth and the slot are in range, they just name the
/// wrong variable, and only the namespace check at run time notices.
pub mod verify {
    use super::*;

    /// Why a function was refused.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Error {
        /// The instruction stream itself is malformed.
        Malformed(CodeError),
        /// A jump lands somewhere that is not the start of an instruction —
        /// including past the end, and including the middle of one.
        JumpIntoNowhere {
            /// The jump.
            at: usize,
            /// Where it would have gone.
            target: usize,
        },
        /// An instruction would pop more than is there.
        Underflow {
            /// Where.
            at: usize,
            /// How deep the stack was.
            depth: usize,
            /// How much it wanted.
            wanted: usize,
        },
        /// Two paths reach the same instruction with the stack at different
        /// depths, so there is no one answer to "what is on the stack here".
        Inconsistent {
            /// Where they meet.
            at: usize,
            /// What one path said.
            had: usize,
            /// What the other said.
            then: usize,
        },
        /// The function needs a deeper stack than it declared, so a call
        /// would reserve too little.
        TooDeep {
            /// What it actually needs.
            needs: usize,
            /// What `max-stack` claims.
            declared: usize,
        },
        /// Control runs past the last instruction instead of leaving.
        FallsOffTheEnd {
            /// The last instruction reached.
            at: usize,
        },
        /// An instruction names a constant the function does not have.
        BadConstant {
            /// Where.
            at: usize,
            /// Which index it asked for.
            index: usize,
        },
        /// A `BIND` names a constant that is not a vector of names.
        BadBinding {
            /// Where.
            at: usize,
        },
    }

    /// How many stack slots an unwind record takes. Must agree with
    /// `crate::vm::mark::SIZE`, which `the_record_size_agrees` checks.
    pub const RECORD: usize = 7;

    /// What one instruction does to the stack, and where control can go next.
    struct Effect {
        /// Slots popped before anything is pushed.
        pops: usize,
        /// Slots pushed after.
        pushes: usize,
        /// Does the next instruction run?
        falls_through: bool,
        /// A jump target, and how deep the stack is on arrival **relative to
        /// the depth before this instruction**.
        ///
        /// Relative, not absolute, because the depth is not known here —
        /// which was worth getting wrong once: writing these as absolute
        /// made every jump claim to arrive at depth zero, and the first
        /// program with a jump in it was reported inconsistent at a label
        /// that was perfectly consistent.
        branch: Option<(usize, i32)>,
    }

    /// Check `function`, and every function among its constants.
    ///
    /// Returns the deepest the operand stack gets, which is what `max-stack`
    /// should have said.
    pub fn function<'h>(mc: &Mutation<'h>, f: Value<'h>) -> Result<usize, Error> {
        let bytes = code_bytes(mc, f);
        let constants = mc.slot(f, super::function::CONSTANTS);
        let count = if constants.is_object() {
            mc.header(constants).slots
        } else {
            0
        };
        let declared = mc
            .slot(f, super::function::MAX_STACK)
            .as_fixnum()
            .unwrap_or(0)
            .max(0) as usize;

        let needs = walk(mc, &bytes, constants, count)?;
        if needs > declared {
            return Err(Error::TooDeep { needs, declared });
        }

        // Nested functions are reached through CLOSURE, so verifying the
        // outermost one is not enough: a closure made at run time would
        // otherwise be the first thing nobody checked.
        for i in 0..count {
            let c = mc.slot(constants, i);
            if c.is_object() && mc.header(c).object_type == ObjectType::Function {
                self::function(mc, c)?;
            }
        }
        Ok(needs)
    }

    fn walk<'h>(
        mc: &Mutation<'h>,
        bytes: &[u8],
        constants: Value<'h>,
        count: usize,
    ) -> Result<usize, Error> {
        // Every instruction start, found by reading the stream once. A jump
        // may only land on one of these — which is the check that would have
        // caught a jump into the middle of an operand.
        let mut starts = std::collections::HashSet::new();
        let mut at = 0;
        while at < bytes.len() {
            starts.insert(at);
            at = decode(bytes, at).map_err(Error::Malformed)?.next;
        }

        let mut depth_at: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut work = vec![(0usize, 0usize)];
        let mut deepest = 0usize;

        while let Some((at, depth)) = work.pop() {
            if at >= bytes.len() {
                // Reached by falling off the end of the last instruction.
                return Err(Error::FallsOffTheEnd { at });
            }
            if let Some(&known) = depth_at.get(&at) {
                if known != depth {
                    return Err(Error::Inconsistent {
                        at,
                        had: known,
                        then: depth,
                    });
                }
                continue;
            }
            depth_at.insert(at, depth);

            let d = decode(bytes, at).map_err(Error::Malformed)?;
            let effect = effect_of(mc, &d, at, constants, count)?;
            if depth < effect.pops {
                return Err(Error::Underflow {
                    at,
                    depth,
                    wanted: effect.pops,
                });
            }
            let after = depth - effect.pops + effect.pushes;
            deepest = deepest.max(depth.max(after));

            if let Some((target, relative)) = effect.branch {
                if !starts.contains(&target) {
                    return Err(Error::JumpIntoNowhere { at, target });
                }
                let arrives = depth as i32 + relative;
                if arrives < 0 {
                    return Err(Error::Underflow {
                        at,
                        depth,
                        wanted: (-relative) as usize,
                    });
                }
                work.push((target, arrives as usize));
            }
            if effect.falls_through {
                work.push((d.next, after));
            }
        }
        Ok(deepest)
    }

    fn effect_of<'h>(
        mc: &Mutation<'h>,
        d: &Decoded,
        at: usize,
        constants: Value<'h>,
        count: usize,
    ) -> Result<Effect, Error> {
        let plain = |pops: usize, pushes: usize| Effect {
            pops,
            pushes,
            falls_through: true,
            branch: None,
        };
        let terminator = Effect {
            pops: 0,
            pushes: 0,
            falls_through: false,
            branch: None,
        };
        let constant = |index: i32| -> Result<usize, Error> {
            let i = index as usize;
            if index < 0 || i >= count {
                return Err(Error::BadConstant { at, index: i });
            }
            Ok(i)
        };
        // Where a jump operand lands: offsets are from the end of the
        // instruction, which is the convention the assembler relies on.
        let target = |offset: i32| (d.next as i64 + offset as i64).max(0) as usize;

        Ok(match d.op {
            Op::Nil | Op::T | Op::Fix => plain(0, 1),
            Op::Const | Op::Closure => {
                constant(d.a)?;
                plain(0, 1)
            }
            Op::Pop => plain(1, 0),
            Op::Lref | Op::Fref => plain(0, 1),
            Op::Lset => plain(1, 1),
            Op::Gref | Op::Gfref => {
                constant(d.a)?;
                plain(0, 1)
            }
            Op::Gset => {
                constant(d.a)?;
                plain(1, 1)
            }
            Op::Bind | Op::Bindf => {
                let i = constant(d.a)?;
                let names = mc.slot(constants, i);
                if !names.is_object() || mc.header(names).object_type != ObjectType::Vector {
                    return Err(Error::BadBinding { at });
                }
                plain(mc.header(names).slots, 0)
            }
            Op::Unbind => plain(0, 0),
            // A call pops its arguments and the callee beneath them.
            Op::Call | Op::Mvcall => plain(d.a as usize + 1, 1),
            Op::Prim => plain(d.b as usize, 1),
            Op::Values => plain(d.a as usize, 1),
            Op::Mvsave | Op::Mvrestore => plain(1, 1),
            // Control leaves and does not come back here.
            Op::Ret | Op::Tcall | Op::Unprotect | Op::Go => terminator,
            Op::Retfrom => Effect {
                pops: 1,
                ..terminator
            },
            Op::Throw => Effect {
                pops: 2,
                ..terminator
            },
            // Unchanged depth: a jump moves control, not the stack.
            Op::Jmp => Effect {
                falls_through: false,
                branch: Some((target(d.a), 0)),
                ..plain(0, 0)
            },
            // Both paths have popped the test by the time they get anywhere.
            Op::JmpNil | Op::JmpNotNil => Effect {
                pops: 1,
                pushes: 0,
                falls_through: true,
                branch: Some((target(d.a), -1)),
            },
            // A record occupies the stack, and the label it names is reached
            // by an unwind that drops the record and pushes one value — so
            // control arrives there one deeper than it was before the
            // instruction, not after it.
            Op::Block | Op::Tag | Op::Handler => {
                constant(d.a)?;
                Effect {
                    pops: 0,
                    pushes: RECORD,
                    falls_through: true,
                    branch: Some((target(d.b), 1)),
                }
            }
            // Pops the tag, and an unwind that finds it pushes one value in
            // its place — so the label is reached at the depth before this.
            Op::Catch => Effect {
                pops: 1,
                pushes: RECORD,
                falls_through: true,
                branch: Some((target(d.a), 0)),
            },
            // The cleanup fragment is only ever entered by the unwinder,
            // which leaves a paused record where the protect record was — the
            // same size, so the same depth.
            Op::Protect => Effect {
                pops: 0,
                pushes: RECORD,
                falls_through: true,
                branch: Some((target(d.a), RECORD as i32)),
            },
            Op::Unmark => plain(RECORD + 1, 1),
        })
    }
}

/// Turning a Lisp list of instructions into a verified [`function`] object.
///
/// The compiler is written in Lisp and emits instructions as lists —
/// `(LREF 0 0)`, `(JMP-NIL ELSE)`, `(:LABEL ELSE)`. This resolves the labels,
/// encodes the operands, verifies the result and computes `max-stack`.
///
/// # Why this is not written in Lisp
///
/// Because then the opcode numbers and the operand widths would exist in two
/// places with nothing checking that they agree, and so would the table of
/// what each instruction does to the stack. That is the mistake issue #30 was
/// about, and it is cheaper not to make it than to guard it.
///
/// What is left in Lisp is everything with a decision in it: what `let`
/// compiles to, which calls are in tail position, when a primitive may be
/// inlined. What is here has no judgement in it at all — byte layout, label
/// arithmetic, stack accounting.
pub mod assemble {
    use super::*;
    use crate::eval::EvalError;

    /// Assemble `instructions` into a function.
    ///
    /// # Rule 9
    ///
    /// Unbounded in its argument: the cost grows with the length of the
    /// program, and a slice cannot end in the middle of it. The same note as
    /// `%MAKE-VECTOR` and the same answer — chunk it when slices start to
    /// matter, which for a compiler that runs once per build they do not.
    pub fn assemble<'h>(
        mc: &mut Mutation<'h>,
        instructions: Value<'h>,
        constants: Value<'h>,
        params: Value<'h>,
        name: Value<'h>,
    ) -> Result<Value<'h>, EvalError> {
        let items = to_items(mc, instructions)?;

        // One pass to place the labels. Possible in one pass precisely
        // because an instruction's size depends only on its opcode, which is
        // what fixing the operand widths bought (chapter 24).
        let mut labels: Vec<(Value<'h>, usize)> = Vec::new();
        let mut at = 0usize;
        for item in &items {
            match item {
                Item::Label(name) => labels.push((*name, at)),
                Item::Op(op, _, _) => at += op.width(),
            }
        }

        let mut bytes = Vec::new();
        for item in &items {
            let Item::Op(op, a, b) = item else { continue };
            let end = bytes.len() + op.width();
            let resolve = |operand: &Operand<'h>| -> Result<i32, EvalError> {
                match operand {
                    Operand::Number(n) => Ok(*n),
                    Operand::Label(l) => {
                        let Some((_, target)) = labels.iter().find(|(name, _)| name == l) else {
                            return Err(EvalError::BadSyntax {
                                operator: "ASSEMBLE",
                                detail: "names a label that is not there",
                            });
                        };
                        // Offsets are from the end of the instruction, which
                        // is the convention the VM's `jump` reads them with.
                        Ok(*target as i32 - end as i32)
                    }
                    Operand::Missing => Ok(0),
                }
            };
            let (a, b) = (resolve(a)?, resolve(b)?);
            if !fits(*op, a, b) {
                return Err(EvalError::BadSyntax {
                    operator: "ASSEMBLE",
                    detail: "has an operand too large for the field the opcode gives it",
                });
            }
            encode(&mut bytes, *op, a, b);
        }

        // Built with a generous `max-stack` so the verification can only fail
        // for a real reason, then rebuilt with the depth it reports. The
        // compiler never has to work out how deep its own output gets.
        let draft = make_function(mc, &bytes, constants, params, MAX_DEPTH, name);
        let needs = match verify::function(mc, draft) {
            Ok(n) => n,
            Err(e) => {
                let _ = e;
                return Err(EvalError::BadSyntax {
                    operator: "ASSEMBLE",
                    detail: "produced a function the verifier refused",
                });
            }
        };
        Ok(make_function(mc, &bytes, constants, params, needs, name))
    }

    /// A stack depth no real function reaches, for the draft above.
    const MAX_DEPTH: usize = 1 << 20;

    enum Operand<'h> {
        Number(i32),
        Label(Value<'h>),
        Missing,
    }

    enum Item<'h> {
        Label(Value<'h>),
        Op(Op, Operand<'h>, Operand<'h>),
    }

    fn bad(detail: &'static str) -> EvalError {
        EvalError::BadSyntax {
            operator: "ASSEMBLE",
            detail,
        }
    }

    fn to_items<'h>(
        mc: &Mutation<'h>,
        instructions: Value<'h>,
    ) -> Result<Vec<Item<'h>>, EvalError> {
        let mut out = Vec::new();
        let mut cursor = instructions;
        while cursor.is_cons() {
            let form = mc.car(cursor);
            cursor = mc.cdr(cursor);
            if !form.is_cons() {
                return Err(bad("wants a list of instructions"));
            }
            let head = mc.car(form);
            let rest = mc.cdr(form);
            if !mc.is_symbol(head) {
                return Err(bad("wants each instruction to start with a symbol"));
            }
            let text = mc.symbol_name(head);
            if mc.is_keyword(head) && text == "LABEL" {
                if !rest.is_cons() {
                    return Err(bad(":label wants a name"));
                }
                out.push(Item::Label(mc.car(rest)));
                continue;
            }
            let Some(op) = Op::from_name(&text) else {
                return Err(bad("names an instruction that does not exist"));
            };
            let (a, rest) = operand(mc, rest)?;
            let (b, _) = operand(mc, rest)?;
            out.push(Item::Op(op, a, b));
        }
        Ok(out)
    }

    fn operand<'h>(
        mc: &Mutation<'h>,
        rest: Value<'h>,
    ) -> Result<(Operand<'h>, Value<'h>), EvalError> {
        if !rest.is_cons() {
            return Ok((Operand::Missing, Value::NIL));
        }
        let v = mc.car(rest);
        let next = mc.cdr(rest);
        if let Some(n) = v.as_fixnum() {
            let Ok(n) = i32::try_from(n) else {
                return Err(bad("has an operand that is not a 32-bit number"));
            };
            return Ok((Operand::Number(n), next));
        }
        if mc.is_symbol(v) {
            return Ok((Operand::Label(v), next));
        }
        Err(bad("wants each operand to be a number or a label"))
    }

    /// Does the operand fit the field this opcode gives it?
    ///
    /// Checked here so that an overlong jump or a constant index past 65,535
    /// is an error the compiler can report, rather than the panic `encode`
    /// would raise. Those ceilings are listed in `docs/bytecode.md`.
    fn fits(op: Op, a: i32, b: i32) -> bool {
        let byte = |v: i32| (0..=255).contains(&v);
        let word = |v: i32| (0..=65_535).contains(&v);
        let signed = |v: i32| (-32_768..=32_767).contains(&v);
        match op.operands() {
            Operands::None => a == 0 && b == 0,
            Operands::U8 => byte(a),
            Operands::U8U8 => byte(a) && byte(b),
            Operands::U16 => word(a),
            Operands::S16 => signed(a),
            Operands::U16S16 => word(a) && signed(b),
        }
    }
}

#[cfg(test)]
#[path = "unit/code.rs"]
mod tests;
