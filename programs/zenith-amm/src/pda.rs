//! PDA derivation.
//!
//! All program-owned accounts are Program Derived Addresses: their address is a
//! deterministic hash of seeds + the program id, and no private key exists for
//! them, so only this program can authorize changes. These helpers are the
//! canonical derivations the SDK mirrors.

use anchor_lang::prelude::*;

use crate::constants::*;

/// Canonical ordering of a pool's two mints, so the pool PDA does not depend on
/// the order the caller passes them.
pub fn sort_mints<'a>(a: &'a Pubkey, b: &'a Pubkey) -> (&'a Pubkey, &'a Pubkey) {
    if a.to_bytes() <= b.to_bytes() {
        (a, b)
    } else {
        (b, a)
    }
}

/// Config PDA for a given index.
pub fn config_pda(index: u16) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG_SEED, &index.to_le_bytes()], &crate::ID)
}

/// Pool PDA for a config and its (unordered) token mints.
pub fn pool_pda(config: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey) -> (Pubkey, u8) {
    let (m0, m1) = sort_mints(mint_a, mint_b);
    Pubkey::find_program_address(
        &[POOL_SEED, config.as_ref(), m0.as_ref(), m1.as_ref()],
        &crate::ID,
    )
}

/// Pool authority PDA — signs for the pool's token vaults.
pub fn pool_authority_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POOL_AUTHORITY_SEED, pool.as_ref()], &crate::ID)
}

/// Token vault PDA for a pool + the mint it holds.
pub fn vault_pda(pool: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, pool.as_ref(), mint.as_ref()], &crate::ID)
}

/// Position PDA for a position-NFT mint.
pub fn position_pda(nft_mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POSITION_SEED, nft_mint.as_ref()], &crate::ID)
}

/// Position-NFT custody PDA (token account holding the NFT).
pub fn position_nft_custody_pda(nft_mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POSITION_NFT_SEED, nft_mint.as_ref()], &crate::ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdas_are_deterministic() {
        let config = Pubkey::new_unique();
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        assert_eq!(pool_pda(&config, &a, &b), pool_pda(&config, &a, &b));
        let pool = pool_pda(&config, &a, &b).0;
        assert_eq!(pool_authority_pda(&pool), pool_authority_pda(&pool));
        assert_eq!(vault_pda(&pool, &a), vault_pda(&pool, &a));
        assert_eq!(config_pda(3), config_pda(3));
    }

    #[test]
    fn pool_pda_is_mint_order_independent() {
        let config = Pubkey::new_unique();
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        assert_eq!(pool_pda(&config, &a, &b), pool_pda(&config, &b, &a));
    }

    #[test]
    fn distinct_seeds_give_distinct_pdas() {
        let pool = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        // vault and authority derive from different seeds -> different addresses
        assert_ne!(vault_pda(&pool, &mint).0, pool_authority_pda(&pool).0);
        // different config index -> different config PDA
        assert_ne!(config_pda(1).0, config_pda(2).0);
    }
}
