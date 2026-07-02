//! Pool state.
//!
//! `Pool` is the largest account and is read/written on every swap and
//! liquidity change, so it is `zero_copy`: it is cast directly from the account
//! bytes (no per-instruction deserialization) and mutated in place.
//!
//! ## Layout
//!
//! `zero_copy` requires the struct to be `Pod` — `repr(C)` with no padding
//! bytes. Fields are therefore ordered by **descending alignment** (`u128` →
//! `Pubkey`/`u64` → small ints) with explicit trailing padding so the total
//! size is a multiple of 16. Do not reorder fields without re-checking the
//! layout test.
//!
//! Prices and fee accumulators are stored as raw Q64.64 bits (`u128`); convert
//! with `zenith_math::Q64x64::from_bits` when computing.

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
    /// Active liquidity `L` at the current price.
    pub liquidity: u128,
    /// Current price (sqrt price, Q64.64 raw bits).
    pub sqrt_price: u128,
    /// Lower bound of the tradable band (sqrt price, Q64.64 raw bits).
    pub sqrt_min_price: u128,
    /// Upper bound of the tradable band (sqrt price, Q64.64 raw bits).
    pub sqrt_max_price: u128,
    /// Global fee growth per unit of liquidity, token A (Q64.64 raw bits).
    pub fee_growth_global_a: u128,
    /// Global fee growth per unit of liquidity, token B (Q64.64 raw bits).
    pub fee_growth_global_b: u128,
    /// Anchor price for the volatility window (sqrt price, Q64.64 raw bits).
    /// Price moves are measured relative to this; reset when the filter period
    /// elapses. Initialized to the pool's opening price.
    pub sqrt_price_reference: u128,
    /// Volatility accumulator: grows with price moves, decays over idle slots.
    pub volatility_accumulator: u128,
    /// Decayed accumulator carried into the next swap (the base each swap adds
    /// the new price move onto).
    pub volatility_reference: u128,
    /// Reserved 16-byte fields.
    pub reserved_u128: [u128; 1],

    // --- 1-byte aligned (Pubkey = [u8; 32]) ---
    /// Config this pool was created from.
    pub config: Pubkey,
    /// Token A mint.
    pub token_a_mint: Pubkey,
    /// Token B mint.
    pub token_b_mint: Pubkey,
    /// Token A vault (holds the pool's token A reserves).
    pub token_a_vault: Pubkey,
    /// Token B vault (holds the pool's token B reserves).
    pub token_b_vault: Pubkey,

    // --- 8-byte aligned (u64) ---
    /// Protocol fees accrued in token A (raw token units).
    pub protocol_fee_a: u64,
    /// Protocol fees accrued in token B (raw token units).
    pub protocol_fee_b: u64,
    /// Slot/timestamp at which the pool becomes tradable.
    pub activation_point: u64,
    /// Number of open positions (informational).
    pub position_count: u64,
    /// Slot of the last volatility-accumulator update.
    pub last_volatility_update: u64,
    /// Partner fees accrued in token A (carved from the protocol share).
    pub partner_fee_a: u64,
    /// Partner fees accrued in token B (carved from the protocol share).
    pub partner_fee_b: u64,
    /// Reserved 8-byte fields.
    pub reserved_u64: [u64; 5],

    // --- small ---
    /// Informational snapshot of the config's `base_fee_bps` at creation (the
    /// constant fee / decay floor). NOT the live swap fee: `swap` derives the
    /// current fee from the config's scheduler + `activation_point` each trade.
    pub base_fee_bps: u16,
    /// Tick spacing: only ticks that are multiples of this are usable as
    /// position bounds. Copied from the config at pool creation.
    pub tick_spacing: u16,
    /// Lifecycle status (see [`PoolStatus`]).
    pub status: u8,
    /// Bump for the pool authority PDA.
    pub pool_authority_bump: u8,
    /// Bump for the pool account's own PDA (cached to avoid re-derivation on
    /// the swap hot path).
    pub pool_bump: u8,
    /// Bump for the token A vault PDA.
    pub token_a_vault_bump: u8,
    /// Bump for the token B vault PDA.
    pub token_b_vault_bump: u8,
    /// Token program flavor for mint A: 0 = SPL Token, 1 = Token-2022.
    pub token_a_flags: u8,
    /// Token program flavor for mint B: 0 = SPL Token, 1 = Token-2022.
    pub token_b_flags: u8,
    /// Trailing padding to keep the struct 16-byte sized (no Pod padding).
    pub padding: [u8; 5],
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
}
