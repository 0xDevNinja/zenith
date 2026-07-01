import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { dlmm } from "../src/index.js";
import { Rounding } from "../src/math/rounding.js";

const vectors = JSON.parse(
  readFileSync(fileURLToPath(new URL("./fixtures/dlmm_math_vectors.json", import.meta.url)), "utf8"),
) as {
  binPrice: Array<{ binStep: number; binId: number; bits: string }>;
  fillIn: Array<{
    binId: number;
    inAvail: string;
    reserveOut: string;
    dir: number;
    inUsed: string;
    out: string;
    drained: boolean;
  }>;
  fillOut: Array<{
    binId: number;
    outNeed: string;
    reserveOut: string;
    dir: number;
    inUsed: string;
    out: string;
    drained: boolean;
  }>;
  varFee: Array<{
    active: number;
    elapsed: number;
    va0: number;
    vr0: number;
    variableFeeBps: number;
    va: string;
    vr: string;
    idxRef: number;
  }>;
};

describe("dlmm bin price parity", () => {
  it("bin 0 is exactly 1.0", () => {
    expect(dlmm.binPrice(25, 0, Rounding.Down)!.toBits()).toBe(1n << 64n);
  });

  it("matches the program bit-for-bit across steps and ids", () => {
    for (const v of vectors.binPrice) {
      const p = dlmm.binPrice(v.binStep, v.binId, Rounding.Down);
      expect(p, `binStep ${v.binStep} binId ${v.binId}`).not.toBeNull();
      expect(p!.toBits().toString()).toBe(v.bits);
    }
  });

  it("is strictly monotonic in bin id", () => {
    let prev = -1n;
    for (const id of [-13, -5, -1, 0, 1, 5, 13]) {
      const bits = dlmm.binPrice(25, id, Rounding.Down)!.toBits();
      expect(bits).toBeGreaterThan(prev);
      prev = bits;
    }
  });
});

describe("dlmm single-bin fill parity", () => {
  it("fillExactIn matches the program", () => {
    for (const v of vectors.fillIn) {
      const price = dlmm.binPrice(25, v.binId, Rounding.Down)!;
      const f = dlmm.fillExactIn(BigInt(v.inAvail), BigInt(v.reserveOut), price, v.dir)!;
      expect(f.inUsed.toString(), JSON.stringify(v)).toBe(v.inUsed);
      expect(f.out.toString(), JSON.stringify(v)).toBe(v.out);
      expect(f.drained, JSON.stringify(v)).toBe(v.drained);
    }
  });

  it("fillExactOut matches the program", () => {
    for (const v of vectors.fillOut) {
      const price = dlmm.binPrice(25, v.binId, Rounding.Down)!;
      const f = dlmm.fillExactOut(BigInt(v.outNeed), BigInt(v.reserveOut), price, v.dir)!;
      expect(f.inUsed.toString(), JSON.stringify(v)).toBe(v.inUsed);
      expect(f.out.toString(), JSON.stringify(v)).toBe(v.out);
      expect(f.drained, JSON.stringify(v)).toBe(v.drained);
    }
  });
});

describe("dlmm volatility fee parity", () => {
  it("computeVariableFee matches the program", () => {
    for (const v of vectors.varFee) {
      const s = dlmm.computeVariableFee({
        activeBin: v.active,
        indexReference: 0,
        volatilityAccumulator: BigInt(v.va0),
        volatilityReference: BigInt(v.vr0),
        elapsed: BigInt(v.elapsed),
        filterPeriod: 10,
        decayPeriod: 100,
        reductionFactorBps: 5_000,
        maxVa: 100_000,
        binStep: 25,
        variableFeeControl: 1_000_000,
        maxDynamicFeeBps: 1_000,
      });
      const ctx = JSON.stringify(v);
      expect(s.variableFeeBps, ctx).toBe(v.variableFeeBps);
      expect(s.volatilityAccumulator.toString(), ctx).toBe(v.va);
      expect(s.volatilityReference.toString(), ctx).toBe(v.vr);
      expect(s.indexReference, ctx).toBe(v.idxRef);
    }
  });
});

// ---------------------------------------------------------------------------
// Full bin-walk quote
// ---------------------------------------------------------------------------

/// Build an LbPair with the dynamic fee OFF (base fee only), so the walk is easy
/// to reason about. Only the fields quoteSwap reads are meaningful.
function pair(overrides: Partial<dlmm.LbPair> = {}): dlmm.LbPair {
  return {
    activeBinId: 0,
    indexReference: 0,
    volatilityAccumulator: 0n,
    volatilityReference: 0n,
    lastUpdateSlot: 0n,
    filterPeriod: 0,
    decayPeriod: 0,
    volatilityReductionFactor: 0,
    maxVolatilityAccumulator: 0,
    binStep: 25,
    variableFeeControl: 0, // dynamic fee off
    maxDynamicFeeBps: 0,
    baseFeeBps: 30, // 0.30%
    protocolFeeRate: 2_000, // 20%
    ...(overrides as object),
  } as dlmm.LbPair;
}

function bin(amountX: bigint, amountY: bigint): dlmm.Bin {
  return { feeGrowthX: 0n, feeGrowthY: 0n, liquiditySupply: 1n, amountX, amountY };
}

function binArray(index: number, bins: Record<number, dlmm.Bin>): dlmm.BinArray {
  const arr: dlmm.Bin[] = [];
  for (let i = 0; i < dlmm.BINS_PER_ARRAY; i++) {
    arr.push(bins[i] ?? bin(0n, 0n));
  }
  return { bins: arr, lbPair: undefined as never, index: BigInt(index), bump: 0 };
}

describe("dlmm quoteSwap (bin walk)", () => {
  it("stays in the active bin (X->Y, price 1.0)", () => {
    // bin 0 price = 1.0, 1000 Y available. Swap 100 X in.
    const p = pair();
    const arr0 = binArray(0, { 0: bin(0n, 1_000n) });
    const q = dlmm.quoteSwap({
      pair: p,
      binArrays: [arr0],
      slot: 0n,
      direction: dlmm.Direction.XtoY,
      mode: dlmm.SwapMode.ExactIn,
      amount: 100n,
    });
    // fee = ceil(100 * 30/10000) = 1; net 99; out = 99 at price 1.0
    expect(q.fee).toBe(1n);
    expect(q.amountIn).toBe(100n);
    expect(q.amountOut).toBe(99n);
    expect(q.binsCrossed).toBe(1);
    expect(q.endBinId).toBe(0);
    // protocol 20% of 1 = 0 (floor); lp = 1
    expect(q.protocolFee).toBe(0n);
    expect(q.lpFee).toBe(1n);
    // slippage 0.5% floor
    expect(q.otherAmountThreshold).toBe(98n);
  });

  it("crosses bins and matches a manual replay", () => {
    // bin 0 has only 40 Y; the rest fills from bin -1.
    const p = pair();
    const arr0 = binArray(0, { 0: bin(0n, 40n) });
    // bin -1 lives in array -1 at slot 69.
    const arrNeg = binArray(-1, { 69: bin(0n, 1_000n) });
    const amount = 500n;
    const q = dlmm.quoteSwap({
      pair: p,
      binArrays: [arr0, arrNeg],
      slot: 0n,
      direction: dlmm.Direction.XtoY,
      mode: dlmm.SwapMode.ExactIn,
      amount,
    });
    expect(q.binsCrossed).toBe(2);
    expect(q.endBinId).toBe(-1);

    // Manual replay with the (separately parity-checked) fill fn.
    const fee = 500n - (500n - 1n); // ceil(500*30/10000)=2
    void fee;
    const net = amount - 2n;
    const price0 = dlmm.binPrice(25, 0, Rounding.Down)!;
    const f0 = dlmm.fillExactIn(net, 40n, price0, dlmm.Direction.XtoY)!;
    const priceNeg = dlmm.binPrice(25, -1, Rounding.Down)!;
    const f1 = dlmm.fillExactIn(net - f0.inUsed, 1_000n, priceNeg, dlmm.Direction.XtoY)!;
    expect(q.amountOut).toBe(f0.out + f1.out);
    expect(q.fee).toBe(2n);
  });

  it("throws with the missing bin-array index when liquidity is elsewhere", () => {
    const p = pair({ activeBinId: 0 });
    const arr0 = binArray(0, { 0: bin(0n, 5n) }); // drains immediately, needs bin -1
    try {
      dlmm.quoteSwap({
        pair: p,
        binArrays: [arr0],
        slot: 0n,
        direction: dlmm.Direction.XtoY,
        mode: dlmm.SwapMode.ExactIn,
        amount: 10_000n,
      });
      expect.unreachable("should have thrown");
    } catch (e) {
      expect(e).toBeInstanceOf(dlmm.DlmmQuoteError);
      expect((e as dlmm.DlmmQuoteError).neededBinArrayIndex).toBe(-1);
    }
  });

  it("ExactOut computes gross input with fee on top", () => {
    const p = pair();
    const arr0 = binArray(0, { 0: bin(0n, 1_000n) });
    const q = dlmm.quoteSwap({
      pair: p,
      binArrays: [arr0],
      slot: 0n,
      direction: dlmm.Direction.XtoY,
      mode: dlmm.SwapMode.ExactOut,
      amount: 100n, // want 100 Y out
    });
    // net in for 100 Y at price 1.0 = 100; fee = ceil(100*30/(10000-30)) = 1
    expect(q.amountOut).toBe(100n);
    expect(q.fee).toBe(1n);
    expect(q.amountIn).toBe(101n);
  });
});
