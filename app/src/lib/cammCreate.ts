// Permissionless pool creation for the constant-product engine: pick any two
// SPL mints you hold and spin up a fresh camm pool, then seed it with an initial
// deposit. Every account is a PDA derived from the pool + mints, so a created
// pool is fully addressable afterwards.
import {
  Connection,
  type Keypair,
  PublicKey,
  Transaction,
  type VersionedTransaction,
} from "@solana/web3.js";
import {
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { camm } from "@zenith/sdk";
import type { ZenithConnection } from "@zenith/sdk";

import ammManifest from "../devnet.json";
import dlmmManifest from "../dlmm-devnet.json";
import cammManifest from "../camm-devnet.json";

export interface SelectableMint {
  mint: PublicKey;
  symbol: string;
  decimals: number;
}

// The test mints the wallet already holds, gathered from every engine manifest
// (deduped) — the menu of tokens a user can pair into a new pool.
export const SELECTABLE_MINTS: SelectableMint[] = (() => {
  const out = new Map<string, SelectableMint>();
  for (const m of [ammManifest, dlmmManifest, cammManifest]) {
    for (const [mintStr, meta] of Object.entries(
      m.mints as Record<string, { symbol: string; decimals: number }>,
    )) {
      if (!out.has(mintStr)) {
        out.set(mintStr, { mint: new PublicKey(mintStr), symbol: meta.symbol, decimals: meta.decimals });
      }
    }
  }
  return [...out.values()];
})();

export const CAMM_PROGRAM_ID = new PublicKey(cammManifest.programId);

// Derived addresses for a (sorted) mint pair's camm pool.
export function derivePool(mintA: PublicKey, mintB: PublicKey) {
  const [a, b] = camm.sortMints(mintA, mintB);
  const pool = camm.poolPda(a, b, CAMM_PROGRAM_ID).address;
  return {
    tokenA: a,
    tokenB: b,
    pool,
    poolAuthority: camm.poolAuthorityPda(pool, CAMM_PROGRAM_ID).address,
    reserveA: camm.reservePda(pool, a, CAMM_PROGRAM_ID).address,
    reserveB: camm.reservePda(pool, b, CAMM_PROGRAM_ID).address,
    lpMint: camm.lpMintPda(pool, CAMM_PROGRAM_ID).address,
    lockedLp: camm.lockedLpPda(pool, CAMM_PROGRAM_ID).address,
  };
}

// Does a camm pool already exist for this pair?
export async function poolExists(zenith: ZenithConnection, mintA: PublicKey, mintB: PublicKey): Promise<boolean> {
  const { pool } = derivePool(mintA, mintB);
  const p = await camm.fetchPool(zenith, pool);
  return p !== null;
}

type SendTransaction = (
  tx: Transaction | VersionedTransaction,
  connection: Connection,
  options?: { signers?: Keypair[] },
) => Promise<string>;

interface Base {
  connection: Connection;
  sendTransaction: SendTransaction;
  owner: PublicKey;
}

async function send(base: Base, tx: Transaction): Promise<string> {
  const { blockhash, lastValidBlockHeight } = await base.connection.getLatestBlockhash();
  tx.feePayer = base.owner;
  tx.recentBlockhash = blockhash;
  const sig = await base.sendTransaction(tx, base.connection);
  await base.connection.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, "confirmed");
  return sig;
}

export interface CreatePoolArgs {
  mintA: PublicKey;
  mintB: PublicKey;
  baseFeeBps: number;
  protocolFeeRate: number;
  // Raw (base-unit) initial deposit for the sorted tokenA / tokenB.
  amountA: bigint;
  amountB: bigint;
}

// Create the pool, then seed it — two transactions (initialize_pool creates five
// PDAs, so the bootstrap deposit is a separate tx). Returns the pool address.
export async function executeCreatePool(base: Base, args: CreatePoolArgs): Promise<PublicKey> {
  const d = derivePool(args.mintA, args.mintB);

  // tx1 — create the pool (reserves, LP mint, locked-liquidity account).
  await send(
    base,
    new Transaction().add(
      camm.buildInitializePool({
        creator: base.owner,
        tokenAMint: d.tokenA,
        tokenBMint: d.tokenB,
        pool: d.pool,
        poolAuthority: d.poolAuthority,
        reserveAVault: d.reserveA,
        reserveBVault: d.reserveB,
        lpMint: d.lpMint,
        lockedLp: d.lockedLp,
        baseFeeBps: args.baseFeeBps,
        protocolFeeRate: args.protocolFeeRate,
        programId: CAMM_PROGRAM_ID,
      }),
    ),
  );

  // tx2 — bootstrap liquidity (ensure the LP token account exists first).
  const userA = getAssociatedTokenAddressSync(d.tokenA, base.owner);
  const userB = getAssociatedTokenAddressSync(d.tokenB, base.owner);
  const userLp = getAssociatedTokenAddressSync(d.lpMint, base.owner);
  await send(
    base,
    new Transaction()
      .add(createAssociatedTokenAccountIdempotentInstruction(base.owner, userLp, base.owner, d.lpMint))
      .add(
        camm.buildAddLiquidity({
          owner: base.owner,
          pool: d.pool,
          poolAuthority: d.poolAuthority,
          lpMint: d.lpMint,
          lockedLp: d.lockedLp,
          reserveAVault: d.reserveA,
          reserveBVault: d.reserveB,
          userTokenA: userA,
          userTokenB: userB,
          userLp,
          desiredA: args.amountA,
          desiredB: args.amountB,
          programId: CAMM_PROGRAM_ID,
        }),
      ),
  );

  return d.pool;
}
