//! Thin program-side wrappers over `zenith-math` for the AMM handlers.
//!
//! These keep the token-amount/return-type plumbing (and the protocol-favoring
//! rounding choices) in one place so the instruction handlers stay readable and
//! the logic is unit-testable on the host.

use anchor_lang::prelude::*;
use zenith_math::{
    delta_a, delta_b, mul_div, mul_shr, next_sqrt_price_from_amount_x,
    next_sqrt_price_from_amount_y, pow, shl_div, Q64x64, Rounding, SCALE_OFFSET,
};

use crate::errors::ZenithError;
use crate::state::Position;

/// Basis-point denominator (100%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Fee scheduler modes (stored as `u8` on the config).
pub const FEE_MODE_CONSTANT: u8 = 0;
pub const FEE_MODE_LINEAR: u8 = 1;
pub const FEE_MODE_EXPONENTIAL: u8 = 2;

/// Denominator for the dynamic-fee formula `dynamic = va^2 * control / 1e9`.
pub const DYNAMIC_FEE_DENOMINATOR: u128 = 1_000_000_000;

/// Relative price move vs the volatility anchor, in basis points:
/// `|sqrt_now - sqrt_ref| * 10000 / sqrt_ref`. Returns 0 if no anchor is set.
pub fn price_move_bps(sqrt_ref: u128, sqrt_now: u128) -> u128 {
    if sqrt_ref == 0 {
        return 0;
    }
    let diff = sqrt_now.abs_diff(sqrt_ref);
    mul_div(diff, BPS_DENOMINATOR, sqrt_ref, Rounding::Down).unwrap_or(u128::MAX)
}

/// Decay the stored accumulator by idle time into the reference the next swap
/// builds on: unchanged within the filter window, scaled by
/// `reduction_factor_bps` between filter and decay, fully reset past decay.
pub fn decayed_volatility_reference(
    accumulator: u128,
    elapsed: u64,
    filter_period: u32,
    decay_period: u32,
    reduction_factor_bps: u16,
) -> u128 {
    if elapsed >= decay_period as u64 {
        0
    } else if elapsed >= filter_period as u64 {
        mul_div(
            accumulator,
            reduction_factor_bps as u128,
            BPS_DENOMINATOR,
            Rounding::Down,
        )
        .unwrap_or(0)
    } else {
        accumulator
    }
}

/// New accumulator after a price move: `reference + move`, capped at `max_va`.
pub fn accumulate_volatility(reference: u128, move_bps: u128, max_va: u32) -> u128 {
    reference.saturating_add(move_bps).min(max_va as u128)
}

/// Dynamic surcharge in bps: `va^2 * control / 1e9`, capped at `max_dynamic`.
/// Zero `control` disables it.
pub fn dynamic_fee_bps(va: u128, variable_fee_control: u32, max_dynamic_fee_bps: u16) -> u16 {
    if variable_fee_control == 0 {
        return 0;
    }
    let sq = va.saturating_mul(va);
    let fee = sq.saturating_mul(variable_fee_control as u128) / DYNAMIC_FEE_DENOMINATOR;
    fee.min(max_dynamic_fee_bps as u128) as u16
}

/// Result of folding a swap into the volatility state. The caller persists all
/// four onto the pool.
pub struct DynamicFeeState {
    /// Surcharge to add to the base fee this swap, bps.
    pub dynamic_fee_bps: u16,
    /// Volatility accumulator after the swap (the live value: reference + move).
    pub volatility_accumulator: u128,
    /// Decayed carry for the next swap (fixed within a volatility window).
    pub volatility_reference: u128,
    /// Price anchor moves are measured from (re-set when a window starts).
    pub sqrt_price_reference: u128,
}

/// Fold a swap into the volatility state and derive the dynamic surcharge.
///
/// A "volatility window" begins whenever `elapsed >= filter_period`: the anchor
/// re-sets to the current price and the *reference* becomes the decayed prior
/// accumulator (scaled by `reduction_factor` between filter and decay windows,
/// zero past decay). Within a window the reference and anchor are held fixed, so
/// the accumulator is `reference + price_move_from_anchor` — the drift is
/// measured once from a stable anchor (no double-counting), capped at `max_va`.
/// Computed on the pre-swap price, so a swap's own move is surcharged on the
/// next trade.
#[allow(clippy::too_many_arguments)]
pub fn compute_dynamic_fee(
    sqrt_price: u128,
    sqrt_price_reference: u128,
    volatility_accumulator: u128,
    volatility_reference: u128,
    elapsed: u64,
    filter_period: u32,
    decay_period: u32,
    reduction_factor_bps: u16,
    max_va: u32,
    variable_fee_control: u32,
    max_dynamic_fee_bps: u16,
) -> DynamicFeeState {
    let new_window = elapsed >= filter_period as u64;
    let (reference, anchor) = if new_window {
        // Start a new window: anchor at the current price, carry the decayed
        // prior accumulator. (elapsed >= filter, so this returns the decayed
        // value, not the unchanged accumulator.)
        let carry = decayed_volatility_reference(
            volatility_accumulator,
            elapsed,
            filter_period,
            decay_period,
            reduction_factor_bps,
        );
        (carry, sqrt_price)
    } else {
        // Inside the window: reference and anchor stay put.
        (volatility_reference, sqrt_price_reference)
    };

    let move_bps = price_move_bps(anchor, sqrt_price);
    let va = accumulate_volatility(reference, move_bps, max_va);
    let fee = dynamic_fee_bps(va, variable_fee_control, max_dynamic_fee_bps);
    DynamicFeeState {
        dynamic_fee_bps: fee,
        volatility_accumulator: va,
        volatility_reference: reference,
        sqrt_price_reference: anchor,
    }
}

/// Current base swap fee (bps) for a scheduler at `elapsed_slots` since pool
/// creation. `Constant` returns `base_fee_bps`; the decaying modes start at
/// `cliff_fee_bps` and step down every `fee_period` slots (capped at
/// `max_fee_steps`), clamped to the `[base_fee_bps, cliff_fee_bps]` band:
/// - Linear: `cliff - reduction_factor * steps`.
/// - Exponential: `cliff * (1 - reduction_factor/10000)^steps` (Q64.64).
///
/// Output is monotonically non-increasing in `elapsed_slots` and never leaves
/// the band. Caller validates the params at config creation.
#[allow(clippy::too_many_arguments)]
pub fn scheduled_base_fee_bps(
    mode: u8,
    base_fee_bps: u16,
    cliff_fee_bps: u16,
    reduction_factor: u16,
    fee_period: u64,
    max_fee_steps: u16,
    elapsed_slots: u64,
) -> Result<u16> {
    if mode == FEE_MODE_CONSTANT {
        return Ok(base_fee_bps);
    }

    let steps = if fee_period == 0 {
        0
    } else {
        (elapsed_slots / fee_period).min(max_fee_steps as u64)
    };
    let cliff = cliff_fee_bps as u128;
    let floor = base_fee_bps as u128;

    let raw = match mode {
        FEE_MODE_LINEAR => {
            let dec = (reduction_factor as u128).saturating_mul(steps as u128);
            cliff.saturating_sub(dec)
        }
        FEE_MODE_EXPONENTIAL => {
            // (1 - reduction/10000) in Q64.64, raised to `steps`.
            let base_bits = shl_div(
                BPS_DENOMINATOR - reduction_factor as u128,
                SCALE_OFFSET,
                BPS_DENOMINATOR,
                Rounding::Down,
            )
            .map_err(|_| ZenithError::MathOverflow)?;
            let factor = pow(Q64x64::from_bits(base_bits), steps as i32, Rounding::Down)
                .ok_or(ZenithError::MathOverflow)?;
            factor
                .mul_int(cliff, Rounding::Down)
                .ok_or(ZenithError::MathOverflow)?
        }
        _ => return Err(ZenithError::InvalidFeeConfig.into()),
    };

    // Decaying modes only ever reduce, but clamp defensively to the band.
    Ok(raw.clamp(floor, cliff) as u16)
}

/// Narrow a `u128` math result to a `u64` token amount, erroring on overflow.
pub fn to_token_amount(x: u128) -> Result<u64> {
    u64::try_from(x).map_err(|_| error!(ZenithError::MathOverflow))
}

/// Validate a pool's price band and that the current price sits strictly inside.
///
/// All values are Q64.64 raw bits. Requires `0 < min < price < max`.
pub fn validate_price_band(sqrt_min: u128, sqrt_price: u128, sqrt_max: u128) -> Result<()> {
    require!(
        sqrt_min > 0 && sqrt_min < sqrt_max,
        ZenithError::InvalidPriceBand
    );
    require!(
        sqrt_price > sqrt_min && sqrt_price < sqrt_max,
        ZenithError::PriceOutOfBand
    );
    Ok(())
}

/// Token amounts for `liquidity` at `sqrt_price` within the band.
///
/// For a price strictly inside the band a position holds both tokens:
/// - token A (base) over `[price, max]` → [`delta_a`]
/// - token B (quote) over `[min, price]` → [`delta_b`]
///
/// `rounding` is the caller's protocol-favoring choice: **up** when the user is
/// depositing (they must back the liquidity with at least this much) and
/// **down** when the pool is paying out (never more than backed). Returns
/// `(amount_a, amount_b)`.
pub fn liquidity_amounts(
    liquidity: u128,
    sqrt_price: u128,
    sqrt_min: u128,
    sqrt_max: u128,
    rounding: Rounding,
) -> Result<(u64, u64)> {
    let price = Q64x64::from_bits(sqrt_price);
    let lo = Q64x64::from_bits(sqrt_min);
    let hi = Q64x64::from_bits(sqrt_max);

    let amount_a = delta_a(liquidity, price, hi, rounding).ok_or(ZenithError::MathOverflow)?;
    let amount_b = delta_b(liquidity, lo, price, rounding).ok_or(ZenithError::MathOverflow)?;

    Ok((to_token_amount(amount_a)?, to_token_amount(amount_b)?))
}

/// Token amounts a creator must deposit to mint `liquidity` (always rounds up).
pub fn initial_liquidity_amounts(
    liquidity: u128,
    sqrt_price: u128,
    sqrt_min: u128,
    sqrt_max: u128,
) -> Result<(u64, u64)> {
    liquidity_amounts(liquidity, sqrt_price, sqrt_min, sqrt_max, Rounding::Up)
}

/// Settle the fees a position has earned since its last checkpoint into its
/// `fee_pending_*` buckets, then advance the checkpoints to the current global
/// fee growth. Must run **before** any change to the position's liquidity, so
/// fees are attributed to the liquidity that actually earned them.
///
/// Fees accrue on the position's *total* liquidity (unlocked + vested + locked);
/// all of it earns. Per-liquidity growth is Q64.64, so the earned token amount
/// is `total_liquidity * Δgrowth >> 64`, rounded **down** (the pool never pays
/// out more than it accrued). The global accumulator is allowed to wrap, so the
/// delta is a wrapping subtraction.
pub fn settle_position_fees(
    position: &mut Position,
    fee_growth_global_a: u128,
    fee_growth_global_b: u128,
) -> Result<()> {
    let liquidity = position.total_liquidity();

    let earned_a = accrued_fee(
        liquidity,
        fee_growth_global_a,
        position.fee_growth_checkpoint_a,
    )?;
    let earned_b = accrued_fee(
        liquidity,
        fee_growth_global_b,
        position.fee_growth_checkpoint_b,
    )?;

    position.fee_pending_a = position
        .fee_pending_a
        .checked_add(earned_a)
        .ok_or(ZenithError::MathOverflow)?;
    position.fee_pending_b = position
        .fee_pending_b
        .checked_add(earned_b)
        .ok_or(ZenithError::MathOverflow)?;

    position.fee_growth_checkpoint_a = fee_growth_global_a;
    position.fee_growth_checkpoint_b = fee_growth_global_b;

    Ok(())
}

/// Token fees earned for `liquidity` between a checkpoint and the current global
/// fee growth (both Q64.64 per-liquidity raw bits). Rounds down.
fn accrued_fee(liquidity: u128, fee_growth_global: u128, checkpoint: u128) -> Result<u64> {
    let delta = fee_growth_global.wrapping_sub(checkpoint);
    let earned = mul_shr(liquidity, delta, SCALE_OFFSET, Rounding::Down)
        .map_err(|_| ZenithError::MathOverflow)?;
    to_token_amount(earned)
}

/// Which way a swap moves the pool.
///
/// `AToB` sells token A (base) for token B (quote); adding A lowers the price
/// toward `sqrt_min`. `BToA` is the reverse, raising the price toward `sqrt_max`.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapDirection {
    /// Sell A, receive B (price falls).
    AToB,
    /// Sell B, receive A (price rises).
    BToA,
}

/// How the caller specified the swap amount.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapMode {
    /// `amount` is the exact input; reverts if the fill would leave the band.
    ExactIn,
    /// `amount` is the exact desired output; reverts if it would leave the band.
    ExactOut,
    /// `amount` is the input, but fill only up to the band boundary and report
    /// the unspent remainder instead of reverting.
    PartialFill,
}

/// Result of a single swap step within the pool's one price band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapStep {
    /// Pool sqrt-price after the step (Q64.64 raw bits).
    pub next_sqrt_price: u128,
    /// Gross input consumed, including fee (raw token units).
    pub amount_in: u64,
    /// Output paid to the trader (raw token units).
    pub amount_out: u64,
    /// Total fee taken from the input (raw token units of the input token).
    pub fee: u64,
    /// Unspent input returned to the trader (only nonzero for `PartialFill`).
    pub amount_remaining: u64,
}

/// Fee included in a gross input amount: `ceil(gross * bps / 10000)`.
fn fee_on_gross(gross: u128, fee_bps: u128) -> Result<u128> {
    mul_div(gross, fee_bps, BPS_DENOMINATOR, Rounding::Up)
        .map_err(|_| ZenithError::MathOverflow.into())
}

/// Fee to add on top of a net input: `ceil(net * bps / (10000 - bps))`.
/// Requires `fee_bps < 10000` (caller-validated).
fn fee_on_net(net: u128, fee_bps: u128) -> Result<u128> {
    mul_div(net, fee_bps, BPS_DENOMINATOR - fee_bps, Rounding::Up)
        .map_err(|_| ZenithError::MathOverflow.into())
}

/// Compute one swap step over the pool's single liquidity band.
///
/// `amount` is the input (ExactIn/PartialFill) or the desired output
/// (ExactOut), in raw token units. `fee_bps` is the base swap fee. All prices
/// are Q64.64 raw bits and `liquidity > 0`. Outputs round **down** and required
/// inputs/deltas round **up**, so rounding never favors the trader; the price
/// can never leave `[sqrt_min, sqrt_max]`.
pub fn compute_swap_step(
    sqrt_price: u128,
    liquidity: u128,
    sqrt_min: u128,
    sqrt_max: u128,
    direction: SwapDirection,
    mode: SwapMode,
    amount: u64,
    fee_bps: u16,
) -> Result<SwapStep> {
    require!(liquidity > 0, ZenithError::InsufficientLiquidity);
    require!(amount > 0, ZenithError::ZeroAmount);
    let fee_bps = fee_bps as u128;
    require!(fee_bps < BPS_DENOMINATOR, ZenithError::InvalidFeeConfig);

    let price = Q64x64::from_bits(sqrt_price);
    let a_to_b = direction == SwapDirection::AToB;
    let boundary_bits = if a_to_b { sqrt_min } else { sqrt_max };
    let boundary = Q64x64::from_bits(boundary_bits);

    match mode {
        SwapMode::ExactIn | SwapMode::PartialFill => {
            let gross = amount as u128;
            let fee = fee_on_gross(gross, fee_bps)?;
            let net_in = gross - fee; // fee <= gross since bps < 10000

            // Provisional next price assuming the whole net input is consumed.
            let next = if a_to_b {
                next_sqrt_price_from_amount_x(price, liquidity, net_in, true)
            } else {
                next_sqrt_price_from_amount_y(price, liquidity, net_in, true)
            }
            .ok_or(ZenithError::MathOverflow)?;

            let crosses = if a_to_b {
                next.to_bits() < boundary_bits
            } else {
                next.to_bits() > boundary_bits
            };

            if !crosses {
                // Full fill within the band: the whole input is consumed.
                let amount_out = swap_output(liquidity, next, price, a_to_b)?;
                require!(amount_out > 0, ZenithError::ZeroAmount);
                return Ok(SwapStep {
                    next_sqrt_price: next.to_bits(),
                    amount_in: amount,
                    amount_out,
                    fee: to_token_amount(fee)?,
                    amount_remaining: 0,
                });
            }

            // Would leave the band.
            require!(mode == SwapMode::PartialFill, ZenithError::PriceOutOfBand);

            // Fill exactly to the boundary; recompute consumed input + fee.
            let net_consumed = swap_input_to(liquidity, price, boundary, a_to_b)?;
            require!(net_consumed > 0, ZenithError::PriceOutOfBand);
            let fee_consumed = fee_on_net(net_consumed, fee_bps)?;
            let amount_in = net_consumed
                .checked_add(fee_consumed)
                .ok_or(ZenithError::MathOverflow)?;
            // Clamp to the caller's input (rounding can only ever match or
            // slightly undershoot the provisional gross).
            let amount_in = amount_in.min(gross);
            let fee_consumed = amount_in - net_consumed;
            let amount_out = swap_output(liquidity, boundary, price, a_to_b)?;
            require!(amount_out > 0, ZenithError::PriceOutOfBand);
            let amount_in = to_token_amount(amount_in)?;

            Ok(SwapStep {
                next_sqrt_price: boundary_bits,
                amount_in,
                amount_out,
                fee: to_token_amount(fee_consumed)?,
                amount_remaining: amount - amount_in,
            })
        }
        SwapMode::ExactOut => {
            let want_out = amount as u128;

            // Price after paying out `want_out`.
            let next = if a_to_b {
                // Output is B (y): removing y lowers the price.
                next_sqrt_price_from_amount_y(price, liquidity, want_out, false)
            } else {
                // Output is A (x): removing x raises the price.
                next_sqrt_price_from_amount_x(price, liquidity, want_out, false)
            }
            .ok_or(ZenithError::MathOverflow)?;

            let crosses = if a_to_b {
                next.to_bits() < boundary_bits
            } else {
                next.to_bits() > boundary_bits
            };
            require!(!crosses, ZenithError::PriceOutOfBand);

            let net_in = swap_input_to(liquidity, price, next, a_to_b)?;
            require!(net_in > 0, ZenithError::ZeroAmount);
            let fee = fee_on_net(net_in, fee_bps)?;
            let amount_in = net_in.checked_add(fee).ok_or(ZenithError::MathOverflow)?;

            Ok(SwapStep {
                next_sqrt_price: next.to_bits(),
                amount_in: to_token_amount(amount_in)?,
                amount_out: amount, // exact
                fee: to_token_amount(fee)?,
                amount_remaining: 0,
            })
        }
    }
}

/// Output tokens when the price moves from `from` to `to` for liquidity `L`.
/// `a_to_b` pays token B (price fell); otherwise token A (price rose). Down.
fn swap_output(liquidity: u128, to: Q64x64, from: Q64x64, a_to_b: bool) -> Result<u64> {
    let out = if a_to_b {
        delta_b(liquidity, to, from, Rounding::Down)
    } else {
        delta_a(liquidity, from, to, Rounding::Down)
    }
    .ok_or(ZenithError::MathOverflow)?;
    to_token_amount(out)
}

/// Net input (pre-fee) needed to move the price from `from` to `to`.
/// `a_to_b` charges token A; otherwise token B. Up.
fn swap_input_to(liquidity: u128, from: Q64x64, to: Q64x64, a_to_b: bool) -> Result<u128> {
    if a_to_b {
        delta_a(liquidity, to, from, Rounding::Up)
    } else {
        delta_b(liquidity, from, to, Rounding::Up)
    }
    .ok_or(ZenithError::MathOverflow.into())
}

/// Split a collected fee into (protocol share, LP share). The protocol share is
/// `protocol_fee_bps` of the fee, rounded down; the LP keeps the remainder.
pub fn split_fee(fee: u64, protocol_fee_bps: u16) -> Result<(u64, u64)> {
    let protocol = mul_div(
        fee as u128,
        protocol_fee_bps as u128,
        BPS_DENOMINATOR,
        Rounding::Down,
    )
    .map_err(|_| ZenithError::MathOverflow)?;
    let protocol = to_token_amount(protocol)?;
    Ok((protocol, fee - protocol))
}

/// Per-liquidity fee-growth increment for an LP fee of `lp_fee` tokens spread
/// over `liquidity`: `lp_fee << 64 / liquidity`, rounded down (Q64.64).
///
/// Flooring strands sub-unit-per-liquidity fee dust: those tokens are already in
/// the vault but go unattributed. This is safe — it favors the pool and the
/// dust is never withdrawable as liquidity (payouts derive from L/price/growth,
/// never the raw vault balance) — it just leaves a tiny growing vault surplus.
pub fn fee_growth_delta(lp_fee: u64, liquidity: u128) -> Result<u128> {
    if lp_fee == 0 || liquidity == 0 {
        return Ok(0);
    }
    zenith_math::shl_div(lp_fee as u128, SCALE_OFFSET, liquidity, Rounding::Down)
        .map_err(|_| ZenithError::MathOverflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Q64.64 helpers for the tests.
    const ONE: u128 = 1u128 << 64;

    #[test]
    fn band_validation() {
        // 0 < min < price < max is OK
        assert!(validate_price_band(ONE, 2 * ONE, 4 * ONE).is_ok());
        // min == 0 rejected
        assert!(validate_price_band(0, ONE, 2 * ONE).is_err());
        // min >= max rejected
        assert!(validate_price_band(4 * ONE, 2 * ONE, 4 * ONE).is_err());
        // price at or outside the band rejected
        assert!(validate_price_band(ONE, ONE, 4 * ONE).is_err()); // price == min
        assert!(validate_price_band(ONE, 4 * ONE, 4 * ONE).is_err()); // price == max
        assert!(validate_price_band(ONE, 5 * ONE, 4 * ONE).is_err()); // price > max
    }

    #[test]
    fn initial_amounts_known() {
        // L = 1000, band S in [1, 4], current S = 2 (sqrt prices 1,2,4 -> bits).
        // delta_a(L, price=2, max=4) and delta_b(L, min=1, price=2).
        // delta_b = L * (2-1) = 1000.
        // delta_a = L * (4-2)/(2*4) = 1000 * 2/8 = 250.
        let (a, b) = initial_liquidity_amounts(1000, 2 * ONE, ONE, 4 * ONE).unwrap();
        assert_eq!(a, 250);
        assert_eq!(b, 1000);
    }

    #[test]
    fn initial_amounts_overflow_is_caught() {
        // Force the token amount past u64 with a huge liquidity over a wide band.
        let res = initial_liquidity_amounts(u128::MAX, 2 * ONE, ONE, 4 * ONE);
        assert!(res.is_err());
    }

    #[test]
    fn to_token_amount_bounds() {
        assert_eq!(to_token_amount(123).unwrap(), 123);
        assert_eq!(to_token_amount(u64::MAX as u128).unwrap(), u64::MAX);
        assert!(to_token_amount(u64::MAX as u128 + 1).is_err());
    }

    #[test]
    fn add_never_pays_less_than_remove_returns() {
        // Deposit (round up) must always be >= withdrawal (round down) for the
        // same liquidity at the same price — rounding never favors the user.
        for &l in &[1u128, 7, 333, 10_000, 1_000_000] {
            let (add_a, add_b) = liquidity_amounts(l, 3 * ONE, ONE, 4 * ONE, Rounding::Up).unwrap();
            let (rem_a, rem_b) =
                liquidity_amounts(l, 3 * ONE, ONE, 4 * ONE, Rounding::Down).unwrap();
            assert!(add_a >= rem_a, "A: add {add_a} < remove {rem_a} for L={l}");
            assert!(add_b >= rem_b, "B: add {add_b} < remove {rem_b} for L={l}");
            // Rounding gap is at most 1 unit per side.
            assert!(add_a - rem_a <= 1 && add_b - rem_b <= 1);
        }
    }

    #[test]
    fn fee_settlement_accrues_and_advances_checkpoint() {
        let mut pos = Position {
            pool: Pubkey::default(),
            nft_mint: Pubkey::default(),
            liquidity: 1_000_000,
            vested_liquidity: 0,
            permanent_locked_liquidity: 0,
            fee_growth_checkpoint_a: 0,
            fee_growth_checkpoint_b: 0,
            fee_pending_a: 0,
            fee_pending_b: 0,
            bump: 0,
            reserved: [0u8; 64],
        };
        // Global grew by 0.5 (Q64.64) for A, 2.0 for B since the checkpoint.
        let half = ONE / 2;
        let two = 2 * ONE;
        settle_position_fees(&mut pos, half, two).unwrap();
        // earned = L * Δgrowth >> 64
        assert_eq!(pos.fee_pending_a, 500_000); // 1e6 * 0.5
        assert_eq!(pos.fee_pending_b, 2_000_000); // 1e6 * 2
        assert_eq!(pos.fee_growth_checkpoint_a, half);
        assert_eq!(pos.fee_growth_checkpoint_b, two);

        // Settling again with no further growth adds nothing.
        settle_position_fees(&mut pos, half, two).unwrap();
        assert_eq!(pos.fee_pending_a, 500_000);
        assert_eq!(pos.fee_pending_b, 2_000_000);
    }

    #[test]
    fn position_checkpointed_at_current_growth_earns_nothing_retroactively() {
        // Mirrors create_position seeding the checkpoint from the pool's live
        // global growth: a position opened after fees already accrued must not
        // be credited any of that pre-existing growth on its first settle.
        let global_a = 5 * ONE;
        let global_b = 9 * ONE;
        let mut pos = Position {
            pool: Pubkey::default(),
            nft_mint: Pubkey::default(),
            liquidity: 1_000_000,
            vested_liquidity: 0,
            permanent_locked_liquidity: 0,
            fee_growth_checkpoint_a: global_a, // seeded at creation
            fee_growth_checkpoint_b: global_b,
            fee_pending_a: 0,
            fee_pending_b: 0,
            bump: 0,
            reserved: [0u8; 64],
        };
        settle_position_fees(&mut pos, global_a, global_b).unwrap();
        assert_eq!(pos.fee_pending_a, 0);
        assert_eq!(pos.fee_pending_b, 0);
        // Only growth after creation is credited.
        settle_position_fees(&mut pos, global_a + ONE, global_b).unwrap();
        assert_eq!(pos.fee_pending_a, 1_000_000); // 1e6 * 1.0
        assert_eq!(pos.fee_pending_b, 0);
    }

    #[test]
    fn fee_growth_wraps_around() {
        let mut pos = Position {
            pool: Pubkey::default(),
            nft_mint: Pubkey::default(),
            liquidity: ONE, // 2^64, so >>64 gives the raw growth delta in tokens
            vested_liquidity: 0,
            permanent_locked_liquidity: 0,
            fee_growth_checkpoint_a: u128::MAX - 2,
            fee_growth_checkpoint_b: 0,
            fee_pending_a: 0,
            fee_pending_b: 0,
            bump: 0,
            reserved: [0u8; 64],
        };
        // Global wrapped past u128::MAX to 5: delta = 5 - (MAX-2) = 8 (wrapping).
        settle_position_fees(&mut pos, 5, 0).unwrap();
        assert_eq!(pos.fee_pending_a, 8);
    }

    // --- swap step ---

    // Band [1, 4] in sqrt terms, current sqrt-price 2.
    const LO: u128 = ONE;
    const MID: u128 = 2 * ONE;
    const HI: u128 = 4 * ONE;

    #[test]
    fn exact_in_within_band_consumes_all_input() {
        // Sell B (price rises toward HI). No fee for a clean check.
        let step = compute_swap_step(
            MID,
            1_000_000,
            LO,
            HI,
            SwapDirection::BToA,
            SwapMode::ExactIn,
            1_000,
            0,
        )
        .unwrap();
        assert_eq!(step.amount_in, 1_000); // fully consumed
        assert_eq!(step.fee, 0);
        assert_eq!(step.amount_remaining, 0);
        assert!(step.amount_out > 0);
        // Price rose but stayed in band.
        assert!(step.next_sqrt_price > MID && step.next_sqrt_price <= HI);
    }

    #[test]
    fn exact_in_takes_fee_from_input() {
        let step = compute_swap_step(
            MID,
            1_000_000,
            LO,
            HI,
            SwapDirection::BToA,
            SwapMode::ExactIn,
            10_000,
            100, // 1%
        )
        .unwrap();
        // 1% of 10_000 = 100 fee.
        assert_eq!(step.fee, 100);
        assert_eq!(step.amount_in, 10_000);
    }

    #[test]
    fn exact_in_crossing_band_reverts() {
        // Huge B input would push price past HI.
        let res = compute_swap_step(
            MID,
            1_000_000,
            LO,
            HI,
            SwapDirection::BToA,
            SwapMode::ExactIn,
            u64::MAX,
            0,
        );
        assert!(res.is_err());
    }

    #[test]
    fn partial_fill_clamps_to_boundary_and_returns_remainder() {
        let l = 1_000_000u128;
        let step = compute_swap_step(
            MID,
            l,
            LO,
            HI,
            SwapDirection::BToA,
            SwapMode::PartialFill,
            u64::MAX,
            0,
        )
        .unwrap();
        // Filled exactly to the upper boundary.
        assert_eq!(step.next_sqrt_price, HI);
        assert!(step.amount_remaining > 0);
        // Output equals all of token A available between MID and HI (floor).
        let max_a = delta_a(
            l,
            Q64x64::from_bits(MID),
            Q64x64::from_bits(HI),
            Rounding::Down,
        )
        .unwrap();
        assert_eq!(step.amount_out as u128, max_a);
        // Consumed input + remainder == provided.
        assert_eq!(step.amount_in as u64 + step.amount_remaining, u64::MAX);
    }

    #[test]
    fn exact_out_requires_more_input_than_output_returns() {
        // Want a fixed amount of A out (price rises). Input is B.
        let step = compute_swap_step(
            MID,
            10_000_000,
            LO,
            HI,
            SwapDirection::BToA,
            SwapMode::ExactOut,
            1_000,
            30, // 0.3%
        )
        .unwrap();
        assert_eq!(step.amount_out, 1_000);
        assert!(step.amount_in > step.amount_out); // input incl fee + price impact
        assert!(step.next_sqrt_price > MID && step.next_sqrt_price <= HI);
    }

    #[test]
    fn exact_out_crossing_band_reverts() {
        let res = compute_swap_step(
            MID,
            1_000,
            LO,
            HI,
            SwapDirection::BToA,
            SwapMode::ExactOut,
            u64::MAX,
            0,
        );
        assert!(res.is_err());
    }

    #[test]
    fn three_way_fee_split_is_exact() {
        // fee -> (protocol, lp); protocol -> (partner, protocol_remaining).
        // lp + protocol_remaining + partner must equal the fee exactly, for any
        // rates (no leakage, no double-count).
        for &fee in &[0u64, 1, 7, 1000, u64::MAX / 2] {
            for &(p_bps, partner_bps) in &[(0u16, 0u16), (1000, 2500), (10000, 10000), (3333, 7777)]
            {
                let (protocol, lp) = split_fee(fee, p_bps).unwrap();
                let (partner, protocol_remaining) = split_fee(protocol, partner_bps).unwrap();
                assert_eq!(
                    lp as u128 + protocol_remaining as u128 + partner as u128,
                    fee as u128,
                    "fee={fee} p={p_bps} partner={partner_bps}"
                );
                // partner is carved only from the protocol share, never the LP's.
                assert!(partner <= protocol);
            }
        }
    }

    #[test]
    fn fee_split_and_growth() {
        // fee 1000, protocol 25% -> 250 protocol, 750 LP.
        let (protocol, lp) = split_fee(1_000, 2_500).unwrap();
        assert_eq!((protocol, lp), (250, 750));
        // growth: lp_fee << 64 / L ; with L = 2^64 the delta is lp_fee.
        assert_eq!(fee_growth_delta(750, ONE).unwrap(), 750);
        assert_eq!(fee_growth_delta(0, ONE).unwrap(), 0);
    }

    // --- fee scheduler ---

    #[test]
    fn fee_constant_mode_ignores_time() {
        for slot in [0u64, 1, 1_000, u64::MAX] {
            assert_eq!(
                scheduled_base_fee_bps(FEE_MODE_CONSTANT, 30, 500, 10, 100, 50, slot).unwrap(),
                30
            );
        }
    }

    #[test]
    fn fee_linear_steps_down_and_floors() {
        // start 500, floor 30, -50 bps per 100 slots, cap 8 steps.
        let f = |slot| scheduled_base_fee_bps(FEE_MODE_LINEAR, 30, 500, 50, 100, 8, slot).unwrap();
        assert_eq!(f(0), 500); // step 0
        assert_eq!(f(99), 500); // still step 0
        assert_eq!(f(100), 450); // step 1
        assert_eq!(f(500), 250); // step 5
        assert_eq!(f(800), 100); // step 8 -> 500 - 50*8 = 100
        assert_eq!(f(900), 100); // step 9 capped at 8 -> still 100
        assert_eq!(f(100_000), 100); // capped, never drops further
    }

    #[test]
    fn fee_linear_clamps_to_floor() {
        // big reduction quickly crosses the floor; clamp holds it at 30.
        let f = scheduled_base_fee_bps(FEE_MODE_LINEAR, 30, 500, 200, 100, 50, 1_000).unwrap();
        assert_eq!(f, 30); // 500 - 200*10 = -1500 -> floor 30
    }

    #[test]
    fn fee_exponential_decays_and_is_monotonic() {
        // start 1000, floor 10, 50% off per period, cap 10 steps.
        let f = |slot| {
            scheduled_base_fee_bps(FEE_MODE_EXPONENTIAL, 10, 1000, 5000, 100, 10, slot).unwrap()
        };
        assert_eq!(f(0), 1000); // (1)^0
        assert_eq!(f(100), 500); // *0.5
        assert_eq!(f(200), 250); // *0.25
        assert_eq!(f(300), 125);
        // monotonic non-increasing
        let mut prev = u16::MAX;
        for s in (0..1500).step_by(50) {
            let cur = f(s);
            assert!(cur <= prev, "not monotonic at slot {s}: {cur} > {prev}");
            prev = cur;
        }
        // deep in time -> clamped to floor.
        assert_eq!(f(1_000_000), 10);
    }

    #[test]
    fn fee_invalid_mode_errors() {
        assert!(scheduled_base_fee_bps(7, 30, 500, 10, 100, 8, 100).is_err());
    }

    // --- dynamic (volatility) fee ---

    #[test]
    fn price_move_is_relative_bps() {
        // +10% move on the sqrt price = 1000 bps.
        assert_eq!(price_move_bps(100 * ONE, 110 * ONE), 1000);
        // symmetric on the way down.
        assert_eq!(price_move_bps(100 * ONE, 90 * ONE), 1000);
        // no anchor -> 0 (avoids div by zero).
        assert_eq!(price_move_bps(0, 100 * ONE), 0);
    }

    #[test]
    fn volatility_decays_with_idle_time() {
        let va = 10_000u128;
        // within filter window -> unchanged.
        assert_eq!(decayed_volatility_reference(va, 5, 10, 100, 5000), va);
        // between filter and decay -> scaled by reduction factor (50%).
        assert_eq!(decayed_volatility_reference(va, 50, 10, 100, 5000), 5_000);
        // past decay -> fully reset.
        assert_eq!(decayed_volatility_reference(va, 200, 10, 100, 5000), 0);
    }

    #[test]
    fn volatility_accumulates_and_caps() {
        assert_eq!(accumulate_volatility(1_000, 500, 100_000), 1_500);
        // capped at max_va.
        assert_eq!(accumulate_volatility(90_000, 50_000, 100_000), 100_000);
    }

    #[test]
    fn dynamic_fee_rises_with_volatility_and_clamps() {
        // control chosen so va=1000 -> 1000^2 * 1000 / 1e9 = 1 bps.
        let f = |va| dynamic_fee_bps(va, 1_000, 500);
        assert_eq!(f(0), 0);
        assert_eq!(f(1_000), 1);
        assert_eq!(f(10_000), 100); // 1e8 * 1e3 / 1e9 = 100
                                    // grows quadratically, then clamps to max_dynamic (500).
        assert_eq!(f(30_000), 500); // 9e8*1e3/1e9 = 900 -> clamp 500
                                    // zero control disables.
        assert_eq!(dynamic_fee_bps(50_000, 0, 500), 0);
    }

    #[test]
    fn compute_dynamic_fee_window_semantics() {
        // params: filter 10, decay 100, reduction 50%, max_va 100k, control 1000, max 500.
        let p = |sqrt_now, anchor, acc, vr, elapsed| {
            compute_dynamic_fee(
                sqrt_now, anchor, acc, vr, elapsed, 10, 100, 5000, 100_000, 1_000, 500,
            )
        };

        // In-window (elapsed < filter): anchor/reference fixed, va = ref + drift.
        let s = p(110 * ONE, 100 * ONE, 0, 0, 5);
        assert_eq!((s.dynamic_fee_bps, s.volatility_accumulator), (1, 1000));
        assert_eq!(s.sqrt_price_reference, 100 * ONE);
        assert_eq!(s.volatility_reference, 0);

        // New window (filter <= elapsed < decay): re-anchor, reference = decayed
        // prior accumulator (2000 * 50% = 1000), drift 0 -> va = reference.
        let s = p(110 * ONE, 100 * ONE, 2_000, 0, 20);
        assert_eq!(s.volatility_reference, 1_000);
        assert_eq!(s.sqrt_price_reference, 110 * ONE);
        assert_eq!(s.volatility_accumulator, 1_000);

        // Idle past decay: reference resets to 0.
        let s = p(110 * ONE, 100 * ONE, 50_000, 9_999, 200);
        assert_eq!(s.volatility_reference, 0);
        assert_eq!(s.sqrt_price_reference, 110 * ONE);
    }

    #[test]
    fn in_window_drift_does_not_double_count() {
        // Same fixed anchor across rapid in-window swaps: the accumulator tracks
        // total drift from the anchor once, not the sum of per-swap legs.
        let f = |sqrt_now| {
            compute_dynamic_fee(
                sqrt_now,
                100 * ONE,
                0,
                0,
                5,
                10,
                100,
                5000,
                1_000_000,
                1_000,
                10_000,
            )
            .volatility_accumulator
        };
        assert_eq!(f(110 * ONE), 1000); // +10%
        assert_eq!(f(120 * ONE), 2000); // +20% from anchor (not 1000+2000)
    }

    proptest! {
        // Swaps never leave the band and never pay out more than the band's
        // reserve of the output token, in either direction (PartialFill so large
        // inputs clamp instead of reverting).
        #[test]
        fn swap_step_stays_in_band_and_bounded(
            l in 1_000u128..1_000_000_000u128,
            amount in 1u64..1_000_000_000u64,
            a_to_b in any::<bool>(),
            fee_bps in 0u16..1_000u16,
        ) {
            let dir = if a_to_b { SwapDirection::AToB } else { SwapDirection::BToA };
            let step = compute_swap_step(
                MID, l, LO, HI, dir, SwapMode::PartialFill, amount, fee_bps,
            );
            // Tiny amounts can floor output to zero -> handler-level ZeroAmount;
            // only assert invariants when a step is produced.
            if let Ok(s) = step {
                // Price stays within [LO, HI].
                prop_assert!(s.next_sqrt_price >= LO && s.next_sqrt_price <= HI);
                // Never consumes more than provided.
                prop_assert!(s.amount_in as u64 + s.amount_remaining == amount);
                // Output bounded by the band's reserve of the output token.
                let reserve = if a_to_b {
                    delta_b(l, Q64x64::from_bits(LO), Q64x64::from_bits(MID), Rounding::Down).unwrap()
                } else {
                    delta_a(l, Q64x64::from_bits(MID), Q64x64::from_bits(HI), Rounding::Down).unwrap()
                };
                prop_assert!(s.amount_out as u128 <= reserve);
                // Fee never exceeds the consumed input.
                prop_assert!(s.fee <= s.amount_in);
            }
        }
    }
}
