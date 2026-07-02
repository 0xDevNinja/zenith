//! Emits hex-encoded golden account bytes (discriminator + payload) for the
//! SDK decoder parity tests. Run with:
//!   cargo test -p zenith-amm --test golden_account_bytes -- --nocapture
//! then copy the printed hex into sdk/test/coder.test.ts.
//!
//! Field values are deliberately distinct per field so a wrong offset in the
//! TS decoder produces a wrong value rather than silently passing.

use anchor_lang::{prelude::*, Discriminator};
use bytemuck::Zeroable;
use zenith_amm::state::{Config, Pool, Position};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn emit_pool() {
    let mut pool = Pool::zeroed();
    pool.liquidity = 1;
    pool.sqrt_price = 2;
    pool.sqrt_min_price = 3;
    pool.sqrt_max_price = 4;
    pool.fee_growth_global_a = 5;
    pool.fee_growth_global_b = 6;
    pool.sqrt_price_reference = 7;
    pool.volatility_accumulator = 8;
    pool.volatility_reference = 9;
    pool.config = Pubkey::new_from_array([10u8; 32]);
    pool.token_a_mint = Pubkey::new_from_array([11u8; 32]);
    pool.token_b_mint = Pubkey::new_from_array([12u8; 32]);
    pool.token_a_vault = Pubkey::new_from_array([13u8; 32]);
    pool.token_b_vault = Pubkey::new_from_array([14u8; 32]);
    pool.protocol_fee_a = 15;
    pool.protocol_fee_b = 16;
    pool.activation_point = 17;
    pool.position_count = 18;
    pool.last_volatility_update = 19;
    pool.partner_fee_a = 20;
    pool.partner_fee_b = 21;
    pool.base_fee_bps = 22;
    pool.status = 1;
    pool.pool_authority_bump = 23;
    pool.pool_bump = 24;
    pool.token_a_vault_bump = 25;
    pool.token_b_vault_bump = 26;
    pool.token_a_flags = 1;
    pool.token_b_flags = 0;
    pool.tick_spacing = 64;

    let mut data = Pool::DISCRIMINATOR.to_vec();
    data.extend_from_slice(bytemuck::bytes_of(&pool));
    println!("POOL_HEX={}", hex(&data));
}

#[test]
fn emit_position() {
    let position = Position {
        pool: Pubkey::new_from_array([31u8; 32]),
        nft_mint: Pubkey::new_from_array([32u8; 32]),
        liquidity: 33,
        vested_liquidity: 34,
        permanent_locked_liquidity: 35,
        fee_growth_checkpoint_a: 36,
        fee_growth_checkpoint_b: 37,
        fee_pending_a: 38,
        fee_pending_b: 39,
        bump: 40,
        compounding: 1,
        tick_lower: -60,
        tick_upper: 61,
        reserved: [0u8; 55],
    };
    let mut data = Vec::new();
    position.try_serialize(&mut data).unwrap();
    println!("POSITION_HEX={}", hex(&data));
}

#[test]
fn emit_config() {
    let config = Config {
        admin: Pubkey::new_from_array([41u8; 32]),
        fee_authority: Pubkey::new_from_array([42u8; 32]),
        partner: Pubkey::new_from_array([43u8; 32]),
        sqrt_min_price: 44,
        sqrt_max_price: 45,
        fee_period: 46,
        index: 47,
        base_fee_bps: 48,
        protocol_fee_bps: 49,
        cliff_fee_bps: 50,
        reduction_factor: 51,
        max_fee_steps: 52,
        variable_fee_control: 53,
        max_volatility_accumulator: 54,
        filter_period: 55,
        decay_period: 56,
        volatility_reduction_factor: 57,
        max_dynamic_fee_bps: 58,
        partner_fee_bps: 59,
        tick_spacing: 64,
        fee_scheduler_mode: 2,
        bump: 60,
        reserved: [0u8; 14],
    };
    let mut data = Vec::new();
    config.try_serialize(&mut data).unwrap();
    println!("CONFIG_HEX={}", hex(&data));
}
