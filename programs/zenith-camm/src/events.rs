//! Emitted events for off-chain indexers and the SDK.

use anchor_lang::prelude::*;

#[event]
pub struct PoolInitialized {
    pub pool: Pubkey,
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub lp_mint: Pubkey,
    pub base_fee_bps: u16,
    pub protocol_fee_rate: u16,
}

#[event]
pub struct LiquidityAdded {
    pub pool: Pubkey,
    pub owner: Pubkey,
    pub amount_a: u64,
    pub amount_b: u64,
    pub shares_minted: u64,
}

#[event]
pub struct LiquidityRemoved {
    pub pool: Pubkey,
    pub owner: Pubkey,
    pub amount_a: u64,
    pub amount_b: u64,
    pub shares_burned: u64,
}

#[event]
pub struct Swap {
    pub pool: Pubkey,
    /// 0 = A→B, 1 = B→A.
    pub direction: u8,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee: u64,
    pub protocol_fee: u64,
}
