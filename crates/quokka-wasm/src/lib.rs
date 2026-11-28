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
//!
//! # The environment
//!
//! Since phase 4 (#137, `docs/display.md`) a page can also [`start`] a
//! program instead of the read-eval-print loop: the environment's command
//! loop, which shows things through `%display` messages and waits for the
//! person through `%await-event`. This host keeps two queues for it. The
//! messages the world sent wait in one until the page takes them
//! ([`take_messages`]); the events the page posted wait in the other until
//! the world asks for one ([`post_event`]). A world waiting for an event
//! with none queued, and nothing else to run, is **idle**: [`run`] returns
//! false at once, and the page stops running slices until the person does
//! something. An idle world costs the computer nothing.
//!
//! [`start`]: Quokka::start
//! [`take_messages`]: Quokka::take_messages
//! [`post_event`]: Quokka::post_event
//! [`run`]: Quokka::run

use std::collections::VecDeque;

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
    /// A program the page started, which runs until it returns (#137).
    Program,
}

/// A world, owned by the page.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Debug)]
pub struct Quokka {
    heap: Heap,
    /// Console output and printed values the page has not collected yet.
    output: String,
    doing: Doing,
    /// The program the page started, to start again after an interrupt.
    program: Option<String>,
    /// Display messages the page has not collected yet.
    messages: Vec<String>,
    /// Events the world has not asked for yet.
    events: VecDeque<String>,
    /// How many times in a row the started program has failed without
    /// once waiting for the person: three, and it is not started again.
    failures: u32,
}

/// Send a panic's message to the browser's console before the trap.
///
/// On wasm32 a panic cannot be caught: it becomes a trap, the call into the
/// module throws a `RuntimeError`, and the instance is not to be used again.
/// Without a hook the only trace is "unreachable executed" (#88). With one,
/// the page's console says which line of the machine failed — and the page,
/// which keeps a snapshot of the world from before the line it ran, offers
/// that to save. A panic is a bug in the machine, never an answer to input.
#[cfg(target_arch = "wasm32")]
fn report_panics() {
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        fn error(message: &str);
    }
    std::panic::set_hook(Box::new(|info| {
        error(&format!("Quokka's machine failed: {info}"))
    }));
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
        #[cfg(target_arch = "wasm32")]
        report_panics();
        let mut heap = image::load(image, SOFT_LIMIT_WORDS).map_err(|e| e.to_string())?;
        // The page has no console to read, so no debugger to talk through:
        // off, whatever the world was saved with (#131).
        heap.mutate(|mc| {
            let switch = mc.intern("%DEBUGGER-ENABLED");
            mc.set_slot(
                switch,
                quokka_vm::heap::symbol::VALUE,
                quokka_vm::value::Value::NIL,
            );
        });
        Ok(Quokka {
            heap,
            output: String::new(),
            doing: Doing::Nothing,
            program: None,
            messages: Vec::new(),
            events: VecDeque::new(),
            failures: 0,
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

    /// Start the global function NAME, with the argument nil, in place of
    /// the read-eval-print loop: a program that runs until it returns, such
    /// as the environment's command loop, `%RUN-ENVIRONMENT` (#137).
    ///
    /// NAME is looked up in the system table, where the library's names
    /// are, as a host's `%host-step` is.
    ///
    /// # Errors
    /// If something is running already, or NAME is not a function.
    pub fn start(&mut self, name: &str) -> Result<(), String> {
        if self.doing != Doing::Nothing {
            return Err("something is running already".to_string());
        }
        self.failures = 0;
        self.begin_program(name)
    }

    /// Queue an event for the world: the text of an S-expression
    /// (`docs/display.md`), which it reads when it next asks.
    ///
    /// An edit says the whole of an editor's text, so one that is still
    /// waiting when the next for the same editor arrives says nothing the
    /// new one does not: it is replaced. Typing faster than the world
    /// reads no longer queues a copy of the text for every character
    /// (#194). Only the last event queued is looked at, so an edit never
    /// moves past a key.
    pub fn post_event(&mut self, event: &str) {
        let same_editor = match (self.events.back(), edit_target(event)) {
            (Some(waiting), Some(target)) => edit_target(waiting) == Some(target),
            _ => false,
        };
        if same_editor {
            self.events.pop_back();
        }
        self.events.push_back(event.to_string());
    }

    /// Every display message the world has sent since the last call, and
    /// forget them. One S-expression each, one after the other, so the
    /// whole is read as a sequence of forms.
    pub fn take_messages(&mut self) -> String {
        core::mem::take(&mut self.messages).join("\n")
    }

    /// Run at most `steps` steps. True if there is more to do; false when
    /// nothing is running, or when the world is idle, waiting for an event
    /// the page has not posted (#137).
    ///
    /// The page picks `steps` from how long the last call took, so that a
    /// call lasts about a frame — the budget is in steps and not
    /// milliseconds so that what a program computes cannot depend on how
    /// fast the machine is (design, *Slices*).
    pub fn run(&mut self, steps: u32) -> bool {
        let mut left = u64::from(steps);
        while self.doing != Doing::Nothing && left > 0 {
            // Answer whatever can be answered — a process may have parked
            // on the page while others ran (#34) — and stop if nothing can
            // go on without the person.
            if !self.answer_all() {
                self.failures = 0;
                return false;
            }
            match vm::one_slice(&mut self.heap, left) {
                Ok(Status::Yielded) => left = 0,
                Ok(Status::Blocked { steps }) => left = left.saturating_sub(steps.max(1)),
                Ok(Status::Done { steps }) => {
                    left = left.saturating_sub(steps.max(1));
                    self.finished();
                }
                Err(e) => {
                    self.output.push_str(&format!("; {e}\n"));
                    vm::recover(&mut self.heap);
                    // As the native loop does: on to the next form, or the
                    // program from the start, unless it keeps failing
                    // before it ever waits, which starting again would
                    // repeat for ever. This call ends here either way.
                    left = 0;
                    self.failures += 1;
                    if self.program.is_some() && self.failures >= 3 {
                        self.output.push_str(
                            "; the program failed three times running, and was stopped\n",
                        );
                        self.doing = Doing::Nothing;
                        self.program = None;
                    } else {
                        self.restart();
                    }
                }
            }
        }
        self.doing != Doing::Nothing && self.answer_all()
    }

    /// Give up the line that is running. What it has changed stays changed.
    /// A program the page started is started again, and told so with the
    /// event `(:interrupted)` (#137): the environment's command loop keeps
    /// its state in the heap, and redraws from it.
    pub fn interrupt(&mut self) {
        if self.doing != Doing::Nothing {
            // The form runs its cleanups before it is let go of (#173):
            // dropped where it stood, it left locks held and the
            // listener's own state half changed.
            vm::stop(&mut self.heap);
            // Stop means stop: a thread that runs for ever kept the page
            // working at full speed with no way to end it but to have kept
            // the thread (#195). Each is told to end and runs its cleanups
            // when the world next runs.
            vm::end_background_processes(&mut self.heap);
            self.doing = Doing::Nothing;
            if self.program.is_some() {
                self.events.push_front("(:interrupted)".to_string());
                self.restart();
            } else {
                self.output.push_str("; interrupted\n");
            }
        }
    }

    /// How many times the heap has been collected, and how many words the
    /// last collection found alive. The page times each call of `run`; one
    /// in which this count rose took at least a collection's pause, which
    /// is the measurement the decision on the collector waits for (#86,
    /// #154). The world itself is never told: when a collection happens
    /// depends on how its run is cut into slices, which it must not see.
    pub fn collections(&self) -> u32 {
        self.heap.collections().min(u64::from(u32::MAX)) as u32
    }

    /// See [`Quokka::collections`].
    pub fn live_words(&self) -> u32 {
        self.heap.live_words().min(u32::MAX as usize) as u32
    }

    /// Is a line, or a program, still running? An idle program is running.
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

    /// Start the program NAME, counting failures on from before.
    fn begin_program(&mut self, name: &str) -> Result<(), String> {
        vm::begin_call(&mut self.heap, name).map_err(|e| e.to_string())?;
        self.doing = Doing::Program;
        self.program = Some(name.to_string());
        Ok(())
    }

    /// After a machine error: the program from the start, or the loop's
    /// next form.
    fn restart(&mut self) {
        match self.program.clone() {
            Some(name) => {
                self.doing = Doing::Nothing;
                if let Err(e) = self.begin_program(&name) {
                    self.output.push_str(&format!("; {e}\n"));
                }
            }
            None => self.begin(Doing::Step, ""),
        }
    }

    /// The function that was running has returned.
    fn finished(&mut self) {
        if self.doing == Doing::Program {
            self.doing = Doing::Nothing;
            self.program = None;
            return;
        }
        if self.doing == Doing::Step {
            match vm::step_outcome(&mut self.heap) {
                // The line is used up.
                None => {
                    self.doing = Doing::Nothing;
                    return;
                }
                Some(Outcome::Value(text)) => self.output.push_str(&format!("{text}\n")),
                Some(Outcome::Error { message, .. }) => {
                    self.output.push_str(&format!("; {message}\n"))
                }
                Some(Outcome::Failed(e)) => self.output.push_str(&format!("; {e}\n")),
            }
        }
        self.begin(Doing::Step, "");
    }

    /// Answer every request that can be answered now. True if the world
    /// can go on: a process can run once they are answered. False if every
    /// process is waiting for an event that has not come.
    fn answer_all(&mut self) -> bool {
        // One at a time: each answer takes a request off the list, so the
        // places of the rest change.
        loop {
            let waiting = host::waiting(&mut self.heap);
            let Some((index, reply)) = waiting
                .into_iter()
                .enumerate()
                .find_map(|(i, request)| self.reply(request).map(|r| (i, r)))
            else {
                break;
            };
            host::answer(&mut self.heap, index, reply);
        }
        vm::can_run(&mut self.heap)
    }

    /// The answer to REQUEST, or `None` to leave it waiting.
    fn reply(&mut self, request: Request) -> Option<Result<Option<String>, HostError>> {
        Some(match request {
            // Both go to the page's console strip; in the environment the
            // listener binds `*error-output*` to itself and shows it there.
            Request::WriteConsole(text) | Request::WriteError(text) => {
                self.output.push_str(&text);
                Ok(Some(String::new()))
            }
            Request::Display(message) => {
                self.messages.push(message);
                Ok(Some(String::new()))
            }
            Request::ReadSource(name) => Err(HostError::NotFound(name)),
            // The page has no console: the end of the input, as for any
            // host with nobody at one (#131). The environment's listener
            // is the console in the page, through its own stream (#141).
            Request::ReadConsole(_) => Ok(None),
            Request::AwaitEvent => match self.events.pop_front() {
                Some(event) => Ok(Some(event)),
                // The environment waits for the person; the read-eval-print
                // loop has nobody to wait for, and is told so.
                None if self.doing == Doing::Program => return None,
                None => Ok(None),
            },
        })
    }
}

/// The editor an `:edit` event is for, as the text of its record number;
/// `None` for any other event.
fn edit_target(event: &str) -> Option<&str> {
    let rest = event.strip_prefix("(:edit :target ")?;
    rest.split(' ').next()
}

#[cfg(test)]
#[path = "unit/lib.rs"]
mod tests;
