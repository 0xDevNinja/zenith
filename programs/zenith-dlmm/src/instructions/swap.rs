//! `swap` — trade across bins with active-bin crossing.
//!
//! Inside a bin the price is fixed (constant-sum, zero slippage). The trade
//! fills the active bin, and when that bin's output reserve drains it crosses to
//! the next bin — downward for `XtoY` (price falls), upward for `YtoX` (price
//! rises) — walking across bin-array boundaries until the order is filled. The
//! bin arrays the walk needs are passed as `remaining_accounts`.
//!
//! The fee is `base + variable` (volatility) on the input. It is split by the
//! pair's `protocol_fee_rate`: the protocol share accrues to `protocol_fee_x/y`
//! (claimable by the authority) and the LP share is spread back over the bins
//! the swap traded through. Per-bin *claimable* fee growth and a partner split
//! land in later M4b issues. Output rounds down; input rounds up.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};
use zenith_math::{bin_price, mul_div, Rounding};

use crate::constants::PAIR_AUTHORITY_SEED;
use crate::errors::DlmmError;
use crate::events::Swap as SwapEvent;
use crate::state::{BinArray, LbPair};
use crate::swap_math::{fill_exact_in, fill_exact_out, Direction, SwapMode};

const BPS_DENOMINATOR: u128 = 10_000;

#[derive(Accounts)]
pub struct Swap<'info> {
    pub trader: Signer<'info>,

    #[account(mut)]
    pub lb_pair: AccountLoader<'info, LbPair>,

    /// CHECK: PDA that owns the reserves; signs the output payout.
    #[account(seeds = [PAIR_AUTHORITY_SEED, lb_pair.key().as_ref()], bump)]
    pub pair_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub reserve_x: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub reserve_y: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = trader)]
    pub user_token_x: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = trader)]
    pub user_token_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    // remaining_accounts: the BinArray accounts the walk crosses (mut).
}

/// Execute a swap. `amount` is the input for `ExactIn` or the desired output for
/// `ExactOut`; `other_amount_threshold` is the min output (ExactIn) or max input
/// (ExactOut). Reverts if the pair lacks the liquidity to fill the order.
pub fn swap<'info>(
    ctx: Context<'_, '_, 'info, 'info, Swap<'info>>,
    direction: u8,
    mode: u8,
    amount: u64,
    other_amount_threshold: u64,
) -> Result<()> {
    let dir = Direction::from_u8(direction).ok_or(DlmmError::InvalidSwapParams)?;
    let swap_mode = SwapMode::from_u8(mode).ok_or(DlmmError::InvalidSwapParams)?;
    require!(amount > 0, DlmmError::ZeroAmount);

    let lb_pair_key = ctx.accounts.lb_pair.key();

    let now = Clock::get()?.slot;

    // --- read the pair, verify reserves, derive the fee for this swap ---
    // The variable fee is computed on the PRE-swap active bin (this swap pays
    // for volatility built by prior trades; its own bin movement surcharges the
    // next swap), so there is no circular dependency with the swap output.
    let (active_bin_id, bin_step, total_fee_bps, protocol_fee_rate, fee_state) = {
        let pair = ctx.accounts.lb_pair.load()?;
        require!(pair.is_active(), DlmmError::PairNotActive);
        require_keys_eq!(
            ctx.accounts.reserve_x.key(),
            pair.reserve_x,
            DlmmError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.reserve_y.key(),
            pair.reserve_y,
            DlmmError::Unauthorized
        );
        let elapsed = now.saturating_sub(pair.last_update_slot);
        let state = crate::fee::compute_variable_fee(
            pair.active_bin_id,
            pair.index_reference,
            pair.volatility_accumulator,
            pair.volatility_reference,
            elapsed,
            pair.filter_period,
            pair.decay_period,
            pair.volatility_reduction_factor,
            pair.max_volatility_accumulator,
            pair.bin_step,
            pair.variable_fee_control,
            pair.max_dynamic_fee_bps,
        );
        let total = crate::fee::total_fee_bps(pair.base_fee_bps, state.variable_fee_bps);
        (
            pair.active_bin_id,
            pair.bin_step,
            total as u128,
            pair.protocol_fee_rate,
            state,
        )
    };

    // Collect and validate the bin arrays passed as remaining accounts.
    let bin_arrays: Vec<AccountLoader<'info, BinArray>> = ctx
        .remaining_accounts
        .iter()
        .map(AccountLoader::try_from)
        .collect::<Result<_>>()?;
    require!(!bin_arrays.is_empty(), DlmmError::InsufficientLiquidity);
    for loader in &bin_arrays {
        let arr = loader.load()?;
        require_keys_eq!(arr.lb_pair, lb_pair_key, DlmmError::Unauthorized);
        // Pin each account to the canonical bin-array PDA for its index, so a
        // fabricated account with a spoofed `index`/reserves can't be injected.
        let (expected, _) = crate::pda::bin_array_pda(&lb_pair_key, arr.index);
        require_keys_eq!(loader.key(), expected, DlmmError::Unauthorized);
    }

    // For ExactIn the fee comes off the input up front; the net is what we route
    // through the bins.
    let (walk_budget, gross_in_known, fee_known) = match swap_mode {
        SwapMode::ExactIn => {
            let fee = mul_div(amount as u128, total_fee_bps, BPS_DENOMINATOR, Rounding::Up)
                .map_err(|_| DlmmError::MathOverflow)? as u64;
            let net = amount.checked_sub(fee).ok_or(DlmmError::MathOverflow)?;
            require!(net > 0, DlmmError::ZeroAmount);
            (net, Some(amount), Some(fee))
        }
        // ExactOut: the budget is the desired output; fee is computed from the
        // realized net input after the walk.
        SwapMode::ExactOut => (amount, None, None),
    };

    // --- walk the bins ---
    let mut budget = walk_budget;
    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut cur = active_bin_id;
    let mut new_active = active_bin_id;
    let mut bins_crossed: usize = 0;
    // Per-bin net input, used to spread the LP fee share back over the bins the
    // swap actually traded through.
    let mut bin_inputs: Vec<(i32, u64)> = Vec::new();

    'walk: while budget > 0 {
        let arr_index = BinArray::index_of(cur);
        let loader = bin_arrays
            .iter()
            .find(|l| l.load().map(|a| a.index == arr_index).unwrap_or(false))
            .ok_or(DlmmError::InsufficientLiquidity)?;
        let mut arr = loader.load_mut()?;

        while BinArray::index_of(cur) == arr_index {
            if budget == 0 {
                break 'walk;
            }
            // Bound total bins touched so a thin-liquidity walk fails cleanly
            // instead of silently exhausting the compute budget.
            bins_crossed += 1;
            require!(
                bins_crossed <= crate::constants::MAX_BINS_PER_SWAP,
                DlmmError::SwapTooManyBins
            );
            let price =
                bin_price(bin_step, cur, Rounding::Down).ok_or(DlmmError::BinIdOutOfRange)?;
            let slot = arr.slot_of(cur).ok_or(DlmmError::BinArrayIndexMismatch)?;
            let bin = &mut arr.bins[slot];
            let reserve_out = match dir {
                Direction::XtoY => bin.amount_y,
                Direction::YtoX => bin.amount_x,
            };

            let fill = match swap_mode {
                SwapMode::ExactIn => fill_exact_in(budget, reserve_out, price, dir),
                SwapMode::ExactOut => fill_exact_out(budget, reserve_out, price, dir),
            }
            .ok_or(DlmmError::MathOverflow)?;

            match dir {
                Direction::XtoY => {
                    bin.amount_y = bin
                        .amount_y
                        .checked_sub(fill.out)
                        .ok_or(DlmmError::MathOverflow)?;
                    bin.amount_x = bin
                        .amount_x
                        .checked_add(fill.in_used)
                        .ok_or(DlmmError::MathOverflow)?;
                }
                Direction::YtoX => {
                    bin.amount_x = bin
                        .amount_x
                        .checked_sub(fill.out)
                        .ok_or(DlmmError::MathOverflow)?;
                    bin.amount_y = bin
                        .amount_y
                        .checked_add(fill.in_used)
                        .ok_or(DlmmError::MathOverflow)?;
                }
            }

            if fill.in_used > 0 {
                bin_inputs.push((cur, fill.in_used));
            }
            total_in = total_in
                .checked_add(fill.in_used)
                .ok_or(DlmmError::MathOverflow)?;
            total_out = total_out
                .checked_add(fill.out)
                .ok_or(DlmmError::MathOverflow)?;
            budget -= match swap_mode {
                SwapMode::ExactIn => fill.in_used,
                SwapMode::ExactOut => fill.out,
            };

            if !fill.drained || budget == 0 {
                // Order filled — either inside this bin, or this drain finished
                // it. Stop on the current bin; only cross when work remains, so
                // exactly draining the band-edge bin doesn't spuriously revert
                // on an out-of-band cross. The active bin may sit on a just-
                // drained (empty) bin; the next swap crosses past it.
                new_active = cur;
                break 'walk;
            }

            // Bin drained with work left — cross to the next one (down for XtoY,
            // up for YtoX).
            cur = match dir {
                Direction::XtoY => cur.checked_sub(1).ok_or(DlmmError::BinIdOutOfRange)?,
                Direction::YtoX => cur.checked_add(1).ok_or(DlmmError::BinIdOutOfRange)?,
            };
            // The next bin must still be inside the supported price band.
            require!(
                bin_price(bin_step, cur, Rounding::Down).is_some(),
                DlmmError::InsufficientLiquidity
            );
            new_active = cur;
        }
    }

    // The order must be fully filled.
    require!(budget == 0, DlmmError::InsufficientLiquidity);
    require!(
        total_out > 0 && total_in > 0,
        DlmmError::InsufficientLiquidity
    );

    // Settle fee + slippage per mode.
    let (gross_in, amount_out, fee) = match swap_mode {
        SwapMode::ExactIn => {
            let gross = gross_in_known.unwrap();
            let fee = fee_known.unwrap();
            // net consumed must equal what we budgeted.
            require!(total_in == walk_budget, DlmmError::InsufficientLiquidity);
            require!(
                total_out >= other_amount_threshold,
                DlmmError::SlippageExceeded
            );
            (gross, total_out, fee)
        }
        SwapMode::ExactOut => {
            require!(total_out == amount, DlmmError::InsufficientLiquidity);
            // gross = net / (1 - rate); fee taken on top of the net input.
            let fee = mul_div(
                total_in as u128,
                total_fee_bps,
                BPS_DENOMINATOR - total_fee_bps,
                Rounding::Up,
            )
            .map_err(|_| DlmmError::MathOverflow)? as u64;
            let gross = total_in.checked_add(fee).ok_or(DlmmError::MathOverflow)?;
            require!(gross <= other_amount_threshold, DlmmError::SlippageExceeded);
            (gross, total_out, fee)
        }
    };

    // Split the fee: the protocol takes its rate, the rest is the LP share. The
    // LP share is spread back over the bins the swap traded through (in
    // proportion to the input each absorbed) so it compounds into those bins'
    // reserves and the LPs that hold them. Any rounding remainder accrues to the
    // protocol, so `protocol_accrued + lp_assigned == fee` exactly. (Per-bin
    // claimable fee growth — claiming without withdrawing — is #39.)
    let (protocol_share, lp_share) = crate::fee::split_protocol_fee(fee, protocol_fee_rate);
    let mut lp_assigned: u64 = 0;
    if lp_share > 0 {
        for (bin_id, net_in) in &bin_inputs {
            let lp_bin = mul_div(
                lp_share as u128,
                *net_in as u128,
                total_in as u128,
                Rounding::Down,
            )
            .map_err(|_| DlmmError::MathOverflow)? as u64;
            if lp_bin == 0 {
                continue;
            }
            let arr_index = BinArray::index_of(*bin_id);
            let loader = bin_arrays
                .iter()
                .find(|l| l.load().map(|a| a.index == arr_index).unwrap_or(false))
                .ok_or(DlmmError::BinArrayIndexMismatch)?;
            let mut arr = loader.load_mut()?;
            let slot = arr
                .slot_of(*bin_id)
                .ok_or(DlmmError::BinArrayIndexMismatch)?;
            let bin = &mut arr.bins[slot];
            // Accrue the fee as per-share growth (claimable via claim_fee), not
            // into the bin's tradable reserve — the lp_bin tokens stay in the
            // vault until claimed. The input token is X for XtoY, Y for YtoX.
            let delta = crate::fee::fee_growth_delta(lp_bin, bin.liquidity_supply);
            match dir {
                Direction::XtoY => bin.fee_growth_x = bin.fee_growth_x.wrapping_add(delta),
                Direction::YtoX => bin.fee_growth_y = bin.fee_growth_y.wrapping_add(delta),
            }
            lp_assigned = lp_assigned
                .checked_add(lp_bin)
                .ok_or(DlmmError::MathOverflow)?;
        }
    }
    // lp_assigned <= lp_share (each share floored, Σ net_in == total_in), but
    // use checked_sub so a future refactor can't silently wrap.
    let lp_remainder = lp_share
        .checked_sub(lp_assigned)
        .ok_or(DlmmError::MathOverflow)?;
    let protocol_accrued = protocol_share
        .checked_add(lp_remainder)
        .ok_or(DlmmError::MathOverflow)?;

    // --- write pair: active bin, volatility state, accrued protocol fee ---
    {
        let mut pair = ctx.accounts.lb_pair.load_mut()?;
        pair.active_bin_id = new_active;
        // Persist the (pre-swap-derived) volatility window; this swap's own bin
        // movement is folded in on the next swap via `index_reference`.
        pair.volatility_accumulator = fee_state.volatility_accumulator;
        pair.volatility_reference = fee_state.volatility_reference;
        pair.index_reference = fee_state.index_reference;
        pair.last_update_slot = now;
        match dir {
            Direction::XtoY => {
                pair.protocol_fee_x = pair
                    .protocol_fee_x
                    .checked_add(protocol_accrued)
                    .ok_or(DlmmError::MathOverflow)?
            }
            Direction::YtoX => {
                pair.protocol_fee_y = pair
                    .protocol_fee_y
                    .checked_add(protocol_accrued)
                    .ok_or(DlmmError::MathOverflow)?
            }
        }
    }

    // --- transfers: pull gross input, pay output (pair authority signs) ---
    let (user_in, reserve_in, reserve_out_acct, user_out) = match dir {
        Direction::XtoY => (
            &ctx.accounts.user_token_x,
            &ctx.accounts.reserve_x,
            &ctx.accounts.reserve_y,
            &ctx.accounts.user_token_y,
        ),
        Direction::YtoX => (
            &ctx.accounts.user_token_y,
            &ctx.accounts.reserve_y,
            &ctx.accounts.reserve_x,
            &ctx.accounts.user_token_x,
        ),
    };

    transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: user_in.to_account_info(),
                to: reserve_in.to_account_info(),
                authority: ctx.accounts.trader.to_account_info(),
            },
        ),
        gross_in,
    )?;

    let signer_seeds: &[&[&[u8]]] = &[&[
        PAIR_AUTHORITY_SEED,
        lb_pair_key.as_ref(),
        &[ctx.bumps.pair_authority],
    ]];
    transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: reserve_out_acct.to_account_info(),
                to: user_out.to_account_info(),
                authority: ctx.accounts.pair_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount_out,
    )?;

    emit!(SwapEvent {
        lb_pair: lb_pair_key,
        trader: ctx.accounts.trader.key(),
        direction,
        mode,
        amount_in: gross_in,
        amount_out,
        fee,
        active_bin_id: new_active,
        volatility_accumulator: fee_state.volatility_accumulator,
    });

    Ok(())
}
