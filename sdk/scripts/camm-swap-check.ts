// End-to-end devnet proof for the CAMM: quote a swap with the SDK, execute the
// SAME swap on-chain, and assert the realized amounts equal the quote exactly.
//   npx tsx scripts/camm-swap-check.ts
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  clusterApiUrl,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { ZenithConnection, camm } from "../dist/index.js";

const manifest = JSON.parse(
  readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "camm-devnet.json"), "utf8"),
);
const loadKeypair = () =>
  Keypair.fromSecretKey(
    Uint8Array.from(
      JSON.parse(readFileSync(join(homedir(), ".config", "solana", "id.json"), "utf8")),
    ),
  );
const bal = async (conn: Connection, ata: PublicKey): Promise<bigint> => {
  try {
    return BigInt((await conn.getTokenAccountBalance(ata)).value.amount);
  } catch {
    return 0n;
  }
};

async function main() {
  const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
  const payer = loadKeypair();
  const zc = new ZenithConnection(conn, { commitment: "confirmed" });

  const pool = new PublicKey(manifest.pool);
  const tokenA = new PublicKey(manifest.tokenA);
  const tokenB = new PublicKey(manifest.tokenB);
  const decoded = await camm.fetchPool(zc, pool);
  if (!decoded) throw new Error("pool not found — run camm-seed first");

  const amount = 1_000_000n; // 1 token A in
  const direction = camm.Direction.AtoB;
  const mode = camm.SwapMode.ExactIn;
  const quote = camm.quoteSwap({ pool: decoded, direction, mode, amount });
  console.log(`quote: in ${quote.amountIn} -> out ${quote.amountOut} | fee ${quote.fee}`);

  const userA = getAssociatedTokenAddressSync(tokenA, payer.publicKey);
  const userB = getAssociatedTokenAddressSync(tokenB, payer.publicKey);
  const ix = camm.buildSwap({
    user: payer.publicKey,
    pool,
    poolAuthority: new PublicKey(manifest.poolAuthority),
    reserveAVault: new PublicKey(manifest.reserveA),
    reserveBVault: new PublicKey(manifest.reserveB),
    userTokenA: userA,
    userTokenB: userB,
    direction,
    mode,
    amount,
    otherAmountThreshold: quote.otherAmountThreshold,
  });

  const aBefore = await bal(conn, userA);
  const bBefore = await bal(conn, userB);
  const sig = await sendAndConfirmTransaction(conn, new Transaction().add(ix), [payer], {
    commitment: "confirmed",
  });
  console.log(`tx ${sig}`);
  const spent = aBefore - (await bal(conn, userA));
  const got = (await bal(conn, userB)) - bBefore;
  console.log(`realized: spent ${spent} A | got ${got} B`);

  let ok = true;
  if (spent !== quote.amountIn) (ok = false), console.error(`✗ input ${spent} != ${quote.amountIn}`);
  else console.log("✓ input matches quote");
  if (got !== quote.amountOut) (ok = false), console.error(`✗ output ${got} != ${quote.amountOut}`);
  else console.log("✓ output matches quote EXACTLY");
  if (!ok) process.exit(1);
  console.log("PASS — on-chain CAMM swap == quote");
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
