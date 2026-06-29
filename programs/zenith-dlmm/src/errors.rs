//! Program error set for the DLMM.

use anchor_lang::prelude::*;

#[error_code]
pub enum DlmmError {
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Token mints must be different")]
    IdenticalMints,
    #[msg("Invalid bin step (must be nonzero and within the supported range)")]
    InvalidBinStep,
    #[msg("Active bin id is outside the price range the bin step supports")]
    BinIdOutOfRange,
    #[msg("Invalid bin range (lower must be <= upper)")]
    InvalidBinRange,
    #[msg("Bin range is wider than a position can hold")]
    BinRangeTooWide,
    #[msg("Bin array index does not match the requested bin id")]
    BinArrayIndexMismatch,
    #[msg("Pair is not active")]
    PairNotActive,
    #[msg("Signer is not authorized for this action")]
    Unauthorized,
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Insufficient liquidity for the requested operation")]
    InsufficientLiquidity,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Position still holds liquidity")]
    PositionNotEmpty,
}
