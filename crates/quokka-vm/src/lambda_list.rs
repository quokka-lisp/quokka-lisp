//! Lambda lists: turning what people write into something the machine binds.
//!
//! Common Lisp lets a parameter list say rather a lot:
//!
//! ```lisp
//! (defun draw (shape &optional (colour 'black) (width 1 width-given)
//!                    &key (fill nil) origin
//!                    &aux (area (* width width)))
//!   ...)
//! ```
//!
//! Three of those features need *evaluation* at call time — `'black`, `1` and
//! `(* width width)` are forms, not values, and CLHS says each is evaluated in
//! a scope containing every parameter to its left. So `(lambda (a &optional
//! (b (* a 2))) ...)` must see `a` when it computes `b`.
//!
//! # Why this is a rewrite and not a new machine state
//!
//! Binding happens in `eval::Machine::bind_lambda_list`, which is ordinary
//! Rust with no evaluator in reach. Calling the evaluator from inside it would
//! put Lisp evaluation back on the Rust call stack, which chapter 2 and rule 4
//! exist to prevent.
//!
//! The alternative is a new frame kind — a `BIND` state that evaluates each
//! default as the call sets up. That is more faithful to "the machine does
//! everything", and it is also a reimplementation of `let*`, which already
//! evaluates each init form in a scope containing the bindings before it.
//! That is precisely the rule CLHS states for default forms.
//!
//! So instead, *once, when the closure is made*, this module rewrites
//!
//! ```lisp
//! (lambda (a &optional (b (* a 2)) (c 0 c-given)) BODY)
//! ```
//!
//! into a simple lambda list plus a body that computes the rest:
//!
//! ```lisp
//! (lambda (a &optional b c)
//!   (let* ((b      (if (eq b '<unsupplied>) (* a 2) b))
//!          (c-given (if (eq c '<unsupplied>) nil t))
//!          (c      (if c-given c 0)))
//!     BODY))
//! ```
//!
//! Every feature becomes `let*` and `if`, which already work and are already
//! tested. The cost is that a lambda list with defaults allocates a wrapper
//! when the closure is made — once per `defun`, not once per call. A list of
//! plain required parameters, which is almost all of them, is left exactly as
//! it was.
//!
//! This is a *source transformation*, which is to say it is what a macro does.
//! It lives in Rust rather than in the prelude only because `lambda` is a
//! special operator, and `lambda` is a special operator because `function`
//! must be able to build a closure out of a lambda expression (chapter 16).
//!
//! # The unsupplied marker
//!
//! "No argument was given" has to be distinguishable from "nil was given",
//! because `(f)` and `(f nil)` must be able to differ. The marker is a symbol
//! interned under a name containing a space:
//!
//! ```text
//! UNSUPPLIED MARKER
//! ```
//!
//! The reader has no `|bar quoting|`, so a space ends a token and no input can
//! name this symbol. It is the same trick the backquote markers use
//! (chapter 15), for the same reason: a value the user cannot forge, obtained
//! by making it unwriteable rather than by hiding it.
//!
//! # What is not here
//!
//! Destructuring macro lambda lists — `(defmacro m ((a b) &body c) ...)` —
//! are not implemented. They apply only to `defmacro`, and they need this
//! rewrite to recurse into nested lists.
//!
//! `&environment` and `&whole` are also absent, and `:allow-other-keys t`
//! passed *as an argument* is not honoured; only the `&allow-other-keys`
//! declaration is.

use crate::eval::EvalError;
use crate::heap::Mutation;
use crate::value::Value;

/// The name of the symbol that means "no argument was supplied".
///
/// Contains a space, so no input can produce it. See the module docs.
pub const UNSUPPLIED: &str = "UNSUPPLIED MARKER";

/// Which kind of lambda list this is.
///
/// Only macros destructure. `(defun f ((a b)) ...)` is an error in Common Lisp
/// and stays one here: an ordinary function receives *values*, and a value has
/// no shape the lambda list could take apart. A macro receives *forms*, which
/// are lists, and taking them apart is the whole point.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `lambda`, `defun`, `flet`, `labels`.
    Ordinary,
    /// `defmacro`, which may destructure and may be dotted.
    Macro,
}

/// Which part of the lambda list we are reading.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Required,
    Optional,
    Rest,
    Key,
    Aux,
}

/// One `&optional`, `&key` or `&aux` entry, after its shorthand is unpacked.
struct Entry<'h> {
    /// The variable to bind.
    name: Value<'h>,
    /// The form to evaluate when nothing was supplied. `None` means nil.
    default: Option<Value<'h>>,
    /// A variable bound to whether an argument was supplied, if asked for.
    supplied: Option<Value<'h>>,
    /// For `&key`, the keyword that selects this parameter. Defaults to the
    /// parameter's own name as a keyword, which is what `&key b` means.
    keyword: Option<Value<'h>>,
}

fn bad(detail: &'static str) -> EvalError {
    EvalError::BadSyntax {
        operator: "a lambda list",
        detail,
    }
}

/// Rewrite a lambda list and body into a simple lambda list and a body that
/// computes everything the simple one cannot express.
///
/// The returned lambda list contains only bare symbols, `&optional`, `&rest`,
/// `&key` with `(keyword variable)` pairs, and `&allow-other-keys` —
/// everything `bind_lambda_list` knows how to do without evaluating anything.
pub fn rewrite<'h>(
    mc: &mut Mutation<'h>,
    params: Value<'h>,
    body: Value<'h>,
    kind: Kind,
) -> Result<(Value<'h>, Value<'h>), EvalError> {
    let mut section = Section::Required;
    let mut required: Vec<Value<'h>> = Vec::new();
    // Nested patterns, as (placeholder, pattern) pairs. The placeholder goes
    // into the lambda list and the pattern is taken apart in the `let*`.
    let mut patterns: Vec<(Value<'h>, Value<'h>)> = Vec::new();
    let mut optional: Vec<Entry<'h>> = Vec::new();
    let mut keys: Vec<Entry<'h>> = Vec::new();
    let mut aux: Vec<Entry<'h>> = Vec::new();
    let mut rest: Option<Value<'h>> = None;
    let mut allow_other_keys = false;
    let mut saw_key = false;
    let mut dotted = false;

    let mut cursor = params;
    while cursor.is_cons() {
        let item = mc.car(cursor);
        cursor = mc.cdr(cursor);

        if mc.is_symbol(item) {
            if mc.symbol_is(item, "&OPTIONAL") {
                section = Section::Optional;
                continue;
            }
            if mc.symbol_is(item, "&REST") || mc.symbol_is(item, "&BODY") {
                section = Section::Rest;
                continue;
            }
            if mc.symbol_is(item, "&KEY") {
                section = Section::Key;
                saw_key = true;
                continue;
            }
            if mc.symbol_is(item, "&ALLOW-OTHER-KEYS") {
                allow_other_keys = true;
                continue;
            }
            if mc.symbol_is(item, "&AUX") {
                section = Section::Aux;
                continue;
            }
        }

        match section {
            Section::Required => {
                if mc.is_symbol(item) {
                    required.push(item);
                } else if kind == Kind::Macro && item.is_cons() {
                    // A nested lambda list. The function gets a name it can
                    // bind, and the pattern is taken apart in the `let*`.
                    let placeholder = mc.gensym();
                    required.push(placeholder);
                    patterns.push((placeholder, item));
                } else {
                    return Err(bad("wants a symbol for a required parameter"));
                }
            }
            Section::Rest => {
                if !mc.is_symbol(item) {
                    return Err(bad("&rest wants a symbol"));
                }
                if rest.is_some() {
                    return Err(bad("more than one &rest parameter"));
                }
                rest = Some(item);
            }
            Section::Optional => optional.push(parse_entry(mc, item, false)?),
            Section::Key => keys.push(parse_entry(mc, item, true)?),
            Section::Aux => aux.push(parse_entry(mc, item, false)?),
        }
    }
    if !cursor.is_nil() {
        // `(defmacro m (a . b) ...)` — a dotted macro lambda list, where the
        // tail is `&rest` by another spelling. CLHS 3.4.4 allows it; an
        // ordinary lambda list may not be dotted.
        if kind == Kind::Macro && mc.is_symbol(cursor) && rest.is_none() {
            rest = Some(cursor);
            // The list has to be rebuilt as a proper one even if nothing else
            // needs rewriting, because the binder walks conses and would
            // never reach a dotted tail.
            dotted = true;
        } else {
            return Err(bad("a lambda list must be a proper list"));
        }
    }

    // The common case: nothing that needs evaluating, so nothing is rewritten
    // and the closure holds exactly what was written.
    if optional.is_empty() && keys.is_empty() && aux.is_empty() && patterns.is_empty() && !dotted {
        return Ok((params, body));
    }

    let marker = mc.intern(UNSUPPLIED);
    let quoted_marker = quote(mc, marker);

    // ---- the simple lambda list -----------------------------------------
    let mut simple: Vec<Value<'h>> = required.clone();
    if !optional.is_empty() {
        simple.push(mc.intern("&OPTIONAL"));
        for e in &optional {
            simple.push(e.name);
        }
    }
    if let Some(r) = rest {
        simple.push(mc.intern("&REST"));
        simple.push(r);
    }
    if saw_key {
        simple.push(mc.intern("&KEY"));
        for e in &keys {
            let keyword = match e.keyword {
                Some(k) => k,
                None => {
                    let name = mc.symbol_name(e.name);
                    mc.intern_keyword(&name)
                }
            };
            let pair = list2(mc, keyword, e.name);
            simple.push(pair);
        }
        if allow_other_keys {
            simple.push(mc.intern("&ALLOW-OTHER-KEYS"));
        }
    }
    let simple = list_of(mc, &simple);

    // ---- the let* that fills in what is missing --------------------------
    //
    // Order matters and is the standard's: each default sees every parameter
    // to its left, which is exactly what `let*` provides.
    let eq = mc.intern("EQ");
    let if_ = mc.intern("IF");
    let mut bindings: Vec<Value<'h>> = Vec::new();

    // Patterns first: a required parameter is bound before any default form
    // runs, so a default may refer to a name a pattern introduced.
    for (placeholder, pattern) in &patterns {
        destructure(mc, *pattern, *placeholder, &mut bindings)?;
    }
    for e in optional.iter().chain(keys.iter()) {
        let default = e.default.unwrap_or(Value::NIL);
        let test = list3(mc, eq, e.name, quoted_marker);
        match e.supplied {
            // (given (if (eq v '<marker>) nil t))
            // (v     (if given v DEFAULT))
            Some(given) => {
                let is_given = list_of(mc, &[if_, test, Value::NIL, Value::T]);
                let b = list2(mc, given, is_given);
                bindings.push(b);
                let pick = list_of(mc, &[if_, given, e.name, default]);
                let b = list2(mc, e.name, pick);
                bindings.push(b);
            }
            // (v (if (eq v '<marker>) DEFAULT v))
            None => {
                let pick = list_of(mc, &[if_, test, default, e.name]);
                let b = list2(mc, e.name, pick);
                bindings.push(b);
            }
        }
    }
    // &aux is not a parameter at all — it is a `let*` that someone wrote
    // inside the lambda list. Here it becomes one.
    for e in &aux {
        let default = e.default.unwrap_or(Value::NIL);
        let b = list2(mc, e.name, default);
        bindings.push(b);
    }

    // A dotted lambda list and nothing else needs a new parameter list but no
    // bindings at all, and `(let* () ...)` around every such macro would be
    // noise in every expansion anyone ever printed.
    if bindings.is_empty() {
        return Ok((simple, body));
    }

    let bindings = list_of(mc, &bindings);
    let let_star = mc.intern("LET*");
    let form = mc.cons(bindings, body);
    let form = mc.cons(let_star, form);
    let new_body = mc.cons(form, Value::NIL);

    Ok((simple, new_body))
}

/// Take a nested pattern apart, appending `let*` bindings that bind every
/// name in it from the value of `source`.
///
/// The generated code is `car`, `cdr`, `if` and `null` — nothing that did not
/// already exist. A pattern like
///
/// ```lisp
/// (var name &optional (mode 'read))
/// ```
///
/// matched against a cursor `g` becomes, roughly:
///
/// ```lisp
/// (c    g)
/// (var  (car c))   (c (cdr c))
/// (name (car c))   (c (cdr c))
/// (mode (if (null c) 'read (car c)))
/// (c    (if (null c) nil (cdr c)))
/// ```
///
/// The cursor is rebound rather than advanced, which `let*` allows and which
/// keeps the whole thing to one list of bindings. Nested patterns recurse,
/// taking a fresh cursor of their own.
///
/// **Arity is not checked.** `(with-thing (h) ...)` against the pattern
/// `(var name)` binds `name` to nil rather than complaining, where a real
/// Common Lisp signals. Checking would mean emitting a test per position;
/// worth doing, and worth doing *after* there is a condition system to signal
/// into (group E of #18).
fn destructure<'h>(
    mc: &mut Mutation<'h>,
    pattern: Value<'h>,
    source: Value<'h>,
    bindings: &mut Vec<Value<'h>>,
) -> Result<(), EvalError> {
    let car = mc.intern("CAR");
    let cdr = mc.intern("CDR");
    let null = mc.intern("NULL");
    let if_ = mc.intern("IF");

    let cursor = mc.gensym();
    let b = list2(mc, cursor, source);
    bindings.push(b);

    let mut optional = false;
    let mut item = pattern;
    while item.is_cons() {
        let entry = mc.car(item);
        item = mc.cdr(item);

        if mc.is_symbol(entry) {
            if mc.symbol_is(entry, "&OPTIONAL") {
                optional = true;
                continue;
            }
            if mc.symbol_is(entry, "&REST") || mc.symbol_is(entry, "&BODY") {
                if !item.is_cons() {
                    return Err(bad("&rest wants a name after it"));
                }
                let name = mc.car(item);
                item = mc.cdr(item);
                bind_here(mc, name, cursor, bindings)?;
                return finish(mc, item, cursor, bindings);
            }
            if mc.symbol_is(entry, "&KEY") || mc.symbol_is(entry, "&AUX") {
                // Keyword and aux parameters inside a *nested* pattern would
                // need keyword scanning in generated Lisp. Refused rather
                // than half-done; the top level of a macro lambda list
                // supports both.
                return Err(bad(
                    "&key and &aux are not supported inside a nested pattern",
                ));
            }
        }

        if optional {
            let e = parse_entry(mc, entry, false)?;
            let default = e.default.unwrap_or(Value::NIL);
            let empty = list2(mc, null, cursor);
            if let Some(given) = e.supplied {
                let is_given = list_of(mc, &[if_, empty, Value::NIL, Value::T]);
                let b = list2(mc, given, is_given);
                bindings.push(b);
            }
            let take = list2(mc, car, cursor);
            let pick = list_of(mc, &[if_, empty, default, take]);
            let b = list2(mc, e.name, pick);
            bindings.push(b);
            let step = list2(mc, cdr, cursor);
            let advance = list_of(mc, &[if_, empty, Value::NIL, step]);
            let b = list2(mc, cursor, advance);
            bindings.push(b);
            continue;
        }

        // A required position: take the head, then move on.
        let take = list2(mc, car, cursor);
        bind_here(mc, entry, take, bindings)?;
        let step = list2(mc, cdr, cursor);
        let b = list2(mc, cursor, step);
        bindings.push(b);
    }

    finish(mc, item, cursor, bindings)
}

/// Bind one pattern element — a name, or a nested pattern — to an expression.
fn bind_here<'h>(
    mc: &mut Mutation<'h>,
    target: Value<'h>,
    source: Value<'h>,
    bindings: &mut Vec<Value<'h>>,
) -> Result<(), EvalError> {
    if mc.is_symbol(target) {
        let b = list2(mc, target, source);
        bindings.push(b);
        return Ok(());
    }
    if target.is_cons() {
        return destructure(mc, target, source, bindings);
    }
    Err(bad(
        "a pattern element must be a symbol or a nested pattern",
    ))
}

/// A dotted pattern tail — `(a . rest)` — binds the rest of the list.
fn finish<'h>(
    mc: &mut Mutation<'h>,
    tail: Value<'h>,
    cursor: Value<'h>,
    bindings: &mut Vec<Value<'h>>,
) -> Result<(), EvalError> {
    if tail.is_nil() {
        return Ok(());
    }
    if mc.is_symbol(tail) {
        let b = list2(mc, tail, cursor);
        bindings.push(b);
        return Ok(());
    }
    Err(bad("a pattern tail must be a symbol"))
}

/// `name`, `(name default)`, `(name default supplied-p)`, or for `&key` also
/// `((:keyword name) default ...)`.
fn parse_entry<'h>(
    mc: &mut Mutation<'h>,
    item: Value<'h>,
    is_key: bool,
) -> Result<Entry<'h>, EvalError> {
    if mc.is_symbol(item) {
        return Ok(Entry {
            name: item,
            default: None,
            supplied: None,
            keyword: None,
        });
    }
    if !item.is_cons() {
        return Err(bad("wants a symbol or a (name default) list"));
    }

    let head = mc.car(item);
    let (name, keyword) = if is_key && head.is_cons() {
        // ((:outside inside) default)
        let keyword = mc.car(head);
        let inner = mc.cdr(head);
        if !inner.is_cons() || !mc.is_symbol(mc.car(inner)) {
            return Err(bad("&key wants (keyword variable) when naming both"));
        }
        (mc.car(inner), Some(keyword))
    } else {
        if !mc.is_symbol(head) {
            return Err(bad("wants a symbol for a parameter name"));
        }
        (head, None)
    };

    let after_name = mc.cdr(item);
    let default = after_name.is_cons().then(|| mc.car(after_name));
    let after_default = if after_name.is_cons() {
        mc.cdr(after_name)
    } else {
        Value::NIL
    };
    let supplied = if after_default.is_cons() {
        let s = mc.car(after_default);
        if !mc.is_symbol(s) {
            return Err(bad("the supplied-p parameter must be a symbol"));
        }
        Some(s)
    } else {
        None
    };

    Ok(Entry {
        name,
        default,
        supplied,
        keyword,
    })
}

// ---- small list builders -------------------------------------------------

fn quote<'h>(mc: &mut Mutation<'h>, v: Value<'h>) -> Value<'h> {
    let q = mc.intern("QUOTE");
    list2(mc, q, v)
}

fn list2<'h>(mc: &mut Mutation<'h>, a: Value<'h>, b: Value<'h>) -> Value<'h> {
    let tail = mc.cons(b, Value::NIL);
    mc.cons(a, tail)
}

fn list3<'h>(mc: &mut Mutation<'h>, a: Value<'h>, b: Value<'h>, c: Value<'h>) -> Value<'h> {
    let tail = list2(mc, b, c);
    mc.cons(a, tail)
}

fn list_of<'h>(mc: &mut Mutation<'h>, items: &[Value<'h>]) -> Value<'h> {
    let mut out = Value::NIL;
    for item in items.iter().rev() {
        out = mc.cons(*item, out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::Heap;
    use crate::printer::print;
    use crate::reader::Reader;

    /// Read `(params . body)` and show what the rewrite made of it.
    fn rewritten(source: &str) -> String {
        let chars = Reader::chars_of(source);
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            let params = mc.car(form);
            let body = mc.cdr(form);
            let (params, body) = rewrite(mc, params, body, Kind::Ordinary).expect("rewrites");
            let whole = mc.cons(params, body);
            print(mc, whole)
        })
    }

    /// The common case must cost nothing: a list of required parameters comes
    /// back untouched, body and all.
    #[test]
    fn a_simple_lambda_list_is_left_alone() {
        assert_eq!(rewritten("((a b) (+ a b))"), "((A B) (+ A B))");
        assert_eq!(rewritten("(() 1)"), "(NIL 1)");
        assert_eq!(rewritten("((a &rest r) r)"), "((A &REST R) R)");
    }

    /// A default becomes a `let*` binding, and the parameter list left behind
    /// is one `bind_lambda_list` can match without evaluating anything.
    #[test]
    fn a_default_becomes_a_let_star() {
        assert_eq!(
            rewritten("((a &optional (b 10)) b)"),
            "((A &OPTIONAL B) (LET* ((B (IF (EQ B (QUOTE UNSUPPLIED MARKER)) 10 B))) B))"
        );
    }

    /// `&aux` is not a parameter at all; it leaves the lambda list entirely.
    #[test]
    fn aux_leaves_the_lambda_list() {
        assert_eq!(
            rewritten("((a &aux (d (* a a))) d)"),
            "((A) (LET* ((D (* A A))) D))"
        );
    }

    /// `&key b` means the keyword `:b`, and the pair is made explicit so the
    /// binder never has to turn a name into a keyword at call time.
    #[test]
    fn key_parameters_carry_their_keyword() {
        assert_eq!(
            rewritten("((&key b) b)"),
            "((&KEY (:B B)) (LET* ((B (IF (EQ B (QUOTE UNSUPPLIED MARKER)) NIL B))) B))"
        );
    }

    fn rewritten_macro(source: &str) -> String {
        let chars = Reader::chars_of(source);
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            let params = mc.car(form);
            let body = mc.cdr(form);
            let (params, body) = rewrite(mc, params, body, Kind::Macro).expect("rewrites");
            let whole = mc.cons(params, body);
            print(mc, whole)
        })
    }

    /// Only macros destructure. A function receives values, and a value has
    /// no shape to take apart.
    #[test]
    fn only_macros_destructure() {
        let chars = Reader::chars_of("(((a b)) a)");
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            let params = mc.car(form);
            let body = mc.cdr(form);
            assert!(rewrite(mc, params, body, Kind::Ordinary).is_err());
            assert!(rewrite(mc, params, body, Kind::Macro).is_ok());
        });
    }

    /// A nested pattern leaves a placeholder in the lambda list and becomes
    /// `car`/`cdr` walking in the body. The gensym names are not pinned —
    /// only the shape is, which is what matters.
    #[test]
    fn a_pattern_becomes_car_and_cdr() {
        let out = rewritten_macro("(((var name) &body body) var)");
        assert!(out.contains("LET*"), "{out}");
        assert!(out.contains("(CAR "), "{out}");
        assert!(out.contains("(CDR "), "{out}");
        assert!(out.contains("(VAR (CAR "), "{out}");
        assert!(out.contains("(NAME (CAR "), "{out}");
        // &body is still a plain parameter, not part of the pattern.
        assert!(
            out.contains("&REST BODY") || out.contains("&BODY BODY"),
            "{out}"
        );
    }

    /// `(defmacro m (a . b) ...)` — the tail is `&rest` by another spelling,
    /// and the list must be rebuilt as a proper one or the binder, which
    /// walks conses, never reaches it.
    #[test]
    fn a_dotted_macro_lambda_list_becomes_rest() {
        assert_eq!(rewritten_macro("((a . b) a)"), "((A &REST B) A)");
    }

    /// Nothing to rewrite means nothing is rewritten, macro or not.
    #[test]
    fn a_macro_lambda_list_with_no_patterns_is_left_alone() {
        assert_eq!(rewritten_macro("((a b) a)"), "((A B) A)");
    }

    #[test]
    fn a_malformed_lambda_list_is_an_error() {
        let chars = Reader::chars_of("((a . b) 1)");
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            let params = mc.car(form);
            let body = mc.cdr(form);
            assert!(rewrite(mc, params, body, Kind::Ordinary).is_err());
        });
    }
}
