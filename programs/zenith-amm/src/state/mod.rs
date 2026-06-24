//! On-chain account state for the AMM.

mod config;
mod pool;
mod position;

pub use config::Config;
pub use pool::{Pool, PoolStatus, TokenFlavor};
pub use position::Position;

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::*;

    #[test]
    fn pool_layout_is_pod_sound() {
        // 16-byte alignment (u128) and a size that is a multiple of it => no
        // compiler-inserted padding, which is what makes the zero_copy cast sound.
        assert_eq!(core::mem::align_of::<Pool>(), 16);
        assert_eq!(core::mem::size_of::<Pool>() % 16, 0);
        // Expected size from the documented layout (432 data bytes).
        assert_eq!(core::mem::size_of::<Pool>(), 432);
        assert_eq!(Pool::LEN, 8 + 432);
    }

    #[test]
    fn pool_zero_copy_round_trip() {
        let mut pool: Pool = bytemuck::Zeroable::zeroed();
        pool.liquidity = 123_456_789;
        pool.sqrt_price = 1u128 << 64;
        pool.sqrt_min_price = 1u128 << 32;
        pool.sqrt_max_price = 1u128 << 96;
        pool.fee_growth_global_a = 42;
        pool.base_fee_bps = 30;
        pool.status = PoolStatus::Active as u8;
        pool.pool_authority_bump = 254;
        pool.config = Pubkey::new_unique();
        pool.token_a_vault = Pubkey::new_unique();

        let bytes = bytemuck::bytes_of(&pool);
        let back: &Pool = bytemuck::from_bytes(bytes);
        assert_eq!(back.liquidity, 123_456_789);
        assert_eq!(back.sqrt_price, 1u128 << 64);
        assert_eq!(back.sqrt_max_price, 1u128 << 96);
        assert_eq!(back.base_fee_bps, 30);
        assert_eq!(back.status(), PoolStatus::Active);
        assert!(back.is_active());
        assert_eq!(back.config, pool.config);
    }

    // Note: zero padding is guaranteed at compile time — bytemuck's
    // `#[derive(Pod)]` (emitted by `#[account(zero_copy)]`) fails to compile if
    // the struct has any padding bytes. So the struct compiling IS the proof; a
    // runtime all-0xFF cast would additionally have to manage 16-byte alignment.

    #[test]
    fn status_and_flavor_decoding() {
        assert_eq!(PoolStatus::from_u8(0), PoolStatus::Uninitialized);
        assert_eq!(PoolStatus::from_u8(1), PoolStatus::Active);
        assert_eq!(PoolStatus::from_u8(2), PoolStatus::Disabled);
        assert_eq!(PoolStatus::from_u8(7), PoolStatus::Uninitialized); // unknown -> closed
        assert_eq!(TokenFlavor::from_u8(0), TokenFlavor::SplToken);
        assert_eq!(TokenFlavor::from_u8(1), TokenFlavor::Token2022);
        assert_eq!(TokenFlavor::from_u8(9), TokenFlavor::SplToken);
    }

    #[test]
    fn config_borsh_round_trip() {
        let c = Config {
            admin: Pubkey::new_unique(),
            fee_authority: Pubkey::new_unique(),
            sqrt_min_price: 1u128 << 32,
            sqrt_max_price: 1u128 << 96,
            fee_period: 0,
            index: 7,
            base_fee_bps: 25,
            protocol_fee_bps: 1000,
            cliff_fee_bps: 0,
            reduction_factor: 0,
            max_fee_steps: 0,
            fee_scheduler_mode: 0,
            bump: 251,
            reserved: [0u8; 48],
        };
        let bytes = c.try_to_vec().unwrap();
        let back = Config::try_from_slice(&bytes).unwrap();
        assert_eq!(back.index, 7);
        assert_eq!(back.base_fee_bps, 25);
        assert_eq!(back.sqrt_max_price, 1u128 << 96);
        assert_eq!(back.admin, c.admin);
    }

    #[test]
    fn position_borsh_round_trip_and_total() {
        let p = Position {
            pool: Pubkey::new_unique(),
            nft_mint: Pubkey::new_unique(),
            liquidity: 100,
            vested_liquidity: 20,
            permanent_locked_liquidity: 5,
            fee_growth_checkpoint_a: 9,
            fee_growth_checkpoint_b: 11,
            fee_pending_a: 1,
            fee_pending_b: 2,
            bump: 250,
            reserved: [0u8; 64],
        };
        let bytes = p.try_to_vec().unwrap();
        let back = Position::try_from_slice(&bytes).unwrap();
        assert_eq!(back.total_liquidity(), 125);
        assert_eq!(back.nft_mint, p.nft_mint);
    }
}
