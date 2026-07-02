//! Idle-reserve yield engine — the constant-product pool's differentiator.
//!
//! A full-range pool holds most of its capital far from the price, so that idle
//! capital can earn yield. On devnet there is no real lending market, so this is
//! an honest **mock**: `initialize_yield` sets a per-slot rate and a solvency
//! buffer and creates two pre-funded *yield-source* vaults; `rebalance_to_vault`
//! marks the reserve above the buffer as deployed principal; `harvest_yield`
//! pays the accrued yield out of the yield source into the reserve, raising the
//! LP share price for everyone.
//!
//! The deployed principal never physically leaves the reserve vault — "deployed"
//! is an accounting marker for what the yield accrues on — so swaps and
//! withdrawals are always solvent and need no withdraw-on-demand path.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Mint, Token, TokenAccount, Transfer};

use crate::constants::{MAX_BUFFER_BPS, POOL_AUTHORITY_SEED, RESERVE_SEED, YIELD_SOURCE_SEED};
use crate::errors::CammError;
use crate::events::{Rebalanced, YieldHarvested, YieldInitialized};
use crate::state::Pool;
use crate::yield_math::{accrued_yield, deployable};

// ---------------------------------------------------------------------------
// initialize_yield
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeYield<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    pub token_a_mint: Box<Account<'info, Mint>>,
    pub token_b_mint: Box<Account<'info, Mint>>,

    /// CHECK: PDA authority for the yield-source vaults; holds no data.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = creator,
        seeds = [YIELD_SOURCE_SEED, pool.key().as_ref(), token_a_mint.key().as_ref()],
        bump,
        token::mint = token_a_mint,
        token::authority = pool_authority,
    )]
    pub yield_source_a: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = creator,
        seeds = [YIELD_SOURCE_SEED, pool.key().as_ref(), token_b_mint.key().as_ref()],
        bump,
        token::mint = token_b_mint,
        token::authority = pool_authority,
    )]
    pub yield_source_b: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Validate the yield configuration. The rate must enable the engine (nonzero)
/// and the buffer must leave some principal deployable (strictly below 100%).
/// Pure so it can be unit-tested directly.
pub fn validate_yield_config(yield_rate: u64, buffer_bps: u16) -> Result<()> {
    require!(yield_rate > 0, CammError::InvalidYieldConfig);
    require!(buffer_bps < MAX_BUFFER_BPS, CammError::InvalidYieldConfig);
    Ok(())
}

/// Configure the mock yield engine and create its pre-funded source vaults.
/// Creator-gated. `yield_rate` is yield per deployed unit per slot scaled by
/// `YIELD_SCALE`; `buffer_bps` is the reserve fraction held back for solvency.
pub fn initialize_yield(
    ctx: Context<InitializeYield>,
    yield_rate: u64,
    buffer_bps: u16,
) -> Result<()> {
    validate_yield_config(yield_rate, buffer_bps)?;

    let now = Clock::get()?.slot;
    let pool_key = ctx.accounts.pool.key();
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require_keys_eq!(
            ctx.accounts.creator.key(),
            pool.creator,
            CammError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.token_a_mint.key(),
            pool.token_a_mint,
            CammError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.token_b_mint.key(),
            pool.token_b_mint,
            CammError::Unauthorized
        );
        pool.yield_rate = yield_rate;
        pool.buffer_bps = buffer_bps as u64;
        pool.last_accrual_slot = now;
        pool.deployed_a = 0;
        pool.deployed_b = 0;
    }

    emit!(YieldInitialized {
        pool: pool_key,
        yield_rate,
        buffer_bps: buffer_bps as u64,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// shared accrual
// ---------------------------------------------------------------------------

/// Accounts common to the two accrual instructions (rebalance + harvest).
#[derive(Accounts)]
pub struct YieldAccrue<'info> {
    /// Permissionless: anyone may trigger accrual (it only benefits LPs).
    pub caller: Signer<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA that owns the yield sources; signs the payout.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [YIELD_SOURCE_SEED, pool.key().as_ref(), token_a_mint.key().as_ref()],
        bump,
    )]
    pub yield_source_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [YIELD_SOURCE_SEED, pool.key().as_ref(), token_b_mint.key().as_ref()],
        bump,
    )]
    pub yield_source_b: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [RESERVE_SEED, pool.key().as_ref(), token_a_mint.key().as_ref()],
        bump,
    )]
    pub reserve_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [RESERVE_SEED, pool.key().as_ref(), token_b_mint.key().as_ref()],
        bump,
    )]
    pub reserve_b_vault: Box<Account<'info, TokenAccount>>,

    pub token_a_mint: Box<Account<'info, Mint>>,
    pub token_b_mint: Box<Account<'info, Mint>>,

    pub token_program: Program<'info, Token>,
}

/// Pay the yield accrued since the last update into the reserves and advance the
/// accrual clock. The payout per side is capped at the yield source's balance,
/// so an underfunded source simply pays what it has. Returns `(paid_a, paid_b)`.
fn accrue(ctx: &Context<YieldAccrue>, pool: &mut Pool, now: u64) -> Result<(u64, u64)> {
    require!(pool.yield_enabled(), CammError::YieldNotConfigured);
    // The mints must be the pool's (the seeds tie the vaults to these mints).
    require_keys_eq!(
        ctx.accounts.token_a_mint.key(),
        pool.token_a_mint,
        CammError::Unauthorized
    );
    require_keys_eq!(
        ctx.accounts.token_b_mint.key(),
        pool.token_b_mint,
        CammError::Unauthorized
    );

    let elapsed = now.saturating_sub(pool.last_accrual_slot);
    // Clamp the accrual base to the present reserve: a swap can shrink the
    // reserve below the stale `deployed` snapshot, and yield should never accrue
    // on more than the capital actually idle now. (rebalance_to_vault re-clamps
    // `deployed` itself.)
    let base_a = pool.deployed_a.min(pool.reserve_a);
    let base_b = pool.deployed_b.min(pool.reserve_b);
    let want_a =
        accrued_yield(base_a, pool.yield_rate, elapsed).map_err(|_| CammError::MathOverflow)?;
    let want_b =
        accrued_yield(base_b, pool.yield_rate, elapsed).map_err(|_| CammError::MathOverflow)?;
    // Never pay more than the source holds.
    let paid_a = want_a.min(ctx.accounts.yield_source_a.amount);
    let paid_b = want_b.min(ctx.accounts.yield_source_b.amount);

    let pool_key = ctx.accounts.pool.key();
    let signer_seeds: &[&[&[u8]]] = &[&[
        POOL_AUTHORITY_SEED,
        pool_key.as_ref(),
        &[ctx.bumps.pool_authority],
    ]];
    if paid_a > 0 {
        pay(
            ctx,
            &ctx.accounts.yield_source_a,
            &ctx.accounts.reserve_a_vault,
            paid_a,
            signer_seeds,
        )?;
        pool.reserve_a = pool
            .reserve_a
            .checked_add(paid_a)
            .ok_or(CammError::MathOverflow)?;
    }
    if paid_b > 0 {
        pay(
            ctx,
            &ctx.accounts.yield_source_b,
            &ctx.accounts.reserve_b_vault,
            paid_b,
            signer_seeds,
        )?;
        pool.reserve_b = pool
            .reserve_b
            .checked_add(paid_b)
            .ok_or(CammError::MathOverflow)?;
    }
    pool.last_accrual_slot = now;
    Ok((paid_a, paid_b))
}

/// Transfer `amount` from a yield source into a reserve (pool authority signs).
fn pay<'info>(
    ctx: &Context<YieldAccrue<'info>>,
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

// ---------------------------------------------------------------------------
// harvest_yield
// ---------------------------------------------------------------------------

/// Pay the accrued yield into the reserves without changing the deployed
/// principal.
pub fn harvest_yield(ctx: Context<YieldAccrue>) -> Result<()> {
    let now = Clock::get()?.slot;
    let pool_key = ctx.accounts.pool.key();
    let (harvested_a, harvested_b);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        let (a, b) = accrue(&ctx, &mut pool, now)?;
        harvested_a = a;
        harvested_b = b;
    }
    emit!(YieldHarvested {
        pool: pool_key,
        harvested_a,
        harvested_b,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// rebalance_to_vault
// ---------------------------------------------------------------------------

/// Harvest pending yield, then re-mark the reserve above the buffer as deployed
/// principal (so subsequent yield accrues on the current idle balance).
pub fn rebalance_to_vault(ctx: Context<YieldAccrue>) -> Result<()> {
    let now = Clock::get()?.slot;
    let pool_key = ctx.accounts.pool.key();
    let (harvested_a, harvested_b, deployed_a, deployed_b);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        // Accrue on the OLD principal first, then reset the deployed snapshot.
        let (a, b) = accrue(&ctx, &mut pool, now)?;
        harvested_a = a;
        harvested_b = b;
        let buffer = pool.buffer_bps as u16;
        pool.deployed_a =
            deployable(pool.reserve_a, buffer).map_err(|_| CammError::MathOverflow)?;
        pool.deployed_b =
            deployable(pool.reserve_b, buffer).map_err(|_| CammError::MathOverflow)?;
        deployed_a = pool.deployed_a;
        deployed_b = pool.deployed_b;
    }
    emit!(Rebalanced {
        pool: pool_key,
        deployed_a,
        deployed_b,
        harvested_a,
        harvested_b,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yield_config_validation() {
        assert!(validate_yield_config(1, 0).is_ok());
        assert!(validate_yield_config(1_000_000, 5_000).is_ok());
        assert!(validate_yield_config(1, MAX_BUFFER_BPS - 1).is_ok());
        // rate 0 leaves the engine disabled -> rejected as a config
        assert!(validate_yield_config(0, 1_000).is_err());
        // a full buffer would deploy nothing
        assert!(validate_yield_config(1, MAX_BUFFER_BPS).is_err());
    }
}
