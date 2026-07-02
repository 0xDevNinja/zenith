//! Idle-reserve yield arithmetic for the mock lending vault.
//!
//! A full-range pool keeps most of its capital far from the current price, so
//! that idle capital can be "deployed" to earn yield. On devnet there is no real
//! lending market, so this is a mock: the deployed principal accrues a fixed
//! per-slot rate, and the accrued amount is paid into the reserve out of a
//! pre-funded yield source (raising the LP share price for everyone). The
//! principal itself never leaves the reserve vault, so swaps stay solvent.
//!
//! Both functions are pure integer arithmetic so they can be unit-tested and
//! ported bit-exact to the SDK.

use zenith_math::{mul_div, MathResult, Rounding};

use crate::constants::YIELD_SCALE;

const BPS: u128 = 10_000;

/// Yield accrued on `deployed` principal over `elapsed` slots at `yield_rate`
/// (scaled by [`YIELD_SCALE`]): `deployed * yield_rate * elapsed / YIELD_SCALE`.
///
/// Rounded **down**. The result is what the pool *would* pay if the yield source
/// is funded for it; the caller caps the payout at the source's balance.
pub fn accrued_yield(deployed: u64, yield_rate: u64, elapsed: u64) -> MathResult<u64> {
    if deployed == 0 || yield_rate == 0 || elapsed == 0 {
        return Ok(0);
    }
    // deployed * yield_rate fits u128; multiply by elapsed in a second step so a
    // long idle period cannot overflow the first product.
    let rate_slots = (yield_rate as u128)
        .checked_mul(elapsed as u128)
        .ok_or(zenith_math::MathError::Overflow)?;
    let accrued = mul_div(deployed as u128, rate_slots, YIELD_SCALE, Rounding::Down)?;
    // Accrued yield on real reserves is small relative to u64; guard anyway.
    accrued
        .try_into()
        .map_err(|_| zenith_math::MathError::Overflow)
}

/// Principal eligible to be deployed to the yield vault: the reserve minus a
/// solvency buffer of `buffer_bps`. The buffer is rounded **up** (kept
/// generous), so `deployed = reserve - ceil(reserve * buffer_bps / 10000)`.
pub fn deployable(reserve: u64, buffer_bps: u16) -> MathResult<u64> {
    let buffer = mul_div(reserve as u128, buffer_bps as u128, BPS, Rounding::Up)?;
    // buffer <= reserve since buffer_bps <= 10000.
    Ok(reserve - buffer as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accrued_scales_with_principal_rate_and_time() {
        // rate = 1e6 (= 0.001 per slot after /1e9), deployed 1_000_000, 10 slots:
        // 1_000_000 * 1e6 * 10 / 1e9 = 10_000.
        assert_eq!(accrued_yield(1_000_000, 1_000_000, 10).unwrap(), 10_000);
        // Any zero input yields zero.
        assert_eq!(accrued_yield(0, 1_000_000, 10).unwrap(), 0);
        assert_eq!(accrued_yield(1_000_000, 0, 10).unwrap(), 0);
        assert_eq!(accrued_yield(1_000_000, 1_000_000, 0).unwrap(), 0);
    }

    #[test]
    fn accrued_rounds_down() {
        // 100 * 1 * 1 / 1e9 = 0.0000001 -> 0.
        assert_eq!(accrued_yield(100, 1, 1).unwrap(), 0);
    }

    #[test]
    fn deployable_keeps_buffer() {
        // 10% buffer of 1000 = 100 -> deployable 900.
        assert_eq!(deployable(1000, 1000).unwrap(), 900);
        // Zero buffer deploys everything; full buffer deploys nothing.
        assert_eq!(deployable(1000, 0).unwrap(), 1000);
        assert_eq!(deployable(1000, 10_000).unwrap(), 0);
        // Buffer rounds up (generous): ceil(1 * 5000/10000) = 1 -> deployable 0.
        assert_eq!(deployable(1, 5000).unwrap(), 0);
    }

    #[test]
    fn accrued_no_overflow_on_long_idle() {
        // Large deployed + long elapsed must not panic; two-step multiply.
        let got = accrued_yield(u64::MAX, 1, u64::MAX);
        // Result overflows u64 -> clean error, not a wrap.
        assert!(got.is_err());
    }
}
