// End-to-end devnet proof for the DLMM: quote a swap with the SDK, execute the
// SAME swap on-chain via the SDK's buildSwap, and assert the realized on-chain
// amounts equal the quote exactly.
//
//   npx tsx scripts/dlmm-swap-check.ts
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
import { ZenithConnection, dlmm } from "../dist/index.js";

const manifest = JSON.parse(
  readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "dlmm-devnet.json"), "utf8"),
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

  const programId = new PublicKey(manifest.programId);
  const lbPairPk = new PublicKey(manifest.lbPair);
  const tokenX = new PublicKey(manifest.tokenX);
  const tokenY = new PublicKey(manifest.tokenY);

  const pair = await dlmm.fetchLbPair(zc, lbPairPk);
  if (!pair) throw new Error("lb_pair not found — run the seeder first");

  const binArrayPks = [
    new PublicKey(manifest.binArrays["0"]),
    new PublicKey(manifest.binArrays["-1"]),
  ];
  const binArrays = (await Promise.all(binArrayPks.map((p) => dlmm.fetchBinArray(zc, p)))).filter(
    (a): a is NonNullable<typeof a> => a !== null,
  );

  const slot = BigInt(await connection.getSlot());
  const amount = 1_000_000n; // 1 tBIN in
  const direction = dlmm.Direction.XtoY; // sell X for Y
  const mode = dlmm.SwapMode.ExactIn;

  const quote = dlmm.quoteSwap({ pair, binArrays, slot, direction, mode, amount });
  console.log(
    `quote: in ${quote.amountIn} tBIN -> out ${quote.amountOut} tUSD | fee ${quote.fee} (${quote.feeBps}bps) | bins ${quote.binsCrossed} | ${quote.startBinId}->${quote.endBinId}`,
  );

  const userX = getAssociatedTokenAddressSync(tokenX, payer.publicKey);
  const userY = getAssociatedTokenAddressSync(tokenY, payer.publicKey);

  const swapIx = dlmm.buildSwap({
    trader: payer.publicKey,
    lbPair: lbPairPk,
    pairAuthority: new PublicKey(manifest.pairAuthority),
    reserveX: new PublicKey(manifest.reserveX),
    reserveY: new PublicKey(manifest.reserveY),
    userTokenX: userX,
    userTokenY: userY,
    binArrays: binArrayPks,
    oracle: new PublicKey(manifest.oracle),
    direction,
    mode,
    amount,
    otherAmountThreshold: quote.otherAmountThreshold,
    programId,
  });

  const xBefore = await bal(connection, userX);
  const yBefore = await bal(connection, userY);

  const sig = await sendAndConfirmTransaction(connection, new Transaction().add(swapIx), [payer], {
    commitment: "confirmed",
  });
  console.log(`tx: ${sig}`);

  const xAfter = await bal(connection, userX);
  const yAfter = await bal(connection, userY);
  const spent = xBefore - xAfter;
  const got = yAfter - yBefore;
  console.log(`realized: spent ${spent} tBIN | got ${got} tUSD`);

  let ok = true;
  if (spent !== quote.amountIn) {
    console.error(`✗ input mismatch: on-chain ${spent} != quote ${quote.amountIn}`);
    ok = false;
  } else console.log("✓ input matches quote");
  if (got !== quote.amountOut) {
    console.error(`✗ output mismatch: on-chain ${got} != quote ${quote.amountOut}`);
    ok = false;
  } else console.log("✓ output matches quote EXACTLY");

  if (!ok) process.exit(1);
  console.log("PASS — on-chain DLMM swap == quote");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
