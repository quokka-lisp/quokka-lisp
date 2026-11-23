//! Closures: a function plus the scope it was written in.
//!
//! Capturing the environment is what makes `(let ((x 10)) (lambda () x))` still
//! know that `x` is 10 after the `let` has finished. Without it a lambda could
//! only see globals.

use crate::error::EvalError;
use crate::heap::{Mutation, ObjectType};
use crate::value::Value;

/// The parameter list.
pub const PARAMS: usize = 0;
/// The body forms, an implicit `progn`.
pub const BODY: usize = 1;
/// The scope the lambda was evaluated in.
pub const ENV: usize = 2;

/// Make a closure.
pub fn make<'h>(
    mc: &mut Mutation<'h>,
    params: Value<'h>,
    body: Value<'h>,
    env: Value<'h>,
) -> Value<'h> {
    let c = mc.alloc(ObjectType::Closure, 3);
    mc.set_slot(c, PARAMS, params);
    mc.set_slot(c, BODY, body);
    mc.set_slot(c, ENV, env);
    c
}

/// Is this a closure?
pub fn is<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::Closure
}

/// Make a macro: the same three slots, a different type.
pub fn make_macro<'h>(
    mc: &mut Mutation<'h>,
    params: Value<'h>,
    body: Value<'h>,
    env: Value<'h>,
) -> Value<'h> {
    let m = mc.alloc(ObjectType::Macro, 3);
    mc.set_slot(m, PARAMS, params);
    mc.set_slot(m, BODY, body);
    mc.set_slot(m, ENV, env);
    m
}

/// Is this a macro?
pub fn is_macro<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::Macro
}

/// Is this either?
pub fn is_callable<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    is(mc, v) || is_macro(mc, v)
}

/// What a *function designator* designates (CLHS 1.4.1.5): a symbol, wherever
/// a function is called, means its global function — `(funcall 'car x)`,
/// `:test 'eql`. Anything else is itself. A symbol with no function, or
/// whose function cell holds a macro, signals `undefined-function`. Both
/// evaluators call this before applying anything (#62, part 3).
pub fn designated<'h>(mc: &Mutation<'h>, f: Value<'h>) -> Result<Value<'h>, EvalError> {
    if !mc.is_symbol(f) {
        return Ok(f);
    }
    let function = mc.slot(f, crate::heap::symbol::FUNCTION);
    if function == Value::UNBOUND || is_macro(mc, function) {
        return Err(EvalError::UndefinedFunction(mc.symbol_name(f).to_string()));
    }
    Ok(function)
}
