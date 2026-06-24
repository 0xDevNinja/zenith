//! Emits shared math test vectors (inputs + Rust outputs) for the SDK's
//! bit-exact TS port. Run with:
//!   cargo test -p zenith-amm --test golden_math_vectors -- --nocapture
//! then capture the `VECTORS_JSON=` line into sdk/test/fixtures/math_vectors.json.
//!
//! Every numeric value is emitted as a decimal STRING (u128/u64 exceed JS's
//! safe integer range). `null` encodes a None/overflow/revert result. Rounding
//! is 0 = Down, 1 = Up (matches the TS enum).

use zenith_amm::math::{compute_swap_step, SwapDirection, SwapMode};
use zenith_math::{
    delta_a, delta_b, liquidity_from_amount_a, liquidity_from_amount_b, mul_div, mul_shr,
    next_sqrt_price_from_amount_x, next_sqrt_price_from_amount_y, price_from_sqrt_price, shl_div,
    sqrt_price_from_price, Q64x64, Rounding,
};

const ONE: u128 = 1u128 << 64;

/// Deterministic LCG (Numerical Recipes constants) — no rng dependency, fully
/// reproducible so regenerated fixtures are stable.
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

fn opt(x: Option<u128>) -> String {
    match x {
        Some(v) => format!("\"{v}\""),
        None => "null".to_string(),
    }
}

fn rounds() -> [Rounding; 2] {
    [Rounding::Down, Rounding::Up]
}
fn r_code(r: Rounding) -> u8 {
    match r {
        Rounding::Down => 0,
        Rounding::Up => 1,
    }
}

/// Curated edge values shared by the u128-domain ops.
fn values() -> Vec<u128> {
    vec![
        0,
        1,
        2,
        3,
        7,
        10,
        1000,
        1u128 << 32,
        1u128 << 63,
        ONE,
        ONE + 1,
        2 * ONE,
        4 * ONE,
        3u128 << 63,
        7u128 << 62,
        1u128 << 96,
        1u128 << 127,
        u128::MAX,
    ]
}

#[test]
fn emit_vectors() {
    let mut out = String::from("{");

    // --- mul_div / mul_shr / shl_div ---
    let vals = values();
    let denoms = [1u128, 2, 3, 7, 10, ONE, 1u128 << 96, u128::MAX];
    let shifts = [0u32, 1, 63, 64, 127, 128, 200, 255];
    let mut lcg = Lcg(0x5EED_1234_ABCD_0001);

    let mut mul_div_v = Vec::new();
    for &a in &vals {
        for &b in &vals {
            for &d in &denoms {
                for r in rounds() {
                    mul_div_v.push(format!(
                        "{{\"a\":\"{a}\",\"b\":\"{b}\",\"d\":\"{d}\",\"r\":{},\"out\":{}}}",
                        r_code(r),
                        opt(mul_div(a, b, d, r).ok())
                    ));
                }
            }
        }
    }
    for _ in 0..200 {
        let (a, b, d) = (lcg.next_u128(), lcg.next_u128(), lcg.next_u128());
        for r in rounds() {
            mul_div_v.push(format!(
                "{{\"a\":\"{a}\",\"b\":\"{b}\",\"d\":\"{d}\",\"r\":{},\"out\":{}}}",
                r_code(r),
                opt(mul_div(a, b, d, r).ok())
            ));
        }
    }
    out.push_str(&format!("\"mul_div\":[{}],", mul_div_v.join(",")));

    let mut mul_shr_v = Vec::new();
    for &a in &vals {
        for &b in &vals {
            for &s in &shifts {
                for r in rounds() {
                    mul_shr_v.push(format!(
                        "{{\"a\":\"{a}\",\"b\":\"{b}\",\"s\":{s},\"r\":{},\"out\":{}}}",
                        r_code(r),
                        opt(mul_shr(a, b, s, r).ok())
                    ));
                }
            }
        }
    }
    out.push_str(&format!("\"mul_shr\":[{}],", mul_shr_v.join(",")));

    let mut shl_div_v = Vec::new();
    for &a in &vals {
        for &s in &shifts {
            for &d in &denoms {
                for r in rounds() {
                    shl_div_v.push(format!(
                        "{{\"a\":\"{a}\",\"s\":{s},\"d\":\"{d}\",\"r\":{},\"out\":{}}}",
                        r_code(r),
                        opt(shl_div(a, s, d, r).ok())
                    ));
                }
            }
        }
    }
    out.push_str(&format!("\"shl_div\":[{}],", shl_div_v.join(",")));

    // --- Q64x64 methods ---
    let mut q_ratio = Vec::new();
    let mut q_mul = Vec::new();
    let mut q_div = Vec::new();
    let mut q_recip = Vec::new();
    let mut q_mul_int = Vec::new();
    let mut q_div_int = Vec::new();
    for &a in &vals {
        for r in rounds() {
            q_recip.push(format!(
                "{{\"a\":\"{a}\",\"r\":{},\"out\":{}}}",
                r_code(r),
                opt(Q64x64::from_bits(a).recip(r).map(|q| q.to_bits()))
            ));
        }
        for &b in &vals {
            for r in rounds() {
                let qa = Q64x64::from_bits(a);
                let qb = Q64x64::from_bits(b);
                q_ratio.push(format!(
                    "{{\"a\":\"{a}\",\"b\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(Q64x64::from_ratio(a, b, r).map(|q| q.to_bits()))
                ));
                q_mul.push(format!(
                    "{{\"a\":\"{a}\",\"b\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(qa.mul(qb, r).map(|q| q.to_bits()))
                ));
                q_div.push(format!(
                    "{{\"a\":\"{a}\",\"b\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(qa.div(qb, r).map(|q| q.to_bits()))
                ));
                q_mul_int.push(format!(
                    "{{\"bits\":\"{a}\",\"amt\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(qa.mul_int(b, r))
                ));
                q_div_int.push(format!(
                    "{{\"bits\":\"{a}\",\"amt\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(qa.div_int(b, r))
                ));
            }
        }
    }
    out.push_str(&format!("\"q64_from_ratio\":[{}],", q_ratio.join(",")));
    out.push_str(&format!("\"q64_mul\":[{}],", q_mul.join(",")));
    out.push_str(&format!("\"q64_div\":[{}],", q_div.join(",")));
    out.push_str(&format!("\"q64_recip\":[{}],", q_recip.join(",")));
    out.push_str(&format!("\"q64_mul_int\":[{}],", q_mul_int.join(",")));
    out.push_str(&format!("\"q64_div_int\":[{}],", q_div_int.join(",")));

    // --- sqrt-price <-> price ---
    let mut spfp = Vec::new();
    for &num in &vals {
        for &den in &denoms {
            spfp.push(format!(
                "{{\"num\":\"{num}\",\"den\":\"{den}\",\"out\":{}}}",
                opt(sqrt_price_from_price(num, den).map(|q| q.to_bits()))
            ));
        }
    }
    out.push_str(&format!("\"sqrt_price_from_price\":[{}],", spfp.join(",")));

    let mut pfsp = Vec::new();
    for &sp in &vals {
        for r in rounds() {
            pfsp.push(format!(
                "{{\"sp\":\"{sp}\",\"r\":{},\"out\":{}}}",
                r_code(r),
                opt(price_from_sqrt_price(Q64x64::from_bits(sp), r).map(|q| q.to_bits()))
            ));
        }
    }
    out.push_str(&format!("\"price_from_sqrt_price\":[{}],", pfsp.join(",")));

    // --- deltas + liquidity inverses ---
    // sqrt-price pairs: curated ordered bits + random.
    let mut sp_pairs: Vec<(u128, u128)> = vec![
        (ONE, 2 * ONE),
        (ONE, 4 * ONE),
        (2 * ONE, 4 * ONE),
        (3u128 << 63, 7u128 << 62),
        (1, ONE),
        (ONE, ONE), // degenerate
        (0, ONE),   // zero lo
    ];
    for _ in 0..60 {
        sp_pairs.push((lcg.next_u64() as u128, lcg.next_u64() as u128));
    }
    let liqs = [1u128, 7, 1000, 1_000_000, 1u128 << 40, u128::MAX];

    let mut da = Vec::new();
    let mut db = Vec::new();
    let mut la = Vec::new();
    let mut lb = Vec::new();
    for &(a, b) in &sp_pairs {
        let (qa, qb) = (Q64x64::from_bits(a), Q64x64::from_bits(b));
        for &l in &liqs {
            for r in rounds() {
                da.push(format!(
                    "{{\"l\":\"{l}\",\"a\":\"{a}\",\"b\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(delta_a(l, qa, qb, r))
                ));
                db.push(format!(
                    "{{\"l\":\"{l}\",\"a\":\"{a}\",\"b\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(delta_b(l, qa, qb, r))
                ));
                la.push(format!(
                    "{{\"amt\":\"{l}\",\"a\":\"{a}\",\"b\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(liquidity_from_amount_a(l, qa, qb, r))
                ));
                lb.push(format!(
                    "{{\"amt\":\"{l}\",\"a\":\"{a}\",\"b\":\"{b}\",\"r\":{},\"out\":{}}}",
                    r_code(r),
                    opt(liquidity_from_amount_b(l, qa, qb, r))
                ));
            }
        }
    }
    out.push_str(&format!("\"delta_a\":[{}],", da.join(",")));
    out.push_str(&format!("\"delta_b\":[{}],", db.join(",")));
    out.push_str(&format!("\"liq_from_a\":[{}],", la.join(",")));
    out.push_str(&format!("\"liq_from_b\":[{}],", lb.join(",")));

    // --- next sqrt price ---
    let mut nx = Vec::new();
    let mut ny = Vec::new();
    let sps = [ONE, 2 * ONE, 4 * ONE, 10 * ONE, 1u128 << 96];
    let amts = [0u128, 1, 1000, 1_000_000, 1u128 << 40, u128::MAX];
    let nliqs = [0u128, 1, 1000, 1_000_000, 1u128 << 50];
    for &sp in &sps {
        for &l in &nliqs {
            for &amt in &amts {
                for add in [true, false] {
                    nx.push(format!(
                        "{{\"sp\":\"{sp}\",\"l\":\"{l}\",\"amt\":\"{amt}\",\"add\":{add},\"out\":{}}}",
                        opt(next_sqrt_price_from_amount_x(Q64x64::from_bits(sp), l, amt, add)
                            .map(|q| q.to_bits()))
                    ));
                    ny.push(format!(
                        "{{\"sp\":\"{sp}\",\"l\":\"{l}\",\"amt\":\"{amt}\",\"add\":{add},\"out\":{}}}",
                        opt(next_sqrt_price_from_amount_y(Q64x64::from_bits(sp), l, amt, add)
                            .map(|q| q.to_bits()))
                    ));
                }
            }
        }
    }
    out.push_str(&format!("\"next_x\":[{}],", nx.join(",")));
    out.push_str(&format!("\"next_y\":[{}],", ny.join(",")));

    // --- compute_swap_step ---
    let mut steps = Vec::new();
    let bands = [(ONE, 2 * ONE, 4 * ONE), (ONE, 3 * ONE, 4 * ONE)];
    let sl = [1000u64, 1_000_000, 10_000_000];
    let samt = [1u64, 1000, 10_000, 1_000_000, u64::MAX];
    let sfee = [0u16, 1, 30, 100, 500, 9999];
    let dirs = [SwapDirection::AToB, SwapDirection::BToA];
    let modes = [SwapMode::ExactIn, SwapMode::ExactOut, SwapMode::PartialFill];
    for &(min, sp, max) in &bands {
        for &l in &sl {
            for &amt in &samt {
                for &fee in &sfee {
                    for dir in dirs {
                        for mode in modes {
                            let res =
                                compute_swap_step(sp, l as u128, min, max, dir, mode, amt, fee);
                            let dir_s = match dir {
                                SwapDirection::AToB => "AToB",
                                SwapDirection::BToA => "BToA",
                            };
                            let mode_s = match mode {
                                SwapMode::ExactIn => "ExactIn",
                                SwapMode::ExactOut => "ExactOut",
                                SwapMode::PartialFill => "PartialFill",
                            };
                            let out = match res {
                                Ok(s) => format!(
                                    "{{\"nextSqrtPrice\":\"{}\",\"amountIn\":\"{}\",\"amountOut\":\"{}\",\"fee\":\"{}\",\"amountRemaining\":\"{}\"}}",
                                    s.next_sqrt_price, s.amount_in, s.amount_out, s.fee, s.amount_remaining
                                ),
                                Err(_) => "null".to_string(),
                            };
                            steps.push(format!(
                                "{{\"sp\":\"{sp}\",\"l\":\"{l}\",\"min\":\"{min}\",\"max\":\"{max}\",\"dir\":\"{dir_s}\",\"mode\":\"{mode_s}\",\"amt\":\"{amt}\",\"fee\":{fee},\"out\":{out}}}"
                            ));
                        }
                    }
                }
            }
        }
    }
    out.push_str(&format!("\"swap_step\":[{}]", steps.join(",")));

    out.push('}');
    println!("VECTORS_JSON={out}");
}
