//! `remove_liquidity` — burn LP shares and withdraw the pro-rata reserves.
//!
//! Burning `shares` returns `reserve * shares / supply` of each token, floored
//! so a withdrawal never returns more than the burned shares are worth (the
//! rounding dust stays with the remaining LPs). Both returns must clear the
//! caller's `min_a`/`min_b`. Removal is allowed on a disabled pool so LPs can
//! always exit.
//!
//! Scope: classic SPL Token only.

use anchor_lang::prelude::*;
use anchor_spl::token::{burn, transfer, Burn, Mint, Token, TokenAccount, Transfer};
use zenith_math::{tokens_for_shares, Rounding};

use crate::constants::POOL_AUTHORITY_SEED;
use crate::errors::CammError;
use crate::events::LiquidityRemoved;
use crate::state::Pool;

#[derive(Accounts)]
pub struct RemoveLiquidity<'info> {
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA that owns the reserves; signs payouts. Seed-derived from the
    /// pool, so it can only move this pool's funds.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub lp_mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub reserve_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub reserve_b_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_b: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner, token::mint = lp_mint)]
    pub user_lp: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

pub fn remove_liquidity(
    ctx: Context<RemoveLiquidity>,
    shares: u64,
    min_a: u64,
    min_b: u64,
) -> Result<()> {
    require!(shares > 0, CammError::ZeroAmount);

    let pool_key = ctx.accounts.pool.key();
    let supply = ctx.accounts.lp_mint.supply;

    let (amount_a, amount_b);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require_keys_eq!(
            ctx.accounts.reserve_a_vault.key(),
            pool.reserve_a_vault,
            CammError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.reserve_b_vault.key(),
            pool.reserve_b_vault,
            CammError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.lp_mint.key(),
            pool.lp_mint,
            CammError::Unauthorized
        );

        let (ra, rb) = (pool.reserve_a, pool.reserve_b);
        let a = tokens_for_shares(shares as u128, ra as u128, supply as u128, Rounding::Down)
            .map_err(|_| CammError::MathOverflow)? as u64;
        let b = tokens_for_shares(shares as u128, rb as u128, supply as u128, Rounding::Down)
            .map_err(|_| CammError::MathOverflow)? as u64;
        // A burn that returns nothing is pointless and would forfeit shares.
        require!(a > 0 || b > 0, CammError::InsufficientLiquidity);
        require!(a >= min_a && b >= min_b, CammError::SlippageExceeded);

        pool.reserve_a = ra.checked_sub(a).ok_or(CammError::MathOverflow)?;
        pool.reserve_b = rb.checked_sub(b).ok_or(CammError::MathOverflow)?;
        amount_a = a;
        amount_b = b;
    }

    // Burn the shares (the token owner signs).
    burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.lp_mint.to_account_info(),
                from: ctx.accounts.user_lp.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        shares,
    )?;

    // Pay the reserves out (pool authority signs).
    let signer_seeds: &[&[&[u8]]] = &[&[
        POOL_AUTHORITY_SEED,
        pool_key.as_ref(),
        &[ctx.bumps.pool_authority],
    ]];
    if amount_a > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.reserve_a_vault,
            &ctx.accounts.user_token_a,
            amount_a,
            signer_seeds,
        )?;
    }
    if amount_b > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.reserve_b_vault,
            &ctx.accounts.user_token_b,
            amount_b,
            signer_seeds,
        )?;
    }

    emit!(LiquidityRemoved {
        pool: pool_key,
        owner: ctx.accounts.owner.key(),
        amount_a,
        amount_b,
        shares_burned: shares,
    });

    Ok(())
}

/// Transfer `amount` out of a reserve to a user account (pool authority signs).
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
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )
}
