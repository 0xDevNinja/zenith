//! `initialize_pool` — create an empty full-range constant-product pool.
//!
//! Validates the mints and fee config, then creates the pool account, its two
//! reserve vaults, the fungible LP-share mint, and the locked-liquidity token
//! account — all owned by the pool authority PDA. The pool opens with no
//! liquidity; the first provider seeds it via `add_liquidity`, which mints the
//! initial LP shares and locks the minimum-liquidity floor.
//!
//! Scope: classic SPL Token (each mint records a `SplToken` flavor);
//! Token-2022 support lands later.

use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::constants::{
    LOCKED_LP_SEED, LP_MINT_DECIMALS, LP_MINT_SEED, MAX_FEE_BPS, POOL_AUTHORITY_SEED, POOL_SEED,
    RESERVE_SEED,
};
use crate::errors::CammError;
use crate::events::PoolInitialized;
use crate::state::{Pool, PoolStatus, TokenFlavor};

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    pub token_a_mint: Box<Account<'info, Mint>>,
    pub token_b_mint: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = creator,
        space = Pool::LEN,
        seeds = [POOL_SEED, token_a_mint.key().as_ref(), token_b_mint.key().as_ref()],
        bump
    )]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA that owns the reserves and the LP mint; holds no data.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = creator,
        seeds = [RESERVE_SEED, pool.key().as_ref(), token_a_mint.key().as_ref()],
        bump,
        token::mint = token_a_mint,
        token::authority = pool_authority,
    )]
    pub reserve_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = creator,
        seeds = [RESERVE_SEED, pool.key().as_ref(), token_b_mint.key().as_ref()],
        bump,
        token::mint = token_b_mint,
        token::authority = pool_authority,
    )]
    pub reserve_b_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = creator,
        seeds = [LP_MINT_SEED, pool.key().as_ref()],
        bump,
        mint::decimals = LP_MINT_DECIMALS,
        mint::authority = pool_authority,
    )]
    pub lp_mint: Box<Account<'info, Mint>>,

    /// Permanently holds the locked minimum liquidity minted on first deposit.
    #[account(
        init,
        payer = creator,
        seeds = [LOCKED_LP_SEED, pool.key().as_ref()],
        bump,
        token::mint = lp_mint,
        token::authority = pool_authority,
    )]
    pub locked_lp: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Validate the fee configuration a pool is created with. Both the base fee and
/// the protocol's share of it must be in range (the base fee strictly below
/// 100%; the protocol may take up to 100% of the fee).
pub fn validate_fee_config(base_fee_bps: u16, protocol_fee_rate: u16) -> Result<()> {
    require!(base_fee_bps < MAX_FEE_BPS, CammError::InvalidFeeConfig);
    require!(
        protocol_fee_rate <= MAX_FEE_BPS,
        CammError::InvalidFeeConfig
    );
    Ok(())
}

pub fn initialize_pool(
    ctx: Context<InitializePool>,
    base_fee_bps: u16,
    protocol_fee_rate: u16,
) -> Result<()> {
    // Canonical pool key requires ascending mint order (and rejects identical
    // mints), so the on-chain PDA — seeded with the mints in submitted order —
    // matches `pda::pool_pda`, which sorts them to the same order.
    require!(
        ctx.accounts.token_a_mint.key() < ctx.accounts.token_b_mint.key(),
        CammError::IdenticalMints
    );
    validate_fee_config(base_fee_bps, protocol_fee_rate)?;

    let now = Clock::get()?.slot;
    let pool_key = ctx.accounts.pool.key();
    let lp_mint_key = ctx.accounts.lp_mint.key();
    {
        let mut pool = ctx.accounts.pool.load_init()?;
        pool.reserved_u128 = [0u128; 4];
        pool.token_a_mint = ctx.accounts.token_a_mint.key();
        pool.token_b_mint = ctx.accounts.token_b_mint.key();
        pool.reserve_a_vault = ctx.accounts.reserve_a_vault.key();
        pool.reserve_b_vault = ctx.accounts.reserve_b_vault.key();
        pool.lp_mint = lp_mint_key;
        pool.locked_lp = ctx.accounts.locked_lp.key();
        pool.creator = ctx.accounts.creator.key();
        pool.reserve_a = 0;
        pool.reserve_b = 0;
        pool.protocol_fee_a = 0;
        pool.protocol_fee_b = 0;
        pool.activation_point = now;
        pool.reserved_u64 = [0u64; 6];
        pool.base_fee_bps = base_fee_bps;
        pool.protocol_fee_rate = protocol_fee_rate;
        pool.status = PoolStatus::Active as u8;
        pool.pool_authority_bump = ctx.bumps.pool_authority;
        pool.reserve_a_bump = ctx.bumps.reserve_a_vault;
        pool.reserve_b_bump = ctx.bumps.reserve_b_vault;
        pool.lp_mint_bump = ctx.bumps.lp_mint;
        pool.locked_lp_bump = ctx.bumps.locked_lp;
        pool.token_a_flag = TokenFlavor::SplToken as u8;
        pool.token_b_flag = TokenFlavor::SplToken as u8;
        pool.padding = [0u8; 12];
    }

    emit!(PoolInitialized {
        pool: pool_key,
        token_a_mint: ctx.accounts.token_a_mint.key(),
        token_b_mint: ctx.accounts.token_b_mint.key(),
        lp_mint: lp_mint_key,
        base_fee_bps,
        protocol_fee_rate,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_config_validation() {
        assert!(validate_fee_config(30, 2000).is_ok());
        assert!(validate_fee_config(0, 0).is_ok());
        assert!(validate_fee_config(MAX_FEE_BPS - 1, MAX_FEE_BPS).is_ok());
        // base fee must be strictly below 100%
        assert!(validate_fee_config(MAX_FEE_BPS, 0).is_err());
        // protocol share cannot exceed 100%
        assert!(validate_fee_config(30, MAX_FEE_BPS + 1).is_err());
    }
}
