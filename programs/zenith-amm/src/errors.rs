//! Program error set.

use anchor_lang::prelude::*;

#[error_code]
pub enum ZenithError {
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Token mints must be different")]
    IdenticalMints,
    #[msg("Invalid sqrt-price band (min must be < max and both nonzero)")]
    InvalidPriceBand,
    #[msg("Current price is outside the pool's band")]
    PriceOutOfBand,
    #[msg("Pool is not active")]
    PoolNotActive,
    #[msg("Signer is not authorized for this action")]
    Unauthorized,
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Insufficient liquidity for the requested operation")]
    InsufficientLiquidity,
    #[msg("Invalid fee configuration")]
    InvalidFeeConfig,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Position still holds liquidity or unclaimed fees")]
    PositionNotEmpty,
    #[msg("Invalid tick range (lower must be < upper and both in-domain)")]
    InvalidTickRange,
    #[msg("Tick is not a multiple of the pool's tick spacing")]
    TickNotSpaced,
    #[msg("A required tick array was not provided")]
    TickArrayNotProvided,
    #[msg("Tick array does not match the pool or expected start index")]
    TickArrayMismatch,
    #[msg("Tick is not initialized")]
    TickUninitialized,
}
