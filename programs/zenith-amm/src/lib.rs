//! Zenith AMM — concentrated-liquidity automated market maker.
//!
//! sqrt-price math over a fixed price band per pool; liquidity owned via
//! position NFTs (no LP-token mint). Instruction handlers land in M1/M1b.

use anchor_lang::prelude::*;

// Placeholder program ID — regenerate with `anchor keys sync`.
declare_id!("Zen1AMMpLACEHODLERpLACEHODLERpLACEHODLER111");

#[program]
pub mod zenith_amm {
    use super::*;

    // TODO(M1): initialize_pool, create_position, add_liquidity,
    // remove_liquidity, swap, claim_position_fee, ...
}
