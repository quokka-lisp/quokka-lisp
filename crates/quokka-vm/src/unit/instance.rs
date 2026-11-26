//! Tests for [`crate::instance`].

use super::*;
use crate::heap::Heap;
use crate::primitives::{call_slice, index, Prim};

fn call<'h>(mc: &mut Mutation<'h>, prim: Prim, args: &[Value<'h>]) -> Result<Value<'h>, EvalError> {
    call_slice(mc, index(prim), args)
}

fn n<'h>(x: i64) -> Value<'h> {
    Value::fixnum(x).unwrap()
}

/// Both kinds of instance hold a wrapper and a slot vector, and the
/// accessors read and replace them; anything else has no wrapper.
#[test]
fn an_instance_keeps_its_wrapper_and_slots() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        for prim in [
            Prim::AllocateStandardInstance,
            Prim::AllocateFuncallableInstance,
        ] {
            let i = call(mc, prim, &[n(1), n(2)]).unwrap();
            assert_eq!(call(mc, Prim::InstanceWrapper, &[i]).unwrap(), n(1));
            assert_eq!(call(mc, Prim::InstanceSlots, &[i]).unwrap(), n(2));
            call(mc, Prim::SetInstanceWrapper, &[i, n(3)]).unwrap();
            call(mc, Prim::SetInstanceSlots, &[i, n(4)]).unwrap();
            assert_eq!(wrapper(mc, i), Some(n(3)));
            assert_eq!(slots(mc, i), Some(n(4)));
        }
        let cons = mc.cons(n(1), Value::NIL);
        for other in [cons, n(5), Value::NIL] {
            assert_eq!(
                call(mc, Prim::InstanceWrapper, &[other]).unwrap(),
                Value::NIL
            );
            assert!(call(mc, Prim::InstanceSlots, &[other]).is_err());
            assert!(call(mc, Prim::SetInstanceWrapper, &[other, n(1)]).is_err());
        }
    });
}

/// Only a funcallable instance takes a function, and only a function that is
/// not itself a funcallable instance: calling one replaces it by its
/// function once, so a chain could never end.
#[test]
fn a_funcallable_instance_takes_only_a_plain_function() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let f = call(mc, Prim::AllocateFuncallableInstance, &[n(1), n(2)]).unwrap();
        let s = call(mc, Prim::AllocateStandardInstance, &[n(1), n(2)]).unwrap();
        assert_eq!(
            call(mc, Prim::FuncallableInstancep, &[f]).unwrap(),
            Value::T
        );
        assert_eq!(
            call(mc, Prim::FuncallableInstancep, &[s]).unwrap(),
            Value::NIL
        );
        assert_eq!(function(mc, f), Value::NIL);
        let car = Value::primitive(index(Prim::Car));
        assert_eq!(
            call(mc, Prim::SetFuncallableInstanceFunction, &[f, car]).unwrap(),
            car
        );
        assert_eq!(function(mc, f), car);
        assert!(call(mc, Prim::SetFuncallableInstanceFunction, &[f, f]).is_err());
        assert!(call(mc, Prim::SetFuncallableInstanceFunction, &[f, n(1)]).is_err());
        assert!(call(mc, Prim::SetFuncallableInstanceFunction, &[s, car]).is_err());
        assert_eq!(call(mc, Prim::Functionp, &[f]).unwrap(), Value::T);
        assert_eq!(call(mc, Prim::Functionp, &[s]).unwrap(), Value::NIL);
    });
}

/// The collector traces an instance's slots, and an image keeps both kinds.
#[test]
fn instances_survive_collection_and_an_image() {
    let mut heap = Heap::new();
    heap.mutate(|mc| {
        let inner = mc.cons(n(7), Value::NIL);
        let s = make_standard(mc, n(1), inner);
        let f = make_funcallable(mc, n(2), s);
        let car = Value::primitive(crate::primitives::index(crate::primitives::Prim::Car));
        set_function(mc, f, car).unwrap();
        mc.set_root(f);
    });
    heap.collect();
    heap.verify().unwrap();
    let bytes = crate::image::save(&mut heap);
    let mut loaded = crate::image::load(&bytes, 1 << 20).unwrap();
    loaded.verify().unwrap();
    loaded.mutate(|mc| {
        let f = mc.root();
        assert!(is_funcallable(mc, f));
        assert_eq!(wrapper(mc, f), Some(n(2)));
        let s = slots(mc, f).unwrap();
        assert!(is_standard(mc, s));
        let inner = slots(mc, s).unwrap();
        assert_eq!(mc.car(inner), n(7));
        assert!(function(mc, f).as_primitive().is_some());
    });
}
