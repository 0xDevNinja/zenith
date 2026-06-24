//! `create_config` — define a reusable pool-creation template.

use anchor_lang::prelude::*;

use crate::constants::CONFIG_SEED;
use crate::errors::ZenithError;
use crate::math::{FEE_MODE_CONSTANT, FEE_MODE_EXPONENTIAL, FEE_MODE_LINEAR};
use crate::state::Config;

/// Basis-point denominator (100%).
const BPS_DENOMINATOR: u16 = 10_000;

#[derive(Accounts)]
#[instruction(index: u16)]
pub struct CreateConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED, &index.to_le_bytes()],
        bump
    )]
    pub config: Account<'info, Config>,

    pub system_program: Program<'info, System>,
}

/// Fee scheduler params: starting fee, decay shape, and timing. See
/// [`crate::math::scheduled_base_fee_bps`] for how they map to a live fee.
pub struct FeeSchedulerParams {
    /// 0 = Constant, 1 = Linear, 2 = Exponential.
    pub mode: u8,
    /// Starting (and ceiling) fee for the decaying modes, bps.
    pub cliff_fee_bps: u16,
    /// Linear: bps removed per step. Exponential: bps-fraction removed per step.
    pub reduction_factor: u16,
    /// Slots per reduction step.
    pub fee_period: u64,
    /// Maximum number of reduction steps.
    pub max_fee_steps: u16,
}

/// Create a config template at `index`.
///
/// `sqrt_min_price` / `sqrt_max_price` are Q64.64 raw bits and must satisfy
/// `0 < min < max`. `base_fee_bps` is the constant fee (Constant mode) or the
/// floor (decaying modes); `protocol_fee_bps` is the protocol's share of the
/// fee. Config creation is permissionless and indices are first-come: a config
/// only affects pools that choose it, and a pool re-reads everything from its
/// chosen (seed-validated) config, so a junk config cannot affect other pools.
#[allow(clippy::too_many_arguments)]
pub fn create_config(
    ctx: Context<CreateConfig>,
    index: u16,
    fee_authority: Pubkey,
    sqrt_min_price: u128,
    sqrt_max_price: u128,
    base_fee_bps: u16,
    protocol_fee_bps: u16,
    fee_scheduler: FeeSchedulerParams,
) -> Result<()> {
    require!(
        sqrt_min_price > 0 && sqrt_min_price < sqrt_max_price,
        ZenithError::InvalidPriceBand
    );
    // base_fee_bps must be strictly below 100%: a swap nets `input * (1 -
    // base_fee_bps/10000)` and the on-top fee divides by `10000 - base_fee_bps`,
    // both of which break at exactly 100% (and `compute_swap_step` rejects it),
    // so a 100% config would silently brick every swap on the pool.
    require!(
        base_fee_bps < BPS_DENOMINATOR && protocol_fee_bps <= BPS_DENOMINATOR,
        ZenithError::InvalidFeeConfig
    );
    validate_fee_scheduler(&fee_scheduler, base_fee_bps)?;

    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.fee_authority = fee_authority;
    config.sqrt_min_price = sqrt_min_price;
    config.sqrt_max_price = sqrt_max_price;
    config.fee_period = fee_scheduler.fee_period;
    config.index = index;
    config.base_fee_bps = base_fee_bps;
    config.protocol_fee_bps = protocol_fee_bps;
    config.cliff_fee_bps = fee_scheduler.cliff_fee_bps;
    config.reduction_factor = fee_scheduler.reduction_factor;
    config.max_fee_steps = fee_scheduler.max_fee_steps;
    config.fee_scheduler_mode = fee_scheduler.mode;
    config.bump = ctx.bumps.config;
    config.reserved = [0u8; 48];

    Ok(())
}

/// Reject nonsensical scheduler params. Constant mode ignores the rest; the
/// decaying modes need a positive period/step count, a cliff at or above the
/// floor and below 100%, and (for Exponential) a reduction below 100%.
fn validate_fee_scheduler(s: &FeeSchedulerParams, base_fee_bps: u16) -> Result<()> {
    match s.mode {
        FEE_MODE_CONSTANT => Ok(()),
        FEE_MODE_LINEAR | FEE_MODE_EXPONENTIAL => {
            require!(
                s.fee_period > 0 && s.max_fee_steps > 0,
                ZenithError::InvalidFeeConfig
            );
            require!(
                s.cliff_fee_bps >= base_fee_bps && s.cliff_fee_bps < BPS_DENOMINATOR,
                ZenithError::InvalidFeeConfig
            );
            // Reduction must be in (0, 100%): zero would mean a "decaying"
            // config that never decays (silently stuck at the cliff), and the
            // exponential base `(1 - factor)` must stay positive.
            require!(
                s.reduction_factor > 0 && s.reduction_factor < BPS_DENOMINATOR,
                ZenithError::InvalidFeeConfig
            );
            Ok(())
        }
        _ => Err(ZenithError::InvalidFeeConfig.into()),
    }
}
