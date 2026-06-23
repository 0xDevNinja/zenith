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
    /// Index this config was created under (part of its PDA seeds).
    pub index: u16,
    /// Base swap fee in basis points.
    pub base_fee_bps: u16,
    /// Protocol's share of collected fees in basis points.
    pub protocol_fee_bps: u16,
    /// PDA bump.
    pub bump: u8,
    /// Reserved for forward-compatible fields without a realloc.
    pub reserved: [u8; 64],
}
