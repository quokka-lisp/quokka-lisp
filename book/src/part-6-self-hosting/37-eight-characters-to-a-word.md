# Eight characters to a word

> **Where this fits.** Chapter 5 laid out how objects sit in the heap, and
> chapter 19 how a string is read by index. Both took the simplest layout
> possible: one character per word. Chapter 36 adopted every standard
> Common Lisp name, and the image grew to 478 KB, mostly symbol names. This
> chapter is the decision that one-character-per-word layout had been waiting
> for, and how it was made: measure first, take the free saving, then choose a
> layout by what each one makes impossible. It comes before the reader moves
> into Lisp, because the reader and the printer will make and walk strings all
> day.

*Status: done. Issue #44.*

## A promise in a comment

When strings were first built, `heap.rs` said this about them:

```rust
/// One Unicode scalar per word. That wastes most of each word, and it is
/// deliberate for now: it makes indexing a character O(1) and the code
/// obvious. Packing comes when something measures that it matters.
```

A **Unicode scalar** is one character's number in Unicode, the standard that
gives every character in every writing system a number: `A` is 65, `é` is 233,
`λ` is 955. The largest is a little over a million, so it fits in 21 bits. A
word has 64. Every letter of every string was spending 43 bits on nothing.

That was the right call at the time. It made `(aref s 5)` one memory read, and
it made the code impossible to get wrong. The comment named the condition for
changing it: *when something measures that it matters*. Chapter 36 was the
measurement.

## Measure before choosing

The first step was to find out what the 478 KB actually *was*. An image is a
heap written out word by word (chapter 30), and every object starts with a
header saying its type and size (chapter 5), so a short script can walk the
file and add up the words per type:

| | words | share |
| --- | --- | --- |
| symbol names (constant strings) | 17,509 | 29% |
| other strings | 15,948 | 27% |
| symbols | 9,443 | 16% |
| conses | 8,664 | 15% |
| everything else | ~8,200 | 14% |

Two numbers in that table are surprising. The first: there were 1,287 ordinary
(writable) strings, and every one was a **key in a package table**. The second:
1,287 is exactly 978 `COMMON-LISP` names plus 309 `QUOKKA` names. Every name was
stored twice.

The reason was one line in the `symbol-name` primitive:

```rust
let name = mc.symbol_name(args[0]);
Ok(mc.alloc_string(&name))
```

It handed out a fresh copy of the name on every call. The package code in
chapter 36 does `(setf (gethash (symbol-name symbol) table) symbol)`, so each
table held a copy.

Copying looks like the cautious choice: a caller who scribbles on the copy
cannot hurt the symbol. But Quokka already has a better guard. Since #26 a
symbol's name is a **constant string**, a separate heap type that `%set-aref`
refuses to write (chapter 19). Handing out the name itself is therefore *safer*
than a copy. A program that tries to change a symbol's name now gets an error,
where before the write landed on a throwaway copy and silently did nothing.
CLHS says modifying a symbol's name has undefined consequences, and refusing is
the safe way to read "undefined".

So the first change was to return the name itself:

```rust
Ok(mc.slot(args[0], symbol::NAME))
```

One unit test changed its expected answer, from "the write succeeds on the
copy" to "the write is refused". The image went from **478,528 to 350,944
bytes**, 27% smaller, with no new code. This is the reason to measure before
designing. Without the table, packing would have shrunk 1,287 strings that
should not have existed at all.

## Three ways to pack, and one that was asked about

After the first fix, strings were 17,509 of 43,821 words: 40% of the image.
All 1,391 of them were constant strings, and every one was plain ASCII. Two
standard ways of packing characters into words were on the table, a third
combination came up, and we asked about a fourth.

**Two 32-bit characters per word (UTF-32).** A **character encoding** is a rule
for writing characters' numbers as bytes. UTF-32 writes every character as 32
bits, so two fit in a word. There is one layout for every string, character
`i` is in word `i / 2`, and any character can be written anywhere. It halves
the strings and saves 14% of the image.

**Eight Latin-1 characters per word.** **Latin-1** (ISO 8859-1) is the
encoding that covers Western European text in one byte per character, and its
256 codes are exactly Unicode's first 256. That last fact is what makes it
cheap: a byte *is* the character's Unicode number, so reading one means
shifting it out of the word, with no decoding step. Eight fit in a word. This
packs strings to about a third and saves 28% of the image.

It has a catch. A string full of `λ` cannot be stored this way, so a second,
*wide* layout must exist next to it. And a writable string causes a real
problem: what happens when a program stores `λ` into position 3 of a packed
`"hello"`? The string would need more words. An object cannot grow where it
stands, because other objects hold its address and the words after it belong
to someone else. SBCL has the same split. Its `base-string` holds one byte per
character, and storing a non-base character into one is an error. That is
allowed in SBCL only because `base-string` is a type the programmer asked for.
`make-string` gives a full-width string by default.

**What if only constants are packed?** That is the variant chosen. A constant
string's characters are all known when it is made and never change, so it can
be packed when they all fit in Latin-1 and left wide when they do not. Writable
strings stay one character per word, exactly as before, so a write can never
need to widen anything. And today *every* string in the image is a constant,
so this variant gets the full 28%.

**And UTF-8?** We asked that too, and the answer is the reason this chapter has a
section on choosing. UTF-8 is the encoding the rest of the world uses for
files, the web and Rust's own `String`. It stores ASCII in one byte and other
characters in two to four. Variable width is exactly the problem:

- **Indexing becomes a scan.** To find character 1,000 you have to walk the
  999 before it, because any of them might be wider than a byte. `(char s i)`
  stops being O(1), and a loop over a string, which the reader and printer are
  made of, becomes O(n²). It would also break rule 9's promise that a primitive
  is bounded.
- **Writing can change the size.** Replacing `e` with `λ` turns one byte into
  two, the same wall as above, for every non-ASCII character.
- **It saves nothing here.** For ASCII text, UTF-8 and Latin-1 are byte for
  byte identical.

Systems that keep UTF-8 internally pay to get indexing back. PyPy stores a
flag saying whether a string is all ASCII. Swift keeps "breadcrumbs", a small
index of where every so-many characters start. Where UTF-8 does win is at the
*boundary*. The browser and Rust both speak it, so converting on the way in and
out would be free. Quokka pays that conversion once per file read or message,
not once per character, so it was the cheaper thing to keep paying.

| | image | each string | indexing | a write that does not fit |
| --- | --- | --- | --- | --- |
| one character per word (before) | 351 KB | 8 bytes a character | O(1) | cannot happen |
| UTF-32, two per word | ~300 KB | 4 bytes | O(1) | cannot happen |
| Latin-1 for constants, chosen | 254 KB | 1 byte | O(1) | cannot happen: constants are never written |
| Latin-1 everywhere, SBCL's split | 254 KB | 1 byte | O(1) | error, or an indirection |
| UTF-8 | 254 KB | 1–4 bytes | O(n) | the string must grow |

## The layout

A packed string is a new object type, `PackedString`, numbered 68. It sits
above 64, the line below which the collector traces slots (chapter 5), so it
is never mistaken for references. Its first slot holds the **character count**,
and the bytes follow, least significant byte first:

```
header   [ 3 words | PackedString ]
slot 0   12                               ← how many characters
slot 1   c a f é ␣ a u ␣                  ← eight bytes
slot 2   l a i t · · · ·                  ← four, and padding
```

The count is needed because the header counts *words*. Five characters and
eight both fill one word, so the word count alone cannot say how long the string
is. The other layouts do not need a count: one character per word means the
word count *is* the character count.

Only two functions in the whole VM know the layouts differ:

```rust
pub fn string_length(&self, v: Value<'h>) -> usize {
    let header = self.header(v);
    assert!(header.object_type.is_string(), "not a string");
    if header.object_type == ObjectType::PackedString {
        self.raw_slot(v, 0) as usize
    } else {
        header.slots
    }
}

pub fn string_char(&self, v: Value<'h>, index: usize) -> char {
    assert!(index < self.string_length(v), "index past the end of a string");
    if self.header(v).object_type == ObjectType::PackedString {
        char::from(self.raw_byte(v, 8 + index))
    } else {
        char::from_u32(self.raw_slot(v, index) as u32).expect("a string holds characters")
    }
}
```

`8 + index` skips the count word's eight bytes. `char::from` on a byte is
Rust's own Latin-1 conversion, the same fact about the first 256 code points
said in code. `aref`, `length`, `string_to_rust` and `string_is` (the
comparison the symbol table uses) all go through these two. `%set-aref` needed
one change: it used to refuse `ConstantString`, and now refuses any string that
is not the writable kind.

Who decides which layout a string gets? The allocator, and only for constants:

```rust
if kind == ObjectType::ConstantString && chars.iter().all(|&ch| (ch as u32) < 256) {
    let v = self.alloc(ObjectType::PackedString, 1 + chars.len().div_ceil(8));
    ...
```

From Lisp nothing is visible. Both constant layouts answer `stringp`, both print
the same, both refuse a write. A program has no way to tell whether a literal
was packed, which is what makes this a **representation** choice rather than a
language change.

## What it cost, and what it bought

| | bytes | words |
| --- | --- | --- |
| after packages (#43) | 478,528 | 59,769 |
| `symbol-name` stops copying | 350,944 | 43,821 |
| constants packed | **253,912** | **31,692** |

The image is **47% smaller** than when #44 was opened. Strings went from
17,509 words to about 5,400.

Compressed with gzip, the same three images are 69 KB, 57 KB and 54 KB.
Compression had already removed most of the waste in the *file*: a word that is
mostly zero bits compresses very well. That is why "compress the image" was a
real option in #44, and why it lost anyway. Compression helps the download and
does nothing for the heap the running system lives in. The reader and printer
in Lisp are about to create strings constantly, and those strings live in
memory, not in a file.

The Rust cost was 70 lines, most of them comments: a new object type, two
accessors, a branch in the allocator, the raw-word reader a heap report uses,
and the image format number. The format went to **3**, because an image with
type-68 objects would be nonsense to a version-2 reader. Nothing else changed:
the collector copies a packed string as it copies any raw object, by its header's
word count, without knowing what is inside.

What it gave up: writable strings are still eight bytes a character. A
`make-string` of a megabyte of text is eight megabytes. That is the price of
never having to widen. If something measures that it matters, the next step is
the UTF-32 layout for writable strings, which needs no new rule because
UTF-32 holds any character.

---

**References.** The Unicode Standard, chapter 3, on code points and scalar
values. ISO/IEC 8859-1 for Latin-1. CLHS chapter 13, *Characters*, and its
`base-char` and `base-string` entries for Common Lisp's split between base
characters and the rest. SBCL's
`base-string` is the implementation of that split. For UTF-8 with indexes
added back, see PyPy's 2019 move to UTF-8 strings and the Swift 5 string
redesign, both written up by their implementers.
