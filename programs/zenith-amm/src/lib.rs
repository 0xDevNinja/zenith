//! Zenith AMM — concentrated-liquidity automated market maker.
//!
//! sqrt-price math over a fixed price band per pool; liquidity owned via
//! position NFTs (there is no fungible LP-token mint). Instruction handlers
//! land in M1/M1b — this crate currently defines the on-chain account model,
//! PDA derivation, and error set.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod math;
pub mod pda;
pub mod state;

pub use constants::*;
pub use errors::ZenithError;
pub use instructions::*;
pub use state::*;

// Program ID. The matching keypair lives in target/deploy/ (gitignored);
// run `anchor keys sync` after generating deploy keypairs to keep this and
// Anchor.toml aligned for an actual deploy.
declare_id!("AA8cKcHQj63GEHRaLrrT87W1efRZ44U147JTCXC2Rmkq");

#[program]
pub mod zenith_amm {
    use super::*;

    /// Create a reusable pool-creation config template.
    #[allow(clippy::too_many_arguments)]
    pub fn create_config(
        ctx: Context<CreateConfig>,
        index: u16,
        fee_authority: Pubkey,
        sqrt_min_price: u128,
        sqrt_max_price: u128,
        base_fee_bps: u16,
        protocol_fee_bps: u16,
        partner: Pubkey,
        partner_fee_bps: u16,
        fee_scheduler_mode: u8,
        cliff_fee_bps: u16,
        reduction_factor: u16,
        fee_period: u64,
        max_fee_steps: u16,
        variable_fee_control: u32,
        max_volatility_accumulator: u32,
        filter_period: u32,
        decay_period: u32,
        volatility_reduction_factor: u16,
        max_dynamic_fee_bps: u16,
    ) -> Result<()> {
        instructions::create_config(
            ctx,
            index,
            fee_authority,
            sqrt_min_price,
            sqrt_max_price,
            base_fee_bps,
            protocol_fee_bps,
            partner,
            partner_fee_bps,
            instructions::FeeSchedulerParams {
                mode: fee_scheduler_mode,
                cliff_fee_bps,
                reduction_factor,
                fee_period,
                max_fee_steps,
            },
            instructions::DynamicFeeParams {
                variable_fee_control,
                max_volatility_accumulator,
                filter_period,
                decay_period,
                volatility_reduction_factor,
                max_dynamic_fee_bps,
            },
        )
    }

    /// Create a pool from a config and open the creator's first position.
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        sqrt_price: u128,
        liquidity: u128,
        token_a_max: u64,
        token_b_max: u64,
    ) -> Result<()> {
        instructions::initialize_pool(ctx, sqrt_price, liquidity, token_a_max, token_b_max)
    }

    /// Open an empty liquidity position (mints its ownership NFT).
    pub fn create_position(ctx: Context<CreatePosition>) -> Result<()> {
        instructions::create_position(ctx)
    }

    /// Add liquidity to a position (deposits round up, capped by `*_max`).
    pub fn add_liquidity(
        ctx: Context<ModifyLiquidity>,
        liquidity_delta: u128,
        token_a_max: u64,
        token_b_max: u64,
    ) -> Result<()> {
        instructions::add_liquidity(ctx, liquidity_delta, token_a_max, token_b_max)
    }

    /// Remove liquidity from a position (returns round down, floored by `*_min`).
    pub fn remove_liquidity(
        ctx: Context<ModifyLiquidity>,
        liquidity_delta: u128,
        token_a_min: u64,
        token_b_min: u64,
    ) -> Result<()> {
        instructions::remove_liquidity(ctx, liquidity_delta, token_a_min, token_b_min)
    }

    /// Remove all unlocked liquidity from a position.
    pub fn remove_all_liquidity(
        ctx: Context<ModifyLiquidity>,
        token_a_min: u64,
        token_b_min: u64,
    ) -> Result<()> {
        instructions::remove_all_liquidity(ctx, token_a_min, token_b_min)
    }

    /// Execute a swap (ExactIn / ExactOut / PartialFill) with band protection.
    pub fn swap(
        ctx: Context<Swap>,
        direction: crate::math::SwapDirection,
        mode: crate::math::SwapMode,
        amount: u64,
        other_amount_threshold: u64,
    ) -> Result<()> {
        instructions::swap(ctx, direction, mode, amount, other_amount_threshold)
    }

    /// Settle and pay out a position's accrued LP fees.
    pub fn claim_position_fee(ctx: Context<ClaimPositionFee>) -> Result<()> {
        instructions::claim_position_fee(ctx)
    }

    /// Close an empty position (burn the NFT, reclaim rent).
    pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
        instructions::close_position(ctx)
    }

    /// Claim the pool's accrued protocol fees (config fee authority only).
    pub fn claim_protocol_fee(ctx: Context<ClaimProtocolFee>) -> Result<()> {
        instructions::claim_protocol_fee(ctx)
    }

    /// Claim the pool's accrued partner fees (config partner only).
    pub fn claim_partner_fee(ctx: Context<ClaimPartnerFee>) -> Result<()> {
        instructions::claim_partner_fee(ctx)
    }

    /// Toggle fee-compounding mode for a position.
    pub fn set_position_compounding(
        ctx: Context<SetPositionCompounding>,
        enabled: bool,
    ) -> Result<()> {
        instructions::set_position_compounding(ctx, enabled)
    }
}
