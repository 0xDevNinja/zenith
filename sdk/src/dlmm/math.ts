//! Bit-exact TypeScript port of the DLMM on-chain swap and fee math
//! (`bin_price.rs`, `swap_math.rs`, `fee.rs`). Every lossy step takes the same
//! explicit rounding the program uses, so a quote equals the on-chain result.

import { mulDiv } from "../math/index.js";
import { pow } from "../math/pow.js";
import { Q64 } from "../math/q64.js";
import { Rounding, U64_MAX } from "../math/rounding.js";

/// Swap direction (mirrors the program's `Direction`).
export enum Direction {
  /// Sell X for Y — consumes a bin's Y reserve; active bin moves down.
  XtoY = 0,
  /// Sell Y for X — consumes a bin's X reserve; active bin moves up.
  YtoX = 1,
}

/// Whether the caller fixed the input or the output amount.
export enum SwapMode {
  ExactIn = 0,
  ExactOut = 1,
}

export const BPS_DENOMINATOR = 10_000n;
/// Denominator for `variable = va^2 * control / 1e9`.
export const DYNAMIC_FEE_DENOMINATOR = 1_000_000_000n;
/// Total fee is clamped strictly below 100%.
export const MAX_FEE_BPS = 10_000;
/// Bin-price band (see `bin_price.rs`): `[2^-32, 2^32]` in Q64.64 bits.
const MIN_PRICE_BITS = 1n << 32n;
const MAX_PRICE_BITS = 1n << 96n;
const MAX_BIN_STEP_BPS = 10_000;

// ---------------------------------------------------------------------------
// bin price
// ---------------------------------------------------------------------------

/// Price of bin `binId` for `binStep` bps: `(1 + binStep/10000)^binId` in
/// Q64.64. `null` if `binStep` is out of range or the price leaves the band.
export function binPrice(binStep: number, binId: number, rounding: Rounding): Q64 | null {
  if (binStep === 0 || binStep > MAX_BIN_STEP_BPS) return null;
  const step = Q64.fromRatio(BigInt(binStep), 10_000n, rounding);
  if (step === null) return null;
  const base = Q64.ONE.checkedAdd(step);
  if (base === null) return null;
  const price = pow(base, binId, rounding);
  if (price === null) return null;
  const bits = price.toBits();
  if (bits < MIN_PRICE_BITS || bits > MAX_PRICE_BITS) return null;
  return price;
}

// ---------------------------------------------------------------------------
// single-bin constant-sum fill
// ---------------------------------------------------------------------------

export interface BinFill {
  inUsed: bigint;
  out: bigint;
  drained: boolean;
}

function u64(v: bigint | null): bigint | null {
  if (v === null || v < 0n || v > U64_MAX) return null;
  return v;
}

/// Output for spending `input` at `price`: X->Y `in*P`, Y->X `in/P`.
function outForIn(input: bigint, price: Q64, dir: Direction, r: Rounding): bigint | null {
  return u64(dir === Direction.XtoY ? price.mulInt(input, r) : price.divInt(input, r));
}

/// Input to produce `output` at `price`: X->Y `out/P`, Y->X `out*P`.
function inForOut(output: bigint, price: Q64, dir: Direction, r: Rounding): bigint | null {
  return u64(dir === Direction.XtoY ? price.divInt(output, r) : price.mulInt(output, r));
}

/// Fill a bin with up to `inAvail` input. `reserveOut` is the bin's reserve of
/// the output token. Output rounds down; the drain input rounds up.
export function fillExactIn(
  inAvail: bigint,
  reserveOut: bigint,
  price: Q64,
  dir: Direction,
): BinFill | null {
  if (inAvail === 0n || reserveOut === 0n) {
    return { inUsed: 0n, out: 0n, drained: reserveOut === 0n };
  }
  const inToDrain = inForOut(reserveOut, price, dir, Rounding.Up);
  if (inToDrain === null) return null;
  if (inAvail >= inToDrain) {
    return { inUsed: inToDrain, out: reserveOut, drained: true };
  }
  const out = outForIn(inAvail, price, dir, Rounding.Down);
  if (out === null) return null;
  return { inUsed: inAvail, out, drained: false };
}

/// Fill a bin to produce up to `outNeed` output. Input rounds up.
export function fillExactOut(
  outNeed: bigint,
  reserveOut: bigint,
  price: Q64,
  dir: Direction,
): BinFill | null {
  const outTake = outNeed < reserveOut ? outNeed : reserveOut;
  if (outTake === 0n) {
    return { inUsed: 0n, out: 0n, drained: reserveOut === 0n };
  }
  const inUsed = inForOut(outTake, price, dir, Rounding.Up);
  if (inUsed === null) return null;
  return { inUsed, out: outTake, drained: outTake === reserveOut };
}

// ---------------------------------------------------------------------------
// dynamic (volatility) fee
// ---------------------------------------------------------------------------

/// Price move (bps) for the active bin sitting `|active - reference|` bins away.
export function binMoveBps(indexReference: number, activeBin: number, binStep: number): bigint {
  return BigInt(Math.abs(activeBin - indexReference)) * BigInt(binStep);
}

/// Decay the stored accumulator by idle time into the next window's reference.
export function decayedVolatilityReference(
  accumulator: bigint,
  elapsed: bigint,
  filterPeriod: number,
  decayPeriod: number,
  reductionFactorBps: number,
): bigint {
  if (elapsed >= BigInt(decayPeriod)) return 0n;
  if (elapsed >= BigInt(filterPeriod)) {
    return mulDiv(accumulator, BigInt(reductionFactorBps), BPS_DENOMINATOR, Rounding.Down) ?? 0n;
  }
  return accumulator;
}

/// New accumulator after a move: `reference + move`, capped at `maxVa`.
export function accumulateVolatility(reference: bigint, moveBps: bigint, maxVa: number): bigint {
  const v = reference + moveBps;
  const cap = BigInt(maxVa);
  return v < cap ? v : cap;
}

/// Variable surcharge (bps): `va^2 * control / 1e9`, capped at `maxDynamic`.
export function variableFeeBps(va: bigint, variableFeeControl: number, maxDynamicFeeBps: number): number {
  if (variableFeeControl === 0) return 0;
  const fee = (va * va * BigInt(variableFeeControl)) / DYNAMIC_FEE_DENOMINATOR;
  const cap = BigInt(maxDynamicFeeBps);
  return Number(fee < cap ? fee : cap);
}

/// `base + variable`, clamped strictly below 100%.
export function totalFeeBps(baseFeeBps: number, variableFee: number): number {
  return Math.min(baseFeeBps + variableFee, MAX_FEE_BPS - 1);
}

/// Split a swap fee into `[protocol, lp]` by `protocolFeeRate` (bps); protocol
/// rounds down and the two parts sum to exactly `totalFee`.
export function splitProtocolFee(totalFee: bigint, protocolFeeRate: number): [bigint, bigint] {
  const protocol = (totalFee * BigInt(protocolFeeRate)) / BPS_DENOMINATOR;
  return [protocol, totalFee - protocol];
}

export interface VariableFeeState {
  variableFeeBps: number;
  volatilityAccumulator: bigint;
  volatilityReference: bigint;
  indexReference: number;
}

/// Fold the pre-swap active bin into the volatility state and derive the
/// variable surcharge (mirrors `fee::compute_variable_fee`).
export function computeVariableFee(params: {
  activeBin: number;
  indexReference: number;
  volatilityAccumulator: bigint;
  volatilityReference: bigint;
  elapsed: bigint;
  filterPeriod: number;
  decayPeriod: number;
  reductionFactorBps: number;
  maxVa: number;
  binStep: number;
  variableFeeControl: number;
  maxDynamicFeeBps: number;
}): VariableFeeState {
  const newWindow = params.elapsed >= BigInt(params.filterPeriod);
  const reference = newWindow
    ? decayedVolatilityReference(
        params.volatilityAccumulator,
        params.elapsed,
        params.filterPeriod,
        params.decayPeriod,
        params.reductionFactorBps,
      )
    : params.volatilityReference;
  const refBin = newWindow ? params.activeBin : params.indexReference;
  const moveBps = binMoveBps(refBin, params.activeBin, params.binStep);
  const va = accumulateVolatility(reference, moveBps, params.maxVa);
  const fee = variableFeeBps(va, params.variableFeeControl, params.maxDynamicFeeBps);
  return {
    variableFeeBps: fee,
    volatilityAccumulator: va,
    volatilityReference: reference,
    indexReference: refBin,
  };
}
