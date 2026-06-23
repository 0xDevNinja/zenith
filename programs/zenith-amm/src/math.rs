//! Thin program-side wrappers over `zenith-math` for the AMM handlers.
//!
//! These keep the token-amount/return-type plumbing (and the protocol-favoring
//! rounding choices) in one place so the instruction handlers stay readable and
//! the logic is unit-testable on the host.

use anchor_lang::prelude::*;
use zenith_math::{delta_a, delta_b, Q64x64, Rounding};

use crate::errors::ZenithError;

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

/// Token amounts required to mint `liquidity` at `sqrt_price` within the band.
///
/// For a price strictly inside the band a position holds both tokens:
/// - token A (base) over `[price, max]` → [`delta_a`]
/// - token B (quote) over `[min, price]` → [`delta_b`]
///
/// Both are rounded **up** because the creator must deposit at least enough to
/// back the liquidity (never less). Returns `(amount_a, amount_b)`.
pub fn initial_liquidity_amounts(
    liquidity: u128,
    sqrt_price: u128,
    sqrt_min: u128,
    sqrt_max: u128,
) -> Result<(u64, u64)> {
    let price = Q64x64::from_bits(sqrt_price);
    let lo = Q64x64::from_bits(sqrt_min);
    let hi = Q64x64::from_bits(sqrt_max);

    let amount_a = delta_a(liquidity, price, hi, Rounding::Up).ok_or(ZenithError::MathOverflow)?;
    let amount_b = delta_b(liquidity, lo, price, Rounding::Up).ok_or(ZenithError::MathOverflow)?;

    Ok((to_token_amount(amount_a)?, to_token_amount(amount_b)?))
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
}
