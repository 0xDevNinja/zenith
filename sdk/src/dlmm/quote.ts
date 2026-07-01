//! Exact swap quote for the liquidity book — a faithful TS simulation of the
//! on-chain `swap` bin walk, so the quoted output equals the realized amount.

import { mulDiv } from "../math/index.js";
import { Rounding } from "../math/rounding.js";
import type { BinArray, Bin, LbPair } from "./accounts.js";
import { BINS_PER_ARRAY } from "./constants.js";
import {
  BPS_DENOMINATOR,
  Direction,
  SwapMode,
  binPrice,
  computeVariableFee,
  fillExactIn,
  fillExactOut,
  splitProtocolFee,
  totalFeeBps,
} from "./math.js";

/// Same bound as the program's `MAX_BINS_PER_SWAP`.
const MAX_BINS_PER_SWAP = 500;

/// Thrown when the quote cannot be produced. When `neededBinArrayIndex` is set,
/// the walk needs a `BinArray` that was not supplied — fetch it and retry.
export class DlmmQuoteError extends Error {
  readonly neededBinArrayIndex?: number;
  constructor(message: string, neededBinArrayIndex?: number) {
    super(message);
    this.name = "DlmmQuoteError";
    this.neededBinArrayIndex = neededBinArrayIndex;
  }
}

export interface DlmmSwapQuote {
  /// Gross input the trader pays (includes the fee).
  amountIn: bigint;
  /// Output the trader receives.
  amountOut: bigint;
  /// Total fee charged (input token).
  fee: bigint;
  /// Total fee rate applied, bps (base + volatility surcharge).
  feeBps: number;
  /// Protocol share of the fee.
  protocolFee: bigint;
  /// LP share of the fee.
  lpFee: bigint;
  /// Number of bins the walk touched.
  binsCrossed: number;
  /// Active bin before the swap.
  startBinId: number;
  /// Active bin after the swap.
  endBinId: number;
  /// Min output (ExactIn) or max input (ExactOut) after slippage.
  otherAmountThreshold: bigint;
}

export interface QuoteSwapParams {
  pair: LbPair;
  /// The bin arrays the walk may cross (any order); a missing one throws with
  /// its index so the caller can fetch and retry.
  binArrays: BinArray[];
  /// Current slot (drives the volatility fee decay window).
  slot: bigint;
  direction: Direction;
  mode: SwapMode;
  /// Input (ExactIn) or desired output (ExactOut), raw token units.
  amount: bigint;
  /// Slippage tolerance in bps for the threshold (default 50 = 0.5%).
  slippageBps?: number;
}

function binAt(binArrays: BinArray[], binId: number): Bin {
  const arrayIndex = Math.floor(binId / BINS_PER_ARRAY);
  const arr = binArrays.find((a) => Number(a.index) === arrayIndex);
  if (!arr) {
    throw new DlmmQuoteError(`missing bin array for index ${arrayIndex}`, arrayIndex);
  }
  const slot = binId - arrayIndex * BINS_PER_ARRAY;
  return arr.bins[slot];
}

/// Quote a swap by replaying the on-chain bin walk against the supplied state.
export function quoteSwap(params: QuoteSwapParams): DlmmSwapQuote {
  const { pair, binArrays, slot, direction, mode, amount } = params;
  const slippageBps = params.slippageBps ?? 50;
  if (amount <= 0n) throw new DlmmQuoteError("amount must be positive");

  // --- fee rate from the PRE-swap active bin (mirrors the handler) ---
  const elapsed = slot > pair.lastUpdateSlot ? slot - pair.lastUpdateSlot : 0n;
  const feeState = computeVariableFee({
    activeBin: pair.activeBinId,
    indexReference: pair.indexReference,
    volatilityAccumulator: pair.volatilityAccumulator,
    volatilityReference: pair.volatilityReference,
    elapsed,
    filterPeriod: pair.filterPeriod,
    decayPeriod: pair.decayPeriod,
    reductionFactorBps: pair.volatilityReductionFactor,
    maxVa: pair.maxVolatilityAccumulator,
    binStep: pair.binStep,
    variableFeeControl: pair.variableFeeControl,
    maxDynamicFeeBps: pair.maxDynamicFeeBps,
  });
  const feeBps = totalFeeBps(pair.baseFeeBps, feeState.variableFeeBps);
  const feeBpsBig = BigInt(feeBps);
  const binStep = pair.binStep;

  // For ExactIn the fee comes off the input up front; the net walks the bins.
  let budget: bigint;
  if (mode === SwapMode.ExactIn) {
    const fee = mulDiv(amount, feeBpsBig, BPS_DENOMINATOR, Rounding.Up);
    if (fee === null) throw new DlmmQuoteError("fee overflow");
    const net = amount - fee;
    if (net <= 0n) throw new DlmmQuoteError("amount too small to cover fee");
    budget = net;
  } else {
    budget = amount;
  }

  // --- walk ---
  let totalIn = 0n;
  let totalOut = 0n;
  let cur = pair.activeBinId;
  let newActive = cur;
  let binsCrossed = 0;

  while (budget > 0n) {
    const price = binPrice(binStep, cur, Rounding.Down);
    if (price === null) throw new DlmmQuoteError("insufficient liquidity (price band edge)");
    const bin = binAt(binArrays, cur);
    const reserveOut = direction === Direction.XtoY ? bin.amountY : bin.amountX;

    const fill =
      mode === SwapMode.ExactIn
        ? fillExactIn(budget, reserveOut, price, direction)
        : fillExactOut(budget, reserveOut, price, direction);
    if (fill === null) throw new DlmmQuoteError("fill overflow");

    totalIn += fill.inUsed;
    totalOut += fill.out;
    budget -= mode === SwapMode.ExactIn ? fill.inUsed : fill.out;

    binsCrossed += 1;
    if (binsCrossed > MAX_BINS_PER_SWAP) {
      throw new DlmmQuoteError("swap crosses too many bins");
    }

    if (!fill.drained || budget === 0n) {
      newActive = cur;
      break;
    }
    cur = direction === Direction.XtoY ? cur - 1 : cur + 1;
    if (binPrice(binStep, cur, Rounding.Down) === null) {
      throw new DlmmQuoteError("insufficient liquidity");
    }
    newActive = cur;
  }

  if (budget !== 0n || totalIn === 0n || totalOut === 0n) {
    throw new DlmmQuoteError("insufficient liquidity");
  }

  // --- settle fee + gross by mode ---
  let amountIn: bigint;
  let amountOut: bigint;
  let fee: bigint;
  if (mode === SwapMode.ExactIn) {
    fee = mulDiv(amount, feeBpsBig, BPS_DENOMINATOR, Rounding.Up)!;
    amountIn = amount;
    amountOut = totalOut;
  } else {
    // gross = net + fee, fee taken on top of the net input.
    const f = mulDiv(totalIn, feeBpsBig, BPS_DENOMINATOR - feeBpsBig, Rounding.Up);
    if (f === null) throw new DlmmQuoteError("fee overflow");
    fee = f;
    amountIn = totalIn + fee;
    amountOut = amount;
  }

  const [protocolFee, lpFee] = splitProtocolFee(fee, pair.protocolFeeRate);

  const slip = BigInt(slippageBps);
  const otherAmountThreshold =
    mode === SwapMode.ExactIn
      ? mulDiv(amountOut, BPS_DENOMINATOR - slip, BPS_DENOMINATOR, Rounding.Down)!
      : mulDiv(amountIn, BPS_DENOMINATOR + slip, BPS_DENOMINATOR, Rounding.Up)!;

  return {
    amountIn,
    amountOut,
    fee,
    feeBps,
    protocolFee,
    lpFee,
    binsCrossed,
    startBinId: pair.activeBinId,
    endBinId: newActive,
    otherAmountThreshold,
  };
}
