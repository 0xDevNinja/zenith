//! Seed constants and fixed sizes for the constant-product engine.
//!
//! Every program-owned account is a PDA so it has no external private key and
//! can only be mutated through this program's logic. The seed strings here are
//! the single source of truth shared by the on-chain handlers and the SDK.

/// A constant-product pool, keyed by its (sorted) token mints.
pub const POOL_SEED: &[u8] = b"cp_pool";
/// The pool authority that signs for the reserves and the LP mint.
pub const POOL_AUTHORITY_SEED: &[u8] = b"cp_authority";
/// A pool token reserve (vault), keyed by the pool + the mint it holds.
pub const RESERVE_SEED: &[u8] = b"cp_reserve";
/// The fungible LP-share mint for a pool.
pub const LP_MINT_SEED: &[u8] = b"cp_lp_mint";
/// The token account that permanently holds the locked minimum liquidity.
pub const LOCKED_LP_SEED: &[u8] = b"cp_locked_lp";

/// Largest fee a pool may set (basis points), exclusive. Mirrors the other
/// engines' cap so a pool can never take the entire trade as fee.
pub const MAX_FEE_BPS: u16 = 10_000;

/// Decimals for the fungible LP-share mint. Share amounts are their own unit
/// (geometric mean of the deposited token amounts), so this is purely cosmetic
/// for wallets; 9 matches the Solana-native convention.
pub const LP_MINT_DECIMALS: u8 = 9;
