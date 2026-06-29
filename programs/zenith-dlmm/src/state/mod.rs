//! On-chain account state for the DLMM.

mod bin_array;
mod lb_pair;
mod position;

pub use bin_array::{Bin, BinArray};
pub use lb_pair::{LbPair, PairStatus, TokenFlavor};
pub use position::{Position, PositionBinData};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{MAX_BINS_PER_ARRAY, MAX_BINS_PER_POSITION};
    use anchor_lang::prelude::*;

    /// `create_account` can allocate up to the 10 MiB account cap in one call,
    /// far above what these accounts need. This deliberately conservative 10 KB
    /// budget just asserts the batched accounts stay small enough to create
    /// (and grow via realloc, were that ever needed) without trouble.
    const SINGLE_ALLOC_BUDGET: usize = 10 * 1024;

    #[test]
    fn zero_copy_layouts_are_pod_sound() {
        // The real no-padding guarantee is the `#[derive(Pod)]` emitted by
        // `zero_copy`: it fails to compile if a struct has any padding byte, so
        // these structs compiling IS the proof. This test additionally pins the
        // intended shape — 16-byte alignment (a u128 is present) and a size
        // that is a multiple of it — to catch an accidental field reorder.
        for (align, size) in [
            (
                core::mem::align_of::<LbPair>(),
                core::mem::size_of::<LbPair>(),
            ),
            (core::mem::align_of::<Bin>(), core::mem::size_of::<Bin>()),
            (
                core::mem::align_of::<BinArray>(),
                core::mem::size_of::<BinArray>(),
            ),
            (
                core::mem::align_of::<PositionBinData>(),
                core::mem::size_of::<PositionBinData>(),
            ),
            (
                core::mem::align_of::<Position>(),
                core::mem::size_of::<Position>(),
            ),
        ] {
            assert_eq!(align, 16);
            assert_eq!(size % 16, 0);
        }
    }

    #[test]
    fn account_sizes_match_documented_layout() {
        assert_eq!(core::mem::size_of::<LbPair>(), 352);
        assert_eq!(core::mem::size_of::<Bin>(), 64);
        assert_eq!(core::mem::size_of::<BinArray>(), 70 * 64 + 32 + 8 + 1 + 7);
        assert_eq!(core::mem::size_of::<BinArray>(), 4528);
        assert_eq!(core::mem::size_of::<PositionBinData>(), 48);
        assert_eq!(core::mem::size_of::<Position>(), 4592);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // sizes are const by design
    fn accounts_fit_a_single_allocation() {
        // The two batched accounts are the only large ones; both must allocate
        // in one create_account (no realloc) per the issue's size-limit AC.
        assert!(BinArray::LEN <= SINGLE_ALLOC_BUDGET);
        assert!(Position::LEN <= SINGLE_ALLOC_BUDGET);
    }

    #[test]
    fn lb_pair_zero_copy_round_trip() {
        let mut pair: LbPair = bytemuck::Zeroable::zeroed();
        pair.bin_step = 25;
        pair.active_bin_id = -1234;
        pair.base_fee_bps = 30;
        pair.status = PairStatus::Active as u8;
        pair.pair_authority_bump = 254;
        pair.token_x_mint = Pubkey::new_unique();
        pair.reserve_x = Pubkey::new_unique();
        pair.activation_point = 999;

        let bytes = bytemuck::bytes_of(&pair);
        let back: &LbPair = bytemuck::from_bytes(bytes);
        assert_eq!(back.bin_step, 25);
        assert_eq!(back.active_bin_id, -1234);
        assert_eq!(back.base_fee_bps, 30);
        assert_eq!(back.activation_point, 999);
        assert_eq!(back.status(), PairStatus::Active);
        assert!(back.is_active());
        assert_eq!(back.token_x_mint, pair.token_x_mint);
    }

    #[test]
    fn bin_array_round_trip_and_slot_mapping() {
        let mut arr: BinArray = bytemuck::Zeroable::zeroed();
        arr.index = 2; // covers bins [140, 209]
        arr.bins[0].amount_x = 5;
        arr.bins[3].liquidity_supply = 100;

        let bytes = bytemuck::bytes_of(&arr);
        let back: &BinArray = bytemuck::from_bytes(bytes);
        assert_eq!(back.bins[0].amount_x, 5);
        assert_eq!(back.bins[3].liquidity_supply, 100);

        // bin 140 is local slot 0, bin 143 is slot 3, bin 209 is slot 69.
        assert_eq!(back.slot_of(140), Some(0));
        assert_eq!(back.slot_of(143), Some(3));
        assert_eq!(back.slot_of(209), Some(MAX_BINS_PER_ARRAY - 1));
        // outside this array
        assert_eq!(back.slot_of(139), None);
        assert_eq!(back.slot_of(210), None);
    }

    #[test]
    fn position_round_trip_and_helpers() {
        let mut pos: Position = bytemuck::Zeroable::zeroed();
        pos.lower_bin_id = -5;
        pos.upper_bin_id = 4; // width 10
        pos.owner = Pubkey::new_unique();
        pos.base = Pubkey::new_unique();
        pos.liquidity_shares[0] = 7; // bin -5
        pos.liquidity_shares[9] = 9; // bin 4

        let bytes = bytemuck::bytes_of(&pos);
        let back: &Position = bytemuck::from_bytes(bytes);
        assert_eq!(back.width(), 10);
        assert_eq!(back.slot_of(-5), Some(0));
        assert_eq!(back.slot_of(4), Some(9));
        assert_eq!(back.slot_of(-6), None);
        assert_eq!(back.slot_of(5), None);
        assert!(!back.is_empty());
        assert_eq!(back.owner, pos.owner);

        let empty: Position = bytemuck::Zeroable::zeroed();
        assert!(empty.is_empty());
    }

    #[test]
    fn bin_array_index_is_floor_division() {
        let n = MAX_BINS_PER_ARRAY as i32;
        // positive
        assert_eq!(BinArray::index_of(0), 0);
        assert_eq!(BinArray::index_of(n - 1), 0);
        assert_eq!(BinArray::index_of(n), 1);
        // negative uses floor (not truncation toward zero)
        assert_eq!(BinArray::index_of(-1), -1);
        assert_eq!(BinArray::index_of(-n), -1);
        assert_eq!(BinArray::index_of(-n - 1), -2);

        // bounds round-trip: every bin id maps back into its array's range
        for &bin in &[-3 * n, -n - 1, -1, 0, 1, n, 3 * n + 7] {
            let idx = BinArray::index_of(bin);
            let (lo, hi) = BinArray::bounds(idx);
            assert!(lo <= bin && bin <= hi, "bin {bin} not in [{lo},{hi}]");
        }
    }

    #[test]
    fn try_bounds_rejects_out_of_range_indices() {
        // Normal indices resolve.
        assert_eq!(
            BinArray::try_bounds(0),
            Some((0, MAX_BINS_PER_ARRAY as i32 - 1))
        );
        assert!(BinArray::try_bounds(-1).is_some());
        // An index whose range leaves the i32 bin-id space returns None instead
        // of silently wrapping (checked arithmetic).
        assert_eq!(BinArray::try_bounds(i64::MAX), None);
        assert_eq!(BinArray::try_bounds(i64::MIN), None);
        assert_eq!(BinArray::try_bounds(i32::MAX as i64), None); // *70 overflows i32
        assert_eq!(BinArray::try_bounds(i32::MIN as i64), None);
    }

    #[test]
    fn position_cannot_exceed_max_width() {
        // A full-width position spans exactly MAX_BINS_PER_POSITION bins, which
        // must fit the fixed share array.
        let mut pos: Position = bytemuck::Zeroable::zeroed();
        pos.lower_bin_id = 0;
        pos.upper_bin_id = (MAX_BINS_PER_POSITION - 1) as i32;
        assert_eq!(pos.width(), MAX_BINS_PER_POSITION as i64);
        assert_eq!(pos.liquidity_shares.len(), MAX_BINS_PER_POSITION);
    }
}
