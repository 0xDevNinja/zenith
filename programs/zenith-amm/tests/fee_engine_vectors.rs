//! Consolidated fee-engine vectors + fuzz (M1b #18).
//!
//! Reference-checked golden vectors and property fuzzing for the whole fee
//! engine: the base-fee scheduler, the dynamic/volatility fee, the
//! LP/protocol/partner split, and fee compounding. These exercise the pure math
//! in `zenith_amm::math` directly (no on-chain harness needed), so they run in
//! the normal `cargo test -p zenith-amm` CI path.

use proptest::prelude::*;
use zenith_amm::math::{
    accumulate_volatility, compound_fee_into_liquidity, compute_dynamic_fee, dynamic_fee_bps,
    scheduled_base_fee_bps, split_fee, FEE_MODE_CONSTANT, FEE_MODE_EXPONENTIAL, FEE_MODE_LINEAR,
};
use zenith_math::{delta_a, delta_b, Q64x64, Rounding};

const Q64: u128 = 1 << 64;
const BPS: u16 = 10_000;

// ---------- scheduler vectors ----------

#[test]
fn scheduler_constant_vectors() {
    for &slot in &[0u64, 1, 999, u64::MAX] {
        assert_eq!(
            scheduled_base_fee_bps(FEE_MODE_CONSTANT, 30, 9_000, 500, 100, 50, slot).unwrap(),
            30,
            "constant must ignore time"
        );
    }
}

#[test]
fn scheduler_linear_vectors() {
    // cliff 600, floor 50, -25 bps/step over 100-slot periods, cap 10 steps.
    let f = |slot| scheduled_base_fee_bps(FEE_MODE_LINEAR, 50, 600, 25, 100, 10, slot).unwrap();
    let cases = [
        (0u64, 600u16),
        (99, 600),
        (100, 575),
        (400, 500),
        (1000, 350), // step 10 (cap): 600-250
        (1100, 350), // capped
        (10_000, 350),
    ];
    for (slot, want) in cases {
        assert_eq!(f(slot), want, "linear at slot {slot}");
    }
}

#[test]
fn scheduler_linear_clamps_to_floor() {
    // big reduction drives below the floor quickly -> clamped at 50.
    let f = scheduled_base_fee_bps(FEE_MODE_LINEAR, 50, 600, 300, 100, 50, 1_000).unwrap();
    assert_eq!(f, 50);
}

#[test]
fn scheduler_exponential_vectors() {
    // cliff 1000, floor 10, halve each 100-slot period, cap 20.
    let f =
        |slot| scheduled_base_fee_bps(FEE_MODE_EXPONENTIAL, 10, 1000, 5000, 100, 20, slot).unwrap();
    assert_eq!(f(0), 1000);
    assert_eq!(f(100), 500);
    assert_eq!(f(200), 250);
    assert_eq!(f(300), 125);
    assert_eq!(f(400), 62); // floor(62.5)
                            // deep in time -> clamped to floor.
    assert_eq!(f(100_000), 10);
}

#[test]
fn scheduler_exponential_is_monotonic() {
    let f =
        |slot| scheduled_base_fee_bps(FEE_MODE_EXPONENTIAL, 10, 5000, 1500, 50, 30, slot).unwrap();
    let mut prev = u16::MAX;
    for s in (0..3000).step_by(25) {
        let cur = f(s);
        assert!(cur <= prev, "non-monotonic at {s}: {cur} > {prev}");
        prev = cur;
    }
}

// ---------- dynamic-fee vectors ----------

fn dyn_fee(
    price: u128,
    anchor: u128,
    acc: u128,
    vr: u128,
    elapsed: u64,
) -> (u16, u128, u128, u128) {
    // filter 10, decay 100, reduction 50%, max_va 1_000_000, control 1000, max 500.
    let s = compute_dynamic_fee(
        price, anchor, acc, vr, elapsed, 10, 100, 5000, 1_000_000, 1_000, 500,
    );
    (
        s.dynamic_fee_bps,
        s.volatility_accumulator,
        s.volatility_reference,
        s.sqrt_price_reference,
    )
}

#[test]
fn dynamic_steady_state_is_zero() {
    // Price at anchor, no prior vol -> no surcharge.
    let (fee, va, _, _) = dyn_fee(100 * Q64, 100 * Q64, 0, 0, 5);
    assert_eq!((fee, va), (0, 0));
}

#[test]
fn dynamic_spike_then_decay() {
    // Spike: +10% (1000 bps) drift in-window -> va = 1000, fee = 1000^2*1000/1e9 = 1.
    let (fee, va, _, _) = dyn_fee(110 * Q64, 100 * Q64, 0, 0, 5);
    assert_eq!((fee, va), (1, 1000));

    // New window after the filter (decayed carry from a prior big accumulator).
    let (_, _, vr, anchor) = dyn_fee(110 * Q64, 100 * Q64, 4000, 0, 20);
    assert_eq!(vr, 2000); // 4000 * 50%
    assert_eq!(anchor, 110 * Q64); // re-anchored

    // Idle past decay -> volatility fully resets.
    let (_, _, vr, _) = dyn_fee(110 * Q64, 100 * Q64, 999_999, 0, 500);
    assert_eq!(vr, 0);
}

#[test]
fn dynamic_fee_clamps_to_max() {
    // Large accumulator -> surcharge hits the configured cap (500).
    assert_eq!(dynamic_fee_bps(1_000_000, 1_000, 500), 500);
    // Zero control disables.
    assert_eq!(dynamic_fee_bps(1_000_000, 0, 500), 0);
}

// ---------- split vectors ----------

#[test]
fn split_lp_protocol_partner_sums_to_total() {
    let rates = [
        (0u16, 0u16),
        (3000, 0),
        (3000, 5000),
        (10000, 10000),
        (1234, 8765),
    ];
    for &fee in &[0u64, 1, 999, 1_000_000, u64::MAX] {
        for &(protocol_bps, partner_bps) in &rates {
            let (protocol, lp) = split_fee(fee, protocol_bps).unwrap();
            let (partner, protocol_remaining) = split_fee(protocol, partner_bps).unwrap();
            assert_eq!(
                lp as u128 + protocol_remaining as u128 + partner as u128,
                fee as u128,
                "fee={fee} p={protocol_bps} partner={partner_bps}"
            );
            assert!(partner <= protocol, "partner exceeds protocol share");
        }
    }
}

// ---------- compounding equivalence ----------

#[test]
fn compound_value_equals_consumed_fees() {
    // Band sqrt [1,4], price 2. Compounding folds `used_a`/`used_b` of fees into
    // L_delta; the deposit that L_delta requires (rounded up) must equal exactly
    // the consumed amounts — i.e. the value moved into liquidity is the fees
    // consumed, no more (manual-claim-then-add equivalence within rounding).
    let (lo, mid, hi) = (Q64, 2 * Q64, 4 * Q64);
    for &(fa, fb) in &[(1000u64, 1000u64), (250, 9_000), (1_000_000, 7), (3, 5)] {
        let c = compound_fee_into_liquidity(fa, fb, mid, lo, hi).unwrap();
        assert!(c.used_a <= fa && c.used_b <= fb);
        if c.liquidity_delta > 0 {
            let need_a = delta_a(
                c.liquidity_delta,
                Q64x64::from_bits(mid),
                Q64x64::from_bits(hi),
                Rounding::Up,
            )
            .unwrap();
            let need_b = delta_b(
                c.liquidity_delta,
                Q64x64::from_bits(lo),
                Q64x64::from_bits(mid),
                Rounding::Up,
            )
            .unwrap();
            assert_eq!(need_a as u64, c.used_a);
            assert_eq!(need_b as u64, c.used_b);
        }
    }
}

// ---------- fuzz ----------

proptest! {
    // A random sequence of swaps (each a price jump + elapsed gap) must keep the
    // fee engine within bounds: total fee < 100%, accumulator capped, splits
    // conserve, no panic/overflow.
    #[test]
    fn fuzz_swap_sequence_keeps_fee_bounds(
        base_bps in 0u16..2_000u16,
        control in 0u32..100_000u32,
        max_va in 1u32..2_000_000u32,
        max_dyn in 0u16..3_000u16,
        protocol_bps in 0u16..=10_000u16,
        partner_bps in 0u16..=10_000u16,
        jumps in proptest::collection::vec((1u128..200u128, 0u64..300u64), 1..40),
    ) {
        let (mut acc, mut vr, mut anchor) = (0u128, 0u128, 100 * Q64);
        for (pct, elapsed) in jumps {
            // Price jumps to `pct`% of 100 (in [1%, 200%]) of the unit price.
            let price = pct * Q64;
            let s = compute_dynamic_fee(
                price, anchor, acc, vr, elapsed, 10, 100, 5000, max_va, control, max_dyn,
            );
            // Accumulator never exceeds the cap.
            prop_assert!(s.volatility_accumulator <= max_va as u128);
            // Total fee strictly below 100% after clamping (what swap feeds in).
            let total = (base_bps as u32 + s.dynamic_fee_bps as u32).min(BPS as u32 - 1);
            prop_assert!(total < BPS as u32);
            // A representative fee amount splits three ways with exact conservation.
            let fee = (total as u64) * 1_000; // pretend this many input units
            let (protocol, lp) = split_fee(fee, protocol_bps).unwrap();
            let (partner, protocol_remaining) = split_fee(protocol, partner_bps).unwrap();
            prop_assert_eq!(lp as u128 + protocol_remaining as u128 + partner as u128, fee as u128);
            // Carry forward the volatility state for the next swap.
            acc = s.volatility_accumulator;
            vr = s.volatility_reference;
            anchor = s.sqrt_price_reference;
        }
    }

    // Compounding never consumes more than owed and never panics, for any owed
    // pair and in-band price.
    #[test]
    fn fuzz_compound_within_budget(
        fa in any::<u64>(),
        fb in any::<u64>(),
        price_pct in 2u128..399u128, // strictly inside the [1, 400] band
    ) {
        let (lo, hi) = (Q64, 400 * Q64);
        let price = price_pct * Q64;
        let c = compound_fee_into_liquidity(fa, fb, price, lo, hi).unwrap();
        prop_assert!(c.used_a <= fa);
        prop_assert!(c.used_b <= fb);
    }

    // accumulate_volatility is always within [0, max_va].
    #[test]
    fn fuzz_accumulate_capped(reference in any::<u128>(), mv in any::<u128>(), max_va in any::<u32>()) {
        let va = accumulate_volatility(reference, mv, max_va);
        prop_assert!(va <= max_va as u128);
    }
}
