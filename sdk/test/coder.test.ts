import { describe, expect, it } from "vitest";
import { PublicKey } from "@solana/web3.js";
import {
  decodeConfig,
  decodePool,
  decodePosition,
  DISCRIMINATORS,
  FeeSchedulerMode,
  PoolStatus,
  TokenFlavor,
} from "../src/coder/index.js";

// Golden bytes emitted by the program's own structs
// (`cargo test -p zenith-amm --test golden_account_bytes`). Each field carries
// a distinct value so a wrong offset in the decoder yields a wrong value.
const POOL_HEX =
  "f19a6d0411b16dbc010000000000000000000000000000000200000000000000000000000000000003000000000000000000000000000000040000000000000000000000000000000500000000000000000000000000000006000000000000000000000000000000070000000000000000000000000000000800000000000000000000000000000009000000000000000000000000000000000000000000000000000000000000000a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0f00000000000000100000000000000011000000000000001200000000000000130000000000000014000000000000001500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001600011718191a010000000000000000";
const POSITION_HEX =
  "aabc8fe47a40f7d01f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f20202020202020202020202020202020202020202020202020202020202020202100000000000000000000000000000022000000000000000000000000000000230000000000000000000000000000002400000000000000000000000000000025000000000000000000000000000000260000000000000027000000000000002801000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const CONFIG_HEX =
  "9b0caae01efacc8229292929292929292929292929292929292929292929292929292929292929292a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2c0000000000000000000000000000002d0000000000000000000000000000002e000000000000002f00300031003200330034003500000036000000370000003800000039003a003b00023c00000000000000000000000000000000";

const bytes = (hex: string) => Uint8Array.from(Buffer.from(hex, "hex"));
const key = (b: number) => new PublicKey(new Uint8Array(32).fill(b)).toBase58();

describe("Pool decoder parity", () => {
  const p = decodePool(bytes(POOL_HEX));

  it("decodes u128 price/fee fields", () => {
    expect(p.liquidity).toBe(1n);
    expect(p.sqrtPrice).toBe(2n);
    expect(p.sqrtMinPrice).toBe(3n);
    expect(p.sqrtMaxPrice).toBe(4n);
    expect(p.feeGrowthGlobalA).toBe(5n);
    expect(p.feeGrowthGlobalB).toBe(6n);
    expect(p.sqrtPriceReference).toBe(7n);
    expect(p.volatilityAccumulator).toBe(8n);
    expect(p.volatilityReference).toBe(9n);
  });

  it("decodes pubkeys past the reserved u128", () => {
    expect(p.config.toBase58()).toBe(key(10));
    expect(p.tokenAMint.toBase58()).toBe(key(11));
    expect(p.tokenBMint.toBase58()).toBe(key(12));
    expect(p.tokenAVault.toBase58()).toBe(key(13));
    expect(p.tokenBVault.toBase58()).toBe(key(14));
  });

  it("decodes u64 fields past the pubkeys", () => {
    expect(p.protocolFeeA).toBe(15n);
    expect(p.protocolFeeB).toBe(16n);
    expect(p.activationPoint).toBe(17n);
    expect(p.positionCount).toBe(18n);
    expect(p.lastVolatilityUpdate).toBe(19n);
    expect(p.partnerFeeA).toBe(20n);
    expect(p.partnerFeeB).toBe(21n);
  });

  it("decodes small trailing fields past reserved_u64", () => {
    expect(p.baseFeeBps).toBe(22);
    expect(p.status).toBe(PoolStatus.Active);
    expect(p.poolAuthorityBump).toBe(23);
    expect(p.poolBump).toBe(24);
    expect(p.tokenAVaultBump).toBe(25);
    expect(p.tokenBVaultBump).toBe(26);
    expect(p.tokenAFlavor).toBe(TokenFlavor.Token2022);
    expect(p.tokenBFlavor).toBe(TokenFlavor.SplToken);
  });
});

describe("Position decoder parity", () => {
  const p = decodePosition(bytes(POSITION_HEX));

  it("decodes all fields", () => {
    expect(p.pool.toBase58()).toBe(key(31));
    expect(p.nftMint.toBase58()).toBe(key(32));
    expect(p.liquidity).toBe(33n);
    expect(p.vestedLiquidity).toBe(34n);
    expect(p.permanentLockedLiquidity).toBe(35n);
    expect(p.feeGrowthCheckpointA).toBe(36n);
    expect(p.feeGrowthCheckpointB).toBe(37n);
    expect(p.feePendingA).toBe(38n);
    expect(p.feePendingB).toBe(39n);
    expect(p.bump).toBe(40);
    expect(p.compounding).toBe(1);
  });
});

describe("Config decoder parity", () => {
  const c = decodeConfig(bytes(CONFIG_HEX));

  it("decodes all fields in borsh order", () => {
    expect(c.admin.toBase58()).toBe(key(41));
    expect(c.feeAuthority.toBase58()).toBe(key(42));
    expect(c.partner.toBase58()).toBe(key(43));
    expect(c.sqrtMinPrice).toBe(44n);
    expect(c.sqrtMaxPrice).toBe(45n);
    expect(c.feePeriod).toBe(46n);
    expect(c.index).toBe(47);
    expect(c.baseFeeBps).toBe(48);
    expect(c.protocolFeeBps).toBe(49);
    expect(c.cliffFeeBps).toBe(50);
    expect(c.reductionFactor).toBe(51);
    expect(c.maxFeeSteps).toBe(52);
    expect(c.variableFeeControl).toBe(53);
    expect(c.maxVolatilityAccumulator).toBe(54);
    expect(c.filterPeriod).toBe(55);
    expect(c.decayPeriod).toBe(56);
    expect(c.volatilityReductionFactor).toBe(57);
    expect(c.maxDynamicFeeBps).toBe(58);
    expect(c.partnerFeeBps).toBe(59);
    expect(c.feeSchedulerMode).toBe(FeeSchedulerMode.Exponential);
    expect(c.bump).toBe(60);
  });
});

describe("discriminator guard", () => {
  it("rejects data with the wrong discriminator", () => {
    const wrong = bytes(POOL_HEX);
    wrong[0] ^= 0xff;
    expect(() => decodePool(wrong)).toThrow(/discriminator mismatch/);
  });

  it("rejects truncated data", () => {
    expect(() => decodePool(bytes("f19a6d04"))).toThrow(/too short/);
  });

  it("rejects a valid discriminator with a truncated body (no silent decode)", () => {
    // Correct Position discriminator, but only a few payload bytes: must fail
    // loud on the length check, never read adjacent memory for the pubkeys.
    const head = POSITION_HEX.slice(0, 16 + 8); // discriminator + 4 body bytes
    expect(() => decodePosition(bytes(head))).toThrow(/too short/);
  });

  it("exposes the three account discriminators", () => {
    expect(DISCRIMINATORS.Pool).toHaveLength(8);
    expect(DISCRIMINATORS.Position).toHaveLength(8);
    expect(DISCRIMINATORS.Config).toHaveLength(8);
  });
});
