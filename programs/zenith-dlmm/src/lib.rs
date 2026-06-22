//! Zenith DLMM — liquidity-book dynamic market maker.
//!
//! Liquidity in discrete price bins (constant-sum per bin, zero in-bin
//! slippage); volatility-based dynamic fee. Handlers land in M4/M4b.

use anchor_lang::prelude::*;

// Placeholder program ID — regenerate with `anchor keys sync`.
declare_id!("Zen1DLMMpLACEHODLERpLACEHODLERpLACEHODLER11");

#[program]
pub mod zenith_dlmm {
    use super::*;

    // TODO(M4): initialize_lb_pair, initialize_bin_array, add_liquidity_by_strategy,
    // remove_liquidity, swap, claim_fee, limit orders, ...
}
