//! Tests for [`crate::slice`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

use super::*;

/// A machine that does nothing except count down, so the slice accounting
/// can be tested without an evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Countdown {
    remaining: u64,
    ticks: u64,
}

impl Countdown {
    fn new(n: u64) -> Self {
        Countdown {
            remaining: n,
            ticks: 0,
        }
    }
}

impl Steppable for Countdown {
    fn step(&mut self) -> Transition {
        if self.remaining == 0 {
            return Transition::Halt;
        }
        self.remaining -= 1;
        self.ticks += 1;
        Transition::Continue
    }
}

#[test]
fn finishing_inside_the_budget_reports_the_steps_taken() {
    let mut m = Countdown::new(3);
    // Three steps of work, then a fourth that discovers the machine halted.
    assert_eq!(run_slice(&mut m, 100), Status::Done { steps: 4 });
    assert_eq!(m.ticks, 3);
}

#[test]
fn spending_the_budget_yields_and_keeps_all_state() {
    let mut m = Countdown::new(10);
    assert_eq!(run_slice(&mut m, 4), Status::Yielded);
    assert_eq!(m.ticks, 4);
    assert_eq!(m.remaining, 6);
}

#[test]
fn a_yielded_machine_resumes_where_it_stopped() {
    let mut m = Countdown::new(10);
    assert_eq!(run_slice(&mut m, 4), Status::Yielded);
    assert_eq!(run_slice(&mut m, 4), Status::Yielded);
    assert_eq!(run_slice(&mut m, 100), Status::Done { steps: 3 });
    assert_eq!(m.ticks, 10);
}

#[test]
fn a_zero_budget_yields_without_stepping() {
    let mut m = Countdown::new(1);
    assert_eq!(run_slice(&mut m, 0), Status::Yielded);
    assert_eq!(m.ticks, 0);
}

/// The property the budget shape was chosen for: how a computation is
/// sliced must not change what it computes. If this ever fails, replay,
/// seeded scheduling and the differential tests all lose their footing.
#[test]
fn slicing_does_not_change_the_result() {
    let reference = {
        let mut m = Countdown::new(100);
        while run_slice(&mut m, u64::MAX) == Status::Yielded {}
        m
    };

    for budget in [1, 2, 3, 7, 10, 99, 100, 101, 1000] {
        let mut m = Countdown::new(100);
        let mut slices = 0;
        while run_slice(&mut m, budget) == Status::Yielded {
            slices += 1;
            assert!(slices < 1000, "budget {budget} failed to make progress");
        }
        assert_eq!(m, reference, "budget {budget} produced a different state");
    }
}
