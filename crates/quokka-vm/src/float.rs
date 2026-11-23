//! Double floats (#17): the representation, the arithmetic, and the two
//! conversions to and from text.
//!
//! Bignums were an algorithm problem; floats are a representation problem.
//! An IEEE 754 double needs all 64 bits of a word and three of ours are the
//! tag's, so a double lives in a heap object of one raw slot — a *box*
//! (decision 1). Hardware floating point is O(1), so rule 9 has nothing to
//! say, and the arithmetic is Rust.
//!
//! What #17 decided, in the order the code meets it:
//!
//! - **One format** (decision 2). Common Lisp's four float types are all this
//!   one; CLHS 12.1.4 allows an implementation to merge them.
//! - **The primitives take doubles themselves** (decision 3). `+` tries its
//!   fixnum fast path first, untouched; when that fails and a double is among
//!   the arguments, [`arithmetic`] does the work here, without entering Lisp.
//!   Only a bignum meeting a double goes to the Lisp fallback.
//! - **Traps, as SBCL has them** (decision 4). An operation on finite numbers
//!   whose result is an infinity or a NaN is an error, checked in [`make`] —
//!   wasm32 has no hardware traps, so the check is ours, on every target.
//!   Infinities and NaNs therefore never exist in a Quokka heap.
//! - **Rust converts to and from text** (decision 5): [`parse`] is
//!   `str::parse`, correctly rounded; [`digits`] is the shortest string that
//!   reads back as the same double. Laying the digits out is Lisp.
//! - **The `libm` crate for the transcendental functions** (decision 7), so
//!   that `sin` gives the same bits natively and in a browser.

use crate::error::EvalError;
use crate::heap::{Mutation, ObjectType};
use crate::value::Value;
use core::cmp::Ordering;

/// Is `v` a double?
pub fn is<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::DoubleFloat
}

/// The double in a box.
pub fn get<'h>(mc: &Mutation<'h>, v: Value<'h>) -> f64 {
    f64::from_bits(mc.raw_slot(v, 0))
}

/// A box holding `x` — or, if `x` is an infinity or a NaN, the error SBCL
/// would have signalled instead (decision 4). Every double Lisp can see was
/// made here, which is what makes "no infinities" true.
pub fn make<'h>(mc: &mut Mutation<'h>, x: f64) -> Result<Value<'h>, EvalError> {
    if x.is_nan() {
        return Err(fault(
            "FLOATING-POINT-INVALID-OPERATION",
            "floating-point invalid operation",
        ));
    }
    if x.is_infinite() {
        return Err(fault("FLOATING-POINT-OVERFLOW", "floating-point overflow"));
    }
    let v = mc.alloc(ObjectType::DoubleFloat, 1);
    mc.set_raw_slot(v, 0, x.to_bits());
    Ok(v)
}

fn fault(condition: &'static str, detail: &'static str) -> EvalError {
    EvalError::Arithmetic { condition, detail }
}

fn division_by_zero() -> EvalError {
    fault("DIVISION-BY-ZERO", "division by zero")
}

/// A number the Rust side can do arithmetic on: a fixnum or a double.
#[derive(Clone, Copy)]
enum Real {
    Int(i64),
    Float(f64),
}

impl Real {
    /// Float contagion (CLHS 12.1.4.1): a rational meeting a float becomes
    /// the nearest float first. `as` rounds to nearest, ties to even.
    fn float(self) -> f64 {
        match self {
            Real::Int(n) => n as f64,
            Real::Float(x) => x,
        }
    }
}

fn real<'h>(mc: &Mutation<'h>, v: Value<'h>) -> Option<Real> {
    match v.as_fixnum() {
        Some(n) => Some(Real::Int(n)),
        None if is(mc, v) => Some(Real::Float(get(mc, v))),
        None => None,
    }
}

/// The arguments as reals, or `None` if one is anything else (a bignum goes
/// to the Lisp fallback, anything else is a type error) or, unless
/// `all_ints_too`, if none is a double — then the fixnum path's own error
/// stands.
fn reals<'h>(mc: &Mutation<'h>, args: &[Value<'h>], all_ints_too: bool) -> Option<Vec<Real>> {
    let reals: Vec<Real> = args.iter().map(|a| real(mc, *a)).collect::<Option<_>>()?;
    let any_float = reals.iter().any(|r| matches!(r, Real::Float(_)));
    (any_float || all_ints_too).then_some(reals)
}

/// Which arithmetic operation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `rem`
    Rem,
}

/// `+ - * / rem` over fixnums and doubles, two at a time from the left as
/// SBCL does them, or `None` to leave the fixnum path's error standing.
///
/// `/` has no fixnum path of its own, so it comes here with fixnums alone
/// too: `(/ 6 3)` is `2`, and `(/ 1 3)` would be a ratio, which does not
/// exist yet (#59).
pub fn arithmetic<'h>(
    mc: &mut Mutation<'h>,
    op: Op,
    args: &[Value<'h>],
) -> Option<Result<Value<'h>, EvalError>> {
    let reals = reals(mc, args, op == Op::Div)?;
    let result = match reals.as_slice() {
        [only] if op == Op::Sub => combine(Op::Sub, Real::Int(0), *only),
        [only] if op == Op::Div => combine(Op::Div, Real::Int(1), *only),
        [first, rest @ ..] => rest.iter().try_fold(*first, |a, b| combine(op, a, *b)),
        [] => return None,
    };
    Some(result.and_then(|r| match r {
        Real::Int(n) => Value::fixnum(n).ok_or(EvalError::IntegerOverflow),
        Real::Float(x) => make(mc, x),
    }))
}

fn combine(op: Op, a: Real, b: Real) -> Result<Real, EvalError> {
    if let (Real::Int(x), Real::Int(y)) = (a, b) {
        let exact = match op {
            Op::Add => x.checked_add(y),
            Op::Sub => x.checked_sub(y),
            Op::Mul => x.checked_mul(y),
            Op::Div | Op::Rem if y == 0 => return Err(division_by_zero()),
            Op::Div if x.checked_rem(y).is_some_and(|r| r != 0) => {
                return Err(fault("ARITHMETIC-ERROR", "ratios do not exist yet (#59)"))
            }
            Op::Div => x.checked_div(y),
            Op::Rem => x.checked_rem(y),
        };
        // Overflow: the Lisp fallback starts again with bignums.
        return exact.map(Real::Int).ok_or(EvalError::IntegerOverflow);
    }
    let (x, y) = (a.float(), b.float());
    if matches!(op, Op::Div | Op::Rem) && y == 0.0 {
        return Err(division_by_zero());
    }
    let r = match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Div => x / y,
        // As SBCL computes it: the remainder after truncating the quotient.
        Op::Rem => x - (x / y).trunc() * y,
    };
    // Checked here as well as in `make`, so that an infinity part-way
    // through `(+ 1d308 1d308 -1d308)` is the error it is in SBCL.
    if r.is_finite() {
        Ok(Real::Float(r))
    } else {
        Err(fault("FLOATING-POINT-OVERFLOW", "floating-point overflow"))
    }
}

/// `= < >` over fixnums and doubles, chained, or `None` as [`arithmetic`].
///
/// **Exact**, as CLHS 12.1.4.1 requires: a float is compared as the rational
/// it is, not by converting the integer to a float. `(= (1+ (expt 2 60))
/// (float (expt 2 60)))` is false, though both sides make the same double.
pub fn compare<'h>(
    mc: &Mutation<'h>,
    args: &[Value<'h>],
    test: fn(Ordering) -> bool,
) -> Option<Result<Value<'h>, EvalError>> {
    let reals = reals(mc, args, false)?;
    let holds = reals.windows(2).all(|pair| test(order(pair[0], pair[1])));
    Some(Ok(if holds { Value::T } else { Value::NIL }))
}

fn order(a: Real, b: Real) -> Ordering {
    match (a, b) {
        (Real::Int(x), Real::Int(y)) => x.cmp(&y),
        // No NaN exists (decision 4), so every pair of doubles is ordered.
        (Real::Float(x), Real::Float(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Real::Int(n), Real::Float(x)) => int_with_float(n, x),
        (Real::Float(x), Real::Int(n)) => int_with_float(n, x).reverse(),
    }
}

/// An integer against a double, exactly: compare with the double's integer
/// part (exact, and inside `i64` once the out-of-range cases are gone), and
/// on a tie the fractional part decides.
fn int_with_float(n: i64, x: f64) -> Ordering {
    const TWO_63: f64 = 9_223_372_036_854_775_808.0;
    if x >= TWO_63 {
        return Ordering::Less;
    }
    if x < -TWO_63 {
        return Ordering::Greater;
    }
    let whole = x.trunc();
    n.cmp(&(whole as i64))
        .then_with(|| 0.0.partial_cmp(&(x - whole)).unwrap_or(Ordering::Equal))
}

/// A fixnum or double as a double: `%float`. A bignum is Lisp's to convert.
pub fn float<'h>(mc: &mut Mutation<'h>, v: Value<'h>) -> Result<Value<'h>, EvalError> {
    match real(mc, v) {
        Some(Real::Float(_)) => Ok(v),
        Some(Real::Int(n)) => make(mc, n as f64),
        None => Err(not_a_real("%FLOAT")),
    }
}

fn not_a_real(name: &'static str) -> EvalError {
    EvalError::WrongType {
        name,
        wanted: "a fixnum or a float",
    }
}

/// A fixnum or double's value as an `f64`, for the functions below.
pub fn value<'h>(mc: &Mutation<'h>, name: &'static str, v: Value<'h>) -> Result<f64, EvalError> {
    real(mc, v).map(Real::float).ok_or_else(|| not_a_real(name))
}

/// `f` of one number: `%sqrt`, `%sin` and the rest.
pub fn unary<'h>(
    mc: &mut Mutation<'h>,
    name: &'static str,
    v: Value<'h>,
    f: fn(f64) -> f64,
) -> Result<Value<'h>, EvalError> {
    let x = value(mc, name, v)?;
    make(mc, f(x))
}

/// The double rounded to an integral double: `mode` 0 towards zero, 1 down,
/// 2 up, 3 to nearest with ties to even — the four of CL's `ftruncate`,
/// `ffloor`, `fceiling` and `fround`. The sign of a zero survives, as it
/// does in SBCL: `(ftruncate -0.5)` is `-0.0`.
pub fn round(x: f64, mode: i64) -> f64 {
    match mode {
        0 => x.trunc(),
        1 => x.floor(),
        2 => x.ceil(),
        _ => x.round_ties_even(),
    }
}

/// An integral double as a fixnum, or `None` if it is too large for one
/// (Lisp then builds the bignum from [`decode`]).
pub fn integer<'h>(x: f64) -> Option<Value<'h>> {
    const TWO_62: f64 = 4_611_686_018_427_387_904.0;
    if x.abs() < TWO_62 {
        Value::fixnum(x as i64)
    } else {
        None
    }
}

/// CL's `integer-decode-float`: `(mantissa exponent sign)`, the double being
/// exactly `sign × mantissa × 2^exponent`. The 53-bit mantissa has its
/// implicit leading one put back, except for subnormals and zero, whose
/// exponent field is zero.
pub fn decode(x: f64) -> (i64, i64, i64) {
    let bits = x.to_bits();
    let sign = if bits >> 63 == 0 { 1 } else { -1 };
    let field = ((bits >> 52) & 0x7ff) as i64;
    let fraction = (bits & ((1 << 52) - 1)) as i64;
    if bits << 1 == 0 {
        // Zero, either sign: SBCL says `0 0 ±1`.
        (0, 0, sign)
    } else if field == 0 {
        (fraction, -1074, sign)
    } else {
        (fraction | (1 << 52), field - 1075, sign)
    }
}

/// `x × 2^k`, exact unless the result leaves the normal range.
pub fn scale(x: f64, k: i64) -> f64 {
    // Beyond ±2200 every result is zero or an overflow anyway, and `scalbn`
    // takes an `i32`.
    libm::scalbn(x, k.clamp(-2200, 2200) as i32)
}

/// A float token's text, already in Rust's syntax (`1.5e10`), as the nearest
/// double. Rust's parser rounds correctly — the decimal-to-binary problem
/// Clinger solved in 1990 with bignums, done here with Eisel and Lemire's
/// fast method and a big-decimal fallback.
pub fn parse<'h>(mc: &mut Mutation<'h>, text: &str) -> Result<Value<'h>, EvalError> {
    let x: f64 = text.parse().map_err(|_| EvalError::WrongType {
        name: "%PARSE-DOUBLE",
        wanted: "a float's digits",
    })?;
    make(mc, x)
}

/// The shortest decimal that reads back as `|x|`, as `d.ddde±n` — the
/// binary-to-decimal problem of Steele and White (1990). Rust's formatter
/// answers it with Grisu, falling back to Dragon4 where Grisu cannot be sure.
pub fn digits(x: f64) -> String {
    format!("{:e}", x.abs())
}
