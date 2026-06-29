//! `initialize_oracle` — create a pair's TWAP ring buffer.
//!
//! Allocates the [`Oracle`] account (a fixed-capacity ring) for a pair and sets
//! its configured `length` (how many observations the window can reach back
//! over before the oldest is overwritten). Swaps then record into it.

use anchor_lang::prelude::*;

use crate::constants::{ORACLE_CAPACITY, ORACLE_SEED};
use crate::errors::DlmmError;
use crate::state::{LbPair, Oracle};

#[derive(Accounts)]
pub struct InitializeOracle<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The pair this oracle serves. Loaded to confirm it is a real pair.
    pub lb_pair: AccountLoader<'info, LbPair>,

    #[account(
        init,
        payer = payer,
        space = Oracle::LEN,
        seeds = [ORACLE_SEED, lb_pair.key().as_ref()],
        bump
    )]
    pub oracle: AccountLoader<'info, Oracle>,

    pub system_program: Program<'info, System>,
}

/// Create the oracle ring buffer with `length` slots (`1..=ORACLE_CAPACITY`).
pub fn initialize_oracle(ctx: Context<InitializeOracle>, length: u16) -> Result<()> {
    require!(
        length >= 1 && length as usize <= ORACLE_CAPACITY,
        DlmmError::InvalidOracleLength
    );
    let _ = ctx.accounts.lb_pair.load()?;

    let lb_pair_key = ctx.accounts.lb_pair.key();
    let mut oracle = ctx.accounts.oracle.load_init()?;
    oracle.lb_pair = lb_pair_key;
    oracle.length = length;
    oracle.active_size = 0;
    oracle.last_index = 0;
    oracle.bump = ctx.bumps.oracle;
    oracle.padding = [0u8; 9];
    // `observations` are left zeroed by `init`.
    Ok(())
}
