//! `init_tick_array` — lazily create a tick-array account for a pool.
//!
//! Tick arrays are created on demand (by whoever first needs a tick in that
//! range, who pays the rent). This handler only allocates and stamps the array's
//! identity — pool + start index; all ticks start zeroed (uninitialized). The
//! liquidity handlers (#125) populate individual ticks.

use anchor_lang::prelude::*;
use zenith_math::{MAX_TICK, MIN_TICK};

use crate::constants::TICK_ARRAY_SEED;
use crate::errors::ZenithError;
use crate::state::{Pool, TickArray};

#[derive(Accounts)]
#[instruction(start_tick_index: i32)]
pub struct InitTickArray<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Pool the array belongs to; supplies the tick spacing. Only
    /// owner/discriminator-checked (Anchor guarantees it is a genuine Zenith
    /// `Pool`): this instruction moves no funds and merely allocates a
    /// zero-initialized array bound to `pool.key()` via the tick-array PDA, so
    /// any real pool is a safe target.
    pub pool: AccountLoader<'info, Pool>,

    #[account(
        init,
        payer = payer,
        space = TickArray::LEN,
        seeds = [TICK_ARRAY_SEED, pool.key().as_ref(), &start_tick_index.to_le_bytes()],
        bump
    )]
    pub tick_array: AccountLoader<'info, TickArray>,

    pub system_program: Program<'info, System>,
}

/// Allocate the tick array whose first tick is `start_tick_index`.
///
/// `start_tick_index` must be a multiple of `tick_spacing · TICKS_PER_ARRAY`
/// (so arrays tile the tick space without gaps or overlap) and lie within the
/// supported tick domain.
pub fn init_tick_array(ctx: Context<InitTickArray>, start_tick_index: i32) -> Result<()> {
    let tick_spacing = ctx.accounts.pool.load()?.tick_spacing;
    require!(tick_spacing != 0, ZenithError::InvalidTickRange);

    let span = TickArray::span(tick_spacing);
    // Must be aligned to a whole-array boundary.
    require!(start_tick_index % span == 0, ZenithError::TickArrayMismatch);
    // Must sit inside the usable tick domain.
    require!(
        (MIN_TICK..=MAX_TICK).contains(&start_tick_index),
        ZenithError::InvalidTickRange
    );

    let mut array = ctx.accounts.tick_array.load_init()?;
    array.pool = ctx.accounts.pool.key();
    array.start_tick_index = start_tick_index;
    // `load_init` zero-fills the account, so every tick is uninitialized and the
    // padding is already zero.
    Ok(())
}
