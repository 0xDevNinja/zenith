import { describe, expect, it } from "vitest";
import {
  computeSwapStep,
  deltaA,
  Q64,
  Rounding,
  SwapDirection,
  SwapError,
  SwapMode,
  U128_MAX,
} from "../src/index.js";

// Helpers the golden-vector generator does not exercise, plus swap-step edge
// branches the curated bands miss. Mirrors the Rust `q64.rs::add_sub` and the
// `math.rs` swap-step unit tests with the same known values.

const ONE = 1n << 64n;

describe("Q64 additive + inspection helpers", () => {
  it("checked add/sub", () => {
    const a = Q64.fromInt(3n);
    const b = Q64.fromInt(4n);
    expect(a.checkedAdd(b)?.eq(Q64.fromInt(7n))).toBe(true);
    expect(b.checkedSub(a)?.eq(Q64.fromInt(1n))).toBe(true);
    expect(a.checkedSub(b)).toBeNull(); // underflow
    expect(Q64.MAX.checkedAdd(Q64.fromBits(1n))).toBeNull(); // overflow
  });

  it("saturating add/sub clamp at the bounds", () => {
    expect(Q64.MAX.saturatingAdd(Q64.ONE).eq(Q64.MAX)).toBe(true);
    expect(Q64.ZERO.saturatingSub(Q64.ONE).eq(Q64.ZERO)).toBe(true);
    expect(Q64.fromInt(4n).saturatingSub(Q64.fromInt(1n)).eq(Q64.fromInt(3n))).toBe(true);
  });

  it("constants, fromInt, floorInt, isZero, eq", () => {
    expect(Q64.ZERO.toBits()).toBe(0n);
    expect(Q64.ONE.toBits()).toBe(ONE);
    expect(Q64.MAX.toBits()).toBe(U128_MAX);
    expect(Q64.fromInt(5n).floorInt()).toBe(5n);
    expect(Q64.fromInt(1n).eq(Q64.ONE)).toBe(true);
    expect(Q64.ZERO.isZero()).toBe(true);
    expect(Q64.fromInt((1n << 64n) - 1n).floorInt()).toBe((1n << 64n) - 1n);
  });

  it("fromBits rejects out-of-range u128", () => {
    expect(() => Q64.fromBits(-1n)).toThrow(RangeError);
    expect(() => Q64.fromBits(U128_MAX + 1n)).toThrow(RangeError);
  });
});

describe("computeSwapStep edge branches", () => {
  const LO = ONE;
  const MID = 2n * ONE;
  const HI = 4n * ONE;
  const base = {
    sqrtPrice: MID,
    sqrtMin: LO,
    sqrtMax: HI,
    direction: SwapDirection.BToA,
  };

  it("ExactIn within band consumes all input, no fee", () => {
    const s = computeSwapStep({
      ...base,
      liquidity: 1_000_000n,
      mode: SwapMode.ExactIn,
      amount: 1_000n,
      feeBps: 0,
    });
    expect(s.amountIn).toBe(1_000n);
    expect(s.fee).toBe(0n);
    expect(s.amountRemaining).toBe(0n);
    expect(s.amountOut > 0n).toBe(true);
    expect(s.nextSqrtPrice > MID && s.nextSqrtPrice <= HI).toBe(true);
  });

  it("PartialFill clamps to the boundary and returns the remainder", () => {
    const l = 1_000_000n;
    const s = computeSwapStep({
      ...base,
      liquidity: l,
      mode: SwapMode.PartialFill,
      amount: U128_MAX & ((1n << 64n) - 1n), // u64::MAX
      feeBps: 0,
    });
    expect(s.nextSqrtPrice).toBe(HI); // filled to the upper boundary
    expect(s.amountRemaining > 0n).toBe(true);
    // Output equals all of token A between MID and HI (floor).
    const maxA = deltaA(l, Q64.fromBits(MID), Q64.fromBits(HI), Rounding.Down);
    expect(s.amountOut).toBe(maxA);
    // Consumed + remainder == provided.
    expect(s.amountIn + s.amountRemaining).toBe((1n << 64n) - 1n);
  });

  it("ExactIn crossing the band reverts (PriceOutOfBand)", () => {
    expect(() =>
      computeSwapStep({
        ...base,
        liquidity: 1_000_000n,
        mode: SwapMode.ExactIn,
        amount: (1n << 64n) - 1n,
        feeBps: 0,
      }),
    ).toThrow(SwapError);
  });

  it("ExactOut requires more input than output", () => {
    const s = computeSwapStep({
      ...base,
      liquidity: 10_000_000n,
      mode: SwapMode.ExactOut,
      amount: 1_000n,
      feeBps: 30,
    });
    expect(s.amountOut).toBe(1_000n);
    expect(s.amountIn > s.amountOut).toBe(true);
  });

  it("rejects zero amount and a 100% fee", () => {
    expect(() =>
      computeSwapStep({
        ...base,
        liquidity: 1_000n,
        mode: SwapMode.ExactIn,
        amount: 0n,
        feeBps: 0,
      }),
    ).toThrow(/ZeroAmount/);
    expect(() =>
      computeSwapStep({
        ...base,
        liquidity: 1_000n,
        mode: SwapMode.ExactIn,
        amount: 1_000n,
        feeBps: 10_000,
      }),
    ).toThrow(/InvalidFeeConfig/);
  });
});
