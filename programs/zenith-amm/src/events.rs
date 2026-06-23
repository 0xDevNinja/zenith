//! Program events.

use anchor_lang::prelude::*;

/// Emitted when a pool is created and its first position opened.
#[event]
pub struct PoolInitialized {
    /// The new pool.
    pub pool: Pubkey,
    /// Token A (base) mint.
    pub token_a_mint: Pubkey,
    /// Token B (quote) mint.
    pub token_b_mint: Pubkey,
    /// Initial sqrt price (Q64.64 raw bits).
    pub sqrt_price: u128,
    /// Lower band bound (Q64.64 raw bits).
    pub sqrt_min_price: u128,
    /// Upper band bound (Q64.64 raw bits).
    pub sqrt_max_price: u128,
    /// Initial active liquidity seeded by the creator.
    pub liquidity: u128,
    /// The first position's NFT mint.
    pub position_nft_mint: Pubkey,
    /// Token A deposited to seed the position.
    pub amount_a: u64,
    /// Token B deposited to seed the position.
    pub amount_b: u64,
}
