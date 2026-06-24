//! Zenith AMM — concentrated-liquidity automated market maker.
//!
//! sqrt-price math over a fixed price band per pool; liquidity owned via
//! position NFTs (there is no fungible LP-token mint). Instruction handlers
//! land in M1/M1b — this crate currently defines the on-chain account model,
//! PDA derivation, and error set.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod math;
pub mod pda;
pub mod state;

pub use constants::*;
pub use errors::ZenithError;
pub use instructions::*;
pub use state::*;

// Program ID. The matching keypair lives in target/deploy/ (gitignored);
// run `anchor keys sync` after generating deploy keypairs to keep this and
// Anchor.toml aligned for an actual deploy.
declare_id!("AA8cKcHQj63GEHRaLrrT87W1efRZ44U147JTCXC2Rmkq");

#[program]
pub mod zenith_amm {
    use super::*;

    /// Create a reusable pool-creation config template.
    #[allow(clippy::too_many_arguments)]
    pub fn create_config(
        ctx: Context<CreateConfig>,
        index: u16,
        fee_authority: Pubkey,
        sqrt_min_price: u128,
        sqrt_max_price: u128,
        base_fee_bps: u16,
        protocol_fee_bps: u16,
    ) -> Result<()> {
        instructions::create_config(
            ctx,
            index,
            fee_authority,
            sqrt_min_price,
            sqrt_max_price,
            base_fee_bps,
            protocol_fee_bps,
        )
    }

    /// Create a pool from a config and open the creator's first position.
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        sqrt_price: u128,
        liquidity: u128,
        token_a_max: u64,
        token_b_max: u64,
    ) -> Result<()> {
        instructions::initialize_pool(ctx, sqrt_price, liquidity, token_a_max, token_b_max)
    }

    /// Open an empty liquidity position (mints its ownership NFT).
    pub fn create_position(ctx: Context<CreatePosition>) -> Result<()> {
        instructions::create_position(ctx)
    }

    // Remaining handlers land in later M1 issues:
    // add_liquidity, remove_liquidity, swap, claim_position_fee.
}
