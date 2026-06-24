import { Rounding, U128_MAX } from "./rounding.js";

/// `num / den` with the rounding direction applied to a nonzero remainder.
/// `den` must be nonzero (caller-guaranteed). Mirrors `u256::div_round`.
function divRound(num: bigint, den: bigint, rounding: Rounding): bigint {
  const q = num / den;
  return rounding === Rounding.Up && num % den !== 0n ? q + 1n : q;
}

/// Narrow an arbitrary-precision result back to `u128`, or `null` if it does
/// not fit — the JS stand-in for the Rust `to_u128` / `narrow_512` overflow
/// check. All inputs to the helpers below are non-negative `u128`/`u256`-range
/// `bigint`s; bigint is exact, so the only failure modes are this narrowing
/// (overflow) and division by zero.
function narrowU128(x: bigint): bigint | null {
  return x > U128_MAX ? null : x;
}

/// `(a * b) / denom`, rounded as requested. `null` if `denom == 0` (div by
/// zero) or the quotient exceeds `u128` (overflow). Mirrors `mul_div`.
export function mulDiv(
  a: bigint,
  b: bigint,
  denom: bigint,
  rounding: Rounding,
): bigint | null {
  if (denom === 0n) return null;
  return narrowU128(divRound(a * b, denom, rounding));
}

/// `(a * b) >> shift`, rounded as requested (Q64.64 multiply uses `shift = 64`).
/// `null` if `shift >= 256` or the result exceeds `u128`. Mirrors `mul_shr`.
export function mulShr(
  a: bigint,
  b: bigint,
  shift: bigint,
  rounding: Rounding,
): bigint | null {
  if (shift >= 256n) return null;
  return narrowU128(divRound(a * b, 1n << shift, rounding));
}

/// `(a << shift) / denom`, rounded as requested (Q64.64 divide uses
/// `shift = 64`, reciprocal uses `shift = 128`). `null` if `denom == 0`,
/// `shift > 128`, or the result exceeds `u128`. Mirrors `shl_div`.
export function shlDiv(
  a: bigint,
  shift: bigint,
  denom: bigint,
  rounding: Rounding,
): bigint | null {
  if (denom === 0n) return null;
  if (shift > 128n) return null;
  return narrowU128(divRound(a << shift, denom, rounding));
}
