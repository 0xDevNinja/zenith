//! Bin-price power function for the DLMM (liquidity book).
//!
//! Each bin has a fixed price `(1 + bin_step/10000)^bin_id` in Q64.64, where
//! `bin_step` is the per-bin spacing in basis points and `bin_id` is a *signed*
//! integer (negative ids are below the reference price). This needs a
//! fixed-point power with a signed exponent, implemented via binary
//! exponentiation with an explicit rounding direction.
//!
//! ## Valid price range
//!
//! Q64.64 has 64 fractional bits, so very small prices lose the resolution
//! needed to keep adjacent bins distinct (a deeply negative `bin_id` would make
//! `base^bin_id` underflow and several bins would share one price, breaking the
//! bin <-> price bijection the book relies on). [`bin_price`] therefore only
//! returns a price inside the band `[2^-32, 2^32]` (see [`MIN_PRICE_BITS`] /
//! [`MAX_PRICE_BITS`]); outside it returns `None`. Within the band every
//! supported `bin_step` keeps consecutive bins more than one ulp apart, so the
//! price is strictly increasing in `bin_id` on both the positive and negative
//! side. This spans ~19 orders of magnitude of price — ample for real pairs.

use crate::{Q64x64, Rounding};

/// Largest supported `bin_step` in basis points (100%).
pub const MAX_BIN_STEP_BPS: u16 = 10_000;

/// Smallest bin price kept, `2^-32` (Q64.64 bits `1 << 32`). Below this the
/// fraction has too few significant bits to keep adjacent bins distinct.
pub const MIN_PRICE_BITS: u128 = 1u128 << 32;

/// Largest bin price kept, `2^32` (Q64.64 bits `1 << 96`).
pub const MAX_PRICE_BITS: u128 = 1u128 << 96;

/// Raise a Q64.64 `base` to a signed integer power via binary exponentiation.
///
/// `exp == 0` yields exactly `1.0`. Negative exponents return the reciprocal of
/// the positive power. The same `rounding` is applied to every intermediate
/// multiply and the final reciprocal, so the result is deterministic — but note
/// `rounding` here is a precision knob, not a safety lever (the consumer that
/// turns a price into a token amount picks the protocol-favoring direction at
/// that step). The reciprocal path is lossy for large `|exp|`; [`bin_price`]
/// bounds the range so this never degrades into non-distinct prices. Returns
/// `None` if any intermediate or the result overflows Q64.64.
pub fn pow(base: Q64x64, exp: i32, rounding: Rounding) -> Option<Q64x64> {
    let mut result = Q64x64::ONE;
    let mut b = base;
    let mut e = exp.unsigned_abs(); // i32::MIN-safe (no abs() panic)
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
/// Returns `None` if `bin_step` is `0` or above [`MAX_BIN_STEP_BPS`], or if the
/// resulting price falls outside `[2^-32, 2^32]` (see the module docs) — which
/// also bounds the usable `bin_id` range per step. Within that band the price
/// is strictly monotonic in `bin_id`.
pub fn bin_price(bin_step_bps: u16, bin_id: i32, rounding: Rounding) -> Option<Q64x64> {
    if bin_step_bps == 0 || bin_step_bps > MAX_BIN_STEP_BPS {
        return None;
    }
    // base = 1 + bin_step/10000
    let step = Q64x64::from_ratio(bin_step_bps as u128, 10_000, rounding)?;
    let base = Q64x64::ONE.checked_add(step)?;
    let price = pow(base, bin_id, rounding)?;
    if !(MIN_PRICE_BITS..=MAX_PRICE_BITS).contains(&price.to_bits()) {
        return None;
    }
    Some(price)
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
        assert_eq!(
            pow(two, -1, D).unwrap(),
            Q64x64::from_ratio(1, 2, D).unwrap()
        );
        assert_eq!(
            pow(two, -3, D).unwrap(),
            Q64x64::from_ratio(1, 8, D).unwrap()
        );
        // overflow -> None  (2^64 has bits 2^128)
        assert_eq!(pow(two, 64, D), None);
    }

    #[test]
    fn pow_extreme_exponents_no_panic() {
        // i32::MIN must not panic in unsigned_abs(); base 1.0 stays 1.0.
        assert_eq!(pow(Q64x64::ONE, i32::MIN, D).unwrap(), Q64x64::ONE);
        assert_eq!(pow(Q64x64::ONE, i32::MAX, D).unwrap(), Q64x64::ONE);
        // base > 1 at extreme exponent overflows/underflows -> None, no panic.
        assert_eq!(pow(Q64x64::from_int(2), i32::MAX, D), None);
        assert_eq!(pow(Q64x64::from_int(2), i32::MIN, D), None);
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
        assert_eq!(
            bin_price(10_000, -1, D).unwrap(),
            Q64x64::from_ratio(1, 2, D).unwrap()
        );
        // bin_step = 50% -> base = 1.5; 1.5^2 = 2.25 = 9/4 (exact)
        assert_eq!(
            bin_price(5_000, 2, D).unwrap(),
            Q64x64::from_ratio(9, 4, D).unwrap()
        );
    }

    #[test]
    fn price_band_edges() {
        // base = 2.0: 2^32 sits exactly at MAX_PRICE_BITS (inclusive).
        assert_eq!(bin_price(10_000, 32, D).unwrap().to_bits(), MAX_PRICE_BITS);
        // 2^33 is above the band -> None.
        assert_eq!(bin_price(10_000, 33, D), None);
        // 2^-32 sits exactly at MIN_PRICE_BITS (inclusive).
        assert_eq!(bin_price(10_000, -32, D).unwrap().to_bits(), MIN_PRICE_BITS);
        // 2^-33 is below the band -> None.
        assert_eq!(bin_price(10_000, -33, D), None);
    }

    #[test]
    fn bounds_rejected() {
        // bin_step out of range
        assert_eq!(bin_price(0, 1, D), None);
        assert_eq!(bin_price(10_001, 1, D), None);
        // ids whose price leaves the band -> None (concrete, not tautological)
        assert_eq!(bin_price(25, 9_000, D), None); // > 2^32
        assert_eq!(bin_price(25, -9_000, D), None); // < 2^-32
                                                    // i32 extremes never panic, just None
        assert_eq!(bin_price(1, i32::MIN, D), None);
        assert_eq!(bin_price(1, i32::MAX, D), None);
    }

    #[test]
    fn strictly_monotonic_across_band_both_sides() {
        // step = 25 (0.25%): band edges are near id +/- 8883. Walk a wide range
        // that covers the deep-negative region the old code got wrong, and
        // assert the price strictly increases every step.
        let step = 25u16;
        let mut prev = bin_price(step, -8000, D).unwrap();
        let mut id = -7999;
        while id <= 8000 {
            let cur = bin_price(step, id, D).unwrap();
            assert!(cur > prev, "not strictly increasing at id {id}");
            prev = cur;
            id += 1;
        }
        // ids below 0 are < 1.0, ids above 0 are > 1.0.
        assert!(bin_price(step, -1, D).unwrap() < Q64x64::ONE);
        assert!(bin_price(step, 1, D).unwrap() > Q64x64::ONE);
    }

    #[test]
    fn smallest_step_neighbors_distinct_deep_negative() {
        // step = 1 (0.01%): the worst case for resolution. Adjacent in-band
        // bins must still differ. 2^-32 for step 1 is near id -221806; pick a
        // deep but in-band pair.
        let a = bin_price(1, -200_000, D).unwrap();
        let b = bin_price(1, -199_999, D).unwrap();
        assert!(b > a, "smallest-step deep-negative bins collapsed");
    }

    #[test]
    fn pow_exponent_additivity() {
        // base^a * base^b == base^(a+b) within a few ulps (rounding accumulates).
        let base = Q64x64::from_ratio(10_025, 10_000, D).unwrap(); // 1.0025
        for (a, b) in [(3, 4), (10, 7), (20, 13)] {
            let lhs = pow(base, a, D)
                .unwrap()
                .mul(pow(base, b, D).unwrap(), D)
                .unwrap();
            let rhs = pow(base, a + b, D).unwrap();
            let diff = lhs.to_bits().abs_diff(rhs.to_bits());
            assert!(diff <= 4, "a={a} b={b} diff={diff}");
        }
    }
}
