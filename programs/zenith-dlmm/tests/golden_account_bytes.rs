//! Emits hex-encoded golden account bytes (discriminator + payload) for the
//! DLMM SDK decoder parity tests. Run with:
//!   cargo test -p zenith-dlmm --test golden_account_bytes -- --nocapture
//! then copy the printed hex into sdk/test/dlmm.test.ts.
//!
//! Field values are deliberately distinct per field so a wrong offset in the
//! TS decoder produces a wrong value rather than silently passing. Signed
//! fields use negative values to exercise the two's-complement reads.

use anchor_lang::{prelude::*, Discriminator};
use bytemuck::Zeroable;
use zenith_dlmm::state::{Bin, BinArray, LbPair, Observation, Oracle, Position, PositionBinData};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn emit<T: bytemuck::Pod + Discriminator>(name: &str, v: &T) {
    let mut data = T::DISCRIMINATOR.to_vec();
    data.extend_from_slice(bytemuck::bytes_of(v));
    println!("{name}={}", hex(&data));
}

#[test]
fn emit_lb_pair() {
    let mut p = LbPair::zeroed();
    p.volatility_accumulator = 1;
    p.volatility_reference = 2;
    p.token_x_mint = Pubkey::new_from_array([11u8; 32]);
    p.token_y_mint = Pubkey::new_from_array([12u8; 32]);
    p.reserve_x = Pubkey::new_from_array([13u8; 32]);
    p.reserve_y = Pubkey::new_from_array([14u8; 32]);
    p.creator = Pubkey::new_from_array([15u8; 32]);
    p.protocol_fee_x = 16;
    p.protocol_fee_y = 17;
    p.activation_point = 18;
    p.last_update_slot = 19;
    p.active_bin_id = -20;
    p.index_reference = -21;
    p.variable_fee_control = 22;
    p.max_volatility_accumulator = 23;
    p.filter_period = 24;
    p.decay_period = 25;
    p.bin_step = 26;
    p.base_fee_bps = 27;
    p.volatility_reduction_factor = 28;
    p.max_dynamic_fee_bps = 29;
    p.protocol_fee_rate = 30;
    p.status = 1;
    p.pair_authority_bump = 31;
    p.pair_bump = 32;
    p.reserve_x_bump = 33;
    p.reserve_y_bump = 34;
    p.token_x_flag = 1;
    p.token_y_flag = 0;
    emit("LBPAIR", &p);
}

#[test]
fn emit_bin_array() {
    let mut a = BinArray::zeroed();
    a.lb_pair = Pubkey::new_from_array([41u8; 32]);
    a.index = -3;
    a.bump = 42;
    a.bins[0] = Bin {
        fee_growth_x: 100,
        fee_growth_y: 101,
        liquidity_supply: 102,
        amount_x: 103,
        amount_y: 104,
    };
    a.bins[1] = Bin {
        fee_growth_x: 200,
        fee_growth_y: 201,
        liquidity_supply: 202,
        amount_x: 203,
        amount_y: 204,
    };
    emit("BINARRAY", &a);
}

#[test]
fn emit_position() {
    let mut p = Position::zeroed();
    p.lb_pair = Pubkey::new_from_array([51u8; 32]);
    p.owner = Pubkey::new_from_array([52u8; 32]);
    p.base = Pubkey::new_from_array([53u8; 32]);
    p.lower_bin_id = -5;
    p.upper_bin_id = 5;
    p.bump = 54;
    p.liquidity_shares[0] = 500;
    p.liquidity_shares[1] = 501;
    p.fee_infos[0] = PositionBinData {
        fee_x_checkpoint: 600,
        fee_y_checkpoint: 601,
        fee_x_pending: 602,
        fee_y_pending: 603,
    };
    emit("POSITION", &p);
}

#[test]
fn emit_oracle() {
    let mut o = Oracle::zeroed();
    o.lb_pair = Pubkey::new_from_array([61u8; 32]);
    o.length = 8;
    o.active_size = 2;
    o.last_index = 1;
    o.bump = 62;
    o.observations[0] = Observation {
        cumulative_active_bin: -700,
        timestamp: 701,
        initialized: 1,
        padding: [0u8; 7],
    };
    o.observations[1] = Observation {
        cumulative_active_bin: 800,
        timestamp: 801,
        initialized: 1,
        padding: [0u8; 7],
    };
    emit("ORACLE", &o);
}
