//! Pool-creation template.

use anchor_lang::prelude::*;

/// Reusable parameter set consumed when a pool is created. Lets an admin define
/// vetted defaults (fee rates, price band) once and reuse them across pools.
#[account]
#[derive(InitSpace, Debug)]
pub struct Config {
    /// Admin allowed to update this config.
    pub admin: Pubkey,
    /// Authority allowed to claim protocol fees from pools using this config.
    pub fee_authority: Pubkey,
    /// Default lower bound of the price band (sqrt price, Q64.64 raw bits).
    pub sqrt_min_price: u128,
    /// Default upper bound of the price band (sqrt price, Q64.64 raw bits).
    pub sqrt_max_price: u128,
    /// Slots per scheduler step (one fee reduction every `fee_period` slots).
    /// Ignored in `Constant` mode.
    pub fee_period: u64,
    /// Index this config was created under (part of its PDA seeds).
    pub index: u16,
    /// Base swap fee in basis points. In `Constant` mode this *is* the fee; in
    /// the decaying modes it is the floor the schedule clamps down to.
    pub base_fee_bps: u16,
    /// Protocol's share of collected fees in basis points.
    pub protocol_fee_bps: u16,
    /// Starting fee in basis points for the decaying modes (the value at pool
    /// creation, and the ceiling). Unused in `Constant` mode.
    pub cliff_fee_bps: u16,
    /// Reduction per step: bps subtracted per period (Linear) or the fraction of
    /// 10000 removed per period (Exponential, applied as `(1 - factor)^steps`).
    pub reduction_factor: u16,
    /// Maximum number of reduction steps; elapsed periods clamp to this.
    pub max_fee_steps: u16,
    /// --- dynamic (volatility) fee control ---
    /// Scales the volatility surcharge: `dynamic_fee = va^2 * control / 1e9`.
    /// Zero disables the dynamic fee.
    pub variable_fee_control: u32,
    /// Ceiling on the volatility accumulator (caps the surcharge).
    pub max_volatility_accumulator: u32,
    /// Slots within which the volatility anchor price is NOT reset (high-freq
    /// filter): rapid swaps accumulate against a stable anchor.
    pub filter_period: u32,
    /// Slots after which an idle pool's volatility fully resets to zero.
    pub decay_period: u32,
    /// Fraction (bps) the accumulator decays to between filter and decay window.
    pub volatility_reduction_factor: u16,
    /// Hard cap on the dynamic surcharge, bps.
    pub max_dynamic_fee_bps: u16,
    /// Fee scheduler mode: 0 = Constant, 1 = Linear, 2 = Exponential.
    pub fee_scheduler_mode: u8,
    /// PDA bump.
    pub bump: u8,
    /// Reserved for forward-compatible fields without a realloc.
    pub reserved: [u8; 28],
}
