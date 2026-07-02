import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { PublicKey } from "@solana/web3.js";
import { describe, expect, it } from "vitest";

import { camm } from "../src/index.js";

// Golden vectors emitted by `cargo test -p zenith-math --test cp_vectors_gen`.
const vectors = JSON.parse(
  readFileSync(fileURLToPath(new URL("./fixtures/cp_math_vectors.json", import.meta.url)), "utf8"),
) as {
  minimumLiquidity: string;
  swap: Array<{
    reserveIn: string;
    reserveOut: string;
    amount: string;
    outGivenIn: string | null;
    inGivenOut: string | null;
  }>;
  lp: Array<{
    amountA: string;
    amountB: string;
    reserveA: string;
    reserveB: string;
    supply: string;
    initialShares: string | null;
    sharesFromDeposit: string | null;
    tokensForSharesDown: string | null;
    tokensForSharesUp: string | null;
    matchingAmount: string | null;
  }>;
};

const opt = (v: string | null): bigint | null => (v === null ? null : BigInt(v));

describe("camm curve + LP math matches the Rust golden vectors", () => {
  it("minimum liquidity constant agrees", () => {
    expect(camm.MINIMUM_LIQUIDITY).toBe(BigInt(vectors.minimumLiquidity));
  });

  it("outGivenIn / inGivenOut are bit-exact", () => {
    for (const v of vectors.swap) {
      const rin = BigInt(v.reserveIn);
      const rout = BigInt(v.reserveOut);
      const amt = BigInt(v.amount);
      expect(camm.outGivenIn(rin, rout, amt)).toBe(opt(v.outGivenIn));
      expect(camm.inGivenOut(rin, rout, amt)).toBe(opt(v.inGivenOut));
    }
  });

  it("initialShares / sharesFromDeposit / tokensForShares / matchingAmount are bit-exact", () => {
    for (const v of vectors.lp) {
      const a = BigInt(v.amountA);
      const b = BigInt(v.amountB);
      const ra = BigInt(v.reserveA);
      const rb = BigInt(v.reserveB);
      const supply = BigInt(v.supply);
      expect(camm.initialShares(a, b)).toBe(opt(v.initialShares));
      expect(camm.sharesFromDeposit(a, b, ra, rb, supply)).toBe(opt(v.sharesFromDeposit));
      expect(camm.tokensForShares(a, ra, supply, 0)).toBe(opt(v.tokensForSharesDown));
      expect(camm.tokensForShares(a, ra, supply, 1)).toBe(opt(v.tokensForSharesUp));
      expect(camm.matchingAmount(a, ra, rb)).toBe(opt(v.matchingAmount));
    }
  });
});

describe("camm computeSwap mirrors the program", () => {
  it("exact-in deducts fee then applies the curve", () => {
    const r = camm.computeSwap(1000n, 1000n, 30, 0, camm.SwapMode.ExactIn, 100n);
    expect(r.fee).toBe(1n);
    expect(r.amountIn).toBe(100n);
    expect(r.amountOut).toBe(90n);
    expect(r.protocolFee).toBe(0n);
    expect(r.lpFee).toBe(1n);
  });

  it("exact-out grosses up the input", () => {
    const r = camm.computeSwap(1000n, 1000n, 30, 0, camm.SwapMode.ExactOut, 90n);
    expect(r.amountOut).toBe(90n);
    expect(r.amountIn).toBeGreaterThanOrEqual(100n);
    expect(r.fee).toBe(r.amountIn - 99n);
  });

  it("splits the protocol fee out of the total", () => {
    const r = camm.computeSwap(1_000_000n, 1_000_000n, 100, 5000, camm.SwapMode.ExactIn, 10_000n);
    expect(r.protocolFee + r.lpFee).toBe(r.fee);
    expect(r.protocolFee).toBe(r.fee / 2n);
  });

  it("rejects zero amount and over-reserve output", () => {
    expect(() => camm.computeSwap(1000n, 1000n, 30, 0, camm.SwapMode.ExactIn, 0n)).toThrow(
      camm.CammQuoteError,
    );
    expect(() => camm.computeSwap(1000n, 1000n, 30, 0, camm.SwapMode.ExactOut, 1000n)).toThrow(
      camm.CammQuoteError,
    );
  });
});

describe("camm fee + yield helpers", () => {
  it("fee on input rounds up; split is exact", () => {
    expect(camm.feeOnInput(1000n, 30)).toBe(3n);
    expect(camm.feeOnInput(1n, 30)).toBe(1n);
    expect(camm.splitProtocolFee(100n, 2000)).toEqual([20n, 80n]);
    const [p, l] = camm.splitProtocolFee(7n, 2500);
    expect(p + l).toBe(7n);
  });

  it("accrued yield scales with principal/rate/time; deployable keeps the buffer", () => {
    expect(camm.accruedYield(1_000_000n, 1_000_000n, 10n)).toBe(10_000n);
    expect(camm.accruedYield(0n, 1_000_000n, 10n)).toBe(0n);
    expect(camm.deployable(1000n, 1000)).toBe(900n);
    expect(camm.deployable(1000n, 0)).toBe(1000n);
    expect(camm.deployable(1000n, 10_000)).toBe(0n);
  });
});

describe("camm PDA derivation", () => {
  const a = new PublicKey("So11111111111111111111111111111111111111112");
  const b = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

  it("is mint-order independent and deterministic", () => {
    expect(camm.poolPda(a, b).address.toBase58()).toBe(camm.poolPda(b, a).address.toBase58());
    const pool = camm.poolPda(a, b).address;
    // every derived account is distinct
    const addrs = [
      camm.poolAuthorityPda(pool).address.toBase58(),
      camm.reservePda(pool, a).address.toBase58(),
      camm.reservePda(pool, b).address.toBase58(),
      camm.lpMintPda(pool).address.toBase58(),
      camm.lockedLpPda(pool).address.toBase58(),
      camm.yieldSourcePda(pool, a).address.toBase58(),
      camm.yieldSourcePda(pool, b).address.toBase58(),
    ];
    expect(new Set(addrs).size).toBe(addrs.length);
  });
});
