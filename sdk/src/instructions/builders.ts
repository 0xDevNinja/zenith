import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  type AccountMeta,
  Keypair,
  PublicKey,
  type Signer,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import { ZENITH_AMM_PROGRAM_ID } from "../constants.js";
import { SwapDirection, SwapMode } from "../math/index.js";
import {
  configPda,
  poolAuthorityPda,
  poolPda,
  positionPda,
  sortMints,
  vaultPda,
} from "../pda.js";
import { ixData } from "./encode.js";

/// A builder's output: the instruction(s) to include, plus any extra signers
/// the client must add (e.g. a freshly generated position-NFT mint keypair).
export interface Built {
  instructions: TransactionInstruction[];
  signers: Signer[];
  /// Notable addresses the builder derived (for the caller's convenience).
  derived: Record<string, PublicKey>;
}

const r = (pubkey: PublicKey, isSigner = false, isWritable = false): AccountMeta => ({
  pubkey,
  isSigner,
  isWritable,
});
const ata = (mint: PublicKey, owner: PublicKey): PublicKey =>
  getAssociatedTokenAddressSync(mint, owner, true);

/// SwapDirection / SwapMode as their borsh variant indices (declaration order).
const DIRECTION_INDEX: Record<SwapDirection, number> = {
  [SwapDirection.AToB]: 0,
  [SwapDirection.BToA]: 1,
};
const MODE_INDEX: Record<SwapMode, number> = {
  [SwapMode.ExactIn]: 0,
  [SwapMode.ExactOut]: 1,
  [SwapMode.PartialFill]: 2,
};

/// Full parameter set for a Config template.
export interface CreateConfigParams {
  index: number;
  feeAuthority: PublicKey;
  sqrtMinPrice: bigint;
  sqrtMaxPrice: bigint;
  baseFeeBps: number;
  protocolFeeBps: number;
  partner: PublicKey;
  partnerFeeBps: number;
  feeSchedulerMode: number;
  cliffFeeBps: number;
  reductionFactor: number;
  feePeriod: bigint;
  maxFeeSteps: number;
  variableFeeControl: number;
  maxVolatilityAccumulator: number;
  filterPeriod: number;
  decayPeriod: number;
  volatilityReductionFactor: number;
  maxDynamicFeeBps: number;
}

/// `create_config` — stamp a reusable pool template at `index`.
export function buildCreateConfig(
  args: { admin: PublicKey; params: CreateConfigParams },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const p = args.params;
  const config = configPda(p.index, programId).address;
  const data = ixData("createConfig", (w) => {
    w.u16(p.index)
      .pubkey(p.feeAuthority)
      .u128(p.sqrtMinPrice)
      .u128(p.sqrtMaxPrice)
      .u16(p.baseFeeBps)
      .u16(p.protocolFeeBps)
      .pubkey(p.partner)
      .u16(p.partnerFeeBps)
      .u8(p.feeSchedulerMode)
      .u16(p.cliffFeeBps)
      .u16(p.reductionFactor)
      .u64(p.feePeriod)
      .u16(p.maxFeeSteps)
      .u32(p.variableFeeControl)
      .u32(p.maxVolatilityAccumulator)
      .u32(p.filterPeriod)
      .u32(p.decayPeriod)
      .u16(p.volatilityReductionFactor)
      .u16(p.maxDynamicFeeBps);
  });
  const keys = [r(args.admin, true, true), r(config, false, true), r(SystemProgram.programId)];
  return {
    instructions: [new TransactionInstruction({ programId, keys, data })],
    signers: [],
    derived: { config },
  };
}

/// `initialize_pool` — open a live pool from a config and seed the first
/// position. Generates the position-NFT mint keypair (returned as a signer).
export function buildInitializePool(
  args: {
    creator: PublicKey;
    config: PublicKey;
    mintA: PublicKey;
    mintB: PublicKey;
    sqrtPrice: bigint;
    liquidity: bigint;
    tokenAMax: bigint;
    tokenBMax: bigint;
  },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const [tokenA, tokenB] = sortMints(args.mintA, args.mintB);
  const pool = poolPda(args.config, tokenA, tokenB, programId).address;
  const poolAuthority = poolAuthorityPda(pool, programId).address;
  const tokenAVault = vaultPda(pool, tokenA, programId).address;
  const tokenBVault = vaultPda(pool, tokenB, programId).address;
  const nftMintKp = Keypair.generate();
  const nftMint = nftMintKp.publicKey;
  const position = positionPda(nftMint, programId).address;
  const positionNftAccount = ata(nftMint, args.creator);

  const data = ixData("initializePool", (w) => {
    w.u128(args.sqrtPrice).u128(args.liquidity).u64(args.tokenAMax).u64(args.tokenBMax);
  });

  const keys = [
    r(args.creator, true, true),
    r(args.config),
    r(tokenA),
    r(tokenB),
    r(pool, false, true),
    r(poolAuthority),
    r(tokenAVault, false, true),
    r(tokenBVault, false, true),
    r(ata(tokenA, args.creator), false, true),
    r(ata(tokenB, args.creator), false, true),
    r(nftMint, true, true),
    r(positionNftAccount, false, true),
    r(position, false, true),
    r(TOKEN_PROGRAM_ID),
    r(ASSOCIATED_TOKEN_PROGRAM_ID),
    r(SystemProgram.programId),
  ];
  return {
    instructions: [new TransactionInstruction({ programId, keys, data })],
    signers: [nftMintKp],
    derived: { pool, poolAuthority, tokenAVault, tokenBVault, nftMint, position },
  };
}

/// `create_position` — open an empty position in an existing pool. Generates
/// the position-NFT mint keypair (returned as a signer).
export function buildCreatePosition(
  args: { creator: PublicKey; pool: PublicKey },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const poolAuthority = poolAuthorityPda(args.pool, programId).address;
  const nftMintKp = Keypair.generate();
  const nftMint = nftMintKp.publicKey;
  const position = positionPda(nftMint, programId).address;
  const positionNftAccount = ata(nftMint, args.creator);

  const keys = [
    r(args.creator, true, true),
    r(args.pool, false, true),
    r(poolAuthority),
    r(nftMint, true, true),
    r(positionNftAccount, false, true),
    r(position, false, true),
    r(TOKEN_PROGRAM_ID),
    r(ASSOCIATED_TOKEN_PROGRAM_ID),
    r(SystemProgram.programId),
  ];
  return {
    instructions: [
      new TransactionInstruction({ programId, keys, data: ixData("createPosition") }),
    ],
    signers: [nftMintKp],
    derived: { nftMint, position, poolAuthority },
  };
}

/// Shared account layout for the modify-liquidity family.
interface LiquidityAccounts {
  owner: PublicKey;
  pool: PublicKey;
  position: PublicKey;
  nftMint: PublicKey;
  mintA: PublicKey;
  mintB: PublicKey;
  /// Prepend idempotent create-ATA instructions for the owner's token accounts
  /// (default true) so a payout never lands on a missing account.
  createAtas?: boolean;
}

function liquidityKeys(a: LiquidityAccounts, programId: PublicKey) {
  const [tokenA, tokenB] = sortMints(a.mintA, a.mintB);
  const poolAuthority = poolAuthorityPda(a.pool, programId).address;
  const userTokenA = ata(tokenA, a.owner);
  const userTokenB = ata(tokenB, a.owner);
  const keys = [
    r(a.owner, true, false),
    r(a.pool, false, true),
    r(a.position, false, true),
    r(ata(a.nftMint, a.owner)),
    r(poolAuthority),
    r(vaultPda(a.pool, tokenA, programId).address, false, true),
    r(vaultPda(a.pool, tokenB, programId).address, false, true),
    r(userTokenA, false, true),
    r(userTokenB, false, true),
    r(TOKEN_PROGRAM_ID),
  ];
  const ataIxs =
    a.createAtas === false
      ? []
      : [
          createAssociatedTokenAccountIdempotentInstruction(a.owner, userTokenA, a.owner, tokenA),
          createAssociatedTokenAccountIdempotentInstruction(a.owner, userTokenB, a.owner, tokenB),
        ];
  return { keys, ataIxs, poolAuthority };
}

/// `add_liquidity` — deposit tokens for `liquidityDelta` (slippage-capped).
export function buildAddLiquidity(
  args: LiquidityAccounts & { liquidityDelta: bigint; tokenAMax: bigint; tokenBMax: bigint },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const { keys, ataIxs, poolAuthority } = liquidityKeys(args, programId);
  const data = ixData("addLiquidity", (w) => {
    w.u128(args.liquidityDelta).u64(args.tokenAMax).u64(args.tokenBMax);
  });
  return {
    instructions: [...ataIxs, new TransactionInstruction({ programId, keys, data })],
    signers: [],
    derived: { poolAuthority },
  };
}

/// `remove_liquidity` — withdraw `liquidityDelta` (slippage-floored).
export function buildRemoveLiquidity(
  args: LiquidityAccounts & { liquidityDelta: bigint; tokenAMin: bigint; tokenBMin: bigint },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const { keys, ataIxs, poolAuthority } = liquidityKeys(args, programId);
  const data = ixData("removeLiquidity", (w) => {
    w.u128(args.liquidityDelta).u64(args.tokenAMin).u64(args.tokenBMin);
  });
  return {
    instructions: [...ataIxs, new TransactionInstruction({ programId, keys, data })],
    signers: [],
    derived: { poolAuthority },
  };
}

/// `remove_all_liquidity` — withdraw the position's entire free liquidity.
export function buildRemoveAllLiquidity(
  args: LiquidityAccounts & { tokenAMin: bigint; tokenBMin: bigint },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const { keys, ataIxs, poolAuthority } = liquidityKeys(args, programId);
  const data = ixData("removeAllLiquidity", (w) => {
    w.u64(args.tokenAMin).u64(args.tokenBMin);
  });
  return {
    instructions: [...ataIxs, new TransactionInstruction({ programId, keys, data })],
    signers: [],
    derived: { poolAuthority },
  };
}

/// `swap` — trade against the pool. `otherAmountThreshold` is the min-out
/// (ExactIn/PartialFill) or max-in (ExactOut), e.g. from a `quoteSwap`.
export function buildSwap(
  args: {
    owner: PublicKey;
    pool: PublicKey;
    config: PublicKey;
    mintA: PublicKey;
    mintB: PublicKey;
    direction: SwapDirection;
    mode: SwapMode;
    amount: bigint;
    otherAmountThreshold: bigint;
    createAtas?: boolean;
  },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const [tokenA, tokenB] = sortMints(args.mintA, args.mintB);
  const poolAuthority = poolAuthorityPda(args.pool, programId).address;
  const userTokenA = ata(tokenA, args.owner);
  const userTokenB = ata(tokenB, args.owner);
  const data = ixData("swap", (w) => {
    w.u8(DIRECTION_INDEX[args.direction])
      .u8(MODE_INDEX[args.mode])
      .u64(args.amount)
      .u64(args.otherAmountThreshold);
  });
  const keys = [
    r(args.owner, true, false),
    r(args.pool, false, true),
    r(args.config),
    r(poolAuthority),
    r(vaultPda(args.pool, tokenA, programId).address, false, true),
    r(vaultPda(args.pool, tokenB, programId).address, false, true),
    r(userTokenA, false, true),
    r(userTokenB, false, true),
    r(TOKEN_PROGRAM_ID),
  ];
  const ataIxs =
    args.createAtas === false
      ? []
      : [
          createAssociatedTokenAccountIdempotentInstruction(args.owner, userTokenA, args.owner, tokenA),
          createAssociatedTokenAccountIdempotentInstruction(args.owner, userTokenB, args.owner, tokenB),
        ];
  return {
    instructions: [...ataIxs, new TransactionInstruction({ programId, keys, data })],
    signers: [],
    derived: { poolAuthority, userTokenA, userTokenB },
  };
}

/// `claim_position_fee` — collect (or compound) a position's earned fees.
export function buildClaimPositionFee(
  args: LiquidityAccounts,
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const [tokenA, tokenB] = sortMints(args.mintA, args.mintB);
  const poolAuthority = poolAuthorityPda(args.pool, programId).address;
  const ownerTokenA = ata(tokenA, args.owner);
  const ownerTokenB = ata(tokenB, args.owner);
  const keys = [
    r(args.owner, true, false),
    r(args.pool, false, true),
    r(args.position, false, true),
    r(ata(args.nftMint, args.owner)),
    r(poolAuthority),
    r(vaultPda(args.pool, tokenA, programId).address, false, true),
    r(vaultPda(args.pool, tokenB, programId).address, false, true),
    r(ownerTokenA, false, true),
    r(ownerTokenB, false, true),
    r(TOKEN_PROGRAM_ID),
  ];
  const ataIxs =
    args.createAtas === false
      ? []
      : [
          createAssociatedTokenAccountIdempotentInstruction(args.owner, ownerTokenA, args.owner, tokenA),
          createAssociatedTokenAccountIdempotentInstruction(args.owner, ownerTokenB, args.owner, tokenB),
        ];
  return {
    instructions: [...ataIxs, new TransactionInstruction({ programId, keys, data: ixData("claimPositionFee") })],
    signers: [],
    derived: { poolAuthority, ownerTokenA, ownerTokenB },
  };
}

/// Shared builder for the protocol/partner fee claims (identical account
/// layout; only the signer role and discriminator differ).
function buildAuthorityClaim(
  name: "claimProtocolFee" | "claimPartnerFee",
  args: {
    authority: PublicKey;
    config: PublicKey;
    pool: PublicKey;
    mintA: PublicKey;
    mintB: PublicKey;
    recipientA?: PublicKey;
    recipientB?: PublicKey;
    createAtas?: boolean;
  },
  programId: PublicKey,
): Built {
  const [tokenA, tokenB] = sortMints(args.mintA, args.mintB);
  const poolAuthority = poolAuthorityPda(args.pool, programId).address;
  const recipientA = args.recipientA ?? ata(tokenA, args.authority);
  const recipientB = args.recipientB ?? ata(tokenB, args.authority);
  const keys = [
    r(args.authority, true, false),
    r(args.config),
    r(args.pool, false, true),
    r(poolAuthority),
    r(vaultPda(args.pool, tokenA, programId).address, false, true),
    r(vaultPda(args.pool, tokenB, programId).address, false, true),
    r(recipientA, false, true),
    r(recipientB, false, true),
    r(TOKEN_PROGRAM_ID),
  ];
  // Only auto-create when the recipient is the authority's own ATA.
  const ataIxs =
    args.createAtas === false
      ? []
      : [
          ...(args.recipientA ? [] : [createAssociatedTokenAccountIdempotentInstruction(args.authority, recipientA, args.authority, tokenA)]),
          ...(args.recipientB ? [] : [createAssociatedTokenAccountIdempotentInstruction(args.authority, recipientB, args.authority, tokenB)]),
        ];
  return {
    instructions: [...ataIxs, new TransactionInstruction({ programId, keys, data: ixData(name) })],
    signers: [],
    derived: { poolAuthority, recipientA, recipientB },
  };
}

/// `claim_protocol_fee` — the config's `fee_authority` withdraws protocol fees.
export function buildClaimProtocolFee(
  args: {
    feeAuthority: PublicKey;
    config: PublicKey;
    pool: PublicKey;
    mintA: PublicKey;
    mintB: PublicKey;
    recipientA?: PublicKey;
    recipientB?: PublicKey;
    createAtas?: boolean;
  },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  return buildAuthorityClaim("claimProtocolFee", { ...args, authority: args.feeAuthority }, programId);
}

/// `claim_partner_fee` — the config's `partner` withdraws partner fees.
export function buildClaimPartnerFee(
  args: {
    partner: PublicKey;
    config: PublicKey;
    pool: PublicKey;
    mintA: PublicKey;
    mintB: PublicKey;
    recipientA?: PublicKey;
    recipientB?: PublicKey;
    createAtas?: boolean;
  },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  return buildAuthorityClaim("claimPartnerFee", { ...args, authority: args.partner }, programId);
}

/// `set_position_compounding` — toggle a position's fee-compounding flag.
export function buildSetPositionCompounding(
  args: { owner: PublicKey; position: PublicKey; nftMint: PublicKey; enabled: boolean },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const keys = [
    r(args.owner, true, false),
    r(args.position, false, true),
    r(ata(args.nftMint, args.owner)),
  ];
  const data = ixData("setPositionCompounding", (w) => w.bool(args.enabled));
  return {
    instructions: [new TransactionInstruction({ programId, keys, data })],
    signers: [],
    derived: {},
  };
}

/// `close_position` — burn the NFT and reclaim rent (position must be empty).
export function buildClosePosition(
  args: { owner: PublicKey; pool: PublicKey; position: PublicKey; nftMint: PublicKey },
  programId = ZENITH_AMM_PROGRAM_ID,
): Built {
  const positionNftAccount = ata(args.nftMint, args.owner);
  const keys = [
    r(args.owner, true, true),
    r(args.pool, false, true),
    r(args.position, false, true),
    r(args.nftMint, false, true),
    r(positionNftAccount, false, true),
    r(TOKEN_PROGRAM_ID),
  ];
  return {
    instructions: [new TransactionInstruction({ programId, keys, data: ixData("closePosition") })],
    signers: [],
    derived: { positionNftAccount },
  };
}
