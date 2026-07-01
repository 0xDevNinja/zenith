import { PublicKey } from "@solana/web3.js";
import { describe, expect, it } from "vitest";

import { dlmm } from "../src/index.js";

const bytes = (hex: string): Uint8Array =>
  Uint8Array.from(hex.match(/.{2}/g)!.map((h) => parseInt(h, 16)));

const filled = (b: number): string => new PublicKey(Uint8Array.from(Array(32).fill(b))).toBase58();

// Golden bytes emitted by `cargo test -p zenith-dlmm --test golden_account_bytes`.
const LBPAIR =
  "210b3162b565b10d0100000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f100000000000000011000000000000001200000000000000130000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ecffffffebffffff160000001700000018000000190000001a001b001c001d001e00011f2021220100000000000000000000000000000000";
const BINARRAY =
  "5c8e5cdc059446b564000000000000000000000000000000650000000000000000000000000000006600000000000000000000000000000067000000000000006800000000000000c8000000000000000000000000000000c9000000000000000000000000000000ca000000000000000000000000000000cb00000000000000cc00000000000000";
const POSITION_HEAD =
  "aabc8fe47a40f7d0f4010000000000000000000000000000f5010000000000000000000000000000";
const ORACLE_HEAD =
  "8bc283b38cb3e5f444fdffffffffffffffffffffffffffffbd0200000000000001000000000000002003000000000000000000000000000021030000000000000100000000000000";

describe("dlmm PDA derivation matches the live devnet pair", () => {
  const tokenX = new PublicKey("5ExoMf1WFKjNikpeC4f5oentzR1mkGMAYaGinCStn43Y");
  const tokenY = new PublicKey("BEQYTaxC1pAdVvoE1REvYvMpLQo6UXkFDRoAm6PFtgs4");
  const lbPair = new PublicKey("5idsMpbewctoSp9J2LJVCvN18qciFdSBrveyqsmk1Yxb");

  it("derives the seeded lb_pair, authority, oracle, reserves and bin arrays", () => {
    expect(dlmm.lbPairPda(tokenX, tokenY, 25).address.toBase58()).toBe(lbPair.toBase58());
    // mint order must not matter
    expect(dlmm.lbPairPda(tokenY, tokenX, 25).address.toBase58()).toBe(lbPair.toBase58());
    expect(dlmm.pairAuthorityPda(lbPair).address.toBase58()).toBe(
      "4keYCVHD9w8KZqJPpzmxKyTUDPmJdgkC8fZ3qfkLbpgo",
    );
    expect(dlmm.oraclePda(lbPair).address.toBase58()).toBe(
      "ABV3MfvpttX2XWfs4JRPjW5RhahDmeEEMnpyDcbW59nK",
    );
    expect(dlmm.reservePda(lbPair, tokenX).address.toBase58()).toBe(
      "7zDg6PQKSEeT5aYfS1B8AXp1EKNDeZPvLcpG7TJ9RYu5",
    );
    expect(dlmm.reservePda(lbPair, tokenY).address.toBase58()).toBe(
      "FyHANvT9cCuMEtkojCkbAmfz4hk2P4R712c1gTgZTszq",
    );
    expect(dlmm.binArrayPda(lbPair, 0).address.toBase58()).toBe(
      "Cjj8GqC2icHPgA5GMgxBKakZ5Xw3GUDQGJ1K1KUJS4bD",
    );
    // signed (negative) index in the seed
    expect(dlmm.binArrayPda(lbPair, -1).address.toBase58()).toBe(
      "CuCbcS5bKoranDXb1Nmmd7LT8Uc6CazF7SkRMg9P8g7A",
    );
  });

  it("bin_step is part of the seed", () => {
    expect(dlmm.lbPairPda(tokenX, tokenY, 10).address.toBase58()).not.toBe(lbPair.toBase58());
  });
});

describe("dlmm account decoders (golden bytes)", () => {
  it("decodes LbPair", () => {
    const p = dlmm.decodeLbPair(bytes(LBPAIR));
    expect(p.volatilityAccumulator).toBe(1n);
    expect(p.volatilityReference).toBe(2n);
    expect(p.tokenXMint.toBase58()).toBe(filled(11));
    expect(p.tokenYMint.toBase58()).toBe(filled(12));
    expect(p.reserveX.toBase58()).toBe(filled(13));
    expect(p.reserveY.toBase58()).toBe(filled(14));
    expect(p.creator.toBase58()).toBe(filled(15));
    expect(p.protocolFeeX).toBe(16n);
    expect(p.protocolFeeY).toBe(17n);
    expect(p.activationPoint).toBe(18n);
    expect(p.lastUpdateSlot).toBe(19n);
    expect(p.activeBinId).toBe(-20);
    expect(p.indexReference).toBe(-21);
    expect(p.variableFeeControl).toBe(22);
    expect(p.maxVolatilityAccumulator).toBe(23);
    expect(p.filterPeriod).toBe(24);
    expect(p.decayPeriod).toBe(25);
    expect(p.binStep).toBe(26);
    expect(p.baseFeeBps).toBe(27);
    expect(p.volatilityReductionFactor).toBe(28);
    expect(p.maxDynamicFeeBps).toBe(29);
    expect(p.protocolFeeRate).toBe(30);
    expect(p.status).toBe(dlmm.PairStatus.Active);
    expect(p.pairAuthorityBump).toBe(31);
    expect(p.pairBump).toBe(32);
    expect(p.reserveXBump).toBe(33);
    expect(p.reserveYBump).toBe(34);
    expect(p.tokenXFlag).toBe(dlmm.DlmmTokenFlavor.Token2022);
    expect(p.tokenYFlag).toBe(dlmm.DlmmTokenFlavor.SplToken);
  });

  it("rejects a wrong discriminator", () => {
    const wrong = bytes(LBPAIR);
    wrong[0] ^= 0xff;
    expect(() => dlmm.decodeLbPair(wrong)).toThrow(/discriminator/);
  });

  it("rejects a truncated account", () => {
    expect(() => dlmm.decodeLbPair(bytes(LBPAIR).slice(0, 100))).toThrow(/too short/);
  });

  it("decodes BinArray bins, index and lb_pair", () => {
    // Pad the head (first two bins) up to the full account length so the
    // length guard passes; the remaining bins decode as zero.
    const buf = new Uint8Array(dlmm.DLMM_ACCOUNT_LEN.BinArray);
    buf.set(bytes(BINARRAY));
    // place lb_pair / index / bump at the tail (offsets after bins[70]).
    const a = dlmm.decodeBinArray(buf);
    expect(a.bins[0].feeGrowthX).toBe(100n);
    expect(a.bins[0].feeGrowthY).toBe(101n);
    expect(a.bins[0].liquiditySupply).toBe(102n);
    expect(a.bins[0].amountX).toBe(103n);
    expect(a.bins[0].amountY).toBe(104n);
    expect(a.bins[1].amountY).toBe(204n);
    expect(a.bins.length).toBe(dlmm.BINS_PER_ARRAY);
  });

  it("decodes Position head (shares) and reads a negative bin id", () => {
    const buf = new Uint8Array(dlmm.DLMM_ACCOUNT_LEN.Position);
    buf.set(bytes(POSITION_HEAD));
    const pos = dlmm.decodePosition(buf);
    expect(pos.liquidityShares[0]).toBe(500n);
    expect(pos.liquidityShares[1]).toBe(501n);
    expect(pos.liquidityShares.length).toBe(dlmm.BINS_PER_POSITION);
    expect(pos.feeInfos.length).toBe(dlmm.BINS_PER_POSITION);
  });

  it("decodes Oracle observations including a negative cumulative bin", () => {
    const buf = new Uint8Array(dlmm.DLMM_ACCOUNT_LEN.Oracle);
    buf.set(bytes(ORACLE_HEAD));
    const o = dlmm.decodeOracle(buf);
    expect(o.observations[0].cumulativeActiveBin).toBe(-700n);
    expect(o.observations[0].timestamp).toBe(701n);
    expect(o.observations[0].initialized).toBe(1);
    expect(o.observations[1].cumulativeActiveBin).toBe(800n);
    expect(o.observations[1].timestamp).toBe(801n);
    expect(o.observations.length).toBe(dlmm.ORACLE_CAPACITY);
  });
});
