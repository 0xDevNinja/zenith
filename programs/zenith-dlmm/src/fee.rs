//! Dynamic (volatility) fee for the liquidity book.
//!
//! The swap fee is `base + variable`. The variable part is driven by a
//! volatility accumulator that grows as the active bin moves away from a
//! reference bin and decays when the pair is idle — the discrete-bin analog of
//! the AMM's price-move dynamic fee. Because each bin is `bin_step` basis points
//! apart, crossing `|Δbin|` bins is a price move of `|Δbin| * bin_step` bps, so
//! the accumulator math is identical to the AMM's once the move is expressed in
//! bps.
//!
//! The fee is computed on the **pre-swap** active bin (like the AMM): a swap is
//! charged for the volatility built up by prior trades, and its own bin movement
//! surcharges the next swap. This avoids any circular dependency between the fee
//! and the swap's output.

use zenith_math::{mul_div, mul_shr, shl_div, MathResult, Rounding, SCALE_OFFSET};

/// Basis-points denominator.
pub const BPS_DENOMINATOR: u128 = 10_000;
/// Denominator for `variable = va^2 * control / 1e9` (matches the AMM).
pub const DYNAMIC_FEE_DENOMINATOR: u128 = 1_000_000_000;
/// Total fee is clamped strictly below 100%.
pub const MAX_FEE_BPS: u16 = 10_000;

/// Price move (bps) implied by the active bin sitting `|active - reference|`
/// bins away: each bin is `bin_step` bps wide.
pub fn bin_move_bps(index_reference: i32, active_bin: i32, bin_step: u16) -> u128 {
    (active_bin.abs_diff(index_reference) as u128).saturating_mul(bin_step as u128)
}

/// Decay the stored accumulator by idle time into the reference the next swap
/// builds on: unchanged within the filter window, scaled by
/// `reduction_factor_bps` between filter and decay, fully reset past decay.
pub fn decayed_volatility_reference(
    accumulator: u128,
    elapsed: u64,
    filter_period: u32,
    decay_period: u32,
    reduction_factor_bps: u16,
) -> u128 {
    if elapsed >= decay_period as u64 {
        0
    } else if elapsed >= filter_period as u64 {
        mul_div(
            accumulator,
            reduction_factor_bps as u128,
            BPS_DENOMINATOR,
            Rounding::Down,
        )
        .unwrap_or(0)
    } else {
        accumulator
    }
}

/// New accumulator after a move: `reference + move`, capped at `max_va`.
pub fn accumulate_volatility(reference: u128, move_bps: u128, max_va: u32) -> u128 {
    reference.saturating_add(move_bps).min(max_va as u128)
}

/// Variable surcharge in bps: `va^2 * control / 1e9`, capped at `max_dynamic`.
/// Zero `control` disables it.
pub fn variable_fee_bps(va: u128, variable_fee_control: u32, max_dynamic_fee_bps: u16) -> u16 {
    if variable_fee_control == 0 {
        return 0;
    }
    let sq = va.saturating_mul(va);
    let fee = sq.saturating_mul(variable_fee_control as u128) / DYNAMIC_FEE_DENOMINATOR;
    fee.min(max_dynamic_fee_bps as u128) as u16
}

/// `base + variable`, clamped strictly below 100%.
pub fn total_fee_bps(base_fee_bps: u16, variable_fee_bps: u16) -> u16 {
    (base_fee_bps as u32 + variable_fee_bps as u32).min(MAX_FEE_BPS as u32 - 1) as u16
}

/// Split a swap fee into `(protocol_share, lp_share)` by `protocol_fee_rate`
/// (bps). The protocol share rounds down (favoring LPs) and the two parts sum
/// to exactly `total_fee`.
pub fn split_protocol_fee(total_fee: u64, protocol_fee_rate: u16) -> (u64, u64) {
    let protocol = ((total_fee as u128 * protocol_fee_rate as u128) / BPS_DENOMINATOR) as u64;
    (protocol, total_fee - protocol)
}

/// Per-share fee-growth increment when `lp_fee` tokens are earned by a bin with
/// `supply` LP shares: `lp_fee << 64 / supply` (Q64.64 per share, rounded
/// down). Returns 0 if there is no fee or no supply.
pub fn fee_growth_delta(lp_fee: u64, supply: u128) -> u128 {
    if lp_fee == 0 || supply == 0 {
        return 0;
    }
    shl_div(lp_fee as u128, SCALE_OFFSET, supply, Rounding::Down).unwrap_or(0)
}

/// Token fees owed for `shares` between a `checkpoint` and the current per-share
/// `growth` (both Q64.64 raw bits): `shares * (growth - checkpoint) >> 64`,
/// rounded down. The subtraction wraps, so it stays correct after the growth
/// accumulator overflows u128. Errors (rather than silently forfeiting) if the
/// product somehow exceeds u128.
pub fn owed_fee(shares: u128, growth: u128, checkpoint: u128) -> MathResult<u128> {
    let delta = growth.wrapping_sub(checkpoint);
    mul_shr(shares, delta, SCALE_OFFSET, Rounding::Down)
}

/// Volatility state folded forward by a swap; the caller persists all four onto
/// the pair.
pub struct VariableFeeState {
    /// Surcharge to add to the base fee this swap, bps.
    pub variable_fee_bps: u16,
    /// Volatility accumulator after the (pre-swap) move (reference + move).
    pub volatility_accumulator: u128,
    /// Decayed carry for the next swap (fixed within a volatility window).
    pub volatility_reference: u128,
    /// Reference bin moves are measured from (re-set when a window starts).
    pub index_reference: i32,
}

/// Fold the pre-swap active bin into the volatility state and derive the
/// variable surcharge.
///
/// A "volatility window" begins whenever `elapsed >= filter_period`: the
/// reference bin re-sets to the current active bin and the *reference* becomes
/// the decayed prior accumulator (scaled between filter and decay, zero past
/// decay). Within a window the reference is held fixed, so the accumulator is
/// `reference + |active - reference_bin| * bin_step`, capped at `max_va`.
#[allow(clippy::too_many_arguments)]
pub fn compute_variable_fee(
    active_bin: i32,
    index_reference: i32,
    volatility_accumulator: u128,
    volatility_reference: u128,
    elapsed: u64,
    filter_period: u32,
    decay_period: u32,
    reduction_factor_bps: u16,
    max_va: u32,
    bin_step: u16,
    variable_fee_control: u32,
    max_dynamic_fee_bps: u16,
) -> VariableFeeState {
    let new_window = elapsed >= filter_period as u64;
    let (reference, ref_bin) = if new_window {
        let carry = decayed_volatility_reference(
            volatility_accumulator,
            elapsed,
            filter_period,
            decay_period,
            reduction_factor_bps,
        );
        (carry, active_bin)
    } else {
        (volatility_reference, index_reference)
    };

    let move_bps = bin_move_bps(ref_bin, active_bin, bin_step);
    let va = accumulate_volatility(reference, move_bps, max_va);
    let fee = variable_fee_bps(va, variable_fee_control, max_dynamic_fee_bps);
    VariableFeeState {
        variable_fee_bps: fee,
        volatility_accumulator: va,
        volatility_reference: reference,
        index_reference: ref_bin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_grows_with_bin_distance() {
        assert_eq!(bin_move_bps(0, 0, 25), 0);
        assert_eq!(bin_move_bps(0, 4, 25), 100); // 4 bins * 25 bps
        assert_eq!(bin_move_bps(3, -2, 10), 50); // |(-2)-3| * 10
    }

    #[test]
    fn reference_decays_by_window() {
        // within filter: unchanged
        assert_eq!(decayed_volatility_reference(1000, 5, 10, 100, 5000), 1000);
        // between filter and decay: scaled by reduction (50%)
        assert_eq!(decayed_volatility_reference(1000, 50, 10, 100, 5000), 500);
        // past decay: reset
        assert_eq!(decayed_volatility_reference(1000, 100, 10, 100, 5000), 0);
    }

    #[test]
    fn variable_fee_is_quadratic_and_capped() {
        // control 0 disables
        assert_eq!(variable_fee_bps(10_000, 0, 1000), 0);
        // va^2 * control / 1e9: va=10000, control=1e6 -> 1e8*1e6/1e9 = 100000 -> capped
        assert_eq!(variable_fee_bps(10_000, 1_000_000, 1000), 1000);
        // small va stays small: va=100, control=1e6 -> 1e4*1e6/1e9 = 10 bps
        assert_eq!(variable_fee_bps(100, 1_000_000, 1000), 10);
    }

    #[test]
    fn total_clamps_below_full() {
        assert_eq!(total_fee_bps(30, 20), 50);
        assert_eq!(total_fee_bps(9000, 5000), MAX_FEE_BPS - 1);
    }

    #[test]
    fn fee_growth_and_owed_round_trip() {
        // 1000 fee over a bin with 4 shares -> each share owed 250.
        let g = fee_growth_delta(1000, 4);
        assert_eq!(owed_fee(4, g, 0).unwrap(), 1000); // all shares
        assert_eq!(owed_fee(1, g, 0).unwrap(), 250); // one share
                                                     // checkpoint already at growth -> nothing owed.
        assert_eq!(owed_fee(4, g, g).unwrap(), 0);
        // no supply / no fee -> zero growth.
        assert_eq!(fee_growth_delta(0, 4), 0);
        assert_eq!(fee_growth_delta(1000, 0), 0);
        // splitting the supply's shares never owes more than the fee (floor each).
        let total: u128 = (0..4).map(|_| owed_fee(1, g, 0).unwrap()).sum();
        assert!(total <= 1000);
    }

    #[test]
    fn owed_fee_survives_growth_wraparound() {
        // checkpoint near u128::MAX, growth wrapped past 0.
        let checkpoint = u128::MAX - 10;
        let growth = fee_growth_delta(100, 2).wrapping_add(checkpoint);
        assert_eq!(owed_fee(2, growth, checkpoint).unwrap(), 100);
    }

    #[test]
    fn protocol_split_is_exact() {
        // 20% protocol of 1000 -> (200, 800), sums to total.
        assert_eq!(split_protocol_fee(1000, 2000), (200, 800));
        // rounds the protocol share down (LP-favoring): 1/3 of 100 -> 33.
        let (p, lp) = split_protocol_fee(100, 3333);
        assert_eq!(p, 33);
        assert_eq!(p + lp, 100);
        // edges
        assert_eq!(split_protocol_fee(1000, 0), (0, 1000)); // all LP
        assert_eq!(split_protocol_fee(1000, 10_000), (1000, 0)); // all protocol
    }

    #[test]
    fn new_window_resets_reference_and_carries_decayed() {
        // idle past filter (=10) but within decay (=100): new window, reference
        // = decayed prior accumulator (2000 * 50% = 1000), ref bin -> active 7.
        let s = compute_variable_fee(
            7, 0, 2000, 1500, 50, 10, 100, 5000, 100_000, 25, 1_000_000, 1000,
        );
        assert_eq!(s.index_reference, 7);
        assert_eq!(s.volatility_reference, 1000);
        // move from new ref bin (7) to active (7) is 0 -> va == reference.
        assert_eq!(s.volatility_accumulator, 1000);
    }

    #[test]
    fn within_window_accumulates_from_fixed_reference() {
        // elapsed (5) < filter (10): same window, reference bin stays 3.
        let s = compute_variable_fee(8, 3, 0, 500, 5, 10, 100, 5000, 100_000, 20, 1_000_000, 5000);
        assert_eq!(s.index_reference, 3); // unchanged
        assert_eq!(s.volatility_reference, 500); // carried
                                                 // move = |8-3| * 20 = 100; va = 500 + 100 = 600.
        assert_eq!(s.volatility_accumulator, 600);
    }
}
