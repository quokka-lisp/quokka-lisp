//! Processes (#34, phase 3 step E): several computations in one world, taking
//! turns on one machine.
//!
//! The heap's registers — the operand stack, the frame and mark pointers, the
//! program counter, the function, the environment and the rest — are one
//! computation's. A **process** is a heap object holding a saved set of them,
//! and switching processes stores the running set into the running process's
//! object and loads another's. The registers are then the running process's,
//! cached; `Register::Process` says which one that is, and the processes are
//! a ring, through their `NEXT` slots, which the scheduler walks round.
//!
//! Special variables are bound shallowly (#40): the value is in the symbol,
//! and a binding record on the process's stack holds the value from before.
//! So a switch also **swaps the bindings**: switching out puts back, record by
//! record from the innermost, the value each symbol had outside the process,
//! keeping the process's own in the record; switching in does the opposite,
//! from the outermost. This is how Genera's processes kept their bindings.
//!
//! The object is opaque to Lisp: only these functions read or write it. A
//! program that could write a saved stack pointer could make the machine read
//! outside a stack, which no input may (rule 10); `Heap::verify` checks a
//! saved set as it checks the live one.
//!
//! Scheduling is in the machine (`Vm::step`): round robin, a process running
//! until its slice ends, it yields, it blocks on a host request, or it
//! finishes. What a program does with processes — threads, locks, waiting —
//! is Lisp (`lisp/library/threads.lisp`).

use crate::heap::{symbol, Mutation, ObjectType, Register};
use crate::host::request;
use crate::value::Value;
use crate::vm::mark;

/// Whether the process can run: `ALIVE`, or how it ended.
pub const STATE: usize = 0;
/// The next process in the ring.
pub const NEXT: usize = 1;
/// How it ended: its values as a list, or the machine's error as a string.
pub const RESULT: usize = 2;
/// `t` for the main process, the one each host call runs in.
pub const MAIN: usize = 3;
/// Whatever the library keeps for it: its `thread` object.
pub const DATA: usize = 4;
/// The saved registers, from here on.
pub const ENV: usize = 5;
/// The `Value` register.
pub const VALUE: usize = 6;
/// The `Unwind` register.
pub const UNWIND: usize = 7;
/// The `Values` register.
pub const VALUES: usize = 8;
/// The `Function` register.
pub const FUNCTION: usize = 9;
/// The `Operands` register: the process's own stack.
pub const OPERANDS: usize = 10;
/// The `Request` register: a host request it is waiting on, or `nil`.
pub const REQUEST: usize = 11;
/// The program counter, a fixnum.
pub const PC: usize = 12;
/// The stack pointer, a fixnum.
pub const SP: usize = 13;
/// The frame pointer, a fixnum.
pub const FP: usize = 14;
/// The mark pointer, a fixnum; -1 for no record.
pub const MP: usize = 15;
/// `t` when the process has been told to end and has not yet begun to:
/// `%process-terminate` asks, and the machine acts at the process's next
/// step at which ending is safe (`Vm::step`).
pub const TERMINATE: usize = 16;
/// How many slots.
pub const SLOTS: usize = 17;

/// Running, or able to run.
pub const ALIVE: i64 = 0;
/// Its function returned; `RESULT` is the list of its values.
pub const FINISHED: i64 = 1;
/// `terminate` unwound it to its base.
pub const TERMINATED: i64 = 2;
/// The machine stopped it with an error of its own (a fault, or a Lisp
/// error with nothing to catch it); `RESULT` is the message.
pub const FAILED: i64 = 3;

/// Is V a process?
pub fn is<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::Process
}

fn fixnum<'h>(n: i64) -> Value<'h> {
    Value::fixnum(n).expect("a stack index or a state is a fixnum")
}

fn index(v: Value<'_>) -> usize {
    match v.as_fixnum() {
        Some(n) if n >= 0 => n as usize,
        _ => usize::MAX,
    }
}

/// The process that is running, making the ring if there is none yet: the
/// computation in the registers becomes the main process, alone in it.
pub fn current<'h>(mc: &mut Mutation<'h>) -> Value<'h> {
    let running = mc.register(Register::Process);
    if is(mc, running) {
        return running;
    }
    let main = blank(mc);
    mc.set_slot(main, MAIN, Value::T);
    mc.set_slot(main, NEXT, main);
    mc.set_register(Register::Process, main);
    main
}

/// A process object with nothing in it: no stack, alive.
fn blank<'h>(mc: &mut Mutation<'h>) -> Value<'h> {
    let p = mc.alloc(ObjectType::Process, SLOTS);
    for slot in 0..SLOTS {
        mc.set_slot(p, slot, Value::NIL);
    }
    mc.set_slot(p, STATE, fixnum(ALIVE));
    mc.set_slot(p, VALUES, Value::UNBOUND);
    mc.set_slot(p, UNWIND, Value::UNBOUND);
    for slot in [PC, SP, FP] {
        mc.set_slot(p, slot, fixnum(0));
    }
    mc.set_slot(p, MP, fixnum(-1));
    p
}

/// A new process that will run TRAMPOLINE, a compiled function of no
/// arguments, on a stack of its own of STACK slots, put in the ring after
/// the running one. It runs when the scheduler next comes round to it.
pub fn make<'h>(mc: &mut Mutation<'h>, trampoline: Value<'h>, stack: usize) -> Value<'h> {
    let running = current(mc);
    let p = blank(mc);
    let operands = mc.alloc(ObjectType::Vector, stack);
    mc.set_slot(p, FUNCTION, trampoline);
    mc.set_slot(p, OPERANDS, operands);
    let after = mc.slot(running, NEXT);
    mc.set_slot(p, NEXT, after);
    mc.set_slot(running, NEXT, p);
    p
}

/// The main process, if there is a ring.
pub fn main<'h>(mc: &Mutation<'h>) -> Option<Value<'h>> {
    ring(mc)
        .into_iter()
        .find(|p| mc.slot(*p, MAIN) != Value::NIL)
}

/// Every process, the running one first, then round the ring.
pub fn ring<'h>(mc: &Mutation<'h>) -> Vec<Value<'h>> {
    let start = mc.register(Register::Process);
    let mut found = Vec::new();
    if !is(mc, start) {
        return found;
    }
    let mut p = start;
    loop {
        found.push(p);
        p = mc.slot(p, NEXT);
        // `Heap::verify` keeps every `NEXT` a process; the bound is for a
        // ring that does not come back to its start, which no code makes.
        if p == start || !is(mc, p) || found.len() > mc.used_words() {
            return found;
        }
    }
}

/// Is P alive? Whether it can run also depends on its request.
pub fn alive<'h>(mc: &Mutation<'h>, p: Value<'h>) -> bool {
    mc.slot(p, STATE).as_fixnum() == Some(ALIVE)
}

/// The request record P is blocked on, if the host has not answered it.
fn waiting<'h>(mc: &Mutation<'h>, p: Value<'h>) -> Option<Value<'h>> {
    let record = if p == mc.register(Register::Process) {
        mc.register(Register::Request)
    } else {
        mc.slot(p, REQUEST)
    };
    waits(mc, record).then_some(record)
}

/// Is RECORD a request the host has not answered?
fn waits<'h>(mc: &Mutation<'h>, record: Value<'h>) -> bool {
    record.is_object()
        && mc.header(record).object_type == ObjectType::Vector
        && mc.header(record).slots == request::SLOTS
        && mc.symbol_is(mc.slot(record, request::STATE), request::WAITING)
}

/// Can P run now: alive, and not waiting for the host?
pub fn runnable<'h>(mc: &Mutation<'h>, p: Value<'h>) -> bool {
    alive(mc, p) && waiting(mc, p).is_none()
}

/// The first process after the running one, round the ring, that can run;
/// `None` if no other can.
pub fn next_runnable<'h>(mc: &Mutation<'h>) -> Option<Value<'h>> {
    ring(mc).into_iter().skip(1).find(|p| runnable(mc, *p))
}

/// A request some process is waiting on, for the host to answer: the
/// running process's first, then the others', round the ring (#34: a
/// process blocked on the host is *parked* while the others run).
pub fn waiting_request<'h>(mc: &Mutation<'h>) -> Option<Value<'h>> {
    if !is(mc, mc.register(Register::Process)) {
        let running = mc.register(Register::Request);
        return waits(mc, running).then_some(running);
    }
    ring(mc).into_iter().find_map(|p| waiting(mc, p))
}

/// Can some process run now? With no processes, whether the one computation
/// is not waiting on the host.
pub fn can_run(mc: &Mutation<'_>) -> bool {
    if !is(mc, mc.register(Register::Process)) {
        return !waits(mc, mc.register(Register::Request));
    }
    ring(mc).into_iter().any(|p| runnable(mc, p))
}

/// Every request a process is waiting on: the main process's first, then
/// the others', round the ring (#137, `host::waiting`). The main one first,
/// because a host hands the first event request it finds the next event,
/// and the environment's command loop is the main process's.
pub fn waiting_requests<'h>(mc: &Mutation<'h>) -> Vec<Value<'h>> {
    if !is(mc, mc.register(Register::Process)) {
        let running = mc.register(Register::Request);
        return if waits(mc, running) {
            vec![running]
        } else {
            Vec::new()
        };
    }
    let mut processes = ring(mc);
    if let Some(at) = processes
        .iter()
        .position(|p| mc.slot(*p, MAIN) != Value::NIL)
    {
        let main = processes.remove(at);
        processes.insert(0, main);
    }
    processes
        .into_iter()
        .filter_map(|p| waiting(mc, p))
        .collect()
}

/// Is the request the host will answer next the main process's, or the only
/// computation's when there are no processes? Only a line the person typed
/// for the main process renews the host's step budget (`vm::answer`): a
/// background thread reading lines must not keep a runaway form alive.
pub fn main_is_asking(mc: &Mutation<'_>) -> bool {
    match main(mc) {
        None => true,
        Some(main) => waiting(mc, main).is_some() && waiting(mc, main) == waiting_request(mc),
    }
}

/// Make P the running process, if it is not already: the running one's
/// bindings swapped out and its registers stored, P's registers loaded and
/// its bindings swapped in.
pub fn switch_to<'h>(mc: &mut Mutation<'h>, p: Value<'h>) {
    let running = mc.register(Register::Process);
    if running == p || !is(mc, running) || !is(mc, p) {
        return;
    }
    swap_bindings(mc, false);
    save(mc, running);
    load(mc, p);
    mc.set_register(Register::Process, p);
    swap_bindings(mc, true);
}

/// Make the main process the running one: what a host call runs in.
pub fn focus_main(mc: &mut Mutation<'_>) {
    if let Some(main) = main(mc) {
        switch_to(mc, main);
    }
}

/// The registers into P.
fn save<'h>(mc: &mut Mutation<'h>, p: Value<'h>) {
    let registers = [
        (ENV, mc.register(Register::Env)),
        (VALUE, mc.register(Register::Value)),
        (UNWIND, mc.register(Register::Unwind)),
        (VALUES, mc.register(Register::Values)),
        (FUNCTION, mc.register(Register::Function)),
        (OPERANDS, mc.register(Register::Operands)),
        (REQUEST, mc.register(Register::Request)),
        (PC, fixnum(mc.pc() as i64)),
        (SP, fixnum(mc.sp() as i64)),
        (FP, fixnum(mc.fp() as i64)),
        (
            MP,
            fixnum(if mc.mp() == usize::MAX {
                -1
            } else {
                mc.mp() as i64
            }),
        ),
    ];
    for (slot, v) in registers {
        mc.set_slot(p, slot, v);
    }
}

/// P's saved registers into the machine.
fn load<'h>(mc: &mut Mutation<'h>, p: Value<'h>) {
    for (slot, register) in [
        (ENV, Register::Env),
        (VALUE, Register::Value),
        (UNWIND, Register::Unwind),
        (VALUES, Register::Values),
        (FUNCTION, Register::Function),
        (OPERANDS, Register::Operands),
        (REQUEST, Register::Request),
    ] {
        let v = mc.slot(p, slot);
        mc.set_register(register, v);
    }
    let pc = index(mc.slot(p, PC));
    let sp = index(mc.slot(p, SP));
    let fp = index(mc.slot(p, FP));
    let mp = index(mc.slot(p, MP));
    mc.set_pc(if pc == usize::MAX { 0 } else { pc });
    mc.set_sp(if sp == usize::MAX { 0 } else { sp });
    mc.set_fp(if fp == usize::MAX { 0 } else { fp });
    mc.set_mp(mp);
}

/// Exchange each special binding's value in the running process's records
/// with its symbol's: from the innermost record outwards when the process
/// is leaving (INWARD false), from the outermost inwards when it arrives.
fn swap_bindings(mc: &mut Mutation<'_>, inward: bool) {
    let stack = mc.register(Register::Operands);
    if !stack.is_object() || mc.header(stack).object_type != ObjectType::Vector {
        return;
    }
    let slots = mc.header(stack).slots;
    let mut records = Vec::new();
    let mut at = mc.mp();
    // Each record's PREV is below it, which `Heap::verify` cannot see but
    // every push makes true; a chain that does not go down is not followed.
    while at != usize::MAX && at + mark::SIZE <= slots {
        if mc.slot(stack, at + mark::KIND).as_fixnum() == Some(mark::SPECIAL) {
            records.push(at);
        }
        let prev = index(mc.slot(stack, at + mark::PREV));
        if prev != usize::MAX && prev >= at {
            break;
        }
        at = prev;
    }
    if inward {
        records.reverse();
    }
    for at in records {
        let symbol = mc.slot(stack, at + mark::DATUM);
        if !mc.is_symbol(symbol) {
            continue;
        }
        let kept = mc.slot(stack, at + mark::SAVED);
        let now = mc.slot(symbol, symbol::VALUE);
        mc.set_slot(symbol, symbol::VALUE, kept);
        mc.set_slot(stack, at + mark::SAVED, now);
    }
}

/// Take P, which has ended and is not running, out of the ring: a process
/// that has ended is kept by whoever holds it, to ask how it ended, and
/// not by the scheduler, which would otherwise walk past every one ever
/// made. Its `NEXT` is itself, so it is still a ring of one, as
/// `Heap::verify` wants every process's `NEXT` to be a process.
pub fn unlink<'h>(mc: &mut Mutation<'h>, p: Value<'h>) {
    if p == mc.register(Register::Process) || alive(mc, p) {
        return;
    }
    let after = mc.slot(p, NEXT);
    if let Some(before) = ring(mc).into_iter().find(|q| mc.slot(*q, NEXT) == p) {
        mc.set_slot(before, NEXT, after);
    }
    mc.set_slot(p, NEXT, p);
}

/// The running process, which is not the main one, has ended in STATE
/// with RESULT: say so, and let go of its stack.
pub fn end<'h>(mc: &mut Mutation<'h>, state: i64, result: Value<'h>) {
    let p = mc.register(Register::Process);
    if !is(mc, p) {
        return;
    }
    mc.set_slot(p, STATE, fixnum(state));
    mc.set_slot(p, RESULT, result);
    mc.set_register(Register::Request, Value::NIL);
    mc.set_register(Register::Unwind, Value::UNBOUND);
    mc.set_register(Register::Operands, Value::NIL);
    mc.set_sp(0);
    mc.set_fp(0);
    mc.set_mp(usize::MAX);
}
