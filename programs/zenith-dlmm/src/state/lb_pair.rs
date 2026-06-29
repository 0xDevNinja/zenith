//! Liquidity-book pair state.
//!
//! `LbPair` is the central account for a DLMM market: it tracks the two token
//! mints, the bin step (price spacing), the currently active bin, and the
//! reserves. It is read/written on every swap and liquidity change, so it is
//! `zero_copy`: cast directly from the account bytes and mutated in place.
//!
//! ## Layout
//!
//! `zero_copy` requires the struct to be `Pod` — `repr(C)` with no padding
//! bytes. Fields are ordered by **descending alignment** (`u128` →
//! `Pubkey`/`u64` → small ints) with explicit trailing padding so the total
//! size is a multiple of 16. Do not reorder fields without re-checking the
//! layout test.
//!
//! Per-bin prices are not stored here; they are derived from `bin_step` and the
//! bin id via `zenith_math::bin_price`.

use anchor_lang::prelude::*;
use zenith_math::{bin_price, Q64x64, Rounding};

/// Lifecycle state of a pair, stored as a `u8`.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PairStatus {
    /// Not yet initialized.
    Uninitialized = 0,
    /// Open for swaps and liquidity changes.
    Active = 1,
    /// Frozen by the authority; no swaps.
    Disabled = 2,
}

impl PairStatus {
    /// Decode from the stored byte, defaulting unknown values to `Uninitialized`.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => PairStatus::Active,
            2 => PairStatus::Disabled,
            _ => PairStatus::Uninitialized,
        }
    }
}

/// Which token program a mint belongs to, stored as a `u8` flag on the pair.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenFlavor {
    /// Classic SPL Token program.
    SplToken = 0,
    /// Token-2022 (extensions program).
    Token2022 = 1,
}

impl TokenFlavor {
    /// Decode from the stored byte, defaulting unknown values to `SplToken`.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => TokenFlavor::Token2022,
            _ => TokenFlavor::SplToken,
        }
    }
}

#[account(zero_copy)]
#[repr(C)]
pub struct LbPair {
    // --- 16-byte aligned (u128) ---
    /// Volatility accumulator (bps): grows as the active bin moves from
    /// `index_reference`, decays when idle. Drives the variable fee.
    pub volatility_accumulator: u128,
    /// Decayed accumulator carried into the next volatility window (the base the
    /// next move adds onto).
    pub volatility_reference: u128,
    /// Reserved 16-byte slots (e.g. per-bin fee growth in a later issue).
    pub reserved_u128: [u128; 4],

    // --- 1-byte aligned (Pubkey = [u8; 32]) ---
    /// Token X mint (the canonically smaller of the two mints).
    pub token_x_mint: Pubkey,
    /// Token Y mint (the canonically larger of the two mints).
    pub token_y_mint: Pubkey,
    /// Reserve vault holding token X.
    pub reserve_x: Pubkey,
    /// Reserve vault holding token Y.
    pub reserve_y: Pubkey,
    /// Authority allowed to pause the pair and (in M4b) claim protocol fees.
    pub creator: Pubkey,

    // --- 8-byte aligned (u64) ---
    /// Protocol fees accrued in token X (raw token units). Populated in M4b.
    pub protocol_fee_x: u64,
    /// Protocol fees accrued in token Y (raw token units). Populated in M4b.
    pub protocol_fee_y: u64,
    /// Slot/timestamp at which the pair becomes tradable.
    pub activation_point: u64,
    /// Slot of the last volatility-state update.
    pub last_update_slot: u64,
    /// Reserved 8-byte slots for forward-compatible fields.
    pub reserved_u64: [u64; 5],

    // --- 4-byte aligned (i32 / u32) ---
    /// The bin currently holding the market price. Signed: bins extend in both
    /// directions from bin 0 (price 1.0).
    pub active_bin_id: i32,
    /// Reference bin the volatility move is measured from (re-set when a
    /// volatility window begins).
    pub index_reference: i32,
    /// Scales the variable fee: `variable = va^2 * control / 1e9`. Zero
    /// disables the dynamic fee.
    pub variable_fee_control: u32,
    /// Ceiling on the volatility accumulator (caps the surcharge).
    pub max_volatility_accumulator: u32,
    /// Slots within which the reference bin is NOT reset (high-frequency
    /// filter): rapid swaps accumulate against a stable reference.
    pub filter_period: u32,
    /// Slots after which an idle pair's volatility fully resets to zero.
    pub decay_period: u32,

    // --- 2-byte aligned (u16) ---
    /// Per-bin price spacing in basis points: adjacent bins differ in price by
    /// a factor of `1 + bin_step/10000`.
    pub bin_step: u16,
    /// Base swap fee in basis points (the constant floor; the live fee adds the
    /// volatility surcharge on top).
    pub base_fee_bps: u16,
    /// Fraction (bps) the accumulator decays to between the filter and decay
    /// windows.
    pub volatility_reduction_factor: u16,
    /// Hard cap on the variable surcharge, bps.
    pub max_dynamic_fee_bps: u16,
    /// Protocol's share of each swap fee, in basis points (the rest is the LP
    /// share). Claimed by the pair authority via `claim_protocol_fee`.
    pub protocol_fee_rate: u16,

    // --- 1-byte ---
    /// Lifecycle status (see [`PairStatus`]).
    pub status: u8,
    /// Bump for the pair authority PDA (signs for the reserves).
    pub pair_authority_bump: u8,
    /// Bump for the pair account's own PDA (cached for the swap hot path).
    pub pair_bump: u8,
    /// Bump for the token X reserve PDA.
    pub reserve_x_bump: u8,
    /// Bump for the token Y reserve PDA.
    pub reserve_y_bump: u8,
    /// Token program flavor for mint X: 0 = SPL Token, 1 = Token-2022.
    pub token_x_flag: u8,
    /// Token program flavor for mint Y: 0 = SPL Token, 1 = Token-2022.
    pub token_y_flag: u8,
    /// Trailing padding to keep the struct 16-byte sized (no Pod padding).
    pub padding: [u8; 15],
}

impl LbPair {
    /// On-chain byte length including the 8-byte account discriminator.
    pub const LEN: usize = 8 + core::mem::size_of::<LbPair>();

    /// Decoded lifecycle status.
    pub fn status(&self) -> PairStatus {
        PairStatus::from_u8(self.status)
    }

    /// `true` if the pair is open for swaps.
    pub fn is_active(&self) -> bool {
        self.status() == PairStatus::Active
    }

    /// Price of `bin_id` for this pair: `(1 + bin_step/10000)^bin_id` in
    /// Q64.64. `None` if `bin_step` is invalid or the id falls outside the
    /// supported price band (see [`zenith_math::bin_price`]).
    pub fn bin_price(&self, bin_id: i32, rounding: Rounding) -> Option<Q64x64> {
        bin_price(self.bin_step, bin_id, rounding)
    }

    /// Price of the currently active bin.
    pub fn active_bin_price(&self, rounding: Rounding) -> Option<Q64x64> {
        self.bin_price(self.active_bin_id, rounding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair_with_step(bin_step: u16, active_bin_id: i32) -> LbPair {
        let mut pair: LbPair = bytemuck::Zeroable::zeroed();
        pair.bin_step = bin_step;
        pair.active_bin_id = active_bin_id;
        pair
    }

    /// Q64.64 raw bits -> f64, for comparison against a floating reference.
    fn to_f64(p: Q64x64) -> f64 {
        p.to_bits() as f64 / 2f64.powi(64)
    }

    #[test]
    fn bin_price_matches_floating_reference_across_ids() {
        // Independent off-chain reference: price(id) = (1 + step/1e4)^id.
        for &step in &[1u16, 10, 25, 100, 1_000] {
            let pair = pair_with_step(step, 0);
            let base = 1.0 + step as f64 / 10_000.0;
            for id in -50i32..=50 {
                let got = to_f64(pair.bin_price(id, Rounding::Down).unwrap());
                let want = base.powi(id);
                let rel = (got - want).abs() / want;
                assert!(
                    rel < 1e-9,
                    "step {step} id {id}: got {got} want {want} rel {rel}"
                );
            }
        }
    }

    #[test]
    fn bin_zero_is_one_and_prices_are_monotonic() {
        let pair = pair_with_step(25, 7);
        assert_eq!(pair.bin_price(0, Rounding::Down).unwrap(), Q64x64::ONE);
        // strictly increasing in bin id
        let mut prev = pair.bin_price(-30, Rounding::Down).unwrap().to_bits();
        for id in -29i32..=30 {
            let cur = pair.bin_price(id, Rounding::Down).unwrap().to_bits();
            assert!(cur > prev, "not increasing at id {id}");
            prev = cur;
        }
        // active_bin_price reflects active_bin_id (7)
        assert_eq!(
            pair.active_bin_price(Rounding::Down),
            pair.bin_price(7, Rounding::Down)
        );
    }

    #[test]
    fn bin_price_none_outside_band() {
        let pair = pair_with_step(10_000, 0); // base 2 -> band |id| <= 32
        assert!(pair.bin_price(33, Rounding::Down).is_none());
        // invalid bin step yields no price
        let bad = pair_with_step(0, 0);
        assert!(bad.bin_price(0, Rounding::Down).is_none());
    }
}
