//! Thin program-side wrappers over `zenith-math` for the AMM handlers.
//!
//! These keep the token-amount/return-type plumbing (and the protocol-favoring
//! rounding choices) in one place so the instruction handlers stay readable and
//! the logic is unit-testable on the host.

use anchor_lang::prelude::*;
use zenith_math::{delta_a, delta_b, mul_shr, Q64x64, Rounding, SCALE_OFFSET};

use crate::errors::ZenithError;
use crate::state::Position;

/// Narrow a `u128` math result to a `u64` token amount, erroring on overflow.
pub fn to_token_amount(x: u128) -> Result<u64> {
    u64::try_from(x).map_err(|_| error!(ZenithError::MathOverflow))
}

/// Validate a pool's price band and that the current price sits strictly inside.
///
/// All values are Q64.64 raw bits. Requires `0 < min < price < max`.
pub fn validate_price_band(sqrt_min: u128, sqrt_price: u128, sqrt_max: u128) -> Result<()> {
    require!(
        sqrt_min > 0 && sqrt_min < sqrt_max,
        ZenithError::InvalidPriceBand
    );
    require!(
        sqrt_price > sqrt_min && sqrt_price < sqrt_max,
        ZenithError::PriceOutOfBand
    );
    Ok(())
}

/// Token amounts for `liquidity` at `sqrt_price` within the band.
///
/// For a price strictly inside the band a position holds both tokens:
/// - token A (base) over `[price, max]` → [`delta_a`]
/// - token B (quote) over `[min, price]` → [`delta_b`]
///
/// `rounding` is the caller's protocol-favoring choice: **up** when the user is
/// depositing (they must back the liquidity with at least this much) and
/// **down** when the pool is paying out (never more than backed). Returns
/// `(amount_a, amount_b)`.
pub fn liquidity_amounts(
    liquidity: u128,
    sqrt_price: u128,
    sqrt_min: u128,
    sqrt_max: u128,
    rounding: Rounding,
) -> Result<(u64, u64)> {
    let price = Q64x64::from_bits(sqrt_price);
    let lo = Q64x64::from_bits(sqrt_min);
    let hi = Q64x64::from_bits(sqrt_max);

    let amount_a = delta_a(liquidity, price, hi, rounding).ok_or(ZenithError::MathOverflow)?;
    let amount_b = delta_b(liquidity, lo, price, rounding).ok_or(ZenithError::MathOverflow)?;

    Ok((to_token_amount(amount_a)?, to_token_amount(amount_b)?))
}

/// Token amounts a creator must deposit to mint `liquidity` (always rounds up).
pub fn initial_liquidity_amounts(
    liquidity: u128,
    sqrt_price: u128,
    sqrt_min: u128,
    sqrt_max: u128,
) -> Result<(u64, u64)> {
    liquidity_amounts(liquidity, sqrt_price, sqrt_min, sqrt_max, Rounding::Up)
}

/// Settle the fees a position has earned since its last checkpoint into its
/// `fee_pending_*` buckets, then advance the checkpoints to the current global
/// fee growth. Must run **before** any change to the position's liquidity, so
/// fees are attributed to the liquidity that actually earned them.
///
/// Fees accrue on the position's *total* liquidity (unlocked + vested + locked);
/// all of it earns. Per-liquidity growth is Q64.64, so the earned token amount
/// is `total_liquidity * Δgrowth >> 64`, rounded **down** (the pool never pays
/// out more than it accrued). The global accumulator is allowed to wrap, so the
/// delta is a wrapping subtraction.
pub fn settle_position_fees(
    position: &mut Position,
    fee_growth_global_a: u128,
    fee_growth_global_b: u128,
) -> Result<()> {
    let liquidity = position.total_liquidity();

    let earned_a = accrued_fee(
        liquidity,
        fee_growth_global_a,
        position.fee_growth_checkpoint_a,
    )?;
    let earned_b = accrued_fee(
        liquidity,
        fee_growth_global_b,
        position.fee_growth_checkpoint_b,
    )?;

    position.fee_pending_a = position
        .fee_pending_a
        .checked_add(earned_a)
        .ok_or(ZenithError::MathOverflow)?;
    position.fee_pending_b = position
        .fee_pending_b
        .checked_add(earned_b)
        .ok_or(ZenithError::MathOverflow)?;

    position.fee_growth_checkpoint_a = fee_growth_global_a;
    position.fee_growth_checkpoint_b = fee_growth_global_b;

    Ok(())
}

/// Token fees earned for `liquidity` between a checkpoint and the current global
/// fee growth (both Q64.64 per-liquidity raw bits). Rounds down.
fn accrued_fee(liquidity: u128, fee_growth_global: u128, checkpoint: u128) -> Result<u64> {
    let delta = fee_growth_global.wrapping_sub(checkpoint);
    let earned = mul_shr(liquidity, delta, SCALE_OFFSET, Rounding::Down)
        .map_err(|_| ZenithError::MathOverflow)?;
    to_token_amount(earned)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Q64.64 helpers for the tests.
    const ONE: u128 = 1u128 << 64;

    #[test]
    fn band_validation() {
        // 0 < min < price < max is OK
        assert!(validate_price_band(ONE, 2 * ONE, 4 * ONE).is_ok());
        // min == 0 rejected
        assert!(validate_price_band(0, ONE, 2 * ONE).is_err());
        // min >= max rejected
        assert!(validate_price_band(4 * ONE, 2 * ONE, 4 * ONE).is_err());
        // price at or outside the band rejected
        assert!(validate_price_band(ONE, ONE, 4 * ONE).is_err()); // price == min
        assert!(validate_price_band(ONE, 4 * ONE, 4 * ONE).is_err()); // price == max
        assert!(validate_price_band(ONE, 5 * ONE, 4 * ONE).is_err()); // price > max
    }

    #[test]
    fn initial_amounts_known() {
        // L = 1000, band S in [1, 4], current S = 2 (sqrt prices 1,2,4 -> bits).
        // delta_a(L, price=2, max=4) and delta_b(L, min=1, price=2).
        // delta_b = L * (2-1) = 1000.
        // delta_a = L * (4-2)/(2*4) = 1000 * 2/8 = 250.
        let (a, b) = initial_liquidity_amounts(1000, 2 * ONE, ONE, 4 * ONE).unwrap();
        assert_eq!(a, 250);
        assert_eq!(b, 1000);
    }

    #[test]
    fn initial_amounts_overflow_is_caught() {
        // Force the token amount past u64 with a huge liquidity over a wide band.
        let res = initial_liquidity_amounts(u128::MAX, 2 * ONE, ONE, 4 * ONE);
        assert!(res.is_err());
    }

    #[test]
    fn to_token_amount_bounds() {
        assert_eq!(to_token_amount(123).unwrap(), 123);
        assert_eq!(to_token_amount(u64::MAX as u128).unwrap(), u64::MAX);
        assert!(to_token_amount(u64::MAX as u128 + 1).is_err());
    }

    #[test]
    fn add_never_pays_less_than_remove_returns() {
        // Deposit (round up) must always be >= withdrawal (round down) for the
        // same liquidity at the same price — rounding never favors the user.
        for &l in &[1u128, 7, 333, 10_000, 1_000_000] {
            let (add_a, add_b) = liquidity_amounts(l, 3 * ONE, ONE, 4 * ONE, Rounding::Up).unwrap();
            let (rem_a, rem_b) =
                liquidity_amounts(l, 3 * ONE, ONE, 4 * ONE, Rounding::Down).unwrap();
            assert!(add_a >= rem_a, "A: add {add_a} < remove {rem_a} for L={l}");
            assert!(add_b >= rem_b, "B: add {add_b} < remove {rem_b} for L={l}");
            // Rounding gap is at most 1 unit per side.
            assert!(add_a - rem_a <= 1 && add_b - rem_b <= 1);
        }
    }

    #[test]
    fn fee_settlement_accrues_and_advances_checkpoint() {
        let mut pos = Position {
            pool: Pubkey::default(),
            nft_mint: Pubkey::default(),
            liquidity: 1_000_000,
            vested_liquidity: 0,
            permanent_locked_liquidity: 0,
            fee_growth_checkpoint_a: 0,
            fee_growth_checkpoint_b: 0,
            fee_pending_a: 0,
            fee_pending_b: 0,
            bump: 0,
            reserved: [0u8; 64],
        };
        // Global grew by 0.5 (Q64.64) for A, 2.0 for B since the checkpoint.
        let half = ONE / 2;
        let two = 2 * ONE;
        settle_position_fees(&mut pos, half, two).unwrap();
        // earned = L * Δgrowth >> 64
        assert_eq!(pos.fee_pending_a, 500_000); // 1e6 * 0.5
        assert_eq!(pos.fee_pending_b, 2_000_000); // 1e6 * 2
        assert_eq!(pos.fee_growth_checkpoint_a, half);
        assert_eq!(pos.fee_growth_checkpoint_b, two);

        // Settling again with no further growth adds nothing.
        settle_position_fees(&mut pos, half, two).unwrap();
        assert_eq!(pos.fee_pending_a, 500_000);
        assert_eq!(pos.fee_pending_b, 2_000_000);
    }

    #[test]
    fn fee_growth_wraps_around() {
        let mut pos = Position {
            pool: Pubkey::default(),
            nft_mint: Pubkey::default(),
            liquidity: ONE, // 2^64, so >>64 gives the raw growth delta in tokens
            vested_liquidity: 0,
            permanent_locked_liquidity: 0,
            fee_growth_checkpoint_a: u128::MAX - 2,
            fee_growth_checkpoint_b: 0,
            fee_pending_a: 0,
            fee_pending_b: 0,
            bump: 0,
            reserved: [0u8; 64],
        };
        // Global wrapped past u128::MAX to 5: delta = 5 - (MAX-2) = 8 (wrapping).
        settle_position_fees(&mut pos, 5, 0).unwrap();
        assert_eq!(pos.fee_pending_a, 8);
    }
}
