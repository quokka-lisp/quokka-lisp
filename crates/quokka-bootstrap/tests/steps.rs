//! The step benchmark (#169): what common operations cost, in steps of the
//! machine, held to a baseline as ansi-test's passes are.
//!
//! Until the review after phase 4 the system was measured for being right
//! and never for being fast, and a generic function call had come to cost
//! 1,446 steps beside 4 for a plain one without anything noticing. A step
//! count is the measure that needs no quiet machine: the machine has no
//! clock, a slice is counted in steps, and the same world given the same
//! forms takes the same steps every time, on every computer. So a rise is
//! a change someone made, and this test says which form it made dearer.
//!
//! `tests/steps/setup.lisp` defines what the forms use; `forms.txt` names
//! the forms; `baseline.txt` holds a count for each. A run fails if a form
//! costs more than its baseline, or has none. When a change makes forms
//! cheaper, or adds one, run with `STEPS_UPDATE=1` and commit the baseline
//! with the change:
//!
//! ```text
//! STEPS_UPDATE=1 cargo test -p quokka-bootstrap --test steps
//! ```
//!
//! A count that must rise, because a fix costs something, is raised the
//! same way, on purpose and in the commit that says why.

use quokka_bootstrap::boot;
use quokka_vm::heap::{Heap, Register};
use quokka_vm::slice::Status;
use quokka_vm::value::Value;
use quokka_vm::vm;

const SETUP: &str = include_str!("../../../tests/steps/setup.lisp");
const FORMS: &str = include_str!("../../../tests/steps/forms.txt");
const BASELINE: &str = "tests/steps/baseline.txt";

/// How many times each form is evaluated; its cost is the mean.
const TIMES: u64 = 1000;

/// A slice long enough to be rare and short enough that the heap is
/// collected as it is in a host: a collection happens between slices.
const SLICE: u64 = 200_000;

/// Read, evaluate and print every form of SOURCE in the world, as a host
/// does, and return the steps it took.
fn steps_of(heap: &mut Heap, source: &str) -> u64 {
    let mut host = quokka_vm::host::NullHost::default();
    let mut run = |heap: &mut Heap, name: &str| -> u64 {
        vm::begin_global(heap, name).expect("begins");
        let mut steps = 0;
        loop {
            match vm::one_slice(heap, SLICE).expect("runs") {
                Status::Done { steps: last } => return steps + last,
                Status::Yielded => steps += SLICE,
                Status::Blocked { steps: last } => {
                    steps += last;
                    assert!(
                        quokka_vm::host::serve(heap, &mut host),
                        "asked a host nothing"
                    );
                }
            }
        }
    };
    heap.mutate(|mc| {
        quokka_vm::process::focus_main(mc);
        let text = mc.alloc_string(source);
        mc.set_register(Register::Value, text);
    });
    run(heap, "%HOST-SOURCE");
    let mut steps = 0;
    loop {
        heap.mutate(|mc| {
            quokka_vm::process::focus_main(mc);
            mc.set_register(Register::Value, Value::NIL)
        });
        steps += run(heap, "%HOST-STEP");
        match vm::step_outcome(heap) {
            None => return steps,
            Some(vm::Outcome::Value(_)) => {}
            Some(other) => panic!("{source}\n  did not evaluate: {other:?}"),
        }
    }
}

/// The benchmarks, as (name, form).
fn forms() -> Vec<(&'static str, &'static str)> {
    FORMS
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (name, form) = line.split_once(": ").expect("a name, a colon, a form");
            (name, form)
        })
        .collect()
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
#[cfg_attr(target_arch = "wasm32", ignore)]
fn no_form_costs_more_steps_than_its_baseline() {
    // The counts are the machine's, not the collector's, but which steps a
    // table keyed by address takes depends on where a collection put its
    // keys; under GC torture that differs, and the run is the ordinary one's.
    if quokka_vm::heap::TORTURE {
        return;
    }
    let mut heap = Heap::new();
    boot::build_image(&mut heap).expect("builds");
    steps_of(&mut heap, SETUP);

    // Each form in a function of its own, compiled once, so that what is
    // measured is the form and not the reading and compiling of it.
    let forms = forms();
    let mut definitions = String::from("(defun bench-empty () (dotimes (i 1000) nil))\n");
    for (index, (_, form)) in forms.iter().enumerate() {
        definitions.push_str(&format!(
            "(defun bench-{index} () (dotimes (i {TIMES}) {form}))\n"
        ));
    }
    steps_of(&mut heap, &definitions);
    // Once each before measuring: a first call fills caches.
    for index in 0..forms.len() {
        steps_of(&mut heap, &format!("(bench-{index})"));
    }
    let empty = steps_of(&mut heap, "(bench-empty)");
    let measured: Vec<(&str, u64)> = forms
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            let steps = steps_of(&mut heap, &format!("(bench-{index})"));
            (*name, (steps.saturating_sub(empty) + TIMES / 2) / TIMES)
        })
        .collect();

    let path = root().join(BASELINE);
    if std::env::var_os("STEPS_UPDATE").is_some() {
        let mut text = String::from(
            "# Steps of the machine for one evaluation of each form in forms.txt.\n\
             # Written by: STEPS_UPDATE=1 cargo test -p quokka-bootstrap --test steps\n",
        );
        for (name, steps) in &measured {
            text.push_str(&format!("{name} {steps}\n"));
        }
        std::fs::write(&path, text).expect("writes the baseline");
        return;
    }

    let baseline = std::fs::read_to_string(&path).expect("tests/steps/baseline.txt");
    let allowed: std::collections::HashMap<&str, u64> = baseline
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let (name, steps) = line.rsplit_once(' ')?;
            Some((name, steps.parse().ok()?))
        })
        .collect();
    let mut dearer = Vec::new();
    let mut cheaper = Vec::new();
    for (name, steps) in &measured {
        match allowed.get(name) {
            None => dearer.push(format!("{name}: {steps} steps, and no baseline")),
            Some(was) if steps > was => dearer.push(format!("{name}: {steps} steps, was {was}")),
            Some(was) if steps < was => cheaper.push(format!("{name}: {steps} steps, was {was}")),
            Some(_) => {}
        }
    }
    if !cheaper.is_empty() {
        println!(
            "cheaper than the baseline; run with STEPS_UPDATE=1 and commit it:\n  {}",
            cheaper.join("\n  ")
        );
    }
    assert!(
        dearer.is_empty(),
        "forms cost more steps than tests/steps/baseline.txt allows.\n  {}\n\
         If the rise is meant, run with STEPS_UPDATE=1 and say why in the commit.",
        dearer.join("\n  ")
    );
}
