// CAMM (full-range constant-product + yield) wiring for the app: live pool
// state, the caller's LP position, a swap quote, and wallet-signed swap /
// add / remove / harvest — all against the seeded devnet pool.
import { useCallback, useEffect, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import {
  Connection,
  type Keypair,
  PublicKey,
  Transaction,
  type VersionedTransaction,
} from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { camm } from "@zenith/sdk";
import { useZenith } from "./sdk";
import manifest from "../camm-devnet.json";

export interface CammToken {
  mint: PublicKey;
  symbol: string;
  decimals: number;
}

const tok = (mintStr: string): CammToken => ({
  mint: new PublicKey(mintStr),
  symbol: manifest.mints[mintStr as keyof typeof manifest.mints].symbol,
  decimals: manifest.mints[mintStr as keyof typeof manifest.mints].decimals,
});

// Static market description from the seed manifest.
export const CAMM_MARKET = {
  programId: new PublicKey(manifest.programId),
  pool: new PublicKey(manifest.pool),
  poolAuthority: new PublicKey(manifest.poolAuthority),
  lpMint: new PublicKey(manifest.lpMint),
  lockedLp: new PublicKey(manifest.lockedLp),
  reserveA: new PublicKey(manifest.reserveA),
  reserveB: new PublicKey(manifest.reserveB),
  yieldSourceA: new PublicKey(manifest.yieldSourceA),
  yieldSourceB: new PublicKey(manifest.yieldSourceB),
  baseFeeBps: manifest.baseFeeBps as number,
  protocolFeeRate: manifest.protocolFeeRate as number,
  yieldRate: BigInt(manifest.yieldRate),
  bufferBps: manifest.bufferBps as number,
  tokenA: tok(manifest.tokenA),
  tokenB: tok(manifest.tokenB),
};

const POLL_MS = 15_000;

export interface CammState {
  pool: camm.Pool | null;
  supply: bigint;
  slot: bigint;
  loading: boolean;
  error: string | null;
  refetch: () => void;
}

// Live pool + LP-mint supply + current slot. Polls, keeps last good state on
// transient RPC failures.
export function useCammPool(): CammState {
  const { zenith } = useZenith();
  const { connection } = useConnection();
  const [pool, setPool] = useState<camm.Pool | null>(null);
  const [supply, setSupply] = useState<bigint>(0n);
  const [slot, setSlot] = useState<bigint>(0n);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    let active = true;
    const load = async (initial: boolean) => {
      try {
        const p = await camm.fetchPool(zenith, CAMM_MARKET.pool);
        if (!p) throw new Error("CAMM pool not found on devnet — is it seeded?");
        const [sup, s] = await Promise.all([
          connection.getTokenSupply(CAMM_MARKET.lpMint),
          connection.getSlot(),
        ]);
        if (!active) return;
        setPool(p);
        setSupply(BigInt(sup.value.amount));
        setSlot(BigInt(s));
        setError(null);
      } catch (e) {
        if (!active) return;
        if (initial) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (active && initial) setLoading(false);
      }
    };
    setLoading(true);
    void load(true);
    const t = setInterval(() => void load(false), POLL_MS);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, [zenith, connection, nonce]);

  return { pool, supply, slot, loading, error, refetch };
}

// The connected wallet's LP balance for the pool. Polls; keeps last good state.
export function useCammLpBalance(): { balance: bigint; refetch: () => void } {
  const { connection } = useConnection();
  const { publicKey } = useWallet();
  const [balance, setBalance] = useState<bigint>(0n);
  const [nonce, setNonce] = useState(0);
  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    if (!publicKey) {
      setBalance(0n);
      return;
    }
    let active = true;
    const ata = getAssociatedTokenAddressSync(CAMM_MARKET.lpMint, publicKey);
    const load = async () => {
      try {
        const b = await connection.getTokenAccountBalance(ata);
        if (active) setBalance(BigInt(b.value.amount));
      } catch {
        if (active) setBalance(0n); // no LP account yet
      }
    };
    void load();
    const t = setInterval(load, POLL_MS);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, [connection, publicKey, nonce]);

  return { balance, refetch };
}

// The tokens a given LP-share amount is worth at the pool's current reserves.
export function shareValue(
  pool: camm.Pool,
  supply: bigint,
  shares: bigint,
): { a: bigint; b: bigint } {
  if (supply === 0n) return { a: 0n, b: 0n };
  return {
    a: (pool.reserveA * shares) / supply,
    b: (pool.reserveB * shares) / supply,
  };
}

// Yield accrued but not yet harvested, on the current deployed principal.
export function pendingYield(pool: camm.Pool, slot: bigint): { a: bigint; b: bigint } {
  const elapsed = slot > pool.lastAccrualSlot ? slot - pool.lastAccrualSlot : 0n;
  return {
    a: camm.accruedYield(pool.deployedA, pool.yieldRate, elapsed) ?? 0n,
    b: camm.accruedYield(pool.deployedB, pool.yieldRate, elapsed) ?? 0n,
  };
}

type SendTransaction = (
  tx: Transaction | VersionedTransaction,
  connection: Connection,
  options?: { signers?: Keypair[] },
) => Promise<string>;

export interface CammBase {
  connection: Connection;
  sendTransaction: SendTransaction;
  owner: PublicKey;
}

async function sendCamm({ connection, sendTransaction, owner }: CammBase, tx: Transaction): Promise<string> {
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  tx.feePayer = owner;
  tx.recentBlockhash = blockhash;
  const sig = await sendTransaction(tx, connection);
  await connection.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, "confirmed");
  return sig;
}

function userAtas(owner: PublicKey) {
  return {
    userA: getAssociatedTokenAddressSync(CAMM_MARKET.tokenA.mint, owner),
    userB: getAssociatedTokenAddressSync(CAMM_MARKET.tokenB.mint, owner),
    userLp: getAssociatedTokenAddressSync(CAMM_MARKET.lpMint, owner),
  };
}

export function executeCammSwap(
  base: CammBase,
  args: { direction: camm.Direction; mode: camm.SwapMode; amount: bigint; otherAmountThreshold: bigint },
): Promise<string> {
  const { userA, userB } = userAtas(base.owner);
  const tx = new Transaction().add(
    camm.buildSwap({
      user: base.owner,
      pool: CAMM_MARKET.pool,
      poolAuthority: CAMM_MARKET.poolAuthority,
      reserveAVault: CAMM_MARKET.reserveA,
      reserveBVault: CAMM_MARKET.reserveB,
      userTokenA: userA,
      userTokenB: userB,
      direction: args.direction,
      mode: args.mode,
      amount: args.amount,
      otherAmountThreshold: args.otherAmountThreshold,
      programId: CAMM_MARKET.programId,
    }),
  );
  return sendCamm(base, tx);
}

export function executeCammAdd(
  base: CammBase,
  args: { desiredA: bigint; desiredB: bigint; minA: bigint; minB: bigint },
): Promise<string> {
  const { userA, userB, userLp } = userAtas(base.owner);
  const tx = new Transaction().add(
    camm.buildAddLiquidity({
      owner: base.owner,
      pool: CAMM_MARKET.pool,
      poolAuthority: CAMM_MARKET.poolAuthority,
      lpMint: CAMM_MARKET.lpMint,
      lockedLp: CAMM_MARKET.lockedLp,
      reserveAVault: CAMM_MARKET.reserveA,
      reserveBVault: CAMM_MARKET.reserveB,
      userTokenA: userA,
      userTokenB: userB,
      userLp,
      desiredA: args.desiredA,
      desiredB: args.desiredB,
      minA: args.minA,
      minB: args.minB,
      programId: CAMM_MARKET.programId,
    }),
  );
  return sendCamm(base, tx);
}

export function executeCammRemove(
  base: CammBase,
  args: { shares: bigint; minA: bigint; minB: bigint },
): Promise<string> {
  const { userA, userB, userLp } = userAtas(base.owner);
  const tx = new Transaction().add(
    camm.buildRemoveLiquidity({
      owner: base.owner,
      pool: CAMM_MARKET.pool,
      poolAuthority: CAMM_MARKET.poolAuthority,
      lpMint: CAMM_MARKET.lpMint,
      reserveAVault: CAMM_MARKET.reserveA,
      reserveBVault: CAMM_MARKET.reserveB,
      userTokenA: userA,
      userTokenB: userB,
      userLp,
      shares: args.shares,
      minA: args.minA,
      minB: args.minB,
      programId: CAMM_MARKET.programId,
    }),
  );
  return sendCamm(base, tx);
}

// Harvest the pool's accrued yield into the reserves (permissionless; anyone can
// trigger it — it raises every LP's share value).
export function executeCammHarvest(base: CammBase): Promise<string> {
  const tx = new Transaction().add(
    camm.buildHarvestYield({
      caller: base.owner,
      pool: CAMM_MARKET.pool,
      poolAuthority: CAMM_MARKET.poolAuthority,
      yieldSourceA: CAMM_MARKET.yieldSourceA,
      yieldSourceB: CAMM_MARKET.yieldSourceB,
      reserveAVault: CAMM_MARKET.reserveA,
      reserveBVault: CAMM_MARKET.reserveB,
      tokenAMint: CAMM_MARKET.tokenA.mint,
      tokenBMint: CAMM_MARKET.tokenB.mint,
      programId: CAMM_MARKET.programId,
    }),
  );
  return sendCamm(base, tx);
}
