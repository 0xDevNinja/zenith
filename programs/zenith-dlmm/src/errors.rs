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
    #[msg("Invalid fee configuration")]
    InvalidFeeConfig,
    #[msg("Bin range is wider than a position can hold")]
    BinRangeTooWide,
    #[msg("Position range must lie within a single bin array")]
    PositionCrossesBinArray,
    #[msg("Bin array index does not match the requested bin id")]
    BinArrayIndexMismatch,
    #[msg("Unknown liquidity distribution strategy")]
    InvalidStrategy,
    #[msg("Unknown swap direction or mode")]
    InvalidSwapParams,
    #[msg("Cannot deposit this token for the position's bin range")]
    DepositTokenMismatch,
    #[msg("Pair is not active")]
    PairNotActive,
    #[msg("Signer is not authorized for this action")]
    Unauthorized,
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Active bin moved outside the accepted range")]
    ActiveBinIdMoved,
    #[msg("Insufficient liquidity for the requested operation")]
    InsufficientLiquidity,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Position still holds liquidity")]
    PositionNotEmpty,
    #[msg("Removal percentage must be between 1 and 10000 basis points")]
    InvalidBps,
}
