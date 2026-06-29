//! Zenith DLMM — liquidity-book dynamic market maker.
//!
//! Liquidity lives in discrete price bins (constant-sum per bin, zero in-bin
//! slippage); a swap walks bin to bin across the book. Volatility-based dynamic
//! fees land in M4b. Remaining handlers (position, add/remove liquidity, swap)
//! land in later M4 issues.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod pda;
pub mod state;
pub mod strategy;

pub use constants::*;
pub use errors::DlmmError;
// The `#[program]` macro resolves handlers as `crate::<name>`, so the
// instruction free functions must be re-exported at the crate root. That glob
// overlaps the same-named functions the macro itself generates; the overlap is
// benign (both point at the same handlers), so the lint is allowed.
#[allow(ambiguous_glob_reexports)]
pub use instructions::*;
pub use state::*;

// Program ID. The matching keypair lives in target/deploy/ (gitignored);
// run `anchor keys sync` after generating deploy keypairs to keep this and
// Anchor.toml aligned for an actual deploy.
declare_id!("7pxn8tEm44gXjfPH9YXsLywuYpAbgbxq9nPwG1XQczsz");

#[program]
pub mod zenith_dlmm {
    use super::*;

    /// Create an empty liquidity-book pair at a chosen bin step + active bin.
    pub fn initialize_lb_pair(
        ctx: Context<InitializeLbPair>,
        bin_step: u16,
        active_bin_id: i32,
        base_fee_bps: u16,
    ) -> Result<()> {
        instructions::initialize_lb_pair(ctx, bin_step, active_bin_id, base_fee_bps)
    }

    /// Allocate a bin array (a packed run of bins) for a pair.
    pub fn initialize_bin_array(ctx: Context<InitializeBinArray>, index: i64) -> Result<()> {
        instructions::initialize_bin_array(ctx, index)
    }

    /// Open an empty position over a bin range (within one bin array).
    pub fn initialize_position(
        ctx: Context<InitializePosition>,
        lower_bin_id: i32,
        width: u32,
    ) -> Result<()> {
        instructions::initialize_position(ctx, lower_bin_id, width)
    }

    /// Add liquidity to a position, shaped across its bins by `strategy`
    /// (0 = Spot, 1 = Curve, 2 = BidAsk).
    pub fn add_liquidity_by_strategy(
        ctx: Context<AddLiquidity>,
        amount_x: u64,
        amount_y: u64,
        strategy: u8,
        min_liquidity_shares: u128,
    ) -> Result<()> {
        instructions::add_liquidity_by_strategy(
            ctx,
            amount_x,
            amount_y,
            strategy,
            min_liquidity_shares,
        )
    }

    // TODO(M4): remove_liquidity, swap.
}
