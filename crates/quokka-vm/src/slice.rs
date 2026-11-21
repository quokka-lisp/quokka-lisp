//! Slices: how the VM gives control back.
//!
//! A browser tab has one thread and no preemption: once JavaScript calls into
//! WASM, nothing can take the CPU away until that call returns. A Lisp
//! interpreter that evaluates `(loop)` by looping in Rust therefore freezes the
//! page. CLAUDE.md rule 4 exists for this reason — the VM runs in *slices* and
//! returns to its caller between them.
//!
//! The consequence is larger than it looks. The evaluator cannot hold its state
//! on the Rust call stack, because returning to the browser destroys that stack.
//! All execution state — the form being evaluated, the environment, the pending
//! frames — lives in our own heap instead, and the evaluator becomes an explicit
//! state machine. This is SICP §5.4's *explicit-control evaluator*. (It is also
//! what the Rust compiler does to an `async fn`: our [`run_slice`] is a
//! hand-written `Future::poll`, written by hand because the state has to be
//! GC-visible Lisp objects that the debugger can inspect.)
//!
//! Having paid for that, three other things in `docs/design.md` become easy:
//! reified frames for the debugger, saving an image mid-computation and resuming
//! it, and green threads (several machines and a scheduler that picks which one
//! to step).
//!
//! # Who decides when a slice ends
//!
//! **Decided: the caller passes a budget.** [`run_slice`] takes a number of
//! steps and returns when they are spent. The alternatives were for the VM to
//! poll the host, or for the host to set a wall-clock deadline that the VM
//! watches. Both put timing inside the VM; this keeps it outside, in the host,
//! where measuring time is cheap and cannot affect what the VM computes.
//!
//! That buys determinism: the same program takes the same path regardless of how
//! it was sliced, so a run can be replayed exactly and the phase 3 scheduler can
//! be seeded rather than raced. It also makes `run_slice(m, 1)` a single-step
//! debugger primitive, and leaves the native host with nothing to implement —
//! it passes `u64::MAX` and never thinks about slices again.
//!
//! The browser gets the responsiveness of a deadline by adjusting the budget
//! from how long the last slice actually took:
//!
//! ```js
//! let budget = 50_000;
//! function pump() {
//!   const t0 = performance.now();
//!   const status = vm.run_slice(budget);
//!   const elapsed = performance.now() - t0;
//!   budget = Math.max(1000, Math.round(budget * 8 / Math.max(elapsed, 0.1)));
//!   if (status === "yielded") requestAnimationFrame(pump);
//! }
//! ```
//!
//! # What this requires of everything else
//!
//! A budget only bounds time if a single step is short. A primitive that sorts a
//! million elements in one call blows any budget, and no scheduler can help once
//! control is inside Rust. Every primitive must therefore be either fast or
//! itself chunked. See rule 9 in `CLAUDE.md`.
//!
//! Two invariants keep slices semantically invisible, which is the whole point:
//! collect garbage on allocation rather than on slice boundaries, and never let
//! Lisp observe how much budget is left.

/// What one step of a machine did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// The machine advanced and has more work to do.
    Continue,
    /// The machine has finished. Further steps also report `Halt`.
    Halt,
    /// The machine has asked the host for something and cannot go on until it
    /// is answered. The request is in the heap (see `crate::host`); further
    /// steps also report `Block` until the host completes it.
    Block,
}

/// How a slice ended.
///
/// `Blocked` was promised here in phase 0 — *"the machine reports what it
/// needs, the host fetches it, and the next call resumes with the answer"* —
/// and deliberately left out until there was a real caller, so that the shape
/// of a request would be decided by one rather than guessed. The caller is
/// `%read-source` (#35), and the shape is in `crate::host`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The machine halted before the budget ran out.
    Done {
        /// Steps actually taken. The host uses this to avoid inflating its
        /// budget estimate from a slice that finished almost immediately.
        steps: u64,
    },
    /// The budget ran out. All state is in the machine; call again to continue.
    Yielded,
    /// The machine is waiting for the host to answer a request.
    ///
    /// Not a failure and not the end: the host reads the request with
    /// `host::pending`, does the work in whatever way suits it — at once, or
    /// when a browser Promise resolves — answers with `host::complete`, and
    /// calls again. Calling again *without* answering is harmless and returns
    /// `Blocked` straight away, having done nothing.
    ///
    /// The machine cannot tell how long it waited, which is what keeps
    /// slicing semantically invisible (rule 4) even across I/O.
    Blocked {
        /// Steps taken in this slice, including the one that made the request.
        steps: u64,
    },
}

/// Something [`run_slice`] can drive.
///
/// The evaluator will implement this. It is a trait rather than an inherent
/// method so that the slice accounting can be tested on its own, before there is
/// an evaluator to test it with.
pub trait Steppable {
    /// Perform exactly one state transition.
    ///
    /// Implementations must not loop for an unbounded time and must not recurse
    /// into themselves: a step that does not return promptly defeats slicing.
    fn step(&mut self) -> Transition;
}

/// Run `machine` for at most `budget` steps.
///
/// Returns [`Status::Done`] if it halted, [`Status::Yielded`] if the budget ran
/// out first. Yielding consumes exactly `budget` steps, so only `Done` reports a
/// count.
///
/// A budget of zero always yields without stepping, even for a machine that has
/// already halted; the next call with a real budget reports `Done { steps: 1 }`,
/// having spent one step learning that there was nothing to do.
pub fn run_slice<M>(machine: &mut M, budget: u64) -> Status
where
    M: Steppable + ?Sized,
{
    let mut steps = 0;
    while steps < budget {
        steps += 1;
        match machine.step() {
            Transition::Continue => {}
            Transition::Halt => return Status::Done { steps },
            Transition::Block => return Status::Blocked { steps },
        }
    }
    Status::Yielded
}

#[cfg(test)]
#[path = "unit/slice.rs"]
mod tests;
