//! Transaction-instruction builders for zenith-dlmm. Each builder resolves the
//! account metas in the exact order and with the exact signer/writable flags of
//! the program's `#[derive(Accounts)]`, and lays out the borsh args, so the
//! instruction is accepted on-chain.

import {
  type AccountMeta,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

import { Writer } from "../instructions/encode.js";
import { ZENITH_DLMM_PROGRAM_ID } from "./constants.js";
import { Direction, SwapMode } from "./math.js";

/// Anchor instruction discriminators: `sha256("global:<snake>")[..8]`.
export const DLMM_INSTRUCTION_DISCRIMINATORS = {
  initializeLbPair: [45, 154, 237, 210, 221, 15, 166, 92],
  initializeBinArray: [35, 86, 19, 185, 78, 212, 75, 211],
  initializeOracle: [144, 223, 131, 120, 196, 253, 181, 99],
  initializePosition: [219, 192, 234, 71, 190, 191, 102, 80],
  addLiquidityByStrategy: [7, 3, 150, 127, 148, 40, 61, 200],
  removeLiquidity: [80, 85, 209, 72, 24, 206, 177, 108],
  closePosition: [123, 134, 81, 0, 49, 68, 98, 98],
  swap: [248, 198, 158, 145, 225, 117, 135, 200],
  claimProtocolFee: [165, 228, 133, 48, 99, 249, 255, 33],
  claimFee: [169, 32, 79, 137, 136, 232, 70, 137],
} as const;

type IxName = keyof typeof DLMM_INSTRUCTION_DISCRIMINATORS;

function data(name: IxName, write?: (w: Writer) => void): Buffer {
  const w = new Writer();
  w.bytes([...DLMM_INSTRUCTION_DISCRIMINATORS[name]]);
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

export interface InitializeLbPairParams {
  creator: PublicKey;
  tokenXMint: PublicKey;
  tokenYMint: PublicKey;
  lbPair: PublicKey;
  pairAuthority: PublicKey;
  reserveX: PublicKey;
  reserveY: PublicKey;
  binStep: number;
  activeBinId: number;
  baseFeeBps: number;
  protocolFeeRate: number;
  variableFeeControl?: number;
  maxVolatilityAccumulator?: number;
  filterPeriod?: number;
  decayPeriod?: number;
  volatilityReductionFactor?: number;
  maxDynamicFeeBps?: number;
  programId?: PublicKey;
}

export function buildInitializeLbPair(p: InitializeLbPairParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [
    signer(p.creator, true),
    ro(p.tokenXMint),
    ro(p.tokenYMint),
    wr(p.lbPair),
    ro(p.pairAuthority),
    wr(p.reserveX),
    wr(p.reserveY),
    ro(TOKEN_PROGRAM_ID),
    ro(SystemProgram.programId),
  ];
  const d = data("initializeLbPair", (w) =>
    w
      .u16(p.binStep)
      .i32(p.activeBinId)
      .u16(p.baseFeeBps)
      .u16(p.protocolFeeRate)
      .u32(p.variableFeeControl ?? 0)
      .u32(p.maxVolatilityAccumulator ?? 0)
      .u32(p.filterPeriod ?? 0)
      .u32(p.decayPeriod ?? 0)
      .u16(p.volatilityReductionFactor ?? 0)
      .u16(p.maxDynamicFeeBps ?? 0),
  );
  return ix(keys, d, programId);
}

export function buildInitializeBinArray(p: {
  payer: PublicKey;
  lbPair: PublicKey;
  binArray: PublicKey;
  index: number | bigint;
  programId?: PublicKey;
}): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [signer(p.payer, true), ro(p.lbPair), wr(p.binArray), ro(SystemProgram.programId)];
  return ix(keys, data("initializeBinArray", (w) => w.i64(BigInt(p.index))), programId);
}

export function buildInitializeOracle(p: {
  payer: PublicKey;
  lbPair: PublicKey;
  oracle: PublicKey;
  length: number;
  programId?: PublicKey;
}): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [signer(p.payer, true), ro(p.lbPair), wr(p.oracle), ro(SystemProgram.programId)];
  return ix(keys, data("initializeOracle", (w) => w.u16(p.length)), programId);
}

export function buildInitializePosition(p: {
  owner: PublicKey;
  base: PublicKey;
  lbPair: PublicKey;
  position: PublicKey;
  lowerBinId: number;
  width: number;
  programId?: PublicKey;
}): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [
    signer(p.owner, true),
    signer(p.base, false),
    ro(p.lbPair),
    wr(p.position),
    ro(SystemProgram.programId),
  ];
  return ix(keys, data("initializePosition", (w) => w.i32(p.lowerBinId).u32(p.width)), programId);
}

export interface AddLiquidityParams {
  owner: PublicKey;
  lbPair: PublicKey;
  position: PublicKey;
  binArray: PublicKey;
  reserveX: PublicKey;
  reserveY: PublicKey;
  userTokenX: PublicKey;
  userTokenY: PublicKey;
  amountX: bigint;
  amountY: bigint;
  strategy: number; // 0 Spot, 1 Curve, 2 BidAsk
  minLiquidityShares?: bigint;
  expectedActiveBinId: number;
  activeIdSlippage?: number;
  programId?: PublicKey;
}

export function buildAddLiquidityByStrategy(p: AddLiquidityParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [
    signer(p.owner, false),
    ro(p.lbPair),
    wr(p.position),
    wr(p.binArray),
    wr(p.reserveX),
    wr(p.reserveY),
    wr(p.userTokenX),
    wr(p.userTokenY),
    ro(TOKEN_PROGRAM_ID),
  ];
  const d = data("addLiquidityByStrategy", (w) =>
    w
      .u64(p.amountX)
      .u64(p.amountY)
      .u8(p.strategy)
      .u128(p.minLiquidityShares ?? 0n)
      .i32(p.expectedActiveBinId)
      .u32(p.activeIdSlippage ?? 0),
  );
  return ix(keys, d, programId);
}

export interface RemoveLiquidityParams {
  owner: PublicKey;
  lbPair: PublicKey;
  position: PublicKey;
  binArray: PublicKey;
  pairAuthority: PublicKey;
  reserveX: PublicKey;
  reserveY: PublicKey;
  userTokenX: PublicKey;
  userTokenY: PublicKey;
  bps: number;
  minAmountX?: bigint;
  minAmountY?: bigint;
  programId?: PublicKey;
}

export function buildRemoveLiquidity(p: RemoveLiquidityParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [
    signer(p.owner, false),
    ro(p.lbPair),
    wr(p.position),
    wr(p.binArray),
    ro(p.pairAuthority),
    wr(p.reserveX),
    wr(p.reserveY),
    wr(p.userTokenX),
    wr(p.userTokenY),
    ro(TOKEN_PROGRAM_ID),
  ];
  const d = data("removeLiquidity", (w) =>
    w.u16(p.bps).u64(p.minAmountX ?? 0n).u64(p.minAmountY ?? 0n),
  );
  return ix(keys, d, programId);
}

export interface SwapParams {
  trader: PublicKey;
  lbPair: PublicKey;
  pairAuthority: PublicKey;
  reserveX: PublicKey;
  reserveY: PublicKey;
  userTokenX: PublicKey;
  userTokenY: PublicKey;
  /// Bin arrays the walk may cross (appended as remaining accounts, writable).
  binArrays: PublicKey[];
  /// Optional TWAP oracle (recorded when present).
  oracle?: PublicKey;
  direction: Direction;
  mode: SwapMode;
  amount: bigint;
  otherAmountThreshold: bigint;
  programId?: PublicKey;
}

export function buildSwap(p: SwapParams): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [
    signer(p.trader, false),
    wr(p.lbPair),
    ro(p.pairAuthority),
    wr(p.reserveX),
    wr(p.reserveY),
    wr(p.userTokenX),
    wr(p.userTokenY),
    ro(TOKEN_PROGRAM_ID),
    // optional oracle: pass the account (writable) when present, else the
    // program id as the anchor "None" placeholder.
    p.oracle ? wr(p.oracle) : ro(programId),
    ...p.binArrays.map((b) => wr(b)),
  ];
  const d = data("swap", (w) =>
    w.u8(p.direction).u8(p.mode).u64(p.amount).u64(p.otherAmountThreshold),
  );
  return ix(keys, d, programId);
}

export function buildClaimFee(p: {
  owner: PublicKey;
  lbPair: PublicKey;
  position: PublicKey;
  binArray: PublicKey;
  pairAuthority: PublicKey;
  reserveX: PublicKey;
  reserveY: PublicKey;
  userTokenX: PublicKey;
  userTokenY: PublicKey;
  programId?: PublicKey;
}): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [
    signer(p.owner, false),
    ro(p.lbPair),
    wr(p.position),
    ro(p.binArray),
    ro(p.pairAuthority),
    wr(p.reserveX),
    wr(p.reserveY),
    wr(p.userTokenX),
    wr(p.userTokenY),
    ro(TOKEN_PROGRAM_ID),
  ];
  return ix(keys, data("claimFee"), programId);
}

export function buildClaimProtocolFee(p: {
  authority: PublicKey;
  lbPair: PublicKey;
  pairAuthority: PublicKey;
  reserveX: PublicKey;
  reserveY: PublicKey;
  recipientTokenX: PublicKey;
  recipientTokenY: PublicKey;
  programId?: PublicKey;
}): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [
    signer(p.authority, false),
    wr(p.lbPair),
    ro(p.pairAuthority),
    wr(p.reserveX),
    wr(p.reserveY),
    wr(p.recipientTokenX),
    wr(p.recipientTokenY),
    ro(TOKEN_PROGRAM_ID),
  ];
  return ix(keys, data("claimProtocolFee"), programId);
}

export function buildClosePosition(p: {
  owner: PublicKey;
  position: PublicKey;
  programId?: PublicKey;
}): TransactionInstruction {
  const programId = p.programId ?? ZENITH_DLMM_PROGRAM_ID;
  const keys = [signer(p.owner, true), wr(p.position)];
  return ix(keys, data("closePosition"), programId);
}
