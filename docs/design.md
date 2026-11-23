# Quokka — Design Sketch

Draft v0.1 · November 2026 · Carsten

> This is the repository copy of the design. The live version, with all diagrams, is at
> https://claude.ai/artifact/SbwCzDv2s8uG2xW18Afuv2. Diagrams are noted below as *[Diagram: …]*.
> When a design decision changes, update both.
>
> How to build, run, rebuild and test Quokka is in [the developer manual](manual.md).

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
| Oberon ([Project Oberon](https://people.inf.ethz.ch/wirth/ProjectOberon/index.html), Acme) | Smallness as a goal; text as executable commands; tiled viewers, no overlaps | Static modules |

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

*What it costs:* collection can only happen between mutations, so the heap must absorb whatever one allocates. Cheap **only if mutations stay short** — one evaluator step per mutation gives fine-grained collection for free, and a whole evaluation in one gives none at all.

That sentence was in this document from the start and was implemented nowhere, which phase 1 found three times over: a compiler that looked like it hung, a source file that would not load, a REPL line that died — all one mutation filling the heap with its own garbage. Two things now make it true. `Heap::mutate` collects at the boundary when the heap asks, so no caller has to remember; and `eval::eval_root` drives the machine a slice at a time, each slice its own mutation, passing the form and the answer through the root because a value cannot cross a boundary. `eval_now` keeps the single-mutation shape for prelude forms and anything small. And it protects callers from the collector, not the collector from itself; tracing bugs still need the torture mode.

**Image.** The whole heap can be saved to a file and loaded back ("save world"). Pointers are stored relative to the heap base, so an image loads by relocation or straight `mmap`. Booting means loading an image, not running a build.

**Bytecode, not native code.** A compact stack-based bytecode keeps the VM small and portable, including to WASM. Speed comes later from threaded dispatch and inline caches, not a JIT.

*The shape of it*, decided in #25 before any of it was written:

- **Packed bytes, with each opcode's operand widths fixed.** Not one 64-bit word per instruction, which would be clearer to read and about three times larger (`docs/bytecode.md` measures 2.6 bytes an instruction on real code, correcting an earlier guess of two) — wasm32 is a 32-bit address space, a copying collector doubles every figure, and the image is downloaded in the browser. Fixing operand widths *per opcode* is what keeps this cheap: an instruction's size is then a function of its opcode alone, so jump distances are known in one pass and the branch-relaxation fixpoint that variable-width operands force never arises. Ceilings are compiler errors rather than wraparound — a 16-bit jump offset means a function over 64 KB of code is reported, as the JVM reports the same limit per method.
- **The instruction set is written out** in [`bytecode.md`](bytecode.md): forty opcodes, the frame layout, what each special operator compiles to, and every ceiling that is a compiler error rather than a truncation.
- **The encoding is a boundary.** The compiler emits instructions as lists; an assembler encodes them. Only the assembler, the VM's fetch and the disassembler know the byte layout, so changing it later touches three places and not the compiler.
- **A compiled function is a traced object** holding a raw `Code` object, a constants vector, an arity, a `max-stack` and a name. Constants cannot live inside `Code` because its slots are untraced; `max-stack` lets a call reserve its operand space once, as a JVM class file's `max_stack` does.
- **One evaluator at a time.** The tree-walking interpreter is the *bootstrap* evaluator, not a second production one. It runs the compiler, the compiler builds the image, and from then on only the bytecode VM runs: a form typed at a REPL, or passed to `eval`, is compiled and then run, never interpreted. The interpreter stays in the binary to rebuild an image from source with nothing but `cargo`, and is otherwise cold. This is SBCL's arrangement — `sb-ext:*evaluator-mode*` is `:compile` — and it means compiled and interpreted code never share a stack, a scope or a call.

  One rule keeps it true through the bootstrap: while the compiler is itself interpreted, it evaluates `defmacro` into the interpreter, so an expander is never compiled while its caller is not.

  The cost is that **phase 1 has no half-way image**. The VM must handle every special operator before the image runs at all, because a form the compiler cannot compile is a form nothing can run. The alternative — compile the easy forms and push an interpreter frame for the rest — would get a system running sooner at the price of machinery built to be deleted. Chosen deliberately in #25.
- **A contiguous stack.** Compiled frames are regions of one growable heap vector, not individual `Frame` objects on a cons list: a call pushes words and allocates nothing. Possible precisely because the two evaluators never interleave, so there is no boundary for an unwind to cross. The vector is an ordinary traced heap object, so slicing, image save and the collector are unchanged — except that popped slots are cleared, one store, so that stale values above the stack pointer are not kept alive.
- **Variables keep the environment chain**, with the name scan replaced by a `(depth, index)` pair — which is what `ObjectType::Environment` was shaped for. Flat closures (Dybvig, 1987) and stack-allocated locals are the faster design and need escape analysis to know which variables a closure captures. Deferred because it is compiler work rather than VM work and phase 1 is already a long walk before anything runs; the instruction set will have to change when it arrives, which phase 2's rebuild-from-source makes affordable.

**Literals are read-only.** A string read from source, and a symbol's name, are a distinct heap type that `%set-aref` refuses. CLHS 3.7.1 leaves modifying a literal undefined, which is permission to share literals and no help when a program modifies one: before this, `(setf (aref (greeting) 0) #\J)` edited the source of `greeting` silently. Immutability is also what makes sharing *sound* rather than merely licensed, so coalescing identical literals in an image stays open rather than being ruled out (#26). A program that wants to write characters uses `copy-seq` or `make-string`.

**Constant strings are packed; writable strings are wide** (#44). A constant string whose characters are all Latin-1 is a `PackedString`: a character count in its first raw slot, then eight bytes a word, so `char` is still one shift. Any other constant, and every writable string, holds one Unicode scalar per word. Only constants are packed because only a constant's characters are known for good when it is made; a writable packed string would have to grow in place to accept a `λ`, which no heap object can. Lisp cannot tell the layouts apart. UTF-8 was rejected because it makes indexing a scan; UTF-32 at two per word for every string halved the saving. Image after packages: 478,528 bytes; after `symbol-name` stopped copying, 350,944; packed, 253,912.

**Reified frames.** Stack frames are inspectable and resumable objects. The debugger, restarts and fix-and-retry all depend on this, so it is a VM design constraint from day one.

**Slices.** The VM never runs to completion on its own: it executes a bounded number of steps and returns to its caller, so the browser's event loop keeps turning. This is the same constraint as reified frames arrived at from the other side — an evaluator that can stop and resume cannot keep its state on the Rust call stack, so it is an explicit state machine (SICP §5.4's explicit-control evaluator) whose frames are objects in our heap. Saving an image mid-computation, green threads and an interruptible REPL all fall out of this.

*Who ends a slice:* the caller passes a step budget — `run_slice(budget)` — rather than the VM polling the host or watching a wall clock. Timing stays outside the VM, in the host, where it cannot affect what is computed. Slicing is therefore semantically invisible: the same program takes the same path however it is cut up, which makes runs replayable and lets the phase 3 scheduler be seeded rather than raced. The browser recovers a deadline's responsiveness by adjusting the next budget from how long the last slice took. The native host passes `u64::MAX` and ignores the whole question.

*What it costs:* a budget only bounds time if one step is short, so every primitive must be fast or itself chunked. Two invariants keep slicing invisible: collect on allocation rather than on slice boundaries, and never let Lisp observe the remaining budget.

*Waiting for the outside world:* a third way for a slice to end, `Blocked`. The VM never calls its host. A primitive that needs something from outside — the first is `%read-source` — leaves a **request** in the heap (the `Request` register) and the machine stops; the host reads it with `host::pending`, does the work at once or when a browser Promise resolves, answers with `host::complete`, and runs the next slice, which carries on as if the primitive had returned. This is a blocking system call in miniature, or a completion queue holding one request. Rust `async` and callbacks were rejected for the same reason: neither a future nor a closure can be saved in an image, and a request in the heap can — an image saved while waiting reloads still waiting. The computation cannot observe how long it waited, so blocking keeps slicing invisible. Only the bytecode VM can make a request; the interpreter, which has no I/O, refuses. See #35 and chapter 31.

**Processes.** Lightweight green threads scheduled by the VM, like Genera's processes, on one OS thread to start with. A blocked request becomes a *parked* process: the scheduler runs another instead of ending the slice, and each process holds its own request. The questions phase 3 must answer, and which ones the code must not foreclose before then, are in #34.

**Primitives.** A single table of Rust primitives (arithmetic, allocation, I/O, graphics calls) is the only door from Lisp into the host. Its size is part of the comprehensibility budget.

## Language layer

Decided: a subset of Common Lisp — a Lisp-2 with CL's names and semantics, growing toward ANSI where it pays — rather than a new dialect. Code written for it should also run in SBCL, which gives us a reference implementation to test against.

**Core features**

- **Conditions and restarts** in the CL style. Signalling does not unwind; handlers and the debugger choose a restart. This is what makes "errors are conversations" work.
- **Dynamic variables** for context (current viewer, current output stream, current package).
- **Generic functions with multiple dispatch**, a small CLOS-like object system. Presentations, commands and inspector views all dispatch on object type, so this is load-bearing, not optional.
- **Packages** for namespaces, kept simple.
- **`defmacro`-style macros**, unhygienic with gensyms. Easier to implement and to read than syntax-case.
- **First-class code objects** that know their source, their definition location and their callers (Masterscope-style cross-reference). Planned for the **start of phase 3** (#55), because its gate — fix a function from inside an error — needs the function's source, and so does the inspector. Until then a compiled function keeps only bytecode: a definition typed into the world loses its text, and code compiled before a macro changed keeps the old expansion. With source kept per definition, the world can **recompile what it holds** (all of it, or only what a changed macro reaches, via the cross-reference) instead of reloading files — Smalltalk's `compileAll`, Genera's *Compile Changed Definitions*.

**Bootstrap path**

1. **Interpreter (phase 0):** a small tree-walking interpreter in Rust, grown just far enough to run the compiler's source: macros, closures, a minimal condition system. *(This list once said "special variables" too; the interpreter never had them, and does not need them — they are tier 1, compiled only, #40.)*
2. **First build:** the interpreter runs the CL compiler, which compiles itself and the core library to bytecode and saves the first image. Slow, but it happens once. *Built: `boot::build_image` (#33) — two passes, compile everything with the interpreted compiler and then run the results on the VM, followed by a heap scan proving nothing interpreted survived.*
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

- **Tiled viewers.** Oberon-style tracks: a wide main track and a narrow tool track. Viewers split and resize but never overlap. Rooms-style workspaces come later, as named sets of viewers per task. This is a want, not a default — overlapping windows are what a system grows if nobody decides otherwise, and Oberon's own reason for rejecting them (too much complication for too small an advantage, 1987) is rule 7 in other words. What is still open is the *model*: fixed tracks or a tree of splits, keyboard-driven or mouse-driven, and whether the display protocol can express layout at all — because if it cannot, the window manager ends up in the renderer where Lisp cannot change it. See [#21](https://github.com/quokka-lisp/quokka-lisp/issues/21).
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
- **Image backup.** In the browser, the whole image can be downloaded as a file and uploaded again to restore or move it. Periodic snapshots go to browser storage automatically. What is **not** decided is whether anything records the work *between* two snapshots — a change log, in the prevalence sense, which is also what a user-built in-image database would need. See [#24](https://github.com/quokka-lisp/quokka-lisp/issues/24). The one part that cannot wait, because it constrains the image format phase 1 is fixing, is a monotonic sequence number in the image header saying which point in history a snapshot is.
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

> **Phase 1 finished** (#23): the compiled world is built by `boot::build_image`, the gate holds in it, and every differential file agrees interpreted, compiled and with SBCL (#38).

No dates yet: this is a hobby-pace project. Phases 0 to 3 are mostly Rust and compiler work; phases 4 to 6 are mostly Lisp.

**Where the work is.** Phases 0 and 1 are complete and their gates passed ([#9](https://github.com/quokka-lisp/quokka-lisp/issues/9), [#23](https://github.com/quokka-lisp/quokka-lisp/issues/23)), as is tier 0 ([#18](https://github.com/quokka-lisp/quokka-lisp/issues/18)). Quokka starts from a compiled image in which the prelude, the macros and the compiler are bytecode, and every differential file agrees interpreted, compiled and with SBCL. **Phase 2 is under way** ([#39](https://github.com/quokka-lisp/quokka-lisp/issues/39)) and **its gate passes** ([#50](https://github.com/quokka-lisp/quokka-lisp/issues/50)). Special variables, packages, the reader, printer and read-eval-print loop in Lisp, and the warm rebuild are done; the compiled compiler reproduces itself — every named function equal in C1, C2 and C3, and C2 and C3 the same world word for word — as a standing test. `setf` expanders are done (#52), built on tier 0's `setf` so that the compiler still compiles itself the same, and so are the last three special operators, with macros that see their lexical environment (#53), and sequences, with the list, iteration and string basics they needed (#54), and basic `format` (#57), bignums (#16) and double floats (#17). Every part of tier 1 now exists; the phase goes on until tier 1 is measured against ansi-test (step N).

## Testing before there is a UI

Every phase is testable from the terminal and in CI long before Quokka has a renderer or file hosts, because the test harness lives in the Rust host, which has the operating system's files and console.

*[Diagram: differential testing — one test file runs on Quokka native, Quokka in WASM and SBCL; results are compared]*

The backbone is differential testing: the same Lisp test file runs on Quokka interpreted and Quokka compiled, natively and in WASM, and on SBCL, and all of them must print the same result.

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
- [x] **Source of truth:** files on any file host are the truth; the image is a live cache of them. It follows that **images are not committed**: they are build artefacts (`.gitignore` has `*.qi`), rebuilt in seconds by the cold build or the warm rebuild, and a committed one would be an undiffable second truth that every format bump invalidates. The system's own source changes in files and reaches an image by `quokka rebuild`; when the editor lives in the image, editing a definition writes its file back. A browser will load a published image — a CI artefact or release download — not one from the repository.
- [x] **Rooting:** lifetime branding, not a shadow stack. Holding a value across a collection is a compile error. Chosen over an explicit root stack because the Rust surface is small and permanently bounded by rule 1, so the cost of the stricter scheme is bounded too. See Substrate: the Rust VM.
- [x] **Yielding:** the caller passes a step budget (`run_slice(budget)`); no clock and no yield callback in the VM. Chosen over a host-set wall-clock deadline for determinism and replayability. See Substrate: the Rust VM.
- [x] **Browser target:** WASM is first-class from phase 0. The VM runs client-side in the browser, yields to the browser's event loop, and keeps images in browser storage.
- [x] **Rendering stack:** browser-first hybrid renderer (DOM for text, canvas or SVG for drawing) behind a display protocol; native builds in a Tauri shell.
- [x] **Instruction encoding:** packed bytes with per-opcode operand widths, not a fixed 64-bit instruction word. Chosen for size under wasm32's 32-bit address space, and affordable because fixed widths avoid branch relaxation. See Substrate and #25.
- [x] **One evaluator at a time:** the interpreter bootstraps and is then cold; everything a running image evaluates is compiled first. Chosen over a mixed image, which would have bought an earlier running system with a boundary that every unwinding path had to understand. See #25.
- [x] **Host interface:** requests in the heap, answered between slices — not a synchronous trait, not Rust `async`, not callbacks. Chosen before anything was built on the synchronous shape, and because it is the only one of the four an image can save. The interpreter has no I/O. See #35.
- [x] **`tagbody` and `go`:** a `TAG` record that survives a `go` and resumes through a jump table, plus `JMP-TAG`, a one-step jump for a local `go` whose tagbody's record is innermost. Chosen over building `tagbody` from `catch`/`throw` (no VM change, about ten steps a `go`, and the opcodes left unused) and over local jumps with explicit unwinding computed by the compiler (which would need the compiler to track stack depth, where `JMP-TAG` lets the record do it). The bytecode specification's original plan did not survive implementation, and says so. See #37.
- [x] **The image build is Rust, for now:** `boot::build_image` runs during the one moment the system is half built and neither evaluator is fully in charge, and phase 2's self-rebuilding image replaces it. Writing it in Lisp would still need a Rust driver to call it. See #33.
- [x] **Phase 2's scope is all of tier 1**, not only the gate (Carsten, planning #39). The phase ends when tier 1 is complete and measured against its ansi-test groups *and* the fixpoint holds. The fixpoint is established early (after packages, the Lisp reader and `eval`/`load`) and then kept as a standing test.
- [x] **Tier 1 is a library, compiled only.** The prelude and the compiler stay tier 0, because the interpreter must run them (rule 5). Tier 1 is a third body of Lisp, loaded into the compiled world by the image's own compiler; the interpreter never learns it. Tier-1 differential files run compiled and against SBCL. See #39.
- [x] **Special variables use shallow binding** (Carsten, #39, #40): the value lives in the symbol, and binding pushes an unwind record holding the old value, restored by `UNMARK` or by the unwinder passing through. Chosen over deep binding for one-load reads and for reusing the record machinery; the cost is that phase 3's thread switch must swap bindings, as Genera's did. Settles item 3 of #34. Implemented in #40: `DBIND` pushes the record; an uncaught error or an abandoned computation also restores every binding left on the stack, since otherwise a binding would outlive the computation that made it.
- [x] **Compiled frames:** a contiguous stack in the heap, which the previous point makes free of boundary problems. See #25.
- [x] **Variables:** the environment chain with `(depth, index)`, not flat closures. Deferred rather than rejected — it is compiler work, and the instruction set will change when it lands. See #25.
- [x] **Literals are immutable:** source strings and symbol names are a separate heap type and refuse mutation, rather than relying on CLHS 3.7.1's "undefined". Chosen because undefined behaviour permits sharing without making it debuggable. See #26.
- [x] **Strings: Latin-1 constants packed eight to a word** (Carsten, #44), writable strings one scalar per word. Chosen over two UTF-32 scalars per word for every string (half the saving, one layout) and SBCL's split for writable strings too (a write of a wide character into a narrow string must fail or need an indirection). `symbol-name` returns the symbol's own constant name rather than a copy. Image 478 KB → 254 KB; format version 3.
- [x] **The reader in Lisp reads from string input streams and is readtable-driven** (Carsten, #45). Streams: a Lisp structure with `read-char`, `peek-char`, `unread-char`, `make-string-input-stream`, `with-input-from-string` — #42's option 1, input half now, output half with the printer (G); chosen over a string-and-index reader whose signature would change when streams arrive. Readtable: the standard macro characters are Lisp functions in `*readtable*`, `#` dispatches, `set-macro-character` works; chosen over a fixed `cond` that would be rewritten when readtables came. Backquote expands exactly as `reader.rs` does, checked by reading the whole system with both readers, because the fixpoint (F) compares compiled bytes. Ratios are refused, not read as symbols (floats are read since #17). Character case and letters are primitives, because Unicode's tables come free with Rust and would be kilobytes in Lisp.
- [x] **The printer in Lisp; console output by host request** (Carsten, #46). `prin1` and friends are Lisp, with package prefixes by CLHS 22.1.3.3.1 and bars by SBCL's measured rules. Output streams complete #42's option 1: string output streams, and `*standard-output*` as a console stream whose every write is a `%write-console` host request (#35), chosen over a buffer the host drains after each form (output late, every host must remember) and no console yet. Integer division arrived with it (`%quotient`, `rem`; `truncate`, `floor`, `mod` in Lisp). **The differential harness prints with the Lisp printer from step D, not before**: with test files read by the Rust reader, 66 forms printed `QUOKKA::A` for a test's `a`, correctly, because the Rust reader's flat table hands tests the build's own symbols. Making `CL-USER` use `QUOKKA` would have passed at once and reversed #41's "QUOKKA is internal" for good; waiting for the Lisp reader to read the tests changes nothing in the design. Until D the printer is checked by its own SBCL file and by printing the whole system with both printers.
- [x] **`eval` compiles; the read-eval-print loop is Lisp; image hosts read with the Lisp reader** (Carsten, #47). `eval` is `(funcall (compile-form form))` — #25's one evaluator — and the VM calls a bare compiled `Function` as a closure over nothing. `load` takes a host file name or a stream and rebinds `*package*` and `*readtable*`. The loop is `%host-step` in `eval.lisp` (read one form, `eval`, `prin1`, catch errors), chosen over a Rust loop calling Lisp three times a form; hosts call it through `vm::eval_source`. All image hosts switched at once, chosen over switching only `eval`/`load`: the REPL with an image and the compiled differential leg, which now prints with the Lisp printer as #46 planned. `compile-form` is made re-entrant by saving and restoring its five globals under `unwind-protect`, as `%assemble-lambda` already did, because a macro expander that calls `eval` compiles inside a compilation; before that it silently produced the inner form's code. Concurrent compilation by two processes is a separate question, recorded in #34.
- [x] **The image rebuilds itself, warm, in Lisp** (Carsten, #48). `%rebuild-world` (`rebuild.lisp`) reads the source in `QUOKKA` with the Lisp reader, compiles every prelude and compiler form with the current compiler, installs them all, then loads the library form by form — the cold build's two passes, chosen over a list of `load`s that would compile late compiler functions with a half-replaced compiler (GCC's stages, for the same reason). The source comes from the host by name through `%read-source` — files on disk natively, so an edited compiler rebuilds with no Rust recompiled; the binary's embedded copies elsewhere — chosen over carrying 163 KB of source in the image. The rebuild resets `*readtable*` (`defvar` would keep the old generation's reader functions) and proves that every name the source defines now holds a new function. The cold build stays, and runs once. Found by the first rebuild: a bare compiled function called from Lisp bound its empty parameter list into a scope that every `defun` inside captured; it now runs in no scope, as `Vm::start` runs one, and C2 and C3 are byte-for-byte the same size.
- [x] **The fixpoint compares functions, and whole worlds** (Carsten, #50). Every compiled function a world holds under a name is described — code bytes, parameters, stack depth, name, constants recursively, symbols with their packages, gensyms numbered by first appearance — and must be equal in C1, C2 and C3. C2 and C3 are also compared as image files word by word, allowing only gensym names that correspond one to one and the header's checksum, sequence number and gensym counter. Chosen over named functions alone (blind to code held in data, such as reader macros) and over making the files literally identical (deterministic gensym names: cosmetic, and Rust to expose the counter). Test code only. It passed first time; allocation order did not matter, because saving collects and Cheney's algorithm lays the heap out by reachability.
- [x] **The interpreter is a separate crate** (Carsten, #29). `quokka-vm` is the machine a world runs on; `quokka-bootstrap` holds the Rust reader, the tree-walking interpreter and the cold build, and depends on `quokka-vm`, never the reverse — so *one evaluator at a time* (#25) is enforced by the compiler. Chosen over #29's original Cargo feature, which would have left the interpreter in the VM's tree. The layouts both evaluators share (`closure`, `env`, `EvalError`, the special-operator table) moved into modules of their own first. No item had to become public. `quokka-wasm` still links the interpreter for its dev console until the browser loads a world.
- [x] **`quokka` runs a world; the bootstrap is a subcommand; `:save` saves from inside** (Carsten, #51). `quokka WORLD.qi` is a REPL in a world, and `quokka` without one refuses. `build`, `rebuild` and `bootstrap` (the tier-0 interpreter, prompt `boot>`) are subcommands, and each session's banner says what is running. `:save FILE` is a REPL command, handled between forms so a saved world is never mid-computation. Chosen over a Lisp `save-world` function, which would save inside its own call and resume there when loaded — possible, given that images resume mid-computation, but in need of more design.
- [x] **Licence:** MIT for everything, including the remote file service. Borrowed components must have compatible licences.
- [x] **Name and mascot:** Quokka (quokka-lisp.org). The mascot is a quokka holding a laptop showing `(λ)`, in brown and teal; assets and palette are in [`assets/`](../assets/README.md). An earlier note here said the tail was a quote mark (`'quokka`) — a good joke, since a quote is how Lisp marks something as data rather than code, but the artwork does not do it and the artwork is what exists. Recorded in case someone wonders where the pun went.

- [x] **`nil` is a symbol, in a static area the collector never moves.** The tagged-word table originally made `nil` an immediate, for the cheapest possible `null` test and a compile-time constant in Rust. Common Lisp says `nil` *is* a symbol — `(symbolp nil)` is true, it has a name, a value cell holding itself, a property list and a home package — and an immediate has nowhere to keep any of that, so every symbol operation needed a special case. Settled SBCL's way: `nil` and `t` are real symbols at fixed, known heap positions in a **static area**, a region at the bottom of the heap that the collector scans but never relocates. `Value::NIL` is therefore an object reference whose index is a compile-time constant, so `null` is still one word comparison, and the special cases disappeared. Four things broke when it landed, none of them in new code; see `book/src/part-2-reading-and-running/11-the-trouble-with-nil.md`.

- [x] **Keywords now, packages in phase 2 — and keywords are the smallest package system.** Common Lisp says a keyword is a symbol in the `KEYWORD` package, which means `foo` and `:foo` have the **same name** and are **different symbols**: `(symbol-name :foo)` is `"FOO"`, and `(eq :foo 'foo)` is false. A symbol table keyed on the name alone cannot express that, so keywords cannot be bolted on without deciding something about namespaces.

  The cheap alternative — interning `:foo` under the name `":FOO"` — was rejected. It makes `symbol-name` quietly wrong, and wrong in a way nothing detects until `symbol-name` is exposed to Lisp. `symbol-name` is therefore a primitive now, and `tests/lisp/keywords.lisp` checks it against SBCL, so the claim is testable rather than asserted.

  What exists is a `HOME` slot on every symbol and a symbol table keyed on *(namespace, name)*, with exactly two namespaces (`home::CURRENT` and `home::KEYWORD`) and none of the machinery: no `use-package`, no `export`, no `::`, no `*package*`. A keyword's value cell holds the keyword itself — CLHS says a keyword is a constant variable bound to itself — so `:foo` evaluates without the evaluator knowing keywords exist. Any other colon (`:`, `::foo`, `foo:bar`) is a read error rather than an ordinary symbol with a colon in its name, because the second would read without complaint today and mean something different later.

  **Packages proper stay tier 1, phase 2.** Nothing is blocked by their absence: `%`-prefixed internals are what every implementation does, and the full system — `use-package`, `shadow`, `export`, `::` versus `:`, package locks, `*package*` at read time — is several hundred lines against the rule 7 budget, for problems that are currently cosmetic. It also wants a hash table per package, which is group C and not built, so building it now would mean building it on lists and rebuilding it later.

  One scheduling note: packages belong at the **start** of phase 2, not inside it. The compiler's sources are written during phase 1, and packages landing after self-hosting begins means re-packaging every file mid-bootstrap. When they arrive, `symbol::HOME` stops being a small integer and becomes a pointer to a package object; nothing else in the symbol representation has to change.

  **Decided in #41** (Carsten, phase 2): packages are **Lisp objects in the tier-1 library**, not Rust structures. The Rust reader and Rust's `intern` keep their single flat list as the *system symbol table*, so the bootstrap — and the interpreter, which is tier 0 — never learns about packages. When the library loads it creates `COMMON-LISP` (`CL`), `KEYWORD`, `QUOKKA` and `COMMON-LISP-USER` (`CL-USER`) and **adopts** every existing symbol: a name among the standard's 978 `COMMON-LISP` symbols (a data file generated once from SBCL) into `COMMON-LISP`, a keyword into `KEYWORD`, everything else into `QUOKKA`. From then on `symbol::HOME` holds the package — **except for keywords**, which keep the small integer `1`, because Rust must recognise a keyword (for `keywordp`, the printer's colon, a value bound to itself) without knowing what a Lisp package object is (#43) — and new symbols are made by the Lisp `intern`. **One internal package**, `QUOKKA`, for every implementation name; user-facing extensions get packages of their own in their phases. **The `%` prefix stays** — it still says *implementation, not API*, as inside SBCL's packages, and nothing is renamed before the fixpoint. **The useful core** of the package system is tier 1 (`defpackage`, `in-package`, `*package*`, `intern`, `find-symbol`, `export`, `use-package`, nicknames, `pkg:sym` and `pkg::sym`); `shadow`, `import`, `unexport`, `rename-package`, `delete-package`, `do-symbols` and package locks arrive when something needs them. Because reading and printing a prefix needs `*package*` and use lists, the Lisp reader and printer follow packages directly in phase 2's order. Differential files run in `COMMON-LISP-USER`, as SBCL's oracle does.


- [x] **Hash tables hash contents, not addresses.** An `eq` or `eql` hash table in a real Lisp hashes an object's identity, and Cheney's collector moves objects — so an address-based hash is correct until the next collection and silently wrong afterwards. Four ways out were considered: content hashing only; a stable hash word stored in every hashable object; rehashing live tables after each collection (what SBCL does); and pinning hashed objects in the static area.

  Settled on **content hashing**, because what is actually blocked by the absence of hash tables is content-keyed: the obarray (strings), and the compiler's symbol and label tables. The collector gains nothing to do, which keeps #3 as it is through phase 1's image work.

  What this costs, stated plainly: keys may be fixnums, characters and symbols in an `eql` table, and additionally strings in an `equal` one. Any other key — a cons, a vector — is **refused with an error** rather than quietly hashed by something unreliable. `(make-hash-table :test #'eq)` therefore exists but cannot take the keys a real Common Lisp would allow, which is a divergence from SBCL under rule 6 and is why it signals loudly instead of guessing. Identity hashing comes back on the table when there is a concrete use for it, most likely alongside phase 1's image work, which touches the same code. See [#22](https://github.com/quokka-lisp/quokka-lisp/issues/22).

- [x] **`setf` was pulled forward from tier 1 into tier 0.** The tier table puts `setf` expanders in tier 1, and that is still right for the *extensible* mechanism — `define-setf-expander`, `defsetf`, and the machinery `defstruct` needs to make its accessors settable. What moved is a fixed-place `setf`: variables, `aref`, `svref`, `gethash`, `symbol-function`, `car` and `cdr`.

  The reason is testing, not convenience. Writing a vector element or a hash-table entry has no name in Common Lisp other than `(setf (aref v i) x)` and `(setf (gethash k h) v)`. Without `setf`, the only way to write either was `%set-aref` and `%set-gethash` — Quokka-only names SBCL has no counterpart for, so those behaviours were not *different* from the oracle's but **inexpressible to it**. Two of the three things tier 0's group C added were therefore unverifiable against the standard, which is a hole in the main defence against implementing Lisp wrong. Twenty lines of prelude closed it, and the mutation tests moved from Rust assertions back into `tests/lisp/`.

  What is still tier 1 is the extensibility. A program cannot teach this `setf` a new place; it knows a fixed list. `defstruct` will need the real expander.

- [x] **The extensible `setf` is built on tier 0's, not beside it** (#52, step H, Carsten). The tier-1 library (`lisp/library/setf.lisp`) adds `get-setf-expansion`, both forms of `defsetf`, `define-setf-expander`, `define-modify-macro`, and `incf`, `decf`, `push`, `pop`, `pushnew`, `rotatef`, `shiftf`, `psetf`, `remf`. It redefines `setf`, and the warm rebuild compiles the compiler with the running world's macros — so a place tier 0 knows **must expand exactly as before**, or the fixpoint fails without anything being wrong. The prelude's knowledge of places is one function, `%setf-known-place` (nil for a place it does not know); the library's `setf` asks it first and sends only unknown places through the five values, and `get-setf-expansion` builds a known place's five values by asking the same function to write into temporaries. One definition per place, two uses, so a place cannot mean one thing to `setf` and another to `incf`. Rejected: uniform expansion everywhere (every `setf` gains temporaries our compiler cannot remove, and the protocol would have to move into tier 0 to keep the fixpoint), and a separate compilation environment for the rebuild (principled — SBCL's `SB-XC` — but the compiler would take a macro environment everywhere, for a problem the shortcut does not have; it stays possible). SBCL takes the same shortcut: `(setf (car x) v)` is `(sb-kernel:%rplaca x v)`.

  Also decided: tier 0's `(setf (car p) v)` evaluated `v` before `p`, against CLHS 5.1.1.1; **fixed in the prelude**. New places are those whose reader exists (`cadr` and the like, `second`, `third`, `nth`, `elt`, `symbol-value`, `values`, `the`) and `getf`, the place inside a place. Not yet: `(setf foo)` functions (`defun` takes symbols only), and `&environment`, which is accepted and bound to nil — there is no lexical macro environment, so a `macrolet` place is not seen (revisit with `symbol-macrolet`, step I).

- [x] **The last three special operators are tier 1** (#53, step I, Carsten). CLHS 3.1.2.1.2.2 lets a special operator be a macro, and none of the three is in the interpreter's table, so the interpreter never learns them. **`progv`** is a library macro over a recursive `%progv` that binds one name per level through the compiler form `%dynamic-bind`, which emits one new instruction, `DBIND-SYMBOL` — `DBIND` with the symbol from the stack — plus a `makunbound` primitive for names given no value: every binding is an ordinary record, restored by everything that restores a `let`. Rejected: a `PROGV` instruction binding a whole list (a new record kind and an unbounded restore loop, against rule 9), and `set` with `unwind-protect` (an abandoned computation runs no cleanup; cannot unbind). **`load-time-value`** is a library macro that calls `eval` while it expands — expansion is compile time in a system whose `eval` compiles, which is the moment CLHS gives for `compile`. **`symbol-macrolet`** is in the compiler: a compile-only scope in the lexical environment chain, stepped over by `%lookup` when counting run-time depth, so a `let` inside it shadows it by construction and `setq` of one compiles as `setf` of its expansion.

  **Macros see their lexical environment.** While an expander runs, the compiler's scopes and local macros are its environment, `(:environment scopes local-macros)`; `&environment` in `defmacro` and `macrolet` is taken out of the lambda list by the compiler (the Rust rewrite never sees it) and bound to that object. The compiler expands macros itself instead of calling `macroexpand-1`, so the library can redefine `macroexpand-1` and `macroexpand` to take an environment without changing how the compiler compiles itself; `get-setf-expansion` passes it on, so `incf` on a symbol macro evaluates its subforms once, and a `macrolet` place works. Rejected: `symbol-macrolet` without environments (read/`setq`/`setf` right, `incf` evaluating twice). Not yet: `define-symbol-macro` (global symbol macros). A local function shadowing a global macro of the same name was fixed in J1 (#54).

- [x] **Bignums: the algorithms in Lisp, the boundary in Rust** (#16, step L, Carsten). Rust owns the representation and what is bounded by it: a `Bignum` object of raw slots, a sign word and **limbs of 30 bits, sign and magnitude** (as CPython's `int`) — thirty so that a limb product plus a carry is still a fixnum and the limb loops in Lisp never need bignums themselves; normalising (a value that fits is always a fixnum, so a bignum never equals a fixnum), comparing and hashing, justified under rule 9 as one pass over an object whose allocation already touched every limb. Lisp owns addition, multiplication, division (Knuth's algorithm D), printing and the bit operations (on two's complement by conversion, as CPython). **The boundary is a fallback in the machine**: when `+ - * = < > %quotient rem` fail with overflow or meet a bignum, `Vm::call_value` calls `%generic-+` and its kin with the same arguments, so the fixnum path is untouched. Rejected: redefining `+` in Lisp (every addition a Lisp call consing a rest list) and signal-and-retry. Not in L: ratios and `/` (#59). **Found on the way:** every call with `&optional`, `&rest` or `&key` interned the unsupplied marker by name, a walk of the whole symbol table (~2,000 symbols, ~50 µs a call); fixed with a heap **symbol cache** — a root, not saved in images — which the arithmetic fallbacks use too.

- [x] **Floats: one format, boxed, arithmetic and conversions in Rust** (#17, step M, Carsten, seven decisions). (1) A double is a **boxed** heap object, `DoubleFloat`, one raw slot; NaN boxing rejected (it replaces the tag scheme and breaks every image); specialised float arrays wait for phase 4. (2) **Doubles only**: `short-float`, `single-float`, `double-float` and `long-float` all name the IEEE double (CLHS 12.1.4 permits it); `*read-default-float-format*` keeps its initial value `single-float`, the same type, so `1.5` is a double. Known difference from SBCL, whose `1.5`, `(sqrt 2)` and `(float 1)` are 32-bit singles; differential files bind the variable to `double-float`. (3) **The fastest float arithmetic**: `+ - * / = < >` and `rem` take fixnums and doubles in Rust after the untouched fixnum path fails (`crate::float`), two at a time from the left as SBCL does, with contagion (CLHS 12.1.4.1); only a bignum meeting a double goes through the Lisp fallback. Comparison is exact. `/` exists: exact on integers that divide, an error for a ratio until #59. (4) **Traps as SBCL's**: a result that would be an infinity or a NaN signals (`division-by-zero`, `floating-point-overflow`, `floating-point-invalid-operation`), checked when a double is boxed since wasm32 has no hardware traps; underflow is quiet; no Lisp code ever holds an infinity or a NaN; `eql` compares bits, so `0.0` and `-0.0` differ. (5) **Rust converts** between floats and text: `str::parse` (correctly rounded) and the formatter's shortest round-trip digits; the layout (positional between 10^-3 and 10^7, as SBCL) is Lisp. `format`'s `~F ~E ~G ~$` round the double's exact value with halves up, as SBCL's do, in Lisp over bignums. Rejected: Clinger's reader and Burger and Dybvig's printer in Lisp (readable but tens of microseconds to a millisecond a number). (6) Scope: CLHS 12's float functions; not `rational`/`rationalize` (#59), no complex results. (7) Transcendental functions from the **`libm` crate on every target**, the VM crate's first dependency, so an image computes the same bits natively and in a browser; SBCL differs in the last bit (`(exp 1d0)`), so tests print them rounded. Cost: 515 lines of Rust against ~185 estimated.

- [x] **Step J, sequences, is three sub-steps** (#54, Carsten), because the gap was wider than CLHS 17: J1 equality, control and iteration macros and the CLHS 14 list functions; J2 the sequence functions; J3 strings — a commit and a differential file each. **`eql` exists before bignums**, as `eq`: exact while nothing is boxed (every integer a fixnum, every character an immediate), so every `:test` is written with CL's default from the start and step L changes one function. **The library redefines the prelude's narrower functions with CL's full signatures** (`member`, `string=`, `char=` …): calls are late-bound, so the compiler's compiled code is untouched and only what it *calls* after a rebuild changes — checked by the fixpoint on every run. **No prelude macro is redefined.** **The sequence functions are written once each over an iteration macro** that expands into a list loop and a vector loop — SBCL's `seq-dispatch` idea — rather than twice by hand or over a runtime protocol, which would cost a call per element without an inliner. An inliner was costed in the discussion (a declaration table, kept sources, let-conversion, capture safety needing a `macroexpand-all` walker, and repeated simplification for funargs) and left for when a profile asks for it. The cost of the macro is code size: each body is expanded four times (list or vector, forwards or backwards), and J2 took the cold-built image from 474 KB to 565 KB.

- [x] **Conditions are flat, not a class hierarchy.** Tier 0 asks for `error` and `handler-case`, and Common Lisp's are built on CLOS: `simple-error` is an `error` is a `serious-condition` is a `condition`, programs add their own with `define-condition`, and `handler-case` matches by *subtype*. Quokka has no class system until phase 3.

  So a condition is an object with a type symbol, a message and some data, and matching is flat: a clause naming `error`, `condition` or `t` catches everything, anything else must name the condition's own type exactly. The evaluator's own errors are given the standard's type names — `type-error`, `unbound-variable`, `undefined-function`, `program-error`, `control-error`, `arithmetic-error` — so code written against Common Lisp catches what it expects, and the differential tests check that it does.

  What this costs: a program cannot define a condition type. `(error 'my-error)` works here and is not portable, because a real Common Lisp requires that symbol to name a class. That half is therefore untestable against the oracle, and is covered by Rust tests. `define-condition` and subtype matching arrive with the class system in phase 3, along with `handler-bind` and restarts.

  `HeapExhausted` and `BudgetExhausted` are deliberately **not** conditions: neither is a fault in the program being run, and a handler catching them would be catching the machinery. Heap exhaustion becomes a `storage-condition` in phase 3, when there is a hierarchy to put it in.


**Still open** (none blocks phase 0)

- [ ] **Concurrency:** green threads on one OS thread for good, or plan for real parallelism later? Open in [#34](https://github.com/quokka-lisp/quokka-lisp/issues/34), which recommends one OS thread per world for good.
- [ ] **Remote protocol:** WebDAV, an S3-compatible API, or our own protocol over HTTPS?
- [ ] **Git in the browser:** through a Git hosting API, or a Git implementation compiled to WASM?

## To-dos

- [x] Register the quokka-lisp.org domain.
- [x] Create the repository: [github.com/quokka-lisp/quokka-lisp](https://github.com/quokka-lisp/quokka-lisp) (private for now).
- [x] Claim the quokka-lisp GitHub organisation and transfer the repository there.
- [ ] Build the quokka-lisp.org website.
- [ ] Reserve the crates.io names (e.g. `quokka-lisp`, `quokka-lisp-vm`) — publish a small real first version, not an empty placeholder.
- [ ] Reserve the npm package name for the renderer (e.g. `@quokka-lisp/renderer`).
