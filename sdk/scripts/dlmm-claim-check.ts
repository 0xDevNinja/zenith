// End-to-end devnet proof for DLMM fee claims: claim the LP fees accrued to a
// seeded position (from prior swaps), assert a payout, and assert a second
// claim pays nothing.
//
//   npx tsx scripts/dlmm-claim-check.ts
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
import { dlmm } from "../dist/index.js";

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
const send = (conn: Connection, tx: Transaction, signers: Keypair[]) =>
  sendAndConfirmTransaction(conn, tx, signers, { commitment: "confirmed" });

async function main() {
  const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
  const payer = loadKeypair();
  const programId = new PublicKey(manifest.programId);
  const lbPair = new PublicKey(manifest.lbPair);
  const reserveX = new PublicKey(manifest.reserveX);
  const reserveY = new PublicKey(manifest.reserveY);
  const binArray0 = new PublicKey(manifest.binArrays["0"]);
  const pairAuthority = new PublicKey(manifest.pairAuthority);
  const tokenX = new PublicKey(manifest.tokenX);
  const tokenY = new PublicKey(manifest.tokenY);
  const userX = getAssociatedTokenAddressSync(tokenX, payer.publicKey);
  const userY = getAssociatedTokenAddressSync(tokenY, payer.publicKey);

  // Position A ([0,10], array 0) — owned by the seeder wallet, holds the bin-0
  // shares that earned the swap-check's LP fee.
  const position = new PublicKey(manifest.positions[0]);

  const claimIx = () =>
    dlmm.buildClaimFee({
      owner: payer.publicKey,
      lbPair,
      position,
      binArray: binArray0,
      pairAuthority,
      reserveX,
      reserveY,
      userTokenX: userX,
      userTokenY: userY,
      programId,
    });

  const x0 = await bal(conn, userX);
  const y0 = await bal(conn, userY);
  console.log(`claim 1: ${await send(conn, new Transaction().add(claimIx()), [payer])}`);
  const x1 = await bal(conn, userX);
  const y1 = await bal(conn, userY);
  const gotX = x1 - x0;
  const gotY = y1 - y0;
  console.log(`claimed: X ${gotX}, Y ${gotY}`);

  let ok = true;
  if (gotX <= 0n && gotY <= 0n) {
    console.error("✗ no fees claimed (expected LP fees from prior swaps)");
    ok = false;
  } else console.log("✓ claimed accrued LP fees");

  // Second claim pays nothing (pending zeroed, checkpoints advanced).
  console.log(`claim 2: ${await send(conn, new Transaction().add(claimIx()), [payer])}`);
  const x2 = await bal(conn, userX);
  const y2 = await bal(conn, userY);
  if (x2 - x1 !== 0n || y2 - y1 !== 0n) {
    console.error(`✗ re-claim paid ${x2 - x1} X / ${y2 - y1} Y (expected 0)`);
    ok = false;
  } else console.log("✓ re-claim pays nothing");

  if (!ok) process.exit(1);
  console.log("PASS — DLMM claim_fee pays accrued fees once");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
