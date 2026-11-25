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
//! # Macros
//!
//! A macro's expander is a function of two arguments, the macro form and the
//! environment, as CLHS 3.1.2.1.2.2 defines it (#76). Its lambda list is
//! therefore not bound by [`bind`] at all: the expander's own parameters are
//! two placeholders, and everything the programmer wrote — patterns, `&key`,
//! `&whole`, `&environment` — is taken apart by generated `car`/`cdr` code
//! in the `let*` (the private `destructure`). `destructuring-bind` uses the same
//! code over the one list it is given.

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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `lambda`, `defun`, `flet`, `labels`.
    Ordinary,
    /// `defmacro`, `macrolet`: an expander, a function of the macro form and
    /// the environment (#76), whose lambda list takes apart `(cdr form)`.
    Macro,
    /// `destructuring-bind`: a function of the one list it takes apart.
    Destructuring,
}

/// Which part of the lambda list we are reading. In the order the parts
/// must come in (CLHS 3.4.1), which [`enter`] holds a lambda list to.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Section {
    Required,
    Optional,
    Rest,
    Key,
    /// After `&allow-other-keys`: no parameter may follow, only `&aux`.
    AfterKeys,
    Aux,
}

/// Move from one part of a lambda list to the part a lambda-list keyword
/// opens, or refuse.
///
/// The parts come in one order and each at most once, so a keyword may only
/// move *forwards*: `(&optional a &optional b)` and `(&key a &optional b)`
/// are both a keyword that does not. Until #110 either was accepted, and
/// meant whatever the loop below happened to make of it — the second
/// `&optional` simply carried on the first. `&allow-other-keys` belongs to
/// `&key` and to nothing else. SBCL refuses the same lists, as a
/// `program-error`, which `BadSyntax` is.
fn enter(from: Section, to: Section) -> Result<Section, EvalError> {
    if to == Section::AfterKeys && from != Section::Key {
        return Err(bad("&allow-other-keys comes after &key's parameters, once"));
    }
    if from >= to {
        return Err(bad(
            "has a lambda-list keyword twice or out of order; the order is &optional, &rest, &key, &aux",
        ));
    }
    Ok(to)
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
        operator: "lambda list",
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
    if mc.list_elements(params).is_none() {
        return Err(bad("wants a list that ends"));
    }
    if kind != Kind::Ordinary {
        return rewrite_destructuring(mc, params, body, kind);
    }
    let mut section = Section::Required;
    let mut required: Vec<Value<'h>> = Vec::new();
    let mut optional: Vec<Entry<'h>> = Vec::new();
    let mut keys: Vec<Entry<'h>> = Vec::new();
    let mut aux: Vec<Entry<'h>> = Vec::new();
    let mut rest: Option<Value<'h>> = None;
    let mut allow_other_keys = false;
    let mut saw_key = false;

    let mut cursor = params;
    while cursor.is_cons() {
        let item = mc.car(cursor);
        cursor = mc.cdr(cursor);

        if mc.is_symbol(item) {
            if mc.symbol_is(item, "&OPTIONAL") {
                section = enter(section, Section::Optional)?;
                continue;
            }
            if mc.symbol_is(item, "&REST") || mc.symbol_is(item, "&BODY") {
                section = enter(section, Section::Rest)?;
                continue;
            }
            if mc.symbol_is(item, "&KEY") {
                section = enter(section, Section::Key)?;
                saw_key = true;
                continue;
            }
            if mc.symbol_is(item, "&ALLOW-OTHER-KEYS") {
                section = enter(section, Section::AfterKeys)?;
                allow_other_keys = true;
                continue;
            }
            if mc.symbol_is(item, "&AUX") {
                section = enter(section, Section::Aux)?;
                continue;
            }
        }

        match section {
            Section::Required => {
                if !mc.is_symbol(item) {
                    return Err(bad("wants a symbol for a required parameter"));
                }
                required.push(item);
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
            Section::Optional => optional.push(parse_entry(mc, item, false, false)?),
            Section::Key => keys.push(parse_entry(mc, item, true, false)?),
            Section::AfterKeys => return Err(bad("only &aux may follow &allow-other-keys")),
            Section::Aux => aux.push(parse_entry(mc, item, false, false)?),
        }
    }
    if !cursor.is_nil() {
        return Err(bad("a lambda list must be a proper list"));
    }

    // The common case: nothing that needs evaluating, so nothing is rewritten
    // and the closure holds exactly what was written.
    if optional.is_empty() && keys.is_empty() && aux.is_empty() {
        return Ok((params, body));
    }

    let marker = mc.intern(UNSUPPLIED);
    let quoted_marker = quote(mc, marker);

    // ---- the simple lambda list -----------------------------------------
    //
    // Every `&optional`, `&rest` and `&key` parameter is bound here under a
    // fresh name, and under its own only in the `let*`, in order, so that a
    // default form sees exactly the parameters to its left (CLHS 3.4.1, #76).
    // Bound under their own names, `(&optional (x (1+ x)))` saw the
    // parameter `x` holding the marker, and not the `x` outside.
    let mut fresh = Vec::new();
    for _ in optional.iter().chain(keys.iter()) {
        fresh.push(mc.gensym());
    }
    let rest_fresh = rest.map(|_| mc.gensym());
    let mut simple: Vec<Value<'h>> = required.clone();
    if !optional.is_empty() {
        simple.push(mc.intern("&OPTIONAL"));
        simple.extend_from_slice(&fresh[..optional.len()]);
    }
    if let Some(r) = rest_fresh {
        simple.push(mc.intern("&REST"));
        simple.push(r);
    }
    if saw_key {
        simple.push(mc.intern("&KEY"));
        for (e, g) in keys.iter().zip(&fresh[optional.len()..]) {
            let keyword = keyword_of(mc, e);
            let pair = list2(mc, keyword, *g);
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

    for (i, (e, g)) in optional.iter().chain(keys.iter()).zip(&fresh).enumerate() {
        // `&rest` sits between the optional and the keyword parameters.
        if i == optional.len() {
            if let (Some(r), Some(g)) = (rest, rest_fresh) {
                let b = list2(mc, r, g);
                bindings.push(b);
            }
        }
        let default = e.default.unwrap_or(Value::NIL);
        let test = list3(mc, eq, *g, quoted_marker);
        match e.supplied {
            // (given (if (eq g '<marker>) nil t))
            // (v     (if given g DEFAULT))
            Some(given) => {
                let is_given = list_of(mc, &[if_, test, Value::NIL, Value::T]);
                let b = list2(mc, given, is_given);
                bindings.push(b);
                let pick = list_of(mc, &[if_, given, *g, default]);
                let b = list2(mc, e.name, pick);
                bindings.push(b);
            }
            // (v (if (eq g '<marker>) DEFAULT g))
            None => {
                let pick = list_of(mc, &[if_, test, default, *g]);
                let b = list2(mc, e.name, pick);
                bindings.push(b);
            }
        }
    }
    if keys.is_empty() {
        if let (Some(r), Some(g)) = (rest, rest_fresh) {
            let b = list2(mc, r, g);
            bindings.push(b);
        }
    }
    // &aux is not a parameter at all — it is a `let*` that someone wrote
    // inside the lambda list. Here it becomes one.
    for e in &aux {
        let default = e.default.unwrap_or(Value::NIL);
        let b = list2(mc, e.name, default);
        bindings.push(b);
    }

    Ok((simple, let_star(mc, &bindings, body, &required)))
}

/// `((let* BINDINGS . BODY))`, a body of one form.
///
/// A `special` declaration of a REQUIRED parameter also goes in front of the
/// `let*`, where it reaches the function that binds the parameter, so that
/// it is bound dynamically before any default form runs: CLHS 3.4.1 binds
/// each parameter before the next one's default is evaluated (ansi-test
/// lambda.64, #80). Nothing else is copied there. A declaration of a name
/// the lambda list does not bind is *free*, and CLHS 3.3.4 keeps a free
/// declaration away from the default forms (ansi-test lambda.52), which is
/// where the body's declarations inside the `let*` leave it.
fn let_star<'h>(
    mc: &mut Mutation<'h>,
    bindings: &[Value<'h>],
    body: Value<'h>,
    required: &[Value<'h>],
) -> Value<'h> {
    let mut specials = Vec::new();
    let mut rest = body;
    while rest.is_cons() && mc.car(rest).is_cons() && mc.symbol_is(mc.car(mc.car(rest)), "DECLARE")
    {
        let mut specifiers = mc.cdr(mc.car(rest));
        while specifiers.is_cons() {
            let specifier = mc.car(specifiers);
            if specifier.is_cons() && mc.symbol_is(mc.car(specifier), "SPECIAL") {
                let mut names = mc.cdr(specifier);
                while names.is_cons() {
                    if required.contains(&mc.car(names)) {
                        specials.push(mc.car(names));
                    }
                    names = mc.cdr(names);
                }
            }
            specifiers = mc.cdr(specifiers);
        }
        rest = mc.cdr(rest);
    }
    let bindings = list_of(mc, bindings);
    let let_star = mc.intern("LET*");
    let form = mc.cons(bindings, body);
    let form = mc.cons(let_star, form);
    let out = mc.cons(form, Value::NIL);
    if specials.is_empty() {
        return out;
    }
    let special = mc.intern("SPECIAL");
    let declare = mc.intern("DECLARE");
    let names = list_of(mc, &specials);
    let specifier = mc.cons(special, names);
    let declaration = list2(mc, declare, specifier);
    mc.cons(declaration, out)
}

/// A macro's lambda list, or `destructuring-bind`'s, rewritten.
///
/// The function's own parameters are fixed, and everything the programmer
/// wrote is taken apart in the `let*` by [`destructure`]. An expander is a
/// function of the form and the environment, as CLHS 3.1.2.1.2.2 defines it
/// (#76): `&whole` is the form, `&environment` the environment, and the rest
/// of the lambda list matches the arguments, `(cdr form)`. Both are bound
/// first, so every default form can see them. `destructuring-bind`'s
/// function takes the list it destructures, and `&whole` is that list.
fn rewrite_destructuring<'h>(
    mc: &mut Mutation<'h>,
    params: Value<'h>,
    body: Value<'h>,
    kind: Kind,
) -> Result<(Value<'h>, Value<'h>), EvalError> {
    check_pattern(mc, params)?;
    let whole = mc.gensym();
    let mut bindings = Vec::new();
    if kind == Kind::Destructuring {
        destructure(mc, params, whole, &mut bindings)?;
        let simple = mc.cons(whole, Value::NIL);
        return Ok((simple, let_star(mc, &bindings, body, &[])));
    }
    let environment = mc.gensym();
    // `&environment` may stand anywhere at the top level; take it out.
    let mut items = Vec::new();
    let mut variable = None;
    let mut cursor = params;
    while cursor.is_cons() {
        let item = mc.car(cursor);
        cursor = mc.cdr(cursor);
        if mc.symbol_is(item, "&ENVIRONMENT") && cursor.is_cons() {
            variable = Some(mc.car(cursor));
            cursor = mc.cdr(cursor);
        } else {
            items.push(item);
        }
    }
    let mut pattern = cursor;
    for item in items.iter().rev() {
        pattern = mc.cons(*item, pattern);
    }
    // `&whole` may stand only first, and here it is the form, not `(cdr form)`.
    if pattern.is_cons() && mc.symbol_is(mc.car(pattern), "&WHOLE") {
        let after = mc.cdr(pattern);
        if !after.is_cons() {
            return Err(bad("&whole wants a name after it"));
        }
        let target = mc.car(after);
        bind_here(mc, target, whole, &mut bindings)?;
        pattern = mc.cdr(after);
    }
    if let Some(v) = variable {
        let b = list2(mc, v, environment);
        bindings.push(b);
    }
    let cdr = mc.intern("CDR");
    let arguments = list2(mc, cdr, whole);
    destructure(mc, pattern, arguments, &mut bindings)?;
    let simple = list2(mc, whole, environment);
    Ok((simple, let_star(mc, &bindings, body, &[])))
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
///
/// `&whole` binds the list the pattern matches, `&key` searches what is left
/// as a property list ([`destructure_keys`]), and `&aux` binds after
/// everything else (#76).
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

    let mut item = pattern;
    // `&whole`, first or not at all: the whole list this pattern matches.
    if item.is_cons() && mc.symbol_is(mc.car(item), "&WHOLE") {
        let after = mc.cdr(item);
        if !after.is_cons() {
            return Err(bad("&whole wants a name after it"));
        }
        let target = mc.car(after);
        bind_here(mc, target, cursor, bindings)?;
        item = mc.cdr(after);
    }

    let mut section = Section::Required;
    // Whether `&rest`, `&key` or a dotted tail takes what is left, so that
    // nothing is left over to complain about.
    let mut tail_taken = false;
    let mut keys: Vec<Entry<'h>> = Vec::new();
    let mut aux: Vec<Entry<'h>> = Vec::new();
    let mut allow_other_keys = false;
    let mut saw_key = false;
    while item.is_cons() {
        let entry = mc.car(item);
        item = mc.cdr(item);

        if mc.is_symbol(entry) && mc.symbol_name(entry).starts_with('&') {
            if mc.symbol_is(entry, "&OPTIONAL") {
                section = enter(section, Section::Optional)?;
            } else if mc.symbol_is(entry, "&REST") || mc.symbol_is(entry, "&BODY") {
                section = enter(section, Section::Rest)?;
                if !item.is_cons() {
                    return Err(bad("&rest wants a name after it"));
                }
                let name = mc.car(item);
                item = mc.cdr(item);
                bind_here(mc, name, cursor, bindings)?;
                tail_taken = true;
            } else if mc.symbol_is(entry, "&KEY") {
                section = enter(section, Section::Key)?;
                tail_taken = true;
                saw_key = true;
            } else if mc.symbol_is(entry, "&ALLOW-OTHER-KEYS") {
                section = enter(section, Section::AfterKeys)?;
                allow_other_keys = true;
            } else if mc.symbol_is(entry, "&AUX") {
                section = enter(section, Section::Aux)?;
            } else {
                return Err(bad("a lambda-list keyword this pattern cannot have here"));
            }
            continue;
        }

        match section {
            // A required position: take the head, then move on. The `consp`
            // test is the arity check — a pattern of two against a call of
            // one signals rather than binding nil.
            Section::Required => {
                let have = list2(mc, consp, cursor);
                let short = mc.cons(too_short, Value::NIL);
                let take = list2(mc, car, cursor);
                let take = list_of(mc, &[if_, have, take, short]);
                bind_here(mc, entry, take, bindings)?;
                let step = list2(mc, cdr, cursor);
                let b = list2(mc, cursor, step);
                bindings.push(b);
            }
            Section::Optional => {
                let e = parse_entry(mc, entry, false, true)?;
                let default = e.default.unwrap_or(Value::NIL);
                let empty = list2(mc, null, cursor);
                if let Some(given) = e.supplied {
                    let is_given = list_of(mc, &[if_, empty, Value::NIL, Value::T]);
                    let b = list2(mc, given, is_given);
                    bindings.push(b);
                }
                let take = list2(mc, car, cursor);
                let pick = list_of(mc, &[if_, empty, default, take]);
                bind_here(mc, e.name, pick, bindings)?;
                let step = list2(mc, cdr, cursor);
                let advance = list_of(mc, &[if_, empty, Value::NIL, step]);
                let b = list2(mc, cursor, advance);
                bindings.push(b);
            }
            Section::Rest => return Err(bad("&rest takes one name")),
            Section::Key => keys.push(parse_entry(mc, entry, true, true)?),
            Section::AfterKeys => return Err(bad("only &aux may follow &allow-other-keys")),
            Section::Aux => aux.push(parse_entry(mc, entry, false, false)?),
        }
    }

    // A dotted tail — `(a . rest)` — binds the rest of the list.
    if !item.is_nil() {
        if !mc.is_symbol(item) || tail_taken {
            return Err(bad("a pattern tail must be a symbol"));
        }
        let b = list2(mc, item, cursor);
        bindings.push(b);
        tail_taken = true;
    }
    if saw_key {
        destructure_keys(mc, &keys, cursor, allow_other_keys, bindings)?;
    }
    // Anything still in the cursor is an argument the pattern did not ask for.
    if !tail_taken {
        let too_long = mc.intern("%DESTRUCTURING-TOO-LONG");
        let empty = list2(mc, null, cursor);
        let complain = mc.cons(too_long, Value::NIL);
        let check = list_of(mc, &[if_, empty, Value::NIL, complain]);
        let ignored = mc.gensym();
        let b = list2(mc, ignored, check);
        bindings.push(b);
    }
    for e in &aux {
        let default = e.default.unwrap_or(Value::NIL);
        let b = list2(mc, e.name, default);
        bindings.push(b);
    }
    Ok(())
}

/// `&key` in a pattern: the rest of the list is a property list, searched in
/// generated code by two prelude functions, since there is no argument
/// binder here to do it. `%destructuring-keys` checks the list once (an even
/// length, no keyword the pattern does not know unless other keys are
/// allowed); each parameter is then `(v (%destructuring-key c :k marker))`,
/// the leftmost value or the marker, filled in from its default as an
/// `&optional` one is.
fn destructure_keys<'h>(
    mc: &mut Mutation<'h>,
    keys: &[Entry<'h>],
    cursor: Value<'h>,
    allow_other_keys: bool,
    bindings: &mut Vec<Value<'h>>,
) -> Result<(), EvalError> {
    let quote_ = mc.intern("QUOTE");
    let eq = mc.intern("EQ");
    let if_ = mc.intern("IF");
    let marker = mc.intern(UNSUPPLIED);
    let marker = list2(mc, quote_, marker);
    let mut keywords = Vec::new();
    for e in keys {
        let keyword = keyword_of(mc, e);
        keywords.push(keyword);
    }
    let keywords = list_of(mc, &keywords);
    let keywords = list2(mc, quote_, keywords);
    let check = mc.intern("%DESTRUCTURING-KEYS");
    let allow = if allow_other_keys {
        Value::T
    } else {
        Value::NIL
    };
    let check = list_of(mc, &[check, cursor, keywords, allow]);
    let ignored = mc.gensym();
    let b = list2(mc, ignored, check);
    bindings.push(b);
    let find = mc.intern("%DESTRUCTURING-KEY");
    for e in keys {
        let keyword = keyword_of(mc, e);
        let value = mc.gensym();
        let lookup = list_of(mc, &[find, cursor, keyword, marker]);
        let b = list2(mc, value, lookup);
        bindings.push(b);
        let missing = list3(mc, eq, value, marker);
        if let Some(given) = e.supplied {
            let is_given = list_of(mc, &[if_, missing, Value::NIL, Value::T]);
            let b = list2(mc, given, is_given);
            bindings.push(b);
        }
        let default = e.default.unwrap_or(Value::NIL);
        let pick = list_of(mc, &[if_, missing, default, value]);
        bind_here(mc, e.name, pick, bindings)?;
    }
    Ok(())
}

/// The keyword that selects a `&key` parameter: the one written, or its
/// name's.
fn keyword_of<'h>(mc: &mut Mutation<'h>, e: &Entry<'h>) -> Value<'h> {
    match e.keyword {
        Some(k) => k,
        None => {
            let name = mc.symbol_name(e.name);
            mc.intern_keyword(&name)
        }
    }
}

/// Refuse a pattern that [`destructure`] could not finish (#93).
///
/// `destructure` calls itself for each nested pattern, on the Rust stack,
/// where no step budget reaches (rule 4); and a pattern is Lisp data, which
/// may nest to any depth or lead back into itself. So the pattern is walked
/// here first, with a list of work in place of recursion, and refused past
/// a depth no written lambda list reaches or a size no finite one does.
fn check_pattern<'h>(mc: &Mutation<'h>, pattern: Value<'h>) -> Result<(), EvalError> {
    const MAX_DEPTH: usize = 256;
    const MAX_CELLS: usize = 1 << 16;
    let mut cells = 0usize;
    let mut pending = vec![(pattern, 0usize)];
    while let Some((mut at, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err(bad("is nested too deeply"));
        }
        while at.is_cons() {
            cells += 1;
            if cells > MAX_CELLS {
                return Err(bad("is too large, or leads back into itself"));
            }
            let element = mc.car(at);
            if element.is_cons() {
                pending.push((element, depth + 1));
            }
            at = mc.cdr(at);
        }
    }
    Ok(())
}

/// Bind one pattern element — a name, or a nested pattern — to an expression.
///
/// `nil` is the empty pattern `()`, not a variable (#75): it matches only an
/// empty list, so it is taken apart like any pattern, with nothing to bind.
fn bind_here<'h>(
    mc: &mut Mutation<'h>,
    target: Value<'h>,
    source: Value<'h>,
    bindings: &mut Vec<Value<'h>>,
) -> Result<(), EvalError> {
    if target.is_cons() || target.is_nil() {
        return destructure(mc, target, source, bindings);
    }
    if mc.is_symbol(target) {
        let b = list2(mc, target, source);
        bindings.push(b);
        return Ok(());
    }
    Err(bad(
        "a pattern element must be a symbol or a nested pattern",
    ))
}

/// `name`, `(name default)`, `(name default supplied-p)`, or for `&key` also
/// `((:keyword name) default ...)`. In a destructuring pattern (PATTERNS),
/// the name may itself be a nested pattern, `&optional ((a b) '(1 2))` or
/// `&key ((:k (a b)))` (CLHS 3.4.4; ansi-test macrolet.22, #80); `bind_here`
/// takes it apart.
fn parse_entry<'h>(
    mc: &mut Mutation<'h>,
    item: Value<'h>,
    is_key: bool,
    patterns: bool,
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
        let named = inner.is_cons() && (mc.is_symbol(mc.car(inner)) || patterns);
        if !named {
            return Err(bad("&key wants (keyword variable) when naming both"));
        }
        (mc.car(inner), Some(keyword))
    } else {
        if !(mc.is_symbol(head) || patterns && head.is_cons()) {
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
    if mc.list_elements(params).is_none() {
        return Err(bad("wants a list that ends"));
    }
    let mut out = Vec::new();
    let mut cursor = params;
    // A required and an optional parameter are both one name, so nothing
    // here tells them apart: `Required` stands for either.
    let mut section = Section::Required;
    while cursor.is_cons() {
        let item = mc.car(cursor);
        cursor = mc.cdr(cursor);
        if mc.is_symbol(item) {
            if mc.symbol_is(item, "&OPTIONAL") {
                continue;
            }
            if mc.symbol_is(item, "&REST") || mc.symbol_is(item, "&BODY") {
                section = Section::Rest;
                continue;
            }
            if mc.symbol_is(item, "&KEY") {
                section = Section::Key;
                continue;
            }
            if mc.symbol_is(item, "&ALLOW-OTHER-KEYS") {
                continue;
            }
            out.push(item);
            if section == Section::Rest {
                // `&rest` takes exactly one name and then the list is over as
                // far as positional binding goes.
                section = Section::Required;
            }
            continue;
        }
        if section == Section::Key && item.is_cons() {
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
    let mut section = Section::Required;
    // `&rest` swallows everything left, so leftover arguments stop being
    // an error once one has been bound. `&key` still scans the same tail,
    // which is what CLHS says the two do together.
    let mut has_rest = false;
    // `(&key)` names no keys and still takes keyword arguments (#76).
    let mut saw_key = false;
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
                section = Section::Optional;
                continue;
            }
            if mc.symbol_is(name, "&REST") || mc.symbol_is(name, "&BODY") {
                let Some(rest_name) = (param.is_cons()).then(|| mc.car(param)) else {
                    return Err(EvalError::BadSyntax {
                        operator: "lambda list",
                        detail: "&rest wants a name after it",
                    });
                };
                param = mc.cdr(param);
                names.push(rest_name);
                values.push(arg);
                has_rest = true;
                section = Section::Rest;
                continue;
            }
            if mc.symbol_is(name, "&KEY") {
                section = Section::Key;
                saw_key = true;
                continue;
            }
            if mc.symbol_is(name, "&ALLOW-OTHER-KEYS") {
                allow_other_keys = true;
                continue;
            }
        }

        if section == Section::Key {
            // `(keyword variable)`, which is what the rewrite emits.
            if !name.is_cons() {
                return Err(EvalError::BadSyntax {
                    operator: "lambda list",
                    detail: "&key wants (keyword variable) pairs here",
                });
            }
            let keyword = mc.car(name);
            let after = mc.cdr(name);
            if !after.is_cons() {
                return Err(bad("&key wants (keyword variable) pairs here"));
            }
            keys.push((keyword, mc.car(after)));
            continue;
        }

        if !mc.is_symbol(name) {
            return Err(EvalError::BadSyntax {
                operator: "lambda list",
                detail: "wants symbols for parameters",
            });
        }

        if section == Section::Required {
            required += 1;
        }
        if arg.is_cons() {
            names.push(name);
            values.push(mc.car(arg));
            arg = mc.cdr(arg);
            supplied += 1;
        } else if section == Section::Optional {
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

    if saw_key || allow_other_keys {
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
