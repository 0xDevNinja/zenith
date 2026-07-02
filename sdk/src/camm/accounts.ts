import type { PublicKey } from "@solana/web3.js";

import { Reader } from "../coder/reader.js";

/// Anchor account discriminator for the zero-copy `Pool`:
/// `sha256("account:Pool")[..8]`. (Same struct name as the AMM's `Pool`, so the
/// bytes match — the accounts are disambiguated by owning program and by the
/// fixed account length below.)
export const CAMM_DISCRIMINATORS = {
  Pool: Uint8Array.from([241, 154, 109, 4, 17, 177, 109, 188]),
} as const;

const DISCRIMINATOR_LEN = 8;

/// Full on-chain byte length of the `Pool` account (8-byte discriminator + the
/// 400-byte zero-copy payload). A shorter buffer is rejected so a truncated or
/// wrong account fails loud rather than decoding garbage.
export const CAMM_ACCOUNT_LEN = {
  Pool: 8 + 400,
} as const;

function checkAccount(name: keyof typeof CAMM_DISCRIMINATORS, data: Uint8Array): void {
  const want = CAMM_DISCRIMINATORS[name];
  if (data.length < CAMM_ACCOUNT_LEN[name]) {
    throw new Error(
      `${name}: account data too short (got ${data.length}, want ${CAMM_ACCOUNT_LEN[name]})`,
    );
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

/// Token program flavor of a mint (0 = SPL Token, 1 = Token-2022).
export enum CammTokenFlavor {
  SplToken = 0,
  Token2022 = 1,
}

/// Decoded constant-product `Pool`. `u64` fields are `bigint`. The reserves are
/// the *curve* reserves (what LP shares are backed by), tracked separately from
/// the physical vault balance; accrued protocol fees sit in `protocolFee*`.
export interface Pool {
  tokenAMint: PublicKey;
  tokenBMint: PublicKey;
  reserveAVault: PublicKey;
  reserveBVault: PublicKey;
  lpMint: PublicKey;
  lockedLp: PublicKey;
  creator: PublicKey;
  reserveA: bigint;
  reserveB: bigint;
  protocolFeeA: bigint;
  protocolFeeB: bigint;
  activationPoint: bigint;
  deployedA: bigint;
  deployedB: bigint;
  lastAccrualSlot: bigint;
  yieldRate: bigint;
  bufferBps: bigint;
  baseFeeBps: number;
  protocolFeeRate: number;
  status: PoolStatus;
  poolAuthorityBump: number;
  reserveABump: number;
  reserveBBump: number;
  lpMintBump: number;
  lockedLpBump: number;
  tokenAFlag: CammTokenFlavor;
  tokenBFlag: CammTokenFlavor;
}

export function decodePool(data: Uint8Array): Pool {
  checkAccount("Pool", data);
  const r = new Reader(data, DISCRIMINATOR_LEN);
  r.skip(16 * 4); // reserved_u128[4]
  const tokenAMint = r.pubkey();
  const tokenBMint = r.pubkey();
  const reserveAVault = r.pubkey();
  const reserveBVault = r.pubkey();
  const lpMint = r.pubkey();
  const lockedLp = r.pubkey();
  const creator = r.pubkey();
  const reserveA = r.u64();
  const reserveB = r.u64();
  const protocolFeeA = r.u64();
  const protocolFeeB = r.u64();
  const activationPoint = r.u64();
  const deployedA = r.u64();
  const deployedB = r.u64();
  const lastAccrualSlot = r.u64();
  const yieldRate = r.u64();
  const bufferBps = r.u64();
  r.skip(8); // reserved_u64[1]
  const baseFeeBps = r.u16();
  const protocolFeeRate = r.u16();
  const status = r.u8() as PoolStatus;
  const poolAuthorityBump = r.u8();
  const reserveABump = r.u8();
  const reserveBBump = r.u8();
  const lpMintBump = r.u8();
  const lockedLpBump = r.u8();
  const tokenAFlag = r.u8() as CammTokenFlavor;
  const tokenBFlag = r.u8() as CammTokenFlavor;
  return {
    tokenAMint,
    tokenBMint,
    reserveAVault,
    reserveBVault,
    lpMint,
    lockedLp,
    creator,
    reserveA,
    reserveB,
    protocolFeeA,
    protocolFeeB,
    activationPoint,
    deployedA,
    deployedB,
    lastAccrualSlot,
    yieldRate,
    bufferBps,
    baseFeeBps,
    protocolFeeRate,
    status,
    poolAuthorityBump,
    reserveABump,
    reserveBBump,
    lpMintBump,
    lockedLpBump,
    tokenAFlag,
    tokenBFlag,
  };
}
