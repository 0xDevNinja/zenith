//! `close_position` — reclaim the rent of an empty position.
//!
//! Only the owner may close, and only once every bin's shares are gone (after a
//! full `remove_liquidity`). The handler verifies emptiness before Anchor's
//! `close` returns the lamports, so a position that still holds shares cannot be
//! closed (the failed check reverts the whole transaction).

use anchor_lang::prelude::*;

use crate::errors::DlmmError;
use crate::events::PositionClosed;
use crate::state::Position;

#[derive(Accounts)]
pub struct ClosePosition<'info> {
    /// Position owner; receives the reclaimed rent.
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut, close = owner)]
    pub position: AccountLoader<'info, Position>,
}

/// Close an empty position and return its rent to the owner.
pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
    let (lb_pair, position_key, owner_key) = {
        let pos = ctx.accounts.position.load()?;
        require_keys_eq!(pos.owner, ctx.accounts.owner.key(), DlmmError::Unauthorized);
        // Empty == no shares in any bin. TODO(M4b): once fees accrue into
        // fee_infos, also require pending fees are 0/claimed so closing can't
        // forfeit owed fees.
        require!(pos.is_empty(), DlmmError::PositionNotEmpty);
        (pos.lb_pair, ctx.accounts.position.key(), pos.owner)
    };

    emit!(PositionClosed {
        lb_pair,
        position: position_key,
        owner: owner_key,
    });

    Ok(())
}
