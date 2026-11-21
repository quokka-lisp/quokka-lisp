//! The reader: text in, heap objects out.
//!
//! In most languages this is a *parser*, and it is a large, fiddly thing that
//! has to know the whole grammar. In Lisp it is small, because there is almost
//! no grammar to know. `(* 6 7)` is not special syntax for multiplication — it
//! is a list of three things, built from the same cons cells as any other list.
//! The reader produces data; working out that the data means multiplication is
//! the evaluator's job, and it happens later.
//!
//! That is why Lisp systems are built reader-first: the output is ordinary
//! structure, so everything downstream shares one representation.
//!
//! # What it handles
//!
//! Tier 0 (see `docs/design.md`): lists, dotted pairs, symbols, fixnums,
//! strings, characters, `quote`, and both comment forms. Not yet: `#(`
//! vectors and backquote, which need no new machinery and are better written
//! once there is an evaluator to exercise them.
//!
//! # Depth, and why there is a limit
//!
//! This reader is recursive: reading a list calls itself for each element.
//! That is the clear way to write it and it has a failure mode — input like
//! `((((((...))))))` recurses once per level and overflows the Rust stack,
//! which is not an error but a dead process.
//!
//! It matters more than it looks. The input is attacker-controlled the moment a
//! file comes from anywhere but us, and the wasm32 stack is about a megabyte
//! against eight for a native thread, so the browser dies first and on input
//! that was fine on the development machine.
//!
//! So depth is counted and [`MAX_DEPTH`] is an error, not a crash. The
//! alternative — an explicit stack, as the evaluator must use — is better and
//! is not worth building here, because this reader is temporary: tier 1 moves
//! the reader into Lisp and this code is deleted.
//!
//! # Allocation
//!
//! Reading allocates — every list is cons cells. It is therefore the canonical
//! place for the bug described in [`crate::heap`], where a value held across an
//! allocation is reclaimed. It cannot happen here: everything runs inside one
//! `mutate`, so no collection can intervene. That is the whole point of the
//! arrangement, and it is why this module contains no rooting code at all.

//! Everything the reader produces is a `Value<'h>` branded to the mutation it
//! was built in, and the methods are generic over that brand rather than
//! returning anything free of it. An earlier draft had a helper that converted
//! to `Value<'static>` so the signatures would be shorter; that is precisely the
//! escape [`crate::heap`] exists to prevent, and the compiler refused it.
//! Threading the brand is not a workaround — it is correct, because doing
//! anything with a form needs the heap anyway.

use crate::heap::Mutation;
use crate::value::Value;
use core::fmt;

/// How deeply lists may nest before the reader refuses.
///
/// Generous for real code — Lisp rarely nests past twenty — and far below what
/// would trouble even the wasm32 stack.
pub const MAX_DEPTH: usize = 200;

/// Why reading failed.
///
/// These become Lisp conditions once there is a condition system. Until then
/// they are values, and specifically not panics: malformed input is an ordinary
/// event, and a REPL that dies on a stray bracket is not a REPL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// Input ended in the middle of something.
    UnexpectedEnd {
        /// What was being read.
        what: &'static str,
    },
    /// A `)` with no `(`.
    UnbalancedClose {
        /// Where, as a character offset.
        at: usize,
    },
    /// A dot that is not between two things in a list.
    MisplacedDot {
        /// Where, as a character offset.
        at: usize,
    },
    /// Nesting passed [`MAX_DEPTH`].
    TooDeep {
        /// Where, as a character offset.
        at: usize,
    },
    /// A character that cannot start a form.
    UnexpectedCharacter {
        /// The character.
        ch: char,
        /// Where, as a character offset.
        at: usize,
    },
    /// A `#\` name that is not a character.
    UnknownCharacterName {
        /// What was written after `#\`.
        name: String,
    },
    /// A `:` this reader cannot make sense of: a bare `:`, a `::`, or
    /// `package:symbol`, none of which mean anything until packages exist.
    PackageMarker {
        /// The whole token, as written.
        text: String,
    },
    /// A comma with no backquote around it.
    CommaOutsideBackquote {
        /// Where, as a character offset.
        at: usize,
    },
    /// An integer too large for a fixnum. Bignums are not implemented.
    IntegerTooLarge {
        /// The text of the number.
        text: String,
    },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::UnexpectedEnd { what } => write!(f, "input ended inside {what}"),
            ReadError::UnbalancedClose { at } => write!(f, "unmatched `)` at {at}"),
            ReadError::MisplacedDot { at } => write!(f, "misplaced `.` at {at}"),
            ReadError::TooDeep { at } => {
                write!(f, "nested deeper than {MAX_DEPTH} at {at}")
            }
            ReadError::UnexpectedCharacter { ch, at } => {
                write!(f, "unexpected `{ch}` at {at}")
            }
            ReadError::PackageMarker { text } => {
                write!(
                    f,
                    "`{text}` uses a package marker, and packages are phase 2; \
                     `:name` is a keyword and is the only colon this reader knows"
                )
            }
            ReadError::CommaOutsideBackquote { at } => {
                write!(f, "`,` at {at} is not inside a backquote")
            }
            ReadError::UnknownCharacterName { name } => {
                write!(f, "unknown character name `#\\{name}`")
            }
            ReadError::IntegerTooLarge { text } => {
                write!(
                    f,
                    "`{text}` does not fit in a fixnum, and bignums do not exist yet"
                )
            }
        }
    }
}

/// Reads forms from a string.
///
/// Holds a position, nothing more. Several forms can be read from one source by
/// calling [`Reader::read`] until it returns `None`.
pub struct Reader<'t> {
    text: &'t [char],
    at: usize,
    /// How many backquotes are open. A comma outside one is an error rather
    /// than a symbol, which is the only reason this is tracked.
    depth: usize,
}

impl<'t> Reader<'t> {
    /// Prepare to read from `chars`.
    ///
    /// Takes characters rather than a `&str` so positions are character offsets
    /// rather than byte offsets, which is what a person reporting an error
    /// counts. Use [`Reader::chars_of`] to make one.
    pub fn new(chars: &'t [char]) -> Reader<'t> {
        Reader {
            text: chars,
            at: 0,
            depth: 0,
        }
    }

    /// Turn text into what [`Reader::new`] wants.
    pub fn chars_of(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    /// How far through the input we are, in characters.
    pub fn position(&self) -> usize {
        self.at
    }

    /// Read one form, or `None` if only whitespace and comments remain.
    pub fn read<'h>(&mut self, mc: &mut Mutation<'h>) -> Result<Option<Value<'h>>, ReadError> {
        self.skip_blanks()?;
        if self.peek().is_none() {
            return Ok(None);
        }
        self.read_form(mc, 0).map(Some)
    }

    // ---- the grammar, such as it is --------------------------------------

    fn read_form<'h>(
        &mut self,
        mc: &mut Mutation<'h>,
        depth: usize,
    ) -> Result<Value<'h>, ReadError> {
        self.skip_blanks()?;
        let at = self.at;
        let Some(ch) = self.peek() else {
            return Err(ReadError::UnexpectedEnd { what: "a form" });
        };

        match ch {
            '(' => {
                self.at += 1;
                self.read_list(mc, depth + 1)
            }
            ')' => Err(ReadError::UnbalancedClose { at }),
            '"' => {
                self.at += 1;
                self.read_string(mc)
            }
            '\'' => {
                self.at += 1;
                let quoted = self.read_form(mc, depth + 1)?;
                // 'x reads as (quote x). The reader does not know what quote
                // means; it only builds the list the evaluator will recognise.
                let quote = mc.intern("QUOTE");
                let tail = mc.cons(quoted, Value::NIL);
                Ok(mc.cons(quote, tail))
            }
            // `(a ,b ,@c) is a template. The reader expands it here, into
            // calls to cons and append, so the evaluator never hears of it —
            // which is what Common Lisp implementations do, and keeps
            // quasiquotation out of the twenty-five special operators.
            '`' => {
                self.at += 1;
                self.depth += 1;
                let template = self.read_form(mc, depth + 1);
                self.depth -= 1;
                let template = template?;
                Ok(self.quasiquote(mc, template))
            }
            ',' if self.depth == 0 => Err(ReadError::CommaOutsideBackquote { at }),
            ',' => {
                self.at += 1;
                let splicing = self.peek() == Some('@');
                if splicing {
                    self.at += 1;
                }
                let inner = self.read_form(mc, depth + 1)?;
                // A marker, consumed by `quasiquote` a few frames up and never
                // seen by anything else.
                //
                // The names contain a space on purpose. This reader has no
                // `|bar quoting|`, so a space ends a token and *no input can
                // name these symbols*. A marker a user could write by hand —
                // `%UNQUOTE`, say — would make `(a (%unquote b)) expand as
                // though a comma had been typed. Unlikely, but wrong, and the
                // fix costs one character.
                let tag = mc.intern(if splicing {
                    "SPLICE MARKER"
                } else {
                    "UNQUOTE MARKER"
                });
                let tail = mc.cons(inner, Value::NIL);
                Ok(mc.cons(tag, tail))
            }
            '#' => {
                self.at += 1;
                self.read_hash(mc, at)
            }
            _ => self.read_atom(mc),
        }
    }

    fn read_list<'h>(
        &mut self,
        mc: &mut Mutation<'h>,
        depth: usize,
    ) -> Result<Value<'h>, ReadError> {
        if depth > MAX_DEPTH {
            return Err(ReadError::TooDeep { at: self.at });
        }

        // Collected first, then built back to front, so the list comes out in
        // the order it was written without needing to walk it twice.
        let mut items: Vec<Value<'h>> = Vec::new();
        let mut tail = Value::NIL;

        loop {
            self.skip_blanks()?;
            let at = self.at;
            match self.peek() {
                None => return Err(ReadError::UnexpectedEnd { what: "a list" }),
                Some(')') => {
                    self.at += 1;
                    break;
                }
                Some('.') if self.dot_is_a_dot() => {
                    self.at += 1;
                    if items.is_empty() {
                        return Err(ReadError::MisplacedDot { at });
                    }
                    tail = self.read_form(mc, depth)?;
                    self.skip_blanks()?;
                    match self.peek() {
                        Some(')') => self.at += 1,
                        None => {
                            return Err(ReadError::UnexpectedEnd {
                                what: "a dotted list",
                            })
                        }
                        Some(_) => return Err(ReadError::MisplacedDot { at: self.at }),
                    }
                    break;
                }
                Some(_) => items.push(self.read_form(mc, depth)?),
            }
        }

        let mut list = tail;
        for item in items.into_iter().rev() {
            list = mc.cons(item, list);
        }
        Ok(list)
    }

    /// A `.` is the dotted-pair marker only when it stands alone. `1.5` and
    /// `foo.bar` are atoms, and `...` is a symbol.
    fn dot_is_a_dot(&self) -> bool {
        match self.text.get(self.at + 1) {
            None => true,
            Some(ch) => is_delimiter(*ch),
        }
    }

    fn read_string<'h>(&mut self, mc: &mut Mutation<'h>) -> Result<Value<'h>, ReadError> {
        let mut out = String::new();
        loop {
            let Some(ch) = self.peek() else {
                return Err(ReadError::UnexpectedEnd { what: "a string" });
            };
            self.at += 1;
            match ch {
                '"' => return Ok(mc.alloc_constant_string(&out)),
                // Common Lisp strings have exactly two escapes: \" and \\.
                // There is no \n — a newline in a string is a newline.
                '\\' => {
                    let Some(next) = self.peek() else {
                        return Err(ReadError::UnexpectedEnd {
                            what: "a string escape",
                        });
                    };
                    self.at += 1;
                    out.push(next);
                }
                _ => out.push(ch),
            }
        }
    }

    fn read_hash<'h>(
        &mut self,
        mc: &mut Mutation<'h>,
        start: usize,
    ) -> Result<Value<'h>, ReadError> {
        match self.peek() {
            Some('\\') => {
                self.at += 1;
                self.read_character()
            }
            // `#'f` reads as `(function f)`, exactly as `'x` reads as
            // `(quote x)`. The reader does not know what either means.
            Some('\'') => {
                self.at += 1;
                let named = self.read_form(mc, 1)?;
                let function = mc.intern("FUNCTION");
                let tail = mc.cons(named, Value::NIL);
                Ok(mc.cons(function, tail))
            }
            Some('|') => {
                self.at += 1;
                self.skip_block_comment()?;
                self.read_form(mc, 0)
            }
            // `#(1 2 3)` is a vector literal. Read it as a list and copy the
            // elements across, which is the shortest honest route and costs
            // one list that is immediately garbage.
            Some('(') => {
                self.at += 1;
                let items = self.read_list(mc, 1)?;
                let mut count = 0usize;
                let mut cursor = items;
                while cursor.is_cons() {
                    count += 1;
                    cursor = mc.cdr(cursor);
                }
                if !cursor.is_nil() {
                    return Err(ReadError::MisplacedDot { at: start });
                }
                let v = mc.alloc_vector(count);
                let mut cursor = items;
                for i in 0..count {
                    let item = mc.car(cursor);
                    mc.set_slot(v, i, item);
                    cursor = mc.cdr(cursor);
                }
                Ok(v)
            }
            Some(ch) => Err(ReadError::UnexpectedCharacter { ch, at: self.at }),
            None => Err(ReadError::UnexpectedEnd { what: "a `#` form" }),
        }
        .map_err(|e| match e {
            ReadError::UnexpectedCharacter { ch, .. } => {
                ReadError::UnexpectedCharacter { ch, at: start }
            }
            other => other,
        })
    }

    fn read_character<'h>(&mut self) -> Result<Value<'h>, ReadError> {
        let Some(first) = self.peek() else {
            return Err(ReadError::UnexpectedEnd {
                what: "a character",
            });
        };
        self.at += 1;

        // `#\a` is one character; `#\space` is a name. The difference is
        // whether more non-delimiter characters follow.
        let mut name = String::new();
        name.push(first);
        if first.is_alphanumeric() {
            while let Some(ch) = self.peek() {
                if is_delimiter(ch) {
                    break;
                }
                name.push(ch);
                self.at += 1;
            }
        }

        if name.chars().count() == 1 {
            return Ok(Value::character(first));
        }
        let named = match name.to_ascii_lowercase().as_str() {
            "space" => ' ',
            "newline" | "linefeed" => '\n',
            "tab" => '\t',
            "return" => '\r',
            "page" => '\u{c}',
            "backspace" => '\u{8}',
            "rubout" | "delete" => '\u{7f}',
            "nul" | "null" => '\0',
            _ => return Err(ReadError::UnknownCharacterName { name }),
        };
        Ok(Value::character(named))
    }

    /// A run of non-delimiter characters: a number if it looks like one, a
    /// symbol otherwise. Symbols are upcased, as Common Lisp's reader does, so
    /// `foo`, `Foo` and `FOO` are one symbol.
    fn read_atom<'h>(&mut self, mc: &mut Mutation<'h>) -> Result<Value<'h>, ReadError> {
        let start = self.at;
        while let Some(ch) = self.peek() {
            if is_delimiter(ch) {
                break;
            }
            self.at += 1;
        }
        let text: String = self.text[start..self.at].iter().collect();

        if let Some(n) = parse_fixnum(&text) {
            if let Some(v) = Value::fixnum(n) {
                return Ok(v);
            }
        }

        // `:foo` is a keyword. The colon is a *package marker*, not part of
        // the name, so it is stripped here and put back by the printer —
        // `(symbol-name :foo)` is "FOO" in Common Lisp, not ":FOO".
        if let Some(rest) = text.strip_prefix(':') {
            if rest.is_empty() || rest.starts_with(':') {
                return Err(ReadError::PackageMarker { text });
            }
            return Ok(mc.intern_keyword(&rest.to_ascii_uppercase()));
        }
        // Any other colon means `package:symbol`, and packages are phase 2.
        // Making it part of an ordinary symbol name instead would read
        // without complaint and mean something different later, which is
        // worse than refusing it now.
        if text.contains(':') {
            return Err(ReadError::PackageMarker { text });
        }

        Ok(mc.intern(&text.to_ascii_uppercase()))
    }

    /// Turn a backquote template into the code that builds it.
    ///
    /// ```text
    /// `atom        =>  (quote atom)
    /// `,x          =>  x
    /// `(a . rest)  =>  (cons `a `rest)
    /// `(,@x . rest) => (append x `rest)
    /// ```
    ///
    /// Three rules and a recursion. The result is ordinary code — `cons`,
    /// `append` and `quote` — so nothing downstream knows a backquote was
    /// involved.
    ///
    /// Nested backquotes are not handled: the inner one expands first, which
    /// is wrong for `` `(a `(b ,,c)) `` and right for everything anyone writes
    /// in anger. Noted rather than hidden.
    fn quasiquote<'h>(&mut self, mc: &mut Mutation<'h>, form: Value<'h>) -> Value<'h> {
        if !form.is_cons() {
            // An atom stands for itself, which means quoting it.
            let quote = mc.intern("QUOTE");
            let tail = mc.cons(form, Value::NIL);
            return mc.cons(quote, tail);
        }

        let head = mc.car(form);
        if mc.symbol_is(head, "UNQUOTE MARKER") {
            return mc.car(mc.cdr(form));
        }

        let rest = self.quasiquote(mc, mc.cdr(form));
        if head.is_cons() && mc.symbol_is(mc.car(head), "SPLICE MARKER") {
            // ,@x contributes its *elements*, so the list is appended rather
            // than consed on.
            let spliced = mc.car(mc.cdr(head));
            let append = mc.intern("APPEND");
            let tail = mc.cons(rest, Value::NIL);
            let args = mc.cons(spliced, tail);
            return mc.cons(append, args);
        }
        let first = self.quasiquote(mc, head);
        let cons = mc.intern("CONS");
        let tail = mc.cons(rest, Value::NIL);
        let args = mc.cons(first, tail);
        mc.cons(cons, args)
    }

    // ---- whitespace and comments -----------------------------------------

    fn skip_blanks(&mut self) -> Result<(), ReadError> {
        loop {
            match self.peek() {
                Some(ch) if ch.is_whitespace() => self.at += 1,
                Some(';') => {
                    while let Some(ch) = self.peek() {
                        self.at += 1;
                        if ch == '\n' {
                            break;
                        }
                    }
                }
                Some('#') if self.text.get(self.at + 1) == Some(&'|') => {
                    self.at += 2;
                    self.skip_block_comment()?;
                }
                _ => return Ok(()),
            }
        }
    }

    /// `#| ... |#`, which nests, because Common Lisp says so.
    fn skip_block_comment(&mut self) -> Result<(), ReadError> {
        let mut depth = 1usize;
        while depth > 0 {
            let Some(ch) = self.peek() else {
                return Err(ReadError::UnexpectedEnd {
                    what: "a block comment",
                });
            };
            self.at += 1;
            match (ch, self.peek()) {
                ('|', Some('#')) => {
                    self.at += 1;
                    depth -= 1;
                }
                ('#', Some('|')) => {
                    self.at += 1;
                    depth += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn peek(&self) -> Option<char> {
        self.text.get(self.at).copied()
    }
}

fn is_delimiter(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '(' | ')' | '"' | '\'' | ';' | '`' | ',')
}

/// Is this text an integer, and if so which?
///
/// Deliberately strict: a lone `-` or `+` is the symbol, not a number, and
/// anything with a non-digit is a symbol. `1.5` is a symbol for now, because
/// floats are tier 1.
fn parse_fixnum(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}

#[cfg(test)]
#[path = "tests/reader.rs"]
mod tests;
