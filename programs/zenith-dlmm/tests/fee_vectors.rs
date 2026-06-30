//! Consolidated reference vectors + fuzz for the DLMM fee, volatility, per-bin
//! fee-accrual, and TWAP math. Runs in CI via `cargo test -p zenith-dlmm`.
//!
//! These exercise the public math directly (no on-chain harness), pinning known
//! values and asserting the invariants the money handlers rely on.

use proptest::prelude::*;

use zenith_dlmm::fee::{
    compute_variable_fee, fee_growth_delta, owed_fee, split_protocol_fee, total_fee_bps,
    variable_fee_bps,
};
use zenith_dlmm::state::Oracle;
use zenith_dlmm::strategy::{distribute, plan_deposit, Strategy};
use zenith_dlmm::swap_math::{fill_exact_in, Direction};

// ---------------------------------------------------------------------------
// 1. Volatility vectors: bursts accumulate, idle decays, long idle resets.
// ---------------------------------------------------------------------------

// A fixed fee config for the volatility sequences.
const FILTER: u32 = 10;
const DECAY: u32 = 100;
const REDUCTION: u16 = 5_000; // 50%
const MAX_VA: u32 = 100_000;
const BIN_STEP: u16 = 25;
const CONTROL: u32 = 1_000_000;
const MAX_DYN: u16 = 1_000;

/// Drive the volatility state one swap forward, returning the new state.
fn step(
    va: u128,
    vr: u128,
    index_ref: i32,
    active_bin: i32,
    elapsed: u64,
) -> (u128, u128, i32, u16) {
    let s = compute_variable_fee(
        active_bin, index_ref, va, vr, elapsed, FILTER, DECAY, REDUCTION, MAX_VA, BIN_STEP,
        CONTROL, MAX_DYN,
    );
    (
        s.volatility_accumulator,
        s.volatility_reference,
        s.index_reference,
        s.variable_fee_bps,
    )
}

#[test]
fn volatility_burst_accumulates() {
    // Same window (elapsed < FILTER): reference bin stays put, so each step away
    // raises the accumulator by bin_step per bin of distance.
    let (mut va, mut vr, mut idx) = (0u128, 0u128, 0i32);
    // first swap seeds the window at bin 0
    let (a, b, c, _) = step(va, vr, idx, 0, 0);
    (va, vr, idx) = (a, b, c);
    assert_eq!(idx, 0);
    // price jumps to bin 4 quickly -> va = |4-0|*25 = 100
    let (a, b, c, _) = step(va, vr, idx, 4, 2);
    (va, vr, idx) = (a, b, c);
    assert_eq!(va, 100);
    assert_eq!(idx, 0); // reference unchanged within the window
                        // further to bin 10 -> va = 10*25 = 250 (from the SAME reference)
    let (a, _, _, _) = step(va, vr, idx, 10, 3);
    assert_eq!(a, 250);
}

#[test]
fn volatility_decays_then_resets() {
    // Build some volatility, then go idle.
    let (va, vr, idx, _) = step(0, 0, 0, 0, 0);
    let (va, _vr, idx, _) = step(va, vr, idx, 8, 2); // va = 8*25 = 200
    assert_eq!(va, 200);

    // Idle within [FILTER, DECAY): new window, reference decays to 50%.
    let (va2, vr2, idx2, _) = step(va, _vr, idx, 8, 50);
    assert_eq!(vr2, 100); // 200 * 50%
    assert_eq!(idx2, 8); // reference re-anchored at the current bin
    assert_eq!(va2, 100); // move from new ref (8) to active (8) is 0

    // Idle past DECAY: full reset.
    let (va3, vr3, idx3, _) = step(va2, vr2, idx2, 3, 200);
    assert_eq!(vr3, 0);
    assert_eq!(va3, 0);
    assert_eq!(idx3, 3);
}

#[test]
fn volatility_steady_state_is_base_only() {
    // No movement across many windows -> accumulator stays 0, no surcharge.
    let (mut va, mut vr, mut idx) = (0u128, 0u128, 5i32);
    let (a, b, c, _) = step(va, vr, idx, 5, 0);
    (va, vr, idx) = (a, b, c);
    for k in 1..20 {
        let (a, b, c, fee) = step(va, vr, idx, 5, k * 50);
        (va, vr, idx) = (a, b, c);
        assert_eq!(va, 0);
        assert_eq!(fee, 0);
    }
}

// ---------------------------------------------------------------------------
// 2. Fee split vectors: protocol + LP == total, exactly.
// ---------------------------------------------------------------------------

#[test]
fn fee_split_vectors() {
    let cases = [
        (1000u64, 0u16, 0u64, 1000u64),
        (1000, 10_000, 1000, 0),
        (1000, 2_000, 200, 800),
        (999, 3_333, 332, 667),
        (1, 5_000, 0, 1),
        (7, 2_500, 1, 6),
    ];
    for (fee, rate, exp_p, exp_lp) in cases {
        let (p, lp) = split_protocol_fee(fee, rate);
        assert_eq!((p, lp), (exp_p, exp_lp), "fee {fee} rate {rate}");
        assert_eq!(p + lp, fee);
    }
}

// ---------------------------------------------------------------------------
// 3. Per-bin accrual vectors across positions sharing a bin.
// ---------------------------------------------------------------------------

#[test]
fn per_bin_accrual_splits_by_share() {
    // Bin with two LPs: 3 and 1 shares (supply 4). 800 fee accrues.
    let supply = 4u128;
    let growth = fee_growth_delta(800, supply);
    let owed_big = owed_fee(3, growth, 0).unwrap();
    let owed_small = owed_fee(1, growth, 0).unwrap();
    assert_eq!(owed_big, 600); // 3/4
    assert_eq!(owed_small, 200); // 1/4
    assert_eq!(owed_big + owed_small, 800); // no dust here
                                            // a second accrual against the same checkpoint advances correctly
    let growth2 = growth.wrapping_add(fee_growth_delta(400, supply));
    assert_eq!(owed_fee(3, growth2, growth).unwrap(), 300); // only the new 400's share
}

// ---------------------------------------------------------------------------
// 4. TWAP vectors over recorded sequences.
// ---------------------------------------------------------------------------

fn oracle(length: u16) -> Oracle {
    let mut o: Oracle = bytemuck::Zeroable::zeroed();
    o.length = length;
    o
}

#[test]
fn twap_vectors() {
    // Constant bin 7 -> TWAP 7.
    let mut o = oracle(8);
    o.record(7, 0);
    o.record(7, 100);
    assert_eq!(o.twap(7, 200, 200), Some(7));

    // Two-segment: bin 2 for [0,100], bin 8 for [100,300]. Over 300 slots:
    // (2*100 + 8*200)/300 = (200+1600)/300 = 1800/300 = 6.
    let mut o = oracle(8);
    o.record(2, 0);
    o.record(2, 100);
    assert_eq!(o.twap(8, 300, 300), Some(6));
    // sub-window [200,300] is all bin 8.
    assert_eq!(o.twap(8, 300, 100), Some(8));
}

// ---------------------------------------------------------------------------
// 5. Fuzz: bounds + no leakage across the fee/volatility/accrual/swap math.
// ---------------------------------------------------------------------------

proptest! {
    /// The volatility accumulator never exceeds max_va and the surcharge never
    /// exceeds max_dynamic, for any (move, params).
    #[test]
    fn variable_fee_is_bounded(
        active in -50_000i32..=50_000,
        index_ref in -50_000i32..=50_000,
        vr in 0u128..=200_000,
        elapsed in 0u64..=1_000,
        control in 0u32..=5_000_000,
        max_dyn in 0u16..=5_000,
    ) {
        let s = compute_variable_fee(
            active, index_ref, vr, vr, elapsed, FILTER, DECAY, REDUCTION, MAX_VA,
            BIN_STEP, control, max_dyn,
        );
        prop_assert!(s.volatility_accumulator <= MAX_VA as u128);
        prop_assert!(s.variable_fee_bps <= max_dyn);
        // total fee always strictly below 100%.
        prop_assert!(total_fee_bps(9_000, s.variable_fee_bps) <= 9_999);
        // surcharge derived from the capped accumulator only grows with control.
        let f0 = variable_fee_bps(s.volatility_accumulator, 0, max_dyn);
        prop_assert_eq!(f0, 0);
    }

    /// Fee split always conserves and never lets the protocol take more than the
    /// fee.
    #[test]
    fn fee_split_conserves(fee in 0u64..=u64::MAX, rate in 0u16..=10_000) {
        let (p, lp) = split_protocol_fee(fee, rate);
        prop_assert!(p <= fee);
        prop_assert_eq!(p as u128 + lp as u128, fee as u128);
    }

    /// Per-bin accrual never owes more than was deposited (floor dust stays in
    /// the pool), and shares of the supply are monotone.
    #[test]
    fn accrual_never_over_owes(
        lp_fee in 0u64..=1_000_000_000u64,
        supply in 1u128..=1_000_000u128,
        a in 0u128..=1_000_000u128,
    ) {
        let a = a.min(supply);
        let b = supply - a;
        let growth = fee_growth_delta(lp_fee, supply);
        let owed_a = owed_fee(a, growth, 0).unwrap();
        let owed_b = owed_fee(b, growth, 0).unwrap();
        prop_assert!(owed_a + owed_b <= lp_fee as u128);
        // a larger share never owes less.
        if a >= b {
            prop_assert!(owed_a >= owed_b);
        }
    }

    /// A TWAP over any recorded sequence stays within the min/max bin seen.
    #[test]
    fn twap_within_observed_range(
        bins in proptest::collection::vec(-1000i32..=1000, 2..12),
    ) {
        let mut o = oracle(16);
        let mut t = 0u64;
        for &bin in &bins {
            o.record(bin, t);
            t += 10;
        }
        let current = *bins.last().unwrap();
        if let Some(tw) = o.twap(current, t + 10, 10_000) {
            let lo = *bins.iter().min().unwrap() as i64;
            let hi = *bins.iter().max().unwrap() as i64;
            prop_assert!(tw >= lo && tw <= hi, "twap {tw} outside [{lo},{hi}]");
        }
    }

    /// A single-bin swap fill never produces more output than the reserve nor
    /// consumes more than offered (the swap-step bound the walk relies on).
    #[test]
    fn swap_fill_is_bounded(
        in_avail in 0u64..=u64::MAX,
        reserve in 0u64..=u64::MAX,
        price_bits in 1u128..=(u128::MAX >> 1),
    ) {
        let price = zenith_math::Q64x64::from_bits(price_bits);
        for dir in [Direction::XtoY, Direction::YtoX] {
            if let Some(f) = fill_exact_in(in_avail, reserve, price, dir) {
                prop_assert!(f.out <= reserve);
                prop_assert!(f.in_used <= in_avail);
            }
        }
    }

    /// Strategy distribution always sums to the total (no tokens created/lost).
    #[test]
    fn distribution_conserves(total in 0u64..=u64::MAX, count in 1u32..=70) {
        for strat in [Strategy::Spot, Strategy::Curve, Strategy::BidAsk] {
            let parts = distribute(total, count, strat).unwrap();
            let sum: u128 = parts.iter().map(|&p| p as u128).sum();
            prop_assert_eq!(sum, total as u128);
        }
    }

    /// A two-sided deposit plan conserves both token totals.
    #[test]
    fn plan_deposit_conserves(
        lower in -50i32..=0, width in 1i32..=20, ax in 0u64..=1_000_000, ay in 0u64..=1_000_000,
    ) {
        let upper = lower + width - 1;
        // active in the middle so both sides exist.
        let active = (lower + upper) / 2;
        if let Ok(plan) = plan_deposit(lower, upper, active, ax, ay, Strategy::Spot) {
            let (sx, sy) = plan.iter().fold((0u64, 0u64), |(x, y), b| (x + b.x, y + b.y));
            prop_assert_eq!((sx, sy), (ax, ay));
        }
    }
}
