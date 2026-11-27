# Editing a form in the browser

> *Where this fits.* Phase 4's gate is *edit and evaluate a form in the
> browser*. Chapters 94 to 98 built what that needs: a protocol between a
> world and its page, streams a program can define, output records and
> viewers, a renderer, and the listener. This chapter shows the gate
> passing, says how it is tested, and what it does and does not mean.

*Status: done, issue #142, step H of phase 4.*

## The gate, in a browser

Open the page, and the listener is there with its prompt. Type a form and
press Enter, and its value appears under it and a new prompt below. Go
back up to the form, change it, and press Cmd-Enter, and it runs again as a
new entry at the bottom, with the old result struck through. Every part of
what you see was drawn by Lisp in the world, through messages, by a
renderer that decided nothing; every key you pressed that mattered was
sent to the world and acted on there.

Three tests hold the gate, one for each half of the system and one for the
whole.

1. The world's half, in Rust: `crates/quokka-wasm/src/unit/lib.rs` drives a
   world through the object the page drives, posting the events the page
   would post, and checks the messages that come back. One test is named
   for the gate, `a_form_entered_in_the_listener_is_evaluated`.
2. The world and the renderer's model, in Node: `scripts/smoke-wasm.mjs`
   loads the WASM build, types a definition, a call, an edited definition
   and the call again, and compares the model's text dump of the screen,
   the screen of chapter 98, character for character.
3. The whole page, in a headless Chrome: `scripts/smoke-browser.sh` opens
   `index.html` with a form after `#` in the address, which the page types
   into the listener's editor and enters as a person would, and looks for
   the value in the document.

The third runs in CI on every push, with the second, as the dev console's
checks did before; both run against a world that CI builds from the
source.

## What it means

Quokka is graphical now, in the sense the design meant when it said the
system would be usable in a terminal from phase 0 and graphical from phase
4. More to the point, the screen is the world's. A person who wants the
prompt to say something else defines a method; a viewer that draws a
model is a class and a method; a key does what a Lisp function says. None
of the three tutorials can be written yet, but each of them would now be
written on what exists: records, styles and inks, viewers that redraw from
their model, key tables, the command loop.

## What it does not mean

There is one viewer, and the window manager that splits the screen is
phase 6's, though the protocol already describes splits. Results are text:
clicking one does nothing, which is phase 5's gate. Stopping a form that
will not end abandons it and starts the command loop again, rather than
interrupting it into the debugger. An editor is a plain text area, so code
is not coloured, and there is no structural editing yet. At the gate the
renderer drew no pictures; chapter 100 gives it shapes. And the world, once the
page is closed, is gone unless you saved it with the button, because
saving as you go is phase 6's journal.

## What did it cost?

The gate itself cost the page's way of typing a form from its address,
and the two smoke tests. Phase 4 so far is chapters 94 to 99: five library
files of Lisp, two JavaScript modules and a page, and in Rust three
primitives, two kinds of host request and a host that can leave a request
waiting. The fixpoint, the check that the compiler compiles itself into
the same code generation after generation (chapter 42), held through all
of it, and ansi-test stayed at 12,568 passing, with no plain failure.
