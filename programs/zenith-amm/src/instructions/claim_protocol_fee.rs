//! `claim_protocol_fee` — withdraw the protocol's accrued fee share.
//!
//! Each swap splits its fee into an LP share (into the per-liquidity
//! accumulator) and a protocol share (parked on `pool.protocol_fee_a/b`). This
//! handler pays the parked balances out of the vaults to the config's
//! `fee_authority` and zeroes them. Authority-gated; allowed on disabled pools
//! so the protocol can always collect.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::constants::{CONFIG_SEED, POOL_AUTHORITY_SEED};
use crate::errors::ZenithError;
use crate::events::ProtocolFeeClaimed;
use crate::state::{Config, Pool};

#[derive(Accounts)]
pub struct ClaimProtocolFee<'info> {
    /// Must equal the config's `fee_authority`.
    pub fee_authority: Signer<'info>,

    /// The config the pool was created from (holds `fee_authority`). Pinned to
    /// `pool.config` in the handler.
    #[account(
        seeds = [CONFIG_SEED, &config.index.to_le_bytes()],
        bump = config.bump,
    )]
    pub config: Account<'info, Config>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA that owns the vaults; signs the payout. Seed-derived from the
    /// pool, so it can only move this pool's funds.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub token_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub token_b_vault: Box<Account<'info, TokenAccount>>,

    /// Destinations for the claimed fees (the token program enforces that each
    /// shares its vault's mint on transfer).
    #[account(mut)]
    pub recipient_token_a: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub recipient_token_b: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

/// Pay out and zero the pool's accrued protocol fees.
pub fn claim_protocol_fee(ctx: Context<ClaimProtocolFee>) -> Result<()> {
    let pool_key = ctx.accounts.pool.key();
    let authority_bump = ctx.bumps.pool_authority;

    let (amount_a, amount_b);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        // The config must be the pool's own, and the signer its fee authority.
        require_keys_eq!(
            ctx.accounts.config.key(),
            pool.config,
            ZenithError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.fee_authority.key(),
            ctx.accounts.config.fee_authority,
            ZenithError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.token_a_vault.key(),
            pool.token_a_vault,
            ZenithError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.token_b_vault.key(),
            pool.token_b_vault,
            ZenithError::Unauthorized
        );

        amount_a = pool.protocol_fee_a;
        amount_b = pool.protocol_fee_b;
        // Effects before interactions: zero the balances, then pay out.
        pool.protocol_fee_a = 0;
        pool.protocol_fee_b = 0;
    }

    let signer_seeds: &[&[&[u8]]] = &[&[POOL_AUTHORITY_SEED, pool_key.as_ref(), &[authority_bump]]];
    if amount_a > 0 {
        pay_out(
            &ctx,
            &ctx.accounts.token_a_vault,
            &ctx.accounts.recipient_token_a,
            amount_a,
            signer_seeds,
        )?;
    }
    if amount_b > 0 {
        pay_out(
            &ctx,
            &ctx.accounts.token_b_vault,
            &ctx.accounts.recipient_token_b,
            amount_b,
            signer_seeds,
        )?;
    }

    emit!(ProtocolFeeClaimed {
        pool: pool_key,
        amount_a,
        amount_b,
    });
    Ok(())
}

/// Transfer `amount` out of a vault to a recipient (pool authority signs).
fn pay_out<'info>(
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
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )
}
