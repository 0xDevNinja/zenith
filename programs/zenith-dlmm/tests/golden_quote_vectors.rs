//! Emit cross-language parity vectors for the DLMM quote math: bin prices,
//! single-bin constant-sum fills, and the volatility fee. The SDK's TypeScript
//! port asserts bit-exact equality against these. Run with:
//!   cargo test -p zenith-dlmm --test golden_quote_vectors -- --nocapture
//! It writes sdk/test/fixtures/dlmm_math_vectors.json (read by dlmm-quote.test.ts).

use std::{fs, path::PathBuf};

use zenith_dlmm::fee::compute_variable_fee;
use zenith_dlmm::swap_math::{fill_exact_in, fill_exact_out, Direction};
use zenith_math::{bin_price, Rounding};

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sdk/test/fixtures")
}

#[test]
fn emit_quote_vectors() {
    let mut bin_prices = Vec::new();
    let steps = [1u16, 10, 25, 100, 500];
    for &step in &steps {
        for id in [-500i32, -70, -13, -1, 0, 1, 13, 70, 500] {
            if let Some(p) = bin_price(step, id, Rounding::Down) {
                bin_prices.push(format!(
                    "{{\"binStep\":{step},\"binId\":{id},\"bits\":\"{}\"}}",
                    p.to_bits()
                ));
            }
        }
    }

    // Fills at a few representative prices (bin 0/±1/±13 for step 25).
    let mut fills_in = Vec::new();
    let mut fills_out = Vec::new();
    let price_ids = [-13i32, -1, 0, 1, 13];
    let amounts = [1u64, 7, 100, 9_999, 1_000_000, 1_000_000_000];
    let reserves = [0u64, 1, 50, 100_000, 1_000_000_000];
    for &id in &price_ids {
        let price = bin_price(25, id, Rounding::Down).unwrap();
        for &amt in &amounts {
            for &res in &reserves {
                for (dir, dv) in [(Direction::XtoY, 0u8), (Direction::YtoX, 1u8)] {
                    if let Some(f) = fill_exact_in(amt, res, price, dir) {
                        fills_in.push(format!(
                            "{{\"binId\":{id},\"inAvail\":\"{amt}\",\"reserveOut\":\"{res}\",\"dir\":{dv},\"inUsed\":\"{}\",\"out\":\"{}\",\"drained\":{}}}",
                            f.in_used, f.out, f.drained
                        ));
                    }
                    if let Some(f) = fill_exact_out(amt, res, price, dir) {
                        fills_out.push(format!(
                            "{{\"binId\":{id},\"outNeed\":\"{amt}\",\"reserveOut\":\"{res}\",\"dir\":{dv},\"inUsed\":\"{}\",\"out\":\"{}\",\"drained\":{}}}",
                            f.in_used, f.out, f.drained
                        ));
                    }
                }
            }
        }
    }

    // Volatility-fee vectors over varied windows/params.
    let mut var_fees = Vec::new();
    for (i, &active) in [-30i32, -5, 0, 5, 30].iter().enumerate() {
        for &elapsed in &[0u64, 5, 10, 50, 100, 200] {
            let s = compute_variable_fee(
                active,
                0,
                (i as u128) * 100,
                40,
                elapsed,
                10,
                100,
                5_000,
                100_000,
                25,
                1_000_000,
                1_000,
            );
            var_fees.push(format!(
                "{{\"active\":{active},\"elapsed\":{elapsed},\"va0\":{},\"vr0\":40,\"variableFeeBps\":{},\"va\":\"{}\",\"vr\":\"{}\",\"idxRef\":{}}}",
                (i as u128) * 100,
                s.variable_fee_bps,
                s.volatility_accumulator,
                s.volatility_reference,
                s.index_reference
            ));
        }
    }

    let json = format!(
        "{{\n  \"binPrice\": [{}],\n  \"fillIn\": [{}],\n  \"fillOut\": [{}],\n  \"varFee\": [{}]\n}}\n",
        bin_prices.join(","),
        fills_in.join(","),
        fills_out.join(","),
        var_fees.join(",")
    );

    let dir = out_dir();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("dlmm_math_vectors.json");
    fs::write(&path, json).unwrap();
    println!(
        "wrote {} bin-price, {} fillIn, {} fillOut, {} varFee vectors -> {}",
        bin_prices.len(),
        fills_in.len(),
        fills_out.len(),
        var_fees.len(),
        path.display()
    );
}
