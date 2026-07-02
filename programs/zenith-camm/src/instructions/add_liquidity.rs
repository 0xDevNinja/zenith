//! `add_liquidity` — deposit both tokens and mint fungible LP shares.
//!
//! The caller passes desired amounts of each token plus per-token minimums. On
//! the **first** deposit the pool bootstraps: LP shares are the geometric mean
//! `sqrt(a*b)` of the deposit, of which [`MINIMUM_LIQUIDITY`] is permanently
//! locked (minted to the pool-owned `locked_lp` account) to defuse the
//! share-inflation donation attack, and the rest go to the depositor.
//!
//! On a **subsequent** deposit the amounts are trimmed to the current pool ratio
//! (router logic): the token that would over-deposit is reduced so the pair goes
//! in balanced, and shares are minted proportionally. The trimmed amounts must
//! clear the caller's `min_a`/`min_b` (price-movement slippage) and the minted
//! shares must clear `min_shares`.
//!
//! Scope: classic SPL Token only.

use anchor_lang::prelude::*;
use anchor_spl::token::{mint_to, transfer, Mint, MintTo, Token, TokenAccount, Transfer};
use zenith_math::{initial_shares, matching_amount, shares_from_deposit, MINIMUM_LIQUIDITY};

use crate::constants::POOL_AUTHORITY_SEED;
use crate::errors::CammError;
use crate::events::LiquidityAdded;
use crate::state::Pool;
use crate::yield_math::accrued_yield;

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA, mint authority for the LP mint. Seed-derived from the pool.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub lp_mint: Box<Account<'info, Mint>>,

    /// Locked-liquidity sink for the minimum-liquidity floor (first deposit).
    #[account(mut)]
    pub locked_lp: Box<Account<'info, TokenAccount>>,

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

pub fn add_liquidity(
    ctx: Context<AddLiquidity>,
    desired_a: u64,
    desired_b: u64,
    min_a: u64,
    min_b: u64,
    min_shares: u64,
) -> Result<()> {
    require!(desired_a > 0 && desired_b > 0, CammError::ZeroAmount);

    let pool_key = ctx.accounts.pool.key();
    let supply = ctx.accounts.lp_mint.supply;

    // Amounts actually deposited, shares minted to the depositor, and shares
    // locked (only nonzero on the first deposit).
    let (amount_a, amount_b, shares_out, locked): (u64, u64, u64, u64);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require!(pool.is_active(), CammError::PoolNotActive);
        // The reserves and LP mint must be this pool's.
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
        require_keys_eq!(
            ctx.accounts.locked_lp.key(),
            pool.locked_lp,
            CammError::Unauthorized
        );

        if supply == 0 {
            // Bootstrap: shares = geometric mean of the deposit. Lock the
            // minimum-liquidity floor and give the depositor the remainder.
            let total = initial_shares(desired_a as u128, desired_b as u128)
                .map_err(|_| CammError::MathOverflow)?;
            require!(total > MINIMUM_LIQUIDITY, CammError::BelowMinimumLiquidity);
            let out = total - MINIMUM_LIQUIDITY;
            let out: u64 = out.try_into().map_err(|_| CammError::MathOverflow)?;
            require!(out >= min_shares, CammError::SlippageExceeded);
            amount_a = desired_a;
            amount_b = desired_b;
            shares_out = out;
            locked = MINIMUM_LIQUIDITY as u64;
        } else {
            // Price in yield accrued but not yet harvested, so a deposit made
            // just before a harvest cannot mint shares cheaply and capture the
            // pending lump (the yield-JIT attack). The effective reserve is the
            // physical reserve plus pending yield on the currently-deployed
            // principal (clamped to the reserve, matching the accrual base in
            // harvest). Deposited amounts and tracked reserves stay physical;
            // only the ratio and share-count math see the effective reserve.
            let now = Clock::get()?.slot;
            let elapsed = now.saturating_sub(pool.last_accrual_slot);
            let pending_a = accrued_yield(
                pool.deployed_a.min(pool.reserve_a),
                pool.yield_rate,
                elapsed,
            )
            .map_err(|_| CammError::MathOverflow)?;
            let pending_b = accrued_yield(
                pool.deployed_b.min(pool.reserve_b),
                pool.yield_rate,
                elapsed,
            )
            .map_err(|_| CammError::MathOverflow)?;
            let ra = pool
                .reserve_a
                .checked_add(pending_a)
                .ok_or(CammError::MathOverflow)?;
            let rb = pool
                .reserve_b
                .checked_add(pending_b)
                .ok_or(CammError::MathOverflow)?;

            // Trim to the current ratio so the pair deposits balanced. Try to use
            // all of A and compute the matching B; if that needs more B than
            // desired, pin B instead and compute the matching A.
            let b_opt = matching_amount(desired_a as u128, ra as u128, rb as u128)
                .map_err(|_| CammError::MathOverflow)?;
            let (a_used, b_used) = if b_opt <= desired_b as u128 {
                let b_used: u64 = b_opt.try_into().map_err(|_| CammError::MathOverflow)?;
                require!(b_used >= min_b, CammError::SlippageExceeded);
                (desired_a, b_used)
            } else {
                let a_opt = matching_amount(desired_b as u128, rb as u128, ra as u128)
                    .map_err(|_| CammError::MathOverflow)?;
                let a_used: u64 = a_opt.try_into().map_err(|_| CammError::MathOverflow)?;
                // a_opt corresponds to desired_b, which needed less A than desired.
                require!(a_used <= desired_a, CammError::SlippageExceeded);
                require!(a_used >= min_a, CammError::SlippageExceeded);
                (a_used, desired_b)
            };

            let shares = shares_from_deposit(
                a_used as u128,
                b_used as u128,
                ra as u128,
                rb as u128,
                supply as u128,
            )
            .map_err(|_| CammError::MathOverflow)?;
            let shares: u64 = shares.try_into().map_err(|_| CammError::MathOverflow)?;
            // A deposit too small to mint a share would silently donate tokens.
            require!(shares > 0, CammError::InsufficientLiquidity);
            require!(shares >= min_shares, CammError::SlippageExceeded);
            amount_a = a_used;
            amount_b = b_used;
            shares_out = shares;
            locked = 0;
        }

        // Track the new curve reserves.
        pool.reserve_a = pool
            .reserve_a
            .checked_add(amount_a)
            .ok_or(CammError::MathOverflow)?;
        pool.reserve_b = pool
            .reserve_b
            .checked_add(amount_b)
            .ok_or(CammError::MathOverflow)?;
    }

    // Pull the deposited tokens into the reserves (owner signs).
    transfer_in(
        &ctx,
        &ctx.accounts.user_token_a,
        &ctx.accounts.reserve_a_vault,
        amount_a,
    )?;
    transfer_in(
        &ctx,
        &ctx.accounts.user_token_b,
        &ctx.accounts.reserve_b_vault,
        amount_b,
    )?;

    // Mint LP shares (pool authority signs). Lock the floor first, then pay out.
    let signer_seeds: &[&[&[u8]]] = &[&[
        POOL_AUTHORITY_SEED,
        pool_key.as_ref(),
        &[ctx.bumps.pool_authority],
    ]];
    if locked > 0 {
        mint_lp(&ctx, &ctx.accounts.locked_lp, locked, signer_seeds)?;
    }
    mint_lp(&ctx, &ctx.accounts.user_lp, shares_out, signer_seeds)?;

    emit!(LiquidityAdded {
        pool: pool_key,
        owner: ctx.accounts.owner.key(),
        amount_a,
        amount_b,
        shares_minted: shares_out,
    });

    Ok(())
}

/// Transfer `amount` from a user account into a reserve (the owner signs).
fn transfer_in<'info>(
    ctx: &Context<AddLiquidity<'info>>,
    from: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    amount: u64,
) -> Result<()> {
    transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: from.to_account_info(),
                to: to.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        amount,
    )
}

/// Mint `amount` LP shares to `to` (the pool authority PDA signs).
fn mint_lp<'info>(
    ctx: &Context<AddLiquidity<'info>>,
    to: &Account<'info, TokenAccount>,
    amount: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: to.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )
}
