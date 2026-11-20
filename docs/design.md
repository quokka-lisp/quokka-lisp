# Quokka — Design Sketch

Draft v0.1 · November 2026 · Carsten

> This is the repository copy of the design. The live version, with all diagrams, is at
> https://claude.ai/artifact/SbwCzDv2s8uG2xW18Afuv2. Diagrams are noted below as *[Diagram: …]*.
> When a design decision changes, update both.

## Vision and principles

Quokka is one live Lisp world — VM, language, editor, UI and tools in a single image — small enough for one person to understand completely. It is browser-first via WASM, runs natively inside a desktop app shell, and never on bare metal. It takes its soul from the Lisp machines, its malleability from Smalltalk and its discipline of smallness from Oberon, with a modern look.

**Principles**

1. **One world.** Editor, debugger, UI and your code are objects in the same image. No separate editor process talking over a socket.
2. **Comprehensible.** The whole system should be readable by one person. We set a size budget early and track it (proposal: the Rust core stays under 15k lines).
3. **Text is executable.** Any S-expression on screen can be evaluated where it stands. There is no separate shell.
4. **Output remembers its object.** Everything printed stays linked to the live object it represents, so you can point at it, inspect it and act on it.
5. **Errors are conversations.** An error stops and asks; it never throws away state. You fix the code and resume.
6. **Modern look, old soul.** Proportional type, HiDPI, light and dark themes, tiled viewers. No 1985 bitmap aesthetic.

**Non-goals:** running Genera or Medley software, bare-metal drivers, ANSI Common Lisp compliance, multi-user security.

## Inheritance

Each tradition is strongest where the others are weak, so we take three ideas from each and leave behind its main burden.

*[Diagram: inheritance — three traditions, one synthesis]*

| Tradition | What we take | What we leave behind |
| --- | --- | --- |
| Lisp machines (Genera, Interlisp-D) | Tagged objects in one image; presentations (live output); conditions and restarts | Sheer size |
| Smalltalk (Squeak, Pharo, Glamorous Toolkit) | Direct manipulation; browsers over live code; moldable inspectors | Image-only code |
| Oberon (Project Oberon, Acme) | Smallness as a goal; text as executable commands; tiled viewers, no overlaps | Static modules |

The combination that none of the three had on its own: Oberon's executable text becomes Lisp S-expressions, and their results come back as live presentations inside the same text.

## Architecture overview

The system has five layers and one hard boundary: Rust below it, Lisp above it.

*[Diagram: architecture — five layers, one boundary]*

| Layer | Language | Contents |
| --- | --- | --- |
| Environment | Lisp (in the image) | Tiled viewers, live text, listener, inspector, browser, debugger |
| Presentation and UI toolkit | Lisp (in the image) | Presentation types, commands, layout, incremental redisplay, input |
| Language and runtime library | Lisp (in the image) | Compiler, conditions and restarts, generic functions, streams, packages |
| *Primitive table* | — | *The only door between Lisp and Rust* |
| VM core | Rust (host program) | Bytecode interpreter, tagged heap, garbage collector, image save and load, processes |
| Host layer | Rust (host program) | Display protocol renderer (DOM and canvas), browser storage, network, clock; Tauri shell for native |

Rust supplies only what a Lisp machine's hardware and microcode once did. Everything a user sees or changes — including the compiler and the window system — lives in the image and can be edited while it runs.

## Substrate: the Rust VM

The substrate is a small Rust program that plays the role of Lisp machine hardware: tagged words, a garbage-collected heap, a bytecode interpreter and an image file. Everything above it is Lisp.

**Tagged words.** Every value is one 64-bit word, tagged in its low bits like Ivory's tagged memory. Heap objects are 8-byte aligned, so the low 3 bits are free for the tag. Proposed scheme:

| Low bits | Meaning | Notes |
| --- | --- | --- |
| `xx0` | Fixnum | 63-bit integer, arithmetic without untagging |
| `001` | Cons pointer | The most common heap object gets its own tag |
| `011` | Heap object pointer | Header word gives the type: symbol, vector, string, closure, code, instance, hash table |
| `101` | Immediate | Characters, `nil`, `t`, unbound marker |
| `111` | Reserved | Forwarding pointers during GC |

**Object headers.** A heap object (tag `011`) starts with a header word giving its type and its length in slots: `[ length : 48 ][ type : 8 ][ subtag : 5 ][ 101 ]`. Cons cells have no header — the tag is the type — which is why they get a tag of their own.

That creates a problem the encoding has to solve. The collector scans to-space linearly, so at every position it must know how big the object there is, and a header and a cons cell's car sit in the same place. Giving conses a header too would cost a third more memory on the commonest object in a Lisp. Instead the header is encoded in the immediate space under a subtag that is never handed out, so no value can be mistaken for one: the scan is a single test, and conses stay two words.

The type says whether the object's slots are values. Strings, code and bignums hold raw words, and tracing those would follow numbers as if they were references, so this distinction is load-bearing rather than descriptive.

**Garbage collection.** Start with a simple copying (Cheney) collector: easy to get right and to read. Move to a generational collector once the environment allocates heavily (presentations and redisplay will).

Cheney needs no auxiliary stack: `scan` trails `free` through the objects already copied in to-space, so to-space is its own work queue. A loop, not a recursion, so an arbitrarily deep or tangled object graph costs nothing — which matters more than it would elsewhere, since the wasm32 stack is about a megabyte and fixed at link time. Forwarding pointers (tag `111`) go in an object's first word in from-space, which is safe to overwrite because it is about to be abandoned; they are what keeps shared structure shared and stops cycles looping.

*Torture mode.* The classical version collects on every allocation. That is impossible here and need not be: collection cannot happen inside a mutation and allocation only happens inside one, so the bug it hunts — a value lost across an allocation — is unrepresentable (see Rooting). What remains to hunt is the collector's own mistakes: a mis-sized header, an untraced slot, a root nobody forwarded. So the mode collects at every mutation boundary, pads to-space by a varying amount so nothing lands where it was, and then verifies — walking the heap to record object boundaries and checking every reference points at one.

**Rooting.** A copying collector decides what is alive by tracing from its roots — locations outside the heap holding values — and it cannot see Rust local variables. A value held in a Rust local across an allocation is therefore not stale but dead: the collector concluded nobody wanted it and reclaimed it. Because our references are offsets rather than pointers, the result is a wrong answer rather than a crash, which is harder to notice, not easier.

*How it is prevented:* work on the heap happens inside `Heap::mutate`, and collection cannot happen during a mutation. `mutate` takes `F: for<'h> FnOnce(&mut Mutation<'h>) -> R`; the higher-ranked brand means the closure must work for every `'h`, and `R` is fixed outside it, so no value can be carried out. `collect` takes `&mut self`, which a live mutation borrows, so collecting mid-mutation does not compile. Holding a value across a collection is a compile error, not a convention. Thirteen lines of types, no `unsafe`, no runtime check.

The brand must be *invariant* — `PhantomData<fn(&'h ()) -> &'h ()>`, not `PhantomData<&'h ()>`, which is covariant and would silently permit the escape. Heap-reference constructors are crate-private, since the brand stops a value escaping a mutation but not being invented outside one.

*What it costs:* collection can only happen between mutations, so the heap must absorb whatever one allocates. Cheap here, because rule 4 already forbids long mutations — one evaluator step per mutation gives fine-grained collection for free. And it protects callers from the collector, not the collector from itself; tracing bugs still need the torture mode.

**Image.** The whole heap can be saved to a file and loaded back ("save world"). Pointers are stored relative to the heap base, so an image loads by relocation or straight `mmap`. Booting means loading an image, not running a build.

**Bytecode, not native code.** A compact stack-based bytecode keeps the VM small and portable, including to WASM. Speed comes later from threaded dispatch and inline caches, not a JIT.

**Reified frames.** Stack frames are inspectable and resumable objects. The debugger, restarts and fix-and-retry all depend on this, so it is a VM design constraint from day one.

**Slices.** The VM never runs to completion on its own: it executes a bounded number of steps and returns to its caller, so the browser's event loop keeps turning. This is the same constraint as reified frames arrived at from the other side — an evaluator that can stop and resume cannot keep its state on the Rust call stack, so it is an explicit state machine (SICP §5.4's explicit-control evaluator) whose frames are objects in our heap. Saving an image mid-computation, green threads and an interruptible REPL all fall out of this.

*Who ends a slice:* the caller passes a step budget — `run_slice(budget)` — rather than the VM polling the host or watching a wall clock. Timing stays outside the VM, in the host, where it cannot affect what is computed. Slicing is therefore semantically invisible: the same program takes the same path however it is cut up, which makes runs replayable and lets the phase 3 scheduler be seeded rather than raced. The browser recovers a deadline's responsiveness by adjusting the next budget from how long the last slice took. The native host passes `u64::MAX` and ignores the whole question.

*What it costs:* a budget only bounds time if one step is short, so every primitive must be fast or itself chunked. Two invariants keep slicing invisible: collect on allocation rather than on slice boundaries, and never let Lisp observe the remaining budget.

**Processes.** Lightweight green threads scheduled by the VM, like Genera's processes, on one OS thread to start with.

**Primitives.** A single table of Rust primitives (arithmetic, allocation, I/O, graphics calls) is the only door from Lisp into the host. Its size is part of the comprehensibility budget.

## Language layer

Decided: a subset of Common Lisp — a Lisp-2 with CL's names and semantics, growing toward ANSI where it pays — rather than a new dialect. Code written for it should also run in SBCL, which gives us a reference implementation to test against.

**Core features**

- **Conditions and restarts** in the CL style. Signalling does not unwind; handlers and the debugger choose a restart. This is what makes "errors are conversations" work.
- **Dynamic variables** for context (current viewer, current output stream, current package).
- **Generic functions with multiple dispatch**, a small CLOS-like object system. Presentations, commands and inspector views all dispatch on object type, so this is load-bearing, not optional.
- **Packages** for namespaces, kept simple.
- **`defmacro`-style macros**, unhygienic with gensyms. Easier to implement and to read than syntax-case.
- **First-class code objects** that know their source, their definition location and their callers (Masterscope-style cross-reference).

**Bootstrap path**

1. **Interpreter (phase 0):** a small tree-walking interpreter in Rust, grown just far enough to run the compiler's source: macros, closures, special variables, a minimal condition system.
2. **First build:** the interpreter runs the CL compiler, which compiles itself and the core library to bytecode and saves the first image. Slow, but it happens once.
3. **Fixpoint:** the compiled compiler, now running in the image, rebuilds itself. When its output matches the first build byte for byte, the system is self-hosting.
4. **Afterwards:** the image rebuilds itself, in the browser too. The interpreter stays as a debugging fallback and a way to rebuild from source with nothing but `cargo`.

The compiler is written once, in CL. The build needs only `cargo`, and SBCL is a test oracle rather than a build dependency: code that runs in both must give the same results.

**Source of truth.** Smalltalk's weak spot is that code lives only in the image, which fights version control. Every definition has a canonical text form, saved per module to files on a file host (see Files and storage). On native builds these are plain files that Git can diff and merge. The image is a fast, live cache of those files, and editing a definition in the image writes the file back.

### The CL subset boundary

The subset grows in three tiers, each the minimum needed to unlock a phase. ANSI conformance is measured as we go rather than promised up front.

*[Diagram: CL subset — three tiers and what is deferred]*

| Tier | Unlocks | Contents |
| --- | --- | --- |
| Tier 0 | Runs the compiler · phase 0 | 22 of 25 special operators; `defmacro` with full lambda lists; fixnums, conses, symbols; strings, vectors, hash tables; `defstruct`, `error`, `handler-case` |
| Tier 1 | Self-hosting core · phase 2 | All 25 special operators; `setf` expanders, packages; reader, printer, `eval`, `load`; sequences, basic `format`; double floats, bignums |
| Tier 2 | Runs the environment · phases 3–5 | CLOS generic functions; live class redefinition; conditions, restarts, debugger; `loop`, full `format`, streams; pathnames over file hosts |
| Deferred | Until someone needs it | Ratios, complex numbers, the full MOP, pretty-print dispatch, `*print-circle*`, `compile-file` and fasls |

Tier 0 is the first target: it is everything the phase 0 interpreter must support, which is why the compiler's own source is written to stay inside it.

- **The tier 0 rule.** The compiler and core library use only tier 0: no `loop`, no CLOS, only basic `format`. CI also runs the compiler under SBCL as the oracle.
- **Left out of tier 0:** `progv`, `load-time-value` and `symbol-macrolet`. The interpreter can add them later at little cost.
- **Live redefinition is not optional.** Updating existing instances when a class changes (CL's `update-instance-for-redefined-class`) is in tier 2, because a live environment depends on it.
- **Floats by phase 4.** Graphics needs double floats, so they arrive in tier 1 even though the compiler barely uses them.
- **Measuring conformance.** The ANSI test suite (ansi-test) runs per tier; each tier is defined by which test groups must pass.
- **Quokka extensions** (processes, presentations, file hosts, weak references) live in their own packages, never in `COMMON-LISP`.

## Environment and UI

The screen is a set of tiled viewers holding live text: any form can be run where it stands, and every result stays a live object you can point at and act on.

*[Diagram: screen mock-up — listener, editor and inspector in tiled viewers]*

One interaction loop runs through everything: evaluate a form, get presentations back, pick a command that fits the object's type, and land in another viewer with the object itself.

- **Tiled viewers.** Oberon-style tracks: a wide main track and a narrow tool track. Viewers split and resize but never overlap. Rooms-style workspaces come later, as named sets of viewers per task.
- **Live text.** Every viewer is a rich-text document mixing prose, code and embedded results. Evaluating a form inserts its result below it as a presentation, so notes, scratch code and the command line are the same thing.
- **Presentations.** Each printed object keeps a reference to the object and its presentation type. When a command needs an argument, clicking any matching presentation on screen supplies it, as Genera's `accept` did.
- **Moldable inspectors.** Objects add views by defining methods on one generic function. A heap object might show its tag and raw words, a file its text, a function its callers.
- **Debugger as a viewer.** An unhandled condition opens a viewer with frames as presentations and restarts as commands. You edit the failing function in place, then retry the frame.
- **Editor.** Text-based with structural (paredit-style) commands. Text keeps the system approachable and Git-friendly.

## Display: hybrid renderer

Lisp code describes what to show in Lisp terms, and a small renderer in the page decides how: the DOM for text, canvas or SVG for drawing. HTML never appears in Lisp code.

*[Diagram: display pipeline — Lisp UI code → output records → display protocol → renderer (DOM | canvas/SVG)]*

The display protocol is the boundary between the VM and the screen, so the VM can run as WASM, natively in a Tauri shell, or later on a server, without the Lisp side changing.

**A Lisp-shaped API.** The UI vocabulary follows CLIM: presentation types, commands, formatting macros and drawing functions. A sketch of how it should read (proposed names):

```lisp
(define-presentation-type file-object ())

(define-command (open-in-editor :applies-to file-object)
    ((file 'file-object))
  (edit-file file))

(defun show-directory (dir)
  (formatting-table ()
    (dolist (file (directory dir))
      (formatting-row ()
        (formatting-cell () (present file 'file-object))
        (formatting-cell () (format t "~D bytes" (file-size file)))))))

(defun show-sketch ()
  (with-drawing (:width 240 :height 120)
    (draw-circle* 60 60 40 :ink +accent+)
    (draw-text* "heap" 120 64 :text-style '(:sans :bold :large))))
```

- **Output records.** Every piece of output becomes a record in a tree that Lisp owns. Redisplay compares records, not DOM nodes, as CLIM's incremental redisplay does.
- **Styles, not CSS.** Text styles (family, face, size) and named inks are Lisp objects. A theme maps them to light and dark colours; the renderer turns them into CSS internally.
- **Layout in Lisp terms.** Tracks, viewers, tables and indentation. No box model, margins or tags are exposed to Lisp.
- **Graphics where needed.** A drawing record renders to canvas or SVG; everything else renders to the DOM, so the browser handles text, selection, input methods and accessibility.

## Files and storage

All file access goes through CL pathnames, whose host component selects a file host behind one protocol. Genera and Interlisp worked the same way for local and network files.

*[Diagram: file hosts — one protocol, four backends]*

Code never knows which backend it is talking to, so the same environment works natively, in the browser and against a remote service.

- **One protocol.** A file host implements a small set of generic functions: open a stream, list a directory, probe, rename, delete, write date. A new backend is just a new class.
- **Image host** (`image:/…`). Files stored as objects in the heap, like Interlisp's `{CORE}` device. Always available, and they travel with the image.
- **Browser host** (`browser:/…`). The browser's Origin Private File System (OPFS). Files persist across page reloads without saving the image.
- **Local host** (`local:/…`). The real filesystem, on native builds only. This is where Git working trees live.
- **Remote hosts (later)** (`cloud:/…`). Files on a server over HTTPS. A documented protocol lets people self-host; a subscription file service can be offered on top. Offline edits need a sync and conflict model, and the Krill Notes Swarm protocol work may carry over.
- **Versions.** CL pathnames have a version component. Hosts that can keep versions (image, remote) expose file history, as Genera's file system did.
- **Image backup.** In the browser, the whole image can be downloaded as a file and uploaded again to restore or move it. Periodic snapshots go to browser storage automatically.
- **Credentials stay out of the image.** Tokens for remote hosts live in the browser's or OS's credential store, so a downloaded or shared image never carries them. Opening someone else's image means running their code.

## Roadmap

The work runs in seven phases, each ending at a gate we can demonstrate, so the system is usable (in a terminal) from phase 0 and graphical from phase 4.

*[Diagram: roadmap — seven phases, each with a demonstrable gate]*

| Phase | Scope | Gate to pass before moving on |
| --- | --- | --- |
| 0 · Spike | Tagged heap, reader and interpreter in Rust; native and WASM builds | `(fact 20)` runs; the GC survives an allocation stress test |
| 1 · Bytecode and image | Bytecode VM; the interpreter runs the CL compiler to build the image | An image saves, reloads and resumes where it left off |
| 2 · Self-hosting | The compiled compiler rebuilds itself and the core library | Rebuilt compiler matches the first build byte for byte |
| 3 · Conditions and processes | Restarts, reified frames, green threads, a terminal debugger | Fix a function from inside an error and retry the frame |
| 4 · Renderer and live text | Display protocol, browser renderer, one live-text viewer | Edit and evaluate a form in the browser |
| 5 · Presentations and inspector | Presentation types, commands, a moldable inspector | Click a result to inspect it or pass it to a command |
| 6 · The environment | Tiled tracks, browser, debugger viewer, file sync | Daily work on the system happens inside the system |

No dates yet: this is a hobby-pace project. Phases 0 to 3 are mostly Rust and compiler work; phases 4 to 6 are mostly Lisp.

## Testing before there is a UI

Every phase is testable from the terminal and in CI long before Quokka has a renderer or file hosts, because the test harness lives in the Rust host, which has the operating system's files and console.

*[Diagram: differential testing — one test file runs on Quokka native, Quokka in WASM and SBCL; results are compared]*

The backbone is differential testing: the same Lisp test file runs on Quokka natively, on Quokka in WASM and on SBCL, and all three must print the same result.

- **No file system? The host has one.** Before file hosts exist, one host primitive loads source files: `std::fs` natively, WASI in WASM tests, files bundled into the page in the browser. File hosts arrive later on top of it.
- **A terminal REPL from phase 0.** stdin and stdout are enough to drive everything up to phase 3.
- **A browser dev console from phase 0.** A plain page with a text area wired to the WASM VM, long before the real renderer. It proves the browser build works every day.
- **WASM in CI from day one.** The same tests run under wasmtime and in a headless browser, so the browser target never drifts.

| Phase | How it is tested |
| --- | --- |
| 0 · Spike | Rust unit and property tests; GC torture mode (collect on every allocation); differential tests against SBCL |
| 1 · Bytecode and image | Image round-trips (save, reload, compare heaps); interpreted and compiled results must match |
| 2 · Self-hosting | The byte-for-byte fixpoint; ANSI test suite groups for tier 1 |
| 3 · Conditions and processes | Scripted handlers invoke restarts programmatically; seeded, deterministic scheduler tests |
| 4 · Renderer and live text | Snapshot tests of the display protocol stream; a text-dump renderer for the terminal; browser tests for the renderer only |
| 5 · Presentations and inspector | Tests drive commands through the Lisp API and assert on output records, not pixels |

## Decisions and open questions

**Decided**

- [x] **Dialect:** a Common Lisp subset, tested against SBCL as the reference.
- [x] **Bootstrap host:** bootstrap by interpretation — the Rust interpreter runs the CL compiler to build the first image.
- [x] **Subset boundary:** three tiers matched to phases (see The CL subset boundary).
- [x] **Source of truth:** files on any file host are the truth; the image is a live cache of them.
- [x] **Rooting:** lifetime branding, not a shadow stack. Holding a value across a collection is a compile error. Chosen over an explicit root stack because the Rust surface is small and permanently bounded by rule 1, so the cost of the stricter scheme is bounded too. See Substrate: the Rust VM.
- [x] **Yielding:** the caller passes a step budget (`run_slice(budget)`); no clock and no yield callback in the VM. Chosen over a host-set wall-clock deadline for determinism and replayability. See Substrate: the Rust VM.
- [x] **Browser target:** WASM is first-class from phase 0. The VM runs client-side in the browser, yields to the browser's event loop, and keeps images in browser storage.
- [x] **Rendering stack:** browser-first hybrid renderer (DOM for text, canvas or SVG for drawing) behind a display protocol; native builds in a Tauri shell.
- [x] **Licence:** MIT for everything, including the remote file service. Borrowed components must have compatible licences.
- [x] **Name:** Quokka (quokka-lisp.org). The mascot's tail is a quote mark: `'quokka`.

**Still open** (none blocks phase 0)

- [ ] **Is `nil` an immediate or a symbol?** The tagged-word table above makes `nil` an immediate, which gives the cheapest possible `null` test and a compile-time constant for the Rust code. But Common Lisp says `nil` *is* a symbol: `(symbolp nil)` is true, it has a name, a home package, a value cell holding itself, and a settable property list. An immediate has nowhere to keep any of that, so every symbol operation needs a special case, or `nil` needs a shadow heap object anyway. The alternative is SBCL's: `nil` is a real symbol at a fixed, known heap position, which keeps the `null` test to a single word comparison and makes the special cases disappear. Phase 0 follows the table as written; every test for `nil` goes through `Value::is_nil` and every mention through `Value::NIL`, so switching later touches two functions plus the bootstrap. Decide when symbols are built.

- [ ] **Concurrency:** green threads on one OS thread for good, or plan for real parallelism later?
- [ ] **Remote protocol:** WebDAV, an S3-compatible API, or our own protocol over HTTPS?
- [ ] **Git in the browser:** through a Git hosting API, or a Git implementation compiled to WASM?

## To-dos

- [x] Register the quokka-lisp.org domain.
- [x] Create the repository: [github.com/quokka-lisp/quokka-lisp](https://github.com/quokka-lisp/quokka-lisp) (private for now).
- [x] Claim the quokka-lisp GitHub organisation and transfer the repository there.
- [ ] Build the quokka-lisp.org website.
- [ ] Reserve the crates.io names (e.g. `quokka-lisp`, `quokka-lisp-vm`) — publish a small real first version, not an empty placeholder.
- [ ] Reserve the npm package name for the renderer (e.g. `@quokka-lisp/renderer`).
