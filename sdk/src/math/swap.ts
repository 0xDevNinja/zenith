import { Q64 } from "./q64.js";
import { Rounding, U64_MAX } from "./rounding.js";
import {
  deltaA,
  deltaB,
  nextSqrtPriceFromAmountX,
  nextSqrtPriceFromAmountY,
} from "./sqrtPrice.js";
import { mulDiv } from "./u256.js";

/// Basis-point denominator (100%).
const BPS_DENOMINATOR = 10_000n;

/// Which way a swap moves the pool. `AToB` sells token A (base) for B (quote),
/// lowering the price toward `sqrtMin`; `BToA` is the reverse.
export enum SwapDirection {
  AToB = "AToB",
  BToA = "BToA",
}

/// How the caller specified the swap amount.
export enum SwapMode {
  /// `amount` is the exact input; reverts if the fill would leave the band.
  ExactIn = "ExactIn",
  /// `amount` is the exact desired output; reverts if it would leave the band.
  ExactOut = "ExactOut",
  /// `amount` is the input, filled only to the band boundary; the unspent
  /// remainder is reported instead of reverting.
  PartialFill = "PartialFill",
}

/// Error codes mirroring the program's `ZenithError` variants that
/// `compute_swap_step` can surface. The on-chain program reverts with these;
/// the SDK throws so a quote fails the same way a swap would.
export type SwapErrorCode =
  | "InsufficientLiquidity"
  | "ZeroAmount"
  | "InvalidFeeConfig"
  | "MathOverflow"
  | "PriceOutOfBand";

/// Thrown when a swap step would revert on-chain.
export class SwapError extends Error {
  readonly code: SwapErrorCode;
  constructor(code: SwapErrorCode) {
    super(`swap step would revert: ${code}`);
    this.name = "SwapError";
    this.code = code;
  }
}

/// Result of a single swap step over the pool's one price band. All amounts are
/// raw token units (`u64` range); `nextSqrtPrice` is Q64.64 raw bits.
export interface SwapStep {
  nextSqrtPrice: bigint;
  amountIn: bigint;
  amountOut: bigint;
  fee: bigint;
  amountRemaining: bigint;
}

function require(cond: boolean, code: SwapErrorCode): void {
  if (!cond) throw new SwapError(code);
}

/// Narrow a `u128` math result to a `u64` token amount, throwing on overflow.
function toTokenAmount(x: bigint): bigint {
  if (x > U64_MAX) throw new SwapError("MathOverflow");
  return x;
}

/// `unwrap_or(MathOverflow)` for the `bigint | null` math helpers.
function orOverflow(x: bigint | null): bigint {
  if (x === null) throw new SwapError("MathOverflow");
  return x;
}

/// `unwrap_or(MathOverflow)` for the `Q64 | null` next-price helpers.
function orOverflowPrice(x: Q64 | null): Q64 {
  if (x === null) throw new SwapError("MathOverflow");
  return x;
}

/// Fee included in a gross input: `ceil(gross * bps / 10000)`.
function feeOnGross(gross: bigint, feeBps: bigint): bigint {
  return orOverflow(mulDiv(gross, feeBps, BPS_DENOMINATOR, Rounding.Up));
}

/// Fee to add on top of a net input: `ceil(net * bps / (10000 - bps))`.
function feeOnNet(net: bigint, feeBps: bigint): bigint {
  return orOverflow(mulDiv(net, feeBps, BPS_DENOMINATOR - feeBps, Rounding.Up));
}

/// Output tokens when the price moves from `from` to `to` for liquidity `L`.
/// `aToB` pays token B (price fell); otherwise token A (price rose). Rounds down.
function swapOutput(liquidity: bigint, to: Q64, from: Q64, aToB: boolean): bigint {
  const out = aToB
    ? deltaB(liquidity, to, from, Rounding.Down)
    : deltaA(liquidity, from, to, Rounding.Down);
  return toTokenAmount(orOverflow(out));
}

/// Net input (pre-fee) needed to move the price from `from` to `to`. `aToB`
/// charges token A; otherwise token B. Rounds up.
function swapInputTo(liquidity: bigint, from: Q64, to: Q64, aToB: boolean): bigint {
  const inp = aToB
    ? deltaA(liquidity, to, from, Rounding.Up)
    : deltaB(liquidity, from, to, Rounding.Up);
  return orOverflow(inp);
}

/// Compute one swap step over the pool's single liquidity band. Bit-exact port
/// of the program's `compute_swap_step`: outputs round down and required inputs
/// round up, the price never leaves `[sqrtMin, sqrtMax]`, and any condition that
/// reverts on-chain throws a [`SwapError`] with the matching code.
///
/// `amount` is the input (ExactIn/PartialFill) or desired output (ExactOut) in
/// raw token units; prices are Q64.64 raw bits; `liquidity > 0`.
export function computeSwapStep(params: {
  sqrtPrice: bigint;
  liquidity: bigint;
  sqrtMin: bigint;
  sqrtMax: bigint;
  direction: SwapDirection;
  mode: SwapMode;
  amount: bigint;
  feeBps: number;
}): SwapStep {
  const { sqrtPrice, liquidity, sqrtMin, sqrtMax, direction, mode, amount } = params;
  const feeBps = BigInt(params.feeBps);

  require(liquidity > 0n, "InsufficientLiquidity");
  require(amount > 0n, "ZeroAmount");
  require(feeBps < BPS_DENOMINATOR, "InvalidFeeConfig");

  const price = Q64.fromBits(sqrtPrice);
  const aToB = direction === SwapDirection.AToB;
  const boundaryBits = aToB ? sqrtMin : sqrtMax;
  const boundary = Q64.fromBits(boundaryBits);

  if (mode === SwapMode.ExactIn || mode === SwapMode.PartialFill) {
    const gross = amount;
    const fee = feeOnGross(gross, feeBps);
    const netIn = gross - fee; // fee <= gross since bps < 10000

    // Provisional next price assuming the whole net input is consumed.
    const nextPrice = orOverflowPrice(
      aToB
        ? nextSqrtPriceFromAmountX(price, liquidity, netIn, true)
        : nextSqrtPriceFromAmountY(price, liquidity, netIn, true),
    );
    const next = nextPrice.toBits();

    const crosses = aToB ? next < boundaryBits : next > boundaryBits;

    if (!crosses) {
      const amountOut = swapOutput(liquidity, nextPrice, price, aToB);
      require(amountOut > 0n, "ZeroAmount");
      return {
        nextSqrtPrice: next,
        amountIn: amount,
        amountOut,
        fee: toTokenAmount(fee),
        amountRemaining: 0n,
      };
    }

    // Would leave the band.
    require(mode === SwapMode.PartialFill, "PriceOutOfBand");

    // Fill exactly to the boundary; recompute consumed input + fee.
    const netConsumed = swapInputTo(liquidity, price, boundary, aToB);
    require(netConsumed > 0n, "PriceOutOfBand");
    const feeConsumedRaw = feeOnNet(netConsumed, feeBps);
    let amountIn = netConsumed + feeConsumedRaw;
    // Clamp to the caller's input (rounding can only match or undershoot gross).
    amountIn = amountIn < gross ? amountIn : gross;
    const feeConsumed = amountIn - netConsumed;
    const amountOut = swapOutput(liquidity, boundary, price, aToB);
    require(amountOut > 0n, "PriceOutOfBand");
    const amountInTok = toTokenAmount(amountIn);

    return {
      nextSqrtPrice: boundaryBits,
      amountIn: amountInTok,
      amountOut,
      fee: toTokenAmount(feeConsumed),
      amountRemaining: amount - amountInTok,
    };
  }

  // ExactOut.
  const wantOut = amount;
  const nextPrice = orOverflowPrice(
    aToB
      ? nextSqrtPriceFromAmountY(price, liquidity, wantOut, false)
      : nextSqrtPriceFromAmountX(price, liquidity, wantOut, false),
  );
  const next = nextPrice.toBits();
  const crosses = aToB ? next < boundaryBits : next > boundaryBits;
  require(!crosses, "PriceOutOfBand");

  const netIn = swapInputTo(liquidity, price, nextPrice, aToB);
  require(netIn > 0n, "ZeroAmount");
  const fee = feeOnNet(netIn, feeBps);
  const amountIn = netIn + fee;

  return {
    nextSqrtPrice: next,
    amountIn: toTokenAmount(amountIn),
    amountOut: amount, // exact
    fee: toTokenAmount(fee),
    amountRemaining: 0n,
  };
}
