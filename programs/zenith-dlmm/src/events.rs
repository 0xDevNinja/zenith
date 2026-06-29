//! Program events.

use anchor_lang::prelude::*;

/// Emitted when a liquidity-book pair is created.
#[event]
pub struct LbPairInitialized {
    /// The new pair.
    pub lb_pair: Pubkey,
    /// Token X (canonically smaller) mint.
    pub token_x_mint: Pubkey,
    /// Token Y (canonically larger) mint.
    pub token_y_mint: Pubkey,
    /// Per-bin price spacing in basis points.
    pub bin_step: u16,
    /// The bin the price starts in.
    pub active_bin_id: i32,
    /// Price of the active bin at creation (Q64.64 raw bits).
    pub active_bin_price: u128,
}

/// Emitted when a bin array is allocated for a pair.
#[event]
pub struct BinArrayInitialized {
    /// The pair the array belongs to.
    pub lb_pair: Pubkey,
    /// The new bin array.
    pub bin_array: Pubkey,
    /// Signed array index (which run of bins it covers).
    pub index: i64,
    /// Inclusive lower bin id covered by the array.
    pub lower_bin_id: i32,
    /// Inclusive upper bin id covered by the array.
    pub upper_bin_id: i32,
}
