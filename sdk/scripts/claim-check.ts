// Devnet proof for fee accrual + claim: accrue fees on the seed position with a
// round-trip swap, compute the owed fees with the SAME math the UI uses, then
// claim and assert the wallet received exactly that.  npx tsx scripts/claim-check.ts
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
import { getAssociatedTokenAddressSync, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import {
  buildClaimPositionFee,
  buildSwap,
  fetchPool,
  fetchPosition,
  mulShr,
  positionPda,
  Rounding,
  SCALE_OFFSET,
  SwapDirection,
  SwapMode,
  ZenithConnection,
} from "../dist/index.js";

const manifest = JSON.parse(
  readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "devnet.json"), "utf8"),
);
const payer = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(join(homedir(), ".config", "solana", "id.json"), "utf8"))),
);
const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
const zc = new ZenithConnection(conn, { commitment: "confirmed" });
const mintA = new PublicKey(manifest.tokenA);
const mintB = new PublicKey(manifest.tokenB);
const pool = new PublicKey(manifest.pool);
const config = new PublicKey(manifest.config);
const ataA = getAssociatedTokenAddressSync(mintA, payer.publicKey, true);
const ataB = getAssociatedTokenAddressSync(mintB, payer.publicKey, true);

const U128 = (1n << 128n) - 1n;
const bal = async (a: PublicKey) => {
  try {
    return BigInt((await conn.getTokenAccountBalance(a)).value.amount);
  } catch {
    return 0n;
  }
};
const send = (ixs: any[]) =>
  sendAndConfirmTransaction(conn, new Transaction().add(...ixs), [payer], { commitment: "confirmed" });

function owed(p: any, pos: any) {
  const total = pos.liquidity + pos.vestedLiquidity + pos.permanentLockedLiquidity;
  const accrued = (g: bigint, c: bigint) => mulShr(total, (g - c) & U128, SCALE_OFFSET, Rounding.Down) ?? 0n;
  return {
    a: pos.feePendingA + accrued(p.feeGrowthGlobalA, pos.feeGrowthCheckpointA),
    b: pos.feePendingB + accrued(p.feeGrowthGlobalB, pos.feeGrowthCheckpointB),
  };
}

async function swap(dir: SwapDirection, amount: bigint) {
  const built = buildSwap({
    owner: payer.publicKey, pool, config, mintA, mintB,
    direction: dir, mode: SwapMode.ExactIn, amount, otherAmountThreshold: 0n, createAtas: false,
  });
  await send(built.instructions);
}

async function main() {
  // Find the funded seed position owned by this wallet.
  const { value } = await conn.getParsedTokenAccountsByOwner(payer.publicKey, {
    programId: TOKEN_PROGRAM_ID,
  });
  let target: { address: PublicKey; nftMint: PublicKey } | null = null;
  for (const v of value) {
    const info = v.account.data.parsed.info;
    if (info.tokenAmount.decimals !== 0 || info.tokenAmount.amount !== "1") continue;
    const nftMint = new PublicKey(info.mint);
    const address = positionPda(nftMint).address;
    const pos = await fetchPosition(zc, address);
    if (pos && pos.pool.equals(pool) && pos.liquidity > 0n) {
      target = { address, nftMint };
      break;
    }
  }
  if (!target) throw new Error("no funded position owned by this wallet");
  console.log("position:", target.address.toBase58());

  // Accrue fees both ways.
  await swap(SwapDirection.AToB, 5_000_000n);
  await swap(SwapDirection.BToA, 5_000_000n);

  const p = await fetchPool(zc, pool);
  const pos = await fetchPosition(zc, target.address);
  const o = owed(p, pos);
  console.log("computed owed: A", o.a.toString(), "B", o.b.toString());

  const beforeA = await bal(ataA), beforeB = await bal(ataB);
  const claim = buildClaimPositionFee({
    owner: payer.publicKey, pool, position: target.address, nftMint: target.nftMint,
    mintA, mintB, createAtas: false,
  });
  console.log("claim tx:", await send(claim.instructions));
  const gotA = (await bal(ataA)) - beforeA, gotB = (await bal(ataB)) - beforeB;
  console.log("received:     A", gotA.toString(), "B", gotB.toString());

  const ok = gotA === o.a && gotB === o.b && o.a > 0n && o.b > 0n;
  console.log(ok ? "PASS — claimed fees == computed owed (both sides)" : "✗ mismatch");
  if (!ok) process.exit(1);
}
main().catch((e) => { console.error(e); process.exit(1); });
void config;
