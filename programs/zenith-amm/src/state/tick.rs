//! Tick and tick-array state for per-position concentrated liquidity.
//!
//! A **tick** is the price boundary `1.0001^t` (see [`zenith_math::sqrt_price_at_tick`]).
//! Each tick records the net/gross liquidity that references it and the fee
//! growth accumulated on its far side. Ticks are stored in fixed-size
//! [`TickArray`] accounts (a swap loads only the arrays it crosses), so the swap
//! cost is independent of how many positions exist.
//!
//! ## Layout
//!
//! Both structs are `zero_copy` (`Pod`, `repr(C)`, no padding). [`Tick`] is four
//! 16-byte fields = 64 bytes, naturally 16-aligned. [`TickArray`] leads with the
//! 16-aligned tick array, then the (1-aligned) pool key, then the tick index, so
//! nothing needs interior padding; explicit trailing padding rounds the total to
//! a multiple of 16. Do not reorder without re-checking the layout test.

use anchor_lang::prelude::*;

use crate::constants::TICKS_PER_ARRAY;

/// One price tick. `liquidity_net` is the signed change to apply to the pool's
/// active liquidity when the price crosses this tick moving **upward**;
/// `liquidity_gross` is the sum of `|ΔL|` referencing the tick (used to know
/// when the tick becomes uninitialized). Fee-growth-outside values are Q64.64
/// raw bits and are the far-side accumulators used by the fee-growth-inside
/// identity.
#[zero_copy]
#[repr(C)]
#[derive(Debug, Default)]
pub struct Tick {
    /// Net liquidity added when crossing this tick left-to-right (signed).
    pub liquidity_net: i128,
    /// Total liquidity referencing this tick. `0` ⇒ tick is uninitialized.
    pub liquidity_gross: u128,
    /// Fee growth on the far side of this tick, token A (Q64.64 raw bits).
    pub fee_growth_outside_a: u128,
    /// Fee growth on the far side of this tick, token B (Q64.64 raw bits).
    pub fee_growth_outside_b: u128,
}

impl Tick {
    /// `true` if any liquidity references this tick.
    pub fn is_initialized(&self) -> bool {
        self.liquidity_gross != 0
    }
}

/// A contiguous run of [`TICKS_PER_ARRAY`] ticks starting at `start_tick_index`.
///
/// The array covers tick indices `[start, start + tick_spacing·TICKS_PER_ARRAY)`
/// at stride `tick_spacing`; slot `i` holds tick `start + i·tick_spacing`.
#[account(zero_copy)]
#[repr(C)]
pub struct TickArray {
    /// The ticks, in ascending index order.
    pub ticks: [Tick; TICKS_PER_ARRAY],
    /// Pool this array belongs to.
    pub pool: Pubkey,
    /// Index of the first tick in the array (a multiple of `tick_spacing·TICKS_PER_ARRAY`).
    pub start_tick_index: i32,
    /// Trailing padding to keep the struct 16-byte sized (no Pod padding).
    pub padding: [u8; 12],
}

impl TickArray {
    /// On-chain byte length including the 8-byte account discriminator.
    pub const LEN: usize = 8 + core::mem::size_of::<TickArray>();

    /// The tick span one array covers for a given spacing.
    #[inline]
    pub fn span(tick_spacing: u16) -> i32 {
        tick_spacing as i32 * TICKS_PER_ARRAY as i32
    }

    /// The `start_tick_index` of the array that contains `tick` (floors toward
    /// negative infinity so it is correct for negative ticks).
    #[inline]
    pub fn start_index_for(tick: i32, tick_spacing: u16) -> i32 {
        let span = Self::span(tick_spacing);
        tick.div_euclid(span) * span
    }

    /// `true` if `tick` falls within this array's covered range.
    #[inline]
    pub fn contains(&self, tick: i32, tick_spacing: u16) -> bool {
        tick >= self.start_tick_index && tick < self.start_tick_index + Self::span(tick_spacing)
    }

    /// Slot index of `tick` within this array, or `None` if the tick is out of
    /// range or not aligned to `tick_spacing`.
    #[inline]
    pub fn slot_of(&self, tick: i32, tick_spacing: u16) -> Option<usize> {
        if tick_spacing == 0 || !self.contains(tick, tick_spacing) {
            return None;
        }
        if tick % tick_spacing as i32 != 0 {
            return None;
        }
        Some(((tick - self.start_tick_index) / tick_spacing as i32) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_is_64_bytes_aligned_16() {
        assert_eq!(core::mem::size_of::<Tick>(), 64);
        assert_eq!(core::mem::align_of::<Tick>(), 16);
    }

    #[test]
    fn tick_array_layout_is_pod_sound() {
        assert_eq!(core::mem::align_of::<TickArray>(), 16);
        assert_eq!(core::mem::size_of::<TickArray>() % 16, 0);
        // 88*64 ticks + 32 pool + 4 start + 12 padding = 5680.
        assert_eq!(core::mem::size_of::<TickArray>(), 88 * 64 + 32 + 4 + 12);
        assert_eq!(core::mem::size_of::<TickArray>(), 5680);
        assert_eq!(TickArray::LEN, 8 + 5680);
    }

    #[test]
    fn tick_initialized_flag() {
        let mut t = Tick::default();
        assert!(!t.is_initialized());
        t.liquidity_gross = 1;
        assert!(t.is_initialized());
    }

    #[test]
    fn start_index_floors_toward_negative() {
        // spacing 64 -> span = 64*88 = 5632.
        let s = 64u16;
        assert_eq!(TickArray::start_index_for(0, s), 0);
        assert_eq!(TickArray::start_index_for(5631, s), 0);
        assert_eq!(TickArray::start_index_for(5632, s), 5632);
        // negatives floor down, not toward zero.
        assert_eq!(TickArray::start_index_for(-1, s), -5632);
        assert_eq!(TickArray::start_index_for(-5632, s), -5632);
        assert_eq!(TickArray::start_index_for(-5633, s), -11264);
    }

    #[test]
    fn slot_of_range_and_alignment() {
        let s = 64u16;
        let mut ta: TickArray = bytemuck::Zeroable::zeroed();
        ta.start_tick_index = 0;
        // aligned, in-range
        assert_eq!(ta.slot_of(0, s), Some(0));
        assert_eq!(ta.slot_of(64, s), Some(1));
        assert_eq!(ta.slot_of(64 * 87, s), Some(87));
        // out of range (next array)
        assert_eq!(ta.slot_of(64 * 88, s), None);
        assert_eq!(ta.slot_of(-64, s), None);
        // not aligned to spacing
        assert_eq!(ta.slot_of(1, s), None);
        assert_eq!(ta.slot_of(63, s), None);
        // spacing 0 guarded
        assert_eq!(ta.slot_of(0, 0), None);
    }

    #[test]
    fn tick_array_zero_copy_round_trip() {
        let mut ta: TickArray = bytemuck::Zeroable::zeroed();
        ta.pool = Pubkey::new_unique();
        ta.start_tick_index = -5632;
        ta.ticks[3].liquidity_net = -1234;
        ta.ticks[3].liquidity_gross = 5678;
        ta.ticks[3].fee_growth_outside_a = 42;
        let bytes = bytemuck::bytes_of(&ta);
        let back: &TickArray = bytemuck::from_bytes(bytes);
        assert_eq!(back.start_tick_index, -5632);
        assert_eq!(back.ticks[3].liquidity_net, -1234);
        assert_eq!(back.ticks[3].liquidity_gross, 5678);
        assert_eq!(back.ticks[3].fee_growth_outside_a, 42);
        assert_eq!(back.pool, ta.pool);
        assert!(back.ticks[3].is_initialized());
        assert!(!back.ticks[0].is_initialized());
    }
}
