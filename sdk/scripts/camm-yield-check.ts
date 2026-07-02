// End-to-end devnet proof for CAMM yield: harvest and assert the yield engine
// pays accrued yield out of the pre-funded source into the reserve, raising the
// tracked reserve (and thus every LP's share value).
//   npx tsx scripts/camm-yield-check.ts
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
  const sourceA = new PublicKey(manifest.yieldSourceA);
  const sourceB = new PublicKey(manifest.yieldSourceB);

  const before = await camm.fetchPool(zc, pool);
  if (!before) throw new Error("pool not found — run camm-seed first");
  const srcABefore = await bal(conn, sourceA);
  const srcBBefore = await bal(conn, sourceB);
  console.log(
    `before: reserveA ${before.reserveA} reserveB ${before.reserveB} | deployedA ${before.deployedA} deployedB ${before.deployedB} | lastAccrual ${before.lastAccrualSlot}`,
  );

  await sendAndConfirmTransaction(
    conn,
    new Transaction().add(
      camm.buildHarvestYield({
        caller: payer.publicKey,
        pool,
        poolAuthority: new PublicKey(manifest.poolAuthority),
        yieldSourceA: sourceA,
        yieldSourceB: sourceB,
        reserveAVault: new PublicKey(manifest.reserveA),
        reserveBVault: new PublicKey(manifest.reserveB),
        tokenAMint: new PublicKey(manifest.tokenA),
        tokenBMint: new PublicKey(manifest.tokenB),
      }),
    ),
    [payer],
    { commitment: "confirmed" },
  );

  const after = await camm.fetchPool(zc, pool);
  if (!after) throw new Error("pool vanished");
  const paidA = after.reserveA - before.reserveA;
  const paidB = after.reserveB - before.reserveB;
  const srcADrain = srcABefore - (await bal(conn, sourceA));
  const srcBDrain = srcBBefore - (await bal(conn, sourceB));
  console.log(`harvested: reserveA +${paidA} reserveB +${paidB} | source drained ${srcADrain}/${srcBDrain}`);

  let ok = true;
  if (paidA <= 0n && paidB <= 0n)
    (ok = false), console.error("✗ no yield harvested (deploy + wait some slots, then retry)");
  else console.log("✓ yield harvested into reserves (LP share price rose)");
  // Credited reserve growth must equal the source drain (solvency).
  if (paidA !== srcADrain || paidB !== srcBDrain)
    (ok = false), console.error(`✗ credit != transfer: ${paidA}/${srcADrain}, ${paidB}/${srcBDrain}`);
  else console.log("✓ reserve credit == source transfer (solvent)");
  if (!ok) process.exit(1);
  console.log("PASS — CAMM yield accrual");
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
