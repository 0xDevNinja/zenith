//! Program error set for the constant-product engine.

use anchor_lang::prelude::*;

#[error_code]
pub enum CammError {
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Token mints must be different")]
    IdenticalMints,
    #[msg("Invalid fee configuration")]
    InvalidFeeConfig,
    #[msg("Pool is not active")]
    PoolNotActive,
    #[msg("Signer is not authorized for this action")]
    Unauthorized,
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Insufficient liquidity for the requested operation")]
    InsufficientLiquidity,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Unknown swap direction or mode")]
    InvalidSwapParams,
    #[msg("Requested output exceeds the available reserve")]
    InsufficientReserve,
    #[msg("First deposit is below the minimum liquidity floor")]
    BelowMinimumLiquidity,
    #[msg("Invalid yield configuration")]
    InvalidYieldConfig,
    #[msg("Yield engine is not configured for this pool")]
    YieldNotConfigured,
}
