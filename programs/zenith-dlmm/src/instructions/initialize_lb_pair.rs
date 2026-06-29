//! `initialize_lb_pair` — create an empty liquidity-book pair.
//!
//! Validates the mints, bin step, and active bin, then creates the pair account
//! and its two reserve vaults under the pair-authority PDA. Unlike the AMM, a
//! DLMM pair opens with no liquidity — providers seed bins afterwards via
//! `add_liquidity` (a later M4 issue).
//!
//! M4 scope is the classic SPL Token program (the pair records a `SplToken`
//! flavor for each mint); Token-2022 support lands later.

use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use zenith_math::{bin_price, Rounding, MAX_BIN_STEP_BPS};

use crate::constants::{LB_PAIR_SEED, PAIR_AUTHORITY_SEED, RESERVE_SEED};
use crate::errors::DlmmError;
use crate::events::LbPairInitialized;
use crate::state::{LbPair, PairStatus, TokenFlavor};

/// Largest fee a pair may set (basis points), exclusive. Mirrors the AMM cap.
pub const MAX_FEE_BPS: u16 = 10_000;

#[derive(Accounts)]
#[instruction(bin_step: u16)]
pub struct InitializeLbPair<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    pub token_x_mint: Box<Account<'info, Mint>>,
    pub token_y_mint: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = creator,
        space = LbPair::LEN,
        seeds = [
            LB_PAIR_SEED,
            token_x_mint.key().as_ref(),
            token_y_mint.key().as_ref(),
            &bin_step.to_le_bytes(),
        ],
        bump
    )]
    pub lb_pair: AccountLoader<'info, LbPair>,

    /// CHECK: PDA that owns the reserve vaults; holds no data.
    #[account(seeds = [PAIR_AUTHORITY_SEED, lb_pair.key().as_ref()], bump)]
    pub pair_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = creator,
        seeds = [RESERVE_SEED, lb_pair.key().as_ref(), token_x_mint.key().as_ref()],
        bump,
        token::mint = token_x_mint,
        token::authority = pair_authority,
    )]
    pub reserve_x: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = creator,
        seeds = [RESERVE_SEED, lb_pair.key().as_ref(), token_y_mint.key().as_ref()],
        bump,
        token::mint = token_y_mint,
        token::authority = pair_authority,
    )]
    pub reserve_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Validate the parameters a pair is created with.
///
/// Pure (no account access) so it can be unit-tested directly; the handler
/// calls it before touching any state. Rejects an invalid bin step, an active
/// bin outside the price band the step supports, and an out-of-range fee.
pub fn validate_init_params(bin_step: u16, active_bin_id: i32, base_fee_bps: u16) -> Result<()> {
    require!(
        bin_step > 0 && bin_step <= MAX_BIN_STEP_BPS,
        DlmmError::InvalidBinStep
    );
    // `bin_price` returns `None` exactly when the id leaves the supported band
    // for this step; reuse it as the bounds check so on-chain and off-chain
    // agree on the usable range.
    require!(
        bin_price(bin_step, active_bin_id, Rounding::Down).is_some(),
        DlmmError::BinIdOutOfRange
    );
    require!(base_fee_bps < MAX_FEE_BPS, DlmmError::InvalidFeeConfig);
    Ok(())
}

/// Create the pair and its reserve vaults.
pub fn initialize_lb_pair(
    ctx: Context<InitializeLbPair>,
    bin_step: u16,
    active_bin_id: i32,
    base_fee_bps: u16,
) -> Result<()> {
    // Canonical pair key requires ascending mint order (and rejects identical
    // mints), so the on-chain PDA — seeded with the mints in submitted order —
    // matches `pda::lb_pair_pda`, which sorts them to the same order.
    require!(
        ctx.accounts.token_x_mint.key() < ctx.accounts.token_y_mint.key(),
        DlmmError::IdenticalMints
    );
    validate_init_params(bin_step, active_bin_id, base_fee_bps)?;

    let active_bin_price = bin_price(bin_step, active_bin_id, Rounding::Down)
        .ok_or(DlmmError::BinIdOutOfRange)?
        .to_bits();

    let lb_pair_key = ctx.accounts.lb_pair.key();
    {
        let mut pair = ctx.accounts.lb_pair.load_init()?;
        pair.reserved_u128 = [0u128; 6];
        pair.token_x_mint = ctx.accounts.token_x_mint.key();
        pair.token_y_mint = ctx.accounts.token_y_mint.key();
        pair.reserve_x = ctx.accounts.reserve_x.key();
        pair.reserve_y = ctx.accounts.reserve_y.key();
        pair.creator = ctx.accounts.creator.key();
        pair.protocol_fee_x = 0;
        pair.protocol_fee_y = 0;
        pair.activation_point = Clock::get()?.slot;
        pair.reserved_u64 = [0u64; 6];
        pair.active_bin_id = active_bin_id;
        pair.bin_step = bin_step;
        pair.base_fee_bps = base_fee_bps;
        pair.status = PairStatus::Active as u8;
        pair.pair_authority_bump = ctx.bumps.pair_authority;
        pair.pair_bump = ctx.bumps.lb_pair;
        pair.reserve_x_bump = ctx.bumps.reserve_x;
        pair.reserve_y_bump = ctx.bumps.reserve_y;
        pair.token_x_flag = TokenFlavor::SplToken as u8;
        pair.token_y_flag = TokenFlavor::SplToken as u8;
        pair.padding = [0u8; 9];
    }

    emit!(LbPairInitialized {
        lb_pair: lb_pair_key,
        token_x_mint: ctx.accounts.token_x_mint.key(),
        token_y_mint: ctx.accounts.token_y_mint.key(),
        bin_step,
        active_bin_id,
        active_bin_price,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_and_oversized_bin_step() {
        assert!(validate_init_params(0, 0, 30).is_err());
        assert!(validate_init_params(MAX_BIN_STEP_BPS + 1, 0, 30).is_err());
        // boundary values are accepted
        assert!(validate_init_params(1, 0, 30).is_ok());
        assert!(validate_init_params(MAX_BIN_STEP_BPS, 0, 30).is_ok());
    }

    #[test]
    fn rejects_active_bin_outside_band() {
        // For step = 100% (base 2), the band is |id| <= 32 (price in [2^-32, 2^32]).
        assert!(validate_init_params(MAX_BIN_STEP_BPS, 32, 30).is_ok());
        assert!(validate_init_params(MAX_BIN_STEP_BPS, 33, 30).is_err());
        assert!(validate_init_params(MAX_BIN_STEP_BPS, -33, 30).is_err());
    }

    #[test]
    fn rejects_out_of_range_fee() {
        assert!(validate_init_params(25, 0, MAX_FEE_BPS).is_err());
        assert!(validate_init_params(25, 0, MAX_FEE_BPS - 1).is_ok());
    }
}
