import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { PublicKey } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
import {
  computeDynamicFee,
  type Config,
  effectiveFeeBps,
  type Pool,
  quoteSwap,
  scheduledBaseFeeBps,
  SwapDirection,
  SwapError,
  SwapMode,
  U64_MAX,
} from "../src/index.js";

const fixturePath = fileURLToPath(new URL("./fixtures/math_vectors.json", import.meta.url));
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const V: Record<string, any[]> = JSON.parse(readFileSync(fixturePath, "utf8"));

const b = (s: string) => BigInt(s);
const ONE = 1n << 64n;
const KEY = PublicKey.default;

// Pool/Config matching the constants the generator's `effective_quote` used.
function makePool(over: Partial<Pool>): Pool {
  return {
    liquidity: 10_000_000n,
    sqrtPrice: 2n * ONE,
    sqrtMinPrice: ONE,
    sqrtMaxPrice: 4n * ONE,
    feeGrowthGlobalA: 0n,
    feeGrowthGlobalB: 0n,
    sqrtPriceReference: ONE + ONE / 2n,
    volatilityAccumulator: 0n,
    volatilityReference: 0n,
    config: KEY,
    tokenAMint: KEY,
    tokenBMint: KEY,
    tokenAVault: KEY,
    tokenBVault: KEY,
    protocolFeeA: 0n,
    protocolFeeB: 0n,
    activationPoint: 0n,
    positionCount: 0n,
    lastVolatilityUpdate: 0n,
    partnerFeeA: 0n,
    partnerFeeB: 0n,
    baseFeeBps: 0,
    status: 1,
    poolAuthorityBump: 0,
    poolBump: 0,
    tokenAVaultBump: 0,
    tokenBVaultBump: 0,
    tokenAFlavor: 0,
    tokenBFlavor: 0,
    ...over,
  } as Pool;
}

function makeConfig(over: Partial<Config>): Config {
  return {
    admin: KEY,
    feeAuthority: KEY,
    partner: KEY,
    sqrtMinPrice: ONE,
    sqrtMaxPrice: 4n * ONE,
    feePeriod: 100n,
    index: 0,
    baseFeeBps: 30,
    protocolFeeBps: 2500,
    cliffFeeBps: 500,
    reductionFactor: 50,
    maxFeeSteps: 8,
    variableFeeControl: 1000,
    maxVolatilityAccumulator: 100_000,
    filterPeriod: 10,
    decayPeriod: 100,
    volatilityReductionFactor: 5000,
    maxDynamicFeeBps: 500,
    partnerFeeBps: 0,
    feeSchedulerMode: 1,
    bump: 0,
    ...over,
  } as Config;
}

describe("quoteSwap integrated parity (vs swap.rs fee + step)", () => {
  it("matches the program's fee derivation and swap step", () => {
    const misses: string[] = [];
    for (const r of V.effective_quote) {
      const pool = makePool({ sqrtPrice: b(r.sp), sqrtPriceReference: b(r.sref) });
      const config = makeConfig({
        feeSchedulerMode: r.schedMode,
        baseFeeBps: r.base,
        cliffFeeBps: r.cliff,
        reductionFactor: r.red,
        feePeriod: b(r.period),
        maxFeeSteps: r.maxs,
      });
      const args = {
        pool,
        config,
        slot: b(r.now),
        direction: r.dir as SwapDirection,
        mode: r.mode as SwapMode,
        amount: b(r.amt),
        slippageBps: 50,
      };

      if (r.step === null) {
        // Program reverted -> quoteSwap must throw the same way.
        let threw = false;
        try {
          quoteSwap(args);
        } catch (e) {
          threw = e instanceof SwapError;
        }
        if (!threw && misses.length < 8) misses.push(`expected revert: ${JSON.stringify(r)}`);
        continue;
      }

      const q = quoteSwap(args);
      const ok =
        q.fee.totalFeeBps === Number(r.feeBps) &&
        q.amountIn === b(r.step.amountIn) &&
        q.amountOut === b(r.step.amountOut) &&
        q.feeAmount === b(r.step.fee) &&
        q.amountRemaining === b(r.step.amountRemaining) &&
        q.nextSqrtPrice === b(r.step.nextSqrtPrice);
      if (!ok && misses.length < 8) {
        misses.push(
          `in=${JSON.stringify({ ...r, step: undefined })} feeGot=${q.fee.totalFeeBps} feeWant=${r.feeBps} outGot=${q.amountOut} outWant=${r.step.amountOut}`,
        );
      }
    }
    expect(misses, misses.join("\n")).toHaveLength(0);
  });
});

describe("quoteSwap derived fields", () => {
  const pool = makePool({});
  const config = makeConfig({ feeSchedulerMode: 0, baseFeeBps: 30 }); // constant 30 bps

  it("effectiveFeeBps = base + dynamic, clamped", () => {
    const f = effectiveFeeBps(config, pool, 0n);
    expect(f.baseFeeBps).toBe(30);
    expect(f.totalFeeBps).toBe(f.baseFeeBps + f.dynamicFeeBps);
    expect(f.totalFeeBps).toBeLessThan(10_000);
  });

  it("ExactIn: minAmountOut <= amountOut and is the threshold", () => {
    const q = quoteSwap({
      pool,
      config,
      slot: 0n,
      direction: SwapDirection.BToA,
      mode: SwapMode.ExactIn,
      amount: 1_000_000n,
      slippageBps: 50,
    });
    expect(q.amountOut > 0n).toBe(true);
    expect(q.minAmountOut).toBeDefined();
    expect(q.minAmountOut! <= q.amountOut).toBe(true);
    // 0.5% tolerance: floor(out * 9950 / 10000)
    expect(q.minAmountOut).toBe((q.amountOut * 9950n) / 10_000n);
    expect(q.otherAmountThreshold).toBe(q.minAmountOut);
    expect(q.maxAmountIn).toBeUndefined();
    expect(q.priceImpactBps && q.priceImpactBps > 0n).toBe(true);
  });

  it("ExactOut: maxAmountIn >= amountIn, stays a valid u64, is the threshold", () => {
    const q = quoteSwap({
      pool,
      config,
      slot: 0n,
      direction: SwapDirection.BToA,
      mode: SwapMode.ExactOut,
      amount: 1_000n,
      slippageBps: 100,
    });
    expect(q.amountOut).toBe(1_000n);
    expect(q.maxAmountIn).toBeDefined();
    expect(q.maxAmountIn! >= q.amountIn).toBe(true);
    expect(q.maxAmountIn! <= U64_MAX).toBe(true);
    expect(q.otherAmountThreshold).toBe(q.maxAmountIn);
    expect(q.minAmountOut).toBeUndefined();
  });

  it("PartialFill: clamps, returns remainder, min-out threshold", () => {
    const q = quoteSwap({
      pool,
      config,
      slot: 0n,
      direction: SwapDirection.BToA,
      mode: SwapMode.PartialFill,
      amount: (1n << 64n) - 1n, // u64::MAX — far past the band
      slippageBps: 50,
    });
    expect(q.amountRemaining > 0n).toBe(true);
    expect(q.amountIn + q.amountRemaining).toBe((1n << 64n) - 1n);
    expect(q.minAmountOut).toBeDefined();
    expect(q.minAmountOut!).toBe((q.amountOut * 9950n) / 10_000n);
    expect(q.otherAmountThreshold).toBe(q.minAmountOut);
  });

  it("scheduler uses slots-since-activation; dynamic uses slots-since-last-vol", () => {
    // Distinct, nonzero bases so a swap of the two would change the result.
    const cfg = makeConfig({ feeSchedulerMode: 1 }); // linear, decays with age
    const p = makePool({
      sqrtPrice: 2n * ONE,
      sqrtPriceReference: ONE + ONE / 2n, // 1.5 anchor -> drift -> dynamic fee
      activationPoint: 100n,
      lastVolatilityUpdate: 499n,
    });
    const slot = 500n;
    const f = effectiveFeeBps(cfg, p, slot);

    // Base must read (slot - activationPoint) = 400 slots.
    const wantBase = scheduledBaseFeeBps({
      mode: cfg.feeSchedulerMode,
      baseFeeBps: cfg.baseFeeBps,
      cliffFeeBps: cfg.cliffFeeBps,
      reductionFactor: cfg.reductionFactor,
      feePeriod: cfg.feePeriod,
      maxFeeSteps: cfg.maxFeeSteps,
      elapsedSlots: 400n,
    });
    // Dynamic must read (slot - lastVolatilityUpdate) = 1 slot (in-window).
    const wantDyn = computeDynamicFee({
      sqrtPrice: p.sqrtPrice,
      sqrtPriceReference: p.sqrtPriceReference,
      volatilityAccumulator: p.volatilityAccumulator,
      volatilityReference: p.volatilityReference,
      elapsed: 1n,
      filterPeriod: cfg.filterPeriod,
      decayPeriod: cfg.decayPeriod,
      reductionFactorBps: cfg.volatilityReductionFactor,
      maxVa: cfg.maxVolatilityAccumulator,
      variableFeeControl: cfg.variableFeeControl,
      maxDynamicFeeBps: cfg.maxDynamicFeeBps,
    }).dynamicFeeBps;

    expect(f.baseFeeBps).toBe(wantBase);
    expect(f.dynamicFeeBps).toBe(wantDyn);
    expect(f.totalFeeBps).toBe(Math.min(wantBase + wantDyn, 9999));
    // Sanity: the two elapsed bases genuinely produce different sub-fees here,
    // so a swapped subtraction could not coincidentally pass.
    expect(wantBase).not.toBe(wantDyn);
  });

  it("zero slippage makes the threshold exactly the quoted amount", () => {
    const q = quoteSwap({
      pool,
      config,
      slot: 0n,
      direction: SwapDirection.BToA,
      mode: SwapMode.ExactIn,
      amount: 1_000_000n,
      slippageBps: 0,
    });
    expect(q.minAmountOut).toBe(q.amountOut);
  });

  it("rejects an out-of-range slippage", () => {
    expect(() =>
      quoteSwap({
        pool,
        config,
        slot: 0n,
        direction: SwapDirection.BToA,
        mode: SwapMode.ExactIn,
        amount: 1_000n,
        slippageBps: 20_000,
      }),
    ).toThrow(RangeError);
  });

  it("propagates a SwapError when the trade can't fill (band cross, ExactIn)", () => {
    expect(() =>
      quoteSwap({
        pool,
        config,
        slot: 0n,
        direction: SwapDirection.BToA,
        mode: SwapMode.ExactIn,
        amount: (1n << 64n) - 1n,
        slippageBps: 50,
      }),
    ).toThrow(SwapError);
  });
});
