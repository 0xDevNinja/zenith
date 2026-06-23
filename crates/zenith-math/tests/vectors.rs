//! Reference vectors and property tests for `zenith-math`, exercised through
//! the public API only.
//!
//! The golden vectors below are the values any second implementation (notably
//! the TypeScript SDK port) must reproduce bit-for-bit. The property tests pin
//! the invariants the programs rely on: exact round trips, rounding bounded to
//! one ulp, floored square root, and strictly monotonic bin pricing.

use proptest::prelude::*;
use zenith_math::{
    bin_price, delta_a, delta_b, mul_div, mul_shr, price_from_sqrt_price, shl_div,
    sqrt_price_from_price, sqrt_u128, Q64x64, Rounding, MAX_PRICE_BITS,
};

const D: Rounding = Rounding::Down;
const U: Rounding = Rounding::Up;

// ---------- golden vectors ----------

#[test]
fn q64_reference_vectors() {
    assert_eq!(Q64x64::ONE.to_bits(), 1u128 << 64);
    assert_eq!(Q64x64::from_int(5).to_bits(), 5u128 << 64);
    assert_eq!(Q64x64::from_ratio(1, 2, D).unwrap().to_bits(), 1u128 << 63);
    assert_eq!(Q64x64::from_ratio(1, 4, D).unwrap().to_bits(), 1u128 << 62);
    assert_eq!(Q64x64::from_ratio(3, 2, D).unwrap().to_bits(), 3u128 << 63);
    // 1/3 is inexact: Down floors, Up is one ulp higher.
    assert_eq!(
        Q64x64::from_ratio(1, 3, D).unwrap().to_bits(),
        6_148_914_691_236_517_205
    );
    assert_eq!(
        Q64x64::from_ratio(1, 3, U).unwrap().to_bits(),
        6_148_914_691_236_517_206
    );
}

#[test]
fn mul_div_reference_vectors() {
    assert_eq!(mul_div(6, 7, 3, D).unwrap(), 14);
    assert_eq!(mul_div(10, 1, 3, D).unwrap(), 3);
    assert_eq!(mul_div(10, 1, 3, U).unwrap(), 4);
    // full-width: only correct via the 256-bit intermediate.
    assert_eq!(
        mul_div(u128::MAX, u128::MAX, u128::MAX, D).unwrap(),
        u128::MAX
    );
    assert_eq!(mul_shr(3u128 << 64, 1, 64, D).unwrap(), 3);
    assert_eq!(shl_div(6, 1, 4, D).unwrap(), 3);
}

#[test]
fn sqrt_reference_vectors() {
    let squares = [(0u128, 0u128), (1, 1), (4, 2), (9, 3), (16, 4)];
    for (x, r) in squares {
        assert_eq!(sqrt_u128(x), r);
    }
    assert_eq!(sqrt_u128(1u128 << 64), 1u128 << 32);
    assert_eq!(sqrt_u128(u128::MAX), (1u128 << 64) - 1);
    assert_eq!(sqrt_u128(1_000_000_000_000_000_000), 1_000_000_000);
    // floored non-squares
    for (x, r) in [(2u128, 1u128), (3, 1), (8, 2), (15, 3), (99, 9)] {
        assert_eq!(sqrt_u128(x), r);
    }
}

#[test]
fn sqrt_price_reference_vectors() {
    // price 4 -> sqrt price 2.0 (bits 2^65); inverse back to 4.
    let sp = sqrt_price_from_price(4, 1).unwrap();
    assert_eq!(sp.to_bits(), 2u128 << 64);
    assert_eq!(price_from_sqrt_price(sp, D).unwrap(), Q64x64::from_int(4));
    assert_eq!(sqrt_price_from_price(1, 1).unwrap(), Q64x64::ONE);

    // L = 1000 over S in [1, 2]: delta_a = 500, delta_b = 1000.
    let lo = Q64x64::from_bits(1u128 << 64);
    let hi = Q64x64::from_bits(1u128 << 65);
    assert_eq!(delta_a(1000, lo, hi, D).unwrap(), 500);
    assert_eq!(delta_b(1000, lo, hi, D).unwrap(), 1000);
}

#[test]
fn bin_price_reference_vectors() {
    assert_eq!(bin_price(10_000, 0, D).unwrap(), Q64x64::ONE);
    assert_eq!(bin_price(10_000, 3, D).unwrap(), Q64x64::from_int(8));
    assert_eq!(
        bin_price(10_000, -1, D).unwrap(),
        Q64x64::from_ratio(1, 2, D).unwrap()
    );
    assert_eq!(
        bin_price(5_000, 2, D).unwrap(),
        Q64x64::from_ratio(9, 4, D).unwrap()
    );
    // band edge.
    assert_eq!(bin_price(10_000, 32, D).unwrap().to_bits(), MAX_PRICE_BITS);
    // a realistic step relation: bin 1 at 0.10% = 1 + 1/1000.
    assert_eq!(
        bin_price(10, 1, D).unwrap(),
        Q64x64::ONE
            .checked_add(Q64x64::from_ratio(1, 1000, D).unwrap())
            .unwrap()
    );
}

// ---------- property tests ----------

proptest! {
    // sqrt is the exact floor.
    #[test]
    fn prop_sqrt_is_floor(x in any::<u128>()) {
        let z = sqrt_u128(x);
        // z^2 <= x  (z*z cannot overflow: z <= 2^64-1, so z*z <= ~2^128-2^65 < u128::MAX)
        prop_assert!(z * z <= x);
        // (z+1)^2 > x  (if it overflows u128 it is certainly > x)
        if let Some(z1) = (z + 1).checked_mul(z + 1) {
            prop_assert!(z1 > x);
        }
    }

    // mul_div rounding is bounded to one ulp and Up never below Down.
    #[test]
    fn prop_mul_div_rounding_bound(a in any::<u128>(), b in any::<u128>(), d in 1u128..=u128::MAX) {
        if let (Ok(lo), Ok(hi)) = (mul_div(a, b, d, D), mul_div(a, b, d, U)) {
            prop_assert!(hi >= lo && hi - lo <= 1);
        }
    }

    // sqrt_price then square recovers the price from below, within the
    // floored-sqrt error bound.
    #[test]
    fn prop_sqrt_price_round_trip(num in 1u128..=u64::MAX as u128) {
        let sp = sqrt_price_from_price(num, 1).unwrap();
        let price = price_from_sqrt_price(sp, D).unwrap();
        let expected = num << 64; // num as Q64.64 (fits: num <= 2^64 - 1)
        // sp = floor(sqrt(num) * 2^64) <= true value, so squaring never exceeds.
        prop_assert!(price.to_bits() <= expected);
        // gap is bounded by the sqrt rounding (<= 2*sp + 1 ulps, conservative).
        prop_assert!(expected - price.to_bits() <= 2 * sp.to_bits() + 1);
    }

    // bin price is strictly increasing in bin_id across the valid band.
    #[test]
    fn prop_bin_price_monotonic(step in 1u16..=2_000u16, start in -2_000i32..=1_900i32) {
        // sample a short ascending run; all within the price band for these steps.
        let mut prev: Option<Q64x64> = None;
        for id in start..start + 20 {
            if let Some(p) = bin_price(step, id, D) {
                if let Some(pp) = prev {
                    prop_assert!(p > pp, "step={step} id={id}");
                }
                prev = Some(p);
            } else {
                prev = None; // left the band; stop comparing across the gap
            }
        }
    }

    // delta_a / delta_b rounding bounded to one ulp.
    #[test]
    fn prop_delta_rounding_bound(
        l in 1u128..=u64::MAX as u128,
        a in 1u128..=u64::MAX as u128,
        b in 1u128..=u64::MAX as u128,
    ) {
        let (sa, sb) = (Q64x64::from_bits(a), Q64x64::from_bits(b));
        if let (Some(lo), Some(hi)) = (delta_a(l, sa, sb, D), delta_a(l, sa, sb, U)) {
            prop_assert!(hi >= lo && hi - lo <= 1);
        }
        if let (Some(lo), Some(hi)) = (delta_b(l, sa, sb, D), delta_b(l, sa, sb, U)) {
            prop_assert!(hi >= lo && hi - lo <= 1);
        }
    }
}
