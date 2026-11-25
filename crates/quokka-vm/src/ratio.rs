//! Ratios (#59): the representation, and what hashing needs to know.
//!
//! A ratio is an exact quotient of two integers, `1/3`, kept as a heap object
//! of two traced slots, the numerator and the denominator (decision 2 of
//! #59). The numerator and the denominator are integers, fixnums or bignums;
//! the denominator is greater than 1, and the two have no common divisor —
//! CLHS 12.1.3.2's *rational canonicalisation*, which `ratios.lisp`'s `%ratio`
//! does before it calls [`make`]. So two ratios are `eql` exactly when their
//! numerators and denominators are, and a ratio is never an integer.
//!
//! Everything else is Lisp: the arithmetic, which reaches it through the
//! same fallback as bignums (`Vm::fallback`), reading, printing, converting
//! to and from floats. That is the division #16 drew for bignums:
//! representation in Rust, algorithms in Lisp.

use crate::heap::{Mutation, ObjectType};
use crate::value::Value;

/// The numerator's slot.
pub const NUMERATOR: usize = 0;
/// The denominator's slot.
pub const DENOMINATOR: usize = 1;

/// Is `v` a ratio?
pub fn is<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::Ratio
}

/// `(%make-ratio numerator denominator)`: a ratio of two integers, which the
/// caller has already put in lowest terms with a denominator above 1.
pub fn make<'h>(mc: &mut Mutation<'h>, numerator: Value<'h>, denominator: Value<'h>) -> Value<'h> {
    let r = mc.alloc(ObjectType::Ratio, 2);
    mc.set_slot(r, NUMERATOR, numerator);
    mc.set_slot(r, DENOMINATOR, denominator);
    r
}

/// The double a ratio equals exactly, if it equals one with a fraction: that
/// is, a numerator below 2^53 over a power of two. Every other ratio equals
/// no double, since a double with a fraction is such a numerator over such a
/// power. `equalp` compares numbers with `=`, so `1/2` and `0.5` must hash
/// alike in an `equalp` table, and this is how.
pub fn exact_double<'h>(mc: &Mutation<'h>, r: Value<'h>) -> Option<f64> {
    let n = mc.slot(r, NUMERATOR).as_fixnum()?;
    if n.unsigned_abs() >= 1 << 53 {
        return None;
    }
    let d = mc.slot(r, DENOMINATOR);
    let exponent = match d.as_fixnum() {
        Some(d) if d > 0 && d & (d - 1) == 0 => d.trailing_zeros() as i32,
        Some(_) => return None,
        None => {
            // A bignum is a power of two when its top limb is and every
            // other limb is zero.
            let limbs = crate::bignum::length(mc, d);
            if limbs == 0 {
                return None;
            }
            let top = crate::bignum::limb(mc, d, limbs - 1);
            if top & (top - 1) != 0 || (0..limbs - 1).any(|i| crate::bignum::limb(mc, d, i) != 0) {
                return None;
            }
            (crate::bignum::LIMB_BITS * (limbs as u32 - 1) + top.trailing_zeros()) as i32
        }
    };
    Some(libm::ldexp(n as f64, -exponent))
}

#[cfg(test)]
#[path = "unit/ratio.rs"]
mod tests;
