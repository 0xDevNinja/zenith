import {
  type AddressLookupTableAccount,
  type PublicKey,
  type Signer,
  type TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import type { Built } from "./instructions/builders.js";

/// Flatten one or more builder outputs into a single instruction + signer list,
/// preserving order. Lets callers compose actions (e.g. create-position then
/// add-liquidity) into one transaction.
export function mergeBuilt(...built: Built[]): {
  instructions: TransactionInstruction[];
  signers: Signer[];
} {
  const instructions: TransactionInstruction[] = [];
  const signers: Signer[] = [];
  for (const b of built) {
    instructions.push(...b.instructions);
    signers.push(...b.signers);
  }
  return { instructions, signers };
}

/// Compile instructions into an unsigned v0 `VersionedTransaction` ready for
/// signing. `payerKey` funds the transaction; `recentBlockhash` comes from the
/// cluster. Pass `lookupTables` to compress account lists for large txns.
export function buildTransaction(args: {
  payerKey: PublicKey;
  recentBlockhash: string;
  instructions: TransactionInstruction[];
  lookupTables?: AddressLookupTableAccount[];
}): VersionedTransaction {
  const message = new TransactionMessage({
    payerKey: args.payerKey,
    recentBlockhash: args.recentBlockhash,
    instructions: args.instructions,
  }).compileToV0Message(args.lookupTables);
  return new VersionedTransaction(message);
}

/// Convenience: compile builder outputs into an unsigned transaction and return
/// it together with the extra signers (e.g. a position-NFT mint keypair) the
/// caller must add alongside the fee payer before sending.
export function buildTransactionFrom(args: {
  payerKey: PublicKey;
  recentBlockhash: string;
  built: Built[];
  lookupTables?: AddressLookupTableAccount[];
}): { transaction: VersionedTransaction; signers: Signer[] } {
  const { instructions, signers } = mergeBuilt(...args.built);
  const transaction = buildTransaction({
    payerKey: args.payerKey,
    recentBlockhash: args.recentBlockhash,
    instructions,
    lookupTables: args.lookupTables,
  });
  return { transaction, signers };
}
