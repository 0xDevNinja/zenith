//! Zenith DLMM — liquidity-book dynamic market maker.
//!
//! Liquidity lives in discrete price bins (constant-sum per bin, zero in-bin
//! slippage); a swap walks bin to bin across the book. Volatility-based dynamic
//! fees land in M4b. Instruction handlers land in M4/M4b — this crate currently
//! defines the on-chain account model, PDA derivation, and error set.

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod pda;
pub mod state;

pub use constants::*;
pub use errors::DlmmError;
pub use state::*;

// Program ID. The matching keypair lives in target/deploy/ (gitignored);
// run `anchor keys sync` after generating deploy keypairs to keep this and
// Anchor.toml aligned for an actual deploy.
declare_id!("7pxn8tEm44gXjfPH9YXsLywuYpAbgbxq9nPwG1XQczsz");

#[program]
pub mod zenith_dlmm {
    // TODO(M4): initialize_lb_pair (+ bin-price wiring), initialize_bin_array,
    // initialize_position, add_liquidity_by_strategy, remove_liquidity, swap.
}
