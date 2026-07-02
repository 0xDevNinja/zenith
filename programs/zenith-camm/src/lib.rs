//! Zenith CAMM — full-range constant-product automated market maker.
//!
//! A classic `x*y=k` pool with a fungible LP-share mint: liquidity is fungible
//! (no position NFTs, no price ranges), so an LP is a passive, set-and-forget
//! holder of LP tokens. This engine is the home for idle-reserve yield — a
//! full-range pool keeps most of its capital far from the current price, so
//! those reserves can be lent out for yield (added in a later issue) without the
//! attribution and solvency problems a concentrated engine has.
//!
//! M7 scope (this issue): pool state, LP mint, add/remove liquidity, and swap.
//! The idle-reserve yield vault lands next.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod fee;
pub mod instructions;
pub mod pda;
pub mod state;

pub use constants::*;
pub use errors::CammError;
#[allow(ambiguous_glob_reexports)]
pub use instructions::*;
pub use state::*;

// Program ID. The matching keypair lives in target/deploy/ (gitignored);
// run `anchor keys sync` after generating deploy keypairs to keep this and
// Anchor.toml aligned for an actual deploy.
declare_id!("CjjcK3rnskHswBpTgZquLGgS7P2QyzeaNwwe98FUUdy7");

#[program]
pub mod zenith_camm {
    use super::*;

    /// Create an empty full-range constant-product pool (reserves, LP mint,
    /// locked-liquidity account).
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        base_fee_bps: u16,
        protocol_fee_rate: u16,
    ) -> Result<()> {
        instructions::initialize_pool(ctx, base_fee_bps, protocol_fee_rate)
    }

    /// Deposit both tokens and mint LP shares (first deposit locks the minimum
    /// liquidity; later deposits are trimmed to the pool ratio).
    pub fn add_liquidity(
        ctx: Context<AddLiquidity>,
        desired_a: u64,
        desired_b: u64,
        min_a: u64,
        min_b: u64,
        min_shares: u64,
    ) -> Result<()> {
        instructions::add_liquidity(ctx, desired_a, desired_b, min_a, min_b, min_shares)
    }

    /// Burn LP shares and withdraw the pro-rata reserves.
    pub fn remove_liquidity(
        ctx: Context<RemoveLiquidity>,
        shares: u64,
        min_a: u64,
        min_b: u64,
    ) -> Result<()> {
        instructions::remove_liquidity(ctx, shares, min_a, min_b)
    }

    /// Trade against the curve (ExactIn / ExactOut) with a slippage threshold.
    pub fn swap(
        ctx: Context<Swap>,
        direction: crate::instructions::Direction,
        mode: crate::instructions::SwapMode,
        amount: u64,
        other_amount_threshold: u64,
    ) -> Result<()> {
        instructions::swap(ctx, direction, mode, amount, other_amount_threshold)
    }
}
