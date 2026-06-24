//! `claim_partner_fee` — withdraw a partner/integrator's accrued fee cut.
//!
//! Each swap carves a partner share out of the protocol fee (see `swap`) and
//! parks it on `pool.partner_fee_a/b`. This handler pays those balances out of
//! the vaults to the config's `partner` and zeroes them. Partner-gated; allowed
//! on disabled pools so the partner can always collect.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::constants::{CONFIG_SEED, POOL_AUTHORITY_SEED};
use crate::errors::ZenithError;
use crate::events::PartnerFeeClaimed;
use crate::state::{Config, Pool};

#[derive(Accounts)]
pub struct ClaimPartnerFee<'info> {
    /// Must equal the config's `partner`.
    pub partner: Signer<'info>,

    /// The config the pool was created from (holds `partner`). Pinned to
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

/// Pay out and zero the pool's accrued partner fees.
pub fn claim_partner_fee(ctx: Context<ClaimPartnerFee>) -> Result<()> {
    let pool_key = ctx.accounts.pool.key();
    let authority_bump = ctx.bumps.pool_authority;

    let (amount_a, amount_b);
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require_keys_eq!(
            ctx.accounts.config.key(),
            pool.config,
            ZenithError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.partner.key(),
            ctx.accounts.config.partner,
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

        amount_a = pool.partner_fee_a;
        amount_b = pool.partner_fee_b;
        // Effects before interactions: zero the balances, then pay out.
        pool.partner_fee_a = 0;
        pool.partner_fee_b = 0;
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

    emit!(PartnerFeeClaimed {
        pool: pool_key,
        amount_a,
        amount_b,
    });
    Ok(())
}

/// Transfer `amount` out of a vault to a recipient (pool authority signs).
fn pay_out<'info>(
    ctx: &Context<ClaimPartnerFee<'info>>,
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
