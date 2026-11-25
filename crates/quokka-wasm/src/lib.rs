//! Quokka in the browser: a world, loaded from an image and run in slices.
//!
//! # What a browser host has to be
//!
//! A native host may wait. It calls [`quokka_vm::vm::eval_source`], which runs
//! a line of input to its end, and the person at the terminal waits with it.
//! A page may not: JavaScript runs on the thread that also paints and takes
//! input, so a function that does not return is a frozen tab. That is the
//! whole reason for rule 4 — *the VM must be able to yield* — and this crate
//! is where the rule is finally used rather than merely kept.
//!
//! So nothing here runs to its end. [`Quokka::submit`] hands a line to the
//! world and returns at once; [`Quokka::run`] runs a bounded number of steps
//! and says whether there is more to do; the page calls it again from a
//! timer, and between calls the browser paints. The computation's state is
//! in the heap, as it always was, so *between calls there is nothing in
//! flight on the Rust side* but which of the loop's two functions was
//! called last (the private `Doing`) — the trick is the one an operating system plays
//! on a process, and it works for the same reason: everything the process
//! knows is in memory the kernel can leave alone.
//!
//! # Requests
//!
//! The world asks for things by leaving a request in the heap and blocking
//! (`quokka_vm::host`). This host answers them itself with `host::pending`
//! and `host::complete`, as that module says a browser host must: a console
//! write is appended to the output the page will collect, and a file is not
//! found, until there is a file host to find it (design, *Files and
//! storage*). When there is, the answer will arrive from a Promise, and the
//! only change here is that `run` returns while it waits.
//!
//! Before #84 this crate booted the tier-0 interpreter and evaluated each
//! line in one mutation: no world, no tier 1, no collection during a form,
//! and a page that could not repaint until the form ended.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use quokka_vm::heap::Heap;
use quokka_vm::host::{self, HostError, Request};
use quokka_vm::slice::Status;
use quokka_vm::vm::{self, Outcome};
use quokka_vm::{image, PHASE, VERSION};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// The soft limit a loaded world is given, in words: 4 MiB.
///
/// The hard limit is sixty-four times this, 256 MiB, and a copying collector
/// needs that twice over — which is as much of wasm32's 4 GiB as one tab
/// should take. The line where a collection is wanted moves with what is
/// alive (`Heap::wants_collection`), so this is a floor and not a ceiling.
pub const SOFT_LIMIT_WORDS: usize = 1 << 19;

/// Which of the read-eval-print loop's functions is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Doing {
    /// Nothing: waiting for a line.
    Nothing,
    /// `%host-source`, taking the line in.
    Source,
    /// `%host-step`, reading, evaluating and printing one form.
    Step,
}

/// A world, owned by the page.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug)]
pub struct Quokka {
    heap: Heap,
    /// Console output and printed values the page has not collected yet.
    output: String,
    doing: Doing,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Quokka {
    /// Load a world from the bytes of an image — `quokka build world.qi`.
    ///
    /// # Errors
    /// If the bytes are not an image this build can read; the message says
    /// which way.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(image: &[u8]) -> Result<Quokka, String> {
        let heap = image::load(image, SOFT_LIMIT_WORDS).map_err(|e| e.to_string())?;
        Ok(Quokka {
            heap,
            output: String::new(),
            doing: Doing::Nothing,
        })
    }

    /// The banner the dev console shows on load.
    pub fn banner(&self) -> String {
        format!(
            "Quokka {VERSION} — phase {PHASE} (wasm) · a world of {} words",
            self.heap.used_words()
        )
    }

    /// Give the world a line to read, evaluate and print. Returns at once;
    /// [`run`](Self::run) does the work.
    ///
    /// # Errors
    /// If the last line is still running. The page waits, or calls
    /// [`interrupt`](Self::interrupt).
    pub fn submit(&mut self, input: &str) -> Result<(), String> {
        if self.doing != Doing::Nothing {
            return Err("still running the last line".to_string());
        }
        self.begin(Doing::Source, input);
        Ok(())
    }

    /// Run at most `steps` steps. True if there is more to do.
    ///
    /// The page picks `steps` from how long the last call took, so that a
    /// call lasts about a frame — the budget is in steps and not
    /// milliseconds so that what a program computes cannot depend on how
    /// fast the machine is (design, *Slices*).
    pub fn run(&mut self, steps: u32) -> bool {
        let mut left = u64::from(steps);
        while self.doing != Doing::Nothing && left > 0 {
            match vm::one_slice(&mut self.heap, left) {
                Ok(Status::Yielded) => left = 0,
                Ok(Status::Blocked { steps }) => {
                    left = left.saturating_sub(steps.max(1));
                    self.answer();
                }
                Ok(Status::Done { steps }) => {
                    left = left.saturating_sub(steps.max(1));
                    self.finished();
                }
                Err(e) => {
                    self.output.push_str(&format!("; {e}\n"));
                    vm::recover(&mut self.heap);
                    // As the native loop does: on to the next form.
                    self.begin(Doing::Step, "");
                }
            }
        }
        self.doing != Doing::Nothing
    }

    /// Give up the line that is running. What it has changed stays changed.
    pub fn interrupt(&mut self) {
        if self.doing != Doing::Nothing {
            vm::recover(&mut self.heap);
            self.doing = Doing::Nothing;
            self.output.push_str("; interrupted\n");
        }
    }

    /// Is a line still running?
    pub fn busy(&self) -> bool {
        self.doing != Doing::Nothing
    }

    /// Everything written since the last call, and forget it.
    pub fn take_output(&mut self) -> String {
        core::mem::take(&mut self.output)
    }

    /// The world as an image, to download and load again — mid-computation
    /// included, which is phase 1's gate.
    pub fn save(&mut self) -> Vec<u8> {
        image::save(&mut self.heap)
    }

    /// How many words the heap is using. The dev console shows it, so the
    /// collector is visible rather than theoretical.
    pub fn heap_words(&self) -> usize {
        self.heap.used_words()
    }
}

impl Quokka {
    /// Start one of the loop's two functions.
    fn begin(&mut self, what: Doing, input: &str) {
        let began = match what {
            Doing::Source => vm::begin_source(&mut self.heap, input),
            _ => vm::begin_step(&mut self.heap),
        };
        self.doing = match began {
            Ok(()) => what,
            Err(e) => {
                self.output.push_str(&format!("; {e}\n"));
                Doing::Nothing
            }
        };
    }

    /// The function that was running has returned.
    fn finished(&mut self) {
        if self.doing == Doing::Step {
            match vm::step_outcome(&mut self.heap) {
                // The line is used up.
                None => {
                    self.doing = Doing::Nothing;
                    return;
                }
                Some(Outcome::Value(text)) => self.output.push_str(&format!("{text}\n")),
                Some(Outcome::Error(text)) => self.output.push_str(&format!("; {text}\n")),
                Some(Outcome::Failed(e)) => self.output.push_str(&format!("; {e}\n")),
            }
        }
        self.begin(Doing::Step, "");
    }

    /// Answer the request the world is blocked on.
    fn answer(&mut self) {
        let reply = match host::pending(&mut self.heap) {
            Some(Request::WriteConsole(text)) => {
                self.output.push_str(&text);
                Ok(String::new())
            }
            Some(Request::ReadSource(name)) => Err(HostError::NotFound(name)),
            // Blocked on something this host has never heard of: it cannot
            // be answered, so the line cannot go on.
            None => {
                self.output
                    .push_str("; the world asked its host for something it cannot give\n");
                vm::recover(&mut self.heap);
                self.doing = Doing::Nothing;
                return;
            }
        };
        host::complete(&mut self.heap, reply);
    }
}

#[cfg(test)]
#[path = "unit/lib.rs"]
mod tests;
