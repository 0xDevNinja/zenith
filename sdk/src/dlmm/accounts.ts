import type { PublicKey } from "@solana/web3.js";

import { Reader } from "../coder/reader.js";
import { BINS_PER_ARRAY, BINS_PER_POSITION, ORACLE_CAPACITY } from "./constants.js";

/// Anchor account discriminators for the zero-copy DLMM accounts:
/// `sha256("account:<Name>")[..8]`. The hand-written IDL carries none, so they
/// are precomputed here and verified against golden program bytes in the tests.
export const DLMM_DISCRIMINATORS = {
  LbPair: Uint8Array.from([33, 11, 49, 98, 181, 101, 177, 13]),
  BinArray: Uint8Array.from([92, 142, 92, 220, 5, 148, 70, 181]),
  Position: Uint8Array.from([170, 188, 143, 228, 122, 64, 247, 208]),
  Oracle: Uint8Array.from([139, 194, 131, 179, 140, 179, 229, 244]),
} as const;

const DISCRIMINATOR_LEN = 8;

/// Full on-chain byte length of each account (8-byte discriminator + payload),
/// mirroring the program's fixed-size zero-copy layouts. A shorter buffer is
/// rejected so a truncated/wrong account fails loud rather than decoding garbage.
export const DLMM_ACCOUNT_LEN = {
  LbPair: 8 + 384,
  BinArray: 8 + 4528,
  Position: 8 + 4592,
  Oracle: 8 + 2096,
} as const;

function checkAccount(name: keyof typeof DLMM_DISCRIMINATORS, data: Uint8Array): void {
  const want = DLMM_DISCRIMINATORS[name];
  if (data.length < DLMM_ACCOUNT_LEN[name]) {
    throw new Error(
      `${name}: account data too short (got ${data.length}, want ${DLMM_ACCOUNT_LEN[name]})`,
    );
  }
  for (let i = 0; i < DISCRIMINATOR_LEN; i++) {
    if (data[i] !== want[i]) {
      throw new Error(`${name}: discriminator mismatch (not a ${name} account)`);
    }
  }
}

/// Lifecycle status of a pair (mirrors the program's `PairStatus`).
export enum PairStatus {
  Uninitialized = 0,
  Active = 1,
  Disabled = 2,
}

/// Token program flavor of a mint (0 = SPL Token, 1 = Token-2022).
export enum DlmmTokenFlavor {
  SplToken = 0,
  Token2022 = 1,
}

// ---------------------------------------------------------------------------
// LbPair
// ---------------------------------------------------------------------------

/// Decoded `LbPair`. `u128`/`u64` fields are `bigint`; raw Q64.64 accumulators
/// keep their raw bits.
export interface LbPair {
  volatilityAccumulator: bigint;
  volatilityReference: bigint;
  tokenXMint: PublicKey;
  tokenYMint: PublicKey;
  reserveX: PublicKey;
  reserveY: PublicKey;
  creator: PublicKey;
  protocolFeeX: bigint;
  protocolFeeY: bigint;
  activationPoint: bigint;
  lastUpdateSlot: bigint;
  activeBinId: number;
  indexReference: number;
  variableFeeControl: number;
  maxVolatilityAccumulator: number;
  filterPeriod: number;
  decayPeriod: number;
  binStep: number;
  baseFeeBps: number;
  volatilityReductionFactor: number;
  maxDynamicFeeBps: number;
  protocolFeeRate: number;
  status: PairStatus;
  pairAuthorityBump: number;
  pairBump: number;
  reserveXBump: number;
  reserveYBump: number;
  tokenXFlag: DlmmTokenFlavor;
  tokenYFlag: DlmmTokenFlavor;
}

export function decodeLbPair(data: Uint8Array): LbPair {
  checkAccount("LbPair", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  const volatilityAccumulator = r.u128();
  const volatilityReference = r.u128();
  r.skip(16 * 4); // reserved_u128[4]
  const tokenXMint = r.pubkey();
  const tokenYMint = r.pubkey();
  const reserveX = r.pubkey();
  const reserveY = r.pubkey();
  const creator = r.pubkey();
  const protocolFeeX = r.u64();
  const protocolFeeY = r.u64();
  const activationPoint = r.u64();
  const lastUpdateSlot = r.u64();
  r.skip(8 * 5); // reserved_u64[5]
  const activeBinId = r.i32();
  const indexReference = r.i32();
  const variableFeeControl = r.u32();
  const maxVolatilityAccumulator = r.u32();
  const filterPeriod = r.u32();
  const decayPeriod = r.u32();
  const binStep = r.u16();
  const baseFeeBps = r.u16();
  const volatilityReductionFactor = r.u16();
  const maxDynamicFeeBps = r.u16();
  const protocolFeeRate = r.u16();
  const status = r.u8() as PairStatus;
  const pairAuthorityBump = r.u8();
  const pairBump = r.u8();
  const reserveXBump = r.u8();
  const reserveYBump = r.u8();
  const tokenXFlag = r.u8() as DlmmTokenFlavor;
  const tokenYFlag = r.u8() as DlmmTokenFlavor;
  return {
    volatilityAccumulator,
    volatilityReference,
    tokenXMint,
    tokenYMint,
    reserveX,
    reserveY,
    creator,
    protocolFeeX,
    protocolFeeY,
    activationPoint,
    lastUpdateSlot,
    activeBinId,
    indexReference,
    variableFeeControl,
    maxVolatilityAccumulator,
    filterPeriod,
    decayPeriod,
    binStep,
    baseFeeBps,
    volatilityReductionFactor,
    maxDynamicFeeBps,
    protocolFeeRate,
    status,
    pairAuthorityBump,
    pairBump,
    reserveXBump,
    reserveYBump,
    tokenXFlag,
    tokenYFlag,
  };
}

// ---------------------------------------------------------------------------
// BinArray
// ---------------------------------------------------------------------------

/// One bin's reserves and per-share fee growth.
export interface Bin {
  feeGrowthX: bigint;
  feeGrowthY: bigint;
  liquiditySupply: bigint;
  amountX: bigint;
  amountY: bigint;
}

export interface BinArray {
  bins: Bin[];
  lbPair: PublicKey;
  index: bigint;
  bump: number;
}

function readBin(r: Reader): Bin {
  return {
    feeGrowthX: r.u128(),
    feeGrowthY: r.u128(),
    liquiditySupply: r.u128(),
    amountX: r.u64(),
    amountY: r.u64(),
  };
}

export function decodeBinArray(data: Uint8Array): BinArray {
  checkAccount("BinArray", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  const bins: Bin[] = [];
  for (let i = 0; i < BINS_PER_ARRAY; i++) bins.push(readBin(r));
  const lbPair = r.pubkey();
  const index = r.i64();
  const bump = r.u8();
  return { bins, lbPair, index, bump };
}

/// The bin id of `bins[i]` in an array: `index * BINS_PER_ARRAY + i`.
export function binIdAt(index: bigint, slot: number): number {
  return Number(index) * BINS_PER_ARRAY + slot;
}

// ---------------------------------------------------------------------------
// Position
// ---------------------------------------------------------------------------

/// Per-bin fee accounting for a position.
export interface PositionBinData {
  feeXCheckpoint: bigint;
  feeYCheckpoint: bigint;
  feeXPending: bigint;
  feeYPending: bigint;
}

export interface Position {
  liquidityShares: bigint[];
  feeInfos: PositionBinData[];
  lbPair: PublicKey;
  owner: PublicKey;
  base: PublicKey;
  lowerBinId: number;
  upperBinId: number;
  bump: number;
}

export function decodePosition(data: Uint8Array): Position {
  checkAccount("Position", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  const liquidityShares: bigint[] = [];
  for (let i = 0; i < BINS_PER_POSITION; i++) liquidityShares.push(r.u128());
  const feeInfos: PositionBinData[] = [];
  for (let i = 0; i < BINS_PER_POSITION; i++) {
    feeInfos.push({
      feeXCheckpoint: r.u128(),
      feeYCheckpoint: r.u128(),
      feeXPending: r.u64(),
      feeYPending: r.u64(),
    });
  }
  const lbPair = r.pubkey();
  const owner = r.pubkey();
  const base = r.pubkey();
  const lowerBinId = r.i32();
  const upperBinId = r.i32();
  const bump = r.u8();
  return { liquidityShares, feeInfos, lbPair, owner, base, lowerBinId, upperBinId, bump };
}

// ---------------------------------------------------------------------------
// Oracle
// ---------------------------------------------------------------------------

/// One TWAP observation (cumulative active bin at a slot).
export interface Observation {
  cumulativeActiveBin: bigint;
  timestamp: bigint;
  initialized: number;
}

export interface Oracle {
  observations: Observation[];
  lbPair: PublicKey;
  length: number;
  activeSize: number;
  lastIndex: number;
  bump: number;
}

export function decodeOracle(data: Uint8Array): Oracle {
  checkAccount("Oracle", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  const observations: Observation[] = [];
  for (let i = 0; i < ORACLE_CAPACITY; i++) {
    const cumulativeActiveBin = r.i128();
    const timestamp = r.u64();
    const initialized = r.u8();
    r.skip(7); // padding
    observations.push({ cumulativeActiveBin, timestamp, initialized });
  }
  const lbPair = r.pubkey();
  const length = r.u16();
  const activeSize = r.u16();
  const lastIndex = r.u16();
  const bump = r.u8();
  return { observations, lbPair, length, activeSize, lastIndex, bump };
}
