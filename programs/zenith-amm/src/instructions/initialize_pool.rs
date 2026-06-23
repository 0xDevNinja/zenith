//! `initialize_pool` — create a pool from a config and open the first position.
//!
//! Atomically: validates the mints + initial price, creates the two token
//! vaults under the pool authority PDA, seeds the creator's liquidity into the
//! vaults, mints the position NFT, and records the pool + first position.
//!
//! M1 scope is the classic SPL Token program (the pool records a `SplToken`
//! flavor for each mint); Token-2022 support lands later.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{mint_to, transfer, Mint, MintTo, Token, TokenAccount, Transfer};

use crate::constants::{POOL_AUTHORITY_SEED, POOL_SEED, POSITION_SEED, VAULT_SEED};
use crate::errors::ZenithError;
use crate::events::PoolInitialized;
use crate::math::initial_liquidity_amounts;
use crate::math::validate_price_band;
use crate::state::{Config, Pool, PoolStatus, Position, TokenFlavor};

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        seeds = [crate::constants::CONFIG_SEED, &config.index.to_le_bytes()],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,

    pub token_a_mint: Box<Account<'info, Mint>>,
    pub token_b_mint: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = creator,
        space = Pool::LEN,
        seeds = [POOL_SEED, config.key().as_ref(), token_a_mint.key().as_ref(), token_b_mint.key().as_ref()],
        bump
    )]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA that owns the vaults and the NFT mint authority; no data.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = creator,
        seeds = [VAULT_SEED, pool.key().as_ref(), token_a_mint.key().as_ref()],
        bump,
        token::mint = token_a_mint,
        token::authority = pool_authority,
    )]
    pub token_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = creator,
        seeds = [VAULT_SEED, pool.key().as_ref(), token_b_mint.key().as_ref()],
        bump,
        token::mint = token_b_mint,
        token::authority = pool_authority,
    )]
    pub token_b_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::mint = token_a_mint, token::authority = creator)]
    pub creator_token_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::mint = token_b_mint, token::authority = creator)]
    pub creator_token_b: Box<Account<'info, TokenAccount>>,

    /// New mint for the position NFT (client-generated keypair, supply 1).
    #[account(
        init,
        payer = creator,
        mint::decimals = 0,
        mint::authority = pool_authority,
        mint::freeze_authority = pool_authority,
    )]
    pub position_nft_mint: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = creator,
        associated_token::mint = position_nft_mint,
        associated_token::authority = creator,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = creator,
        space = 8 + Position::INIT_SPACE,
        seeds = [POSITION_SEED, position_nft_mint.key().as_ref()],
        bump
    )]
    pub position: Box<Account<'info, Position>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Create the pool and open the creator's first position.
///
/// `sqrt_price` is the initial price (Q64.64 raw bits) and must sit strictly
/// inside the config's band. `liquidity` is the amount to seed; the required
/// token deposits are derived from it and must not exceed `token_a_max` /
/// `token_b_max` (slippage guard).
pub fn initialize_pool(
    ctx: Context<InitializePool>,
    sqrt_price: u128,
    liquidity: u128,
    token_a_max: u64,
    token_b_max: u64,
) -> Result<()> {
    let config = &ctx.accounts.config;

    // Canonical pool key requires ascending mint order (and rejects identical
    // mints). This makes the on-chain pool PDA — seeded with the mints in the
    // submitted order — identical to `pda::pool_pda`, which sorts the mints to
    // the same canonical order. The two derivations therefore always agree.
    require!(
        ctx.accounts.token_a_mint.key() < ctx.accounts.token_b_mint.key(),
        ZenithError::IdenticalMints
    );
    validate_price_band(config.sqrt_min_price, sqrt_price, config.sqrt_max_price)?;
    require!(liquidity > 0, ZenithError::InsufficientLiquidity);

    let (amount_a, amount_b) = initial_liquidity_amounts(
        liquidity,
        sqrt_price,
        config.sqrt_min_price,
        config.sqrt_max_price,
    )?;
    require!(amount_a > 0 && amount_b > 0, ZenithError::ZeroAmount);
    require!(
        amount_a <= token_a_max && amount_b <= token_b_max,
        ZenithError::SlippageExceeded
    );

    // Pull the creator's liquidity into the vaults.
    transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.creator_token_a.to_account_info(),
                to: ctx.accounts.token_a_vault.to_account_info(),
                authority: ctx.accounts.creator.to_account_info(),
            },
        ),
        amount_a,
    )?;
    transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.creator_token_b.to_account_info(),
                to: ctx.accounts.token_b_vault.to_account_info(),
                authority: ctx.accounts.creator.to_account_info(),
            },
        ),
        amount_b,
    )?;

    // Mint the single position NFT to the creator (pool authority signs).
    let pool_key = ctx.accounts.pool.key();
    let authority_seeds: &[&[u8]] = &[
        POOL_AUTHORITY_SEED,
        pool_key.as_ref(),
        &[ctx.bumps.pool_authority],
    ];
    mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.position_nft_mint.to_account_info(),
                to: ctx.accounts.position_nft_account.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            &[authority_seeds],
        ),
        1,
    )?;

    // Record the pool.
    {
        let mut pool = ctx.accounts.pool.load_init()?;
        pool.config = config.key();
        pool.token_a_mint = ctx.accounts.token_a_mint.key();
        pool.token_b_mint = ctx.accounts.token_b_mint.key();
        pool.token_a_vault = ctx.accounts.token_a_vault.key();
        pool.token_b_vault = ctx.accounts.token_b_vault.key();
        pool.liquidity = liquidity;
        pool.sqrt_price = sqrt_price;
        pool.sqrt_min_price = config.sqrt_min_price;
        pool.sqrt_max_price = config.sqrt_max_price;
        pool.fee_growth_global_a = 0;
        pool.fee_growth_global_b = 0;
        pool.reserved_u128 = [0u128; 4];
        pool.protocol_fee_a = 0;
        pool.protocol_fee_b = 0;
        pool.activation_point = Clock::get()?.slot;
        pool.position_count = 1;
        pool.reserved_u64 = [0u64; 8];
        pool.base_fee_bps = config.base_fee_bps;
        pool.status = PoolStatus::Active as u8;
        pool.pool_authority_bump = ctx.bumps.pool_authority;
        pool.pool_bump = ctx.bumps.pool;
        pool.token_a_vault_bump = ctx.bumps.token_a_vault;
        pool.token_b_vault_bump = ctx.bumps.token_b_vault;
        pool.token_a_flags = TokenFlavor::SplToken as u8;
        pool.token_b_flags = TokenFlavor::SplToken as u8;
        pool.padding = [0u8; 7];
    }

    // Open the first position (all unlocked liquidity, fee checkpoints at zero).
    // Ownership is the NFT: later handlers (claim/remove) must verify the caller
    // holds `position.nft_mint` (amount == 1) before acting on this position.
    let position = &mut ctx.accounts.position;
    position.pool = pool_key;
    position.nft_mint = ctx.accounts.position_nft_mint.key();
    position.liquidity = liquidity;
    position.vested_liquidity = 0;
    position.permanent_locked_liquidity = 0;
    position.fee_growth_checkpoint_a = 0;
    position.fee_growth_checkpoint_b = 0;
    position.fee_pending_a = 0;
    position.fee_pending_b = 0;
    position.bump = ctx.bumps.position;
    position.reserved = [0u8; 64];

    emit!(PoolInitialized {
        pool: pool_key,
        token_a_mint: ctx.accounts.token_a_mint.key(),
        token_b_mint: ctx.accounts.token_b_mint.key(),
        sqrt_price,
        sqrt_min_price: config.sqrt_min_price,
        sqrt_max_price: config.sqrt_max_price,
        liquidity,
        position_nft_mint: ctx.accounts.position_nft_mint.key(),
        amount_a,
        amount_b,
    });

    Ok(())
}
