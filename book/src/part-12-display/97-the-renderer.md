# The renderer

> *Where this fits.* Chapter 96 built output records and the messages that
> describe them, and tested them by reading the messages as text. This
> chapter is the page's half: a small program that turns messages into the
> browser's document and the person's keys into events. It is the first
> JavaScript in Quokka that does more than run slices, and the place where
> the rule "Rust or JavaScript only for what Lisp cannot do" is easiest to
> break.

*Status: done, issue #140, step F of phase 4.*

## What the renderer may decide

Almost nothing. The renderer draws what it is told, where it is told, in
the colours the theme gives the names it is sent. It does not know what a
listener is, what Enter means or where a viewer goes. Everything a person
might want to change is in the world, by rule 1, and the renderer is the
part a person cannot change from inside the world: it is in the page, not
in the image. So every question about the screen is first asked of the
world, and the renderer gets only what the world cannot do itself, which is
to touch the browser.

That leaves three files in `crates/quokka-wasm/www/`. `sexp.js` reads and
prints the protocol's S-expressions, about a hundred lines. `renderer.js`
holds a model and a view. And `index.html` is the host: it loads the world,
starts the environment, and runs slices while the world has work to do.

## A model that knows no browser

The renderer is split in two so that most of it can be tested without a
browser. The **model** applies messages to a tree of plain JavaScript
objects, one for each record the page has, kept in a map by the record's
number:

```js
      case "remove": {
        const node = this.nodes.get(args[0]);
        if (!node) return;
        if (node.parent) {
          const siblings = node.parent.children;
          siblings.splice(siblings.indexOf(node), 1);
        }
        this.forget(node);
        this.notify("remove", node);
        break;
      }
```

The **view** listens to the model and changes the one element each change
is about: a text record is a `<span>`, a block a `<div>`, a viewer a
`<section>` with a title, a split a flexible box whose children's weights
are their `flex`. A message the renderer does not understand is skipped and
the rest of its batch still applies, so an older page can show a newer
world without failing.

Because the model knows no browser, it runs in Node. `scripts/smoke-wasm.mjs`
loads a real world into the WASM machine, starts the environment, types
into the listener by posting events, and asks the model to print the
screen as text. That printout is the "text-dump renderer" the design's
testing table asked for, and the test compares it with the whole screen:

```text
== Listener ==
CL-USER> [(defun twice (x) (* 3 x))]
~TWICE~
edited · ran again below
CL-USER> [(twice 21)]
42
CL-USER> [(defun twice (x) (* 3 x))]
↑ from 2 forms up
TWICE
CL-USER> [(twice 21)]
63
CL-USER> []
```

Brackets are editors and tildes are struck-out text. Chapter 98 is the
story of that screen.

## Editing in the browser

An editor is a `<textarea>` that grows with its text. The browser does the
editing: the caret, selection, copy and paste, an input method for Japanese,
a screen reader. Every change is sent to the world as an event with the
whole text and the selection, `(:edit :target 17 :text "(+ 1 2" :start 6
:end 6)`, so the world always knows what the editor holds. We chose a
`<textarea>` over `contenteditable`, the browser's other way of editing
text, because its caret and selection are the same in every engine. The
price is that its text is all one colour, so coloured code waits for phase
6's editor, behind the same `:editor` kind.

Keys are harder. When you press Enter in the listener, the world decides
whether that runs the form or starts a new line, but the browser has to
know *at once*, before the key event returns, whether to insert the
newline. The world is in another part of the page's work and answers only
after a slice. So the world tells the renderer in advance which keys it
wants: each editor carries a key table, `("Enter" "Meta+Enter"
"Control+Enter")` for a form in the listener, and the renderer stops the
browser's own action for exactly those keys and sends them as events.

```js
    area.addEventListener("keydown", (event) => {
      const keys = node.props.keys;
      if (Array.isArray(keys) && keys.includes(keyName(event))) {
        event.preventDefault();
        this.post(keyEvent(node.id, event, area));
      }
    });
```

Shift-Enter is not in the table, so it makes a new line as a text area
always does. A key event carries the caret's place too, since the person may
have moved it with the arrow keys after the last edit, and an Enter that
inserts a newline must insert it there.

## The theme

The world sends names and never CSS. The page's style sheet gives every
ink a colour twice, once for a light screen and once for a dark one, from
the brand palette in `assets/README.md`: cream and brown in the light
theme, dark teal and mint in the dark. A text style becomes classes,
`q-family-fixed`, `q-face-bold`. A few properties name a way of laying
out or marking text, never a measurement: `:layout :row` sets a block's
children side by side, which is how the listener's prompt sits in front of
its form, and the listener's own `:strike`, `:note` and `:bar` mark struck
results, small notes and printed output.

## The page

`index.html` is the environment now, and the dev console the browser build
has had since phase 0 moved to `console.html`, unchanged, because a page that evaluates a
line with no renderer at all is still the quickest check that the browser
build works. The environment page runs slices while the world has work,
sizing each from how long the last one took, hands the messages to the
view, and stops when the world is idle; a key or an edit posts an event and
starts it again. Its Stop button, and Cmd-period, interrupt a form that
will not end.

A form after `#` in the address is typed into the listener and entered, as
a person would: the page sets the focused editor's text, sends the input
event the browser would send, and presses Enter. `scripts/smoke-browser.sh`
loads both pages that way in a headless Chrome and looks for the answer in
each document.

## What did it cost?

Two small JavaScript modules and the page's style sheet, and no Rust.
Testing in a real browser found one problem the tests could not: the
browser kept old copies of `renderer.js` while we changed it, because the
development server sends no caching headers, so the preview configuration
in `.claude/launch.json` serves the page with caching off.
