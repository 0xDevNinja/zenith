import type { Connection, PublicKey, VersionedTransaction } from "@solana/web3.js";
import {
  buildSwap,
  buildTransactionFrom,
  SwapDirection,
  SwapMode,
  type Pool,
} from "@zenith/sdk";
import { MARKET } from "./market";

// AToB sells tokenA (base) for B; BToA is the reverse. Derived from which side
// the user is paying in.
export function directionFor(inputMint: PublicKey, pool: Pool): SwapDirection {
  return inputMint.equals(pool.tokenAMint) ? SwapDirection.AToB : SwapDirection.BToA;
}

// Matches wallet-adapter's sendTransaction (sign + send in one call). The swap
// needs no extra signers, so options are unused here.
type SendTransaction = (tx: VersionedTransaction, connection: Connection) => Promise<string>;

interface ExecuteArgs {
  connection: Connection;
  sendTransaction: SendTransaction;
  owner: PublicKey;
  direction: SwapDirection;
  mode: SwapMode;
  amount: bigint;
  otherAmountThreshold: bigint;
}

// Build → sign (via wallet) → send → confirm. Returns the signature.
export async function executeSwap({
  connection,
  sendTransaction,
  owner,
  direction,
  mode,
  amount,
  otherAmountThreshold,
}: ExecuteArgs): Promise<string> {
  const built = buildSwap({
    owner,
    pool: MARKET.pool,
    config: MARKET.config,
    mintA: MARKET.tokenA.mint,
    mintB: MARKET.tokenB.mint,
    direction,
    mode,
    amount,
    otherAmountThreshold,
  });

  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  const { transaction } = buildTransactionFrom({
    payerKey: owner,
    recentBlockhash: blockhash,
    built: [built],
  });

  const signature = await sendTransaction(transaction, connection);
  await connection.confirmTransaction(
    { signature, blockhash, lastValidBlockHeight },
    "confirmed",
  );
  return signature;
}
