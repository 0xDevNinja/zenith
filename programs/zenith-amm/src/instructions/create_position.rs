//! `create_position` — open an empty liquidity position in an existing pool.
//!
//! Mints a fresh position NFT (supply 1, 0 decimals) to the creator and records
//! a `Position` PDA with zero liquidity and zeroed fee checkpoints. Liquidity is
//! added later via `add_liquidity`. Ownership of the position is the NFT: any
//! later handler that mutates this position must verify the caller holds
//! `position.nft_mint` (amount == 1), matching the model used by the first
//! position opened in `initialize_pool`.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{mint_to, Mint, MintTo, Token, TokenAccount};

use crate::constants::{POOL_AUTHORITY_SEED, POSITION_SEED};
use crate::errors::ZenithError;
use crate::events::PositionCreated;
use crate::state::{Pool, Position};

#[derive(Accounts)]
pub struct CreatePosition<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    /// Pool the position belongs to. Mutated to bump `position_count`.
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA that holds the NFT mint authority; no data. Seed-derived from
    /// the pool, so it can only sign for this pool's NFTs.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

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

    /// One position PDA per NFT mint (enforced by the seed).
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

/// Open an empty position in `pool` and mint its ownership NFT to the creator.
pub fn create_position(ctx: Context<CreatePosition>) -> Result<()> {
    let pool_key = ctx.accounts.pool.key();

    // Bump the pool's open-position counter (informational; overflow is
    // unreachable in practice but checked rather than wrapped).
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.position_count = pool
            .position_count
            .checked_add(1)
            .ok_or(ZenithError::MathOverflow)?;
    }

    // Mint the single NFT to the creator (pool authority signs).
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

    // Record the empty position.
    let position = &mut ctx.accounts.position;
    position.pool = pool_key;
    position.nft_mint = ctx.accounts.position_nft_mint.key();
    position.liquidity = 0;
    position.vested_liquidity = 0;
    position.permanent_locked_liquidity = 0;
    position.fee_growth_checkpoint_a = 0;
    position.fee_growth_checkpoint_b = 0;
    position.fee_pending_a = 0;
    position.fee_pending_b = 0;
    position.bump = ctx.bumps.position;
    position.reserved = [0u8; 64];

    emit!(PositionCreated {
        pool: pool_key,
        position: position.key(),
        position_nft_mint: ctx.accounts.position_nft_mint.key(),
        owner: ctx.accounts.creator.key(),
    });

    Ok(())
}
