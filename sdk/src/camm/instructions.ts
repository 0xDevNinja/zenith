//! Transaction-instruction builders for zenith-camm. Each builder emits the
//! account metas in the exact order and signer/writable flags of the program's
//! `#[derive(Accounts)]`, and lays out the borsh args, so the instruction is
//! accepted on-chain.

import {
  type AccountMeta,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  TransactionInstruction,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

import { Writer } from "../instructions/encode.js";
import { ZENITH_CAMM_PROGRAM_ID } from "./constants.js";
import { Direction, SwapMode } from "./math.js";

/// Anchor instruction discriminators: `sha256("global:<snake>")[..8]`.
export const CAMM_INSTRUCTION_DISCRIMINATORS = {
  initializePool: [95, 180, 10, 172, 84, 174, 232, 40],
  addLiquidity: [181, 157, 89, 67, 143, 182, 52, 72],
  removeLiquidity: [80, 85, 209, 72, 24, 206, 177, 108],
  swap: [248, 198, 158, 145, 225, 117, 135, 200],
  initializeYield: [163, 202, 79, 101, 57, 190, 112, 240],
  harvestYield: [28, 200, 150, 200, 69, 56, 38, 133],
  rebalanceToVault: [56, 77, 71, 81, 85, 60, 149, 5],
} as const;

type IxName = keyof typeof CAMM_INSTRUCTION_DISCRIMINATORS;

function data(name: IxName, write?: (w: Writer) => void): Buffer {
  const w = new Writer();
  w.bytes([...CAMM_INSTRUCTION_DISCRIMINATORS[name]]);
  write?.(w);
  return w.build();
}

const meta = (pubkey: PublicKey, isSigner: boolean, isWritable: boolean): AccountMeta => ({
  pubkey,
  isSigner,
  isWritable,
});
const ro = (pubkey: PublicKey): AccountMeta => meta(pubkey, false, false);
const wr = (pubkey: PublicKey): AccountMeta => meta(pubkey, false, true);
const signer = (pubkey: PublicKey, writable: boolean): AccountMeta => meta(pubkey, true, writable);

const ix = (keys: AccountMeta[], d: Buffer, programId: PublicKey): TransactionInstruction =>
  new TransactionInstruction({ programId, keys, data: d });

export interface InitializePoolParams {
  creator: PublicKey;
  tokenAMint: PublicKey;
  tokenBMint: PublicKey;
  pool: PublicKey;
  poolAuthority: PublicKey;
  reserveAVault: PublicKey;
  reserveBVault: PublicKey;
  lpMint: PublicKey;
  lockedLp: PublicKey;
  baseFeeBps: number;
  protocolFeeRate: number;
  programId?: PublicKey;
}

export function buildInitializePool(p: InitializePoolParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_CAMM_PROGRAM_ID;
  const keys = [
    signer(p.creator, true),
    ro(p.tokenAMint),
    ro(p.tokenBMint),
    wr(p.pool),
    ro(p.poolAuthority),
    wr(p.reserveAVault),
    wr(p.reserveBVault),
    wr(p.lpMint),
    wr(p.lockedLp),
    ro(TOKEN_PROGRAM_ID),
    ro(SystemProgram.programId),
    ro(SYSVAR_RENT_PUBKEY),
  ];
  const d = data("initializePool", (w) => w.u16(p.baseFeeBps).u16(p.protocolFeeRate));
  return ix(keys, d, programId);
}

export interface AddLiquidityParams {
  owner: PublicKey;
  pool: PublicKey;
  poolAuthority: PublicKey;
  lpMint: PublicKey;
  lockedLp: PublicKey;
  reserveAVault: PublicKey;
  reserveBVault: PublicKey;
  userTokenA: PublicKey;
  userTokenB: PublicKey;
  userLp: PublicKey;
  desiredA: bigint;
  desiredB: bigint;
  minA?: bigint;
  minB?: bigint;
  minShares?: bigint;
  programId?: PublicKey;
}

export function buildAddLiquidity(p: AddLiquidityParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_CAMM_PROGRAM_ID;
  const keys = [
    signer(p.owner, false),
    wr(p.pool),
    ro(p.poolAuthority),
    wr(p.lpMint),
    wr(p.lockedLp),
    wr(p.reserveAVault),
    wr(p.reserveBVault),
    wr(p.userTokenA),
    wr(p.userTokenB),
    wr(p.userLp),
    ro(TOKEN_PROGRAM_ID),
  ];
  const d = data("addLiquidity", (w) =>
    w
      .u64(p.desiredA)
      .u64(p.desiredB)
      .u64(p.minA ?? 0n)
      .u64(p.minB ?? 0n)
      .u64(p.minShares ?? 0n),
  );
  return ix(keys, d, programId);
}

export interface RemoveLiquidityParams {
  owner: PublicKey;
  pool: PublicKey;
  poolAuthority: PublicKey;
  lpMint: PublicKey;
  reserveAVault: PublicKey;
  reserveBVault: PublicKey;
  userTokenA: PublicKey;
  userTokenB: PublicKey;
  userLp: PublicKey;
  shares: bigint;
  minA?: bigint;
  minB?: bigint;
  programId?: PublicKey;
}

export function buildRemoveLiquidity(p: RemoveLiquidityParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_CAMM_PROGRAM_ID;
  const keys = [
    signer(p.owner, false),
    wr(p.pool),
    ro(p.poolAuthority),
    wr(p.lpMint),
    wr(p.reserveAVault),
    wr(p.reserveBVault),
    wr(p.userTokenA),
    wr(p.userTokenB),
    wr(p.userLp),
    ro(TOKEN_PROGRAM_ID),
  ];
  const d = data("removeLiquidity", (w) => w.u64(p.shares).u64(p.minA ?? 0n).u64(p.minB ?? 0n));
  return ix(keys, d, programId);
}

export interface SwapParams {
  user: PublicKey;
  pool: PublicKey;
  poolAuthority: PublicKey;
  reserveAVault: PublicKey;
  reserveBVault: PublicKey;
  userTokenA: PublicKey;
  userTokenB: PublicKey;
  direction: Direction;
  mode: SwapMode;
  amount: bigint;
  otherAmountThreshold: bigint;
  programId?: PublicKey;
}

export function buildSwap(p: SwapParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_CAMM_PROGRAM_ID;
  const keys = [
    signer(p.user, false),
    wr(p.pool),
    ro(p.poolAuthority),
    wr(p.reserveAVault),
    wr(p.reserveBVault),
    wr(p.userTokenA),
    wr(p.userTokenB),
    ro(TOKEN_PROGRAM_ID),
  ];
  const d = data("swap", (w) =>
    w.u8(p.direction).u8(p.mode).u64(p.amount).u64(p.otherAmountThreshold),
  );
  return ix(keys, d, programId);
}

export interface InitializeYieldParams {
  creator: PublicKey;
  pool: PublicKey;
  tokenAMint: PublicKey;
  tokenBMint: PublicKey;
  poolAuthority: PublicKey;
  yieldSourceA: PublicKey;
  yieldSourceB: PublicKey;
  yieldRate: bigint;
  bufferBps: number;
  programId?: PublicKey;
}

export function buildInitializeYield(p: InitializeYieldParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_CAMM_PROGRAM_ID;
  const keys = [
    signer(p.creator, true),
    wr(p.pool),
    ro(p.tokenAMint),
    ro(p.tokenBMint),
    ro(p.poolAuthority),
    wr(p.yieldSourceA),
    wr(p.yieldSourceB),
    ro(TOKEN_PROGRAM_ID),
    ro(SystemProgram.programId),
    ro(SYSVAR_RENT_PUBKEY),
  ];
  const d = data("initializeYield", (w) => w.u64(p.yieldRate).u16(p.bufferBps));
  return ix(keys, d, programId);
}

/// Accounts shared by `harvest_yield` and `rebalance_to_vault`.
export interface YieldAccrueParams {
  caller: PublicKey;
  pool: PublicKey;
  poolAuthority: PublicKey;
  yieldSourceA: PublicKey;
  yieldSourceB: PublicKey;
  reserveAVault: PublicKey;
  reserveBVault: PublicKey;
  tokenAMint: PublicKey;
  tokenBMint: PublicKey;
  programId?: PublicKey;
}

function yieldAccrueKeys(p: YieldAccrueParams): AccountMeta[] {
  return [
    signer(p.caller, false),
    wr(p.pool),
    ro(p.poolAuthority),
    wr(p.yieldSourceA),
    wr(p.yieldSourceB),
    wr(p.reserveAVault),
    wr(p.reserveBVault),
    ro(p.tokenAMint),
    ro(p.tokenBMint),
    ro(TOKEN_PROGRAM_ID),
  ];
}

export function buildHarvestYield(p: YieldAccrueParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_CAMM_PROGRAM_ID;
  return ix(yieldAccrueKeys(p), data("harvestYield"), programId);
}

export function buildRebalanceToVault(p: YieldAccrueParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_CAMM_PROGRAM_ID;
  return ix(yieldAccrueKeys(p), data("rebalanceToVault"), programId);
}
