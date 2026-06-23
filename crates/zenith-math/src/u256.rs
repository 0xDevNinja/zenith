//! 256-bit intermediate arithmetic for 128-bit fixed-point values.
//!
//! Multiplying two `u128` values can need up to 256 bits, and Q64.64 scaling
//! shifts by 64/128 bits. These helpers do the work in `ruint::U256` and narrow
//! back to `u128`, returning [`MathError::Overflow`] instead of wrapping and
//! [`MathError::DivByZero`] instead of panicking. Every lossy result takes an
//! explicit [`Rounding`] so callers choose the protocol-favoring direction.
//!
//! This module is the single home for the U256 narrowing and rounding logic;
//! [`crate::Q64x64`] builds on it rather than re-implementing it.

use ruint::aliases::U256;

use crate::Rounding;

/// Failure modes for the 256-bit helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    /// The result does not fit in `u128`.
    Overflow,
    /// Division (or modular reduction) by zero.
    DivByZero,
}

/// Result alias for the math helpers.
pub type MathResult<T> = Result<T, MathError>;

/// Narrow a `U256` back to `u128`, or [`MathError::Overflow`] if it does not fit.
#[inline]
pub(crate) fn to_u128(x: U256) -> MathResult<u128> {
    let limbs = x.as_limbs();
    if limbs[2] != 0 || limbs[3] != 0 {
        return Err(MathError::Overflow);
    }
    Ok((limbs[0] as u128) | ((limbs[1] as u128) << 64))
}

/// `2^shift` as a `U256`. `shift` must be `< 256` (caller-guaranteed).
#[inline]
pub(crate) fn pow2(shift: u32) -> U256 {
    U256::from(1u128) << (shift as usize)
}

/// `num / den` over `U256`, applying the rounding direction to a nonzero
/// remainder. `den` must be nonzero (caller-guaranteed).
#[inline]
pub(crate) fn div_round(num: U256, den: U256, rounding: Rounding) -> U256 {
    let q = num / den;
    match rounding {
        Rounding::Up if num % den != U256::ZERO => q + U256::from(1u128),
        _ => q,
    }
}

/// Compute `(a * b) / denom` using a 256-bit intermediate, rounded as requested.
///
/// The product `a * b` is exact in 256 bits (`(2^128-1)^2 < 2^256`). Returns
/// [`MathError::DivByZero`] if `denom == 0`, or [`MathError::Overflow`] if the
/// quotient exceeds `u128`.
#[inline]
pub fn mul_div(a: u128, b: u128, denom: u128, rounding: Rounding) -> MathResult<u128> {
    if denom == 0 {
        return Err(MathError::DivByZero);
    }
    let num = U256::from(a) * U256::from(b);
    to_u128(div_round(num, U256::from(denom), rounding))
}

/// Compute `(a * b) >> shift` using a 256-bit intermediate, rounded as requested.
///
/// Useful for Q64.64 multiply (`shift = 64`). `shift` must be `< 256`; larger
/// shifts return [`MathError::Overflow`]. Returns [`MathError::Overflow`] if the
/// result exceeds `u128`.
#[inline]
pub fn mul_shr(a: u128, b: u128, shift: u32, rounding: Rounding) -> MathResult<u128> {
    if shift >= 256 {
        return Err(MathError::Overflow);
    }
    let num = U256::from(a) * U256::from(b);
    to_u128(div_round(num, pow2(shift), rounding))
}

/// Compute `(a << shift) / denom` using a 256-bit intermediate, rounded as
/// requested.
///
/// Useful for Q64.64 divide (`shift = 64`). `shift` must be `<= 128` so the
/// shifted numerator cannot exceed 256 bits; larger shifts return
/// [`MathError::Overflow`]. Returns [`MathError::DivByZero`] if `denom == 0`,
/// or [`MathError::Overflow`] if the quotient exceeds `u128`.
#[inline]
pub fn shl_div(a: u128, shift: u32, denom: u128, rounding: Rounding) -> MathResult<u128> {
    if denom == 0 {
        return Err(MathError::DivByZero);
    }
    // a <= 2^128-1, so a << 128 <= (2^128-1)*2^128 < 2^256 — fits U256 with no
    // bit loss. Reject larger shifts that could wrap.
    if shift > 128 {
        return Err(MathError::Overflow);
    }
    let num = U256::from(a) << (shift as usize);
    to_u128(div_round(num, U256::from(denom), rounding))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const D: Rounding = Rounding::Down;
    const U: Rounding = Rounding::Up;

    #[test]
    fn mul_div_basic() {
        // 6 * 7 / 3 = 14
        assert_eq!(mul_div(6, 7, 3, D).unwrap(), 14);
        // exact: Up == Down
        assert_eq!(mul_div(6, 7, 3, U).unwrap(), 14);
        // div by zero
        assert_eq!(mul_div(1, 1, 0, D), Err(MathError::DivByZero));
    }

    #[test]
    fn mul_div_rounding() {
        // 10 / 3 = 3.33 -> Down 3, Up 4
        assert_eq!(mul_div(10, 1, 3, D).unwrap(), 3);
        assert_eq!(mul_div(10, 1, 3, U).unwrap(), 4);
    }

    #[test]
    fn mul_div_no_intermediate_overflow() {
        // a*b overflows u128 but the quotient fits: result must be exact.
        // (2^127) * 4 / 2 = 2^128 -> overflow of result
        assert_eq!(mul_div(1u128 << 127, 4, 2, D), Err(MathError::Overflow));
        // (2^127) * 2 / 4 = 2^126 -> fits, and only works via 256-bit intermediate
        assert_eq!(mul_div(1u128 << 127, 2, 4, D).unwrap(), 1u128 << 126);
        // MAX * MAX / MAX == MAX (intermediate is 256-bit)
        assert_eq!(mul_div(u128::MAX, u128::MAX, u128::MAX, D).unwrap(), u128::MAX);
    }

    #[test]
    fn mul_shr_basic() {
        // (3 << 64) >> 64 = 3
        assert_eq!(mul_shr(3u128 << 64, 1, 64, D).unwrap(), 3);
        // (1 * 1) >> 1 = 0 down, 1 up
        assert_eq!(mul_shr(1, 1, 1, D).unwrap(), 0);
        assert_eq!(mul_shr(1, 1, 1, U).unwrap(), 1);
        // shift too large
        assert_eq!(mul_shr(1, 1, 256, D), Err(MathError::Overflow));
        // result overflow: (2^127 * 4) >> 1 = 2^128
        assert_eq!(mul_shr(1u128 << 127, 4, 1, D), Err(MathError::Overflow));
    }

    #[test]
    fn shl_div_basic() {
        // (1 << 64) / (1 << 64) = 1
        assert_eq!(shl_div(1, 64, 1u128 << 64, D).unwrap(), 1);
        // (6 << 1) / 4 = 3
        assert_eq!(shl_div(6, 1, 4, D).unwrap(), 3);
        // (1 << 1) / 3 = 0 down, 1 up
        assert_eq!(shl_div(1, 1, 3, D).unwrap(), 0);
        assert_eq!(shl_div(1, 1, 3, U).unwrap(), 1);
        // div by zero, shift too large
        assert_eq!(shl_div(1, 64, 0, D), Err(MathError::DivByZero));
        assert_eq!(shl_div(1, 129, 1, D), Err(MathError::Overflow));
        // result overflow: (2^65 << 64) / 1 = 2^129
        assert_eq!(shl_div(1u128 << 65, 64, 1, D), Err(MathError::Overflow));
    }

    // Reference: u128 mul_div via u128 only works when a*b fits; use a wider
    // check via the same U256 path is circular, so compare against the
    // mathematically-derived floor/ceil using checked u128 where it fits.
    proptest! {
        // mul_div matches floor/ceil of (a*b)/denom for inputs whose product
        // fits u128 (verifiable with native arithmetic).
        #[test]
        fn mul_div_matches_native(a in 0u64..=u64::MAX, b in 0u64..=u64::MAX, denom in 1u128..=u128::MAX) {
            let (a, b) = (a as u128, b as u128);
            let prod = a * b; // u64*u64 fits u128
            let floor = prod / denom;
            let ceil = if prod % denom != 0 { floor + 1 } else { floor };
            prop_assert_eq!(mul_div(a, b, denom, D).unwrap(), floor);
            prop_assert_eq!(mul_div(a, b, denom, U).unwrap(), ceil);
        }

        // Up is never below Down and differs by at most one.
        #[test]
        fn mul_div_rounding_direction(a in any::<u128>(), b in any::<u128>(), denom in 1u128..=u128::MAX) {
            if let (Ok(d), Ok(u)) = (mul_div(a, b, denom, D), mul_div(a, b, denom, U)) {
                prop_assert!(u >= d);
                prop_assert!(u - d <= 1);
            }
        }

        // mul_shr with shift 0 is just the product when it fits.
        #[test]
        fn mul_shr_shift0(a in 0u64..=u64::MAX, b in 0u64..=u64::MAX) {
            let (a, b) = (a as u128, b as u128);
            prop_assert_eq!(mul_shr(a, b, 0, D).unwrap(), a * b);
        }
    }
}
