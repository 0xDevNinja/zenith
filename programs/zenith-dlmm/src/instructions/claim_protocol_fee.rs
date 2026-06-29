//! `claim_protocol_fee` — pay out a pair's accrued protocol fees.
//!
//! Only the pair's authority (its `creator`) may claim. The accrued
//! `protocol_fee_x/y` are zeroed and transferred out of the reserves under the
//! pair-authority PDA. Effects (zeroing) happen before the CPI payout, so a
//! failed transfer reverts the whole transaction.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::constants::PAIR_AUTHORITY_SEED;
use crate::errors::DlmmError;
use crate::events::ProtocolFeeClaimed;
use crate::state::LbPair;

#[derive(Accounts)]
pub struct ClaimProtocolFee<'info> {
    /// Must be the pair's authority (`creator`).
    pub authority: Signer<'info>,

    #[account(mut)]
    pub lb_pair: AccountLoader<'info, LbPair>,

    /// CHECK: PDA that owns the reserves; signs the payout.
    #[account(seeds = [PAIR_AUTHORITY_SEED, lb_pair.key().as_ref()], bump)]
    pub pair_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub reserve_x: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub reserve_y: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub recipient_token_x: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub recipient_token_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

/// Claim the pair's accrued protocol fees to the recipient token accounts.
pub fn claim_protocol_fee(ctx: Context<ClaimProtocolFee>) -> Result<()> {
    let lb_pair_key = ctx.accounts.lb_pair.key();

    let (amount_x, amount_y) = {
        let mut pair = ctx.accounts.lb_pair.load_mut()?;
        require_keys_eq!(
            pair.creator,
            ctx.accounts.authority.key(),
            DlmmError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.reserve_x.key(),
            pair.reserve_x,
            DlmmError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.reserve_y.key(),
            pair.reserve_y,
            DlmmError::Unauthorized
        );
        let (x, y) = (pair.protocol_fee_x, pair.protocol_fee_y);
        // Effects before interactions: zero the accrued fees, then pay out.
        pair.protocol_fee_x = 0;
        pair.protocol_fee_y = 0;
        (x, y)
    };

    let signer_seeds: &[&[&[u8]]] = &[&[
        PAIR_AUTHORITY_SEED,
        lb_pair_key.as_ref(),
        &[ctx.bumps.pair_authority],
    ]];
    if amount_x > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.reserve_x,
            &ctx.accounts.recipient_token_x,
            amount_x,
            signer_seeds,
        )?;
    }
    if amount_y > 0 {
        transfer_out(
            &ctx,
            &ctx.accounts.reserve_y,
            &ctx.accounts.recipient_token_y,
            amount_y,
            signer_seeds,
        )?;
    }

    emit!(ProtocolFeeClaimed {
        lb_pair: lb_pair_key,
        amount_x,
        amount_y,
    });

    Ok(())
}

fn transfer_out<'info>(
    ctx: &Context<ClaimProtocolFee<'info>>,
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
                authority: ctx.accounts.pair_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )
}
