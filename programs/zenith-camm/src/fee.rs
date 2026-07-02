//! Swap-fee arithmetic for the constant-product engine.
//!
//! The fee is a flat basis-point cut taken from the swap's *input* token. For an
//! exact-input swap it is deducted before the curve sees the amount; for an
//! exact-output swap the gross input is grossed up so the post-fee remainder
//! still buys the requested output. The fee then splits into a protocol share
//! (accrued to the pool) and an LP share (left in the reserve, compounding into
//! `k`). All of these are pure integer functions so they can be unit-tested and
//! ported bit-exact to the SDK.

use zenith_math::{mul_div, MathResult, Rounding};

use crate::constants::MAX_FEE_BPS;

const BPS: u128 = 10_000;

/// Fee charged on an exact-input swap: `ceil(amount_in * fee_bps / 10000)`.
///
/// Rounded **up** so the pool is never short-changed by fractional fees. The
/// caller subtracts this from `amount_in` before the curve.
pub fn fee_on_input(amount_in: u64, fee_bps: u16) -> MathResult<u64> {
    let fee = mul_div(amount_in as u128, fee_bps as u128, BPS, Rounding::Up)?;
    Ok(fee as u64)
}

/// Gross input needed on an exact-output swap so that, after the fee is removed,
/// `net_in` remains to buy the requested output:
/// `ceil(net_in * 10000 / (10000 - fee_bps))`.
///
/// Rounded **up** (the payer covers the fee). `fee_bps` must be `< 10000`
/// (guaranteed at pool creation), else the whole trade would be fee.
pub fn gross_input_for_net(net_in: u64, fee_bps: u16) -> MathResult<u64> {
    let denom = BPS - fee_bps as u128; // fee_bps < MAX_FEE_BPS, so denom >= 1
    let gross = mul_div(net_in as u128, BPS, denom, Rounding::Up)?;
    Ok(gross as u64)
}

/// Split a fee into `(protocol, lp)`: the protocol takes
/// `floor(fee * rate / 10000)`, the LP share is the remainder. Flooring the
/// protocol cut leaves any rounding dust with the LPs (in the reserve), and
/// `protocol + lp == fee` exactly.
pub fn split_protocol_fee(fee: u64, rate: u16) -> MathResult<(u64, u64)> {
    let protocol = mul_div(fee as u128, rate as u128, BPS, Rounding::Down)? as u64;
    let lp = fee - protocol; // protocol <= fee since rate <= 10000
    Ok((protocol, lp))
}

/// `true` if `fee_bps` is a valid pool fee (strictly below 100%).
pub fn is_valid_fee_bps(fee_bps: u16) -> bool {
    fee_bps < MAX_FEE_BPS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_on_input_rounds_up() {
        // 30 bps of 1000 = 3 exactly.
        assert_eq!(fee_on_input(1000, 30).unwrap(), 3);
        // 30 bps of 1 = 0.003 -> rounds up to 1 (pool favored).
        assert_eq!(fee_on_input(1, 30).unwrap(), 1);
        // zero fee, zero amount
        assert_eq!(fee_on_input(1000, 0).unwrap(), 0);
        assert_eq!(fee_on_input(0, 30).unwrap(), 0);
    }

    #[test]
    fn gross_up_inverts_fee() {
        // For 30 bps, grossing up 997 net should need ~1000 in (ceil).
        let gross = gross_input_for_net(997, 30).unwrap();
        // The post-fee remainder must be at least the requested net.
        let fee = fee_on_input(gross, 30).unwrap();
        assert!(gross - fee >= 997, "gross {gross} fee {fee}");
    }

    #[test]
    fn split_is_exact_and_dust_to_lp() {
        // 20% protocol rate on 100 -> 20 protocol, 80 lp.
        assert_eq!(split_protocol_fee(100, 2000).unwrap(), (20, 80));
        // Rounding dust stays with the LP: floor(7 * 2500/10000) = 1 protocol, 6 lp.
        let (p, l) = split_protocol_fee(7, 2500).unwrap();
        assert_eq!((p, l), (1, 6));
        assert_eq!(p + l, 7);
        // 100% protocol takes all; 0% takes none.
        assert_eq!(split_protocol_fee(50, 10000).unwrap(), (50, 0));
        assert_eq!(split_protocol_fee(50, 0).unwrap(), (0, 50));
    }

    #[test]
    fn fee_validity() {
        assert!(is_valid_fee_bps(0));
        assert!(is_valid_fee_bps(9_999));
        assert!(!is_valid_fee_bps(MAX_FEE_BPS));
    }
}
