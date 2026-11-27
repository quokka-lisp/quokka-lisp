# The display protocol and live text

Phase 4 (#134 tracks it). Its gate is to **edit and evaluate a form in the
browser**, and its scope in `docs/design.md` is a display protocol, a browser
renderer and one live-text viewer. This file records how they are built and
why; the book's Part XII tells the story as it happened. Everything here is
built for the phases after it too: phase 5 puts presentations and commands on
these records, and phase 6 puts the window manager, the browser and the
debugger viewer on these viewers. `docs/design.md`'s *Environment and UI* and
*Display: hybrid renderer* sections are the target this file serves; where a
decision here refines one of theirs, it says so.

> **Status (27 Nov 2026):** decided while planning phase 4, and built in
> steps B to I (#136–#143). Step H met the gate: a form is typed into the
> listener in a browser, evaluated, edited and run again. Step I added
> drawing. Where building changed a decision, the decision below says so.

## The pieces

A program draws by writing to a stream, as a Common Lisp program always has:
`(format t "~D cards" n)`, `(print x)`, and later `(present card 'card)` and
`(draw-circle* 60 60 40)`. What it writes becomes **output records**, Lisp
objects in a tree the world owns, one tree per viewer. The world sends the
page a description of those records, and of what changed in them, as
**messages**; a small **renderer** in the page turns messages into the DOM
and keeps nothing else. What the person does, typing or clicking, comes back
to the world as **events**. Messages and events are the display protocol.

```
 Lisp, in the world                          JavaScript, in the page
 ─────────────────────────────────────       ──────────────────────────
 a program        output records      messages        renderer       DOM
 (format, print,  ─────────────────►  ─────────►     ─────────►
  present, draw)  per viewer, with                   id → node
                  identities          ◄─────────
 viewers, the     ◄─────────────────   events        key, edit,
 command loop                                        click
```

Everything left of the arrows is Lisp (rule 1), so that a person can change
how anything looks or behaves from inside the world. The renderer knows a
closed set of record kinds and how to draw each; it never decides what to
draw, where a viewer goes, or what a key does.

## Decisions

### 1. Messages and events are S-expressions

A message is a list, printed by the world's own printer under
`with-standard-io-syntax` and read in the page by a reader of forty lines; an
event is a list printed by the page and read by the world's reader. Only
integers, double floats, strings, keywords and lists cross.

Two other shapes were considered. JSON is what a browser parses fastest, but
the world would need a JSON writer and reader of its own, and a person
reading the protocol in a debugger would read a second notation. Converting
heap objects to JavaScript objects in Rust would cost the world no steps to
encode, but it puts a walker over Lisp data below the primitive table, needs
the `js-sys` crate, and gives the native test host nothing to look at.
S-expressions are what SLIME and Swank have used between Emacs and a Lisp
since 2003 for the same reason: both ends already read and print them. A
message is a string, so the native host, the browser and a server pushing
over a WebSocket all carry it the same way, and a test compares strings.

### 2. Two host requests, and a host that may leave one waiting

`(%display message)` hands the host a message, and the host answers at once
with nothing. `(%await-event)` asks the host for the next event, and the host
answers when it has one, which may be much later. Both are host requests
(#35): the machine stops, the host answers between slices, and nothing on the
Rust side waits.

`%await-event` is the first request a host does not answer at once. Until
now a browser host answered every request it saw, and `host::pending` showed
it one at a time, the running process's first; a host that leaves the event
request waiting would then never see another process's `%display`. So the
host learns to list every waiting request and answer one by its place in the
list (`host::waiting`, `host::answer`). The machine is **idle** when every
process is parked on the host and the host has nothing to give: the page then
stops running slices until an event arrives, and an idle world costs no
processor time. A process that busy-waits in Lisp (`%wait-until`, as threads
and channels do today) keeps the machine from being idle; the environment is
built so that none does while it waits for the person.

### 3. Output records are Lisp objects with identities

A record is an instance of a class in `QUOKKA-UI`, with a number, its
**identity**, unique in the world and never reused, which the renderer uses
to find the node it made for it. The kinds the renderer knows are few and
grow by phase:

| Kind | Draws | Phase |
| --- | --- | --- |
| `:viewer` | a viewer's area, holding its contents | 4 |
| `:block` | children stacked downwards; text inside keeps its line breaks | 4 |
| `:text` | a run of text in one style | 4 |
| `:editor` | text the person edits, in place | 4 |
| `:drawing` | shapes in a box of their own, as SVG (`:width`, `:height`, `:scale`) | 4 (step I) |
| `:group` | shapes under a transformation `(:transform (a b c d e f))` | 4 (step I) |
| `:shape` | one shape: `:type` `:rectangle`, `:ellipse`, `:line`, `:polyline`, `:polygon` or `:text`, its geometry, `:ink`, `:filled`, `:line-thickness` | 4 (step I) |
| `:table`, `:row`, `:cell` | `formatting-table` | 5 |
| `:presentation` | a record that shows an object, clickable | 5 |

A message speaks of nodes as `(kind id properties child...)`, such as
`(:text 41 (:style (:fixed) :ink :dim) "CL-USER> ")`. A kind the renderer
does not know is drawn as its children, so an older page shows a newer
world's output plainly instead of failing.

Styles and inks are Lisp objects and names, never CSS (design, *Styles, not
CSS*): a text style is CLIM's family, face and size (`(:fixed :roman
:normal)`), an ink is a theme's name for a colour (`:accent`, `:dim`,
`:error`), and the renderer maps each to a class whose colours the theme
sets, light and dark.

### 4. Two ways a viewer's records change

A **recorded** viewer keeps what was written to it: the listener's transcript
grows at the end as forms run, and an earlier entry changes only when
something says so (a result struck through). Writing appends records and
queues the messages that append them; nothing is drawn twice. This is CLIM's
output recording, and it is what makes a long transcript cheap.

A **redisplayed** viewer draws itself again from its model, through the
generic function `display-viewer`, whenever the command loop says so (design,
*When a viewer redraws*). The new records are compared with the previous
drawing, child by child: a record of the same kind in the same place, or with
the same key when the program gave one, takes over the old record's identity,
and only what differs is sent. This is the reconciliation React made familiar
in 2013 for its "virtual DOM", done here over output records, which is what
`docs/design.md` asks for when it says redisplay compares records and not DOM
nodes. A record that keeps its identity and its place keeps its node, so an
editor in it keeps its cursor; one matched by key in another place is moved
by taking it away and adding it again, so its node is made anew (a `:move`
message could keep it, and is left until a viewer needs it).

Phase 4's listener is recorded; the first redisplayed viewer is a test, and
phase 5's viewers are the first real ones. Both kinds go through the same
messages.

### 5. The browser edits text; the world keeps it

An `:editor` is drawn as a text area the browser edits itself, so typing,
selection, input methods, spelling and accessibility are the browser's, as
the design wants (*Graphics where needed*). Every change is sent to the world
as an event, `(:edit :target 17 :text "(+ 1 2" :start 6 :end 6)`, and the
world keeps the text in the record, so the world always knows what is on the
screen. The record and the browser then agree, and redisplay sends nothing
for the change the browser already made.

Keys the world wants are announced in advance. A page cannot ask the world
whether a key is wanted and wait for the answer before the browser acts on
it, so each editor carries a **key table**, the keys bound to commands for it
(`"Enter"`, `"Meta+Enter"`), and the renderer stops the browser's own action
for exactly those and sends them as events: `(:key :target 17 :key "Enter"
:code "Enter" :modifiers (:meta))`. Every other key belongs to the browser.
This keeps the rule of the design's *Viewers, desktops and layout*, keys are
bindings to commands and the bindings are Lisp, without putting the browser's
text editing in Lisp. A Zmacs-style buffer in Lisp, every keystroke a message
both ways, was the alternative; it would make structural editing easy and
input methods and accessibility our problem. Phase 6's editor can revisit
this behind the same `:editor` kind.

Phase 4 draws an editor as a `<textarea>` that grows with its text, because
its caret and selection are reliable in every engine; `contenteditable`,
needed for coloured code, is a renderer change later.

### Known limits (review of #138–#143)

- A `:set :text` the world sends in answer to a key (a newline inserted
  where Enter was pressed) overwrites anything typed in the editor after
  the key, if the world is slow to answer; the page and the world then
  disagree until the next edit. A fix wants edits numbered, so the world
  can tell which text its answer was computed from.
- While a form reads through the nested loop, `%listener-evaluating` stays
  bound, so an error in another viewer's command would be taken for the
  form's. Nothing reaches this with one viewer.
- A nested read inside a nested read does not restore the outer one's
  state on returning normally; every way out of phase 4's debugger unwinds
  past both.
- An event's text is read with the standard readtable, so a qualified
  symbol in it (`cl::foo`) is interned. The page is the world's own host,
  so this is not a way in; a remote host would want a reader that refuses
  qualified symbols.
- A form after `#` in the page's address is run on load, as the dev
  console did; a link can run code in a fresh world, which cannot read
  files and keeps nothing.

### 6. The command loop is the main process; input is read by nested loops

The environment's top level is a **command loop** running in the main
process: wait for an event, give it to the viewer it is for, run what it asks,
send what changed, and wait again. Evaluating a form in the listener happens
in that loop, inside the command that handles Enter. A form that reads, such
as `(read-line)` or the debugger's prompt after an error, reads from the
listener's stream, and that stream's reader runs a **nested** command loop
until the person enters a line: editing, and other viewers, go on working
while the form waits. This is how Genera's listener and CLIM's input editor
read, and how a modal dialog in a desktop toolkit runs its own event loop.

We considered the listener evaluating in a process of its own, Genera's
arrangement for some activities, with the command loop handing it forms over
a channel. Two processes would have to share the listener's records, and a
process waiting on a channel today busy-waits, which keeps the machine from
ever being idle (decision 2). One process, with nested loops, has neither
problem. A program that wants to run beside the listener still starts a
process of its own (`docs/processes.md`), and redisplays from it.

A form that never ends is stopped from the page: its Stop button, or
Cmd-period, makes the host abandon the computation (`vm::recover`) and start
the command loop again, which redraws from the records, so nothing on the
screen is lost; the program is sent the event `(:interrupted)`. A program
that fails three times running before it once waits for the person is not
started again (review of #137). *Provisional:* an interrupt that enters the
debugger in the running form, through the interrupt frame
`docs/processes.md` designs, is better and is not built.

Errors find their way as follows (#141). The listener runs each form under a
handler that calls the debugger when it is on, and the environment turns it
on, since the listener is a console that can answer it: the debugger's
banner, restarts and backtrace are written into the form's entry, and its
`0]` prompt reads from an editor there, through the nested loop. The
command loop's own handler catches what a viewer's methods signal outside a
form, writes the message into the focused viewer and returns to the loop;
it leaves alone an error inside a form the listener runs
(`%listener-evaluating`), which is the listener's.

### 7. Every record remembers the code that drew it

Phase 5's right-click menu offers *Show the code that drew this* on any
output (design, *Point at anything*), and phase 4 must keep what it needs.
When a record is made, `%drawing-function` asks the machine for the
innermost frame whose function is not part of the output machinery, one
frame at a time through the primitive `%frame-callee`, which walks the
frame chain and allocates nothing; it looks at most 64 frames out, and the
record keeps that function. A function counts as machinery when its name is
in `COMMON-LISP`, `KEYWORD`, or one of Quokka's system packages (`QUOKKA`,
`QUOKKA-GRAY`, `QUOKKA-UI`, `QUOKKA-MOP`, `QUOKKA-DEBUG`, `QUOKKA-THREAD`);
a method counts by its generic function's name, and a local or anonymous
function never counts, since the function around it is the definition to
show. So the prompt's record names the environment's own `listener-prompt`
(`QUOKKA-ENV`), and a line `dump-db` printed names `dump-db`. One
consequence, noted in review: a program's own method on a Gray stream
generic function, such as `stream-write-char`, is named in `QUOKKA-GRAY` and
counts as machinery, so the record names the function that wrote to that
stream; we think that is the right answer.

### 8. Layout is part of the protocol

The message `(:layout tree)` says where each viewer is: a tree of splits
with a direction and weighted children, as decided for the window manager
(design, *Viewers, desktops and layout*), with viewers at the leaves. The
renderer lays the tree out with CSS flexible boxes and decides nothing about
it. Phase 4 sends a tree of one viewer, the listener; phase 6's window
manager sends the rest, without the renderer changing.

### 9. Streams are Gray streams

A viewer's stream must be a stream to `format`, `print` and `read-line`, and
a Lisp class to the toolkit. Common Lisp's standard has no way to define a
stream class; the Gray streams proposal (David Gray, 1989), which every
current implementation provides (SBCL's `SB-GRAY`), does, with classes such as
`fundamental-character-output-stream` and generic functions such as
`stream-write-char` and `stream-read-char`. Quokka's library streams are
structures with fixed kinds; they gain a third kind, any instance of
`fundamental-stream`, dispatched to the generic functions, while string and
console streams keep their fast paths. The names are SBCL's, in
`QUOKKA-GRAY`, so a differential file defining a stream class runs on both.

### 10. Weak references wait

`docs/design.md` planned weak pointers and weak-key tables for phase 4, "with
the output records that first need them". Planning found nothing in phase 4
that needs them: records are reached from their viewers, and redrawing after
a definition changes (*When a viewer redraws*) walks the visible viewers'
records rather than an index from functions to records. They are built when
something does need them, the first likely candidate being phase 5's table
from objects to the presentations that show them.

### 11. Three packages

`QUOKKA-UI` is the platform, the package the tutorials use (`(:use
:common-lisp :quokka-ui)`): records, styles, viewers, redisplay, the command
loop, later drawing, presentations and commands. `QUOKKA-ENV` is the
environment built on it: the listener now, the inspector and browser later;
mock-up 03 finds `listener-prompt` there. `QUOKKA-GRAY` holds the Gray
streams. All three are Quokka's extensions, in packages of their own (rule 6).

### 12. Testing in three places

The world's half is tested from Rust with a scripted host, which plays events
into a world and collects the messages it sends (`crates/quokka-wasm`'s
`Quokka`, the same object the page drives). The messages are compared as
text, the snapshots the design's testing table asks for. The renderer is
split into a **model**, which applies messages to a tree of plain objects and
can print it as text, and a **view**, which keeps the DOM in step with the
model. The model runs in Node against a real world (`scripts/smoke-wasm.mjs`),
which is the design's text-dump renderer; the view runs in headless Chrome
(`scripts/smoke-browser.sh`), which is the gate's test.

### 13. The page is the environment

`www/index.html` becomes the environment: it loads the world, starts the
command loop, and runs slices while the world is busy. The dev console moves
to `www/console.html`, unchanged, because a page that evaluates a line with
no renderer is still the quickest check that the browser build works.

## Message reference

Messages go from the world to the page; each `%display` carries a list of
them, applied in order.

| Message | Meaning |
| --- | --- |
| `(:reset layout)` | forget everything and show LAYOUT, whose viewers carry their contents |
| `(:layout layout)` | arrange the viewers again; their contents stay |
| `(:append parent node)` | add NODE at the end of PARENT's children |
| `(:insert parent before node)` | add NODE in front of PARENT's child BEFORE |
| `(:replace id node)` | put NODE where record ID is |
| `(:remove id)` | take record ID away |
| `(:set id property value...)` | change properties; nil removes one |
| `(:focus id)` | put the caret in editor ID |

Where a new record goes is said by the sibling it goes in front of, worked
out when the world flushes its queue of changes, from the tree as it is
then: a place given as a number would be thrown out by a record added and
taken away again before the flush.

The properties are names, never CSS: `:style` (a text style), `:ink`, `:text`
(of a text or an editor), `:start` and `:end` (an editor's selection),
`:keys` (an editor's key table), `:title` (a viewer's), `:readonly` (an
editor that keeps what was typed), `:layout :row` (a block whose children
stand side by side), and three the listener uses, `:strike` (struck
through), `:note` (a small note) and `:bar` (output set apart by a bar).

A layout is `(:split direction (weight layout)...)`, with direction
`:horizontal` (side by side) or `:vertical` (one above the other), or a
viewer node `(:viewer id (:title "Listener") child...)`.

Events go from the page to the world, one per `%await-event`.

| Event | Meaning |
| --- | --- |
| `(:edit :target id :text string :start n :end n)` | editor ID now holds STRING, selected from START to END |
| `(:key :target id :key name :code code :modifiers (...) :start n :end n)` | a key in ID's key table was pressed, with the caret where it was |
| `(:interrupted)` | the page abandoned a computation and restarted the loop |

`:key` is the character or name the key produces (the browser's `key`), and
`:code` the key's place on the keyboard (the browser's `code`), which the
design's games use (*Keys*); modifiers are `:shift`, `:control`, `:alt` and
`:meta`.

## The steps

The tracking issue's Order table is the authoritative list; each step has an
issue.

| | Step | Needs | Issue |
| --- | --- | --- | --- |
| A | this design | — | #135 |
| B | Gray streams | — | #136 |
| C | the host requests, waiting requests answered by place, idle | — | #137 |
| D | output records, styles, the recording stream, messages | B, C | #138 |
| E | viewers, redisplay by reconciliation, the command loop, nested reading | D | #139 |
| F | the renderer: model and view, layout, editors, key tables | C | #140 |
| G | the listener: entries, prompt, values, re-running an edited form, the debugger at the listener | E, F | #141 |
| H | the gate: edit and evaluate a form in the browser, tested headless | G | #142 |
| I | drawing records, `with-drawing`, SVG | E, F | #143 |
