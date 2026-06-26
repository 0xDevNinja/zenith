import type { Connection, PublicKey, Signer, VersionedTransaction } from "@solana/web3.js";
import {
  buildAddLiquidity,
  buildCreatePosition,
  buildRemoveAllLiquidity,
  buildRemoveLiquidity,
  buildTransactionFrom,
  deltaA,
  deltaB,
  liquidityFromAmountA,
  Q64,
  Rounding,
  type Pool,
} from "@zenith/sdk";
import { MARKET } from "./market";

// Positions in this AMM are full-band over the pool's configured [min,max]; the
// composition of a liquidity amount L depends only on where the current price
// sits in that band.
export interface Composition {
  amountA: bigint;
  amountB: bigint;
}

export function composition(pool: Pool, liquidity: bigint, rounding = Rounding.Down): Composition {
  const cur = Q64.fromBits(pool.sqrtPrice);
  const min = Q64.fromBits(pool.sqrtMinPrice);
  const max = Q64.fromBits(pool.sqrtMaxPrice);
  // token A (base) backs the range above the current price; token B (quote) below.
  const amountA = deltaA(liquidity, cur, max, rounding) ?? 0n;
  const amountB = deltaB(liquidity, min, cur, rounding) ?? 0n;
  return { amountA, amountB };
}

// Liquidity L backed by a given amount of token A at the current price.
export function liquidityForTokenA(pool: Pool, amountA: bigint): bigint | null {
  const cur = Q64.fromBits(pool.sqrtPrice);
  const max = Q64.fromBits(pool.sqrtMaxPrice);
  return liquidityFromAmountA(amountA, cur, max, Rounding.Down);
}

export function slipUp(x: bigint, bps: number): bigint {
  return x + (x * BigInt(bps)) / 10_000n;
}
export function slipDown(x: bigint, bps: number): bigint {
  return x - (x * BigInt(bps)) / 10_000n;
}

type SendTransaction = (
  tx: VersionedTransaction,
  connection: Connection,
  options?: { signers?: Signer[] },
) => Promise<string>;

interface Base {
  connection: Connection;
  sendTransaction: SendTransaction;
  owner: PublicKey;
}

async function sendBuilt(
  { connection, sendTransaction, owner }: Base,
  built: Parameters<typeof buildTransactionFrom>[0]["built"],
): Promise<string> {
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  const { transaction, signers } = buildTransactionFrom({
    payerKey: owner,
    recentBlockhash: blockhash,
    built,
  });
  const signature = await sendTransaction(transaction, connection, { signers });
  await connection.confirmTransaction({ signature, blockhash, lastValidBlockHeight }, "confirmed");
  return signature;
}

// Open a fresh position and seed it in one transaction (create_position +
// add_liquidity sharing the generated NFT mint).
export function executeOpenPosition(
  base: Base,
  args: { liquidityDelta: bigint; tokenAMax: bigint; tokenBMax: bigint },
): Promise<string> {
  const create = buildCreatePosition({ creator: base.owner, pool: MARKET.pool });
  const add = buildAddLiquidity({
    owner: base.owner,
    pool: MARKET.pool,
    position: create.derived.position,
    nftMint: create.derived.nftMint,
    mintA: MARKET.tokenA.mint,
    mintB: MARKET.tokenB.mint,
    liquidityDelta: args.liquidityDelta,
    tokenAMax: args.tokenAMax,
    tokenBMax: args.tokenBMax,
  });
  return sendBuilt(base, [create, add]);
}

interface PositionRef {
  position: PublicKey;
  nftMint: PublicKey;
}

export function executeAddLiquidity(
  base: Base,
  ref: PositionRef,
  args: { liquidityDelta: bigint; tokenAMax: bigint; tokenBMax: bigint },
): Promise<string> {
  const add = buildAddLiquidity({
    owner: base.owner,
    pool: MARKET.pool,
    ...ref,
    mintA: MARKET.tokenA.mint,
    mintB: MARKET.tokenB.mint,
    ...args,
  });
  return sendBuilt(base, [add]);
}

export function executeRemoveLiquidity(
  base: Base,
  ref: PositionRef,
  args: { liquidityDelta: bigint; tokenAMin: bigint; tokenBMin: bigint },
): Promise<string> {
  const rm = buildRemoveLiquidity({
    owner: base.owner,
    pool: MARKET.pool,
    ...ref,
    mintA: MARKET.tokenA.mint,
    mintB: MARKET.tokenB.mint,
    ...args,
  });
  return sendBuilt(base, [rm]);
}

export function executeRemoveAll(
  base: Base,
  ref: PositionRef,
  args: { tokenAMin: bigint; tokenBMin: bigint },
): Promise<string> {
  const rm = buildRemoveAllLiquidity({
    owner: base.owner,
    pool: MARKET.pool,
    ...ref,
    mintA: MARKET.tokenA.mint,
    mintB: MARKET.tokenB.mint,
    ...args,
  });
  return sendBuilt(base, [rm]);
}
