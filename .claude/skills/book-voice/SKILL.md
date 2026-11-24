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

**Headings name the topic, as a question, a verb phrase or a plain noun
phrase.** "What is a place?", "Introducing places", "The missing operators"
are all fine. What is not fine is a heading that is a question followed by a
first sentence that merely answers it in a clipped fragment:

> ## What was missing?
>
> Fifteen operators, led by `delete-package` (202 tests) …

That reads like a quiz, not like a person explaining. Either give the
section a plain heading and open with a full sentence that says what
happened:

> ## The missing operators
>
> While running the tests we found that fifteen operators were still
> missing from the library, especially `delete-package` (202 tests) …

or keep the question and open the section with context that leads to the
answer, never with the answer as a sentence fragment. The fixed sections
may stay "What did it cost?" and "What is still missing?", with the same
rule for their first sentence. Chapter titles may stay short nouns
("Places").

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
2. **Negative parallelisms as a reflex.** "A basis, not a feature list."
   The same shape has three common forms, and all three are tells: "not only
   X, but also Y", "not X, but Y", and the reversed "Y rather than X". They
   read as though correcting a misconception the reader never had. Keep one
   where the contrast is the actual content, as in "offsets, not addresses",
   and otherwise just say what the thing is. The fault is the reflex, not the
   words: "a trait rather than a method on `Vm`" is ordinary English naming a
   real alternative, and should stay.
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
9. **Tidy symmetry, and the rule of three.** Everything in threes, every
   option the same length, every section the same shape. Watch especially for
   "adjective, adjective, adjective" and "short phrase, short phrase, and
   short phrase", which is how a thin observation is dressed up as a thorough
   one. Count the items: if there are three because there are three, keep
   them; if the third is padding, cut it. Let sections be as long as their
   content.
10. **The narrator who knows the ending.** Decisions told as clean arguments.
    The book's rule is to write from the real questions, so tell it as it
    happened: what we expected, what turned up, what we changed. "We first
    tried…; it failed because…; so we…".
11. **Inflated vocabulary.** "Leverage", "robust", "seamlessly", "delve",
    "load-bearing", "crucially", "elegant", "powerful", "unlock". Use the
    plain word. The set shifts as models change, and the words travel in
    packs: where one appears, look for the others. Current company includes
    "underscore", "showcase", "highlight" as a verb, "foster", "enhance",
    "align with", "pivotal", "intricate", "landscape" and "tapestry" as
    abstract nouns, "testament", "meticulous", "multifaceted", "quietly" and
    "steady".
12. **Question heading, fragment answer.** A heading asks "What was
    missing?" and the first line answers "Fifteen operators, led by …".
    Open with a full sentence that tells what happened ("While running the
    tests we found that …"), or use a plain heading. A "Why …?" heading
    answered by "Because …" is the same fault: drop the "Because" and let the
    sentence stand on its own.
13. **Rhetorical questions as section openers** ("So what does a macro see?").
    Allowed when it is the question the reader would actually ask; not as a
    device in every section.
14. **Stage directions.** A sentence, usually a fragment, that announces where
    in the argument the reader is instead of saying anything: "Before the
    answer, the thing being referred to.", "First, some background.", "With
    that out of the way.". Delete it; the paragraph it introduces reads better
    without it, and the section heading has already said what is coming. The
    same goes for a sentence that counts or previews the headings below it
    ("Three questions fall out of that table."): the headings follow
    naturally, and the reader can see them.
15. **Dramatising a plain question.** Announcing that a statement is bold or
    surprising, raising an objection on the reader's behalf and answering it
    in instalments: "That is a strong claim, so start with the obvious
    objection.", "The claim is narrower than it sounds." Ask the reader's
    question plainly instead, in one sentence, and say that the chapter
    answers it. Three sentences of build-up usually compress to one.
16. **Telegraphic sentences.** Economy is a virtue until the reader has to
    decode the sentence: "The scheme is Symbolics'." makes them work out that
    the apostrophe means a company, that the company built hardware, and that
    the scheme was theirs. Write "This is the scheme that was used by
    Symbolics in their Lisp Machine." The compressed version saves eight
    words and costs a re-read, and a reader who does not already know the
    reference gets nothing from it at all. Attribution especially should be a
    plain full sentence: who, what, where.
17. **A payoff with no setup.** A sentence that lands a result the reader
    cannot value, because the chapter never said what was at stake. "Nothing
    is given up to get them." is a relief only once the reader knows that the
    bits in a word are scarce and that a fixnum pays for its tag with a bit of
    range. Before writing that something is free, cheap, exact or fast, say
    what the budget is and what it costs elsewhere; usually one short
    paragraph, placed where the design first spends the resource. More
    generally, name the constraints a design works under before showing how
    the decisions fare against them, because a reader new to the field does
    not know which resources are scarce in a runtime and will not supply them
    for you.

18. **Avoiding "is" and "are".** "The reader serves as the front end", "the
    heap functions as a work queue", "`setf` represents the general
    mechanism", "the crate features three modules". Machine prose reaches for
    "serves as", "stands as", "functions as", "represents", "features",
    "offers", "boasts" and "refers to" where a person writes "is", "are" or
    "has"; one study measured the word "is" dropping by a tenth in academic
    writing the year chatbots arrived. Use the copula. "The reader is the
    front end" is shorter, says the same thing, and loses nothing.
19. **Participle tails.** A sentence that has said its piece and then grows an
    "-ing" clause explaining the significance of what was just said: "…,
    highlighting the flexibility of the design", "…, underscoring its
    importance", "…, reflecting the project's priorities", "…, ensuring
    correctness". The tail almost never carries a fact. It adds a judgement
    the reader did not ask for and cannot check, and it is one of the
    strongest tells there is. Delete it; if the point is real, make it a
    sentence of its own with something concrete in it.
20. **Significance padding.** Sentences that tie the subject to a broader
    trend, a wider history or an ongoing debate in order to make it matter:
    "plays a role in", "forms part of the wider history of", "has shaped
    discussions about", "contributes to the broader field of". A decision's
    significance is what it lets you build next, which is a fact. Its
    contribution to the discourse is not. Say what it enables, point at where
    that happens, and stop.
21. **Vague attribution, and vague connection.** "Experts argue", "industry
    reports suggest", "it is widely regarded as", "several sources" when you
    have one. Name the source or drop the claim. The same goes for
    relationships: "X is associated with Y", "X is connected to Y", where the
    honest sentence is "X wrote Y" or "X calls Y". Where the relation is
    genuinely unclear, that uncertainty is the thing to write down, in those
    words.
22. **Chat residue and placeholders.** Nothing addressed to the person who
    asked for the text should survive in it: "Certainly!", "Here is a draft",
    "I hope this helps", "Would you like me to…", "Note: verify before
    publishing". Nor should placeholders: "(add link here)", "[TODO:
    numbers]", an invented date, a reference that looks plausible and was
    never checked. A text containing any of these has not been finished. A gap
    that is real gets a plain sentence saying what is missing and why.
23. **Mechanical tells.** Title case in headings where the document uses
    sentence case; curly quotes and apostrophes mixed into a file that uses
    straight ones; emoji on headings or list items; asterisks as decoration;
    stacked lists where every item is a bold label and a colon; heading levels
    that jump. Each is trivial alone and the pattern is not. Match the file's
    existing conventions, and compare a rewrite against the original for
    quotes and dashes especially, since those change silently.

These are signs, not rules. Human writing contains all of them, and a text is
not machine-written because one appears; it is the density that gives it away.
Fixing the surface without fixing what it points at only makes the trouble
harder to find: if a sentence grew a participle tail because the paragraph had
nothing to say, deleting the tail is not the repair.

A quick check after a rewrite, from the repository root (counts, not proof):

```bash
grep -cE ' — ' FILE                       # em-dashes
grep -cE '^\*\*[^*]+\.\*\*|^- \*\*' FILE    # bold lead-ins
grep -oiE 'crucial|notably|load-bearing|seamless|robust|leverage|delve|underscor|showcas|pivotal|tapestry|foster|multifaceted' FILE | wc -l
grep -oiE 'serves as|stands as|functions as|serve to|is characterised by|boasts' FILE | wc -l
grep -oiE ', (highlight|underscor|reflect|emphasi|ensur|showcas|demonstrat|contribut)ing' FILE | wc -l
grep -oiE 'not only .{1,40} but|not just .{1,40} but' FILE | wc -l
grep -oE '‘|’|“|”' FILE | wc -l       # curly quotes
```

Count occurrences, not lines, and read every hit before changing it. These
patterns are broad on purpose and most of what they catch is innocent: "rather
than" and "refers to" are ordinary English, and the first draft of this check
flagged thirteen correct uses of "rather than" in three chapters. A bracket
expression of curly quotes also matches the bytes inside an em-dash or an
arrow, which is why the check lists them as alternatives instead. The counts
find candidates; you decide.

Habits 18 to 23, and the sharper forms of 2, 9 and 11, draw on Wikipedia's
*Signs of AI writing* (WP:AISIGNS), the field guide WikiProject AI Cleanup
keeps for spotting undisclosed machine text.

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

From chapter 1, in a first pass of this rewrite:

> ## The heap
>
> Before the answer, the thing being referred to. The heap is an array of
> 64-bit words that the VM owns, in Rust literally a `Vec<u64>`, and an
> object is a run of adjacent words in that array.

In the book's voice, with the stage direction simply gone:

> ## The heap
>
> The heap is an array of 64-bit words that the VM owns, in Rust literally a
> `Vec<u64>`, and an object is a run of adjacent words in that array.

Also from chapter 1, after the sentence saying every object is one 64-bit
word:

> That is a strong claim, so start with the obvious objection. A list of a
> thousand elements is not going to fit in eight bytes, and neither is a
> string. The claim is narrower than it sounds, and the distinction it rests
> on is the subject of this chapter.

In the book's voice, asking the reader's question instead of selling it:

> If every object is represented by only 64 bits, how can we fit in those
> larger objects, like lists, or functions? We will explore how this works in
> this chapter.

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
