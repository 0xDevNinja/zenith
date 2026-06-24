//! `swap` — the core trade over the pool's single liquidity band.
//!
//! Three modes (see [`SwapMode`]): ExactIn and ExactOut revert if the fill
//! would push the price out of `[sqrt_min, sqrt_max]`; PartialFill instead fills
//! to the boundary and returns the unspent input. Outputs round down and
//! required inputs round up, so rounding never favors the trader, and the price
//! can never leave the band. The base fee is taken from the input: the LP share
//! raises the per-liquidity fee accumulator and the protocol share is parked on
//! the pool for later claim.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::constants::{CONFIG_SEED, POOL_AUTHORITY_SEED};
use crate::errors::ZenithError;
use crate::events::Swap as SwapEvent;
use crate::math::{
    compute_dynamic_fee, compute_swap_step, fee_growth_delta, scheduled_base_fee_bps, split_fee,
    SwapDirection, SwapMode, BPS_DENOMINATOR,
};
use crate::state::{Config, Pool};

#[derive(Accounts)]
pub struct Swap<'info> {
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// The config the pool was created from (supplies the protocol fee share).
    /// Pinned to `pool.config` in the handler.
    #[account(
        seeds = [CONFIG_SEED, &config.index.to_le_bytes()],
        bump = config.bump,
    )]
    pub config: Account<'info, Config>,

    /// CHECK: PDA that owns the vaults; signs the payout. Seed-derived from the
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

/// Execute a swap.
///
/// `amount` is the exact input (ExactIn / PartialFill) or the exact desired
/// output (ExactOut). `other_amount_threshold` is the slippage guard: a minimum
/// output for ExactIn / PartialFill, or a maximum input for ExactOut.
pub fn swap(
    ctx: Context<Swap>,
    direction: SwapDirection,
    mode: SwapMode,
    amount: u64,
    other_amount_threshold: u64,
) -> Result<()> {
    let pool_key = ctx.accounts.pool.key();
    let authority_bump = ctx.bumps.pool_authority;
    let a_to_b = direction == SwapDirection::AToB;

    let amount_in;
    let amount_out;
    let fee;
    let protocol_fee;
    let amount_remaining;
    let next_sqrt_price;
    let total_fee_bps_out;
    let volatility_out;
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require!(pool.is_active(), ZenithError::PoolNotActive);
        require_keys_eq!(
            ctx.accounts.config.key(),
            pool.config,
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

        let now = Clock::get()?.slot;
        let config = &ctx.accounts.config;

        // Base fee from the scheduler (slots since pool activation).
        let base_fee_bps = scheduled_base_fee_bps(
            config.fee_scheduler_mode,
            config.base_fee_bps,
            config.cliff_fee_bps,
            config.reduction_factor,
            config.fee_period,
            config.max_fee_steps,
            now.saturating_sub(pool.activation_point),
        )?;

        // Dynamic surcharge from the volatility state (decayed by idle slots,
        // plus the price drift since the anchor). Computed on the pre-swap
        // price; this swap's own move is captured for the next one.
        let vol = compute_dynamic_fee(
            pool.sqrt_price,
            pool.sqrt_price_reference,
            pool.volatility_accumulator,
            pool.volatility_reference,
            now.saturating_sub(pool.last_volatility_update),
            config.filter_period,
            config.decay_period,
            config.volatility_reduction_factor,
            config.max_volatility_accumulator,
            config.variable_fee_control,
            config.max_dynamic_fee_bps,
        );

        // Total fee, clamped strictly below 100% (compute_swap_step requires it).
        let total_fee_bps = (base_fee_bps as u32 + vol.dynamic_fee_bps as u32)
            .min(BPS_DENOMINATOR as u32 - 1) as u16;

        let step = compute_swap_step(
            pool.sqrt_price,
            pool.liquidity,
            pool.sqrt_min_price,
            pool.sqrt_max_price,
            direction,
            mode,
            amount,
            total_fee_bps,
        )?;
        require!(step.amount_out > 0, ZenithError::ZeroAmount);

        // Slippage: a floor on output (ExactIn/PartialFill) or ceiling on input.
        match mode {
            SwapMode::ExactIn | SwapMode::PartialFill => {
                require!(
                    step.amount_out >= other_amount_threshold,
                    ZenithError::SlippageExceeded
                );
            }
            SwapMode::ExactOut => {
                require!(
                    step.amount_in <= other_amount_threshold,
                    ZenithError::SlippageExceeded
                );
            }
        }

        // Route the fee: LP share into the per-liquidity accumulator (in the
        // input token); the protocol share is further split into a partner cut
        // and the remaining protocol fee, each parked on the pool for later
        // claim. All three partitions sum to exactly the fee.
        let (protocol_share, lp_share) = split_fee(step.fee, ctx.accounts.config.protocol_fee_bps)?;
        let (partner_share, protocol_remaining) =
            split_fee(protocol_share, config.partner_fee_bps)?;
        let growth = fee_growth_delta(lp_share, pool.liquidity)?;
        if a_to_b {
            pool.fee_growth_global_a = pool.fee_growth_global_a.wrapping_add(growth);
            pool.protocol_fee_a = pool
                .protocol_fee_a
                .checked_add(protocol_remaining)
                .ok_or(ZenithError::MathOverflow)?;
            pool.partner_fee_a = pool
                .partner_fee_a
                .checked_add(partner_share)
                .ok_or(ZenithError::MathOverflow)?;
        } else {
            pool.fee_growth_global_b = pool.fee_growth_global_b.wrapping_add(growth);
            pool.protocol_fee_b = pool
                .protocol_fee_b
                .checked_add(protocol_remaining)
                .ok_or(ZenithError::MathOverflow)?;
            pool.partner_fee_b = pool
                .partner_fee_b
                .checked_add(partner_share)
                .ok_or(ZenithError::MathOverflow)?;
        }
        pool.sqrt_price = step.next_sqrt_price;

        // Persist the volatility state for the next swap (the anchor re-set on a
        // new window is decided inside compute_dynamic_fee).
        pool.volatility_accumulator = vol.volatility_accumulator;
        pool.volatility_reference = vol.volatility_reference;
        pool.sqrt_price_reference = vol.sqrt_price_reference;
        pool.last_volatility_update = now;

        amount_in = step.amount_in;
        amount_out = step.amount_out;
        fee = step.fee;
        protocol_fee = protocol_share;
        amount_remaining = step.amount_remaining;
        next_sqrt_price = step.next_sqrt_price;
        total_fee_bps_out = total_fee_bps;
        volatility_out = vol.volatility_accumulator;
    }

    // Token in -> vault; vault -> token out (chosen by direction).
    let (in_user, in_vault, out_vault, out_user) = if a_to_b {
        (
            &ctx.accounts.user_token_a,
            &ctx.accounts.token_a_vault,
            &ctx.accounts.token_b_vault,
            &ctx.accounts.user_token_b,
        )
    } else {
        (
            &ctx.accounts.user_token_b,
            &ctx.accounts.token_b_vault,
            &ctx.accounts.token_a_vault,
            &ctx.accounts.user_token_a,
        )
    };

    // Pull the input (owner signs).
    transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: in_user.to_account_info(),
                to: in_vault.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        amount_in,
    )?;

    // Pay the output (pool authority signs).
    let signer_seeds: &[&[&[u8]]] = &[&[POOL_AUTHORITY_SEED, pool_key.as_ref(), &[authority_bump]]];
    transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: out_vault.to_account_info(),
                to: out_user.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount_out,
    )?;

    emit!(SwapEvent {
        pool: pool_key,
        a_to_b,
        amount_in,
        amount_out,
        fee,
        protocol_fee,
        amount_remaining,
        sqrt_price: next_sqrt_price,
        total_fee_bps: total_fee_bps_out,
        volatility_accumulator: volatility_out,
    });
    Ok(())
}
