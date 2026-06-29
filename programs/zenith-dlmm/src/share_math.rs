//! Share ↔ token math for liquidity removal.
//!
//! Removal is price-independent: an LP burning `shares` out of a bin holding
//! `(amount_x, amount_y)` against `supply` total shares gets that pro-rata
//! slice of each reserve. All conversions round **down** in the protocol's
//! favor, so a withdrawal never returns more than the burned shares are worth
//! and any rounding dust stays in the pool.

use zenith_math::{mul_div, MathError, Rounding};

/// Basis-points denominator.
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Number of shares to remove for `bps` (in 1/10000) of `position_shares`,
/// rounded down.
pub fn shares_for_bps(position_shares: u128, bps: u16) -> Result<u128, MathError> {
    mul_div(
        position_shares,
        bps as u128,
        BPS_DENOMINATOR as u128,
        Rounding::Down,
    )
}

/// Token amount owed for burning `shares` out of a bin holding `amount` against
/// `supply` total shares.
///
/// With `shares <= supply` and `Rounding::Down`, the result never exceeds
/// `amount` (no over-withdraw). Returns 0 if the bin is empty or no shares are
/// burned.
pub fn tokens_for_shares(
    amount: u64,
    shares: u128,
    supply: u128,
    rounding: Rounding,
) -> Result<u64, MathError> {
    if supply == 0 || shares == 0 {
        return Ok(0);
    }
    // amount * shares / supply <= amount (since shares <= supply), so it fits u64.
    let out = mul_div(amount as u128, shares, supply, rounding)?;
    debug_assert!(out <= amount as u128);
    Ok(out as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn bps_bounds() {
        assert_eq!(shares_for_bps(1000, 0).unwrap(), 0);
        assert_eq!(shares_for_bps(1000, 10_000).unwrap(), 1000); // 100% = all
        assert_eq!(shares_for_bps(1000, 5_000).unwrap(), 500); // 50%
        assert_eq!(shares_for_bps(999, 3_333).unwrap(), 332); // 999*3333/10000 = 332.96 -> 332
    }

    #[test]
    fn full_share_removal_returns_full_amount() {
        // Sole owner (shares == supply) gets the whole reserve, no dust.
        assert_eq!(
            tokens_for_shares(1_000_000, 50, 50, Rounding::Down).unwrap(),
            1_000_000
        );
    }

    #[test]
    fn zero_cases() {
        assert_eq!(tokens_for_shares(100, 0, 50, Rounding::Down).unwrap(), 0);
        assert_eq!(tokens_for_shares(100, 10, 0, Rounding::Down).unwrap(), 0);
    }

    proptest! {
        /// No over-withdraw: for any bin and any shares <= supply, the payout
        /// never exceeds the reserve, and burning all shares drains it.
        #[test]
        fn never_over_withdraws(
            amount in 0u64..=u64::MAX,
            supply in 1u128..=u128::MAX,
            frac in 0.0f64..=1.0,
        ) {
            let shares = ((supply as f64) * frac) as u128;
            let shares = shares.min(supply);
            let out = tokens_for_shares(amount, shares, supply, Rounding::Down).unwrap();
            prop_assert!(out <= amount);
            // Burning the full supply returns the entire reserve.
            let all = tokens_for_shares(amount, supply, supply, Rounding::Down).unwrap();
            prop_assert_eq!(all, amount);
        }

        /// Splitting a withdrawal across two burns never returns more than one
        /// combined burn (floor super-additivity keeps dust in the pool).
        #[test]
        fn partial_sums_do_not_exceed_whole(
            amount in 0u64..=1_000_000_000_000u64,
            supply in 1u128..=1_000_000_000u128,
            a in 0u128..=1_000_000_000u128,
            b in 0u128..=1_000_000_000u128,
        ) {
            let a = a.min(supply);
            let b = b.min(supply - a.min(supply));
            let out_a = tokens_for_shares(amount, a, supply, Rounding::Down).unwrap();
            let out_b = tokens_for_shares(amount, b, supply, Rounding::Down).unwrap();
            let out_ab = tokens_for_shares(amount, a + b, supply, Rounding::Down).unwrap();
            prop_assert!(out_a + out_b <= out_ab);
            prop_assert!(out_ab <= amount);
        }
    }
}
