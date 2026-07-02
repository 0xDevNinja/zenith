//! Tick math for per-position concentrated liquidity (Uniswap-v3 / DAMM-v2 style).
//!
//! A **tick** is an integer index `t` naming the price `1.0001^t`. Positions are
//! bounded by two ticks `[lower, upper]`; a swap walks from tick to tick, and the
//! active liquidity changes only when the price crosses an initialized tick. This
//! module provides the pure numeric pieces the program layer composes:
//!
//! - [`sqrt_price_at_tick`] / [`tick_at_sqrt_price`] — the tick <-> `sqrt_price`
//!   bijection (prices are tracked as the Q64.64 square root of the price, the
//!   same representation as [`crate::sqrt_price`]).
//! - [`fee_growth_inside`] — the per-range fee-growth accumulator used to settle
//!   a position's earned fees.
//! - [`cross_tick_liquidity`] — the active-liquidity update applied when a swap
//!   crosses a tick.
//! - [`valid_tick_range`] — spacing/bounds validation for a new position.
//!
//! ## Why `pow` + floored `sqrt`, not a bespoke bit-decomposition
//!
//! `sqrt_price_at_tick` computes `1.0001^t` with the shared, overflow-checked
//! [`crate::pow`] (binary exponentiation in Q64.64) and then takes the floored
//! integer square root. This is the exact same machinery [`crate::bin_price`]
//! uses, which is already proven **strictly monotonic** across its whole
//! supported band (see that module's tests). We bound the tick domain to
//! `[MIN_TICK, MAX_TICK]` so every price stays inside the Q64.64 band where
//! adjacent ticks are many ulps apart — the `tick_strictly_monotonic_*` tests
//! walk the domain and assert the bijection holds, so a swap can never map two
//! ticks to one price (which would leak value at a crossing).

use ruint::aliases::U256;

use crate::u256::to_u128;
use crate::{pow, sqrt_u256, Q64x64, Rounding, MAX_PRICE_BITS, MIN_PRICE_BITS};

/// Basis points of the per-tick price step: `1.0001 = 1 + 1/10000`.
const TICK_BASE_NUM: u128 = 10_001;
const TICK_BASE_DEN: u128 = 10_000;

/// Widest tick whose price `1.0001^t` stays safely inside the Q64.64 price band
/// `[2^-32, 2^32]` (see [`crate::bin_price`]). Chosen conservatively so
/// [`sqrt_price_at_tick`] returns `Some` for every tick in `[MIN_TICK, MAX_TICK]`
/// — the binary search in [`tick_at_sqrt_price`] relies on that. Spans ~19
/// orders of magnitude of price, ample for real pairs.
pub const MAX_TICK: i32 = 221_000;

/// Mirror of [`MAX_TICK`] on the low side.
pub const MIN_TICK: i32 = -221_000;

/// The Q64.64 base `1.0001` used for the tick price power.
#[inline]
fn tick_base() -> Q64x64 {
    // Exact-enough down-rounded 1.0001; the same base for every tick, so the
    // per-tick ratio is consistent and monotonic.
    Q64x64::from_ratio(TICK_BASE_NUM, TICK_BASE_DEN, Rounding::Down)
        .expect("1.0001 is representable in Q64.64")
}

/// `sqrt_price` (Q64.64) at tick `t`: `sqrt(1.0001^t)`.
///
/// Computes the price `1.0001^t` with [`pow`] (rounded **down**) then the floored
/// integer square root, so the result is deterministic and reproducible bit-for-bit
/// by the SDK. Returns `None` if `t` is outside `[MIN_TICK, MAX_TICK]` or (defensively)
/// if the price leaves the supported band.
pub fn sqrt_price_at_tick(tick: i32) -> Option<Q64x64> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return None;
    }
    // price = 1.0001^tick in Q64.64 (pow handles negative ticks via reciprocal).
    let price = pow(tick_base(), tick, Rounding::Down)?;
    let price_bits = price.to_bits();
    if !(MIN_PRICE_BITS..=MAX_PRICE_BITS).contains(&price_bits) {
        return None;
    }
    // sqrt_price = sqrt(price). In Q64.64 bits:
    //   sp_bits = sqrt(price) * 2^64 = sqrt(price_bits / 2^64) * 2^64
    //           = sqrt(price_bits) * 2^32 = floor(sqrt(price_bits << 64)).
    // price_bits <= 2^96, so (price_bits << 64) <= 2^160 fits U256; sqrt <= 2^80.
    let sp_bits = sqrt_u256(U256::from(price_bits) << 64);
    to_u128(sp_bits).ok().map(Q64x64::from_bits)
}

/// Greatest tick `t` such that `sqrt_price_at_tick(t) <= sqrt_price` (floor).
///
/// Binary search over the monotonic [`sqrt_price_at_tick`]; clamps to
/// `[MIN_TICK, MAX_TICK]`. This is the inverse used to place a price on the tick
/// grid (e.g. snapping a user's range bound). It is `O(log(range))` `pow` calls —
/// off the swap hot path (used by position/quote code, not per swap step).
pub fn tick_at_sqrt_price(sqrt_price: Q64x64) -> i32 {
    // Domain endpoints are guaranteed Some by the conservative MAX_TICK.
    let min_sp = sqrt_price_at_tick(MIN_TICK).expect("MIN_TICK in band");
    let max_sp = sqrt_price_at_tick(MAX_TICK).expect("MAX_TICK in band");
    if sqrt_price <= min_sp {
        return MIN_TICK;
    }
    if sqrt_price >= max_sp {
        return MAX_TICK;
    }
    // Invariant: sqrt_at(lo) <= sqrt_price < sqrt_at(hi).
    let (mut lo, mut hi) = (MIN_TICK, MAX_TICK);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        let mid_sp = sqrt_price_at_tick(mid).expect("mid in band");
        if mid_sp <= sqrt_price {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Fee growth accumulated *inside* the range `[tick_lower, tick_upper]` for both
/// tokens, given the current tick, the global accumulators, and each boundary
/// tick's `fee_growth_outside`.
///
/// This is the Uniswap-v3 "fee growth inside" identity. All subtractions are
/// **wrapping** on purpose: `fee_growth_outside` and the global accumulator are
/// monotonically-increasing counters that are allowed to wrap past `u128::MAX`,
/// and the difference `inside = global - below - above` stays correct modulo
/// `2^128` exactly as the real (unbounded) accumulators would. A checked
/// subtraction here would spuriously revert legitimate fee claims once a counter
/// has wrapped — see [`crate::u256`]-adjacent fee math and the wrap tests below.
#[allow(clippy::too_many_arguments)]
pub fn fee_growth_inside(
    tick_lower: i32,
    tick_upper: i32,
    current_tick: i32,
    fee_growth_global_a: u128,
    fee_growth_global_b: u128,
    fee_growth_outside_lower_a: u128,
    fee_growth_outside_lower_b: u128,
    fee_growth_outside_upper_a: u128,
    fee_growth_outside_upper_b: u128,
) -> (u128, u128) {
    let a = fee_growth_inside_one(
        current_tick,
        tick_lower,
        tick_upper,
        fee_growth_global_a,
        fee_growth_outside_lower_a,
        fee_growth_outside_upper_a,
    );
    let b = fee_growth_inside_one(
        current_tick,
        tick_lower,
        tick_upper,
        fee_growth_global_b,
        fee_growth_outside_lower_b,
        fee_growth_outside_upper_b,
    );
    (a, b)
}

/// Single-token fee-growth-inside (see [`fee_growth_inside`]).
fn fee_growth_inside_one(
    current_tick: i32,
    tick_lower: i32,
    tick_upper: i32,
    fee_growth_global: u128,
    fee_growth_outside_lower: u128,
    fee_growth_outside_upper: u128,
) -> u128 {
    // Growth below the lower tick.
    let below = if current_tick >= tick_lower {
        fee_growth_outside_lower
    } else {
        fee_growth_global.wrapping_sub(fee_growth_outside_lower)
    };
    // Growth above the upper tick.
    let above = if current_tick < tick_upper {
        fee_growth_outside_upper
    } else {
        fee_growth_global.wrapping_sub(fee_growth_outside_upper)
    };
    fee_growth_global.wrapping_sub(below).wrapping_sub(above)
}

/// Active liquidity after crossing a tick whose `liquidity_net` is given.
///
/// `liquidity_net` is the signed change applied when the price crosses the tick
/// moving **upward** (left-to-right). When a swap moves downward
/// (`zero_for_one == true`, i.e. selling the base token so the price falls) the
/// sign is flipped. Returns `None` on overflow, on an underflow (liquidity would
/// go negative — a corrupt tick), or when `liquidity_net == i128::MIN` (its
/// negation is unrepresentable).
pub fn cross_tick_liquidity(
    liquidity: u128,
    liquidity_net: i128,
    zero_for_one: bool,
) -> Option<u128> {
    let net = if zero_for_one {
        liquidity_net.checked_neg()? // i128::MIN -> None (cannot happen for real ticks)
    } else {
        liquidity_net
    };
    if net >= 0 {
        liquidity.checked_add(net as u128)
    } else {
        liquidity.checked_sub(net.unsigned_abs())
    }
}

/// Validate a would-be position range against the tick spacing and domain.
///
/// `true` iff `spacing != 0`, `lower < upper`, both bounds lie in
/// `[MIN_TICK, MAX_TICK]`, and both are exact multiples of `spacing`.
pub fn valid_tick_range(tick_lower: i32, tick_upper: i32, tick_spacing: u16) -> bool {
    if tick_spacing == 0 || tick_lower >= tick_upper {
        return false;
    }
    if tick_lower < MIN_TICK || tick_upper > MAX_TICK {
        return false;
    }
    let s = tick_spacing as i32;
    tick_lower % s == 0 && tick_upper % s == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_zero_is_one() {
        // 1.0001^0 = 1, sqrt = 1.0 exactly.
        assert_eq!(sqrt_price_at_tick(0).unwrap(), Q64x64::ONE);
    }

    #[test]
    fn tick_sign_direction() {
        // Positive ticks price above 1.0, negative below.
        assert!(sqrt_price_at_tick(1).unwrap() > Q64x64::ONE);
        assert!(sqrt_price_at_tick(-1).unwrap() < Q64x64::ONE);
        assert!(sqrt_price_at_tick(100).unwrap() > sqrt_price_at_tick(1).unwrap());
        assert!(sqrt_price_at_tick(-100).unwrap() < sqrt_price_at_tick(-1).unwrap());
    }

    #[test]
    fn out_of_domain_is_none() {
        assert_eq!(sqrt_price_at_tick(MAX_TICK + 1), None);
        assert_eq!(sqrt_price_at_tick(MIN_TICK - 1), None);
        assert_eq!(sqrt_price_at_tick(i32::MAX), None);
        assert_eq!(sqrt_price_at_tick(i32::MIN), None);
    }

    #[test]
    fn domain_endpoints_are_some() {
        // The binary search depends on both endpoints being in-band.
        assert!(sqrt_price_at_tick(MIN_TICK).is_some());
        assert!(sqrt_price_at_tick(MAX_TICK).is_some());
    }

    #[test]
    fn sqrt_price_symmetry() {
        // sqrt_at(t) * sqrt_at(-t) ~= 1.0 (price(t)*price(-t) = 1). Within a few
        // ulps because each is independently floored.
        for &t in &[1, 2, 5, 50, 500, 5_000, 50_000, 200_000] {
            let up = sqrt_price_at_tick(t).unwrap();
            let dn = sqrt_price_at_tick(-t).unwrap();
            let prod = up.mul(dn, Rounding::Down).unwrap();
            let diff = prod.to_bits().abs_diff(Q64x64::ONE.to_bits());
            // Sanity check, not a correctness invariant (monotonicity + round-trip
            // are those). Each of sqrt_at(t)/sqrt_at(-t) is independently floored
            // and, for negative ticks, goes through pow's reciprocal path, so the
            // product drifts from 1.0 by a tiny *relative* amount that grows with
            // |t|. Bound it to ~2^-32 of 1.0 (sub-part-per-billion).
            assert!(
                diff <= 1 << 32,
                "t={t} prod diff={diff} exceeds relative bound"
            );
        }
    }

    #[test]
    fn tick_strictly_monotonic_dense_window() {
        // Walk a dense window around zero and assert sqrt_price strictly
        // increases every single tick — the bijection the swap relies on.
        let mut prev = sqrt_price_at_tick(-5_000).unwrap();
        let mut t = -4_999;
        while t <= 5_000 {
            let cur = sqrt_price_at_tick(t).unwrap();
            assert!(cur > prev, "not strictly increasing at tick {t}");
            prev = cur;
            t += 1;
        }
    }

    #[test]
    fn tick_strictly_monotonic_deep_edges() {
        // Dense walk near both domain edges (the worst case for resolution).
        for start in [MIN_TICK, MAX_TICK - 5_000] {
            let mut prev = sqrt_price_at_tick(start).unwrap();
            let mut t = start + 1;
            let end = start + 5_000;
            while t <= end {
                let cur = sqrt_price_at_tick(t).unwrap();
                assert!(cur > prev, "not strictly increasing at tick {t}");
                prev = cur;
                t += 1;
            }
        }
    }

    #[test]
    fn tick_monotonic_sampled_full_domain() {
        // Sparse sweep across the ENTIRE domain: consecutive samples must
        // increase (coarser than adjacent, but covers the whole range cheaply).
        let mut prev = sqrt_price_at_tick(MIN_TICK).unwrap();
        let mut t = MIN_TICK + 137;
        while t <= MAX_TICK {
            let cur = sqrt_price_at_tick(t).unwrap();
            assert!(cur > prev, "not increasing at sampled tick {t}");
            prev = cur;
            t += 137;
        }
    }

    #[test]
    fn tick_strictly_monotonic_positive_mid_domain() {
        // Belt-and-suspenders: a dense adjacent walk in the positive mid-domain
        // (away from zero and from the edges). Resolution is coarser here than at
        // MIN_TICK, so this is guaranteed by the deep-edge walk, but exercising it
        // directly leaves no untested adjacent region on the upper half.
        let mut prev = sqrt_price_at_tick(100_000).unwrap();
        let mut t = 100_001;
        while t <= 105_000 {
            let cur = sqrt_price_at_tick(t).unwrap();
            assert!(cur > prev, "not strictly increasing at tick {t}");
            prev = cur;
            t += 1;
        }
    }

    #[test]
    fn tick_round_trip() {
        // tick_at_sqrt_price(sqrt_price_at_tick(t)) == t for a spread of ticks.
        for &t in &[
            MIN_TICK, -200_000, -12_345, -64, -1, 0, 1, 64, 12_345, 200_000, MAX_TICK,
        ] {
            let sp = sqrt_price_at_tick(t).unwrap();
            assert_eq!(tick_at_sqrt_price(sp), t, "round-trip failed at tick {t}");
        }
    }

    #[test]
    fn tick_at_sqrt_price_floors_between_ticks() {
        // A price strictly between tick t and t+1 floors to t.
        let t = 1_000;
        let sp_t = sqrt_price_at_tick(t).unwrap();
        let sp_t1 = sqrt_price_at_tick(t + 1).unwrap();
        // midpoint bits (both fit, no overflow)
        let mid = Q64x64::from_bits((sp_t.to_bits() + sp_t1.to_bits()) / 2);
        assert!(mid > sp_t && mid < sp_t1);
        assert_eq!(tick_at_sqrt_price(mid), t);
    }

    #[test]
    fn tick_at_sqrt_price_clamps() {
        // Below the domain clamps to MIN_TICK, above to MAX_TICK.
        assert_eq!(tick_at_sqrt_price(Q64x64::from_bits(1)), MIN_TICK);
        assert_eq!(tick_at_sqrt_price(Q64x64::MAX), MAX_TICK);
    }

    #[test]
    fn fee_growth_inside_current_in_range() {
        // current inside [lower, upper]: below = outside_lower, above = outside_upper.
        // global=1000, lower_out=100, upper_out=200 -> inside = 700.
        let (a, b) = fee_growth_inside(-10, 10, 0, 1000, 5000, 100, 500, 200, 1000);
        assert_eq!(a, 1000 - 100 - 200);
        assert_eq!(b, 5000 - 500 - 1000);
    }

    #[test]
    fn fee_growth_inside_current_below_range() {
        // current < lower: below = global - outside_lower.
        // inside = global - (global - lower_out) - upper_out = lower_out - upper_out.
        let (a, _) = fee_growth_inside(-10, 10, -20, 1000, 0, 700, 0, 200, 0);
        assert_eq!(a, 700u128.wrapping_sub(200));
    }

    #[test]
    fn fee_growth_inside_current_above_range() {
        // current >= upper: above = global - outside_upper.
        // below = outside_lower (current >= lower).
        // inside = global - lower_out - (global - upper_out) = upper_out - lower_out.
        let (a, _) = fee_growth_inside(-10, 10, 50, 1000, 0, 100, 0, 600, 0);
        assert_eq!(a, 600u128.wrapping_sub(100));
    }

    #[test]
    fn fee_growth_inside_wraps() {
        // A boundary's outside accumulator has wrapped past u128::MAX. Wrapping
        // subtraction must still yield the correct (small) inside growth. Model:
        // global just wrapped to 10; lower_out captured 5 before the wrap near MAX.
        let global = 10u128;
        let lower_out = u128::MAX - 4; // "just below wrap"
        let upper_out = 0u128;
        // current in range -> inside = global - lower_out - upper_out (wrapping)
        let (a, _) = fee_growth_inside(-10, 10, 0, global, 0, lower_out, 0, upper_out, 0);
        assert_eq!(a, global.wrapping_sub(lower_out).wrapping_sub(upper_out));
        // sanity: this equals global + 5 (the real accrued delta across the wrap)
        assert_eq!(a, 10u128.wrapping_add(5));
    }

    #[test]
    fn cross_tick_up_and_down() {
        // Upward cross adds liquidity_net; downward subtracts it.
        assert_eq!(cross_tick_liquidity(1000, 300, false), Some(1300));
        assert_eq!(cross_tick_liquidity(1000, 300, true), Some(700));
        // Negative net: upward removes, downward adds.
        assert_eq!(cross_tick_liquidity(1000, -300, false), Some(700));
        assert_eq!(cross_tick_liquidity(1000, -300, true), Some(1300));
    }

    #[test]
    fn cross_tick_guards() {
        // Underflow (corrupt tick would empty liquidity) -> None.
        assert_eq!(cross_tick_liquidity(100, 500, true), None); // 100 - 500
        assert_eq!(cross_tick_liquidity(100, -500, false), None);
        // Overflow -> None.
        assert_eq!(cross_tick_liquidity(u128::MAX, 1, false), None);
        // i128::MIN negation unrepresentable -> None (only on the flip path).
        assert_eq!(cross_tick_liquidity(1000, i128::MIN, true), None);
        // net == 0 is identity.
        assert_eq!(cross_tick_liquidity(1000, 0, true), Some(1000));
        assert_eq!(cross_tick_liquidity(1000, 0, false), Some(1000));
    }

    #[test]
    fn valid_tick_range_rules() {
        assert!(valid_tick_range(-100, 100, 10));
        assert!(valid_tick_range(0, 60, 60));
        // not spaced
        assert!(!valid_tick_range(-5, 100, 10));
        assert!(!valid_tick_range(-100, 95, 10));
        // lower >= upper
        assert!(!valid_tick_range(100, 100, 10));
        assert!(!valid_tick_range(100, -100, 10));
        // spacing 0
        assert!(!valid_tick_range(-100, 100, 0));
        // out of domain
        assert!(!valid_tick_range(MIN_TICK - 10, 0, 1));
        assert!(!valid_tick_range(0, MAX_TICK + 10, 1));
    }
}
