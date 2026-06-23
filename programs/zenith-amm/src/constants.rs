//! Seed constants for PDA derivation.
//!
//! Every program-owned account is a PDA so it has no external private key and
//! can only be mutated through this program's logic. The seed strings here are
//! the single source of truth shared by the on-chain handlers and the SDK.

/// Reusable pool-creation template, keyed by an index.
pub const CONFIG_SEED: &[u8] = b"config";
/// A liquidity pool, keyed by its config + the two (sorted) token mints.
pub const POOL_SEED: &[u8] = b"pool";
/// The pool authority that signs for the pool's token vaults.
pub const POOL_AUTHORITY_SEED: &[u8] = b"pool_authority";
/// A pool token vault, keyed by the pool + the token mint it holds.
pub const VAULT_SEED: &[u8] = b"vault";
/// A liquidity position, keyed by its position-NFT mint.
pub const POSITION_SEED: &[u8] = b"position";
/// Custody (token account) holding a position's NFT, keyed by the NFT mint.
pub const POSITION_NFT_SEED: &[u8] = b"position_nft";
