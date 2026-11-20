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
) -> Result<(Value<'h>, Value<'h>), EvalError> {
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
                if !mc.is_symbol(item) {
                    // Where destructuring would go, if it existed.
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
            Section::Optional => optional.push(parse_entry(mc, item, false)?),
            Section::Key => keys.push(parse_entry(mc, item, true)?),
            Section::Aux => aux.push(parse_entry(mc, item, false)?),
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

    let bindings = list_of(mc, &bindings);
    let let_star = mc.intern("LET*");
    let form = mc.cons(bindings, body);
    let form = mc.cons(let_star, form);
    let new_body = mc.cons(form, Value::NIL);

    Ok((simple, new_body))
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
            let (params, body) = rewrite(mc, params, body).expect("rewrites");
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

    #[test]
    fn a_malformed_lambda_list_is_an_error() {
        let chars = Reader::chars_of("((a . b) 1)");
        let mut heap = Heap::new();
        heap.mutate(|mc| {
            let mut reader = Reader::new(&chars);
            let form = reader.read(mc).unwrap().unwrap();
            let params = mc.car(form);
            let body = mc.cdr(form);
            assert!(rewrite(mc, params, body).is_err());
        });
    }
}
