import { Q64 } from "./q64.js";
import { Rounding, SCALE_OFFSET } from "./rounding.js";
import { mulShr, shlDiv } from "./u256.js";

/// Floored integer square root of a non-negative `bigint` (`floor(sqrt(n))`).
/// Arbitrary precision, so it covers the `U256`/`U512` intermediates the Rust
/// `sqrt_u256` operates on. Floor sqrt is unique, so this matches the Rust
/// Newton iteration result exactly.
export function isqrt(n: bigint): bigint {
  if (n < 0n) throw new RangeError("isqrt of negative");
  if (n < 2n) return n;
  let x = n;
  let y = (x + 1n) >> 1n;
  while (y < x) {
    x = y;
    y = (x + n / x) >> 1n;
  }
  return x;
}

/// `sqrt_price` (Q64.64) for the price ratio `num / den` (`y / x`):
/// `floor(sqrt((num << 128) / den))`. Rounded down. `null` if `den == 0`.
export function sqrtPriceFromPrice(num: bigint, den: bigint): Q64 | null {
  if (den === 0n) return null;
  const scaled = (num << 128n) / den;
  return Q64.fromBits(isqrt(scaled));
}

/// Price ratio (Q64.64) implied by a `sqrt_price`: `price = sqrt_price^2`,
/// `bits = (sp^2) >> 64`. `null` if the price exceeds `u128`.
export function priceFromSqrtPrice(sqrtPrice: Q64, rounding: Rounding): Q64 | null {
  const v = mulShr(sqrtPrice.toBits(), sqrtPrice.toBits(), SCALE_OFFSET, rounding);
  return v === null ? null : Q64.fromBits(v);
}

/// Order two sqrt prices as `[lowBits, highBits]`.
function order(a: Q64, b: Q64): [bigint, bigint] {
  const x = a.toBits();
  const y = b.toBits();
  return x <= y ? [x, y] : [y, x];
}

/// `num / den` with rounding applied to a nonzero remainder; `den` nonzero.
function divRound(num: bigint, den: bigint, rounding: Rounding): bigint {
  const q = num / den;
  return rounding === Rounding.Up && num % den !== 0n ? q + 1n : q;
}

/// Narrow to `u128`, or `null` on overflow.
function narrowU128(x: bigint): bigint | null {
  return x > (1n << 128n) - 1n ? null : x;
}

/// Amount of token `x` (base) between two sqrt prices for liquidity `L`:
/// `L * 2^64 * (sp_hi - sp_lo) / (sp_lo * sp_hi)`. `null` on overflow or zero
/// price. Mirrors `delta_a`.
export function deltaA(
  liquidity: bigint,
  sqrtA: Q64,
  sqrtB: Q64,
  rounding: Rounding,
): bigint | null {
  const [lo, hi] = order(sqrtA, sqrtB);
  if (lo === 0n) return null;
  const diff = hi - lo;
  const num = (liquidity * diff) << SCALE_OFFSET;
  const den = lo * hi;
  return narrowU128(divRound(num, den, rounding));
}

/// Amount of token `y` (quote) between two sqrt prices for liquidity `L`:
/// `L * (sp_hi - sp_lo) / 2^64`. `null` on overflow. Mirrors `delta_b`.
export function deltaB(
  liquidity: bigint,
  sqrtA: Q64,
  sqrtB: Q64,
  rounding: Rounding,
): bigint | null {
  const [lo, hi] = order(sqrtA, sqrtB);
  const diff = hi - lo;
  return mulShr(liquidity, diff, SCALE_OFFSET, rounding);
}

/// Liquidity backed by `amount` of token `x` (inverse of [`deltaA`]):
/// `amount * sp_lo * sp_hi / (2^64 * (sp_hi - sp_lo))`. `null` on overflow or a
/// degenerate (zero-width) range. Mirrors `liquidity_from_amount_a`.
export function liquidityFromAmountA(
  amount: bigint,
  sqrtA: Q64,
  sqrtB: Q64,
  rounding: Rounding,
): bigint | null {
  const [lo, hi] = order(sqrtA, sqrtB);
  if (lo === hi) return null;
  const diff = hi - lo;
  const num = amount * lo * hi;
  const den = diff << SCALE_OFFSET;
  return narrowU128(divRound(num, den, rounding));
}

/// Liquidity backed by `amount` of token `y` (inverse of [`deltaB`]):
/// `amount * 2^64 / (sp_hi - sp_lo)`. `null` on overflow or a degenerate range.
/// Mirrors `liquidity_from_amount_b`.
export function liquidityFromAmountB(
  amount: bigint,
  sqrtA: Q64,
  sqrtB: Q64,
  rounding: Rounding,
): bigint | null {
  const [lo, hi] = order(sqrtA, sqrtB);
  if (lo === hi) return null;
  const diff = hi - lo;
  const num = amount << SCALE_OFFSET;
  return narrowU128(divRound(num, diff, rounding));
}

/// Next `sqrt_price` after adding/removing `amount` of token `x` (base). Adding
/// `x` lowers the price. Always rounds the price **up** (protocol-favoring on
/// both branches). `null` on overflow, zero liquidity, or a removal that would
/// empty the range. Mirrors `next_sqrt_price_from_amount_x`.
export function nextSqrtPriceFromAmountX(
  sqrtPrice: Q64,
  liquidity: bigint,
  amount: bigint,
  add: boolean,
): Q64 | null {
  if (liquidity === 0n) return null;
  const sp = sqrtPrice.toBits();
  const product = amount * sp;
  const lShifted = liquidity << SCALE_OFFSET;
  let den: bigint;
  if (add) {
    den = lShifted + product;
  } else {
    if (lShifted <= product) return null;
    den = lShifted - product;
  }
  const num = (liquidity * sp) << SCALE_OFFSET;
  const v = narrowU128(divRound(num, den, Rounding.Up));
  return v === null ? null : Q64.fromBits(v);
}

/// Next `sqrt_price` after adding/removing `amount` of token `y` (quote). Adding
/// `y` raises the price. Rounds **down** on add and **up** on remove (both
/// favor the pool). `null` on overflow, zero liquidity, or a removal that would
/// drive the price below zero. Mirrors `next_sqrt_price_from_amount_y`.
export function nextSqrtPriceFromAmountY(
  sqrtPrice: Q64,
  liquidity: bigint,
  amount: bigint,
  add: boolean,
): Q64 | null {
  if (liquidity === 0n) return null;
  const rounding = add ? Rounding.Down : Rounding.Up;
  const deltaBits = shlDiv(amount, SCALE_OFFSET, liquidity, rounding);
  if (deltaBits === null) return null;
  const delta = Q64.fromBits(deltaBits);
  return add ? sqrtPrice.checkedAdd(delta) : sqrtPrice.checkedSub(delta);
}
