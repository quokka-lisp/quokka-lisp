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
    /// Pop it.
    Unhandler = 29,
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
    (Op::Unhandler, "UNHANDLER", Operands::None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::Heap;

    /// `Op::name` and `Op::operands` index `TABLE` by the discriminant, so a
    /// row in the wrong place would silently give one opcode another's name
    /// and — far worse — another's width, which would desynchronise the
    /// decoder from the instruction stream and produce nonsense a long way
    /// from the mistake.
    #[test]
    fn the_table_is_in_opcode_order() {
        for (byte, (op, _, _)) in TABLE.iter().enumerate() {
            assert_eq!(*op as usize, byte, "{} is out of place", op.name());
            assert_eq!(Op::from_u8(byte as u8), Some(*op));
        }
        assert_eq!(Op::from_u8(OPCODE_COUNT as u8), None);
    }

    /// The count `docs/bytecode.md` quotes. Not a law, but a number in prose
    /// that drifts the moment nobody is checking.
    #[test]
    fn there_are_thirty_six_opcodes() {
        assert_eq!(OPCODE_COUNT, 36);
    }

    /// Every operand shape survives a round trip, including the signed one,
    /// which is the only one that can be wrong in an interesting way.
    #[test]
    fn operands_round_trip() {
        let cases = [
            (Op::Ret, 0, 0),
            (Op::Call, 3, 0),
            (Op::Lref, 2, 7),
            (Op::Const, 300, 0),
            (Op::Const, 65_535, 0),
            (Op::Fix, 0, 0),
            (Op::Fix, -1, 0),
            (Op::Fix, -32_768, 0),
            (Op::Fix, 32_767, 0),
            (Op::Jmp, -40, 0),
            (Op::Block, 9, -12),
        ];
        for (op, a, b) in cases {
            let mut bytes = Vec::new();
            encode(&mut bytes, op, a, b);
            assert_eq!(bytes.len(), op.width(), "{} encoded wrong", op.name());
            let d = decode(&bytes, 0).expect("decodes");
            assert_eq!((d.op, d.a, d.b, d.next), (op, a, b, op.width()));
        }
    }

    /// Instructions are read one after another, so decoding has to agree with
    /// encoding about where each one ends — the property the whole fixed-width
    /// scheme exists to make easy.
    #[test]
    fn a_stream_decodes_as_it_was_written() {
        let written = [
            (Op::Lref, 0, 0),
            (Op::Fix, 0, 0),
            (Op::Prim, 3, 2),
            (Op::JmpNil, 4, 0),
            (Op::Fix, 1, 0),
            (Op::Ret, 0, 0),
        ];
        let mut bytes = Vec::new();
        for (op, a, b) in written {
            encode(&mut bytes, op, a, b);
        }
        let mut read = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let d = decode(&bytes, at).expect("decodes");
            read.push((d.op, d.a, d.b));
            at = d.next;
        }
        assert_eq!(read, written);
    }

    #[test]
    fn a_byte_that_is_not_an_opcode_is_refused() {
        assert_eq!(
            decode(&[200], 0),
            Err(CodeError::UnknownOpcode { byte: 200, at: 0 })
        );
    }

    /// An instruction whose operands run off the end is a truncated stream,
    /// not an unknown opcode: the distinction matters when a bug is being
    /// chased, because one means a bad byte and the other a bad length.
    #[test]
    fn an_instruction_cut_short_is_refused() {
        let mut bytes = Vec::new();
        encode(&mut bytes, Op::Const, 7, 0);
        bytes.pop();
        assert_eq!(decode(&bytes, 0), Err(CodeError::Truncated { at: 0 }));
    }

    #[test]
    #[should_panic(expected = "exceeds a byte")]
    fn an_operand_that_does_not_fit_is_a_panic_rather_than_a_wrap() {
        encode(&mut Vec::new(), Op::Call, 256, 0);
    }

    /// A jump prints where it lands, because a relative offset alone is not
    /// something a person can follow.
    #[test]
    fn a_disassembly_resolves_jumps() {
        let mut bytes = Vec::new();
        encode(&mut bytes, Op::Lref, 0, 0);
        encode(&mut bytes, Op::JmpNil, 1, 0);
        encode(&mut bytes, Op::Pop, 0, 0);
        encode(&mut bytes, Op::Ret, 0, 0);
        assert_eq!(
            disassemble(&bytes),
            "   0  LREF 0 0\n   3  JMP-NIL +1 -> 7\n   6  POP\n   7  RET\n"
        );
    }

    /// Bytes are packed eight to a word, so the interesting cases are the
    /// boundaries and a length that does not fill its last word.
    #[test]
    fn code_bytes_survive_the_heap() {
        let mut heap = Heap::new();
        let written: Vec<u8> = (0..20u8).collect();
        heap.mutate(|mc| {
            let name = mc.intern("TEST");
            let f = make_function(mc, &written, Value::NIL, Value::NIL, 3, name);
            assert_eq!(code_bytes(mc, f), written);
            assert_eq!(
                mc.slot(f, function::MAX_STACK).as_fixnum(),
                Some(3),
                "max-stack is what the caller said"
            );
        });
    }

    /// The point of a raw type: a collection must not disturb the bytes, and
    /// must not try to follow them as if they were references.
    #[test]
    fn code_survives_a_collection() {
        let mut heap = Heap::new();
        let written: Vec<u8> = (0..33u8).map(|b| b.wrapping_mul(7)).collect();
        heap.mutate(|mc| {
            let name = mc.intern("TEST");
            let f = make_function(mc, &written, Value::NIL, Value::NIL, 0, name);
            mc.set_root(f);
        });
        heap.collect();
        heap.mutate(|mc| {
            let f = mc.root();
            assert_eq!(code_bytes(mc, f), written);
        });
    }
}
