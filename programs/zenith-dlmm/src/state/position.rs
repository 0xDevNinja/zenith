//! Liquidity position.
//!
//! A DLMM position owns LP shares across a contiguous `[lower_bin_id,
//! upper_bin_id]` range of bins (at most [`MAX_BINS_PER_POSITION`] wide). It is
//! `zero_copy` because the per-bin share array makes it large and it is touched
//! on every add/remove.
//!
//! Ownership is the `owner` field; the account address is a PDA of a
//! caller-supplied `base` pubkey, so one owner can hold many positions.

use anchor_lang::prelude::*;

use crate::constants::MAX_BINS_PER_POSITION;

/// Per-bin fee accounting for a position. Reserved for M4b (per-bin fee
/// accrual + claim); all fields are zero in M4.
#[zero_copy]
#[repr(C)]
#[derive(Default)]
pub struct PositionBinData {
    /// Last-observed per-share fee growth in token X for this bin (Q64.64).
    pub fee_x_checkpoint: u128,
    /// Last-observed per-share fee growth in token Y for this bin (Q64.64).
    pub fee_y_checkpoint: u128,
    /// Fees owed in token X, accrued but not yet claimed.
    pub fee_x_pending: u64,
    /// Fees owed in token Y, accrued but not yet claimed.
    pub fee_y_pending: u64,
}

#[account(zero_copy)]
#[repr(C)]
pub struct Position {
    /// LP shares owned in each bin of the range. `liquidity_shares[i]`
    /// corresponds to bin id `lower_bin_id + i`.
    pub liquidity_shares: [u128; MAX_BINS_PER_POSITION],
    /// Per-bin fee accounting (M4b; zero in M4).
    pub fee_infos: [PositionBinData; MAX_BINS_PER_POSITION],
    /// The pair this position belongs to.
    pub lb_pair: Pubkey,
    /// The owner allowed to add/remove liquidity and claim fees.
    pub owner: Pubkey,
    /// The base pubkey this position's PDA was derived from (its unique id).
    pub base: Pubkey,
    /// Inclusive lower bin id of the position's range.
    pub lower_bin_id: i32,
    /// Inclusive upper bin id of the position's range.
    pub upper_bin_id: i32,
    /// PDA bump.
    pub bump: u8,
    /// Trailing padding to keep the struct 16-byte sized (no Pod padding).
    pub padding: [u8; 7],
}

impl Position {
    /// On-chain byte length including the 8-byte account discriminator.
    pub const LEN: usize = 8 + core::mem::size_of::<Position>();

    /// Number of bins the position spans (inclusive width).
    pub fn width(&self) -> i64 {
        self.upper_bin_id as i64 - self.lower_bin_id as i64 + 1
    }

    /// Local slot in `liquidity_shares` for `bin_id`, or `None` if `bin_id` is
    /// outside the position's range.
    pub fn slot_of(&self, bin_id: i32) -> Option<usize> {
        if bin_id < self.lower_bin_id || bin_id > self.upper_bin_id {
            return None;
        }
        Some((bin_id - self.lower_bin_id) as usize)
    }

    /// `true` if the position holds no shares in any bin.
    pub fn is_empty(&self) -> bool {
        self.liquidity_shares.iter().all(|&s| s == 0)
    }
}
