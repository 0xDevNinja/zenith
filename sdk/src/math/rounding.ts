/// Rounding direction for a lossy fixed-point operation. Mirrors the Rust
/// `zenith_math::Rounding` — callers pick the protocol-favoring side explicitly,
/// there is no implicit default.
export enum Rounding {
  /// Round toward 0 (floor of the exact real result).
  Down = 0,
  /// Round toward +infinity (ceil of the exact real result).
  Up = 1,
}

/// Number of fractional bits in the Q64.64 representation.
export const SCALE_OFFSET = 64n;

/// `2^64`, the value of `1.0` in Q64.64.
export const ONE_Q64 = 1n << SCALE_OFFSET;

/// Largest `u128` (the narrowing ceiling for every fixed-point result).
export const U128_MAX = (1n << 128n) - 1n;

/// Largest `u64` (the token-amount ceiling).
export const U64_MAX = (1n << 64n) - 1n;
