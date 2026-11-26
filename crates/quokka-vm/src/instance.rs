//! Instances of classes (#124): the two heap objects CLOS is built on.
//!
//! A **standard instance** is an object of a class `defclass` made: two
//! traced slots, the **wrapper**, which belongs to the instance's class and
//! says which slots it has, and the **slot vector**, which holds them. The
//! slots are in a vector of their own rather than inline after the wrapper,
//! so that redefining a class, or `change-class`, can give an existing
//! instance a different number of slots while it stays the same object
//! (`eq` to what it was): the library replaces the vector and the wrapper.
//! This is the representation of PCL, SBCL's CLOS.
//!
//! A **funcallable instance** is the same with a function in front of it.
//! A generic function must be called like any function, `(area shape)`, and
//! must also be an instance with slots (its name, its methods), so the
//! machine has to know how to call it: `Vm::call_value` calls its function
//! with the arguments unchanged. That is the only reason this is Rust.
//!
//! Both have their own header type, distinct from the `Instance` a
//! structure is, because every structure path reads an `Instance`'s slot 0
//! as a structure layout. Everything else about classes is Lisp
//! (`lisp/library/clos.lisp`); a wrapper here is whatever object the
//! library put there.

use crate::error::EvalError;
use crate::heap::{Mutation, ObjectType};
use crate::value::Value;

/// A standard instance's slots.
pub const STANDARD_SLOTS: usize = 2;
/// A funcallable instance's slots.
pub const FUNCALLABLE_SLOTS: usize = 3;
/// Where a funcallable instance keeps its function.
pub const FUNCTION: usize = 0;

/// Is `v` a standard instance?
pub fn is_standard<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::StandardInstance
}

/// Is `v` a funcallable instance?
pub fn is_funcallable<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::FuncallableInstance
}

/// Where the wrapper is, for either kind; `None` for anything else.
fn wrapper_slot<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<usize> {
    if is_standard(mc, v) {
        Some(0)
    } else if is_funcallable(mc, v) {
        Some(1)
    } else {
        None
    }
}

/// A new standard instance.
pub fn make_standard<'h>(mc: &mut Mutation<'h>, wrapper: Value<'h>, slots: Value<'h>) -> Value<'h> {
    let i = mc.alloc(ObjectType::StandardInstance, STANDARD_SLOTS);
    mc.set_slot(i, 0, wrapper);
    mc.set_slot(i, 1, slots);
    i
}

/// A new funcallable instance, with no function yet.
pub fn make_funcallable<'h>(
    mc: &mut Mutation<'h>,
    wrapper: Value<'h>,
    slots: Value<'h>,
) -> Value<'h> {
    let i = mc.alloc(ObjectType::FuncallableInstance, FUNCALLABLE_SLOTS);
    mc.set_slot(i, FUNCTION, Value::NIL);
    mc.set_slot(i, 1, wrapper);
    mc.set_slot(i, 2, slots);
    i
}

/// The wrapper of an instance of either kind.
pub fn wrapper<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<Value<'h>> {
    wrapper_slot(mc, v).map(|at| mc.slot(v, at))
}

/// The slot vector of an instance of either kind.
pub fn slots<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<Value<'h>> {
    wrapper_slot(mc, v).map(|at| mc.slot(v, at + 1))
}

/// Replace the wrapper; false if `v` is no instance.
pub fn set_wrapper<'h>(mc: &mut Mutation<'h>, v: Value<'h>, wrapper: Value<'h>) -> bool {
    match wrapper_slot(mc, v) {
        Some(at) => {
            mc.set_slot(v, at, wrapper);
            true
        }
        None => false,
    }
}

/// Replace the slot vector; false if `v` is no instance.
pub fn set_slots<'h>(mc: &mut Mutation<'h>, v: Value<'h>, slots: Value<'h>) -> bool {
    match wrapper_slot(mc, v) {
        Some(at) => {
            mc.set_slot(v, at + 1, slots);
            true
        }
        None => false,
    }
}

/// A funcallable instance's function, nil until one is set.
pub fn function<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Value<'h> {
    mc.slot(v, FUNCTION)
}

/// Is `v` a function the machine can enter directly: a primitive, a
/// compiled function or a closure? A funcallable instance is a function too,
/// but not one of these.
pub fn is_plain_function<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.as_primitive().is_some()
        || (v.is_object()
            && matches!(
                mc.header(v).object_type,
                ObjectType::Closure | ObjectType::Function
            ))
}

/// Give a funcallable instance its function. Refused for anything but a
/// plain function: calling a funcallable instance replaces it by its
/// function once, so its function must not be another funcallable instance.
pub fn set_function<'h>(
    mc: &mut Mutation<'h>,
    instance: Value<'h>,
    function: Value<'h>,
) -> Result<Value<'h>, EvalError> {
    if !is_funcallable(mc, instance) {
        return Err(EvalError::WrongType {
            name: "%SET-FUNCALLABLE-INSTANCE-FUNCTION",
            wanted: "a funcallable instance",
        });
    }
    if !is_plain_function(mc, function) {
        return Err(EvalError::WrongType {
            name: "%SET-FUNCALLABLE-INSTANCE-FUNCTION",
            wanted: "a function that is not a funcallable instance",
        });
    }
    mc.set_slot(instance, FUNCTION, function);
    Ok(function)
}

#[cfg(test)]
#[path = "unit/instance.rs"]
mod tests;
