//! `claim_position_fee` — pay out a position's accrued LP fees.
//!
//! Fees are not auto-compounded: swaps raise the pool's per-liquidity fee
//! accumulators, positions checkpoint against them, and the owner claims the
//! difference here. This is what lets locked/vested liquidity keep earning and
//! claim later. The handler settles pending fees from the live accumulator,
//! pays them out of the vaults, and zeroes the pending buckets — so a second
//! claim with no swaps in between yields nothing.
//!
//! Ownership is the position NFT: the signer must hold `position.nft_mint`
//! (amount == 1) and the position must belong to the passed pool.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::constants::POOL_AUTHORITY_SEED;
use crate::errors::ZenithError;
use crate::events::{FeesClaimed, FeesCompounded};
use crate::math::{compound_fee_into_liquidity, settle_position_fees};
use crate::state::{Pool, Position};

#[derive(Accounts)]
pub struct ClaimPositionFee<'info> {
    /// Position owner — must hold the position NFT.
    pub owner: Signer<'info>,

    /// The pool. Mutated only in compounding mode (folds fees into liquidity);
    /// otherwise read for the live fee accumulators.
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    #[account(
        mut,
        constraint = position.pool == pool.key() @ ZenithError::Unauthorized,
    )]
    pub position: Box<Account<'info, Position>>,

    /// The owner's token account holding the position NFT (proves ownership).
    #[account(
        constraint = position_nft_account.mint == position.nft_mint @ ZenithError::Unauthorized,
        constraint = position_nft_account.owner == owner.key() @ ZenithError::Unauthorized,
        constraint = position_nft_account.amount == 1 @ ZenithError::Unauthorized,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: PDA that owns the vaults; signs the payout. Seed-derived from the
    /// pool, so it can only move this pool's funds.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub token_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub token_b_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub owner_token_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub owner_token_b: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

/// Settle and pay out the position's accrued fees.
pub fn claim_position_fee(ctx: Context<ClaimPositionFee>) -> Result<()> {
    let pool_key = ctx.accounts.pool.key();
    let authority_bump = ctx.bumps.pool_authority;

    let (amount_a, amount_b);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require_keys_eq!(
            ctx.accounts.token_a_vault.key(),
            pool.token_a_vault,
            ZenithError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.token_b_vault.key(),
            pool.token_b_vault,
            ZenithError::Unauthorized
        );

        // Roll the latest accrual into the pending buckets.
        settle_position_fees(
            &mut ctx.accounts.position,
            pool.fee_growth_global_a,
            pool.fee_growth_global_b,
        )?;

        if ctx.accounts.position.compounding != 0 {
            // Compounding mode: fold owed fees into liquidity instead of paying
            // out. The fee tokens already sit in the vaults, so this is pure
            // accounting — no transfer. Leftover dust stays as pending.
            let c = compound_fee_into_liquidity(
                ctx.accounts.position.fee_pending_a,
                ctx.accounts.position.fee_pending_b,
                pool.sqrt_price,
                pool.sqrt_min_price,
                pool.sqrt_max_price,
            )?;
            if c.liquidity_delta > 0 {
                pool.liquidity = pool
                    .liquidity
                    .checked_add(c.liquidity_delta)
                    .ok_or(ZenithError::MathOverflow)?;
                let position = &mut ctx.accounts.position;
                position.liquidity = position
                    .liquidity
                    .checked_add(c.liquidity_delta)
                    .ok_or(ZenithError::MathOverflow)?;
                position.fee_pending_a -= c.used_a;
                position.fee_pending_b -= c.used_b;
            }
            emit!(FeesCompounded {
                pool: pool_key,
                position: ctx.accounts.position.key(),
                liquidity_delta: c.liquidity_delta,
                amount_a: c.used_a,
                amount_b: c.used_b,
            });
            return Ok(());
        }

        // Claim mode: take all pending and pay it out.
        amount_a = ctx.accounts.position.fee_pending_a;
        amount_b = ctx.accounts.position.fee_pending_b;
        ctx.accounts.position.fee_pending_a = 0;
        ctx.accounts.position.fee_pending_b = 0;
    }

    let signer_seeds: &[&[&[u8]]] = &[&[POOL_AUTHORITY_SEED, pool_key.as_ref(), &[authority_bump]]];
    if amount_a > 0 {
        pay_out(
            &ctx,
            &ctx.accounts.token_a_vault,
            &ctx.accounts.owner_token_a,
            amount_a,
            signer_seeds,
        )?;
    }
    if amount_b > 0 {
        pay_out(
            &ctx,
            &ctx.accounts.token_b_vault,
            &ctx.accounts.owner_token_b,
            amount_b,
            signer_seeds,
        )?;
    }

    emit!(FeesClaimed {
        pool: pool_key,
        position: ctx.accounts.position.key(),
        amount_a,
        amount_b,
    });
    Ok(())
}

/// Transfer `amount` out of a vault to the owner (pool authority signs).
fn pay_out<'info>(
    ctx: &Context<ClaimPositionFee<'info>>,
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
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )
}
