//! Seed constants and fixed sizes for the DLMM.
//!
//! Every program-owned account is a PDA so it has no external private key and
//! can only be mutated through this program's logic. The seed strings here are
//! the single source of truth shared by the on-chain handlers and the SDK.

/// A liquidity-book pair, keyed by its (sorted) token mints + bin step.
pub const LB_PAIR_SEED: &[u8] = b"lb_pair";
/// The pair authority that signs for the pair's token reserves.
pub const PAIR_AUTHORITY_SEED: &[u8] = b"pair_authority";
/// A pair token reserve (vault), keyed by the pair + the mint it holds.
pub const RESERVE_SEED: &[u8] = b"reserve";
/// A batched group of bins, keyed by the pair + its signed array index.
pub const BIN_ARRAY_SEED: &[u8] = b"bin_array";
/// A liquidity position, keyed by a caller-supplied base pubkey.
pub const POSITION_SEED: &[u8] = b"position";

/// Number of bins packed into a single [`crate::state::BinArray`] account.
///
/// Chosen so the account stays well under Solana's single-allocation limit (a
/// `BinArray` is ~4.5 KB, far below the 10 KB realloc bound and 10 MB cap), so
/// arrays are created in one `create_account` with no realloc.
pub const MAX_BINS_PER_ARRAY: usize = 70;

/// Maximum number of bins a single [`crate::state::Position`] can span. A
/// position's `[lower_bin_id, upper_bin_id]` range is inclusive and capped to
/// this width so its per-bin share array stays fixed-size.
pub const MAX_BINS_PER_POSITION: usize = 70;
