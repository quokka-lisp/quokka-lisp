//! Tests for [`crate::vm`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.

use super::*;
use crate::code::{encode, OPCODE_COUNT};
use crate::heap::Heap;
use crate::slice::{run_slice, Status};

/// Primitive indices, pinned where they are used so that a reordering of
/// the table shows up here rather than as arithmetic that is quietly
/// wrong. `primitives::the_table_order_is_pinned` is the other half.
const ADD: i32 = 0;
const SUB: i32 = 1;
const MUL: i32 = 2;

/// One item of a hand-written program.
///
/// Counting jump offsets by hand is how the first version of
/// `a_tail_call_does_not_grow_the_stack` came to jump four bytes over a
/// two-byte branch, landing in the *middle* of a later instruction, where
/// the machine decoded the high half of an operand as an opcode and ran
/// on through nonsense. The error that eventually surfaced was three
/// instructions from the cause.
///
/// So: labels. This is the assembler the compiler will need anyway, in
/// miniature, and it exists because of a bug rather than in anticipation
/// of one.
enum I {
    /// An ordinary instruction.
    Ins(Op, i32, i32),
    /// Somewhere a jump can aim at. Takes no space.
    Label(&'static str),
    /// A jump, by label.
    To(Op, &'static str),
    /// A `BLOCK`-shaped instruction: a constant index and a label.
    Mark(Op, i32, &'static str),
}
use I::*;

/// Resolve labels and emit.
///
/// One pass to place the labels and one to emit, which is all it takes
/// because an instruction's size depends only on its opcode — the whole
/// point of fixing operand widths, arriving as a convenience rather than
/// as the theory it was argued as.
fn assemble(items: &[I]) -> Vec<u8> {
    let mut at = 0usize;
    let mut labels = std::collections::HashMap::new();
    for item in items {
        match item {
            Label(name) => {
                labels.insert(*name, at);
            }
            Ins(op, _, _) | To(op, _) | Mark(op, _, _) => at += op.width(),
        }
    }
    let mut bytes = Vec::new();
    for item in items {
        match item {
            Label(_) => {}
            Ins(op, a, b) => encode(&mut bytes, *op, *a, *b),
            To(op, name) => {
                let end = bytes.len() + op.width();
                let target = labels[name] as i32;
                encode(&mut bytes, *op, target - end as i32, 0);
            }
            Mark(op, k, name) => {
                let end = bytes.len() + op.width();
                let target = labels[name] as i32;
                encode(&mut bytes, *op, *k, target - end as i32);
            }
        }
    }
    bytes
}

/// Assemble, wrap in a function, run, print.
fn run_asm<'h>(mc: &mut Mutation<'h>, items: &[I], constants: &[Value<'h>]) -> String {
    let bytes = assemble(items);
    let table = mc.alloc(ObjectType::Vector, constants.len());
    for (i, c) in constants.iter().enumerate() {
        mc.set_slot(table, i, *c);
    }
    let name = mc.intern("TEST");
    let f = verified(mc, &bytes, table, Value::NIL, name);
    match run_now(mc, f, 100_000) {
        Ok(v) => crate::printer::print(mc, v),
        Err(e) => format!("; {e}"),
    }
}

/// Build a function whose `max-stack` is **computed**, not guessed.
///
/// The verifier works out how deep the stack really gets, which is what
/// the assembler should be doing — and what the compiler will do. Until
/// this existed every hand-written test declared a number it had made up,
/// and `nested_cleanups_run_innermost_first` declared sixteen where it
/// needed twenty-three. Nothing noticed at run time, because `ensure`
/// grows the stack when a frame asks for more than is left; `max-stack`
/// was decorative.
fn verified<'h>(
    mc: &mut Mutation<'h>,
    bytes: &[u8],
    constants: Value<'h>,
    params: Value<'h>,
    name: Value<'h>,
) -> Value<'h> {
    // Declared generously so the first verification can only fail for a
    // real reason, then rebuilt with the answer.
    let draft = code::make_function(mc, bytes, constants, params, usize::MAX >> 4, name);
    let needs = code::verify::function(mc, draft).expect("the test program verifies");
    code::make_function(mc, bytes, constants, params, needs, name)
}

/// Assemble a function with no constants.
fn asm<'h>(mc: &mut Mutation<'h>, body: &[(Op, i32, i32)]) -> Value<'h> {
    asm_with(mc, body, &[])
}

fn asm_with<'h>(
    mc: &mut Mutation<'h>,
    body: &[(Op, i32, i32)],
    constants: &[Value<'h>],
) -> Value<'h> {
    let mut bytes = Vec::new();
    for (op, a, b) in body {
        encode(&mut bytes, *op, *a, *b);
    }
    let table = mc.alloc(ObjectType::Vector, constants.len());
    for (i, c) in constants.iter().enumerate() {
        mc.set_slot(table, i, *c);
    }
    let name = mc.intern("TEST");
    verified(mc, &bytes, table, Value::NIL, name)
}

fn run(body: &[(Op, i32, i32)]) -> String {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let f = asm(mc, body);
        match run_now(mc, f, 10_000) {
            Ok(v) => crate::printer::print(mc, v),
            Err(e) => format!("; {e}"),
        }
    })
}

/// The expression from chapter 25, compiled by hand.
///
/// `(+ (* 2 3) 4)` — and the point of the stack is that nothing had to
/// say where the 6 goes.
#[test]
fn an_expression_evaluates_on_the_stack() {
    assert_eq!(
        run(&[
            (Op::Fix, 2, 0),
            (Op::Fix, 3, 0),
            (Op::Prim, MUL, 2),
            (Op::Fix, 4, 0),
            (Op::Prim, ADD, 2),
            (Op::Ret, 0, 0),
        ]),
        "10"
    );
}

/// Popping reverses the order the arguments were pushed in, so a
/// non-commutative primitive is the only thing that catches getting it
/// wrong. Every test built on `+` would have passed either way.
#[test]
fn arguments_reach_a_primitive_in_the_order_they_were_pushed() {
    assert_eq!(
        run(&[
            (Op::Fix, 10, 0),
            (Op::Fix, 4, 0),
            (Op::Prim, SUB, 2),
            (Op::Ret, 0, 0),
        ]),
        "6"
    );
}

#[test]
fn nesting_leaves_the_left_hand_answer_waiting() {
    assert_eq!(
        run(&[
            (Op::Fix, 1, 0),
            (Op::Fix, 2, 0),
            (Op::Prim, ADD, 2),
            (Op::Fix, 10, 0),
            (Op::Fix, 4, 0),
            (Op::Prim, SUB, 2),
            (Op::Prim, MUL, 2),
            (Op::Ret, 0, 0),
        ]),
        "18"
    );
}

#[test]
fn constants_come_from_the_functions_own_table() {
    let mut heap = Heap::new();
    let printed = heap.mutate(|mc| {
        let hello = mc.intern("HELLO");
        let f = asm_with(mc, &[(Op::Const, 0, 0), (Op::Ret, 0, 0)], &[hello]);
        let v = run_now(mc, f, 100).expect("runs");
        crate::printer::print(mc, v)
    });
    assert_eq!(printed, "HELLO");
}

/// A `JMP-NIL` over the then-branch: the shape every `if` compiles to.
#[test]
fn a_conditional_jump_skips_the_branch_not_taken() {
    // (if nil 1 2) — the jump is taken, so 2.
    let taken = run(&[
        (Op::Nil, 0, 0),
        (Op::JmpNil, 4, 0), // over FIX 1 (3 bytes) and RET (1)
        (Op::Fix, 1, 0),
        (Op::Ret, 0, 0),
        (Op::Fix, 2, 0),
        (Op::Ret, 0, 0),
    ]);
    assert_eq!(taken, "2");

    // (if t 1 2) — not taken, so 1.
    let not_taken = run(&[
        (Op::T, 0, 0),
        (Op::JmpNil, 4, 0),
        (Op::Fix, 1, 0),
        (Op::Ret, 0, 0),
        (Op::Fix, 2, 0),
        (Op::Ret, 0, 0),
    ]);
    assert_eq!(not_taken, "1");
}

/// Offsets are relative to the *end* of the jump, which is the convention
/// that lets an assembler backpatch in one pass. A negative one goes
/// backwards, which is what every loop is made of.
///
/// Written so that measuring from the wrong place is visible rather than
/// merely wrong: from the end, this lands on `FIX 11` and returns 11;
/// from the start of the jump it would land back at offset 0 and loop
/// until the budget ran out.
#[test]
fn a_backward_jump_measures_from_the_end_of_the_jump() {
    assert_eq!(
        run(&[
            (Op::Jmp, 4, 0),   //  0 -> 7
            (Op::Fix, 11, 0),  //  3
            (Op::Ret, 0, 0),   //  6
            (Op::Fix, 22, 0),  //  7
            (Op::Jmp, -10, 0), // 10 -> 3
        ]),
        "11"
    );
}

/// Rule 4, tested rather than asserted: the machine stops between two
/// instructions and carries on where it left off.
#[test]
fn a_computation_survives_being_cut_in_half() {
    let mut heap = Heap::new();
    let printed = heap.mutate(|mc| {
        let f = asm(
            mc,
            &[
                (Op::Fix, 2, 0),
                (Op::Fix, 3, 0),
                (Op::Prim, MUL, 2),
                (Op::Fix, 4, 0),
                (Op::Prim, ADD, 2),
                (Op::Ret, 0, 0),
            ],
        );
        {
            let mut vm = Vm::start(mc, f, Value::NIL);
            assert_eq!(run_slice(&mut vm, 2), Status::Yielded);
        }
        // Nothing was carried across in Rust. Everything the machine knew
        // is in the heap, which is the whole claim.
        let answer = {
            let mut vm = Vm::resume(mc);
            assert!(matches!(run_slice(&mut vm, 100), Status::Done { .. }));
            vm.result()
        };
        crate::printer::print(mc, answer)
    });
    assert_eq!(printed, "10");
}

/// A popped slot must be cleared, or the stack keeps every value it has
/// ever held alive — a leak with no symptom until the heap is large.
#[test]
fn a_popped_slot_stops_referring_to_what_was_there() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let marker = mc.intern("MARKER");
        let f = asm_with(
            mc,
            &[
                (Op::Const, 0, 0),
                (Op::Pop, 0, 0),
                (Op::Fix, 1, 0),
                (Op::Ret, 0, 0),
            ],
            &[marker],
        );
        run_now(mc, f, 100).expect("runs");
        let stack = mc.register(Register::Operands);
        assert_eq!(
            mc.slot(stack, 0),
            Value::NIL,
            "the slot still refers to what was popped from it"
        );
    });
}

/// The invariant from chapter 23, which the interpreter got for one line
/// and this machine has to keep per opcode.
#[test]
fn computing_a_value_clears_the_values_register() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let f = asm(mc, &[(Op::Fix, 1, 0), (Op::Ret, 0, 0)]);
        let mut vm = Vm::start(mc, f, Value::NIL);
        // Pretend a previous form returned several values.
        vm.mc.set_register(Register::Values, Value::NIL);
        assert_eq!(vm.step(), Transition::Continue);
        assert_eq!(
            vm.mc.register(Register::Values),
            Value::UNBOUND,
            "FIX pushed one value and left the register saying otherwise"
        );
    });
}

/// `let` is instructions, not a frame: evaluate the inits, `BIND` them
/// into a scope, run the body, `UNBIND`.
#[test]
fn a_scope_binds_and_reads_back() {
    let mut heap = Heap::new();
    let printed = heap.mutate(|mc| {
        let a = mc.intern("A");
        let names = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(names, 0, a);
        let f = asm_with(
            mc,
            &[
                (Op::Fix, 41, 0),
                (Op::Bind, 0, 0),
                (Op::Lref, 0, 0),
                (Op::Fix, 1, 0),
                (Op::Prim, ADD, 2),
                (Op::Unbind, 1, 0),
                (Op::Ret, 0, 0),
            ],
            &[names],
        );
        let v = run_now(mc, f, 1_000).expect("runs");
        crate::printer::print(mc, v)
    });
    assert_eq!(printed, "42");
}

/// The trap book chapter 26 describes: a function scope is a link in the
/// chain although a variable lookup steps over it, so the compiler has to
/// count it. If it miscounts, the reference lands somewhere real and
/// returns the wrong value — silently, were it not for the namespace
/// check.
#[test]
fn a_depth_that_lands_in_the_wrong_namespace_is_refused() {
    let mut heap = Heap::new();
    let message = heap.mutate(|mc| {
        let a = mc.intern("A");
        let vars = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(vars, 0, a);
        let fns = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(fns, 0, a);
        // A variable scope, then a function scope on top of it. `A` the
        // variable is two links out, not one.
        let f = asm_with(
            mc,
            &[
                (Op::Fix, 1, 0),
                (Op::Bind, 0, 0), // variable scope
                (Op::Nil, 0, 0),
                (Op::Bindf, 1, 0), // function scope
                (Op::Lref, 0, 0),  // wrong: depth 0 is the function scope
                (Op::Ret, 0, 0),
            ],
            &[vars, fns],
        );
        match run_now(mc, f, 1_000) {
            Ok(v) => format!("returned {}", crate::printer::print(mc, v)),
            Err(e) => format!("{e}"),
        }
    });
    assert!(message.contains("disagree"), "{message}");
}

/// The same program with the depth the compiler should have worked out.
#[test]
fn counting_the_function_scope_finds_the_variable() {
    let mut heap = Heap::new();
    let printed = heap.mutate(|mc| {
        let a = mc.intern("A");
        let vars = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(vars, 0, a);
        let fns = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(fns, 0, a);
        let f = asm_with(
            mc,
            &[
                (Op::Fix, 7, 0),
                (Op::Bind, 0, 0),
                (Op::Nil, 0, 0),
                (Op::Bindf, 1, 0),
                (Op::Lref, 1, 0), // one link out, past the function scope
                (Op::Ret, 0, 0),
            ],
            &[vars, fns],
        );
        let v = run_now(mc, f, 1_000).expect("runs");
        crate::printer::print(mc, v)
    });
    assert_eq!(printed, "7");
}

/// A global is read through its symbol every time, which is what lets a
/// redefinition take effect. Chapter 26's `*limit*`.
#[test]
fn a_global_is_read_through_its_symbol() {
    let mut heap = Heap::new();
    let printed = heap.mutate(|mc| {
        let limit = mc.intern("*LIMIT*");
        let f = asm_with(
            mc,
            &[
                (Op::Fix, 10, 0),
                (Op::Gset, 0, 0),
                (Op::Pop, 0, 0),
                (Op::Gref, 0, 0),
                (Op::Ret, 0, 0),
            ],
            &[limit],
        );
        let v = run_now(mc, f, 1_000).expect("runs");
        crate::printer::print(mc, v)
    });
    assert_eq!(printed, "10");
}

#[test]
fn an_unbound_global_says_which_one() {
    let mut heap = Heap::new();
    let message = heap.mutate(|mc| {
        let nope = mc.intern("*NOPE*");
        let f = asm_with(mc, &[(Op::Gref, 0, 0), (Op::Ret, 0, 0)], &[nope]);
        match run_now(mc, f, 100) {
            Ok(_) => String::from("ran"),
            Err(e) => format!("{e}"),
        }
    });
    assert!(message.contains("*NOPE*"), "{message}");
}

/// `fact`, hand-assembled exactly as `docs/bytecode.md` and book chapter
/// 25 print it — the first compiled Quokka function to run.
#[test]
fn fact_runs() {
    let mut heap = Heap::new();
    let printed = heap.mutate(|mc| {
        let fact = mc.intern("FACT");
        let n = mc.intern("N");
        let params = mc.cons(n, Value::NIL);
        let constants = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(constants, 0, fact);
        let mut bytes = Vec::new();
        for (op, a, b) in [
            (Op::Lref, 0, 0),   //  0  n
            (Op::Fix, 0, 0),    //  3  0
            (Op::Prim, 3, 2),   //  6  =
            (Op::JmpNil, 4, 0), //  9  -> 16
            (Op::Fix, 1, 0),    // 12
            (Op::Ret, 0, 0),    // 15
            (Op::Lref, 0, 0),   // 16  n, the left argument of *
            (Op::Gfref, 0, 0),  // 19  FACT's cell, read now
            (Op::Lref, 0, 0),   // 22
            (Op::Fix, 1, 0),    // 25
            (Op::Prim, SUB, 2), // 28  (- n 1)
            (Op::Call, 1, 0),   // 31
            (Op::Prim, MUL, 2), // 33
            (Op::Ret, 0, 0),    // 36
        ] {
            encode(&mut bytes, op, a, b);
        }
        let name = mc.intern("FACT");
        let code_object = code::make_function(mc, &bytes, constants, params, 4, name);
        assert_eq!(bytes.len(), 37, "chapter 25 counted thirty-seven bytes");

        // (defun fact ...) — put the closure in the symbol's function cell
        // so that GFREF finds it, which is what the real DEFUN expands to.
        let c = closure::make(mc, params, code_object, Value::NIL);
        mc.set_slot(fact, symbol::FUNCTION, c);

        // (fact 10)
        let caller = {
            let mut b = Vec::new();
            encode(&mut b, Op::Gfref, 0, 0);
            encode(&mut b, Op::Fix, 10, 0);
            encode(&mut b, Op::Call, 1, 0);
            encode(&mut b, Op::Ret, 0, 0);
            code::make_function(mc, &b, constants, Value::NIL, 4, name)
        };
        let v = run_now(mc, caller, 100_000).expect("runs");
        crate::printer::print(mc, v)
    });
    assert_eq!(printed, "3628800");
}

/// `down` from the README, and the property that makes it run at all:
/// `TCALL` reuses the frame, so a hundred thousand calls deep the stack
/// is exactly as deep as it was at the first.
#[test]
fn a_tail_call_does_not_grow_the_stack() {
    let mut heap = Heap::new();
    let (printed, depth) = heap.mutate(|mc| {
        let down = mc.intern("DOWN");
        let n = mc.intern("N");
        let params = mc.cons(n, Value::NIL);
        let constants = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(constants, 0, down);
        let mut bytes = Vec::new();
        for (op, a, b) in [
            (Op::Lref, 0, 0),
            (Op::Fix, 0, 0),
            (Op::Prim, 3, 2), // =
            // Over T and RET, which are one byte each — not four, as in
            // `fact`, where the then-branch is `FIX 1` and `RET`. Getting
            // this wrong lands in the *middle* of a later instruction and
            // the machine runs happily on through nonsense, which is the
            // argument for a label-aware assembler rather than counting.
            (Op::JmpNil, 2, 0),
            (Op::T, 0, 0),
            (Op::Ret, 0, 0),
            (Op::Gfref, 0, 0),
            (Op::Lref, 0, 0),
            (Op::Fix, 1, 0),
            (Op::Prim, SUB, 2),
            (Op::Tcall, 1, 0),
        ] {
            encode(&mut bytes, op, a, b);
        }
        let name = mc.intern("DOWN");
        let code_object = code::make_function(mc, &bytes, constants, params, 4, name);
        let c = closure::make(mc, params, code_object, Value::NIL);
        mc.set_slot(down, symbol::FUNCTION, c);

        let caller = {
            let mut b = Vec::new();
            encode(&mut b, Op::Gfref, 0, 0);
            encode(&mut b, Op::Fix, 20_000, 0);
            encode(&mut b, Op::Call, 1, 0);
            encode(&mut b, Op::Ret, 0, 0);
            code::make_function(mc, &b, constants, Value::NIL, 4, name)
        };
        let v = run_now(mc, caller, 10_000_000).expect("runs");
        (
            crate::printer::print(mc, v),
            mc.header(mc.register(Register::Operands)).slots,
        )
    });
    assert_eq!(printed, "T");
    assert_eq!(
        depth, INITIAL_STACK,
        "twenty thousand tail calls grew the operand stack"
    );
}

/// The other half of the same claim: an ordinary call *does* use stack,
/// so the test above is measuring something.
#[test]
fn an_ordinary_call_does_grow_the_stack() {
    let mut heap = Heap::new();
    let grown = heap.mutate(|mc| {
        let loop_ = mc.intern("LOOP-DOWN");
        let n = mc.intern("N");
        let params = mc.cons(n, Value::NIL);
        let constants = mc.alloc(ObjectType::Vector, 1);
        mc.set_slot(constants, 0, loop_);
        let mut bytes = Vec::new();
        for (op, a, b) in [
            (Op::Lref, 0, 0),
            (Op::Fix, 0, 0),
            (Op::Prim, 3, 2),
            (Op::JmpNil, 2, 0),
            (Op::T, 0, 0),
            (Op::Ret, 0, 0),
            (Op::Gfref, 0, 0),
            (Op::Lref, 0, 0),
            (Op::Fix, 1, 0),
            (Op::Prim, SUB, 2),
            (Op::Call, 1, 0), // not a tail call this time
            (Op::Ret, 0, 0),
        ] {
            encode(&mut bytes, op, a, b);
        }
        let name = mc.intern("LOOP-DOWN");
        let code_object = code::make_function(mc, &bytes, constants, params, 4, name);
        let c = closure::make(mc, params, code_object, Value::NIL);
        mc.set_slot(loop_, symbol::FUNCTION, c);
        let caller = {
            let mut b = Vec::new();
            encode(&mut b, Op::Gfref, 0, 0);
            encode(&mut b, Op::Fix, 500, 0);
            encode(&mut b, Op::Call, 1, 0);
            encode(&mut b, Op::Ret, 0, 0);
            code::make_function(mc, &b, constants, Value::NIL, 4, name)
        };
        run_now(mc, caller, 1_000_000).expect("runs");
        mc.header(mc.register(Register::Operands)).slots
    });
    assert!(
        grown > INITIAL_STACK,
        "five hundred nested calls should have grown the stack"
    );
}

/// The big-bang rule, enforced rather than trusted: compiled code calling
/// an interpreted closure is a bootstrap that went wrong, not a feature.
#[test]
fn compiled_code_refuses_to_call_an_interpreted_closure() {
    let mut heap = Heap::new();
    let message = heap.mutate(|mc| {
        let body = mc.intern("SOMETHING");
        let interpreted = closure::make(mc, Value::NIL, body, Value::NIL);
        let f = asm_with(
            mc,
            &[(Op::Const, 0, 0), (Op::Call, 0, 0), (Op::Ret, 0, 0)],
            &[interpreted],
        );
        match run_now(mc, f, 100) {
            Ok(_) => String::from("ran, which it should not have"),
            Err(e) => format!("{e}"),
        }
    });
    assert!(message.contains("interpreted closure"), "{message}");
}

/// `(block out (return-from out 42) 99)` — the 99 is never reached.
#[test]
fn a_block_can_be_left_early() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let name = mc.intern("OUT");
        run_asm(
            mc,
            &[
                Mark(Op::Block, 0, "after"),
                Ins(Op::Fix, 42, 0),
                Ins(Op::Retfrom, 0, 0),
                Ins(Op::Fix, 99, 0), // unreachable
                Ins(Op::Unmark, 0, 0),
                Label("after"),
                Ins(Op::Ret, 0, 0),
            ],
            &[name],
        )
    });
    assert_eq!(out, "42");
}

/// And a block left the ordinary way drops its record and keeps its
/// value, which is the path that has nothing to do with unwinding.
#[test]
fn a_block_left_normally_keeps_its_value() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let name = mc.intern("OUT");
        run_asm(
            mc,
            &[
                Mark(Op::Block, 0, "after"),
                Ins(Op::Fix, 7, 0),
                Ins(Op::Unmark, 0, 0),
                Label("after"),
                Ins(Op::Ret, 0, 0),
            ],
            &[name],
        )
    });
    assert_eq!(out, "7");
}

/// `catch` and `throw` find each other by tag at run time rather than
/// lexically, which is the whole difference from `block`.
#[test]
fn a_throw_finds_its_catch() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let tag = mc.intern("DONE");
        run_asm(
            mc,
            &[
                Ins(Op::Const, 0, 0),
                To(Op::Catch, "after"),
                Ins(Op::Const, 0, 0),
                Ins(Op::Fix, 5, 0),
                Ins(Op::Throw, 0, 0),
                Ins(Op::Fix, 99, 0),
                Ins(Op::Unmark, 0, 0),
                Label("after"),
                Ins(Op::Ret, 0, 0),
            ],
            &[tag],
        )
    });
    assert_eq!(out, "5");
}

#[test]
fn a_throw_with_no_catch_says_so() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let tag = mc.intern("NOWHERE");
        run_asm(
            mc,
            &[
                Ins(Op::Const, 0, 0),
                Ins(Op::Fix, 1, 0),
                Ins(Op::Throw, 0, 0),
            ],
            &[tag],
        )
    });
    assert!(out.contains("no exit named NOWHERE"), "{out}");
}

/// A cleanup runs while the unwind is in flight, and the unwind then
/// carries on. The value thrown must survive the detour.
#[test]
fn a_cleanup_runs_during_a_throw_and_the_throw_continues() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let tag = mc.intern("DONE");
        let flag = mc.intern("*RAN*");
        run_asm(
            mc,
            &[
                Ins(Op::Const, 0, 0),
                To(Op::Catch, "caught"),
                To(Op::Protect, "cleanup"),
                Ins(Op::Const, 0, 0),
                Ins(Op::Fix, 5, 0),
                Ins(Op::Throw, 0, 0),
                Label("cleanup"),
                Ins(Op::T, 0, 0),
                Ins(Op::Gset, 1, 0),
                Ins(Op::Pop, 0, 0),
                Ins(Op::Unprotect, 0, 0),
                Label("caught"),
                // The thrown value is on the stack; add the flag to show
                // the cleanup ran before the catch was reached.
                Ins(Op::Gref, 1, 0),
                Ins(Op::Prim, 0, 0), // (+) => 0, just to use the value
                Ins(Op::Pop, 0, 0),
                Ins(Op::Gref, 1, 0),
                Ins(Op::Ret, 0, 0),
            ],
            &[tag, flag],
        )
    });
    assert_eq!(out, "T", "the cleanup did not run");
}

/// Chapter 20's test, compiled: cleanups run innermost first.
#[test]
fn nested_cleanups_run_innermost_first() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let tag = mc.intern("DONE");
        let order = mc.intern("*ORDER*");
        let inner = mc.intern("INNER");
        let outer = mc.intern("OUTER");
        run_asm(
            mc,
            &[
                Ins(Op::Nil, 0, 0),
                Ins(Op::Gset, 1, 0),
                Ins(Op::Pop, 0, 0),
                Ins(Op::Const, 0, 0),
                To(Op::Catch, "caught"),
                To(Op::Protect, "outer-cleanup"),
                To(Op::Protect, "inner-cleanup"),
                Ins(Op::Const, 0, 0),
                Ins(Op::Fix, 1, 0),
                Ins(Op::Throw, 0, 0),
                Label("inner-cleanup"),
                Ins(Op::Const, 2, 0),
                Ins(Op::Gref, 1, 0),
                Ins(Op::Prim, 6, 2), // CONS
                Ins(Op::Gset, 1, 0),
                Ins(Op::Pop, 0, 0),
                Ins(Op::Unprotect, 0, 0),
                Label("outer-cleanup"),
                Ins(Op::Const, 3, 0),
                Ins(Op::Gref, 1, 0),
                Ins(Op::Prim, 6, 2),
                Ins(Op::Gset, 1, 0),
                Ins(Op::Pop, 0, 0),
                Ins(Op::Unprotect, 0, 0),
                Label("caught"),
                Ins(Op::Pop, 0, 0),
                Ins(Op::Gref, 1, 0),
                Ins(Op::Ret, 0, 0),
            ],
            &[tag, order, inner, outer],
        )
    });
    assert_eq!(out, "(OUTER INNER)");
}

/// `(handler-case (error "boom") (error () 'caught))`, with the clause
/// dispatch as ordinary compiled code — the VM only knows *that* a
/// handler is here, never which conditions it wants.
#[test]
fn a_handler_catches_a_signalled_error() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let clauses = mc.intern("CLAUSES");
        let caught = mc.intern("CAUGHT");
        let boom = mc.alloc_constant_string("boom");
        run_asm(
            mc,
            &[
                Mark(Op::Handler, 0, "handle"),
                Ins(Op::Const, 2, 0),
                Ins(Op::Prim, 40, 1), // ERROR
                Ins(Op::Unmark, 0, 0),
                To(Op::Jmp, "done"),
                Label("handle"),
                // The condition arrives on the stack. A real dispatch
                // would test its type; this one takes everything.
                Ins(Op::Pop, 0, 0),
                Ins(Op::Const, 1, 0),
                Label("done"),
                Ins(Op::Ret, 0, 0),
            ],
            &[clauses, caught, boom],
        )
    });
    assert_eq!(out, "CAUGHT");
}

#[test]
fn an_unhandled_error_stops_the_machine() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let boom = mc.alloc_constant_string("boom");
        run_asm(
            mc,
            &[
                Ins(Op::Const, 0, 0),
                Ins(Op::Prim, 40, 1),
                Ins(Op::Ret, 0, 0),
            ],
            &[boom],
        )
    });
    assert!(out.contains("boom"), "{out}");
}

/// `(multiple-value-list (values 1 2))` in instructions: VALUES sets the
/// register, MVSAVE turns whatever the last form produced into a list.
#[test]
fn values_can_be_collected() {
    assert_eq!(
        run(&[
            (Op::Fix, 1, 0),
            (Op::Fix, 2, 0),
            (Op::Values, 2, 0),
            (Op::Mvsave, 0, 0),
            (Op::Ret, 0, 0),
        ]),
        "(1 2)"
    );
}

/// One value is the ordinary case and must cost nothing: the register
/// keeps saying UNBOUND, and MVSAVE builds the list only when asked.
#[test]
fn one_value_leaves_the_register_alone() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let f = asm(mc, &[(Op::Fix, 9, 0), (Op::Values, 1, 0), (Op::Ret, 0, 0)]);
        let v = run_now(mc, f, 100).expect("runs");
        assert_eq!(crate::printer::print(mc, v), "9");
        assert_eq!(mc.register(Register::Values), Value::UNBOUND);
    });
}

/// Calling a primitive through its function cell, which is what
/// `(funcall #'+ 1 2)` compiles to when the function is a value rather
/// than a name.
#[test]
fn a_primitive_can_be_called_through_its_symbol() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        primitives::install(mc);
        let plus = mc.intern("+");
        run_asm(
            mc,
            &[
                Ins(Op::Gfref, 0, 0),
                Ins(Op::Fix, 1, 0),
                Ins(Op::Fix, 2, 0),
                Ins(Op::Call, 2, 0),
                Ins(Op::Ret, 0, 0),
            ],
            &[plus],
        )
    });
    assert_eq!(out, "3");
}

/// `(multiple-value-call #'+ (values 1 2) (values 3 4))` => 10.
#[test]
fn multiple_value_call_spreads_every_value() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        primitives::install(mc);
        let plus = mc.intern("+");
        run_asm(
            mc,
            &[
                Ins(Op::Gfref, 0, 0),
                Ins(Op::Fix, 1, 0),
                Ins(Op::Fix, 2, 0),
                Ins(Op::Values, 2, 0),
                Ins(Op::Mvsave, 0, 0),
                Ins(Op::Fix, 3, 0),
                Ins(Op::Fix, 4, 0),
                Ins(Op::Values, 2, 0),
                Ins(Op::Mvsave, 0, 0),
                Ins(Op::Mvcall, 2, 0),
                Ins(Op::Ret, 0, 0),
            ],
            &[plus],
        )
    });
    assert_eq!(out, "10");
}

/// `multiple-value-prog1`: hold the first form's values across the rest.
#[test]
fn prog1_holds_its_values_across_what_follows() {
    assert_eq!(
        run(&[
            (Op::Fix, 1, 0),
            (Op::Fix, 2, 0),
            (Op::Values, 2, 0),
            (Op::Mvsave, 0, 0),
            // something else happens, producing one value
            (Op::Fix, 99, 0),
            (Op::Pop, 0, 0),
            (Op::Mvrestore, 0, 0),
            (Op::Mvsave, 0, 0),
            (Op::Ret, 0, 0),
        ]),
        "(1 2)"
    );
}

/// Chapter 23's transparency, now a property of the unwinder: a block
/// left early carries every value out, not just the first.
#[test]
fn a_block_is_transparent_to_multiple_values() {
    let mut heap = Heap::new();
    let out = heap.mutate(|mc| {
        let name = mc.intern("OUT");
        run_asm(
            mc,
            &[
                Mark(Op::Block, 0, "after"),
                Ins(Op::Fix, 1, 0),
                Ins(Op::Fix, 2, 0),
                Ins(Op::Values, 2, 0),
                Ins(Op::Retfrom, 0, 0),
                Ins(Op::Unmark, 0, 0),
                Label("after"),
                Ins(Op::Mvsave, 0, 0),
                Ins(Op::Ret, 0, 0),
            ],
            &[name],
        )
    });
    assert_eq!(out, "(1 2)");
}

/// Caught before anything runs, which is the verifier's whole point: a
/// byte that is not an opcode is a fault in whatever produced the
/// bytecode, and there is no reason to find that out halfway through.
#[test]
fn a_byte_that_is_not_an_instruction_is_refused_at_load() {
    let mut heap = Heap::new();
    let message = heap.mutate(|mc| {
        let code = mc.alloc_bytes(ObjectType::Code, &[200]);
        let f = mc.alloc(ObjectType::Function, function::SLOTS);
        mc.set_slot(f, function::CODE, code);
        mc.set_slot(f, function::CONSTANTS, Value::NIL);
        mc.set_slot(f, function::MAX_STACK, Value::fixnum(1).expect("small"));
        mc.set_slot(f, function::CODE_LENGTH, Value::fixnum(1).expect("small"));
        match run_now(mc, f, 100) {
            Ok(_) => String::from("ran, which it should not have"),
            Err(e) => format!("{e}"),
        }
    });
    assert!(message.contains("malformed"), "{message}");
    assert!(message.contains("UnknownOpcode"), "{message}");
}

/// A function with no `RET` is a bug in whoever produced it, and the
/// verifier can see that without running a single instruction.
#[test]
fn a_function_that_does_not_return_is_refused_at_load() {
    let mut heap = Heap::new();
    let message = heap.mutate(|mc| {
        // Built raw rather than through `verified`, which would refuse it
        // before the test got a chance to.
        let mut bytes = Vec::new();
        encode(&mut bytes, Op::Fix, 1, 0);
        let name = mc.intern("TEST");
        let f = code::make_function(mc, &bytes, Value::NIL, Value::NIL, 4, name);
        match run_now(mc, f, 100) {
            Ok(_) => String::from("ran, which it should not have"),
            Err(e) => format!("{e}"),
        }
    });
    assert!(message.contains("FallsOffTheEnd"), "{message}");
}

/// There is no "not implemented" arm left to test. The `match` on `Op`
/// in `step` is exhaustive, so the compiler is what now guarantees every
/// opcode in the table does something — which is a better test than this
/// one was.
#[test]
fn every_opcode_is_implemented() {
    for byte in 0..OPCODE_COUNT {
        assert!(Op::from_u8(byte as u8).is_some());
    }
}
