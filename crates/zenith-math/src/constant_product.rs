//! Constant-product (`x*y=k`) curve and LP-share math for the full-range
//! engine (`zenith-camm`).
//!
//! Two concerns live here, both pure integer arithmetic with explicit rounding
//! so the pool never rounds against itself:
//!
//! - **Swap curve** — output for an exact input, and input for an exact output,
//!   on the invariant `reserve_in * reserve_out = k`. Fees are the program's
//!   job; these functions take reserves and a raw amount and stay fee-agnostic.
//! - **LP shares** — how many pool shares a deposit mints, and how many tokens
//!   burning shares returns. The first deposit bootstraps with the geometric
//!   mean of the two amounts; later deposits mint proportionally.
//!
//! All intermediate products go through the 256-bit helpers in [`crate::mul_div`]
//! so a `u128 * u128` multiply never wraps.

use ruint::aliases::U256;

use crate::u256::to_u128;
use crate::{mul_div, sqrt_u256, MathError, MathResult, Rounding};

/// Shares permanently locked on the first deposit to defuse the share-inflation
/// (donation) attack: without a floor, the first LP could mint a single share,
/// donate tokens to skew the share price, and dilute the next depositor. Mirrors
/// Uniswap v2's `MINIMUM_LIQUIDITY`. The program mints
/// `initial_shares - MINIMUM_LIQUIDITY` to the first LP and locks the rest.
pub const MINIMUM_LIQUIDITY: u128 = 1_000;

/// Output of a constant-product swap:
/// `out = reserve_out * amount_in / (reserve_in + amount_in)`.
///
/// Rounded **down** — the pool keeps sub-unit dust, so `k` never decreases.
/// `amount_in` is already net of fees (the caller deducts them). Returns
/// [`MathError::Overflow`] only if `reserve_in + amount_in` exceeds `u128`.
pub fn out_given_in(reserve_in: u128, reserve_out: u128, amount_in: u128) -> MathResult<u128> {
    if amount_in == 0 {
        return Ok(0);
    }
    let denom = reserve_in
        .checked_add(amount_in)
        .ok_or(MathError::Overflow)?;
    // denom >= amount_in > 0, so mul_div never divides by zero.
    mul_div(reserve_out, amount_in, denom, Rounding::Down)
}

/// Input required to receive an exact output:
/// `in = reserve_in * amount_out / (reserve_out - amount_out)`.
///
/// Rounded **up** — the payer always covers the curve, so `k` never decreases.
/// Returns [`MathError::DivByZero`] if `amount_out >= reserve_out`: a swap can
/// neither drain nor exceed the output reserve, so the request is unsatisfiable.
pub fn in_given_out(reserve_in: u128, reserve_out: u128, amount_out: u128) -> MathResult<u128> {
    if amount_out == 0 {
        return Ok(0);
    }
    if amount_out >= reserve_out {
        return Err(MathError::DivByZero);
    }
    let denom = reserve_out - amount_out;
    mul_div(reserve_in, amount_out, denom, Rounding::Up)
}

/// Total shares represented by the very first deposit: the geometric mean
/// `sqrt(amount_a * amount_b)`.
///
/// The product is taken in 256 bits so two `u128` amounts cannot overflow; the
/// square root of a value `< 2^256` always fits `u128`. The program mints this
/// minus [`MINIMUM_LIQUIDITY`] to the first LP and locks the remainder.
pub fn initial_shares(amount_a: u128, amount_b: u128) -> MathResult<u128> {
    let product = U256::from(amount_a) * U256::from(amount_b);
    to_u128(sqrt_u256(product))
}

/// Shares minted for a subsequent deposit into a pool that already holds
/// `supply` shares against `reserve_a`/`reserve_b`.
///
/// To hold the share price constant the deposit must match the pool ratio; when
/// it does not, the smaller ratio-implied amount is minted (Uniswap v2), so the
/// depositor is credited only for the fully-matched portion. Both candidates are
/// floored. Returns [`MathError::DivByZero`] if either reserve is zero (an empty
/// pool must go through [`initial_shares`] instead).
pub fn shares_from_deposit(
    amount_a: u128,
    amount_b: u128,
    reserve_a: u128,
    reserve_b: u128,
    supply: u128,
) -> MathResult<u128> {
    let shares_a = mul_div(amount_a, supply, reserve_a, Rounding::Down)?;
    let shares_b = mul_div(amount_b, supply, reserve_b, Rounding::Down)?;
    Ok(shares_a.min(shares_b))
}

/// Tokens returned for burning `shares` of a `supply`-share pool holding
/// `reserve` of that token: `reserve * shares / supply`.
///
/// Callers withdrawing pass [`Rounding::Down`] so an LP can never pull more than
/// its share. `supply == 0` yields `0` (nothing to redeem).
pub fn tokens_for_shares(
    shares: u128,
    reserve: u128,
    supply: u128,
    rounding: Rounding,
) -> MathResult<u128> {
    if supply == 0 {
        return Ok(0);
    }
    mul_div(shares, reserve, supply, rounding)
}

/// Amount of token B that must accompany `amount_a` to preserve the pool ratio:
/// `amount_a * reserve_b / reserve_a`, rounded **up** so the deposit fully backs
/// the shares it mints. Returns [`MathError::DivByZero`] on an empty pool.
pub fn matching_amount(amount_a: u128, reserve_a: u128, reserve_b: u128) -> MathResult<u128> {
    mul_div(amount_a, reserve_b, reserve_a, Rounding::Up)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const D: Rounding = Rounding::Down;
    const U: Rounding = Rounding::Up;

    #[test]
    fn out_given_in_basic() {
        // Balanced 1000/1000 pool, swap 100 in: 1000*100/1100 = 90 (floor of 90.9).
        assert_eq!(out_given_in(1000, 1000, 100).unwrap(), 90);
        // Zero input yields zero output.
        assert_eq!(out_given_in(1000, 1000, 0).unwrap(), 0);
        // Output can approach but never reach the full out-reserve.
        assert!(out_given_in(1, 1000, u64::MAX as u128).unwrap() < 1000);
    }

    #[test]
    fn in_given_out_basic() {
        // Mirror of out_given_in: to get 90 out of 1000/1000 needs 1000*90/910 = 99 (ceil 98.9).
        assert_eq!(in_given_out(1000, 1000, 90).unwrap(), 99);
        assert_eq!(in_given_out(1000, 1000, 0).unwrap(), 0);
        // Cannot drain or exceed the output reserve.
        assert_eq!(in_given_out(1000, 1000, 1000), Err(MathError::DivByZero));
        assert_eq!(in_given_out(1000, 1000, 1001), Err(MathError::DivByZero));
    }

    #[test]
    fn initial_shares_geomean() {
        assert_eq!(initial_shares(1000, 1000).unwrap(), 1000);
        assert_eq!(initial_shares(4, 9).unwrap(), 6);
        // Inexact geometric mean floors: sqrt(2*3)=sqrt(6)=2.449 -> 2.
        assert_eq!(initial_shares(2, 3).unwrap(), 2);
        // Large amounts whose product overflows u128 still resolve via U256.
        let big = u128::MAX;
        assert_eq!(initial_shares(big, big).unwrap(), big);
    }

    #[test]
    fn shares_from_deposit_ratio() {
        // Proportional deposit into a 1000/1000 pool with 1000 shares: +100/+100 -> +100.
        assert_eq!(
            shares_from_deposit(100, 100, 1000, 1000, 1000).unwrap(),
            100
        );
        // Unbalanced deposit is credited only for the matched side (the min).
        assert_eq!(shares_from_deposit(100, 50, 1000, 1000, 1000).unwrap(), 50);
        // Empty reserve is a bootstrap case, not a proportional one.
        assert_eq!(
            shares_from_deposit(100, 100, 0, 1000, 1000),
            Err(MathError::DivByZero)
        );
    }

    #[test]
    fn tokens_for_shares_basic() {
        // Burn 10% of shares -> 10% of each reserve.
        assert_eq!(tokens_for_shares(100, 1000, 1000, D).unwrap(), 100);
        assert_eq!(tokens_for_shares(100, 2000, 1000, D).unwrap(), 200);
        // Down never over-pays; Up is at most one unit more.
        assert_eq!(tokens_for_shares(1, 10, 3, D).unwrap(), 3);
        assert_eq!(tokens_for_shares(1, 10, 3, U).unwrap(), 4);
        assert_eq!(tokens_for_shares(1, 10, 0, D).unwrap(), 0);
    }

    #[test]
    fn matching_amount_preserves_ratio() {
        // 2:1 pool -> depositing 100 A needs 50 B.
        assert_eq!(matching_amount(100, 1000, 500).unwrap(), 50);
        // Rounds up so the deposit fully backs its shares.
        assert_eq!(matching_amount(1, 3, 1).unwrap(), 1);
        assert_eq!(matching_amount(10, 0, 500), Err(MathError::DivByZero));
    }

    proptest! {
        // The invariant never decreases: (reserve_in + in) * (reserve_out - out) >= k.
        // Output is floored, so the product of new reserves is at least the old k.
        #[test]
        fn out_given_in_preserves_k(
            reserve_in in 1u128..=u64::MAX as u128,
            reserve_out in 1u128..=u64::MAX as u128,
            amount_in in 0u128..=u64::MAX as u128,
        ) {
            let out = out_given_in(reserve_in, reserve_out, amount_in).unwrap();
            prop_assert!(out < reserve_out); // never drains the out-reserve
            let k_old = U256::from(reserve_in) * U256::from(reserve_out);
            let k_new = U256::from(reserve_in + amount_in) * U256::from(reserve_out - out);
            prop_assert!(k_new >= k_old);
        }

        // Round trip: the input needed for the output of a given input is never
        // less than that input (the up/down rounding is protocol-favoring).
        #[test]
        fn in_out_round_trip(
            reserve_in in 1u128..=u64::MAX as u128,
            reserve_out in 2u128..=u64::MAX as u128,
            amount_in in 1u128..=u64::MAX as u128,
        ) {
            let out = out_given_in(reserve_in, reserve_out, amount_in).unwrap();
            if out > 0 {
                let back = in_given_out(reserve_in, reserve_out, out).unwrap();
                prop_assert!(back <= amount_in);
            }
        }

        // Burning all supply returns no more than the whole reserve (no phantom tokens).
        #[test]
        fn tokens_for_shares_never_over_withdraw(
            reserve in 0u128..=u64::MAX as u128,
            supply in 1u128..=u64::MAX as u128,
            shares in 0u128..=u64::MAX as u128,
        ) {
            let shares = shares.min(supply);
            let out = tokens_for_shares(shares, reserve, supply, D).unwrap();
            prop_assert!(out <= reserve);
        }
    }
}
