//! Emits tick-math test vectors (inputs + Rust outputs) for the SDK's bit-exact
//! TypeScript port (`@zenith/sdk` AMM tick math, issue #128). Run:
//!   cargo test -p zenith-math --test tick_vectors_gen -- --nocapture
//! then capture the `TICK_VECTORS_JSON=` line into
//! sdk/test/fixtures/tick_math_vectors.json.
//!
//! Q64.64 bit patterns and liquidity are u128 (beyond JS's safe integer range),
//! so every such value is a decimal STRING; `null` encodes an out-of-domain /
//! overflow `None`. `liquidityNet` is a signed decimal string (i128).

use zenith_math::{
    cross_tick_liquidity, fee_growth_inside, sqrt_price_at_tick, tick_at_sqrt_price, Q64x64,
    MAX_TICK, MIN_TICK,
};

/// Deterministic LCG (Numerical Recipes constants).
struct Lcg(u64);
impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn next_u128(&mut self) -> u128 {
        ((self.next_u64() as u128) << 64) | self.next_u64() as u128
    }
}

fn opt_u128(x: Option<u128>) -> String {
    match x {
        Some(v) => format!("\"{v}\""),
        None => "null".to_string(),
    }
}

fn opt_sp(x: Option<Q64x64>) -> String {
    match x {
        Some(v) => format!("\"{}\"", v.to_bits()),
        None => "null".to_string(),
    }
}

/// The tick grid we emit: domain edges, a dense window near zero, spacing-aligned
/// samples across the whole range, and a few out-of-domain probes.
fn ticks() -> Vec<i32> {
    let mut v = vec![
        MIN_TICK,
        MIN_TICK + 1,
        MIN_TICK - 1, // out of domain
        -200_000,
        -100_000,
        -12_345,
        -1000,
        -64,
        -10,
        -1,
        0,
        1,
        10,
        64,
        1000,
        12_345,
        100_000,
        200_000,
        MAX_TICK - 1,
        MAX_TICK,
        MAX_TICK + 1, // out of domain
    ];
    // dense window near zero
    for t in -20..=20 {
        v.push(t);
    }
    // spacing-aligned sweep
    let mut t = -200_000;
    while t <= 200_000 {
        v.push(t);
        t += 25_000;
    }
    v.sort_unstable();
    v.dedup();
    v
}

#[test]
fn emit_tick_vectors() {
    let mut lcg = Lcg(0x71c4_dec0_de12_3457);

    // 1. sqrt_price_at_tick over the tick grid.
    let mut sp = String::from("[");
    let mut first = true;
    for &t in &ticks() {
        if !first {
            sp.push(',');
        }
        first = false;
        sp.push_str(&format!(
            "{{\"tick\":{t},\"sqrtPrice\":{}}}",
            opt_sp(sqrt_price_at_tick(t))
        ));
    }
    sp.push(']');

    // 2. tick_at_sqrt_price: exact tick boundaries, midpoints, and clamps.
    let mut inv = String::from("[");
    let mut first = true;
    let push_inv = |inv: &mut String, first: &mut bool, bits: u128| {
        if !*first {
            inv.push(',');
        }
        *first = false;
        inv.push_str(&format!(
            "{{\"sqrtPrice\":\"{bits}\",\"tick\":{}}}",
            tick_at_sqrt_price(Q64x64::from_bits(bits))
        ));
    };
    for &t in &ticks() {
        if let Some(spt) = sqrt_price_at_tick(t) {
            push_inv(&mut inv, &mut first, spt.to_bits());
            // midpoint to the next tick (floors back to t)
            if let Some(spt1) = sqrt_price_at_tick(t + 1) {
                let mid = (spt.to_bits() + spt1.to_bits()) / 2;
                push_inv(&mut inv, &mut first, mid);
            }
        }
    }
    // explicit clamp probes
    push_inv(&mut inv, &mut first, 1); // below domain
    push_inv(&mut inv, &mut first, u128::MAX); // above domain
    inv.push(']');

    // 3. fee_growth_inside across the three positional cases + wrap.
    let mut fee = String::from("[");
    let mut first = true;
    let ranges = [(-100i32, 100i32), (-10, 10), (0, 60)];
    let currents = [-200i32, -50, 0, 50, 200];
    let globals: [u128; 3] = [1000, u128::MAX - 4, 0];
    for &(lo, hi) in &ranges {
        for &cur in &currents {
            for &g in &globals {
                let ga = g;
                let gb = g.wrapping_add(777);
                let ol_a = lcg.next_u128();
                let ol_b = lcg.next_u128();
                let ou_a = lcg.next_u128();
                let ou_b = lcg.next_u128();
                let (ia, ib) = fee_growth_inside(lo, hi, cur, ga, gb, ol_a, ol_b, ou_a, ou_b);
                if !first {
                    fee.push(',');
                }
                first = false;
                fee.push_str(&format!(
                    "{{\"tickLower\":{lo},\"tickUpper\":{hi},\"currentTick\":{cur},\"feeGrowthGlobalA\":\"{ga}\",\"feeGrowthGlobalB\":\"{gb}\",\"feeGrowthOutsideLowerA\":\"{ol_a}\",\"feeGrowthOutsideLowerB\":\"{ol_b}\",\"feeGrowthOutsideUpperA\":\"{ou_a}\",\"feeGrowthOutsideUpperB\":\"{ou_b}\",\"insideA\":\"{ia}\",\"insideB\":\"{ib}\"}}"
                ));
            }
        }
    }
    fee.push(']');

    // 4. cross_tick_liquidity: up/down, positive/negative net, guards.
    let mut cross = String::from("[");
    let mut first = true;
    let liqs: [u128; 4] = [0, 1000, 1_000_000_000, u128::MAX];
    let nets: [i128; 7] = [0, 300, -300, 500, -500, i128::MAX, i128::MIN];
    for &liq in &liqs {
        for &net in &nets {
            for &zfo in &[false, true] {
                if !first {
                    cross.push(',');
                }
                first = false;
                cross.push_str(&format!(
                    "{{\"liquidity\":\"{liq}\",\"liquidityNet\":\"{net}\",\"zeroForOne\":{zfo},\"result\":{}}}",
                    opt_u128(cross_tick_liquidity(liq, net, zfo))
                ));
            }
        }
    }
    cross.push(']');

    println!(
        "TICK_VECTORS_JSON={{\"minTick\":{MIN_TICK},\"maxTick\":{MAX_TICK},\"sqrtPriceAtTick\":{sp},\"tickAtSqrtPrice\":{inv},\"feeGrowthInside\":{fee},\"crossTick\":{cross}}}"
    );
}
