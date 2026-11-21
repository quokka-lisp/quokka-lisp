//! The bytecode machine: fetch, decode, execute.
//!
//! `crate::eval` walks a tree and this runs instructions, and after phase 1
//! only one of them is ever running. The interpreter bootstraps the compiler
//! and then goes cold; everything a built image evaluates is compiled first.
//! So these two never share a stack, a scope or a call, and neither has to
//! know the other exists. See #25 for why that question was harder than it
//! looks, and `docs/bytecode.md` for the instruction set.
//!
//! # What makes this sliceable
//!
//! [`Vm::step`] performs **one instruction** and returns, and every scrap of
//! what it was doing — the program counter, the stack pointer, the function —
//! lives in the heap rather than in a Rust local. So the loop can stop
//! between any two instructions (rule 4), and an image saved there resumes.
//! That is chapter 2's constraint collecting on a bill phase 0 paid.

use crate::code::{self, function, CodeError, Op};
use crate::eval::EvalError;
use crate::heap::{Mutation, ObjectType, Register};
use crate::primitives;
use crate::slice::{Steppable, Transition};
use crate::value::Value;

/// How many slots a new operand stack starts with.
///
/// Grown by doubling when a frame needs more than is left. Small enough that
/// a trivial program does not pay for a large vector, large enough that
/// nothing realistic grows more than a few times.
const INITIAL_STACK: usize = 64;

/// The bytecode machine.
pub struct Vm<'a, 'h> {
    mc: &'a mut Mutation<'h>,
    /// Set when an instruction fails. The machine halts and the driver reports
    /// it, exactly as `eval::Machine` does.
    pub error: Option<EvalError>,
    halted: bool,
}

impl<'a, 'h> Vm<'a, 'h> {
    /// Prepare to run `function` in `env`, discarding whatever was in
    /// progress.
    pub fn start(mc: &'a mut Mutation<'h>, function: Value<'h>, env: Value<'h>) -> Vm<'a, 'h> {
        let wanted = mc
            .slot(function, function::MAX_STACK)
            .as_fixnum()
            .unwrap_or(0)
            .max(0) as usize;
        let stack = mc.alloc(ObjectType::Vector, wanted.max(INITIAL_STACK));
        mc.set_register(Register::Function, function);
        mc.set_register(Register::Operands, stack);
        mc.set_register(Register::Env, env);
        mc.set_register(Register::Value, Value::NIL);
        mc.set_register(Register::Values, Value::UNBOUND);
        mc.set_register(Register::Unwind, Value::NIL);
        mc.set_pc(0);
        mc.set_sp(0);
        mc.set_fp(0);
        Vm {
            mc,
            error: None,
            halted: false,
        }
    }

    /// Continue a computation already in the registers.
    pub fn resume(mc: &'a mut Mutation<'h>) -> Vm<'a, 'h> {
        Vm {
            mc,
            error: None,
            halted: false,
        }
    }

    /// The answer, once halted.
    pub fn result(&self) -> Value<'h> {
        self.mc.register(Register::Value)
    }

    // ---- the operand stack -------------------------------------------------

    fn capacity(&self) -> usize {
        let stack = self.mc.register(Register::Operands);
        self.mc.header(stack).slots
    }

    /// Make room for `extra` more slots, growing the stack if needed.
    ///
    /// Growing allocates, which is why it happens once per call — against
    /// `max-stack`, which the compiler worked out — rather than once per push.
    /// A JVM class file carries the same field for the same reason.
    fn ensure(&mut self, extra: usize) {
        let wanted = self.mc.sp() + extra;
        if wanted <= self.capacity() {
            return;
        }
        let mut bigger = self.capacity().max(INITIAL_STACK);
        while bigger < wanted {
            bigger *= 2;
        }
        let old = self.mc.register(Register::Operands);
        let new = self.mc.alloc(ObjectType::Vector, bigger);
        for i in 0..self.mc.sp() {
            let v = self.mc.slot(old, i);
            self.mc.set_slot(new, i, v);
        }
        self.mc.set_register(Register::Operands, new);
    }

    fn push(&mut self, v: Value<'h>) {
        self.ensure(1);
        let stack = self.mc.register(Register::Operands);
        let sp = self.mc.sp();
        self.mc.set_slot(stack, sp, v);
        self.mc.set_sp(sp + 1);
    }

    /// Push a value this instruction has just computed.
    ///
    /// **The multiple-values invariant lives here.** A form that produces one
    /// value must leave the `Values` register saying so, or a stale second
    /// value survives into the next form that looks — and surfaces a long way
    /// from whatever set it. The interpreter had one delivery point and
    /// chapter 23 paid for this in one line; a bytecode VM has an arm per
    /// opcode, so the rule is: anything computing a fresh value pushes it
    /// with *this*, and only `VALUES`, `VALUES-LIST` and a `RET` passing a
    /// callee's values through may use [`Vm::push`] directly.
    fn push_fresh(&mut self, v: Value<'h>) {
        self.mc.set_register(Register::Values, Value::UNBOUND);
        self.push(v);
    }

    /// Pop, clearing the slot behind.
    ///
    /// The clearing is not tidiness. Every slot of a traced object is traced,
    /// so a dead slot above the stack pointer would keep whatever it last
    /// held alive for as long as the stack lives — a leak with no symptom.
    /// One store is cheaper than teaching the collector about a boundary that
    /// moves.
    /// Underflow is a `debug_assert` rather than an error, because a program
    /// that pops an empty stack is not a program with a bug in it — it is a
    /// sequence of bytes that is not a program at all, and the compiler's
    /// `max-stack` accounting is what rules it out. The JVM draws the line in
    /// the same place and calls the thing that draws it a *verifier*: one
    /// pass over the code when it is loaded, rather than a test on every pop.
    /// Quokka has no verifier yet and should; #28 carries it.
    fn pop(&mut self) -> Value<'h> {
        let sp = self.mc.sp();
        debug_assert!(sp > 0, "popped an empty operand stack");
        let stack = self.mc.register(Register::Operands);
        let v = self.mc.slot(stack, sp - 1);
        self.mc.set_slot(stack, sp - 1, Value::NIL);
        self.mc.set_sp(sp - 1);
        v
    }

    /// Pop `n` values, keeping the order they were pushed in.
    ///
    /// Popping reverses, so this fills the vector backwards. Getting it the
    /// other way round would make `(- 10 4)` compute 4 − 10, which is the
    /// kind of mistake that passes every test using `+`.
    fn pop_n(&mut self, n: usize) -> Vec<Value<'h>> {
        let mut out = vec![Value::NIL; n];
        for i in (0..n).rev() {
            out[i] = self.pop();
        }
        out
    }

    // ---- stepping ----------------------------------------------------------

    fn fail(&mut self, e: EvalError) -> Transition {
        self.error = Some(e);
        self.halted = true;
        Transition::Halt
    }

    fn halt_with(&mut self, v: Value<'h>) -> Transition {
        self.mc.set_register(Register::Value, v);
        self.halted = true;
        Transition::Halt
    }

    fn constant(&mut self, k: i32) -> Result<Value<'h>, EvalError> {
        let f = self.mc.register(Register::Function);
        let constants = self.mc.slot(f, function::CONSTANTS);
        let k = k as usize;
        if !constants.is_object() || k >= self.mc.header(constants).slots {
            return Err(EvalError::BadSyntax {
                operator: "a compiled function",
                detail: "names a constant it does not have",
            });
        }
        Ok(self.mc.slot(constants, k))
    }

    fn jump(&mut self, from: usize, offset: i32) -> Transition {
        // Offsets are relative to the end of the jump, which is what makes a
        // single backpatching pass possible: the assembler knows the end as
        // soon as it has emitted the instruction.
        let target = from as i64 + offset as i64;
        if target < 0 {
            return self.fail(EvalError::BadSyntax {
                operator: "a jump",
                detail: "lands before the start of the code",
            });
        }
        self.mc.set_pc(target as usize);
        Transition::Continue
    }
}

impl Steppable for Vm<'_, '_> {
    fn step(&mut self) -> Transition {
        if self.halted {
            return Transition::Halt;
        }
        // Checked first, exactly as the interpreter does: past the hard limit
        // the heap will not grow, so continuing allocates into a ceiling.
        if self.mc.is_exhausted() {
            return self.fail(EvalError::HeapExhausted);
        }

        let f = self.mc.register(Register::Function);
        let code = self.mc.slot(f, function::CODE);
        let length = self
            .mc
            .slot(f, function::CODE_LENGTH)
            .as_fixnum()
            .unwrap_or(0) as usize;
        let at = self.mc.pc();
        let decoded = match code::decode_in_heap(self.mc, code, length, at) {
            Ok(d) => d,
            Err(CodeError::UnknownOpcode { .. }) => {
                return self.fail(EvalError::BadSyntax {
                    operator: "bytecode",
                    detail: "is not an instruction",
                })
            }
            Err(CodeError::Truncated { .. }) => {
                // Running off the end means the function had no RET, which is
                // a bug in whatever produced it rather than in the program.
                return self.fail(EvalError::BadSyntax {
                    operator: "a compiled function",
                    detail: "ran past the end of its code without returning",
                });
            }
        };
        self.mc.set_pc(decoded.next);

        match decoded.op {
            Op::Nil => self.push_fresh(Value::NIL),
            Op::T => self.push_fresh(Value::T),
            Op::Fix => {
                let v = Value::fixnum(decoded.a as i64).expect("a 16-bit operand is a fixnum");
                self.push_fresh(v);
            }
            Op::Const => match self.constant(decoded.a) {
                Ok(v) => self.push_fresh(v),
                Err(e) => return self.fail(e),
            },
            Op::Pop => {
                self.pop();
            }
            Op::Prim => {
                let args = self.pop_n(decoded.b as usize);
                match primitives::call_slice(self.mc, decoded.a as u32, &args) {
                    Ok(v) => self.push_fresh(v),
                    Err(e) => return self.fail(e),
                }
            }
            Op::Jmp => return self.jump(decoded.next, decoded.a),
            Op::JmpNil => {
                let v = self.pop();
                if v.is_nil() {
                    return self.jump(decoded.next, decoded.a);
                }
            }
            Op::JmpNotNil => {
                let v = self.pop();
                if !v.is_nil() {
                    return self.jump(decoded.next, decoded.a);
                }
            }
            Op::Ret => {
                let v = self.pop();
                // Slice 2 has no frames yet, so every RET is the outermost
                // one. `CALL` arrives with the frame layout, in slice 4.
                return self.halt_with(v);
            }
            other => {
                return self.fail(EvalError::BadSyntax {
                    operator: other.name(),
                    detail: "is specified but not implemented in the VM yet",
                })
            }
        }
        Transition::Continue
    }
}

/// Run `function` to completion, for tests and for the REPL once there is a
/// compiler. The interpreter's `eval_now` with a different machine.
pub fn run_now<'h>(
    mc: &mut Mutation<'h>,
    function: Value<'h>,
    budget: u64,
) -> Result<Value<'h>, EvalError> {
    use crate::slice::{run_slice, Status};
    let mut vm = Vm::start(mc, function, Value::NIL);
    match run_slice(&mut vm, budget) {
        Status::Done { .. } => match vm.error.take() {
            Some(e) => Err(e),
            None => Ok(vm.result()),
        },
        Status::Yielded => Err(EvalError::BudgetExhausted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::encode;
    use crate::heap::Heap;
    use crate::slice::{run_slice, Status};

    /// Primitive indices, pinned where they are used so that a reordering of
    /// the table shows up here rather than as arithmetic that is quietly
    /// wrong. `primitives::the_table_order_is_pinned` is the other half.
    const ADD: i32 = 0;
    const SUB: i32 = 1;
    const MUL: i32 = 2;

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
        code::make_function(mc, &bytes, table, Value::NIL, 8, name)
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

    #[test]
    fn a_byte_that_is_not_an_instruction_halts_rather_than_running_on() {
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
        assert!(message.contains("is not an instruction"), "{message}");
    }

    /// A function with no `RET` is a bug in whoever produced it. Running off
    /// the end must say so rather than reading whatever is next in the heap.
    #[test]
    fn running_off_the_end_is_an_error_not_an_adventure() {
        let mut heap = Heap::new();
        let message = heap.mutate(|mc| {
            let f = asm(mc, &[(Op::Fix, 1, 0)]);
            match run_now(mc, f, 100) {
                Ok(_) => String::from("ran, which it should not have"),
                Err(e) => format!("{e}"),
            }
        });
        assert!(message.contains("without returning"), "{message}");
    }

    /// Everything specified but not yet built says so by name, rather than
    /// doing nothing and leaving a wrong answer to be explained later.
    #[test]
    fn an_unimplemented_opcode_names_itself() {
        let message = run(&[(Op::Mvcall, 1, 0)]);
        assert!(message.contains("MVCALL"), "{message}");
        assert!(message.contains("not implemented"), "{message}");
    }
}
