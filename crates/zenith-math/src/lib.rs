//! Shared fixed-point math for Zenith.
//!
//! Provides the numeric primitives both programs depend on:
//! - Q64.64 fixed-point (sqrt price, bin price)
//! - 256-bit intermediates for deltas and fee accumulators
//! - mul-div with explicit rounding direction
//!
//! Implementation lands in milestone M0.

/// Rounding direction for fixed-point division.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    Up,
    Down,
}

/// Number of fractional bits in the Q64.64 representation.
pub const SCALE_OFFSET: u32 = 64;

// TODO(M0): Q64.64 type, U256 mul-div, sqrt, bin-price pow, unit tests.
