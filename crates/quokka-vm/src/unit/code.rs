//! Tests for [`crate::code`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

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
/// that drifts the moment nobody is checking — it has already been wrong
/// once, when the specification said thirty-four and the enum said thirty-six.
/// Thirty-eight since `JMP-TAG` (#37).
#[test]
fn there_are_thirty_eight_opcodes() {
    assert_eq!(OPCODE_COUNT, 38);
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

/// Verify a hand-written program, returning the depth it needs or the
/// reason it was refused.
fn check(body: &[(Op, i32, i32)], constants: usize) -> Result<usize, verify::Error> {
    let mut bytes = Vec::new();
    for (op, a, b) in body {
        encode(&mut bytes, *op, *a, *b);
    }
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let table = mc.alloc(ObjectType::Vector, constants);
        let name = mc.intern("TEST");
        let f = make_function(mc, &bytes, table, Value::NIL, 1024, name);
        verify::function(mc, f)
    })
}

/// `fact`'s body, which chapter 25 prints and says needs a depth of four.
#[test]
fn the_verifier_computes_what_fact_needs() {
    assert_eq!(
        check(
            &[
                (Op::Lref, 0, 0),
                (Op::Fix, 0, 0),
                (Op::Prim, 3, 2),
                (Op::JmpNil, 4, 0),
                (Op::Fix, 1, 0),
                (Op::Ret, 0, 0),
                (Op::Lref, 0, 0),
                (Op::Gfref, 0, 0),
                (Op::Lref, 0, 0),
                (Op::Fix, 1, 0),
                (Op::Prim, 1, 2),
                (Op::Call, 1, 0),
                (Op::Prim, 2, 2),
                (Op::Ret, 0, 0),
            ],
            1
        ),
        Ok(4),
        "chapter 25 says four, and a book that quotes a number should have \
         something checking it"
    );
}

/// **The bug this exists for.** A jump of four bytes over a branch that is
/// two lands inside a later instruction, where the machine reads the high
/// half of an operand as an opcode and runs on happily. It cost twenty
/// minutes to find at run time; it is a load-time error now.
#[test]
fn a_jump_into_the_middle_of_an_instruction_is_refused() {
    let out = check(
        &[
            (Op::Lref, 0, 0),
            (Op::Fix, 0, 0),
            (Op::Prim, 3, 2),
            (Op::JmpNil, 4, 0), // over T and RET, which are one byte each
            (Op::T, 0, 0),
            (Op::Ret, 0, 0),
            (Op::Gfref, 0, 0),
            (Op::Ret, 0, 0),
        ],
        1,
    );
    assert!(
        matches!(
            out,
            Err(verify::Error::JumpIntoNowhere { at: 9, target: 16 })
        ),
        "{out:?}"
    );
}

/// The other half: the same program with the offset the compiler should
/// have worked out.
#[test]
fn the_same_jump_with_the_right_offset_verifies() {
    assert!(check(
        &[
            (Op::Lref, 0, 0),
            (Op::Fix, 0, 0),
            (Op::Prim, 3, 2),
            (Op::JmpNil, 2, 0),
            (Op::T, 0, 0),
            (Op::Ret, 0, 0),
            (Op::Gfref, 0, 0),
            (Op::Ret, 0, 0),
        ],
        1,
    )
    .is_ok());
}

#[test]
fn popping_more_than_is_there_is_refused() {
    assert!(matches!(
        check(&[(Op::Fix, 1, 0), (Op::Prim, 0, 3), (Op::Ret, 0, 0)], 0),
        Err(verify::Error::Underflow { .. })
    ));
}

/// The second bug this exists for, in miniature: an instruction that
/// leaves one item too many. Both paths to the `RET` must agree about how
/// deep the stack is, and here they do not.
#[test]
fn two_paths_that_disagree_about_the_stack_are_refused() {
    let out = check(
        &[
            (Op::Nil, 0, 0),
            (Op::JmpNil, 3, 0), // skip the FIX: arrives with nothing
            (Op::Fix, 1, 0),    // the other path arrives with one
            (Op::Ret, 0, 0),
        ],
        0,
    );
    assert!(
        matches!(out, Err(verify::Error::Inconsistent { .. })),
        "{out:?}"
    );
}

#[test]
fn a_constant_the_function_does_not_have_is_refused() {
    assert!(matches!(
        check(&[(Op::Const, 7, 0), (Op::Ret, 0, 0)], 2),
        Err(verify::Error::BadConstant { index: 7, .. })
    ));
}

/// Unreachable code is not an error — it is what the instruction after an
/// unconditional `RET` is, and a compiler emits it without meaning to.
#[test]
fn unreachable_code_is_allowed() {
    assert_eq!(
        check(&[(Op::Fix, 1, 0), (Op::Ret, 0, 0), (Op::Pop, 0, 0)], 0),
        Ok(1)
    );
}

/// Verify a hand-written program whose constant 0 is a vector of `tags`
/// tag names, which is what `TAG` needs.
fn check_with_tags(body: &[(Op, i32, i32)], tags: usize) -> Result<usize, verify::Error> {
    let mut bytes = Vec::new();
    for (op, a, b) in body {
        encode(&mut bytes, *op, *a, *b);
    }
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let names = mc.alloc(ObjectType::Vector, tags);
        let table = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(table, 0, names);
        let name = mc.intern("TEST");
        let f = make_function(mc, &bytes, table, Value::NIL, 1024, name);
        verify::function(mc, f)
    })
}

/// `JMP-TAG` drops the stack to the innermost record's top, so with no
/// record open there is nowhere for it to go.
#[test]
fn a_jmp_tag_with_no_record_is_refused() {
    assert_eq!(
        check(&[(Op::JmpTag, 0, 0), (Op::Ret, 0, 0)], 0),
        Err(verify::Error::NoTagRecord { at: 0 })
    );
}

/// And the innermost record must be a tagbody's — dropping to a `block`'s
/// would leave the block's record standing where a tagbody's should be.
#[test]
fn a_jmp_tag_inside_a_block_is_refused() {
    assert_eq!(
        check(
            &[
                (Op::Block, 0, 3), // its label is the RET at 8
                (Op::JmpTag, 0, 0),
                (Op::Ret, 0, 0),
            ],
            1
        ),
        Err(verify::Error::NoTagRecord { at: 5 })
    );
}

/// Every entry of a tagbody's jump table is reached only by a `go`
/// landing, so the verifier checks each one is a `JMP` — here the table
/// "starts" at a `NIL`.
#[test]
fn a_jump_table_entry_that_is_not_a_jump_is_refused() {
    assert_eq!(
        check_with_tags(&[(Op::Tag, 0, 0), (Op::Nil, 0, 0), (Op::Ret, 0, 0)], 1),
        Err(verify::Error::BadTable { at: 0, entry: 5 })
    );
}

/// The verifier's idea of how much stack a record takes has to be the
/// VM's idea, and they live in different modules.
#[test]
fn the_record_size_agrees_with_the_machine() {
    assert_eq!(verify::RECORD, crate::vm::mark::SIZE);
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
