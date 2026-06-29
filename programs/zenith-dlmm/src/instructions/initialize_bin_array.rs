//! `initialize_bin_array` — allocate a bin array for a pair.
//!
//! Bins are too small to each own an account, so a contiguous run of
//! [`crate::constants::MAX_BINS_PER_ARRAY`] bins is packed into one account
//! addressed by a signed array index. Liquidity providers create the arrays
//! covering the bins they deposit into; this just zero-initializes one.

use anchor_lang::prelude::*;

use crate::constants::BIN_ARRAY_SEED;
use crate::events::BinArrayInitialized;
use crate::state::{BinArray, LbPair};

#[derive(Accounts)]
#[instruction(index: i64)]
pub struct InitializeBinArray<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The pair this array belongs to. Loaded to confirm it is a real pair
    /// before an array is derived against its key.
    pub lb_pair: AccountLoader<'info, LbPair>,

    #[account(
        init,
        payer = payer,
        space = BinArray::LEN,
        seeds = [BIN_ARRAY_SEED, lb_pair.key().as_ref(), &index.to_le_bytes()],
        bump
    )]
    pub bin_array: AccountLoader<'info, BinArray>,

    pub system_program: Program<'info, System>,
}

/// Create and zero-initialize the bin array at `index`.
pub fn initialize_bin_array(ctx: Context<InitializeBinArray>, index: i64) -> Result<()> {
    // Confirm the pair account is valid (discriminator check).
    let _ = ctx.accounts.lb_pair.load()?;

    let lb_pair_key = ctx.accounts.lb_pair.key();
    let bin_array_key = ctx.accounts.bin_array.key();
    {
        let mut arr = ctx.accounts.bin_array.load_init()?;
        arr.lb_pair = lb_pair_key;
        arr.index = index;
        arr.bump = ctx.bumps.bin_array;
        arr.padding = [0u8; 7];
        // `bins` are left zeroed by `init`.
    }

    let (lower_bin_id, upper_bin_id) = BinArray::bounds(index);
    emit!(BinArrayInitialized {
        lb_pair: lb_pair_key,
        bin_array: bin_array_key,
        index,
        lower_bin_id,
        upper_bin_id,
    });

    Ok(())
}
