//! Bins and bin arrays.
//!
//! A *bin* is a fixed-price bucket of liquidity (constant-sum within the bin, so
//! trades inside one bin have zero slippage). Bins are too small to each own an
//! account, so a contiguous run of [`MAX_BINS_PER_ARRAY`] bins is packed into
//! one `zero_copy` [`BinArray`] account, addressed by a signed array index.

use anchor_lang::prelude::*;

use crate::constants::MAX_BINS_PER_ARRAY;

/// A single price bin: token reserves plus the total LP shares minted against
/// them. Share accounting lets multiple positions own slices of one bin and
/// withdraw proportionally.
#[zero_copy]
#[repr(C)]
#[derive(Default)]
pub struct Bin {
    /// Cumulative per-share fee growth in token X (Q64.64 raw bits); a swap's
    /// LP fee share is added here, and positions claim against their checkpoint.
    pub fee_growth_x: u128,
    /// Cumulative per-share fee growth in token Y (Q64.64 raw bits).
    pub fee_growth_y: u128,
    /// Total LP shares minted against this bin's reserves.
    pub liquidity_supply: u128,
    /// Token X reserve held in this bin (raw token units).
    pub amount_x: u64,
    /// Token Y reserve held in this bin (raw token units).
    pub amount_y: u64,
}

#[account(zero_copy)]
#[repr(C)]
pub struct BinArray {
    /// The packed bins. `bins[i]` is bin id `index * MAX_BINS_PER_ARRAY + i`.
    pub bins: [Bin; MAX_BINS_PER_ARRAY],
    /// The pair this array belongs to.
    pub lb_pair: Pubkey,
    /// Signed array index: which run of bins this account covers.
    pub index: i64,
    /// PDA bump.
    pub bump: u8,
    /// Trailing padding to keep the struct 16-byte sized (no Pod padding).
    pub padding: [u8; 7],
}

impl BinArray {
    /// On-chain byte length including the 8-byte account discriminator.
    pub const LEN: usize = 8 + core::mem::size_of::<BinArray>();

    /// The signed array index that contains `bin_id` (floor division, so it is
    /// correct for negative bin ids too).
    pub fn index_of(bin_id: i32) -> i64 {
        (bin_id as i64).div_euclid(MAX_BINS_PER_ARRAY as i64)
    }

    /// Inclusive `[lower, upper]` bin-id range covered by array `index`, or
    /// `None` if the range would fall outside the `i32` bin-id space.
    ///
    /// All arithmetic is checked, so a caller passing an extreme `index` gets
    /// `None` rather than a silently wrapped range (the `debug_assert` that
    /// previously guarded this is compiled out in release/BPF builds).
    pub fn try_bounds(index: i64) -> Option<(i32, i32)> {
        let n = MAX_BINS_PER_ARRAY as i64;
        let lower = index.checked_mul(n)?;
        let upper = lower.checked_add(n - 1)?;
        Some((i32::try_from(lower).ok()?, i32::try_from(upper).ok()?))
    }

    /// Inclusive `[lower, upper]` bin-id range covered by array `index`.
    ///
    /// Panics if `index` is outside the `i32` bin-id space; callers handling
    /// untrusted input should use [`Self::try_bounds`] instead.
    pub fn bounds(index: i64) -> (i32, i32) {
        Self::try_bounds(index).expect("array index outside the i32 bin-id range")
    }

    /// Local slot within this array's `bins` for a global `bin_id`, or `None`
    /// if `bin_id` does not fall in this array.
    pub fn slot_of(&self, bin_id: i32) -> Option<usize> {
        if Self::index_of(bin_id) != self.index {
            return None;
        }
        let lower = self.index * MAX_BINS_PER_ARRAY as i64;
        Some((bin_id as i64 - lower) as usize)
    }
}
