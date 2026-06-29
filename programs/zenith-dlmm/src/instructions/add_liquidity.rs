//! `add_liquidity_by_strategy` — deposit tokens across a position's bins.
//!
//! The caller supplies a token-X total, a token-Y total, and a [`Strategy`].
//! Bins below the active bin hold only Y, bins above hold only X, and the
//! active bin holds both, so X is distributed over the `[active, upper]` side
//! and Y over the `[lower, active]` side by the strategy weights. Each bin
//! mints LP shares against its constant-sum liquidity `L = price * x + y`:
//! the first deposit into a bin mints `L` shares, later deposits mint
//! `L_added * supply / L_before` (rounded down, so rounding never favors the
//! depositor).
//!
//! M4 scope: the position lies within a single bin array (enforced at
//! `initialize_position`), classic SPL Token only.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};
use zenith_math::{bin_price, mul_div, Rounding};

use crate::errors::DlmmError;
use crate::events::LiquidityAdded;
use crate::state::{BinArray, LbPair, Position};
use crate::strategy::{plan_deposit, PlanError, Strategy};

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    /// Position owner.
    pub owner: Signer<'info>,

    pub lb_pair: AccountLoader<'info, LbPair>,

    #[account(mut)]
    pub position: AccountLoader<'info, Position>,

    /// The bin array covering the position's range (single-array positions).
    #[account(mut)]
    pub bin_array: AccountLoader<'info, BinArray>,

    #[account(mut)]
    pub reserve_x: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub reserve_y: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_x: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = owner)]
    pub user_token_y: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

/// Add liquidity to `position`, distributing `amount_x` / `amount_y` across its
/// bins by `strategy`.
///
/// The active bin decides how the two tokens split across the range, so a
/// caller passes the active bin they expect plus a tolerance; if a swap has
/// moved the active bin outside `[expected - slippage, expected + slippage]`
/// the deposit reverts (sandwich protection that `min_liquidity_shares` — a
/// floor on share *count* — cannot provide). Reverts if the total shares
/// minted is below `min_liquidity_shares`.
pub fn add_liquidity_by_strategy(
    ctx: Context<AddLiquidity>,
    amount_x: u64,
    amount_y: u64,
    strategy: u8,
    min_liquidity_shares: u128,
    expected_active_bin_id: i32,
    active_id_slippage: u32,
) -> Result<()> {
    let strat = Strategy::from_u8(strategy).ok_or(DlmmError::InvalidStrategy)?;
    require!(amount_x > 0 || amount_y > 0, DlmmError::ZeroAmount);

    let lb_pair_key = ctx.accounts.lb_pair.key();

    // --- read the pair (active bin + step), verify reserves ---
    let (active_bin_id, bin_step) = {
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
        (pair.active_bin_id, pair.bin_step)
    };

    // Reject if the active bin has moved outside the caller's accepted window.
    let lo = expected_active_bin_id as i64 - active_id_slippage as i64;
    let hi = expected_active_bin_id as i64 + active_id_slippage as i64;
    require!(
        (active_bin_id as i64) >= lo && (active_bin_id as i64) <= hi,
        DlmmError::ActiveBinIdMoved
    );

    let total_shares: u128;
    {
        let mut pos = ctx.accounts.position.load_mut()?;
        require_keys_eq!(pos.lb_pair, lb_pair_key, DlmmError::Unauthorized);
        require_keys_eq!(pos.owner, ctx.accounts.owner.key(), DlmmError::Unauthorized);
        let (lower, upper) = (pos.lower_bin_id, pos.upper_bin_id);

        let mut arr = ctx.accounts.bin_array.load_mut()?;
        require_keys_eq!(arr.lb_pair, lb_pair_key, DlmmError::Unauthorized);
        require!(
            arr.index == BinArray::index_of(lower),
            DlmmError::BinArrayIndexMismatch
        );

        // Split the deposit across the bins by strategy (token-side aware).
        let plan =
            plan_deposit(lower, upper, active_bin_id, amount_x, amount_y, strat).map_err(|e| {
                match e {
                    PlanError::TokenSideMismatch => DlmmError::DepositTokenMismatch,
                    PlanError::Math => DlmmError::MathOverflow,
                }
            })?;

        let mut minted: u128 = 0;
        for bin_dep in plan {
            let (id, x_k, y_k) = (bin_dep.bin_id, bin_dep.x, bin_dep.y);
            if x_k == 0 && y_k == 0 {
                continue;
            }

            let price =
                bin_price(bin_step, id, Rounding::Down).ok_or(DlmmError::BinIdOutOfRange)?;
            // Liquidity added to this bin, in Y units: price * x + y.
            let l_added = price
                .mul_int(x_k as u128, Rounding::Down)
                .ok_or(DlmmError::MathOverflow)?
                .checked_add(y_k as u128)
                .ok_or(DlmmError::MathOverflow)?;

            let slot = arr.slot_of(id).ok_or(DlmmError::BinArrayIndexMismatch)?;
            let bin = &mut arr.bins[slot];

            let shares = if bin.liquidity_supply == 0 {
                // With adds only, supply == 0 implies the bin is empty. NOTE
                // for #34 (remove): a full withdrawal must zero the bin's
                // reserves too, or fold any dust here, so a fresh depositor
                // can't inherit leftover reserves for free.
                l_added
            } else {
                // L already in the bin, at the same price. Round the
                // denominator Up (and l_added Down, above) so shares never
                // round in the depositor's favor.
                let l_before = price
                    .mul_int(bin.amount_x as u128, Rounding::Up)
                    .ok_or(DlmmError::MathOverflow)?
                    .checked_add(bin.amount_y as u128)
                    .ok_or(DlmmError::MathOverflow)?;
                require!(l_before > 0, DlmmError::InsufficientLiquidity);
                mul_div(l_added, bin.liquidity_supply, l_before, Rounding::Down)
                    .map_err(|_| DlmmError::MathOverflow)?
            };
            // A deposit too small to earn a share would silently donate tokens.
            require!(shares > 0, DlmmError::InsufficientLiquidity);

            // Settle the bin's accrued fees into the position before its share
            // count changes — a new deposit must not earn fees that accrued
            // before it, and a top-up must not rewrite the existing shares'
            // earnings. (First deposit settles 0 and just seeds the checkpoint.)
            let pos_slot = (id - lower) as usize;
            pos.settle_bin(pos_slot, bin.fee_growth_x, bin.fee_growth_y)?;

            bin.amount_x = bin
                .amount_x
                .checked_add(x_k)
                .ok_or(DlmmError::MathOverflow)?;
            bin.amount_y = bin
                .amount_y
                .checked_add(y_k)
                .ok_or(DlmmError::MathOverflow)?;
            bin.liquidity_supply = bin
                .liquidity_supply
                .checked_add(shares)
                .ok_or(DlmmError::MathOverflow)?;

            pos.liquidity_shares[pos_slot] = pos.liquidity_shares[pos_slot]
                .checked_add(shares)
                .ok_or(DlmmError::MathOverflow)?;
            minted = minted.checked_add(shares).ok_or(DlmmError::MathOverflow)?;
        }

        require!(minted > 0, DlmmError::ZeroAmount);
        require!(minted >= min_liquidity_shares, DlmmError::SlippageExceeded);
        total_shares = minted;
    }

    // Pull the deposited tokens into the reserves (owner signs). The per-bin
    // amounts sum to exactly amount_x / amount_y, so the reserves match the
    // recorded bin reserves.
    if amount_x > 0 {
        transfer_in(
            &ctx,
            &ctx.accounts.user_token_x,
            &ctx.accounts.reserve_x,
            amount_x,
        )?;
    }
    if amount_y > 0 {
        transfer_in(
            &ctx,
            &ctx.accounts.user_token_y,
            &ctx.accounts.reserve_y,
            amount_y,
        )?;
    }

    emit!(LiquidityAdded {
        lb_pair: lb_pair_key,
        position: ctx.accounts.position.key(),
        amount_x,
        amount_y,
        shares_minted: total_shares,
        strategy,
    });

    Ok(())
}

/// Transfer `amount` from a user account into a reserve (the owner signs).
fn transfer_in<'info>(
    ctx: &Context<AddLiquidity<'info>>,
    from: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    amount: u64,
) -> Result<()> {
    transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: from.to_account_info(),
                to: to.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        amount,
    )
}
