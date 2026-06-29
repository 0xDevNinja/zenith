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
    /// Reserved 16-byte slots for M4b dynamic-fee accumulators / per-token fee
    /// growth (Q64.64). Zero today.
    pub reserved_u128: [u128; 6],

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
    /// Reserved 8-byte slots for forward-compatible fields.
    pub reserved_u64: [u64; 6],

    // --- 4-byte aligned (i32) ---
    /// The bin currently holding the market price. Signed: bins extend in both
    /// directions from bin 0 (price 1.0).
    pub active_bin_id: i32,

    // --- 2-byte aligned (u16) ---
    /// Per-bin price spacing in basis points: adjacent bins differ in price by
    /// a factor of `1 + bin_step/10000`.
    pub bin_step: u16,
    /// Snapshot of the base swap fee in basis points (the live fee math lands
    /// in M4b).
    pub base_fee_bps: u16,

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
    pub padding: [u8; 9],
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
}
