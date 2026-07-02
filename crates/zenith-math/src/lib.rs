//! Shared fixed-point math for Zenith.
//!
//! Provides the numeric primitives both programs depend on:
//! - Q64.64 fixed-point (sqrt price, bin price)
//! - 256-bit intermediates for deltas and fee accumulators
//! - mul-div with explicit rounding direction
//!
//! Built incrementally over milestone M0.

/// Rounding direction for a lossy fixed-point operation.
///
/// Callers pick the protocol-favoring side explicitly; there is no implicit
/// default, so money never rounds the wrong way by accident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    /// Round toward +∞ (ceil of the exact real result).
    Up,
    /// Round toward 0 (floor of the exact real result).
    Down,
}

/// Number of fractional bits in the Q64.64 representation.
pub const SCALE_OFFSET: u32 = 64;

mod bin_price;
mod constant_product;
mod q64;
mod sqrt_price;
mod tick;
mod u256;

pub use bin_price::{bin_price, pow, MAX_BIN_STEP_BPS, MAX_PRICE_BITS, MIN_PRICE_BITS};
pub use constant_product::{
    in_given_out, initial_shares, matching_amount, out_given_in, shares_from_deposit,
    tokens_for_shares, MINIMUM_LIQUIDITY,
};
pub use q64::Q64x64;
pub use sqrt_price::{
    delta_a, delta_b, liquidity_from_amount_a, liquidity_from_amount_b,
    next_sqrt_price_from_amount_x, next_sqrt_price_from_amount_y, price_from_sqrt_price,
    sqrt_price_from_price, sqrt_u128, sqrt_u256,
};
pub use tick::{
    cross_tick_liquidity, fee_growth_inside, sqrt_price_at_tick, tick_at_sqrt_price,
    valid_tick_range, MAX_TICK, MIN_TICK,
};
pub use u256::{mul_div, mul_shr, shl_div, MathError, MathResult};

// TODO(M0): shared test vectors (#12).
