//! `add_liquidity` / `remove_liquidity` / `remove_all_liquidity`.
//!
//! Liquidity is supplied as a `liquidity_delta` (`L`); the token amounts are
//! derived from `L` and the current price. Deposits round **up** and are capped
//! by caller `*_max` thresholds; withdrawals round **down** and are floored by
//! caller `*_min` thresholds, so rounding never lands in the user's favor.
//!
//! Ownership is the position NFT: the signer must hold `position.nft_mint`
//! (amount == 1) and the position must belong to the passed pool. Pending fees
//! are settled into the position before any liquidity change so they are
//! attributed to the liquidity that earned them. Only `unlocked` liquidity is
//! removable in M1 (vested/locked withdrawal is M1b).

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::constants::POOL_AUTHORITY_SEED;
use crate::errors::ZenithError;
use crate::events::{LiquidityAdded, LiquidityRemoved};
use crate::math::{liquidity_amounts, settle_position_fees};
use crate::state::{Pool, Position};
use zenith_math::Rounding;

#[derive(Accounts)]
pub struct ModifyLiquidity<'info> {
    /// Position owner — must hold the position NFT.
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// The position being modified. Must belong to `pool`.
    #[account(
        mut,
        constraint = position.pool == pool.key() @ ZenithError::Unauthorized,
    )]
    pub position: Box<Account<'info, Position>>,

    /// The owner's token account holding the position NFT. Holding exactly one
    /// unit of `position.nft_mint` is what proves ownership of the position.
    #[account(
        constraint = position_nft_account.mint == position.nft_mint @ ZenithError::Unauthorized,
        constraint = position_nft_account.owner == owner.key() @ ZenithError::Unauthorized,
        constraint = position_nft_account.amount == 1 @ ZenithError::Unauthorized,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: PDA that owns the vaults; signs withdrawals. Seed-derived from the
    /// pool, so it can only move this pool's funds.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub token_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub token_b_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_b: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

impl<'info> ModifyLiquidity<'info> {
    /// Confirm the passed vaults are the ones recorded on the pool. The token
    /// program separately enforces that each user account shares its vault's
    /// mint on transfer, so verifying the vaults pins both sides.
    fn verify_vaults(&self, pool: &Pool) -> Result<()> {
        require_keys_eq!(
            self.token_a_vault.key(),
            pool.token_a_vault,
            ZenithError::Unauthorized
        );
        require_keys_eq!(
            self.token_b_vault.key(),
            pool.token_b_vault,
            ZenithError::Unauthorized
        );
        Ok(())
    }
}

/// Add `liquidity_delta` to the position, depositing the derived token amounts
/// (rounded up). Reverts if either deposit exceeds its `*_max` ceiling.
pub fn add_liquidity(
    ctx: Context<ModifyLiquidity>,
    liquidity_delta: u128,
    token_a_max: u64,
    token_b_max: u64,
) -> Result<()> {
    require!(liquidity_delta > 0, ZenithError::ZeroAmount);

    let (amount_a, amount_b, position_liquidity, pool_liquidity);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require!(pool.is_active(), ZenithError::PoolNotActive);
        ctx.accounts.verify_vaults(&pool)?;

        // Settle earned fees before liquidity changes.
        settle_position_fees(
            &mut ctx.accounts.position,
            pool.fee_growth_global_a,
            pool.fee_growth_global_b,
        )?;

        let (a, b) = liquidity_amounts(
            liquidity_delta,
            pool.sqrt_price,
            pool.sqrt_min_price,
            pool.sqrt_max_price,
            Rounding::Up,
        )?;
        require!(a > 0 && b > 0, ZenithError::ZeroAmount);
        require!(
            a <= token_a_max && b <= token_b_max,
            ZenithError::SlippageExceeded
        );

        pool.liquidity = pool
            .liquidity
            .checked_add(liquidity_delta)
            .ok_or(ZenithError::MathOverflow)?;
        ctx.accounts.position.liquidity = ctx
            .accounts
            .position
            .liquidity
            .checked_add(liquidity_delta)
            .ok_or(ZenithError::MathOverflow)?;

        amount_a = a;
        amount_b = b;
        position_liquidity = ctx.accounts.position.liquidity;
        pool_liquidity = pool.liquidity;
    }

    // Pull the tokens into the vaults (owner signs).
    transfer_in(
        &ctx,
        &ctx.accounts.user_token_a,
        &ctx.accounts.token_a_vault,
        amount_a,
    )?;
    transfer_in(
        &ctx,
        &ctx.accounts.user_token_b,
        &ctx.accounts.token_b_vault,
        amount_b,
    )?;

    emit!(LiquidityAdded {
        pool: ctx.accounts.pool.key(),
        position: ctx.accounts.position.key(),
        liquidity_delta,
        amount_a,
        amount_b,
        position_liquidity,
        pool_liquidity,
    });
    Ok(())
}

/// Remove `liquidity_delta` from the position, returning the derived token
/// amounts (rounded down). Reverts if either return is below its `*_min` floor.
pub fn remove_liquidity(
    ctx: Context<ModifyLiquidity>,
    liquidity_delta: u128,
    token_a_min: u64,
    token_b_min: u64,
) -> Result<()> {
    remove_core(ctx, liquidity_delta, token_a_min, token_b_min)
}

/// Remove all of the position's `unlocked` liquidity (floored by `*_min`).
pub fn remove_all_liquidity(
    ctx: Context<ModifyLiquidity>,
    token_a_min: u64,
    token_b_min: u64,
) -> Result<()> {
    let liquidity_delta = ctx.accounts.position.liquidity;
    require!(liquidity_delta > 0, ZenithError::ZeroAmount);
    remove_core(ctx, liquidity_delta, token_a_min, token_b_min)
}

fn remove_core(
    ctx: Context<ModifyLiquidity>,
    liquidity_delta: u128,
    token_a_min: u64,
    token_b_min: u64,
) -> Result<()> {
    require!(liquidity_delta > 0, ZenithError::ZeroAmount);
    // Only unlocked liquidity is removable in M1.
    require!(
        liquidity_delta <= ctx.accounts.position.liquidity,
        ZenithError::InsufficientLiquidity
    );

    let pool_key = ctx.accounts.pool.key();
    let authority_bump = ctx.bumps.pool_authority;

    let (amount_a, amount_b, position_liquidity, pool_liquidity);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        // Removal is allowed even on a disabled pool, so users can always exit.
        ctx.accounts.verify_vaults(&pool)?;

        settle_position_fees(
            &mut ctx.accounts.position,
            pool.fee_growth_global_a,
            pool.fee_growth_global_b,
        )?;

        let (a, b) = liquidity_amounts(
            liquidity_delta,
            pool.sqrt_price,
            pool.sqrt_min_price,
            pool.sqrt_max_price,
            Rounding::Down,
        )?;
        require!(
            a >= token_a_min && b >= token_b_min,
            ZenithError::SlippageExceeded
        );

        // Effects before interactions: shrink liquidity, then pay out.
        pool.liquidity = pool
            .liquidity
            .checked_sub(liquidity_delta)
            .ok_or(ZenithError::InsufficientLiquidity)?;
        ctx.accounts.position.liquidity = ctx
            .accounts
            .position
            .liquidity
            .checked_sub(liquidity_delta)
            .ok_or(ZenithError::InsufficientLiquidity)?;

        amount_a = a;
        amount_b = b;
        position_liquidity = ctx.accounts.position.liquidity;
        pool_liquidity = pool.liquidity;
    }

    // Pay the tokens out of the vaults (pool authority signs).
    let signer_seeds: &[&[&[u8]]] = &[&[POOL_AUTHORITY_SEED, pool_key.as_ref(), &[authority_bump]]];
    if amount_a > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.token_a_vault,
            &ctx.accounts.user_token_a,
            amount_a,
            signer_seeds,
        )?;
    }
    if amount_b > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.token_b_vault,
            &ctx.accounts.user_token_b,
            amount_b,
            signer_seeds,
        )?;
    }

    emit!(LiquidityRemoved {
        pool: pool_key,
        position: ctx.accounts.position.key(),
        liquidity_delta,
        amount_a,
        amount_b,
        position_liquidity,
        pool_liquidity,
    });
    Ok(())
}

/// Transfer `amount` from a user account into a vault (the owner signs).
fn transfer_in<'info>(
    ctx: &Context<ModifyLiquidity<'info>>,
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

/// Transfer `amount` out of a vault to a user account (the pool authority signs).
fn transfer_out<'info>(
    ctx: &Context<ModifyLiquidity<'info>>,
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
