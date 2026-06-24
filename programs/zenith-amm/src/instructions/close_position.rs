//! `close_position` — retire an empty position and reclaim its rent.
//!
//! Only a position with no remaining liquidity and no unclaimed fees can be
//! closed. The handler settles fees one last time (so silently-accrued fees
//! cannot be lost), requires the pending buckets to be empty, burns the
//! position NFT, and closes both the NFT token account and the `Position` PDA,
//! returning all rent to the owner.
//!
//! Ownership is the position NFT: the signer must hold `position.nft_mint`
//! (amount == 1) and the position must belong to the passed pool.

use anchor_lang::prelude::*;
use anchor_spl::token::{burn, close_account, Burn, CloseAccount, Mint, Token, TokenAccount};

use crate::errors::ZenithError;
use crate::events::PositionClosed;
use crate::math::settle_position_fees;
use crate::state::{Pool, Position};

#[derive(Accounts)]
pub struct ClosePosition<'info> {
    /// Position owner — receives all reclaimed rent.
    #[account(mut)]
    pub owner: Signer<'info>,

    /// The pool. Mutated to decrement `position_count`.
    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// The position to close; its rent is returned to `owner`.
    #[account(
        mut,
        close = owner,
        constraint = position.pool == pool.key() @ ZenithError::Unauthorized,
    )]
    pub position: Box<Account<'info, Position>>,

    /// The position NFT mint (burned to zero supply).
    #[account(mut, address = position.nft_mint @ ZenithError::Unauthorized)]
    pub position_nft_mint: Box<Account<'info, Mint>>,

    /// The owner's NFT token account (burned, then closed for its rent).
    #[account(
        mut,
        constraint = position_nft_account.mint == position.nft_mint @ ZenithError::Unauthorized,
        constraint = position_nft_account.owner == owner.key() @ ZenithError::Unauthorized,
        constraint = position_nft_account.amount == 1 @ ZenithError::Unauthorized,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

/// Close an empty position.
pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
    // Settle any last accrual so it cannot be silently lost, then require the
    // position to be fully empty (no liquidity, no unclaimed fees).
    {
        let pool = ctx.accounts.pool.load()?;
        settle_position_fees(
            &mut ctx.accounts.position,
            pool.fee_growth_global_a,
            pool.fee_growth_global_b,
        )?;
    }
    let position = &ctx.accounts.position;
    require!(
        position.total_liquidity() == 0
            && position.fee_pending_a == 0
            && position.fee_pending_b == 0,
        ZenithError::PositionNotEmpty
    );

    // Burn the NFT (owner signs) and close its token account for the rent.
    burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.position_nft_mint.to_account_info(),
                from: ctx.accounts.position_nft_account.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        1,
    )?;
    close_account(CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        CloseAccount {
            account: ctx.accounts.position_nft_account.to_account_info(),
            destination: ctx.accounts.owner.to_account_info(),
            authority: ctx.accounts.owner.to_account_info(),
        },
    ))?;

    // Decrement the pool's open-position counter.
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        pool.position_count = pool.position_count.saturating_sub(1);
    }

    emit!(PositionClosed {
        pool: ctx.accounts.pool.key(),
        position: ctx.accounts.position.key(),
        position_nft_mint: ctx.accounts.position_nft_mint.key(),
    });
    Ok(())
}
