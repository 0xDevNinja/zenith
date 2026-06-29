//! `claim_fee` — pay out a position's accrued LP fees without withdrawing
//! liquidity.
//!
//! Each bin tracks per-share fee growth; the position holds a checkpoint and a
//! pending balance per bin. This settles every bin in the position's range
//! (folding `shares * (growth - checkpoint)` into pending and advancing the
//! checkpoint), sums and zeroes the pending across bins, and transfers it out
//! of the reserves under the pair-authority PDA. Effects (settle + zero) happen
//! before the CPI payout.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::constants::PAIR_AUTHORITY_SEED;
use crate::errors::DlmmError;
use crate::events::FeesClaimed;
use crate::state::{BinArray, LbPair, Position};

#[derive(Accounts)]
pub struct ClaimFee<'info> {
    /// Position owner.
    pub owner: Signer<'info>,

    pub lb_pair: AccountLoader<'info, LbPair>,

    #[account(mut)]
    pub position: AccountLoader<'info, Position>,

    /// The bin array covering the position's range (read for fee growth).
    pub bin_array: AccountLoader<'info, BinArray>,

    /// CHECK: PDA that owns the reserves; signs the payout.
    #[account(seeds = [PAIR_AUTHORITY_SEED, lb_pair.key().as_ref()], bump)]
    pub pair_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub reserve_x: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub reserve_y: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_x: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

/// Settle and pay out the position's accrued LP fees.
pub fn claim_fee(ctx: Context<ClaimFee>) -> Result<()> {
    let lb_pair_key = ctx.accounts.lb_pair.key();

    {
        let pair = ctx.accounts.lb_pair.load()?;
        require_keys_eq!(
            ctx.accounts.reserve_x.key(),
            pair.reserve_x,
            DlmmError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.reserve_y.key(),
            pair.reserve_y,
            DlmmError::Unauthorized
        );
    }

    let (total_x, total_y) = {
        let mut pos = ctx.accounts.position.load_mut()?;
        require_keys_eq!(pos.lb_pair, lb_pair_key, DlmmError::Unauthorized);
        require_keys_eq!(pos.owner, ctx.accounts.owner.key(), DlmmError::Unauthorized);
        let (lower, upper) = (pos.lower_bin_id, pos.upper_bin_id);

        let arr = ctx.accounts.bin_array.load()?;
        require_keys_eq!(arr.lb_pair, lb_pair_key, DlmmError::Unauthorized);
        require!(
            arr.index == BinArray::index_of(lower),
            DlmmError::BinArrayIndexMismatch
        );
        // Pin the array to its canonical PDA (defense-in-depth, mirrors swap).
        let (expected, _) = crate::pda::bin_array_pda(&lb_pair_key, arr.index);
        require_keys_eq!(
            ctx.accounts.bin_array.key(),
            expected,
            DlmmError::Unauthorized
        );

        // Settle every bin's accrued fees into the position, then collect them.
        for id in lower..=upper {
            let pos_slot = (id - lower) as usize;
            let slot = arr.slot_of(id).ok_or(DlmmError::BinArrayIndexMismatch)?;
            let bin = &arr.bins[slot];
            pos.settle_bin(pos_slot, bin.fee_growth_x, bin.fee_growth_y)?;
        }
        pos.take_pending()?
    };

    let signer_seeds: &[&[&[u8]]] = &[&[
        PAIR_AUTHORITY_SEED,
        lb_pair_key.as_ref(),
        &[ctx.bumps.pair_authority],
    ]];
    if total_x > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.reserve_x,
            &ctx.accounts.user_token_x,
            total_x,
            signer_seeds,
        )?;
    }
    if total_y > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.reserve_y,
            &ctx.accounts.user_token_y,
            total_y,
            signer_seeds,
        )?;
    }

    emit!(FeesClaimed {
        lb_pair: lb_pair_key,
        position: ctx.accounts.position.key(),
        amount_x: total_x,
        amount_y: total_y,
    });

    Ok(())
}

fn transfer_out<'info>(
    ctx: &Context<ClaimFee<'info>>,
    from: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    amount: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: from.to_account_info(),
                to: to.to_account_info(),
                authority: ctx.accounts.pair_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )
}
