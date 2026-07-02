//! Emits constant-product / LP-share test vectors (inputs + Rust outputs) for
//! the SDK's bit-exact TypeScript port (`@zenith/sdk` `camm` namespace). Run:
//!   cargo test -p zenith-math --test cp_vectors_gen -- --nocapture
//! then capture the `CP_VECTORS_JSON=` line into
//! sdk/test/fixtures/cp_math_vectors.json.
//!
//! Every numeric value is a decimal STRING (u128 exceeds JS's safe integer
//! range); `null` encodes a MathError (overflow / div-by-zero / unsatisfiable).
//! Rounding is 0 = Down, 1 = Up (matches the TS enum).

use zenith_math::{
    in_given_out, initial_shares, matching_amount, out_given_in, shares_from_deposit,
    tokens_for_shares, Rounding,
};

/// Deterministic LCG (Numerical Recipes constants) — reproducible, no rng dep.
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

fn opt(x: zenith_math::MathResult<u128>) -> String {
    match x {
        Ok(v) => format!("\"{v}\""),
        Err(_) => "null".to_string(),
    }
}

/// Curated edge amounts plus random spread, all bounded to realistic reserve
/// magnitudes (u64) so products stay well inside U256.
fn amounts(lcg: &mut Lcg) -> Vec<u128> {
    let mut v = vec![
        0u128,
        1,
        2,
        3,
        1000,
        u64::MAX as u128,
        (u64::MAX as u128) / 2,
    ];
    for _ in 0..24 {
        v.push((lcg.next_u128() % (u64::MAX as u128)) + 1);
    }
    v
}

#[test]
fn emit_cp_vectors() {
    let mut lcg = Lcg(0x05ee_d0c0_ffee_1234);
    let mut swap = String::from("[");
    let mut lp = String::from("[");
    let mut first_swap = true;
    let mut first_lp = true;

    let ins = amounts(&mut lcg);
    // Short second axis keeps the LP cartesian product from exploding while
    // still crossing every amount against zero / one / dust / large.
    let outs = vec![0u128, 1, 1000, (u64::MAX as u128) / 2, u64::MAX as u128];

    // Swap-curve vectors: out_given_in and in_given_out over reserve pairs.
    for &reserve_in in &[1u128, 1000, 1_000_000, u64::MAX as u128] {
        for &reserve_out in &[1u128, 1000, 1_000_000, u64::MAX as u128] {
            for &amount in &ins {
                if !first_swap {
                    swap.push(',');
                }
                first_swap = false;
                swap.push_str(&format!(
                    "{{\"reserveIn\":\"{reserve_in}\",\"reserveOut\":\"{reserve_out}\",\"amount\":\"{amount}\",\"outGivenIn\":{},\"inGivenOut\":{}}}",
                    opt(out_given_in(reserve_in, reserve_out, amount)),
                    opt(in_given_out(reserve_in, reserve_out, amount)),
                ));
            }
        }
    }

    // LP-share vectors: initial_shares, shares_from_deposit, tokens_for_shares
    // (both roundings), matching_amount.
    for &amount_a in &ins {
        for &amount_b in &outs {
            for &supply in &[0u128, 1000, u64::MAX as u128] {
                for &reserve_a in &[1u128, u64::MAX as u128] {
                    for &reserve_b in &[1u128, u64::MAX as u128] {
                        if !first_lp {
                            lp.push(',');
                        }
                        first_lp = false;
                        lp.push_str(&format!(
                            "{{\"amountA\":\"{amount_a}\",\"amountB\":\"{amount_b}\",\"reserveA\":\"{reserve_a}\",\"reserveB\":\"{reserve_b}\",\"supply\":\"{supply}\",\"initialShares\":{},\"sharesFromDeposit\":{},\"tokensForSharesDown\":{},\"tokensForSharesUp\":{},\"matchingAmount\":{}}}",
                            opt(initial_shares(amount_a, amount_b)),
                            opt(shares_from_deposit(amount_a, amount_b, reserve_a, reserve_b, supply)),
                            opt(tokens_for_shares(amount_a, reserve_a, supply, Rounding::Down)),
                            opt(tokens_for_shares(amount_a, reserve_a, supply, Rounding::Up)),
                            opt(matching_amount(amount_a, reserve_a, reserve_b)),
                        ));
                    }
                }
            }
        }
    }
    swap.push(']');
    lp.push(']');

    println!(
        "CP_VECTORS_JSON={{\"minimumLiquidity\":\"{}\",\"swap\":{swap},\"lp\":{lp}}}",
        zenith_math::MINIMUM_LIQUIDITY
    );
}
