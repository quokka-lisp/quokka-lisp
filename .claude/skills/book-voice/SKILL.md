---
name: book-voice
description: The voice of The Road to Quokka. Use when writing a new chapter of book/, or rewriting an existing one, so it reads like a person wrote it — Kernighan and Ritchie's economy, Wirth's plainness, Seibel's warmth, and Carsten's own practical, first-person-plural style — and not like a language model. Also use for any longer prose in docs/ meant for people to read.
---

# The book's voice

*The Road to Quokka* is written by Carsten, with Claude drafting. It should
read like one engineer explaining a system he built to another, not like
generated documentation. This skill describes that voice, lists the habits
that give machine-written prose away, and gives a procedure for rewriting a
chapter.

The rules in `CLAUDE.md` under **The book** still hold, all of them: real code,
a "Where this fits" opening, a status line, glossary entries, the cost of each
decision, saying what is unbuilt. This skill changes how those things are
*said*, never whether they are there.

## The reader is paying

A reader pays for every chapter with time, sometimes with money, and always
with attention, and concentrating is hard work. So every chapter, and every
long section, keeps telling them three things:

1. **Where you are**: what is already built, and how this part connects to it.
2. **Why you should care**: the problem this solves, felt before it is
   solved.
3. **What you'll get for your trouble**: what the reader will understand or
   be able to do afterwards, and what this part makes possible next.

The craft is in answering them **without letting the reader see the
questions**. No "Why should you care? Because…", no labels, no checklist of
three paragraphs. The answers are woven into ordinary sentences: a section
opens with the problem in a concrete example (that is the "why"), mentions in
passing the chapter it builds on (the "where"), and ends on what now works
(the "what you get"). If a reader could tick the three off as they read, it
is too visible.

The "Where this fits" note is the chapter-level version; the same three
questions apply in miniature to a section that asks for real effort. This is
the book's first rule of tone: be kind to the reader, and earn their
attention rather than assume it.

## The voice to aim for

Four models, each for something different. Learn the register from them; never
copy their text.

- **Kernighan and Ritchie, *The C Programming Language*.** Economy. Show a
  small complete example, then say what is interesting about it, then stop. No
  adjective that does not carry information. Paragraphs of ordinary length that
  explain one thing each.
- **Niklaus Wirth, *Project Oberon*.** Plainness about design. A decision is
  stated, its reason given in a sentence or two, and the alternative mentioned
  without drama. Wirth trusts the reader; he does not announce that something
  is important, he shows why.
- **Peter Seibel, *Practical Common Lisp*.** Warmth and motivation. Start from
  a problem the reader can feel, build the solution in steps, say "you" and
  "I", allow a dry aside. The reader should feel accompanied, not lectured.
- **Carsten's own writing** (a design document of his was read as the anchor;
  it is a client document, so it is described here and not included). Its
  traits:
  - **"We" for the project's decisions** ("We propose…", "we listen to…",
    "we always fetch the latest data"), and a practical, slightly formal
    register.
  - **Reasons inside the sentence**, not set apart: "This is done because
    webhook notifications are not guaranteed to be received in chronological
    order (or at all), which may cause desynchronisations between the two
    systems."
  - **Concrete numbers and worked examples**, followed by a plain sentence
    saying what they mean ("This means that for a drone flight of 10ha …
    we would get an estimated 98 image files.").
  - **Modest hedges where things are genuinely uncertain**: "a rough estimate",
    "may", "would". Not every claim is absolute.
  - **Lists only for real enumerations** (the options an organisation has, the
    events to subscribe to), introduced by a full sentence.
  - **Headings that name the topic plainly**, not teasers.
  - Long sentences are allowed when the thought is long.

Put together: **"we" for the project, "you" for the reader.** "We" is the
team, Carsten and Claude, who work on Quokka together and discuss things among
themselves. **Never name Carsten (or Claude) in the text**: not "Carsten
chose", not "Carsten's first reaction"; write "we chose", "our first
reaction". A decision made in an issue is ours.

**Active voice, decision first.** "We chose the third option, a shortcut,
even though our first reaction was against shortcuts" — not "The third was a
shortcut, and that is what we chose." Say who did what, then why. Plain
declarative sentences. Paragraphs that flow from one to the next with ordinary
connectives ("so", "but", "which means", "this is why"). Examples before
abstractions.

**Headings are questions or start with a verb.** "What is a place?", "How
is an expander made?", "Introducing places", "Finding a bug in tier 0" — not
a bare noun phrase like "What a place is" or "A bug in tier 0". The fixed
sections follow the same rule: "What did it cost?", "What is still missing?".
Chapter titles may stay short nouns ("Places").

**Remind the reader what a term means when it comes back.** A reader may
open the book at any chapter, or have forgotten. When a key idea from an
earlier chapter reappears (the fixpoint, a tier, the warm rebuild, C1/C2/C3,
a gensym), fold a short reminder into the sentence and point at its chapter:
"The fixpoint, our check that the compiler compiles itself into exactly the
same code generation after generation (chapter 42), held the first time." Do
it at the first mention in the body of a chapter, and again where the term
returns after several sections. A clause or a sentence, not a footnote or a
full definition.

## Habits to remove

These are what make prose read as machine-written. Some are fine once in a
chapter; the problem is the pattern.

1. **Aphoristic closers.** A paragraph or section ending on a punchline: "That
   is the point." "Not luck." "The jargon is the wall." End on the last piece
   of information instead, or on the sentence that leads into the next section.
2. **"X, not Y" contrasts as a reflex.** "A basis, not a feature list." Keep
   one where the contrast is the actual content; otherwise just say what it is.
3. **Bold lead-ins on every paragraph or bullet.** Bold is for the first use of
   a term (the book's convention for glossary terms) and, rarely, for the one
   sentence a reader must not miss. Never as a paragraph label.
4. **Slide-deck structure.** Every section as option A / B / C with matching
   bullets; three items per list; tables for things that are really prose.
   Write the comparison as paragraphs, with a table only where the reader
   genuinely compares along columns.
5. **Colon reveals and dramatic setups.** "Here is where it got bigger:", "The
   catch:", "One word: …". Say the thing.
6. **Em-dash as the main rhythm.** One or two a page is plenty. Use commas,
   parentheses, or two sentences.
7. **Restating summaries.** A sentence that repeats what the paragraph just
   said, or a section ending "In short, …". Delete it.
8. **Announcing importance.** "This is the crucial part", "Crucially",
   "Notably", "It's worth noting". If it matters, the explanation shows it.
9. **Tidy symmetry.** Everything in threes, every option the same length,
   every section the same shape. Let sections be as long as their content.
10. **The narrator who knows the ending.** Decisions told as clean arguments.
    The book's rule is to write from the real questions, so tell it as it
    happened: what we expected, what turned up, what we changed. "We first
    tried…; it failed because…; so we…".
11. **Inflated vocabulary.** "Leverage", "robust", "seamlessly", "delve",
    "load-bearing", "crucially", "elegant", "powerful". Use the plain word.
12. **Rhetorical questions as section openers** ("So what does a macro see?").
    Allowed when it is the question the reader would actually ask; not as a
    device in every section.

A quick check after a rewrite, from the repository root (counts, not proof):

```bash
grep -cE ' — ' FILE                       # em-dashes
grep -cE '^\*\*[^*]+\.\*\*|^- \*\*' FILE    # bold lead-ins
grep -ciE 'crucial|notably|load-bearing|that is the point|seamless|robust' FILE
```

## Before and after

From chapter 43, as first written:

> A known place therefore expands exactly as it did, because the same code
> writes it. That isn't a matter of care. The fixpoint tests confirm it on
> every run.

In the book's voice:

> So a place the prelude knows still expands exactly as before: the same
> function writes it, before and after the library is loaded. We did not have
> to be careful to keep the two in step, and the fixpoint tests check on every
> run that they are.

From chapter 44, as first written:

> **The object.** While the compiler runs an expander, it records its current
> `env` in `%expansion-scopes`, next to `%local-macros`, which `macrolet`
> already kept.

In the book's voice:

> While the compiler runs a macro's expander, it keeps its current
> environment in a variable, `%expansion-scopes`, next to `%local-macros`
> where `macrolet` already kept the local macros. The two together are what
> we hand the macro as its environment.

## Rewriting a chapter

1. **Read the whole chapter first**, and the issue it cites, so the rewrite
   tells the real story of the decision.
2. **Keep everything that is content**: every code block byte for byte, every
   issue and chapter number, every fact, figure and option with its cost, the
   "Where this fits" note and the status line (they may be reworded, not
   removed), and the references.
3. **Rewrite the prose** in the voice above, removing the habits listed. Merge
   bullets back into paragraphs where they were really an argument. Keep a
   list where it is a real enumeration.
4. **Check** that the code blocks are unchanged
   (`git diff --word-diff FILE` and look only at prose) and run the counts
   above.
5. **Leave the decision to Carsten.** A rewrite is a proposal: show the diff or
   the new text, and fold his corrections back into this skill (add the habit
   he spotted, or a better before/after), so the next chapter starts closer.

New chapters are written in this voice from the start.
