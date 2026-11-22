//! Lexical environments: a chain of scopes.
//!
//! Each scope is one heap object: a parent link, then name/value pairs. Looking
//! a name up scans this scope's names, then follows the parent, then falls back
//! to the symbol's own value cell, which is where globals live.
//!
//! # Why not an association list
//!
//! The textbook tree-walker uses an alist — `((x . 1) (y . 2))`, extended by
//! consing onto the front. It is shorter, and this is not.
//!
//! Two reasons. A two-argument call costs one object of six words here against
//! four cons cells of eight there, with better locality. And, decisively, this
//! is the layout *lexical addressing* uses: once the compiler of phase 1 can
//! resolve `x` to "scope 2, slot 0" at compile time, the name scan below
//! becomes an index and the data structure does not change. An alist would have
//! to be thrown away.
//!
//! The genuinely fast option — lexical addressing — is not available yet,
//! because resolving a name to a slot number requires a compiler to walk the
//! body, and a tree-walking interpreter re-reads the source each time with
//! nowhere to attach the answer. This is the efficient option that exists.

use crate::heap::{symbol, Mutation, ObjectType};
use crate::value::Value;

/// Slot 0: the enclosing scope, or `nil`.
pub const PARENT: usize = 0;
/// Slot 1: which namespace this scope binds in.
pub const NAMESPACE: usize = 1;
/// Name/value pairs begin here.
pub const FIRST_PAIR: usize = 2;

/// Variables — what `let` binds and a symbol evaluates to.
pub const VARIABLE: i64 = 0;
/// Block names — what `block` binds and `return-from` looks up.
///
/// A third and fourth namespace, because `(block list ...)` must not
/// disturb either the variable `list` or the function. Common Lisp has
/// several namespaces, not two, and this is two of the others.
pub const BLOCK: i64 = 2;
/// `tagbody` tags — what `go` looks up.
pub const TAG: i64 = 3;

/// Functions — what `flet` and `labels` bind, and what the operator
/// position of a call looks up.
///
/// Two namespaces rather than one is what makes this a Lisp-2, and the
/// separation has to reach into lexical scopes as well as symbols: `(flet
/// ((list (x) x)) (list 1))` must call the local function while `list` the
/// variable is untouched.
pub const FUNCTION: i64 = 1;

/// A scope binding `names` to `values` inside `parent`.
pub fn make<'h>(
    mc: &mut Mutation<'h>,
    parent: Value<'h>,
    namespace: i64,
    names: &[Value<'h>],
    values: &[Value<'h>],
) -> Value<'h> {
    debug_assert_eq!(names.len(), values.len());
    let scope = mc.alloc(ObjectType::Environment, FIRST_PAIR + names.len() * 2);
    mc.set_slot(scope, PARENT, parent);
    mc.set_slot(scope, NAMESPACE, Value::fixnum(namespace).expect("small"));
    for (i, (name, value)) in names.iter().zip(values).enumerate() {
        mc.set_slot(scope, FIRST_PAIR + i * 2, *name);
        mc.set_slot(scope, FIRST_PAIR + 1 + i * 2, *value);
    }
    scope
}

/// Where `name` is bound in `namespace`: the scope and the slot holding it.
///
/// Scopes of the other namespace are stepped over rather than searched,
/// which is how a local function and a local variable of the same name stay
/// apart.
fn find<'h>(
    mc: &Mutation<'h>,
    env: Value<'h>,
    namespace: i64,
    name: Value<'h>,
) -> Option<(Value<'h>, usize)> {
    let mut scope = env;
    // Terminating on `is_object` was right until `nil` became one. The
    // chain ends at `nil`, which is a *symbol* in the static area — so the
    // loop asks what kind of object it is rather than whether it is one.
    // That is also total: anything unexpected ends the walk rather than
    // being read as a scope.
    while scope.is_object() && mc.header(scope).object_type == ObjectType::Environment {
        if mc.slot(scope, NAMESPACE).as_fixnum() == Some(namespace) {
            let slots = mc.header(scope).slots;
            let mut i = FIRST_PAIR;
            while i < slots {
                if mc.slot(scope, i) == name {
                    return Some((scope, i + 1));
                }
                i += 2;
            }
        }
        scope = mc.slot(scope, PARENT);
    }
    None
}

/// The value of `name`, or `None` if it is unbound everywhere.
pub fn lookup<'h>(mc: &Mutation<'h>, env: Value<'h>, name: Value<'h>) -> Option<Value<'h>> {
    if let Some((scope, slot)) = find(mc, env, VARIABLE, name) {
        return Some(mc.slot(scope, slot));
    }
    // Globals live in the symbol's own value cell, which #4 already gave us.
    let global = mc.slot(name, symbol::VALUE);
    if global == Value::UNBOUND {
        None
    } else {
        Some(global)
    }
}

/// The function `name` denotes: a lexical binding from `flet` or `labels`
/// first, then the symbol's own function cell.
pub fn lookup_function<'h>(
    mc: &Mutation<'h>,
    env: Value<'h>,
    name: Value<'h>,
) -> Option<Value<'h>> {
    if let Some((scope, slot)) = find(mc, env, FUNCTION, name) {
        return Some(mc.slot(scope, slot));
    }
    let global = mc.slot(name, symbol::FUNCTION);
    if global == Value::UNBOUND {
        None
    } else {
        Some(global)
    }
}

/// The marker a block name or a tag is bound to, if any.
///
/// Lexical: `return-from` and `go` can only reach a block or tag that
/// encloses them in the source, which is what makes them safe. `throw`
/// is the dynamic alternative and does not come through here.
pub fn lookup_marker<'h>(
    mc: &Mutation<'h>,
    env: Value<'h>,
    namespace: i64,
    name: Value<'h>,
) -> Option<Value<'h>> {
    find(mc, env, namespace, name).map(|(scope, slot)| mc.slot(scope, slot))
}

/// Assign to `name`, creating a global if it is bound nowhere.
pub fn assign<'h>(mc: &mut Mutation<'h>, env: Value<'h>, name: Value<'h>, value: Value<'h>) {
    if let Some((scope, slot)) = find(mc, env, VARIABLE, name) {
        mc.set_slot(scope, slot, value);
    } else {
        mc.set_slot(name, symbol::VALUE, value);
    }
}
