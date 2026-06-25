import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  computeDynamicFee,
  computeSwapStep,
  deltaA,
  deltaB,
  FeeError,
  liquidityFromAmountA,
  liquidityFromAmountB,
  mulDiv,
  mulShr,
  nextSqrtPriceFromAmountX,
  nextSqrtPriceFromAmountY,
  priceFromSqrtPrice,
  Q64,
  Rounding,
  scheduledBaseFeeBps,
  shlDiv,
  sqrtPriceFromPrice,
  SwapDirection,
  SwapError,
  SwapMode,
} from "../src/index.js";

// Shared vectors emitted from the Rust crate
// (`cargo test -p zenith-amm --test golden_math_vectors`). Read at runtime so
// the 2 MB fixture is never bundled into dist.
const fixturePath = fileURLToPath(new URL("./fixtures/math_vectors.json", import.meta.url));
// Each row is a plain decoded-JSON object whose fields are read positionally
// per category below; typed loosely since the shapes differ per category.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Row = any;
const V: Record<string, Row[]> = JSON.parse(readFileSync(fixturePath, "utf8"));

const b = (s: string) => BigInt(s);
const optEq = (got: bigint | null, want: string | null): boolean =>
  want === null ? got === null : got !== null && got === b(want);
const R = (r: number): Rounding => (r === 1 ? Rounding.Up : Rounding.Down);

/// Run a category through `fn` and assert every vector matches Rust, reporting
/// the first few mismatches with their inputs.
function checkAll<T>(
  name: string,
  rows: T[],
  run: (row: T) => bigint | null,
  expected: (row: T) => string | null,
): void {
  const misses: string[] = [];
  for (const row of rows) {
    const got = run(row);
    if (!optEq(got, expected(row))) {
      if (misses.length < 5) {
        misses.push(`in=${JSON.stringify(row)} got=${got} want=${expected(row)}`);
      }
    }
  }
  expect(misses, `${name}: ${misses.length} mismatch(es)\n${misses.join("\n")}`).toHaveLength(0);
}

describe("u256 primitives parity", () => {
  it("mulDiv", () => {
    checkAll(
      "mulDiv",
      V.mul_div,
      (r) => mulDiv(b(r.a), b(r.b), b(r.d), R(r.r)),
      (r) => r.out,
    );
  });
  it("mulShr", () => {
    checkAll(
      "mulShr",
      V.mul_shr,
      (r) => mulShr(b(r.a), b(r.b), BigInt(r.s), R(r.r)),
      (r) => r.out,
    );
  });
  it("shlDiv", () => {
    checkAll(
      "shlDiv",
      V.shl_div,
      (r) => shlDiv(b(r.a), BigInt(r.s), b(r.d), R(r.r)),
      (r) => r.out,
    );
  });
});

describe("Q64 method parity", () => {
  const bitsOf = (q: Q64 | null) => (q === null ? null : q.toBits());
  it("fromRatio", () => {
    checkAll(
      "fromRatio",
      V.q64_from_ratio,
      (r) => bitsOf(Q64.fromRatio(b(r.a), b(r.b), R(r.r))),
      (r) => r.out,
    );
  });
  it("mul", () => {
    checkAll(
      "mul",
      V.q64_mul,
      (r) => bitsOf(Q64.fromBits(b(r.a)).mul(Q64.fromBits(b(r.b)), R(r.r))),
      (r) => r.out,
    );
  });
  it("div", () => {
    checkAll(
      "div",
      V.q64_div,
      (r) => bitsOf(Q64.fromBits(b(r.a)).div(Q64.fromBits(b(r.b)), R(r.r))),
      (r) => r.out,
    );
  });
  it("recip", () => {
    checkAll(
      "recip",
      V.q64_recip,
      (r) => bitsOf(Q64.fromBits(b(r.a)).recip(R(r.r))),
      (r) => r.out,
    );
  });
  it("mulInt", () => {
    checkAll(
      "mulInt",
      V.q64_mul_int,
      (r) => Q64.fromBits(b(r.bits)).mulInt(b(r.amt), R(r.r)),
      (r) => r.out,
    );
  });
  it("divInt", () => {
    checkAll(
      "divInt",
      V.q64_div_int,
      (r) => Q64.fromBits(b(r.bits)).divInt(b(r.amt), R(r.r)),
      (r) => r.out,
    );
  });
});

describe("sqrt-price <-> price parity", () => {
  const bitsOf = (q: Q64 | null) => (q === null ? null : q.toBits());
  it("sqrtPriceFromPrice", () => {
    checkAll(
      "sqrtPriceFromPrice",
      V.sqrt_price_from_price,
      (r) => bitsOf(sqrtPriceFromPrice(b(r.num), b(r.den))),
      (r) => r.out,
    );
  });
  it("priceFromSqrtPrice", () => {
    checkAll(
      "priceFromSqrtPrice",
      V.price_from_sqrt_price,
      (r) => bitsOf(priceFromSqrtPrice(Q64.fromBits(b(r.sp)), R(r.r))),
      (r) => r.out,
    );
  });
});

describe("delta / liquidity parity", () => {
  const sq = (x: string) => Q64.fromBits(b(x));
  const bitsOf = (q: Q64 | null) => (q === null ? null : q.toBits());
  it("deltaA", () => {
    checkAll(
      "deltaA",
      V.delta_a,
      (r) => deltaA(b(r.l), sq(r.a), sq(r.b), R(r.r)),
      (r) => r.out,
    );
  });
  it("deltaB", () => {
    checkAll(
      "deltaB",
      V.delta_b,
      (r) => deltaB(b(r.l), sq(r.a), sq(r.b), R(r.r)),
      (r) => r.out,
    );
  });
  it("liquidityFromAmountA", () => {
    checkAll(
      "liquidityFromAmountA",
      V.liq_from_a,
      (r) => liquidityFromAmountA(b(r.amt), sq(r.a), sq(r.b), R(r.r)),
      (r) => r.out,
    );
  });
  it("liquidityFromAmountB", () => {
    checkAll(
      "liquidityFromAmountB",
      V.liq_from_b,
      (r) => liquidityFromAmountB(b(r.amt), sq(r.a), sq(r.b), R(r.r)),
      (r) => r.out,
    );
  });
  it("nextSqrtPriceFromAmountX", () => {
    checkAll(
      "nextSqrtPriceFromAmountX",
      V.next_x,
      (r) => bitsOf(nextSqrtPriceFromAmountX(Q64.fromBits(b(r.sp)), b(r.l), b(r.amt), r.add)),
      (r) => r.out,
    );
  });
  it("nextSqrtPriceFromAmountY", () => {
    checkAll(
      "nextSqrtPriceFromAmountY",
      V.next_y,
      (r) => bitsOf(nextSqrtPriceFromAmountY(Q64.fromBits(b(r.sp)), b(r.l), b(r.amt), r.add)),
      (r) => r.out,
    );
  });
});

describe("fee engine parity", () => {
  it("scheduledBaseFeeBps", () => {
    const misses: string[] = [];
    for (const r of V.sched_fee) {
      let got: number | null;
      try {
        got = scheduledBaseFeeBps({
          mode: r.mode,
          baseFeeBps: r.base,
          cliffFeeBps: r.cliff,
          reductionFactor: r.red,
          feePeriod: b(r.period),
          maxFeeSteps: r.maxs,
          elapsedSlots: b(r.el),
        });
      } catch (e) {
        if (!(e instanceof FeeError)) throw e;
        got = null;
      }
      const want = r.out === null ? null : Number(r.out);
      if (got !== want && misses.length < 8) misses.push(`in=${JSON.stringify(r)} got=${got} want=${want}`);
    }
    expect(misses, misses.join("\n")).toHaveLength(0);
  });

  it("computeDynamicFee (fee + full state)", () => {
    const misses: string[] = [];
    for (const r of V.dyn_fee) {
      const s = computeDynamicFee({
        sqrtPrice: b(r.sp),
        sqrtPriceReference: b(r.sref),
        volatilityAccumulator: b(r.acc),
        volatilityReference: b(r.vref),
        elapsed: b(r.el),
        filterPeriod: r.filt,
        decayPeriod: r.dec,
        reductionFactorBps: r.vred,
        maxVa: r.maxva,
        variableFeeControl: r.ctrl,
        maxDynamicFeeBps: r.maxd,
      });
      const ok =
        s.dynamicFeeBps === Number(r.dyn) &&
        s.volatilityAccumulator === b(r.va) &&
        s.volatilityReference === b(r.vrefOut) &&
        s.sqrtPriceReference === b(r.sprefOut);
      if (!ok && misses.length < 8) misses.push(`in=${JSON.stringify(r)} got=${JSON.stringify({ d: s.dynamicFeeBps, va: s.volatilityAccumulator.toString() })}`);
    }
    expect(misses, misses.join("\n")).toHaveLength(0);
  });
});

describe("computeSwapStep parity", () => {
  it("matches the program across direction/mode/fee combos", () => {
    const misses: string[] = [];
    for (const r of V.swap_step) {
      let got: string | null;
      try {
        const s = computeSwapStep({
          sqrtPrice: b(r.sp),
          liquidity: b(r.l),
          sqrtMin: b(r.min),
          sqrtMax: b(r.max),
          direction: r.dir as SwapDirection,
          mode: r.mode as SwapMode,
          amount: b(r.amt),
          feeBps: r.fee,
        });
        got = JSON.stringify({
          nextSqrtPrice: s.nextSqrtPrice.toString(),
          amountIn: s.amountIn.toString(),
          amountOut: s.amountOut.toString(),
          fee: s.fee.toString(),
          amountRemaining: s.amountRemaining.toString(),
        });
      } catch (e) {
        if (!(e instanceof SwapError)) throw e;
        got = null; // Rust encodes a revert as null
      }
      const want = r.out === null ? null : JSON.stringify(r.out);
      if (got !== want && misses.length < 8) {
        misses.push(`in=${JSON.stringify({ ...r, out: undefined })} got=${got} want=${want}`);
      } else if (got !== want) {
        misses.push("…");
      }
    }
    expect(misses, `swap_step mismatches:\n${misses.join("\n")}`).toHaveLength(0);
  });
});
