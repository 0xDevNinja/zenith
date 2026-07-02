//! Constant-product pool state.
//!
//! `Pool` is the central account for a full-range `x*y=k` market: the two token
//! mints, the reserve vaults, the fungible LP mint, the tracked curve reserves,
//! and the accrued protocol fees. It is read/written on every swap and
//! liquidity change, so it is `zero_copy`: cast directly from the account bytes
//! and mutated in place.
//!
//! ## Layout
//!
//! `zero_copy` requires the struct to be `Pod` — `repr(C)` with no padding
//! bytes. Fields are ordered by **descending alignment** (`u128` → `Pubkey`/
//! `u64` → small ints) with explicit trailing padding so the total size is a
//! multiple of 16. Do not reorder fields without re-checking the layout test.
//!
//! ## Reserves vs vault balance
//!
//! `reserve_a`/`reserve_b` are the *curve* reserves — what LP shares are backed
//! by and what the `x*y=k` math uses. The physical vault balance is
//! `reserve + protocol_fee` (and, once the yield engine lands, plus principal on
//! loan to the mock vault). Accrued protocol fees sit in the vault but are
//! tracked separately so they are never treated as tradable liquidity.
//!
//! The `reserved_*` slots hold room for the idle-reserve yield fields (deposited
//! principal, accrued-yield accumulators, buffer config, last-accrual slot) that
//! the yield engine adds without changing this account's size.

use anchor_lang::prelude::*;

/// Lifecycle state of a pool, stored as a `u8`.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PoolStatus {
    /// Not yet initialized.
    Uninitialized = 0,
    /// Open for swaps and liquidity changes.
    Active = 1,
    /// Frozen by the authority; no swaps.
    Disabled = 2,
}

impl PoolStatus {
    /// Decode from the stored byte, defaulting unknown values to `Uninitialized`.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => PoolStatus::Active,
            2 => PoolStatus::Disabled,
            _ => PoolStatus::Uninitialized,
        }
    }
}

/// Which token program a mint belongs to, stored as a `u8` flag on the pool.
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
pub struct Pool {
    // --- 16-byte aligned (u128) ---
    /// Reserved 16-byte slots for the idle-reserve yield engine (yield-growth
    /// accumulators) and forward-compatible fields.
    pub reserved_u128: [u128; 4],

    // --- 1-byte aligned (Pubkey = [u8; 32]) ---
    /// Token A mint (the canonically smaller of the two mints).
    pub token_a_mint: Pubkey,
    /// Token B mint (the canonically larger of the two mints).
    pub token_b_mint: Pubkey,
    /// Reserve vault holding token A.
    pub reserve_a_vault: Pubkey,
    /// Reserve vault holding token B.
    pub reserve_b_vault: Pubkey,
    /// Fungible LP-share mint (mint authority is the pool authority PDA).
    pub lp_mint: Pubkey,
    /// Token account permanently holding the locked minimum liquidity.
    pub locked_lp: Pubkey,
    /// Authority allowed to pause the pool and claim protocol fees.
    pub creator: Pubkey,

    // --- 8-byte aligned (u64) ---
    /// Curve reserve of token A (backs LP shares; used by `x*y=k`).
    pub reserve_a: u64,
    /// Curve reserve of token B.
    pub reserve_b: u64,
    /// Protocol fees accrued in token A (raw units; sits in the vault, untracked
    /// as liquidity).
    pub protocol_fee_a: u64,
    /// Protocol fees accrued in token B.
    pub protocol_fee_b: u64,
    /// Slot at which the pool was created.
    pub activation_point: u64,
    /// Principal of token A marked as deployed to the yield vault. The tokens
    /// stay physically in the reserve vault (so swaps are always solvent); this
    /// is the base the per-slot yield accrues on. Set by `rebalance_to_vault`.
    pub deployed_a: u64,
    /// Principal of token B marked as deployed to the yield vault.
    pub deployed_b: u64,
    /// Slot of the last yield accrual (harvest or rebalance).
    pub last_accrual_slot: u64,
    /// Mock lending rate: yield per deployed unit per slot, scaled by
    /// [`crate::constants::YIELD_SCALE`]. Zero means the yield engine is off.
    pub yield_rate: u64,
    /// Fraction (bps) of each reserve kept as a swap-solvency buffer, never
    /// counted as deployed principal.
    pub buffer_bps: u64,
    /// Reserved 8-byte slot for forward-compatible fields.
    pub reserved_u64: [u64; 1],

    // --- 2-byte aligned (u16) ---
    /// Base swap fee in basis points, taken from the input amount.
    pub base_fee_bps: u16,
    /// Protocol's share of each swap fee, in basis points (the rest stays in the
    /// reserve as the LP share, compounding into `k`).
    pub protocol_fee_rate: u16,

    // --- 1-byte ---
    /// Lifecycle status (see [`PoolStatus`]).
    pub status: u8,
    /// Bump for the pool authority PDA (signs for reserves + LP mint).
    pub pool_authority_bump: u8,
    /// Bump for the token A reserve PDA.
    pub reserve_a_bump: u8,
    /// Bump for the token B reserve PDA.
    pub reserve_b_bump: u8,
    /// Bump for the LP mint PDA.
    pub lp_mint_bump: u8,
    /// Bump for the locked-LP token account PDA.
    pub locked_lp_bump: u8,
    /// Token program flavor for mint A: 0 = SPL Token, 1 = Token-2022.
    pub token_a_flag: u8,
    /// Token program flavor for mint B: 0 = SPL Token, 1 = Token-2022.
    pub token_b_flag: u8,
    /// Trailing padding to keep the struct 16-byte sized (no Pod padding).
    pub padding: [u8; 12],
}

impl Pool {
    /// On-chain byte length including the 8-byte account discriminator.
    pub const LEN: usize = 8 + core::mem::size_of::<Pool>();

    /// Decoded lifecycle status.
    pub fn status(&self) -> PoolStatus {
        PoolStatus::from_u8(self.status)
    }

    /// `true` if the pool is open for swaps.
    pub fn is_active(&self) -> bool {
        self.status() == PoolStatus::Active
    }

    /// `true` if the idle-reserve yield engine has been configured.
    pub fn yield_enabled(&self) -> bool {
        self.yield_rate > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_pod_and_16_byte_multiple() {
        // Pod requires no internal padding; the size must be a multiple of the
        // 16-byte (u128) alignment. Pinned to 400 so a field change that shifts
        // the layout (and would misread existing pools) fails the build.
        assert_eq!(core::mem::size_of::<Pool>(), 400);
        assert_eq!(core::mem::size_of::<Pool>() % 16, 0);
        assert_eq!(core::mem::align_of::<Pool>(), 16);
        // A zeroed pool is Uninitialized and inactive.
        let pool: Pool = bytemuck::Zeroable::zeroed();
        assert_eq!(pool.status(), PoolStatus::Uninitialized);
        assert!(!pool.is_active());
    }

    #[test]
    fn status_and_flavor_decode() {
        assert_eq!(PoolStatus::from_u8(1), PoolStatus::Active);
        assert_eq!(PoolStatus::from_u8(2), PoolStatus::Disabled);
        assert_eq!(PoolStatus::from_u8(0), PoolStatus::Uninitialized);
        assert_eq!(PoolStatus::from_u8(9), PoolStatus::Uninitialized);
        assert_eq!(TokenFlavor::from_u8(0), TokenFlavor::SplToken);
        assert_eq!(TokenFlavor::from_u8(1), TokenFlavor::Token2022);
        assert_eq!(TokenFlavor::from_u8(9), TokenFlavor::SplToken);
    }
}
