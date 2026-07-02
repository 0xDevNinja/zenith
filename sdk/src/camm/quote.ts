//! Swap quoting for the constant-product engine. Replays the program's
//! `compute_swap` against the pool's current curve reserves (raw, matching the
//! on-chain swap which does not price in pending yield), then derives a
//! slippage-protected threshold.

import type { Pool } from "./accounts.js";
import { BPS_DENOMINATOR } from "./constants.js";
import { CammQuoteError, computeSwap, Direction, SwapMode, type SwapResult } from "./math.js";

const U64_MAX = (1n << 64n) - 1n;

export interface CammSwapQuote extends SwapResult {
  direction: Direction;
  mode: SwapMode;
  /// Minimum output to accept (ExactIn); equals `amountOut` for ExactOut.
  minAmountOut: bigint;
  /// Maximum input to spend (ExactOut, clamped to u64); equals `amountIn` for ExactIn.
  maxAmountIn: bigint;
  /// The value to pass as the instruction's `other_amount_threshold`.
  otherAmountThreshold: bigint;
}

export interface QuoteSwapParams {
  pool: Pool;
  direction: Direction;
  mode: SwapMode;
  amount: bigint;
  /// Slippage tolerance in basis points (default 50 = 0.5%).
  slippageBps?: number;
}

/// Quote a swap against `pool`. Throws [`CammQuoteError`] if the trade cannot be
/// resolved (zero amount, output exceeds reserve, overflow).
export function quoteSwap(p: QuoteSwapParams): CammSwapQuote {
  const slippageBps = BigInt(p.slippageBps ?? 50);
  const [reserveIn, reserveOut] =
    p.direction === Direction.AtoB
      ? [p.pool.reserveA, p.pool.reserveB]
      : [p.pool.reserveB, p.pool.reserveA];

  const r = computeSwap(
    reserveIn,
    reserveOut,
    p.pool.baseFeeBps,
    p.pool.protocolFeeRate,
    p.mode,
    p.amount,
  );

  let minAmountOut = r.amountOut;
  let maxAmountIn = r.amountIn;
  let otherAmountThreshold: bigint;
  if (p.mode === SwapMode.ExactIn) {
    // Accept at least amountOut minus slippage.
    minAmountOut = r.amountOut - (r.amountOut * slippageBps) / BPS_DENOMINATOR;
    otherAmountThreshold = minAmountOut;
  } else {
    // Spend at most amountIn plus slippage (clamped to u64).
    const bumped = r.amountIn + (r.amountIn * slippageBps) / BPS_DENOMINATOR;
    maxAmountIn = bumped > U64_MAX ? U64_MAX : bumped;
    otherAmountThreshold = maxAmountIn;
  }

  return { ...r, direction: p.direction, mode: p.mode, minAmountOut, maxAmountIn, otherAmountThreshold };
}

export { CammQuoteError };
