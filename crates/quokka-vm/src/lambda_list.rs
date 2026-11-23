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
//! Binding happens in [`bind`] below, which is ordinary
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
//! `&environment` and `&whole` are also absent.

use crate::error::EvalError;
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
/// everything [`bind`] knows how to do without evaluating anything.
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
/// **Arity is checked**, which it was not at first: a required position
/// emits `(if (consp c) (car c) (%destructuring-too-short))` rather than a
/// bare `(car c)`, and a pattern that ends emits a check that nothing is
/// left. Both were deliberately missing until there was a condition system
/// to signal into — emitting a check with nothing to signal would have meant
/// inventing an error mechanism to throw away.
fn destructure<'h>(
    mc: &mut Mutation<'h>,
    pattern: Value<'h>,
    source: Value<'h>,
    bindings: &mut Vec<Value<'h>>,
) -> Result<(), EvalError> {
    let car = mc.intern("CAR");
    let cdr = mc.intern("CDR");
    let null = mc.intern("NULL");
    let consp = mc.intern("CONSP");
    let if_ = mc.intern("IF");
    // A prelude function, called only when a call does not fit its pattern.
    let too_short = mc.intern("%DESTRUCTURING-TOO-SHORT");

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
                // `&rest` swallows whatever is left, so nothing is too long.
                return finish(mc, item, cursor, bindings, false);
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

        // A required position: take the head, then move on. The `consp`
        // test is the arity check — a pattern of two against a call of one
        // signals rather than binding nil.
        let have = list2(mc, consp, cursor);
        let short = mc.cons(too_short, Value::NIL);
        let take = list2(mc, car, cursor);
        let take = list_of(mc, &[if_, have, take, short]);
        bind_here(mc, entry, take, bindings)?;
        let step = list2(mc, cdr, cursor);
        let b = list2(mc, cursor, step);
        bindings.push(b);
    }

    // Anything still in the cursor is an argument the pattern did not ask
    // for, unless an `&optional` or `&rest` already took the tail.
    finish(mc, item, cursor, bindings, !optional)
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
    check_exhausted: bool,
) -> Result<(), EvalError> {
    if tail.is_nil() {
        if check_exhausted {
            let null = mc.intern("NULL");
            let if_ = mc.intern("IF");
            let too_long = mc.intern("%DESTRUCTURING-TOO-LONG");
            let empty = list2(mc, null, cursor);
            let complain = mc.cons(too_long, Value::NIL);
            let check = list_of(mc, &[if_, empty, Value::NIL, complain]);
            let ignored = mc.gensym();
            let b = list2(mc, ignored, check);
            bindings.push(b);
        }
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

// ---------------------------------------------------------------------------
// Binding, which is the other half of a lambda list.
//
// `rewrite` above turns a lambda list into a simple one at closure-creation
// time; this matches arguments against that simple form at call time. It
// lives here rather than in `crate::eval` because **both machines need it**:
// the interpreter when it applies a closure, and the bytecode VM when it
// executes a CALL. They never run at the same time (#25), but they bind
// parameters identically, and a second implementation would be a second
// chance to disagree about `&key`.
// ---------------------------------------------------------------------------

/// The names a simple lambda list binds, in the order [`bind`] binds them.
///
/// The compiler needs this and must not work it out for itself. Its idea of a
/// scope has to match the one the machine builds *slot for slot*, and a
/// second traversal of the same syntax is a second chance to disagree — about
/// where `&rest` goes, or whether a `&key` pair binds its keyword or its
/// variable. `the_names_match_what_bind_binds` checks the two against each
/// other rather than trusting that they read alike.
pub fn names<'h>(mc: &Mutation<'h>, params: Value<'h>) -> Result<Vec<Value<'h>>, EvalError> {
    let mut out = Vec::new();
    let mut cursor = params;
    let mut section = 0u8; // 0 required/optional, 2 rest, 3 key
    while cursor.is_cons() {
        let item = mc.car(cursor);
        cursor = mc.cdr(cursor);
        if mc.is_symbol(item) {
            if mc.symbol_is(item, "&OPTIONAL") {
                continue;
            }
            if mc.symbol_is(item, "&REST") || mc.symbol_is(item, "&BODY") {
                section = 2;
                continue;
            }
            if mc.symbol_is(item, "&KEY") {
                section = 3;
                continue;
            }
            if mc.symbol_is(item, "&ALLOW-OTHER-KEYS") {
                continue;
            }
            out.push(item);
            if section == 2 {
                // `&rest` takes exactly one name and then the list is over as
                // far as positional binding goes.
                section = 0;
            }
            continue;
        }
        if section == 3 && item.is_cons() {
            // `(keyword variable)`, and it is the *variable* that is bound.
            let after = mc.cdr(item);
            if !after.is_cons() {
                return Err(bad("&key wants (keyword variable) pairs here"));
            }
            out.push(mc.car(after));
            continue;
        }
        return Err(bad("a simple lambda list wants symbols here"));
    }
    Ok(out)
}

/// Match arguments to a **simple** lambda list: bare symbols, `&optional`,
/// `&rest`, and `&key` as `(keyword variable)` pairs.
///
/// Nothing here evaluates anything, which is the point —
/// [`crate::lambda_list::rewrite`] has already turned every default form
/// into a `let*` around the body, so this only has to decide *which* value
/// each name gets, never *compute* one.
///
/// A parameter with no argument is bound to the unsupplied marker rather
/// than to nil, because `(f)` and `(f nil)` have to be able to differ. The
/// `let*` upstream turns the marker into the default.
pub fn bind<'h>(
    mc: &mut Mutation<'h>,
    params: Value<'h>,
    arguments: Value<'h>,
) -> Result<(Vec<Value<'h>>, Vec<Value<'h>>), EvalError> {
    let mut names = Vec::new();
    let mut values = Vec::new();
    let mut param = params;
    let mut arg = arguments;
    let mut required = 0usize;
    let mut supplied = 0usize;
    let mut section = 0u8; // 0 required, 1 optional, 2 rest, 3 key
                           // `&rest` swallows everything left, so leftover arguments stop being
                           // an error once one has been bound. `&key` still scans the same tail,
                           // which is what CLHS says the two do together.
    let mut has_rest = false;
    let mut keys: Vec<(Value<'h>, Value<'h>)> = Vec::new();
    let mut allow_other_keys = false;
    let marker = mc.cached_symbol(
        crate::heap::symbol_cache::UNSUPPLIED,
        crate::lambda_list::UNSUPPLIED,
    );

    while param.is_cons() {
        let name = mc.car(param);
        param = mc.cdr(param);

        if mc.is_symbol(name) {
            if mc.symbol_is(name, "&OPTIONAL") {
                section = 1;
                continue;
            }
            if mc.symbol_is(name, "&REST") || mc.symbol_is(name, "&BODY") {
                let Some(rest_name) = (param.is_cons()).then(|| mc.car(param)) else {
                    return Err(EvalError::BadSyntax {
                        operator: "a lambda list",
                        detail: "&rest wants a name after it",
                    });
                };
                param = mc.cdr(param);
                names.push(rest_name);
                values.push(arg);
                has_rest = true;
                section = 2;
                continue;
            }
            if mc.symbol_is(name, "&KEY") {
                section = 3;
                continue;
            }
            if mc.symbol_is(name, "&ALLOW-OTHER-KEYS") {
                allow_other_keys = true;
                continue;
            }
        }

        if section == 3 {
            // `(keyword variable)`, which is what the rewrite emits.
            if !name.is_cons() {
                return Err(EvalError::BadSyntax {
                    operator: "a lambda list",
                    detail: "&key wants (keyword variable) pairs here",
                });
            }
            let keyword = mc.car(name);
            let variable = mc.car(mc.cdr(name));
            keys.push((keyword, variable));
            continue;
        }

        if !mc.is_symbol(name) {
            return Err(EvalError::BadSyntax {
                operator: "a lambda list",
                detail: "wants symbols for parameters",
            });
        }

        if section == 0 {
            required += 1;
        }
        if arg.is_cons() {
            names.push(name);
            values.push(mc.car(arg));
            arg = mc.cdr(arg);
            supplied += 1;
        } else if section == 1 {
            names.push(name);
            values.push(marker);
        } else {
            return Err(EvalError::WrongArgumentCount {
                name: "this function",
                wanted: required,
                got: supplied,
            });
        }
    }

    if !keys.is_empty() || allow_other_keys {
        bind_keys(
            mc,
            &keys,
            arg,
            marker,
            allow_other_keys,
            &mut names,
            &mut values,
        )?;
        return Ok((names, values));
    }

    if arg.is_cons() && !has_rest {
        let mut extra = 0;
        let mut cursor = arg;
        while cursor.is_cons() {
            extra += 1;
            cursor = mc.cdr(cursor);
        }
        return Err(EvalError::WrongArgumentCount {
            name: "this function",
            wanted: names.len(),
            got: names.len() + extra,
        });
    }
    Ok((names, values))
}

/// Scan what is left of the argument list as `:keyword value` pairs.
///
/// CLHS says the *leftmost* occurrence of a keyword wins, which is why
/// this takes the first match rather than the last, and that unknown
/// keywords are an error unless `&allow-other-keys` was declared.
fn bind_keys<'h>(
    mc: &mut Mutation<'h>,
    keys: &[(Value<'h>, Value<'h>)],
    mut arg: Value<'h>,
    marker: Value<'h>,
    allow_other_keys: bool,
    names: &mut Vec<Value<'h>>,
    values: &mut Vec<Value<'h>>,
) -> Result<(), EvalError> {
    let mut given: Vec<(Value<'h>, Value<'h>)> = Vec::new();
    while arg.is_cons() {
        let keyword = mc.car(arg);
        let rest = mc.cdr(arg);
        if !rest.is_cons() {
            return Err(EvalError::BadCall("odd number of keyword arguments"));
        }
        given.push((keyword, mc.car(rest)));
        arg = mc.cdr(rest);
    }

    // `:allow-other-keys` with a true value among the arguments switches the
    // check off for this call, as `&allow-other-keys` does for every call
    // (CLHS 3.4.1.4.1); the first occurrence counts, like any keyword's. It
    // is itself always allowed.
    let is_allow =
        |mc: &Mutation<'h>, k: Value<'h>| mc.is_keyword(k) && mc.symbol_is(k, "ALLOW-OTHER-KEYS");
    let allowed_here = given
        .iter()
        .find(|(k, _)| is_allow(mc, *k))
        .is_some_and(|(_, v)| !v.is_nil());
    if !allow_other_keys && !allowed_here {
        for (keyword, _) in &given {
            if !is_allow(mc, *keyword) && !keys.iter().any(|(k, _)| k == keyword) {
                return Err(EvalError::BadCall("unknown keyword argument"));
            }
        }
    }

    for (keyword, variable) in keys {
        let found = given
            .iter()
            .find(|(k, _)| k == keyword)
            .map(|(_, v)| *v)
            .unwrap_or(marker);
        names.push(*variable);
        values.push(found);
    }
    Ok(())
}
