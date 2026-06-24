//! `create_config` — define a reusable pool-creation template.

use anchor_lang::prelude::*;

use crate::constants::CONFIG_SEED;
use crate::errors::ZenithError;
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

/// Create a config template at `index`.
///
/// `sqrt_min_price` / `sqrt_max_price` are Q64.64 raw bits and must satisfy
/// `0 < min < max` (any such band has interior prices for pools to use); fees
/// are basis points and must be `<= 10000`. Config creation is permissionless
/// and indices are first-come: a config only affects pools that choose it, and
/// `initialize_pool` re-reads the band/fees from the chosen (seed-validated)
/// config, so a junk config cannot affect other pools.
#[allow(clippy::too_many_arguments)]
pub fn create_config(
    ctx: Context<CreateConfig>,
    index: u16,
    fee_authority: Pubkey,
    sqrt_min_price: u128,
    sqrt_max_price: u128,
    base_fee_bps: u16,
    protocol_fee_bps: u16,
) -> Result<()> {
    // A price strictly between the bounds must exist, so reuse the band check
    // with the midpoint as a representative interior price.
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

    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.fee_authority = fee_authority;
    config.sqrt_min_price = sqrt_min_price;
    config.sqrt_max_price = sqrt_max_price;
    config.index = index;
    config.base_fee_bps = base_fee_bps;
    config.protocol_fee_bps = protocol_fee_bps;
    config.bump = ctx.bumps.config;
    config.reserved = [0u8; 64];

    Ok(())
}
