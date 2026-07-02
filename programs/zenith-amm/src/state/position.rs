//! Liquidity position.
//!
//! Ownership of a position is the **position NFT** (`nft_mint`), not a fungible
//! LP token. Whoever holds the NFT controls the position.

use anchor_lang::prelude::*;

/// A single liquidity position in a pool.
#[account]
#[derive(InitSpace, Debug)]
pub struct Position {
    /// The pool this position belongs to.
    pub pool: Pubkey,
    /// The NFT mint that represents ownership of this position.
    pub nft_mint: Pubkey,
    /// Freely withdrawable liquidity.
    pub liquidity: u128,
    /// Liquidity under a vesting schedule (released over time).
    pub vested_liquidity: u128,
    /// Liquidity locked permanently (never withdrawable).
    pub permanent_locked_liquidity: u128,
    /// Last-observed global fee growth for token A (Q64.64 raw bits).
    pub fee_growth_checkpoint_a: u128,
    /// Last-observed global fee growth for token B (Q64.64 raw bits).
    pub fee_growth_checkpoint_b: u128,
    /// Fees owed in token A, accrued but not yet claimed.
    pub fee_pending_a: u64,
    /// Fees owed in token B, accrued but not yet claimed.
    pub fee_pending_b: u64,
    /// PDA bump.
    pub bump: u8,
    /// Compounding mode: when nonzero, `claim_position_fee` folds owed fees back
    /// into the position's liquidity instead of paying them out.
    pub compounding: u8,
    /// Lower tick bound of this position's price range (inclusive).
    pub tick_lower: i32,
    /// Upper tick bound of this position's price range (exclusive).
    pub tick_upper: i32,
    /// Reserved for forward-compatible fields without a realloc.
    pub reserved: [u8; 55],
}

impl Position {
    /// Total liquidity across all buckets (unlocked + vested + permanent).
    pub fn total_liquidity(&self) -> u128 {
        self.liquidity
            .saturating_add(self.vested_liquidity)
            .saturating_add(self.permanent_locked_liquidity)
    }
}
