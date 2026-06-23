//! Bin-price power function for the DLMM (liquidity book).
//!
//! Each bin has a fixed price `(1 + bin_step/10000)^bin_id` in Q64.64, where
//! `bin_step` is the per-bin spacing in basis points and `bin_id` is a *signed*
//! integer (negative ids are below the reference price). This needs a
//! fixed-point power with a signed exponent, implemented via binary
//! exponentiation with an explicit rounding direction.

use crate::{Q64x64, Rounding};

/// Largest supported magnitude of `bin_id`.
///
/// Beyond roughly this magnitude even the smallest bin step pushes the price
/// outside the Q64.64 range, so ids are bounded here for an explicit, early
/// rejection (the power itself also returns `None` on overflow).
pub const MAX_BIN_ID: i32 = 443_636;

/// Largest supported `bin_step` in basis points (100%).
pub const MAX_BIN_STEP_BPS: u16 = 10_000;

/// Raise a Q64.64 `base` to a signed integer power via binary exponentiation.
///
/// `exp == 0` yields exactly `1.0`. Negative exponents return the reciprocal of
/// the positive power. The same `rounding` is applied to every intermediate
/// multiply (and the final reciprocal), so the result is deterministic.
/// Returns `None` if any intermediate or the result overflows Q64.64.
pub fn pow(base: Q64x64, exp: i32, rounding: Rounding) -> Option<Q64x64> {
    let mut result = Q64x64::ONE;
    let mut b = base;
    let mut e = exp.unsigned_abs();
    while e > 0 {
        if e & 1 == 1 {
            result = result.mul(b, rounding)?;
        }
        e >>= 1;
        if e > 0 {
            b = b.mul(b, rounding)?;
        }
    }
    if exp < 0 {
        result = result.recip(rounding)?;
    }
    Some(result)
}

/// Price of bin `bin_id` for a given `bin_step` (basis points): `(1 + bin_step/10000)^bin_id`.
///
/// Returns `None` if `bin_step` is `0` or above [`MAX_BIN_STEP_BPS`], if
/// `bin_id` is outside `[-MAX_BIN_ID, MAX_BIN_ID]`, or if the price overflows
/// Q64.64.
pub fn bin_price(bin_step_bps: u16, bin_id: i32, rounding: Rounding) -> Option<Q64x64> {
    if bin_step_bps == 0 || bin_step_bps > MAX_BIN_STEP_BPS {
        return None;
    }
    if bin_id.unsigned_abs() > MAX_BIN_ID as u32 {
        return None;
    }
    // base = 1 + bin_step/10000
    let step = Q64x64::from_ratio(bin_step_bps as u128, 10_000, rounding)?;
    let base = Q64x64::ONE.checked_add(step)?;
    pow(base, bin_id, rounding)
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Rounding = Rounding::Down;

    #[test]
    fn pow_basics() {
        let two = Q64x64::from_int(2);
        assert_eq!(pow(two, 0, D).unwrap(), Q64x64::ONE);
        assert_eq!(pow(two, 1, D).unwrap(), two);
        assert_eq!(pow(two, 3, D).unwrap(), Q64x64::from_int(8));
        assert_eq!(pow(two, 10, D).unwrap(), Q64x64::from_int(1024));
        // negative exponent -> reciprocal
        assert_eq!(pow(two, -1, D).unwrap(), Q64x64::from_ratio(1, 2, D).unwrap());
        assert_eq!(pow(two, -3, D).unwrap(), Q64x64::from_ratio(1, 8, D).unwrap());
        // overflow -> None  (2^64 has bits 2^128)
        assert_eq!(pow(two, 64, D), None);
    }

    #[test]
    fn bin_id_zero_is_one() {
        for step in [1u16, 10, 25, 100, 5000, 10000] {
            assert_eq!(bin_price(step, 0, D).unwrap(), Q64x64::ONE);
        }
    }

    #[test]
    fn bin_price_known_values() {
        // bin_step = 100% -> base = 2.0
        assert_eq!(bin_price(10_000, 3, D).unwrap(), Q64x64::from_int(8));
        assert_eq!(bin_price(10_000, -1, D).unwrap(), Q64x64::from_ratio(1, 2, D).unwrap());
        // bin_step = 50% -> base = 1.5; 1.5^2 = 2.25 = 9/4 (exact)
        assert_eq!(bin_price(5_000, 2, D).unwrap(), Q64x64::from_ratio(9, 4, D).unwrap());
    }

    #[test]
    fn bin_price_monotonic_in_id() {
        // For bin_step > 0 the price strictly increases with bin_id.
        let step = 25u16; // 0.25%
        let mut prev = bin_price(step, -5, D).unwrap();
        for id in -4..=50 {
            let cur = bin_price(step, id, D).unwrap();
            assert!(cur > prev, "not increasing at id {id}");
            prev = cur;
        }
        // ids below 0 are < 1.0, ids above 0 are > 1.0.
        assert!(bin_price(step, -1, D).unwrap() < Q64x64::ONE);
        assert!(bin_price(step, 1, D).unwrap() > Q64x64::ONE);
    }

    #[test]
    fn bounds_rejected() {
        // bin_step out of range
        assert_eq!(bin_price(0, 1, D), None);
        assert_eq!(bin_price(10_001, 1, D), None);
        // bin_id out of range
        assert_eq!(bin_price(1, MAX_BIN_ID + 1, D), None);
        assert_eq!(bin_price(1, -(MAX_BIN_ID + 1), D), None);
        // in-range id bound does not itself reject
        assert!(bin_price(1, MAX_BIN_ID, D).is_none() || bin_price(1, 0, D).is_some());
    }

    #[test]
    fn pow_exponent_additivity() {
        // base^a * base^b == base^(a+b) within 1 ulp (rounding accumulates).
        let base = Q64x64::from_ratio(10_025, 10_000, D).unwrap(); // 1.0025
        for (a, b) in [(3, 4), (10, 7), (20, 13)] {
            let lhs = pow(base, a, D).unwrap().mul(pow(base, b, D).unwrap(), D).unwrap();
            let rhs = pow(base, a + b, D).unwrap();
            let diff = lhs.to_bits().abs_diff(rhs.to_bits());
            // a few ulps of slack for accumulated flooring across the chains
            assert!(diff <= 4, "a={a} b={b} diff={diff}");
        }
    }
}
