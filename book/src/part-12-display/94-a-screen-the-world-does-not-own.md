# A screen the world does not own

> *Where this fits.* Phases 0 to 3 built a Lisp that runs in a browser tab
> and talks to a person through a console: a line of text in, a line of
> text out, and since chapter 89 a debugger in the same terminal. Phase 4
> gives the world a screen. This chapter is the plan for the whole phase,
> the protocol between a world and the page that shows it, and the first
> piece of it to be built: a host that can leave a request waiting, so
> that a world can wait for a person without costing anything.

*Status: the plan is issue #134 and `docs/display.md`, steps A and C
(#135, #137), done. The rest of the phase is chapters 95 onwards.*

## What the gate asks

Phase 4's gate is one sentence: *edit and evaluate a form in the browser*.
The dev console page has evaluated forms in the browser since chapter 31,
so the sentence must mean more than that, and the design says what. The
form is in a **viewer of live text**, the listener, where every earlier
form stays editable and every result stays on the page; and what the
person sees is drawn by Lisp, through a **display protocol**, by a
**renderer** in the page that does as it is told. The dev console is a
text box and a `<div>` that JavaScript appends to. Nothing in it can be
changed from inside the world.

The three tutorials in `docs/tutorials/` show what has to follow. A Tower
of Hanoi draws disks and moves them when you press a key; a Rolodex has a
list, a search field and an editor; a platform game draws every frame and
plays sounds. None of them is phase 4's work, but each of them will be
written with what phase 4 builds, so phase 4 has to build it in their
shape: output that is a tree of objects the world owns, viewers that
redraw from their model, keys that are bindings to commands, layout that
the world decides.

## Who draws what

The rule that has held since chapter 1, Rust below the primitive table and
Lisp above it, decides most of the plan. A person must be able to change
how anything looks from inside the world: the colour of a prompt, what a
key does, where a viewer goes. So all of that is Lisp. What is left for the
page is what only the page can do: put text on the screen, take keys and
clicks, and let the browser do the things it does well, such as editing
text with its caret, selection, spelling and input methods.

So a program writes to a stream, as Common Lisp programs always have, and
what it writes becomes **output records**: objects in the world, in a tree
for each viewer, each with a number that names it. The world sends the
page **messages** describing the records, and what changed in them; a small
renderer turns messages into the browser's page and remembers only which
record each piece of the page is. What the person does comes back as
**events**. Messages and events are the display protocol. CLIM, the Common
Lisp Interface Manager of the early 1990s, worked in the same way: output
records in Lisp, and a backend that draws them on whatever screen there is.

`docs/display.md` records thirteen decisions about it. Three shape
everything after them, and are worth stating here.

The first is what a message is. We chose **S-expressions**, the same
notation as the language, in both directions: the world prints a message
with its own printer and the page reads it with a reader of forty lines,
and an event is the reverse. JSON would be faster for the browser to read,
but the world would need a second notation, written and read by code of
its own. SLIME, the Emacs environment for Common Lisp, has talked to Lisp
in S-expressions over a socket since 2003, for the same reason.

The second is that the **browser edits text and the world keeps it**. A
text area in the page is edited by the browser itself; every change is
sent to the world as an event, and the world keeps the text. The keys the
world wants, such as Enter to evaluate, are announced to the page in
advance, because the page has to decide at once whether the browser may
act on a key and cannot wait for the world's answer.

The third is where the person's input is waited for. The environment's
top level is a **command loop**: wait for an event, do what it asks, send
what changed, and wait again. A form that reads input, `(read-line)` or the
debugger's prompt, runs the same loop inside itself until its line arrives.
That is how Genera's listener read, and it means that waiting for the
person is always the same thing: one process asking the host for the next
event.

## Waiting without spinning

A world asks its host for things with **host requests** (chapter 31): a
primitive leaves a request in the heap, the machine stops, and the host
answers between slices. Two new primitives use it. `%display` hands the
host a message and is answered at once. `%await-event` asks for the next
event, and is answered when there is one:

```rust
            let wanted = usize::from(kind != "%AWAIT-EVENT");
            if args.len() != wanted {
                return Some(self.fail(EvalError::WrongArgumentCount {
                    name: kind,
                    wanted,
                    got: args.len(),
                }));
            }
```

Until now every host answered every request it saw, at once, and saw them
one at a time: `host::pending` shows the running process's request first,
then the others' round the ring of processes (chapter 90). A host that does
*not* answer `%await-event` until someone presses a key would then be stuck
looking at it, and a thread that wanted to `%display` something in the
meantime would wait behind it for ever. So the host can now ask for the
whole list, `host::waiting`, and answer one request by its place in it,
`host::answer`. The list is good until the next slice runs, which is all a
host needs, since it answers between slices.

The browser host goes through the list and answers what it can:

```rust
    fn reply(&mut self, request: Request) -> Option<Result<Option<String>, HostError>> {
        Some(match request {
            Request::WriteConsole(text) => {
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
```

`None` leaves a request waiting. When every process is waiting on the host
and nothing on the list can be answered, the world is **idle**: there is
nothing a slice could do. `run`, which the page calls from a timer, then
returns false at once, and the page stops calling it until the person does
something; posting the event and calling `run` again wakes the world. An
idle world costs the computer nothing, which matters on a laptop's battery
and in a background tab.

This is the reason the command loop waits in one process. Waiting in Lisp,
as the threads of chapter 91 wait for a lock or a channel, is a loop that
yields: a waiting thread is still runnable, so the machine is never idle.
That was a fine price in a terminal, where a person waiting at a prompt
has the whole computer anyway. In a page it would mean a tab using a whole
core while nobody touches it. Waiting on the host is the one kind of
waiting the machine can see, so the environment waits only that way.

## Starting a program, and stopping one

The page used to give the world a line at a time, through the
read-eval-print loop of chapter 40. Now it can also `start` a function by
name, which runs until it returns: the environment's command loop, once
there is one. And the page's Stop button, which used to abandon the line
that was running, now abandons whatever the program was doing and starts
it again, with an event that says so:

```rust
    pub fn interrupt(&mut self) {
        if self.doing != Doing::Nothing {
            vm::recover(&mut self.heap);
            self.doing = Doing::Nothing;
            if self.program.is_some() {
                self.events.push_front("(:interrupted)".to_string());
                self.restart();
            } else {
                self.output.push_str("; interrupted\n");
            }
        }
    }
```

Starting again loses nothing a person can see, because everything the
command loop knows is in the heap, in its records and viewers; only the
computation that was running is gone. A better interrupt would stop the
running form in the debugger, as Ctrl-C does in SBCL. That needs the
machine to break into a process between two instructions and run a
function there, which chapter 90 designed and nobody has built, so this
is recorded as provisional.

The tests drive the same object the page drives, natively. One starts a
small program that shows each event it is given, upcased, and checks that
the world is idle before the first event, that two events come back as two
messages in order, and that the program's end ends the run:

```rust
#[test]
fn events_wake_the_world_and_messages_come_back() {
    let mut q = echo();
    settle(&mut q);
    q.post_event("hello");
    assert_eq!(settle(&mut q), "HELLO");
    q.post_event("one");
    q.post_event("two");
    assert_eq!(settle(&mut q), "ONE\nTWO");
    q.post_event("stop");
    settle(&mut q);
    assert!(!q.busy(), "the program returned");
}
```

Another has a thread display a message while the main process waits for
an event, which is the case `host::waiting` exists for.

## What did it cost?

Two primitives, two kinds of request, and the two functions that list and
answer waiting requests, about 150 lines of Rust in all, most of it the
browser host's new loop and its tests. No Lisp: what uses the requests
comes in the next chapters. Planning found one thing the design had
promised for phase 4 that phase 4 does not need: weak references, which
`docs/design.md` had placed here "with the output records that first need
them". Records are reached from their viewers, and nothing in this phase
looks records up from anything else, so weak references wait for the
first thing that does.
