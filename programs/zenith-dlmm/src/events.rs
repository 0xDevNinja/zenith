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

/// Emitted when a position is opened over a bin range.
#[event]
pub struct PositionInitialized {
    /// The pair the position belongs to.
    pub lb_pair: Pubkey,
    /// The new position PDA.
    pub position: Pubkey,
    /// The owner allowed to modify the position.
    pub owner: Pubkey,
    /// Inclusive lower bin id.
    pub lower_bin_id: i32,
    /// Inclusive upper bin id.
    pub upper_bin_id: i32,
}

/// Emitted when liquidity is added to a position.
#[event]
pub struct LiquidityAdded {
    /// The pair.
    pub lb_pair: Pubkey,
    /// The position liquidity was added to.
    pub position: Pubkey,
    /// Total token X deposited.
    pub amount_x: u64,
    /// Total token Y deposited.
    pub amount_y: u64,
    /// Total LP shares minted across the range.
    pub shares_minted: u128,
    /// Distribution strategy used (0 Spot, 1 Curve, 2 BidAsk).
    pub strategy: u8,
}

/// Emitted when liquidity is removed from a position.
#[event]
pub struct LiquidityRemoved {
    /// The pair.
    pub lb_pair: Pubkey,
    /// The position liquidity was removed from.
    pub position: Pubkey,
    /// Total token X returned.
    pub amount_x: u64,
    /// Total token Y returned.
    pub amount_y: u64,
    /// Total LP shares burned across the range.
    pub shares_burned: u128,
    /// Basis points of each bin's shares removed.
    pub bps: u16,
}

/// Emitted when an empty position is closed and its rent reclaimed.
#[event]
pub struct PositionClosed {
    /// The pair the position belonged to.
    pub lb_pair: Pubkey,
    /// The closed position PDA.
    pub position: Pubkey,
    /// The owner the rent was returned to.
    pub owner: Pubkey,
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
