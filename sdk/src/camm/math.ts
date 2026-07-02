//! Bit-exact TypeScript port of the constant-product engine's on-chain math:
//! the `x*y=k` curve and LP-share functions (from `zenith-math`), the swap-fee
//! arithmetic (`fee.rs`), and the mock-yield accrual (`yield_math.rs`). The
//! curve + LP functions are verified against golden Rust vectors in the tests;
//! `computeSwap` mirrors the program's `swap.rs` so a quote matches the realized
//! trade exactly.
//!
//! Functions that can hit a Rust `MathError` return `null` (the same value the
//! golden vectors encode); `computeSwap` throws a typed [`CammQuoteError`].

import { isqrt, mulDiv, Rounding } from "../math/index.js";
import { BPS_DENOMINATOR, MINIMUM_LIQUIDITY, YIELD_SCALE } from "./constants.js";

// ---------------------------------------------------------------------------
// curve (x*y=k)
// ---------------------------------------------------------------------------

/// `out = reserveOut * amountIn / (reserveIn + amountIn)`, rounded down.
/// `null` if `reserveIn == 0` (mirrors the program's DivByZero guard).
export function outGivenIn(reserveIn: bigint, reserveOut: bigint, amountIn: bigint): bigint | null {
  if (amountIn === 0n) return 0n;
  if (reserveIn === 0n) return null;
  return mulDiv(reserveOut, amountIn, reserveIn + amountIn, Rounding.Down);
}

/// `in = reserveIn * amountOut / (reserveOut - amountOut)`, rounded up.
/// `null` if `amountOut >= reserveOut` (unsatisfiable).
export function inGivenOut(
  reserveIn: bigint,
  reserveOut: bigint,
  amountOut: bigint,
): bigint | null {
  if (amountOut === 0n) return 0n;
  if (amountOut >= reserveOut) return null;
  return mulDiv(reserveIn, amountOut, reserveOut - amountOut, Rounding.Up);
}

// ---------------------------------------------------------------------------
// LP shares
// ---------------------------------------------------------------------------

/// First-deposit shares: the geometric mean `sqrt(a*b)`.
export function initialShares(amountA: bigint, amountB: bigint): bigint {
  return isqrt(amountA * amountB);
}

/// Proportional shares for a subsequent deposit: `min` of the two ratio-implied
/// amounts, floored. `null` if either reserve is zero.
export function sharesFromDeposit(
  amountA: bigint,
  amountB: bigint,
  reserveA: bigint,
  reserveB: bigint,
  supply: bigint,
): bigint | null {
  const sa = mulDiv(amountA, supply, reserveA, Rounding.Down);
  const sb = mulDiv(amountB, supply, reserveB, Rounding.Down);
  if (sa === null || sb === null) return null;
  return sa < sb ? sa : sb;
}

/// Tokens returned for burning `shares`: `reserve * shares / supply`, floored on
/// withdrawal. `0` when `supply == 0`.
export function tokensForShares(
  shares: bigint,
  reserve: bigint,
  supply: bigint,
  rounding: Rounding = Rounding.Down,
): bigint | null {
  if (supply === 0n) return 0n;
  return mulDiv(shares, reserve, supply, rounding);
}

/// Token B that must accompany `amountA` to preserve the ratio, rounded up.
/// `null` on an empty pool.
export function matchingAmount(amountA: bigint, reserveA: bigint, reserveB: bigint): bigint | null {
  return mulDiv(amountA, reserveB, reserveA, Rounding.Up);
}

// ---------------------------------------------------------------------------
// fee
// ---------------------------------------------------------------------------

/// Fee charged on an exact-input swap: `ceil(amountIn * feeBps / 10000)`.
export function feeOnInput(amountIn: bigint, feeBps: number): bigint {
  return mulDiv(amountIn, BigInt(feeBps), BPS_DENOMINATOR, Rounding.Up) ?? 0n;
}

/// Gross input for an exact-output swap so the post-fee remainder is `netIn`:
/// `ceil(netIn * 10000 / (10000 - feeBps))`. `null` on overflow.
export function grossInputForNet(netIn: bigint, feeBps: number): bigint | null {
  return mulDiv(netIn, BPS_DENOMINATOR, BPS_DENOMINATOR - BigInt(feeBps), Rounding.Up);
}

/// Split a fee into `[protocol, lp]`: protocol takes `floor(fee * rate / 10000)`,
/// the remainder is the LP share (dust to LPs).
export function splitProtocolFee(fee: bigint, rate: number): [bigint, bigint] {
  const protocol = mulDiv(fee, BigInt(rate), BPS_DENOMINATOR, Rounding.Down) ?? 0n;
  return [protocol, fee - protocol];
}

// ---------------------------------------------------------------------------
// mock yield
// ---------------------------------------------------------------------------

/// Yield accrued on `deployed` over `elapsed` slots at `rate` (scaled by
/// `YIELD_SCALE`): `deployed * rate * elapsed / YIELD_SCALE`, rounded down.
export function accruedYield(deployed: bigint, rate: bigint, elapsed: bigint): bigint | null {
  if (deployed === 0n || rate === 0n || elapsed === 0n) return 0n;
  return mulDiv(deployed, rate * elapsed, YIELD_SCALE, Rounding.Down);
}

/// Principal eligible to be deployed: `reserve - ceil(reserve * bufferBps / 10000)`.
export function deployable(reserve: bigint, bufferBps: number): bigint {
  const buffer = mulDiv(reserve, BigInt(bufferBps), BPS_DENOMINATOR, Rounding.Up) ?? 0n;
  return reserve - buffer;
}

// ---------------------------------------------------------------------------
// swap
// ---------------------------------------------------------------------------

/// Trade direction across the pool's two tokens (matches the program enum).
export enum Direction {
  AtoB = 0,
  BtoA = 1,
}

/// Whether `amount` is the exact input or the exact output (matches the enum).
export enum SwapMode {
  ExactIn = 0,
  ExactOut = 1,
}

export type CammQuoteErrorCode =
  | "ZeroAmount"
  | "InsufficientReserve"
  | "MathOverflow";

/// Thrown by `computeSwap` when the trade cannot be resolved, carrying the
/// program error code it maps to.
export class CammQuoteError extends Error {
  constructor(readonly code: CammQuoteErrorCode) {
    super(`camm swap error: ${code}`);
    this.name = "CammQuoteError";
  }
}

/// Fully-resolved swap: `amountIn == netIn + fee`, `fee == protocolFee + lpFee`.
export interface SwapResult {
  amountIn: bigint;
  amountOut: bigint;
  fee: bigint;
  protocolFee: bigint;
  lpFee: bigint;
}

/// Resolve a swap against a `(reserveIn, reserveOut)` curve — the exact mirror
/// of the program's `compute_swap`.
export function computeSwap(
  reserveIn: bigint,
  reserveOut: bigint,
  baseFeeBps: number,
  protocolFeeRate: number,
  mode: SwapMode,
  amount: bigint,
): SwapResult {
  if (amount === 0n) throw new CammQuoteError("ZeroAmount");
  if (mode === SwapMode.ExactIn) {
    const fee = feeOnInput(amount, baseFeeBps);
    const netIn = amount - fee;
    const out = outGivenIn(reserveIn, reserveOut, netIn);
    if (out === null) throw new CammQuoteError("MathOverflow");
    const [protocolFee, lpFee] = splitProtocolFee(fee, protocolFeeRate);
    return { amountIn: amount, amountOut: out, fee, protocolFee, lpFee };
  }
  // ExactOut
  if (amount >= reserveOut) throw new CammQuoteError("InsufficientReserve");
  const net = inGivenOut(reserveIn, reserveOut, amount);
  if (net === null) throw new CammQuoteError("MathOverflow");
  const amountIn = grossInputForNet(net, baseFeeBps);
  if (amountIn === null) throw new CammQuoteError("MathOverflow");
  const fee = amountIn - net;
  const [protocolFee, lpFee] = splitProtocolFee(fee, protocolFeeRate);
  return { amountIn, amountOut: amount, fee, protocolFee, lpFee };
}
