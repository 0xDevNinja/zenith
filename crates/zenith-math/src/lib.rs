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

mod q64;
pub use q64::Q64x64;

// TODO(M0): U256 mul-div helpers, sqrt, bin-price pow (separate issues).
