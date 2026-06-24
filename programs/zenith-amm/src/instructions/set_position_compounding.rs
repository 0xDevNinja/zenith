//! `set_position_compounding` — toggle a position's fee-compounding mode.
//!
//! When enabled, `claim_position_fee` folds owed fees back into the position's
//! liquidity instead of paying them out. Owner-gated via the position NFT.
//! Toggling only flips the flag; it never touches fee checkpoints or balances,
//! so it cannot corrupt accrual.

use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::errors::ZenithError;
use crate::state::Position;

#[derive(Accounts)]
pub struct SetPositionCompounding<'info> {
    pub owner: Signer<'info>,

    #[account(mut)]
    pub position: Box<Account<'info, Position>>,

    /// The owner's token account holding the position NFT (proves ownership).
    #[account(
        constraint = position_nft_account.mint == position.nft_mint @ ZenithError::Unauthorized,
        constraint = position_nft_account.owner == owner.key() @ ZenithError::Unauthorized,
        constraint = position_nft_account.amount == 1 @ ZenithError::Unauthorized,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,
}

/// Enable or disable compounding for the position.
pub fn set_position_compounding(ctx: Context<SetPositionCompounding>, enabled: bool) -> Result<()> {
    ctx.accounts.position.compounding = u8::from(enabled);
    Ok(())
}
