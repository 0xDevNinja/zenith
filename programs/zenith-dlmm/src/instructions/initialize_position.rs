//! `initialize_position` — open an empty position over a bin range.
//!
//! A position owns LP shares across an inclusive `[lower_bin_id, upper_bin_id]`
//! range. The account address is a PDA of a caller-supplied `base` pubkey (a
//! throwaway signer), so one owner can hold many positions. Ownership is the
//! `owner` field — there is no position NFT.
//!
//! M4 constraint: the range must lie within a single bin array, so add/remove
//! never have to touch more than one array account. Multi-array positions can
//! come later.

use anchor_lang::prelude::*;
use zenith_math::{bin_price, Rounding};

use crate::constants::{MAX_BINS_PER_POSITION, POSITION_SEED};
use crate::errors::DlmmError;
use crate::events::PositionInitialized;
use crate::state::{BinArray, LbPair, Position};

#[derive(Accounts)]
pub struct InitializePosition<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    /// Unique base pubkey the position PDA is derived from. A signer so the
    /// caller proves they chose it (and it is not an account they don't control).
    pub base: Signer<'info>,

    pub lb_pair: AccountLoader<'info, LbPair>,

    #[account(
        init,
        payer = owner,
        space = Position::LEN,
        seeds = [POSITION_SEED, base.key().as_ref()],
        bump
    )]
    pub position: AccountLoader<'info, Position>,

    pub system_program: Program<'info, System>,
}

/// Open an empty position spanning `width` bins starting at `lower_bin_id`.
pub fn initialize_position(
    ctx: Context<InitializePosition>,
    lower_bin_id: i32,
    width: u32,
) -> Result<()> {
    require!(
        width >= 1 && width as usize <= MAX_BINS_PER_POSITION,
        DlmmError::BinRangeTooWide
    );
    let upper_bin_id = lower_bin_id
        .checked_add(width as i32 - 1)
        .ok_or(DlmmError::BinIdOutOfRange)?;

    // The whole range must sit inside one bin array (M4 single-array model).
    require!(
        BinArray::index_of(lower_bin_id) == BinArray::index_of(upper_bin_id),
        DlmmError::PositionCrossesBinArray
    );

    // Both ends must price (i.e. be inside the supported band for the step).
    let bin_step = ctx.accounts.lb_pair.load()?.bin_step;
    require!(
        bin_price(bin_step, lower_bin_id, Rounding::Down).is_some()
            && bin_price(bin_step, upper_bin_id, Rounding::Down).is_some(),
        DlmmError::BinIdOutOfRange
    );

    let lb_pair_key = ctx.accounts.lb_pair.key();
    let position_key = ctx.accounts.position.key();
    let owner_key = ctx.accounts.owner.key();
    {
        let mut pos = ctx.accounts.position.load_init()?;
        pos.liquidity_shares = [0u128; MAX_BINS_PER_POSITION];
        pos.fee_infos = core::array::from_fn(|_| Default::default());
        pos.lb_pair = lb_pair_key;
        pos.owner = owner_key;
        pos.base = ctx.accounts.base.key();
        pos.lower_bin_id = lower_bin_id;
        pos.upper_bin_id = upper_bin_id;
        pos.bump = ctx.bumps.position;
        pos.padding = [0u8; 7];
    }

    emit!(PositionInitialized {
        lb_pair: lb_pair_key,
        position: position_key,
        owner: owner_key,
        lower_bin_id,
        upper_bin_id,
    });

    Ok(())
}
