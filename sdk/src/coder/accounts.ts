import type { PublicKey } from "@solana/web3.js";
import { Reader } from "./reader.js";

/// Anchor account discriminators: `sha256("account:<Name>")[..8]`. The IDL is
/// hand-written and carries none, so they are precomputed here. Stable for a
/// given account name; verified against the program in the test suite.
export const DISCRIMINATORS = {
  Pool: Uint8Array.from([241, 154, 109, 4, 17, 177, 109, 188]),
  Position: Uint8Array.from([170, 188, 143, 228, 122, 64, 247, 208]),
  Config: Uint8Array.from([155, 12, 170, 224, 30, 250, 204, 130]),
} as const;

const DISCRIMINATOR_LEN = 8;

function checkDiscriminator(name: keyof typeof DISCRIMINATORS, data: Uint8Array): void {
  const want = DISCRIMINATORS[name];
  if (data.length < DISCRIMINATOR_LEN) {
    throw new Error(`${name}: account data too short (${data.length} bytes)`);
  }
  for (let i = 0; i < DISCRIMINATOR_LEN; i++) {
    if (data[i] !== want[i]) {
      throw new Error(`${name}: discriminator mismatch (not a ${name} account)`);
    }
  }
}

/// Lifecycle status of a pool (mirrors the program's `PoolStatus`).
export enum PoolStatus {
  Uninitialized = 0,
  Active = 1,
  Disabled = 2,
}

/// Token program flavor of a mint (mirrors the program's `TokenFlavor`).
export enum TokenFlavor {
  SplToken = 0,
  Token2022 = 1,
}

/// Decoded `Pool` (zero-copy account). `u128`/`u64` fields are `bigint`; raw
/// Q64.64 prices and fee accumulators are kept as raw bits.
export interface Pool {
  liquidity: bigint;
  sqrtPrice: bigint;
  sqrtMinPrice: bigint;
  sqrtMaxPrice: bigint;
  feeGrowthGlobalA: bigint;
  feeGrowthGlobalB: bigint;
  sqrtPriceReference: bigint;
  volatilityAccumulator: bigint;
  volatilityReference: bigint;
  config: PublicKey;
  tokenAMint: PublicKey;
  tokenBMint: PublicKey;
  tokenAVault: PublicKey;
  tokenBVault: PublicKey;
  protocolFeeA: bigint;
  protocolFeeB: bigint;
  activationPoint: bigint;
  positionCount: bigint;
  lastVolatilityUpdate: bigint;
  partnerFeeA: bigint;
  partnerFeeB: bigint;
  baseFeeBps: number;
  status: PoolStatus;
  poolAuthorityBump: number;
  poolBump: number;
  tokenAVaultBump: number;
  tokenBVaultBump: number;
  tokenAFlavor: TokenFlavor;
  tokenBFlavor: TokenFlavor;
}

/// Decode a `Pool` account from raw bytes (including the 8-byte discriminator).
export function decodePool(data: Uint8Array): Pool {
  checkDiscriminator("Pool", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  const liquidity = r.u128();
  const sqrtPrice = r.u128();
  const sqrtMinPrice = r.u128();
  const sqrtMaxPrice = r.u128();
  const feeGrowthGlobalA = r.u128();
  const feeGrowthGlobalB = r.u128();
  const sqrtPriceReference = r.u128();
  const volatilityAccumulator = r.u128();
  const volatilityReference = r.u128();
  r.skip(16); // reserved_u128: [u128; 1]
  const config = r.pubkey();
  const tokenAMint = r.pubkey();
  const tokenBMint = r.pubkey();
  const tokenAVault = r.pubkey();
  const tokenBVault = r.pubkey();
  const protocolFeeA = r.u64();
  const protocolFeeB = r.u64();
  const activationPoint = r.u64();
  const positionCount = r.u64();
  const lastVolatilityUpdate = r.u64();
  const partnerFeeA = r.u64();
  const partnerFeeB = r.u64();
  r.skip(40); // reserved_u64: [u64; 5]
  const baseFeeBps = r.u16();
  const status = r.u8();
  const poolAuthorityBump = r.u8();
  const poolBump = r.u8();
  const tokenAVaultBump = r.u8();
  const tokenBVaultBump = r.u8();
  const tokenAFlavor = r.u8();
  const tokenBFlavor = r.u8();
  // trailing padding: [u8; 7] — not read

  return {
    liquidity,
    sqrtPrice,
    sqrtMinPrice,
    sqrtMaxPrice,
    feeGrowthGlobalA,
    feeGrowthGlobalB,
    sqrtPriceReference,
    volatilityAccumulator,
    volatilityReference,
    config,
    tokenAMint,
    tokenBMint,
    tokenAVault,
    tokenBVault,
    protocolFeeA,
    protocolFeeB,
    activationPoint,
    positionCount,
    lastVolatilityUpdate,
    partnerFeeA,
    partnerFeeB,
    baseFeeBps,
    status: status as PoolStatus,
    poolAuthorityBump,
    poolBump,
    tokenAVaultBump,
    tokenBVaultBump,
    tokenAFlavor: tokenAFlavor as TokenFlavor,
    tokenBFlavor: tokenBFlavor as TokenFlavor,
  };
}

/// Decoded `Position` account (borsh).
export interface Position {
  pool: PublicKey;
  nftMint: PublicKey;
  liquidity: bigint;
  vestedLiquidity: bigint;
  permanentLockedLiquidity: bigint;
  feeGrowthCheckpointA: bigint;
  feeGrowthCheckpointB: bigint;
  feePendingA: bigint;
  feePendingB: bigint;
  bump: number;
  compounding: number;
}

/// Decode a `Position` account from raw bytes (including the discriminator).
export function decodePosition(data: Uint8Array): Position {
  checkDiscriminator("Position", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  const pool = r.pubkey();
  const nftMint = r.pubkey();
  const liquidity = r.u128();
  const vestedLiquidity = r.u128();
  const permanentLockedLiquidity = r.u128();
  const feeGrowthCheckpointA = r.u128();
  const feeGrowthCheckpointB = r.u128();
  const feePendingA = r.u64();
  const feePendingB = r.u64();
  const bump = r.u8();
  const compounding = r.u8();
  // reserved: [u8; 63] — not read

  return {
    pool,
    nftMint,
    liquidity,
    vestedLiquidity,
    permanentLockedLiquidity,
    feeGrowthCheckpointA,
    feeGrowthCheckpointB,
    feePendingA,
    feePendingB,
    bump,
    compounding,
  };
}

/// Fee scheduler mode (mirrors the program: 0 = Constant, 1 = Linear, 2 = Exponential).
export enum FeeSchedulerMode {
  Constant = 0,
  Linear = 1,
  Exponential = 2,
}

/// Decoded `Config` account (borsh).
export interface Config {
  admin: PublicKey;
  feeAuthority: PublicKey;
  partner: PublicKey;
  sqrtMinPrice: bigint;
  sqrtMaxPrice: bigint;
  feePeriod: bigint;
  index: number;
  baseFeeBps: number;
  protocolFeeBps: number;
  cliffFeeBps: number;
  reductionFactor: number;
  maxFeeSteps: number;
  variableFeeControl: number;
  maxVolatilityAccumulator: number;
  filterPeriod: number;
  decayPeriod: number;
  volatilityReductionFactor: number;
  maxDynamicFeeBps: number;
  partnerFeeBps: number;
  feeSchedulerMode: FeeSchedulerMode;
  bump: number;
}

/// Decode a `Config` account from raw bytes (including the discriminator).
export function decodeConfig(data: Uint8Array): Config {
  checkDiscriminator("Config", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  const admin = r.pubkey();
  const feeAuthority = r.pubkey();
  const partner = r.pubkey();
  const sqrtMinPrice = r.u128();
  const sqrtMaxPrice = r.u128();
  const feePeriod = r.u64();
  const index = r.u16();
  const baseFeeBps = r.u16();
  const protocolFeeBps = r.u16();
  const cliffFeeBps = r.u16();
  const reductionFactor = r.u16();
  const maxFeeSteps = r.u16();
  const variableFeeControl = r.u32();
  const maxVolatilityAccumulator = r.u32();
  const filterPeriod = r.u32();
  const decayPeriod = r.u32();
  const volatilityReductionFactor = r.u16();
  const maxDynamicFeeBps = r.u16();
  const partnerFeeBps = r.u16();
  const feeSchedulerMode = r.u8();
  const bump = r.u8();
  // reserved: [u8; 16] — not read

  return {
    admin,
    feeAuthority,
    partner,
    sqrtMinPrice,
    sqrtMaxPrice,
    feePeriod,
    index,
    baseFeeBps,
    protocolFeeBps,
    cliffFeeBps,
    reductionFactor,
    maxFeeSteps,
    variableFeeControl,
    maxVolatilityAccumulator,
    filterPeriod,
    decayPeriod,
    volatilityReductionFactor,
    maxDynamicFeeBps,
    partnerFeeBps,
    feeSchedulerMode: feeSchedulerMode as FeeSchedulerMode,
    bump,
  };
}
