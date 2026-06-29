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
use crate::errors::DlmmError;

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
    ///
    /// The offset is computed in `i64` so it cannot overflow; the range check
    /// guarantees a non-negative slot, and `debug_assert` guards the
    /// width-<= [`MAX_BINS_PER_POSITION`] invariant the handlers must enforce.
    pub fn slot_of(&self, bin_id: i32) -> Option<usize> {
        if bin_id < self.lower_bin_id || bin_id > self.upper_bin_id {
            return None;
        }
        let slot = (bin_id as i64 - self.lower_bin_id as i64) as usize;
        debug_assert!(slot < MAX_BINS_PER_POSITION);
        Some(slot)
    }

    /// `true` if the position holds no shares in any bin.
    pub fn is_empty(&self) -> bool {
        self.liquidity_shares.iter().all(|&s| s == 0)
    }

    /// `true` if the position has any unclaimed fees pending.
    pub fn has_pending_fees(&self) -> bool {
        self.fee_infos
            .iter()
            .any(|f| f.fee_x_pending != 0 || f.fee_y_pending != 0)
    }

    /// Settle bin `slot`'s accrued fees into its pending balance and advance the
    /// checkpoint to `growth_x` / `growth_y` (the bin's current per-share fee
    /// growth). Call before changing the bin's share count so a share change
    /// never rewrites past earnings.
    pub fn settle_bin(&mut self, slot: usize, growth_x: u128, growth_y: u128) -> Result<()> {
        let shares = self.liquidity_shares[slot];
        let owed_x = u64::try_from(crate::fee::owed_fee(
            shares,
            growth_x,
            self.fee_infos[slot].fee_x_checkpoint,
        ))
        .map_err(|_| DlmmError::MathOverflow)?;
        let owed_y = u64::try_from(crate::fee::owed_fee(
            shares,
            growth_y,
            self.fee_infos[slot].fee_y_checkpoint,
        ))
        .map_err(|_| DlmmError::MathOverflow)?;
        let info = &mut self.fee_infos[slot];
        info.fee_x_pending = info
            .fee_x_pending
            .checked_add(owed_x)
            .ok_or(DlmmError::MathOverflow)?;
        info.fee_y_pending = info
            .fee_y_pending
            .checked_add(owed_y)
            .ok_or(DlmmError::MathOverflow)?;
        info.fee_x_checkpoint = growth_x;
        info.fee_y_checkpoint = growth_y;
        Ok(())
    }

    /// Sum all bins' pending fees and zero them, returning `(total_x, total_y)`.
    pub fn take_pending(&mut self) -> Result<(u64, u64)> {
        let (mut x, mut y) = (0u64, 0u64);
        for info in self.fee_infos.iter_mut() {
            x = x
                .checked_add(info.fee_x_pending)
                .ok_or(DlmmError::MathOverflow)?;
            y = y
                .checked_add(info.fee_y_pending)
                .ok_or(DlmmError::MathOverflow)?;
            info.fee_x_pending = 0;
            info.fee_y_pending = 0;
        }
        Ok((x, y))
    }
}
