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

/// Pool PDA for a (unordered) mint pair. Mints are sorted to ascending order
/// before hashing, yielding one canonical address per unordered pair.
pub fn pool_pda(mint_a: &Pubkey, mint_b: &Pubkey) -> (Pubkey, u8) {
    let (m0, m1) = sort_mints(mint_a, mint_b);
    Pubkey::find_program_address(&[POOL_SEED, m0.as_ref(), m1.as_ref()], &crate::ID)
}

/// Pool authority PDA — signs for the reserves and the LP mint.
pub fn pool_authority_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POOL_AUTHORITY_SEED, pool.as_ref()], &crate::ID)
}

/// Reserve (vault) PDA for a pool + the mint it holds.
pub fn reserve_pda(pool: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[RESERVE_SEED, pool.as_ref(), mint.as_ref()], &crate::ID)
}

/// LP-share mint PDA for a pool.
pub fn lp_mint_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[LP_MINT_SEED, pool.as_ref()], &crate::ID)
}

/// Locked-liquidity token account PDA for a pool (holds the permanently locked
/// minimum liquidity minted on the first deposit).
pub fn locked_lp_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[LOCKED_LP_SEED, pool.as_ref()], &crate::ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdas_are_deterministic() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        assert_eq!(pool_pda(&a, &b), pool_pda(&a, &b));
        let pool = pool_pda(&a, &b).0;
        assert_eq!(pool_authority_pda(&pool), pool_authority_pda(&pool));
        assert_eq!(reserve_pda(&pool, &a), reserve_pda(&pool, &a));
        assert_eq!(lp_mint_pda(&pool), lp_mint_pda(&pool));
        assert_eq!(locked_lp_pda(&pool), locked_lp_pda(&pool));
    }

    #[test]
    fn pool_pda_is_mint_order_independent() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        assert_eq!(pool_pda(&a, &b), pool_pda(&b, &a));
    }

    #[test]
    fn distinct_seeds_give_distinct_pdas() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        let pool = pool_pda(&a, &b).0;
        // Every derived account for a pool has a distinct address.
        let addrs = [
            pool_authority_pda(&pool).0,
            reserve_pda(&pool, &a).0,
            reserve_pda(&pool, &b).0,
            lp_mint_pda(&pool).0,
            locked_lp_pda(&pool).0,
        ];
        for i in 0..addrs.len() {
            for j in (i + 1)..addrs.len() {
                assert_ne!(addrs[i], addrs[j], "addr {i} == addr {j}");
            }
        }
    }
}
