//! `remove_liquidity` — withdraw a percentage of a position's bins.
//!
//! The caller passes `bps` (1..=10000); that fraction of the position's shares
//! is burned in every bin of its range and the pro-rata slice of each bin's
//! reserves is returned. Conversions round down, so a withdrawal never returns
//! more than the burned shares are worth. When a bin's share supply reaches
//! zero its residual reserves are zeroed, keeping the `supply == 0` ⇔ empty
//! invariant that `add_liquidity` relies on.
//!
//! Removal is allowed even on a disabled pair, so LPs can always exit. M4
//! scope: single-bin-array positions, classic SPL Token only.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};
use zenith_math::Rounding;

use crate::constants::PAIR_AUTHORITY_SEED;
use crate::errors::DlmmError;
use crate::events::LiquidityRemoved;
use crate::share_math::{shares_for_bps, tokens_for_shares, BPS_DENOMINATOR};
use crate::state::{BinArray, LbPair, Position};

#[derive(Accounts)]
pub struct RemoveLiquidity<'info> {
    /// Position owner.
    pub owner: Signer<'info>,

    pub lb_pair: AccountLoader<'info, LbPair>,

    #[account(mut)]
    pub position: AccountLoader<'info, Position>,

    /// The bin array covering the position's range (single-array positions).
    #[account(mut)]
    pub bin_array: AccountLoader<'info, BinArray>,

    /// CHECK: PDA that owns the reserves; signs payouts. Seed-derived from the
    /// pair, so it can only move this pair's funds.
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

/// Remove `bps`/10000 of the position's shares from each bin, returning the
/// pro-rata token amounts. Reverts if either return is below its `min_*` floor.
pub fn remove_liquidity(
    ctx: Context<RemoveLiquidity>,
    bps: u16,
    min_amount_x: u64,
    min_amount_y: u64,
) -> Result<()> {
    require!((1..=BPS_DENOMINATOR).contains(&bps), DlmmError::InvalidBps);

    let lb_pair_key = ctx.accounts.lb_pair.key();

    // Verify the reserves are the pair's. Removal is allowed on a disabled pair.
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

    let (total_x, total_y, total_burned);
    {
        let mut pos = ctx.accounts.position.load_mut()?;
        require_keys_eq!(pos.lb_pair, lb_pair_key, DlmmError::Unauthorized);
        require_keys_eq!(pos.owner, ctx.accounts.owner.key(), DlmmError::Unauthorized);
        let (lower, upper) = (pos.lower_bin_id, pos.upper_bin_id);

        let mut arr = ctx.accounts.bin_array.load_mut()?;
        require_keys_eq!(arr.lb_pair, lb_pair_key, DlmmError::Unauthorized);
        require!(
            arr.index == BinArray::index_of(lower),
            DlmmError::BinArrayIndexMismatch
        );

        let mut x_out_total: u64 = 0;
        let mut y_out_total: u64 = 0;
        let mut burned: u128 = 0;
        for id in lower..=upper {
            let pos_slot = (id - lower) as usize;
            let pos_shares = pos.liquidity_shares[pos_slot];
            if pos_shares == 0 {
                continue;
            }
            let remove = shares_for_bps(pos_shares, bps).map_err(|_| DlmmError::MathOverflow)?;
            if remove == 0 {
                continue;
            }

            let slot = arr.slot_of(id).ok_or(DlmmError::BinArrayIndexMismatch)?;
            let bin = &mut arr.bins[slot];

            // Settle the bin's accrued fees into the position before its shares
            // shrink, so the removed shares' earned fees are kept (as pending,
            // claimable via claim_fee) rather than forfeited.
            pos.settle_bin(pos_slot, bin.fee_growth_x, bin.fee_growth_y)?;

            let x_out =
                tokens_for_shares(bin.amount_x, remove, bin.liquidity_supply, Rounding::Down)
                    .map_err(|_| DlmmError::MathOverflow)?;
            let y_out =
                tokens_for_shares(bin.amount_y, remove, bin.liquidity_supply, Rounding::Down)
                    .map_err(|_| DlmmError::MathOverflow)?;

            bin.amount_x = bin
                .amount_x
                .checked_sub(x_out)
                .ok_or(DlmmError::MathOverflow)?;
            bin.amount_y = bin
                .amount_y
                .checked_sub(y_out)
                .ok_or(DlmmError::MathOverflow)?;
            bin.liquidity_supply = bin
                .liquidity_supply
                .checked_sub(remove)
                .ok_or(DlmmError::InsufficientLiquidity)?;
            // Defensive: keep the supply == 0 ⇔ empty-bin invariant that
            // add_liquidity relies on. When supply hits 0 the final burn took
            // the whole remaining supply, so the checked_sub above already
            // drove the reserves to exactly 0 (ratio 1, no remainder) — this is
            // a no-op today, and a guard if any future path leaves dust behind.
            if bin.liquidity_supply == 0 {
                bin.amount_x = 0;
                bin.amount_y = 0;
            }

            pos.liquidity_shares[pos_slot] = pos_shares
                .checked_sub(remove)
                .ok_or(DlmmError::InsufficientLiquidity)?;

            x_out_total = x_out_total
                .checked_add(x_out)
                .ok_or(DlmmError::MathOverflow)?;
            y_out_total = y_out_total
                .checked_add(y_out)
                .ok_or(DlmmError::MathOverflow)?;
            burned = burned.checked_add(remove).ok_or(DlmmError::MathOverflow)?;
        }

        require!(burned > 0, DlmmError::InsufficientLiquidity);
        require!(
            x_out_total >= min_amount_x && y_out_total >= min_amount_y,
            DlmmError::SlippageExceeded
        );
        total_x = x_out_total;
        total_y = y_out_total;
        total_burned = burned;
    }

    // Pay the tokens out of the reserves (pair authority signs).
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

    emit!(LiquidityRemoved {
        lb_pair: lb_pair_key,
        position: ctx.accounts.position.key(),
        amount_x: total_x,
        amount_y: total_y,
        shares_burned: total_burned,
        bps,
    });

    Ok(())
}

/// Transfer `amount` out of a reserve to a user account (pair authority signs).
fn transfer_out<'info>(
    ctx: &Context<RemoveLiquidity<'info>>,
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
