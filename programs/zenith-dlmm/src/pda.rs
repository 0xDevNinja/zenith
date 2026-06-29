//! PDA derivation.
//!
//! All program-owned accounts are Program Derived Addresses: their address is a
//! deterministic hash of seeds + the program id, and no private key exists for
//! them, so only this program can authorize changes. These helpers are the
//! canonical derivations the SDK mirrors.

use anchor_lang::prelude::*;

use crate::constants::*;

/// Canonical ordering of a pair's two mints, so the pair PDA does not depend on
/// the order the caller passes them.
pub fn sort_mints<'a>(a: &'a Pubkey, b: &'a Pubkey) -> (&'a Pubkey, &'a Pubkey) {
    if a.to_bytes() <= b.to_bytes() {
        (a, b)
    } else {
        (b, a)
    }
}

/// LbPair PDA for a (unordered) mint pair and bin step.
///
/// Mints are sorted to ascending order before hashing, yielding one canonical
/// address per (unordered pair, bin step). The bin step is part of the seed so
/// the same two tokens can have several pairs at different price granularity.
pub fn lb_pair_pda(mint_a: &Pubkey, mint_b: &Pubkey, bin_step: u16) -> (Pubkey, u8) {
    let (m0, m1) = sort_mints(mint_a, mint_b);
    Pubkey::find_program_address(
        &[
            LB_PAIR_SEED,
            m0.as_ref(),
            m1.as_ref(),
            &bin_step.to_le_bytes(),
        ],
        &crate::ID,
    )
}

/// Pair authority PDA — signs for the pair's token reserves.
pub fn pair_authority_pda(lb_pair: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PAIR_AUTHORITY_SEED, lb_pair.as_ref()], &crate::ID)
}

/// Reserve (vault) PDA for a pair + the mint it holds.
pub fn reserve_pda(lb_pair: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[RESERVE_SEED, lb_pair.as_ref(), mint.as_ref()], &crate::ID)
}

/// BinArray PDA for a pair + signed array index.
pub fn bin_array_pda(lb_pair: &Pubkey, index: i64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[BIN_ARRAY_SEED, lb_pair.as_ref(), &index.to_le_bytes()],
        &crate::ID,
    )
}

/// Position PDA for a caller-supplied base pubkey (the position's unique id).
pub fn position_pda(base: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POSITION_SEED, base.as_ref()], &crate::ID)
}

/// Oracle (TWAP ring buffer) PDA for a pair.
pub fn oracle_pda(lb_pair: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ORACLE_SEED, lb_pair.as_ref()], &crate::ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdas_are_deterministic() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        assert_eq!(lb_pair_pda(&a, &b, 25), lb_pair_pda(&a, &b, 25));
        let pair = lb_pair_pda(&a, &b, 25).0;
        assert_eq!(pair_authority_pda(&pair), pair_authority_pda(&pair));
        assert_eq!(reserve_pda(&pair, &a), reserve_pda(&pair, &a));
        assert_eq!(bin_array_pda(&pair, -3), bin_array_pda(&pair, -3));
        let base = Pubkey::new_unique();
        assert_eq!(position_pda(&base), position_pda(&base));
    }

    #[test]
    fn lb_pair_pda_is_mint_order_independent() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        assert_eq!(lb_pair_pda(&a, &b, 10), lb_pair_pda(&b, &a, 10));
    }

    #[test]
    fn bin_step_distinguishes_pairs() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        assert_ne!(lb_pair_pda(&a, &b, 10).0, lb_pair_pda(&a, &b, 25).0);
    }

    #[test]
    fn distinct_seeds_give_distinct_pdas() {
        let pair = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        // reserve and authority derive from different seeds -> different addresses
        assert_ne!(reserve_pda(&pair, &mint).0, pair_authority_pda(&pair).0);
        // signed array index participates in the seed
        assert_ne!(bin_array_pda(&pair, 0).0, bin_array_pda(&pair, -1).0);
        assert_ne!(bin_array_pda(&pair, 1).0, bin_array_pda(&pair, -1).0);
    }

    #[test]
    fn reserves_differ_per_mint() {
        let pair = Pubkey::new_unique();
        let x = Pubkey::new_unique();
        let y = Pubkey::new_unique();
        assert_ne!(reserve_pda(&pair, &x).0, reserve_pda(&pair, &y).0);
    }
}
