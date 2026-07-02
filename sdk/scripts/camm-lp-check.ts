// End-to-end devnet proof for CAMM liquidity: add liquidity, then burn exactly
// the minted shares, and assert the round-trip returns no more than deposited
// (dust stays with the pool) and the LP balance returns to its starting point.
//   npx tsx scripts/camm-lp-check.ts
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
import { camm } from "../dist/index.js";

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
  const pool = new PublicKey(manifest.pool);
  const tokenA = new PublicKey(manifest.tokenA);
  const tokenB = new PublicKey(manifest.tokenB);
  const lpMint = new PublicKey(manifest.lpMint);
  const userA = getAssociatedTokenAddressSync(tokenA, payer.publicKey);
  const userB = getAssociatedTokenAddressSync(tokenB, payer.publicKey);
  const userLp = getAssociatedTokenAddressSync(lpMint, payer.publicKey);

  const common = {
    owner: payer.publicKey,
    pool,
    poolAuthority: new PublicKey(manifest.poolAuthority),
    lpMint,
    reserveAVault: new PublicKey(manifest.reserveA),
    reserveBVault: new PublicKey(manifest.reserveB),
    userTokenA: userA,
    userTokenB: userB,
    userLp,
  };

  const desired = 100_000_000n; // 100 of each
  const aStart = await bal(conn, userA);
  const bStart = await bal(conn, userB);
  const lpStart = await bal(conn, userLp);

  await sendAndConfirmTransaction(
    conn,
    new Transaction().add(
      camm.buildAddLiquidity({
        ...common,
        lockedLp: new PublicKey(manifest.lockedLp),
        desiredA: desired,
        desiredB: desired,
      }),
    ),
    [payer],
    { commitment: "confirmed" },
  );
  const lpMid = await bal(conn, userLp);
  const minted = lpMid - lpStart;
  const aDeposited = aStart - (await bal(conn, userA));
  const bDeposited = bStart - (await bal(conn, userB));
  console.log(`added: shares ${minted} for ${aDeposited} A + ${bDeposited} B`);

  await sendAndConfirmTransaction(
    conn,
    new Transaction().add(camm.buildRemoveLiquidity({ ...common, shares: minted })),
    [payer],
    { commitment: "confirmed" },
  );
  const aBack = (await bal(conn, userA)) - (aStart - aDeposited);
  const bBack = (await bal(conn, userB)) - (bStart - bDeposited);
  const lpEnd = await bal(conn, userLp);
  console.log(`removed: got back ${aBack} A + ${bBack} B | lp ${lpStart} -> ${lpEnd}`);

  let ok = true;
  if (lpEnd !== lpStart) (ok = false), console.error(`✗ lp not restored: ${lpEnd} != ${lpStart}`);
  else console.log("✓ LP balance restored");
  // Round-trip returns at most what was deposited (floor rounding keeps dust).
  if (aBack > aDeposited || bBack > bDeposited)
    (ok = false), console.error("✗ round-trip returned MORE than deposited");
  else console.log("✓ round-trip returns <= deposited (no value leak)");
  // The shortfall is bounded: on a yield-enabled pool add_liquidity prices in
  // pending un-harvested yield (the JIT mitigation), so a same-block round-trip
  // legitimately forfeits that pending slice to existing LPs. On a pool with no
  // pending yield this is just floor-rounding dust. Bound it well under 1% to
  // catch a gross accounting error while allowing the JIT charge.
  const cap = desired / 100n; // 1%
  if (aDeposited - aBack > cap || bDeposited - bBack > cap)
    (ok = false),
      console.error(`✗ excessive shortfall: ${aDeposited - aBack} A, ${bDeposited - bBack} B (cap ${cap})`);
  else console.log(`✓ round-trip shortfall within 1% (pending-yield JIT charge + dust)`);
  if (!ok) process.exit(1);
  console.log("PASS — CAMM add/remove round-trip");
}
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
