// Prove permissionless pool creation: pick a fresh pair (no pool yet),
// initialize_pool + bootstrap add_liquidity, assert the pool exists with reserves.
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { Connection, Keypair, PublicKey, Transaction, clusterApiUrl, sendAndConfirmTransaction } from "@solana/web3.js";
import { createAssociatedTokenAccountIdempotentInstruction, getAssociatedTokenAddressSync } from "@solana/spl-token";
import { ZenithConnection, camm } from "../dist/index.js";

const amm = JSON.parse(readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "devnet.json"), "utf8"));
const dlmm = JSON.parse(readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "dlmm-devnet.json"), "utf8"));
const payer = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(join(homedir(), ".config", "solana", "id.json"), "utf8"))));
const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
const zc = new ZenithConnection(conn, { commitment: "confirmed" });
const pid = camm.ZENITH_CAMM_PROGRAM_ID;

// fresh pair: tUSDC (amm) + tBIN (dlmm) — no camm pool exists for it.
const mUSDC = new PublicKey(Object.keys(amm.mints).find((k) => amm.mints[k].symbol === "tUSDC"));
const mBIN = new PublicKey(dlmm.tokenX);

async function main() {
  const [a, b] = camm.sortMints(mUSDC, mBIN);
  const pool = camm.poolPda(a, b, pid).address;
  if (await camm.fetchPool(zc, pool)) { console.log("pool already exists:", pool.toBase58(), "— skipping create"); return; }
  const poolAuthority = camm.poolAuthorityPda(pool, pid).address;
  const reserveA = camm.reservePda(pool, a, pid).address;
  const reserveB = camm.reservePda(pool, b, pid).address;
  const lpMint = camm.lpMintPda(pool, pid).address;
  const lockedLp = camm.lockedLpPda(pool, pid).address;

  await sendAndConfirmTransaction(conn, new Transaction().add(camm.buildInitializePool({
    creator: payer.publicKey, tokenAMint: a, tokenBMint: b, pool, poolAuthority,
    reserveAVault: reserveA, reserveBVault: reserveB, lpMint, lockedLp,
    baseFeeBps: 30, protocolFeeRate: 2000, programId: pid,
  })), [payer], { commitment: "confirmed" });
  console.log("initialize_pool ok:", pool.toBase58());

  const userA = getAssociatedTokenAddressSync(a, payer.publicKey);
  const userB = getAssociatedTokenAddressSync(b, payer.publicKey);
  const userLp = getAssociatedTokenAddressSync(lpMint, payer.publicKey);
  await sendAndConfirmTransaction(conn, new Transaction()
    .add(createAssociatedTokenAccountIdempotentInstruction(payer.publicKey, userLp, payer.publicKey, lpMint))
    .add(camm.buildAddLiquidity({
      owner: payer.publicKey, pool, poolAuthority, lpMint, lockedLp,
      reserveAVault: reserveA, reserveBVault: reserveB, userTokenA: userA, userTokenB: userB, userLp,
      desiredA: 100_000_000n, desiredB: 100_000_000n, programId: pid,
    })), [payer], { commitment: "confirmed" });

  const p = await camm.fetchPool(zc, pool);
  if (!p) { console.error("✗ pool not found after create"); process.exit(1); }
  console.log(`✓ created + seeded: reserveA ${p.reserveA} reserveB ${p.reserveB} | lp supply via mint`);
  console.log("PASS — permissionless CAMM pool create");
}
main().catch((e) => { console.error(e); process.exit(1); });
