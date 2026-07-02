//! `swap` — trade against the constant-product curve.
//!
//! The fee is taken from the input token. For `ExactIn` the gross input is
//! reduced by the fee before the curve computes the output; for `ExactOut` the
//! input is grossed up so the post-fee remainder still buys the requested
//! output. The fee splits into a protocol cut (accrued to the pool, held in the
//! vault but untracked as liquidity) and an LP cut that stays in the reserve,
//! compounding into `k` for every LP.
//!
//! The core arithmetic is the pure [`compute_swap`] so it can be unit-tested and
//! ported bit-exact to the SDK; the handler only moves tokens and updates state.

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};
use zenith_math::{in_given_out, out_given_in};

use crate::constants::POOL_AUTHORITY_SEED;
use crate::errors::CammError;
use crate::events::Swap as SwapEvent;
use crate::fee::{fee_on_input, gross_input_for_net, split_protocol_fee};
use crate::state::Pool;

/// Trade direction across the pool's two tokens.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    /// Sell token A for token B.
    AtoB,
    /// Sell token B for token A.
    BtoA,
}

/// Whether `amount` is the exact input to spend or the exact output to receive.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwapMode {
    /// `amount` is the input; solve for the output.
    ExactIn,
    /// `amount` is the desired output; solve for the input.
    ExactOut,
}

/// Fully-resolved swap: how much goes in, how much comes out, and how the fee
/// split. `amount_in == net_in + fee`, and `fee == protocol_fee + lp_fee`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapResult {
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee: u64,
    pub protocol_fee: u64,
    pub lp_fee: u64,
}

/// Resolve a swap against a `(reserve_in, reserve_out)` curve. Pure: no account
/// access, so it is unit-tested directly and mirrored by the SDK quote.
pub fn compute_swap(
    reserve_in: u64,
    reserve_out: u64,
    base_fee_bps: u16,
    protocol_fee_rate: u16,
    mode: SwapMode,
    amount: u64,
) -> std::result::Result<SwapResult, CammError> {
    require_gt(amount)?;
    match mode {
        SwapMode::ExactIn => {
            let fee = fee_on_input(amount, base_fee_bps).map_err(|_| CammError::MathOverflow)?;
            // fee <= amount (fee_bps < 100%), so net_in never underflows.
            let net_in = amount - fee;
            let out = out_given_in(reserve_in as u128, reserve_out as u128, net_in as u128)
                .map_err(|_| CammError::MathOverflow)?;
            // out < reserve_out (curve invariant), so it fits u64.
            let amount_out: u64 = out.try_into().map_err(|_| CammError::MathOverflow)?;
            let (protocol_fee, lp_fee) =
                split_protocol_fee(fee, protocol_fee_rate).map_err(|_| CammError::MathOverflow)?;
            Ok(SwapResult {
                amount_in: amount,
                amount_out,
                fee,
                protocol_fee,
                lp_fee,
            })
        }
        SwapMode::ExactOut => {
            // Not `require!` — this pure fn returns CammError, whereas require!
            // builds an anchor Error; the handler converts CammError via `?`.
            if amount >= reserve_out {
                return Err(CammError::InsufficientReserve);
            }
            let net = in_given_out(reserve_in as u128, reserve_out as u128, amount as u128)
                .map_err(|_| CammError::MathOverflow)?;
            let net_in: u64 = net.try_into().map_err(|_| CammError::MathOverflow)?;
            let amount_in =
                gross_input_for_net(net_in, base_fee_bps).map_err(|_| CammError::MathOverflow)?;
            // amount_in >= net_in (gross-up rounds up), so fee never underflows.
            let fee = amount_in - net_in;
            let (protocol_fee, lp_fee) =
                split_protocol_fee(fee, protocol_fee_rate).map_err(|_| CammError::MathOverflow)?;
            Ok(SwapResult {
                amount_in,
                amount_out: amount,
                fee,
                protocol_fee,
                lp_fee,
            })
        }
    }
}

fn require_gt(amount: u64) -> std::result::Result<(), CammError> {
    if amount == 0 {
        Err(CammError::ZeroAmount)
    } else {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Swap<'info> {
    pub user: Signer<'info>,

    #[account(mut)]
    pub pool: AccountLoader<'info, Pool>,

    /// CHECK: PDA that owns the reserves; signs the output payout.
    #[account(seeds = [POOL_AUTHORITY_SEED, pool.key().as_ref()], bump)]
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub reserve_a_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub reserve_b_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = user)]
    pub user_token_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = user)]
    pub user_token_b: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

pub fn swap(
    ctx: Context<Swap>,
    direction: Direction,
    mode: SwapMode,
    amount: u64,
    other_amount_threshold: u64,
) -> Result<()> {
    let pool_key = ctx.accounts.pool.key();
    let result;
    {
        let mut pool = ctx.accounts.pool.load_mut()?;
        require!(pool.is_active(), CammError::PoolNotActive);
        require_keys_eq!(
            ctx.accounts.reserve_a_vault.key(),
            pool.reserve_a_vault,
            CammError::Unauthorized
        );
        require_keys_eq!(
            ctx.accounts.reserve_b_vault.key(),
            pool.reserve_b_vault,
            CammError::Unauthorized
        );

        // Orient reserves by direction.
        let (reserve_in, reserve_out) = match direction {
            Direction::AtoB => (pool.reserve_a, pool.reserve_b),
            Direction::BtoA => (pool.reserve_b, pool.reserve_a),
        };
        let r = compute_swap(
            reserve_in,
            reserve_out,
            pool.base_fee_bps,
            pool.protocol_fee_rate,
            mode,
            amount,
        )?;

        // Slippage guard.
        match mode {
            SwapMode::ExactIn => require!(
                r.amount_out >= other_amount_threshold,
                CammError::SlippageExceeded
            ),
            SwapMode::ExactOut => require!(
                r.amount_in <= other_amount_threshold,
                CammError::SlippageExceeded
            ),
        }

        // The input reserve grows by everything except the protocol cut (net_in
        // + lp_fee); the output reserve shrinks by the payout. The protocol cut
        // is set aside in its own accumulator (still physically in the vault).
        let reserve_in_delta = r
            .amount_in
            .checked_sub(r.protocol_fee)
            .ok_or(CammError::MathOverflow)?;
        let new_in = reserve_in
            .checked_add(reserve_in_delta)
            .ok_or(CammError::MathOverflow)?;
        let new_out = reserve_out
            .checked_sub(r.amount_out)
            .ok_or(CammError::InsufficientReserve)?;

        match direction {
            Direction::AtoB => {
                pool.reserve_a = new_in;
                pool.reserve_b = new_out;
                pool.protocol_fee_a = pool
                    .protocol_fee_a
                    .checked_add(r.protocol_fee)
                    .ok_or(CammError::MathOverflow)?;
            }
            Direction::BtoA => {
                pool.reserve_b = new_in;
                pool.reserve_a = new_out;
                pool.protocol_fee_b = pool
                    .protocol_fee_b
                    .checked_add(r.protocol_fee)
                    .ok_or(CammError::MathOverflow)?;
            }
        }
        result = r;
    }

    // Move tokens: pull the input in (user signs), pay the output out (pool
    // authority signs).
    let signer_seeds: &[&[&[u8]]] = &[&[
        POOL_AUTHORITY_SEED,
        pool_key.as_ref(),
        &[ctx.bumps.pool_authority],
    ]];
    let (in_vault, out_vault, user_in, user_out) = match direction {
        Direction::AtoB => (
            &ctx.accounts.reserve_a_vault,
            &ctx.accounts.reserve_b_vault,
            &ctx.accounts.user_token_a,
            &ctx.accounts.user_token_b,
        ),
        Direction::BtoA => (
            &ctx.accounts.reserve_b_vault,
            &ctx.accounts.reserve_a_vault,
            &ctx.accounts.user_token_b,
            &ctx.accounts.user_token_a,
        ),
    };

    transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: user_in.to_account_info(),
                to: in_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        result.amount_in,
    )?;
    transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: out_vault.to_account_info(),
                to: user_out.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        ),
        result.amount_out,
    )?;

    emit!(SwapEvent {
        pool: pool_key,
        direction: direction as u8,
        amount_in: result.amount_in,
        amount_out: result.amount_out,
        fee: result.fee,
        protocol_fee: result.protocol_fee,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_in_matches_curve_and_fee() {
        // 30 bps fee, no protocol cut. 1000/1000 pool, 100 in.
        let r = compute_swap(1000, 1000, 30, 0, SwapMode::ExactIn, 100).unwrap();
        // fee = ceil(100*30/1e4) = 1; net = 99; out = 1000*99/1099 = 90.
        assert_eq!(r.fee, 1);
        assert_eq!(r.amount_in, 100);
        assert_eq!(r.amount_out, 90);
        assert_eq!(r.protocol_fee, 0);
        assert_eq!(r.lp_fee, 1);
    }

    #[test]
    fn exact_out_grosses_up_input() {
        // Ask for 90 out of 1000/1000 at 30 bps. net_in = ceil(1000*90/910)=99,
        // gross = ceil(99*1e4/9970) = 100, fee = 1.
        let r = compute_swap(1000, 1000, 30, 0, SwapMode::ExactOut, 90).unwrap();
        assert_eq!(r.amount_out, 90);
        assert_eq!(r.fee, r.amount_in - 99);
        assert!(r.amount_in >= 100);
    }

    #[test]
    fn protocol_split_is_carved_from_fee() {
        // 100 bps fee, 50% protocol. 1_000_000 pool, 10_000 in.
        let r = compute_swap(1_000_000, 1_000_000, 100, 5000, SwapMode::ExactIn, 10_000).unwrap();
        assert_eq!(r.protocol_fee + r.lp_fee, r.fee);
        assert_eq!(r.protocol_fee, r.fee / 2);
    }

    #[test]
    fn rejects_zero_and_over_reserve() {
        assert!(matches!(
            compute_swap(1000, 1000, 30, 0, SwapMode::ExactIn, 0),
            Err(CammError::ZeroAmount)
        ));
        // Cannot ask for the whole (or more than the) output reserve.
        assert!(matches!(
            compute_swap(1000, 1000, 30, 0, SwapMode::ExactOut, 1000),
            Err(CammError::InsufficientReserve)
        ));
    }

    #[test]
    fn fee_never_lets_out_exceed_reserve() {
        // Huge exact-in never drains the out reserve.
        let r = compute_swap(1000, 1000, 30, 0, SwapMode::ExactIn, u64::MAX).unwrap();
        assert!(r.amount_out < 1000);
    }
}
