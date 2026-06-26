// End-to-end devnet proof: run the SAME buildSwap path the app uses, signed by
// the CLI wallet, and assert the realized on-chain output equals the SDK quote.
//
//   npx tsx scripts/swap-check.ts
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  sendAndConfirmTransaction,
  clusterApiUrl,
} from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import {
  buildSwap,
  fetchPool,
  fetchConfig,
  quoteSwap,
  SwapDirection,
  SwapMode,
  ZenithConnection,
} from "../dist/index.js";

const manifest = JSON.parse(
  readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "devnet.json"), "utf8"),
);

function loadKeypair(): Keypair {
  const path = join(homedir(), ".config", "solana", "id.json");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path, "utf8"))));
}

async function bal(conn: Connection, ata: PublicKey): Promise<bigint> {
  try {
    return BigInt((await conn.getTokenAccountBalance(ata)).value.amount);
  } catch {
    return 0n;
  }
}

async function main() {
  const connection = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
  const payer = loadKeypair();
  const zc = new ZenithConnection(connection, { commitment: "confirmed" });

  const pool = await fetchPool(zc, new PublicKey(manifest.pool));
  const config = await fetchConfig(zc, new PublicKey(manifest.config));
  if (!pool || !config) throw new Error("pool/config not found");
  const slot = BigInt(await connection.getSlot());

  const mintA = new PublicKey(manifest.tokenA); // tUSDT (base)
  const mintB = new PublicKey(manifest.tokenB); // tUSDC (quote)
  const ataA = getAssociatedTokenAddressSync(mintA, payer.publicKey, true);
  const ataB = getAssociatedTokenAddressSync(mintB, payer.publicKey, true);

  const amount = 10_000_000n; // 10 tUSDT (6dp), sell A → B
  const quote = quoteSwap({
    pool,
    config,
    slot,
    direction: SwapDirection.AToB,
    mode: SwapMode.ExactIn,
    amount,
    slippageBps: 50,
  });
  console.log("quote: in", amount.toString(), "→ out", quote.amountOut.toString(), "| fee", quote.feeAmount.toString());

  const beforeA = await bal(connection, ataA);
  const beforeB = await bal(connection, ataB);

  const built = buildSwap({
    owner: payer.publicKey,
    pool: new PublicKey(manifest.pool),
    config: new PublicKey(manifest.config),
    mintA,
    mintB,
    direction: SwapDirection.AToB,
    mode: SwapMode.ExactIn,
    amount,
    otherAmountThreshold: quote.otherAmountThreshold,
  });
  const sig = await sendAndConfirmTransaction(
    connection,
    new Transaction().add(...built.instructions),
    [payer],
    { commitment: "confirmed" },
  );
  console.log("tx:", sig);

  const afterA = await bal(connection, ataA);
  const afterB = await bal(connection, ataB);
  const spent = beforeA - afterA;
  const got = afterB - beforeB;
  console.log("realized: spent", spent.toString(), "tUSDT | got", got.toString(), "tUSDC");

  const okIn = spent === amount;
  const okOut = got === quote.amountOut;
  console.log(okIn ? "✓ input matches" : `✗ input ${spent} != ${amount}`);
  console.log(okOut ? "✓ output matches quote EXACTLY" : `✗ output ${got} != quote ${quote.amountOut}`);
  if (!okIn || !okOut) process.exit(1);
  console.log("PASS — on-chain swap == quote");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
