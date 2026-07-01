// DLMM (liquidity-book) wiring for the app: live pair + bin-array state, a quote
// helper, and a wallet-signed swap — all against the seeded devnet pair.
import { useCallback, useEffect, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  type VersionedTransaction,
} from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { dlmm } from "@zenith/sdk";
import { useZenith } from "./sdk";
import manifest from "../dlmm-devnet.json";

// Byte offsets in a Position account (8-byte disc + shares[70]*16 +
// fee_infos[70]*48 = 4488, then lb_pair, owner). Used to filter by owner.
const POSITION_SIZE = 4600;
const POSITION_LBPAIR_OFFSET = 4488;
const POSITION_OWNER_OFFSET = 4520;

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

// The full Bin at a bin id (with fee growth), or null if not loaded.
export function binAt(binArrays: dlmm.BinArray[], binId: number): dlmm.Bin | null {
  const arrayIndex = Math.floor(binId / dlmm.BINS_PER_ARRAY);
  const arr = binArrays.find((a) => Number(a.index) === arrayIndex);
  if (!arr) return null;
  return arr.bins[binId - arrayIndex * dlmm.BINS_PER_ARRAY];
}

const U128 = 1n << 128n;

// Fees claimable by a position: settled pending plus the live growth since each
// bin's checkpoint (mirrors the program's owed_fee: shares*(growth-cp)>>64).
export function dlmmOwedFees(
  position: dlmm.Position,
  binArrays: dlmm.BinArray[],
): { x: bigint; y: bigint } {
  let x = 0n;
  let y = 0n;
  const n = position.upperBinId - position.lowerBinId + 1;
  for (let i = 0; i < n; i++) {
    const fi = position.feeInfos[i];
    x += fi.feeXPending;
    y += fi.feeYPending;
    const shares = position.liquidityShares[i];
    if (shares > 0n) {
      const bin = binAt(binArrays, position.lowerBinId + i);
      if (bin) {
        const dx = ((bin.feeGrowthX - fi.feeXCheckpoint) % U128 + U128) % U128;
        const dy = ((bin.feeGrowthY - fi.feeYCheckpoint) % U128 + U128) % U128;
        x += (shares * dx) >> 64n;
        y += (shares * dy) >> 64n;
      }
    }
  }
  return { x, y };
}

export interface OwnedDlmmPosition {
  address: PublicKey;
  position: dlmm.Position;
}

// All of `owner`'s positions on the seeded pair (owner + lb_pair memcmp filter).
export async function fetchOwnedDlmmPositions(
  connection: Connection,
  owner: PublicKey,
): Promise<OwnedDlmmPosition[]> {
  const accts = await connection.getProgramAccounts(DLMM_MARKET.programId, {
    filters: [
      { dataSize: POSITION_SIZE },
      { memcmp: { offset: POSITION_LBPAIR_OFFSET, bytes: DLMM_MARKET.lbPair.toBase58() } },
      { memcmp: { offset: POSITION_OWNER_OFFSET, bytes: owner.toBase58() } },
    ],
  });
  return accts.map((a) => ({
    address: a.pubkey,
    position: dlmm.decodePosition(a.account.data),
  }));
}

// Live DLMM positions for the connected wallet. Polls; keeps last good state.
export function useDlmmPositions(): {
  positions: OwnedDlmmPosition[];
  loading: boolean;
  refetch: () => void;
} {
  const { connection } = useConnection();
  const { publicKey } = useWallet();
  const [positions, setPositions] = useState<OwnedDlmmPosition[]>([]);
  const [loading, setLoading] = useState(false);
  const [nonce, setNonce] = useState(0);
  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    if (!publicKey) {
      setPositions([]);
      return;
    }
    let active = true;
    const load = async (initial: boolean) => {
      if (initial) setLoading(true);
      try {
        const p = await fetchOwnedDlmmPositions(connection, publicKey);
        if (active) setPositions(p);
      } catch {
        /* keep last good */
      } finally {
        if (active && initial) setLoading(false);
      }
    };
    void load(true);
    const t = setInterval(() => void load(false), POLL_MS);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, [connection, publicKey, nonce]);

  return { positions, loading, refetch };
}

type SendTransaction = (
  tx: Transaction | VersionedTransaction,
  connection: Connection,
  options?: { signers?: Keypair[] },
) => Promise<string>;

interface DlmmBase {
  connection: Connection;
  sendTransaction: SendTransaction;
  owner: PublicKey;
}

async function sendDlmm(
  { connection, sendTransaction, owner }: DlmmBase,
  ixs: Transaction,
  signers: Keypair[] = [],
): Promise<string> {
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  ixs.feePayer = owner;
  ixs.recentBlockhash = blockhash;
  const sig = await sendTransaction(ixs, connection, { signers });
  await connection.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, "confirmed");
  return sig;
}

function userAtas(owner: PublicKey) {
  return {
    userX: getAssociatedTokenAddressSync(DLMM_MARKET.tokenX.mint, owner),
    userY: getAssociatedTokenAddressSync(DLMM_MARKET.tokenY.mint, owner),
  };
}

function binArrayFor(binId: number): PublicKey {
  const idx = Math.floor(binId / dlmm.BINS_PER_ARRAY);
  return dlmm.binArrayPda(DLMM_MARKET.lbPair, idx, DLMM_MARKET.programId).address;
}

// Open a new position + seed it with a strategy deposit, in one transaction.
export function executeDlmmProvide(
  base: DlmmBase,
  args: {
    positionBase: Keypair;
    lowerBin: number;
    width: number;
    amountX: bigint;
    amountY: bigint;
    strategy: number;
    activeBin: number;
  },
): Promise<string> {
  const { userX, userY } = userAtas(base.owner);
  const position = dlmm.positionPda(args.positionBase.publicKey, DLMM_MARKET.programId).address;
  const binArray = binArrayFor(args.lowerBin);
  const tx = new Transaction()
    .add(
      dlmm.buildInitializePosition({
        owner: base.owner,
        base: args.positionBase.publicKey,
        lbPair: DLMM_MARKET.lbPair,
        position,
        lowerBinId: args.lowerBin,
        width: args.width,
        programId: DLMM_MARKET.programId,
      }),
    )
    .add(
      dlmm.buildAddLiquidityByStrategy({
        owner: base.owner,
        lbPair: DLMM_MARKET.lbPair,
        position,
        binArray,
        reserveX: DLMM_MARKET.reserveX,
        reserveY: DLMM_MARKET.reserveY,
        userTokenX: userX,
        userTokenY: userY,
        amountX: args.amountX,
        amountY: args.amountY,
        strategy: args.strategy,
        expectedActiveBinId: args.activeBin,
        activeIdSlippage: 5,
        programId: DLMM_MARKET.programId,
      }),
    );
  return sendDlmm(base, tx, [args.positionBase]);
}

// Claim a position's accrued LP fees.
export function executeDlmmClaim(base: DlmmBase, p: OwnedDlmmPosition): Promise<string> {
  const { userX, userY } = userAtas(base.owner);
  const tx = new Transaction().add(
    dlmm.buildClaimFee({
      owner: base.owner,
      lbPair: DLMM_MARKET.lbPair,
      position: p.address,
      binArray: binArrayFor(p.position.lowerBinId),
      pairAuthority: DLMM_MARKET.pairAuthority,
      reserveX: DLMM_MARKET.reserveX,
      reserveY: DLMM_MARKET.reserveY,
      userTokenX: userX,
      userTokenY: userY,
      programId: DLMM_MARKET.programId,
    }),
  );
  return sendDlmm(base, tx);
}

// Full exit: claim fees, remove all liquidity, close the position (one tx).
export function executeDlmmClose(base: DlmmBase, p: OwnedDlmmPosition): Promise<string> {
  const { userX, userY } = userAtas(base.owner);
  const binArray = binArrayFor(p.position.lowerBinId);
  const tx = new Transaction()
    .add(
      dlmm.buildClaimFee({
        owner: base.owner,
        lbPair: DLMM_MARKET.lbPair,
        position: p.address,
        binArray,
        pairAuthority: DLMM_MARKET.pairAuthority,
        reserveX: DLMM_MARKET.reserveX,
        reserveY: DLMM_MARKET.reserveY,
        userTokenX: userX,
        userTokenY: userY,
        programId: DLMM_MARKET.programId,
      }),
    )
    .add(
      dlmm.buildRemoveLiquidity({
        owner: base.owner,
        lbPair: DLMM_MARKET.lbPair,
        position: p.address,
        binArray,
        pairAuthority: DLMM_MARKET.pairAuthority,
        reserveX: DLMM_MARKET.reserveX,
        reserveY: DLMM_MARKET.reserveY,
        userTokenX: userX,
        userTokenY: userY,
        bps: 10_000,
        programId: DLMM_MARKET.programId,
      }),
    )
    .add(
      dlmm.buildClosePosition({
        owner: base.owner,
        position: p.address,
        programId: DLMM_MARKET.programId,
      }),
    );
  return sendDlmm(base, tx);
}

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
