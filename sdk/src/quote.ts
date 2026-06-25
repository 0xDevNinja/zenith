import type { Config, Pool } from "./coder/index.js";
import {
  computeSwapStep,
  priceFromSqrtPrice,
  Q64,
  Rounding,
  type SwapDirection,
  SwapMode,
  mulDiv,
} from "./math/index.js";
import { computeDynamicFee, scheduledBaseFeeBps } from "./math/fee.js";

const BPS = 10_000n;
const satSub = (a: bigint, b: bigint): bigint => (a > b ? a - b : 0n);

/// The fee a swap pays right now, broken into its base and dynamic parts.
export interface EffectiveFee {
  /// Base + dynamic, clamped strictly below 100% (what the swap actually uses).
  totalFeeBps: number;
  /// Scheduled base fee for the pool's age.
  baseFeeBps: number;
  /// Volatility surcharge on top.
  dynamicFeeBps: number;
}

/// Derive the effective swap fee from live pool + config state at `slot`.
/// Mirrors exactly how the `swap` handler computes its fee (scheduler base
/// using slots since activation, dynamic surcharge using slots since the last
/// volatility update, summed and clamped below 10000).
export function effectiveFeeBps(config: Config, pool: Pool, slot: bigint): EffectiveFee {
  const baseFeeBps = scheduledBaseFeeBps({
    mode: config.feeSchedulerMode,
    baseFeeBps: config.baseFeeBps,
    cliffFeeBps: config.cliffFeeBps,
    reductionFactor: config.reductionFactor,
    feePeriod: config.feePeriod,
    maxFeeSteps: config.maxFeeSteps,
    elapsedSlots: satSub(slot, pool.activationPoint),
  });

  const vol = computeDynamicFee({
    sqrtPrice: pool.sqrtPrice,
    sqrtPriceReference: pool.sqrtPriceReference,
    volatilityAccumulator: pool.volatilityAccumulator,
    volatilityReference: pool.volatilityReference,
    elapsed: satSub(slot, pool.lastVolatilityUpdate),
    filterPeriod: config.filterPeriod,
    decayPeriod: config.decayPeriod,
    reductionFactorBps: config.volatilityReductionFactor,
    maxVa: config.maxVolatilityAccumulator,
    variableFeeControl: config.variableFeeControl,
    maxDynamicFeeBps: config.maxDynamicFeeBps,
  });

  const total = Math.min(baseFeeBps + vol.dynamicFeeBps, Number(BPS) - 1);
  return { totalFeeBps: total, baseFeeBps, dynamicFeeBps: vol.dynamicFeeBps };
}

/// A pre-trade quote: amounts, the fee applied, price impact, and a
/// slippage-protected threshold to hand to the `swap` instruction.
export interface SwapQuote {
  direction: SwapDirection;
  mode: SwapMode;
  /// Gross input consumed (raw token units).
  amountIn: bigint;
  /// Output paid to the trader (raw token units).
  amountOut: bigint;
  /// Effective fee applied, and its breakdown.
  fee: EffectiveFee;
  /// Fee taken from the input, in input-token units.
  feeAmount: bigint;
  /// Unspent input returned (only nonzero for PartialFill).
  amountRemaining: bigint;
  /// Pool sqrt-price after the trade (Q64.64 raw bits).
  nextSqrtPrice: bigint;
  /// How far the marginal price moved, in bps (`null` if the price overflows).
  priceImpactBps: bigint | null;
  /// Slippage tolerance used to derive the threshold below.
  slippageBps: number;
  /// Floor on output for ExactIn/PartialFill (raw units), else undefined.
  minAmountOut?: bigint;
  /// Ceiling on input for ExactOut (raw units), else undefined.
  maxAmountIn?: bigint;
  /// The value to pass as the swap instruction's `other_amount_threshold`.
  otherAmountThreshold: bigint;
}

/// Build a swap quote from decoded pool + config state.
///
/// Runs the exact on-chain fee + swap-step math, so `amountIn`/`amountOut`
/// match what the program would compute at the same `slot` and pool state.
/// Throws the same `SwapError` the program would revert with if the trade is
/// not fillable (e.g. it would leave the price band in a non-PartialFill mode).
///
/// `slippageBps` (default 50 = 0.5%) yields `minAmountOut` (ExactIn /
/// PartialFill) or `maxAmountIn` (ExactOut). Because the quote uses the same
/// state the swap will, the on-chain slippage check passes at this threshold.
export function quoteSwap(params: {
  pool: Pool;
  config: Config;
  slot: bigint;
  direction: SwapDirection;
  mode: SwapMode;
  amount: bigint;
  slippageBps?: number;
}): SwapQuote {
  const { pool, config, slot, direction, mode, amount } = params;
  const slippageBps = params.slippageBps ?? 50;
  if (slippageBps < 0 || slippageBps > Number(BPS)) {
    throw new RangeError(`slippageBps out of range: ${slippageBps}`);
  }

  const fee = effectiveFeeBps(config, pool, slot);

  const step = computeSwapStep({
    sqrtPrice: pool.sqrtPrice,
    liquidity: pool.liquidity,
    sqrtMin: pool.sqrtMinPrice,
    sqrtMax: pool.sqrtMaxPrice,
    direction,
    mode,
    amount,
    feeBps: fee.totalFeeBps,
  });

  // Price impact: how far the marginal (sqrt-derived) price moved.
  const p0 = priceFromSqrtPrice(Q64.fromBits(pool.sqrtPrice), Rounding.Down);
  const p1 = priceFromSqrtPrice(Q64.fromBits(step.nextSqrtPrice), Rounding.Down);
  let priceImpactBps: bigint | null = null;
  if (p0 !== null && p1 !== null && p0.toBits() !== 0n) {
    const a = p0.toBits();
    const bbits = p1.toBits();
    const diff = bbits > a ? bbits - a : a - bbits;
    priceImpactBps = mulDiv(diff, BPS, a, Rounding.Down);
  }

  const quote: SwapQuote = {
    direction,
    mode,
    amountIn: step.amountIn,
    amountOut: step.amountOut,
    fee,
    feeAmount: step.fee,
    amountRemaining: step.amountRemaining,
    nextSqrtPrice: step.nextSqrtPrice,
    priceImpactBps,
    slippageBps,
    otherAmountThreshold: 0n,
  };

  const bps = BigInt(slippageBps);
  if (mode === SwapMode.ExactOut) {
    // Tolerate paying up to slippage more input (round up — protective).
    const maxIn = mulDiv(step.amountIn, BPS + bps, BPS, Rounding.Up) ?? step.amountIn;
    quote.maxAmountIn = maxIn;
    quote.otherAmountThreshold = maxIn;
  } else {
    // Accept down to slippage less output (round down — protective).
    const minOut = mulDiv(step.amountOut, BPS - bps, BPS, Rounding.Down) ?? 0n;
    quote.minAmountOut = minOut;
    quote.otherAmountThreshold = minOut;
  }

  return quote;
}
