//! Square root and sqrt-price helpers for the concentrated-liquidity AMM.
//!
//! Prices are tracked as a Q64.64 *square root* of the price (`sqrt_price`), the
//! Uniswap-v3 trick that makes swap stepping linear. This module provides:
//! - a deterministic, floored integer square root over `U256`,
//! - conversions between a price ratio and `sqrt_price`,
//! - the liquidity/amount deltas (`delta_a`, `delta_b`), and
//! - next-`sqrt_price` helpers for the amount-in / amount-out swap branches.
//!
//! Token `x` is the base (amount0); token `y` is the quote (amount1). Price is
//! `y / x`, so `sqrt_price = sqrt(y/x)`. `delta_a` is an `x` amount, `delta_b`
//! is a `y` amount.
//!
//! `delta_a` and the next-price helpers multiply three ~128-bit values, which
//! can need up to 320 bits, so they compute in `U512` and narrow back to
//! `u128`, returning `None` on overflow. Each helper documents the rounding
//! direction callers should pick to favor the pool.

use ruint::aliases::{U256, U512};

use crate::u256::to_u128;
use crate::{shl_div, Q64x64, Rounding, SCALE_OFFSET};

/// Floored integer square root of a `U256` (`floor(sqrt(x))`).
///
/// Deterministic Newton iteration from a power-of-two over-estimate. The result
/// always fits `u128`: `floor(sqrt(2^256 - 1)) = 2^128 - 1`, exactly the u128
/// ceiling.
pub fn sqrt_u256(x: U256) -> U256 {
    if x.is_zero() {
        return U256::ZERO;
    }
    // 2^ceil(bits/2) is a power of two >= sqrt(x): a safe Newton over-estimate.
    let bits = 256 - x.leading_zeros();
    let mut z = U256::from(1u128) << bits.div_ceil(2);
    loop {
        let next = (z + x / z) >> 1;
        if next >= z {
            break;
        }
        z = next;
    }
    z
}

/// Floored integer square root of a `u128`.
pub fn sqrt_u128(x: u128) -> u128 {
    // Result always fits u128 (see [`sqrt_u256`]).
    to_u128(sqrt_u256(U256::from(x))).expect("sqrt result fits u128")
}

/// `sqrt_price` (Q64.64) for the price ratio `num / den` (i.e. `y / x`).
///
/// Computes `floor(sqrt(num/den)) * 2^64 = floor(sqrt((num << 128) / den))`.
/// Rounded down. Returns `None` if `den == 0`.
pub fn sqrt_price_from_price(num: u128, den: u128) -> Option<Q64x64> {
    if den == 0 {
        return None;
    }
    // num < 2^128, so num << 128 < 2^256: fits U256 with no loss.
    let scaled = (U256::from(num) << 128) / U256::from(den);
    to_u128(sqrt_u256(scaled)).ok().map(Q64x64::from_bits)
}

/// Price ratio (Q64.64) implied by a `sqrt_price`: `price = sqrt_price^2`.
///
/// `bits = (sp^2) >> 64`. This is a pure conversion with no inherent
/// pool/trader side, so the caller picks `rounding` to match its use-site
/// (round the value the way that favors the pool for that context). Returns
/// `None` if the price exceeds `u128`.
pub fn price_from_sqrt_price(sqrt_price: Q64x64, rounding: Rounding) -> Option<Q64x64> {
    crate::mul_shr(
        sqrt_price.to_bits(),
        sqrt_price.to_bits(),
        SCALE_OFFSET,
        rounding,
    )
    .ok()
    .map(Q64x64::from_bits)
}

/// Amount of token `x` (base) between two sqrt prices for liquidity `L`.
///
/// `delta_a = L * (1/√P_lo - 1/√P_hi) = L * 2^64 * (sp_hi - sp_lo) / (sp_lo * sp_hi)`.
/// Round **up** when this is an input the trader must provide; **down** when it
/// is an amount the pool pays out. Returns `None` on overflow or zero price.
pub fn delta_a(
    liquidity: u128,
    sqrt_a: Q64x64,
    sqrt_b: Q64x64,
    rounding: Rounding,
) -> Option<u128> {
    let (lo, hi) = order(sqrt_a, sqrt_b);
    if lo == 0 {
        return None;
    }
    let diff = hi - lo; // ordered, so no underflow
                        // num = L * diff * 2^64  (<= 2^320),  den = sp_lo * sp_hi  (<= 2^256)
    let num = (U512::from(liquidity) * U512::from(diff)) << SCALE_OFFSET;
    let den = U512::from(lo) * U512::from(hi);
    narrow_512(div_round_512(num, den, rounding))
}

/// Amount of token `y` (quote) between two sqrt prices for liquidity `L`.
///
/// `delta_b = L * (√P_hi - √P_lo) = L * (sp_hi - sp_lo) / 2^64`.
/// Same rounding guidance as [`delta_a`]. Returns `None` on overflow.
pub fn delta_b(
    liquidity: u128,
    sqrt_a: Q64x64,
    sqrt_b: Q64x64,
    rounding: Rounding,
) -> Option<u128> {
    let (lo, hi) = order(sqrt_a, sqrt_b);
    let diff = hi - lo;
    crate::mul_shr(liquidity, diff, SCALE_OFFSET, rounding).ok()
}

/// Next `sqrt_price` after adding (`add = true`) or removing (`add = false`)
/// `amount` of token `x` (base / amount0). Adding `x` lowers the price.
///
/// `sp' = L * sp * 2^64 / (L * 2^64 ± amount * sp)` (`+` for add, `-` for remove).
///
/// Rounding is **not** a caller choice: the price is always rounded **up**,
/// which is the protocol-favoring direction for both branches (less `y` out on
/// add, more `y` charged on remove), so rounding can never leak value to the
/// trader. Returns `None` on overflow, zero liquidity, or a removal that would
/// empty the range.
pub fn next_sqrt_price_from_amount_x(
    sqrt_price: Q64x64,
    liquidity: u128,
    amount: u128,
    add: bool,
) -> Option<Q64x64> {
    if liquidity == 0 {
        return None;
    }
    let sp = U512::from(sqrt_price.to_bits());
    let l = U512::from(liquidity);
    let product = U512::from(amount) * sp;
    let l_shifted = l << SCALE_OFFSET; // L * 2^64
    let den = if add {
        l_shifted + product // >= 2^64 > 0
    } else {
        if l_shifted <= product {
            return None; // price would hit zero or invert
        }
        l_shifted - product // > 0 by the guard above
    };
    let num = (l * sp) << SCALE_OFFSET; // L * sp * 2^64
    narrow_512(div_round_512(num, den, Rounding::Up)).map(Q64x64::from_bits)
}

/// Next `sqrt_price` after adding (`add = true`) or removing (`add = false`)
/// `amount` of token `y` (quote / amount1). Adding `y` raises the price.
///
/// `sp' = sp ± (amount << 64) / L`.
///
/// Rounding is chosen internally per branch to favor the pool: **down** on add
/// (the price rises less, so less `x` out) and **up** on remove (the price
/// falls more, so more `x` charged). Returns `None` on overflow or zero
/// liquidity, or a removal that would drive the price below zero.
pub fn next_sqrt_price_from_amount_y(
    sqrt_price: Q64x64,
    liquidity: u128,
    amount: u128,
    add: bool,
) -> Option<Q64x64> {
    if liquidity == 0 {
        return None;
    }
    let rounding = if add { Rounding::Down } else { Rounding::Up };
    let delta = Q64x64::from_bits(shl_div(amount, SCALE_OFFSET, liquidity, rounding).ok()?);
    if add {
        sqrt_price.checked_add(delta)
    } else {
        sqrt_price.checked_sub(delta)
    }
}

/// Order two sqrt prices as `(low_bits, high_bits)`.
#[inline]
fn order(a: Q64x64, b: Q64x64) -> (u128, u128) {
    let (a, b) = (a.to_bits(), b.to_bits());
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// `num / den` over `U512`, applying the rounding direction to a nonzero
/// remainder. `den` must be nonzero (caller-guaranteed).
#[inline]
fn div_round_512(num: U512, den: U512, rounding: Rounding) -> U512 {
    let q = num / den;
    match rounding {
        Rounding::Up if num % den != U512::ZERO => q + U512::from(1u128),
        _ => q,
    }
}

/// Narrow a `U512` to `u128`, or `None` if it does not fit.
#[inline]
fn narrow_512(x: U512) -> Option<u128> {
    let limbs = x.as_limbs(); // [u64; 8]
    if limbs[2..].iter().any(|&w| w != 0) {
        return None;
    }
    Some((limbs[0] as u128) | ((limbs[1] as u128) << 64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const D: Rounding = Rounding::Down;
    const U: Rounding = Rounding::Up;

    fn one() -> Q64x64 {
        Q64x64::ONE
    }

    #[test]
    fn sqrt_perfect_squares() {
        assert_eq!(sqrt_u128(0), 0);
        assert_eq!(sqrt_u128(1), 1);
        assert_eq!(sqrt_u128(4), 2);
        assert_eq!(sqrt_u128(9), 3);
        assert_eq!(sqrt_u128(1u128 << 64), 1u128 << 32);
        assert_eq!(sqrt_u128(u128::MAX), (1u128 << 64) - 1);
    }

    #[test]
    fn sqrt_floors_non_squares() {
        assert_eq!(sqrt_u128(2), 1);
        assert_eq!(sqrt_u128(3), 1);
        assert_eq!(sqrt_u128(8), 2);
        assert_eq!(sqrt_u128(15), 3);
        assert_eq!(sqrt_u128(99), 9);
        // sqrt of a value above u128 still floors correctly.
        let big = U256::from(1u128) << 200; // 2^200, sqrt = 2^100
        assert_eq!(sqrt_u256(big), U256::from(1u128) << 100);
    }

    #[test]
    fn sqrt_price_round_trip() {
        // price 4 -> sqrt_price 2 (bits 2^65); back to 4.
        let sp = sqrt_price_from_price(4, 1).unwrap();
        assert_eq!(sp.to_bits(), 2u128 << 64);
        let p = price_from_sqrt_price(sp, D).unwrap();
        assert_eq!(p, Q64x64::from_int(4));
        // price 1 -> sqrt_price 1.
        assert_eq!(sqrt_price_from_price(1, 1).unwrap(), one());
        // den 0 -> None.
        assert_eq!(sqrt_price_from_price(1, 0), None);
    }

    #[test]
    fn deltas_known_values() {
        // L = 1000, range S in [1, 2] => sp_lo = 2^64, sp_hi = 2^65.
        let lo = Q64x64::from_bits(1u128 << 64);
        let hi = Q64x64::from_bits(1u128 << 65);
        // delta_b = L * (sp_hi - sp_lo) / 2^64 = 1000 * 2^64 / 2^64 = 1000.
        assert_eq!(delta_b(1000, lo, hi, D).unwrap(), 1000);
        // delta_a = L * 2^64 * diff / (sp_lo*sp_hi)
        //         = 1000 * 2^64 * 2^64 / (2^64 * 2^65) = 1000/2 = 500.
        assert_eq!(delta_a(1000, lo, hi, D).unwrap(), 500);
        // order independence.
        assert_eq!(delta_a(1000, hi, lo, D).unwrap(), 500);
        assert_eq!(delta_b(1000, hi, lo, D).unwrap(), 1000);
    }

    #[test]
    fn delta_rounding_direction() {
        // A range that does not divide evenly: Up >= Down, within 1.
        let lo = Q64x64::from_bits(3u128 << 63); // 1.5
        let hi = Q64x64::from_bits(7u128 << 62); // 1.75
        let d = delta_a(1234, lo, hi, D).unwrap();
        let u = delta_a(1234, lo, hi, U).unwrap();
        assert!(u >= d && u - d <= 1);
    }

    #[test]
    fn next_price_amount_y_add_remove() {
        // sp = 1.0, L = 1000, add 1000 y: delta = (1000<<64)/1000 = 2^64,
        // sp' = 1 + 1 = 2.0.
        let sp = one();
        let up = next_sqrt_price_from_amount_y(sp, 1000, 1000, true).unwrap();
        assert_eq!(up.to_bits(), 2u128 << 64);
        // removing the same amount returns to 1.0.
        let down = next_sqrt_price_from_amount_y(up, 1000, 1000, false).unwrap();
        assert_eq!(down, one());
        // removing more than available -> None.
        assert_eq!(next_sqrt_price_from_amount_y(sp, 1000, 5000, false), None);
        // zero liquidity -> None.
        assert_eq!(next_sqrt_price_from_amount_y(sp, 0, 1, true), None);
        // amount 0 is identity on both branches.
        assert_eq!(
            next_sqrt_price_from_amount_y(sp, 1000, 0, true).unwrap(),
            sp
        );
        assert_eq!(
            next_sqrt_price_from_amount_y(sp, 1000, 0, false).unwrap(),
            sp
        );
    }

    #[test]
    fn next_price_amount_y_rounding_favors_pool() {
        // amount/L does not divide evenly, so the delta has a remainder.
        // add -> round delta DOWN -> sp' as low as possible (less x out).
        // remove -> round delta UP -> sp' as low as possible (more x charged).
        let sp = Q64x64::from_bits(10u128 << 64); // 10.0, headroom for both
        let raw = shl_div(700, SCALE_OFFSET, 999, Rounding::Down).unwrap();
        let raw_up = shl_div(700, SCALE_OFFSET, 999, Rounding::Up).unwrap();
        assert_eq!(raw_up - raw, 1); // genuinely inexact
                                     // add uses the floored delta
        let add = next_sqrt_price_from_amount_y(sp, 999, 700, true).unwrap();
        assert_eq!(add.to_bits(), sp.to_bits() + raw);
        // remove uses the ceiled delta -> strictly lower sp' than a Down would give
        let rem = next_sqrt_price_from_amount_y(sp, 999, 700, false).unwrap();
        assert_eq!(rem.to_bits(), sp.to_bits() - raw_up);
        assert!(rem.to_bits() < sp.to_bits() - raw);
    }

    #[test]
    fn next_price_amount_x_add_remove() {
        // From the deltas test: at sp_hi = 2.0, adding delta_a (500 x) for
        // L = 1000 should move the price back to sp_lo = 1.0.
        let hi = Q64x64::from_bits(1u128 << 65); // 2.0
        let got = next_sqrt_price_from_amount_x(hi, 1000, 500, true).unwrap();
        assert_eq!(got.to_bits(), 1u128 << 64); // 1.0
                                                // remove branch: removing x raises the price (sp' > sp).
        let lo = Q64x64::from_bits(1u128 << 64); // 1.0
        let raised = next_sqrt_price_from_amount_x(lo, 1000, 100, false).unwrap();
        assert!(raised.to_bits() > lo.to_bits());
        // removal large enough to empty the range -> None.
        assert_eq!(
            next_sqrt_price_from_amount_x(lo, 1000, u128::MAX, false),
            None
        );
        // amount 0 is identity.
        assert_eq!(
            next_sqrt_price_from_amount_x(hi, 1000, 0, true).unwrap(),
            hi
        );
        // zero liquidity guard.
        assert_eq!(next_sqrt_price_from_amount_x(hi, 0, 1, true), None);
    }

    #[test]
    fn degenerate_and_overflow_guards() {
        // zero-width range -> zero deltas.
        let sp = Q64x64::from_bits(5u128 << 64);
        assert_eq!(delta_a(1000, sp, sp, D).unwrap(), 0);
        assert_eq!(delta_b(1000, sp, sp, D).unwrap(), 0);
        // delta_a with a zero sqrt price -> None.
        assert_eq!(delta_a(1000, Q64x64::ZERO, sp, D), None);
        // delta_b across a huge range overflows u128 -> None.
        let tiny = Q64x64::from_bits(1);
        let huge = Q64x64::MAX;
        assert_eq!(delta_b(u128::MAX, tiny, huge, D), None);
        // price_from_sqrt_price overflow: sp near MAX -> price > u128.
        assert_eq!(price_from_sqrt_price(Q64x64::MAX, D), None);
    }

    proptest! {
        // sqrt is the floor: z^2 <= x < (z+1)^2.
        #[test]
        fn sqrt_is_floor(x in any::<u128>()) {
            let z = sqrt_u128(x);
            let z2 = U256::from(z) * U256::from(z);
            let z1 = U256::from(z + 1) * U256::from(z + 1);
            prop_assert!(z2 <= U256::from(x));
            prop_assert!(z1 > U256::from(x));
        }

        // sqrt_u256 floor invariant over the FULL 256-bit range (built from
        // two u128 limbs). z^2 <= x < (z+1)^2, computed in U512 to avoid
        // overflow at the top of the range.
        #[test]
        fn sqrt_u256_is_floor(hi in any::<u128>(), lo in any::<u128>()) {
            let x = (U256::from(hi) << 128) | U256::from(lo);
            let z = sqrt_u256(x);
            let zx = U512::from(z);
            let xx = U512::from(x);
            prop_assert!(zx * zx <= xx);
            prop_assert!((zx + U512::from(1u128)) * (zx + U512::from(1u128)) > xx);
        }

        // delta_a/delta_b rounding stays within one unit and Up >= Down.
        #[test]
        fn delta_rounding_bounded(
            l in 1u128..=u64::MAX as u128,
            a in 1u128..=u64::MAX as u128,
            b in 1u128..=u64::MAX as u128,
        ) {
            let (sa, sb) = (Q64x64::from_bits(a), Q64x64::from_bits(b));
            if let (Some(d), Some(u)) = (delta_a(l, sa, sb, D), delta_a(l, sa, sb, U)) {
                prop_assert!(u >= d && u - d <= 1);
            }
            if let (Some(d), Some(u)) = (delta_b(l, sa, sb, D), delta_b(l, sa, sb, U)) {
                prop_assert!(u >= d && u - d <= 1);
            }
        }
    }
}
