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

/// Volatility-fee control parameters supplied at pair creation.
pub struct DynamicFeeParams {
    /// Scales the variable fee (`va^2 * control / 1e9`); 0 disables it.
    pub variable_fee_control: u32,
    /// Ceiling on the volatility accumulator.
    pub max_volatility_accumulator: u32,
    /// High-frequency filter window (slots) — reference bin held fixed within.
    pub filter_period: u32,
    /// Idle window (slots) after which volatility fully resets.
    pub decay_period: u32,
    /// Fraction (bps) the accumulator decays to between filter and decay.
    pub volatility_reduction_factor: u16,
    /// Hard cap on the variable surcharge, bps.
    pub max_dynamic_fee_bps: u16,
}

/// Validate the dynamic-fee parameters. The surcharge cap must be in range, and
/// when the dynamic fee is enabled (`variable_fee_control != 0`) the windows and
/// accumulator cap must be sane — otherwise the fee would be silently dead
/// (e.g. `filter_period == 0` re-anchors every swap so volatility never builds,
/// or a zero cap clamps the accumulator to nothing). When disabled, the unused
/// window params may be anything.
pub fn validate_fee_params(fee: &DynamicFeeParams) -> Result<()> {
    require!(
        fee.max_dynamic_fee_bps < MAX_FEE_BPS,
        DlmmError::InvalidFeeConfig
    );
    if fee.variable_fee_control != 0 {
        require!(
            fee.filter_period > 0 && fee.filter_period <= fee.decay_period,
            DlmmError::InvalidFeeConfig
        );
        require!(
            fee.max_volatility_accumulator > 0,
            DlmmError::InvalidFeeConfig
        );
    }
    Ok(())
}

/// Create the pair and its reserve vaults.
pub fn initialize_lb_pair(
    ctx: Context<InitializeLbPair>,
    bin_step: u16,
    active_bin_id: i32,
    base_fee_bps: u16,
    protocol_fee_rate: u16,
    fee: DynamicFeeParams,
) -> Result<()> {
    // Canonical pair key requires ascending mint order (and rejects identical
    // mints), so the on-chain PDA — seeded with the mints in submitted order —
    // matches `pda::lb_pair_pda`, which sorts them to the same order.
    require!(
        ctx.accounts.token_x_mint.key() < ctx.accounts.token_y_mint.key(),
        DlmmError::IdenticalMints
    );
    validate_init_params(bin_step, active_bin_id, base_fee_bps)?;
    validate_fee_params(&fee)?;
    // The protocol's share of the fee may be up to 100% of it.
    require!(
        protocol_fee_rate <= MAX_FEE_BPS,
        DlmmError::InvalidFeeConfig
    );

    let active_bin_price = bin_price(bin_step, active_bin_id, Rounding::Down)
        .ok_or(DlmmError::BinIdOutOfRange)?
        .to_bits();

    let now = Clock::get()?.slot;
    let lb_pair_key = ctx.accounts.lb_pair.key();
    {
        let mut pair = ctx.accounts.lb_pair.load_init()?;
        // Volatility state starts calm, referenced at the opening bin.
        pair.volatility_accumulator = 0;
        pair.volatility_reference = 0;
        pair.reserved_u128 = [0u128; 4];
        pair.token_x_mint = ctx.accounts.token_x_mint.key();
        pair.token_y_mint = ctx.accounts.token_y_mint.key();
        pair.reserve_x = ctx.accounts.reserve_x.key();
        pair.reserve_y = ctx.accounts.reserve_y.key();
        pair.creator = ctx.accounts.creator.key();
        pair.protocol_fee_x = 0;
        pair.protocol_fee_y = 0;
        pair.activation_point = now;
        pair.last_update_slot = now;
        pair.reserved_u64 = [0u64; 5];
        pair.active_bin_id = active_bin_id;
        pair.index_reference = active_bin_id;
        pair.variable_fee_control = fee.variable_fee_control;
        pair.max_volatility_accumulator = fee.max_volatility_accumulator;
        pair.filter_period = fee.filter_period;
        pair.decay_period = fee.decay_period;
        pair.bin_step = bin_step;
        pair.base_fee_bps = base_fee_bps;
        pair.volatility_reduction_factor = fee.volatility_reduction_factor;
        pair.max_dynamic_fee_bps = fee.max_dynamic_fee_bps;
        pair.protocol_fee_rate = protocol_fee_rate;
        pair.status = PairStatus::Active as u8;
        pair.pair_authority_bump = ctx.bumps.pair_authority;
        pair.pair_bump = ctx.bumps.lb_pair;
        pair.reserve_x_bump = ctx.bumps.reserve_x;
        pair.reserve_y_bump = ctx.bumps.reserve_y;
        pair.token_x_flag = TokenFlavor::SplToken as u8;
        pair.token_y_flag = TokenFlavor::SplToken as u8;
        pair.padding = [0u8; 15];
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

    fn fee(control: u32, max_va: u32, filter: u32, decay: u32, max_dyn: u16) -> DynamicFeeParams {
        DynamicFeeParams {
            variable_fee_control: control,
            max_volatility_accumulator: max_va,
            filter_period: filter,
            decay_period: decay,
            volatility_reduction_factor: 5_000,
            max_dynamic_fee_bps: max_dyn,
        }
    }

    #[test]
    fn fee_params_validated() {
        // Healthy dynamic-fee config.
        assert!(validate_fee_params(&fee(1_000_000, 100_000, 10, 100, 1_000)).is_ok());
        // Dynamic disabled: window params may be anything.
        assert!(validate_fee_params(&fee(0, 0, 0, 0, 0)).is_ok());
        // Enabled but broken windows / cap are rejected.
        assert!(validate_fee_params(&fee(1_000_000, 100_000, 0, 100, 1_000)).is_err()); // filter 0
        assert!(validate_fee_params(&fee(1_000_000, 100_000, 200, 100, 1_000)).is_err()); // filter > decay
        assert!(validate_fee_params(&fee(1_000_000, 0, 10, 100, 1_000)).is_err()); // max_va 0
                                                                                   // Surcharge cap must be < 100% regardless.
        assert!(validate_fee_params(&fee(0, 0, 0, 0, MAX_FEE_BPS)).is_err());
    }

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
