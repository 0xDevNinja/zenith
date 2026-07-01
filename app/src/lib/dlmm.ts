// DLMM (liquidity-book) wiring for the app: live pair + bin-array state, a quote
// helper, and a wallet-signed swap — all against the seeded devnet pair.
import { useCallback, useEffect, useState } from "react";
import {
  Connection,
  PublicKey,
  Transaction,
  type VersionedTransaction,
} from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { dlmm } from "@zenith/sdk";
import { useZenith } from "./sdk";
import manifest from "../dlmm-devnet.json";

export interface DlmmToken {
  mint: PublicKey;
  symbol: string;
  decimals: number;
}

// Static market description from the seed manifest.
export const DLMM_MARKET = {
  programId: new PublicKey(manifest.programId),
  lbPair: new PublicKey(manifest.lbPair),
  pairAuthority: new PublicKey(manifest.pairAuthority),
  oracle: new PublicKey(manifest.oracle),
  reserveX: new PublicKey(manifest.reserveX),
  reserveY: new PublicKey(manifest.reserveY),
  binArrays: [
    new PublicKey(manifest.binArrays["0"]),
    new PublicKey(manifest.binArrays["-1"]),
  ],
  binStep: manifest.binStep as number,
  tokenX: {
    mint: new PublicKey(manifest.tokenX),
    symbol: manifest.mints[manifest.tokenX as keyof typeof manifest.mints].symbol,
    decimals: manifest.mints[manifest.tokenX as keyof typeof manifest.mints].decimals,
  } as DlmmToken,
  tokenY: {
    mint: new PublicKey(manifest.tokenY),
    symbol: manifest.mints[manifest.tokenY as keyof typeof manifest.mints].symbol,
    decimals: manifest.mints[manifest.tokenY as keyof typeof manifest.mints].decimals,
  } as DlmmToken,
};

export interface DlmmState {
  pair: dlmm.LbPair | null;
  binArrays: dlmm.BinArray[];
  loading: boolean;
  error: string | null;
  refetch: () => void;
}

const POLL_MS = 15_000;

// Live LbPair + bin arrays for the seeded pair. Polls, and keeps the last good
// state on transient RPC failures.
export function useDlmmPair(): DlmmState {
  const { zenith } = useZenith();
  const [pair, setPair] = useState<dlmm.LbPair | null>(null);
  const [binArrays, setBinArrays] = useState<dlmm.BinArray[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    let active = true;
    const load = async (initial: boolean) => {
      try {
        const p = await dlmm.fetchLbPair(zenith, DLMM_MARKET.lbPair);
        if (!p) throw new Error("DLMM pair not found on devnet — is it seeded?");
        const arrs = (
          await Promise.all(DLMM_MARKET.binArrays.map((a) => dlmm.fetchBinArray(zenith, a)))
        ).filter((a): a is dlmm.BinArray => a !== null);
        if (!active) return;
        setPair(p);
        setBinArrays(arrs);
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
  }, [zenith, nonce]);

  return { pair, binArrays, loading, error, refetch };
}

// Reserves of a single bin, or zero if the covering array/slot isn't loaded.
export function binReserves(
  binArrays: dlmm.BinArray[],
  binId: number,
): { x: bigint; y: bigint; supply: bigint } {
  const arrayIndex = Math.floor(binId / dlmm.BINS_PER_ARRAY);
  const arr = binArrays.find((a) => Number(a.index) === arrayIndex);
  if (!arr) return { x: 0n, y: 0n, supply: 0n };
  const bin = arr.bins[binId - arrayIndex * dlmm.BINS_PER_ARRAY];
  return { x: bin.amountX, y: bin.amountY, supply: bin.liquiditySupply };
}

// The bin price as a floating number (for display only).
export function binPriceNumber(binStep: number, binId: number): number {
  return Math.pow(1 + binStep / 10_000, binId);
}

type SendTransaction = (
  tx: Transaction | VersionedTransaction,
  connection: Connection,
) => Promise<string>;

// Build a DLMM swap, sign via the wallet, send, and confirm. Returns the sig.
export async function executeDlmmSwap(args: {
  connection: Connection;
  sendTransaction: SendTransaction;
  owner: PublicKey;
  direction: dlmm.Direction;
  mode: dlmm.SwapMode;
  amount: bigint;
  otherAmountThreshold: bigint;
}): Promise<string> {
  const { connection, sendTransaction, owner } = args;
  const userX = getAssociatedTokenAddressSync(DLMM_MARKET.tokenX.mint, owner);
  const userY = getAssociatedTokenAddressSync(DLMM_MARKET.tokenY.mint, owner);

  const ix = dlmm.buildSwap({
    trader: owner,
    lbPair: DLMM_MARKET.lbPair,
    pairAuthority: DLMM_MARKET.pairAuthority,
    reserveX: DLMM_MARKET.reserveX,
    reserveY: DLMM_MARKET.reserveY,
    userTokenX: userX,
    userTokenY: userY,
    binArrays: DLMM_MARKET.binArrays,
    oracle: DLMM_MARKET.oracle,
    direction: args.direction,
    mode: args.mode,
    amount: args.amount,
    otherAmountThreshold: args.otherAmountThreshold,
    programId: DLMM_MARKET.programId,
  });

  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  const tx = new Transaction();
  tx.feePayer = owner;
  tx.recentBlockhash = blockhash;
  tx.add(ix);

  const signature = await sendTransaction(tx, connection);
  await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");
  return signature;
}
