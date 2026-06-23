//! Zenith AMM — concentrated-liquidity automated market maker.
//!
//! sqrt-price math over a fixed price band per pool; liquidity owned via
//! position NFTs (there is no fungible LP-token mint). Instruction handlers
//! land in M1/M1b — this crate currently defines the on-chain account model,
//! PDA derivation, and error set.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod pda;
pub mod state;

pub use constants::*;
pub use errors::ZenithError;
pub use state::*;

// Program ID. The matching keypair lives in target/deploy/ (gitignored);
// run `anchor keys sync` after generating deploy keypairs to keep this and
// Anchor.toml aligned for an actual deploy.
declare_id!("AA8cKcHQj63GEHRaLrrT87W1efRZ44U147JTCXC2Rmkq");

#[program]
pub mod zenith_amm {
    // Handlers land in later M1 issues:
    // initialize_pool, create_position, add_liquidity, remove_liquidity,
    // swap, claim_position_fee, ...
}
