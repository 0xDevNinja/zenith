import { pow } from "./pow.js";
import { Q64 } from "./q64.js";
import { Rounding, SCALE_OFFSET, U128_MAX } from "./rounding.js";
import { mulDiv, shlDiv } from "./u256.js";

/// Basis-point denominator (100%).
export const BPS_DENOMINATOR = 10_000n;
/// Denominator for the dynamic-fee formula `va^2 * control / 1e9`.
export const DYNAMIC_FEE_DENOMINATOR = 1_000_000_000n;

/// Fee scheduler modes (stored as `u8` on the config).
export enum FeeMode {
  Constant = 0,
  Linear = 1,
  Exponential = 2,
}

/// Thrown when a fee parameter set is invalid (mirrors the program reverting).
export class FeeError extends Error {
  constructor(msg: string) {
    super(msg);
    this.name = "FeeError";
  }
}

const satAddU128 = (a: bigint, b: bigint): bigint => {
  const s = a + b;
  return s > U128_MAX ? U128_MAX : s;
};
const satMulU128 = (a: bigint, b: bigint): bigint => {
  const p = a * b;
  return p > U128_MAX ? U128_MAX : p;
};
const clamp = (x: bigint, lo: bigint, hi: bigint): bigint => (x < lo ? lo : x > hi ? hi : x);

/// Current base swap fee (bps) for a scheduler at `elapsedSlots` since pool
/// creation. Bit-exact port of `scheduled_base_fee_bps`. Throws `FeeError` on an
/// unknown mode or a math overflow in the exponential path.
export function scheduledBaseFeeBps(params: {
  mode: number;
  baseFeeBps: number;
  cliffFeeBps: number;
  reductionFactor: number;
  feePeriod: bigint;
  maxFeeSteps: number;
  elapsedSlots: bigint;
}): number {
  const { mode, baseFeeBps, cliffFeeBps, reductionFactor, feePeriod, maxFeeSteps, elapsedSlots } =
    params;

  if (mode === FeeMode.Constant) return baseFeeBps;

  const steps =
    feePeriod === 0n
      ? 0n
      : (() => {
          const s = elapsedSlots / feePeriod;
          return s < BigInt(maxFeeSteps) ? s : BigInt(maxFeeSteps);
        })();
  const cliff = BigInt(cliffFeeBps);
  const floor = BigInt(baseFeeBps);

  let raw: bigint;
  if (mode === FeeMode.Linear) {
    const dec = BigInt(reductionFactor) * steps; // saturating: values are small
    raw = dec > cliff ? 0n : cliff - dec;
  } else if (mode === FeeMode.Exponential) {
    // (1 - reduction/10000) in Q64.64, raised to `steps`.
    const baseBits = shlDiv(BPS_DENOMINATOR - BigInt(reductionFactor), SCALE_OFFSET, BPS_DENOMINATOR, Rounding.Down);
    if (baseBits === null) throw new FeeError("exponential base overflow");
    const factor = pow(Q64.fromBits(baseBits), Number(steps), Rounding.Down);
    if (factor === null) throw new FeeError("exponential pow overflow");
    const r = factor.mulInt(cliff, Rounding.Down);
    if (r === null) throw new FeeError("exponential mul overflow");
    raw = r;
  } else {
    throw new FeeError(`invalid fee scheduler mode: ${mode}`);
  }

  return Number(clamp(raw, floor, cliff));
}

/// Relative price move vs the volatility anchor, in bps:
/// `|sqrt_now - sqrt_ref| * 10000 / sqrt_ref`. 0 if no anchor is set.
export function priceMoveBps(sqrtRef: bigint, sqrtNow: bigint): bigint {
  if (sqrtRef === 0n) return 0n;
  const diff = sqrtNow > sqrtRef ? sqrtNow - sqrtRef : sqrtRef - sqrtNow;
  return mulDiv(diff, BPS_DENOMINATOR, sqrtRef, Rounding.Down) ?? U128_MAX;
}

/// Decay the stored accumulator by idle time into the next swap's reference.
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

/// New accumulator after a price move: `reference + move`, capped at `maxVa`.
export function accumulateVolatility(reference: bigint, moveBps: bigint, maxVa: number): bigint {
  const sum = satAddU128(reference, moveBps);
  const cap = BigInt(maxVa);
  return sum < cap ? sum : cap;
}

/// Dynamic surcharge in bps: `va^2 * control / 1e9`, capped at `maxDynamic`.
export function dynamicFeeBps(va: bigint, variableFeeControl: number, maxDynamicFeeBps: number): number {
  if (variableFeeControl === 0) return 0;
  const sq = satMulU128(va, va);
  const fee = satMulU128(sq, BigInt(variableFeeControl)) / DYNAMIC_FEE_DENOMINATOR;
  const cap = BigInt(maxDynamicFeeBps);
  return Number(fee < cap ? fee : cap);
}

/// The volatility state a swap folds in, plus the dynamic surcharge it derives.
export interface DynamicFeeState {
  dynamicFeeBps: number;
  volatilityAccumulator: bigint;
  volatilityReference: bigint;
  sqrtPriceReference: bigint;
}

/// Fold a swap into the volatility state and derive the dynamic surcharge.
/// Bit-exact port of `compute_dynamic_fee`.
export function computeDynamicFee(params: {
  sqrtPrice: bigint;
  sqrtPriceReference: bigint;
  volatilityAccumulator: bigint;
  volatilityReference: bigint;
  elapsed: bigint;
  filterPeriod: number;
  decayPeriod: number;
  reductionFactorBps: number;
  maxVa: number;
  variableFeeControl: number;
  maxDynamicFeeBps: number;
}): DynamicFeeState {
  const {
    sqrtPrice,
    sqrtPriceReference,
    volatilityAccumulator,
    volatilityReference,
    elapsed,
    filterPeriod,
    decayPeriod,
    reductionFactorBps,
    maxVa,
    variableFeeControl,
    maxDynamicFeeBps,
  } = params;

  const newWindow = elapsed >= BigInt(filterPeriod);
  const reference = newWindow
    ? decayedVolatilityReference(volatilityAccumulator, elapsed, filterPeriod, decayPeriod, reductionFactorBps)
    : volatilityReference;
  const anchor = newWindow ? sqrtPrice : sqrtPriceReference;

  const moveBps = priceMoveBps(anchor, sqrtPrice);
  const va = accumulateVolatility(reference, moveBps, maxVa);
  const fee = dynamicFeeBps(va, variableFeeControl, maxDynamicFeeBps);
  return {
    dynamicFeeBps: fee,
    volatilityAccumulator: va,
    volatilityReference: reference,
    sqrtPriceReference: anchor,
  };
}
