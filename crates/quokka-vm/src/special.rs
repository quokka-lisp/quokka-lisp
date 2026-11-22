//! Special operator codes, as stored in a symbol's `SPECIAL` slot.
//!
//! Common Lisp has 25 special operators. These six are what the phase 0 gate
//! needs; the rest arrive once the shape of one is settled by worked examples
//! rather than written blind.

/// `(quote x)` — return `x` unevaluated.
pub const QUOTE: i64 = 1;
/// `(if test then else)` — evaluate one branch.
pub const IF: i64 = 2;
/// `(lambda (params) body...)` — make a closure.
pub const LAMBDA: i64 = 3;
/// `(progn forms...)` — evaluate in order, yield the last.
pub const PROGN: i64 = 4;
/// `(setq var value ...)` — assign.
pub const SETQ: i64 = 5;
// 6 was `defun`, which lives in `lisp/prelude.lisp` now — it is a macro
// in Common Lisp and never should have been an operator here. The number
// is left unused rather than reused, so an old image cannot silently mean
// something new.
//
// `lambda` (3) stays, and is the interesting case. It is *also* a macro in
// Common Lisp, expanding to `(function (lambda ...))` — but `function`
// still has to turn a lambda expression into a closure, so the Rust code
// below cannot leave. Making the symbol a macro as well would move no
// work, only a dispatch. The line is in the right place already.
/// `(let ((v init)...) body...)` — bind variables, all at once.
pub const LET: i64 = 7;
/// `(let* ((v init)...) body...)` — bind variables in order.
pub const LET_STAR: i64 = 8;
/// `(flet ((f (params) body)...) body...)` — bind functions, not visible
/// to each other.
pub const FLET: i64 = 9;
/// `(labels ((f (params) body)...) body...)` — bind functions that can see
/// themselves and each other.
pub const LABELS: i64 = 10;
/// `(function f)`, written `#'f` — the function a name denotes.
pub const FUNCTION: i64 = 11;
/// `(the type form)` — a declaration the interpreter may ignore.
pub const THE: i64 = 12;
/// `(locally decls... body...)` — a body with declarations.
pub const LOCALLY: i64 = 13;
/// `(eval-when (situations...) body...)` — when this is for.
pub const EVAL_WHEN: i64 = 14;
/// `(block name body...)` — a named region that can be left early.
pub const BLOCK: i64 = 15;
/// `(return-from name value)` — leave it.
pub const RETURN_FROM: i64 = 16;
/// `(catch tag body...)` — a region findable at run time.
pub const CATCH: i64 = 17;
/// `(throw tag value)` — leave the nearest matching catch.
pub const THROW: i64 = 18;
/// `(tagbody tag form... tag form...)` — a body with labels.
pub const TAGBODY: i64 = 19;
/// `(go tag)` — jump to one.
pub const GO: i64 = 20;
/// `(unwind-protect body cleanup...)` — cleanup however the body ends.
pub const UNWIND_PROTECT: i64 = 21;
/// `(defmacro name (params) body...)` — define a macro.
pub const DEFMACRO: i64 = 22;
/// `(%handler-case form (type (var) body...) ...)` — run `form`, and if a
/// condition is signalled inside it, hand it to a matching clause.
///
/// Named with a `%` because **Common Lisp's `handler-case` is a macro**,
/// over `handler-bind` and `block`. Quokka has no `handler-bind`, so the
/// machinery is an operator — but the CL-visible `handler-case` is a
/// prelude macro over this, which keeps the special operator count honest.
pub const HANDLER_CASE: i64 = 23;
/// `(multiple-value-call f form...)` — call `f` with every value every
/// form returned, not just the first of each.
pub const MULTIPLE_VALUE_CALL: i64 = 24;
/// `(multiple-value-prog1 first rest...)` — all of `first`'s values,
/// after running the rest for effect.
pub const MULTIPLE_VALUE_PROG1: i64 = 25;
/// `(macrolet ((name (args) body)...) body...)` — macros, locally.
pub const MACROLET: i64 = 26;

/// Every special operator, by name.
pub const ALL: &[(&str, i64)] = &[
    ("QUOTE", QUOTE),
    ("IF", IF),
    ("LAMBDA", LAMBDA),
    ("PROGN", PROGN),
    ("SETQ", SETQ),
    ("LET", LET),
    ("LET*", LET_STAR),
    ("FLET", FLET),
    ("LABELS", LABELS),
    ("FUNCTION", FUNCTION),
    ("THE", THE),
    ("LOCALLY", LOCALLY),
    ("EVAL-WHEN", EVAL_WHEN),
    ("BLOCK", BLOCK),
    ("RETURN-FROM", RETURN_FROM),
    ("CATCH", CATCH),
    ("THROW", THROW),
    ("TAGBODY", TAGBODY),
    ("GO", GO),
    ("UNWIND-PROTECT", UNWIND_PROTECT),
    ("DEFMACRO", DEFMACRO),
    ("%HANDLER-CASE", HANDLER_CASE),
    ("MULTIPLE-VALUE-CALL", MULTIPLE_VALUE_CALL),
    ("MULTIPLE-VALUE-PROG1", MULTIPLE_VALUE_PROG1),
    ("MACROLET", MACROLET),
];
