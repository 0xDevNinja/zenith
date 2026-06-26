// Devnet proof for the liquidity flows: open a position, add to it, then remove
// all — asserting the on-chain token movements equal the SDK composition math
// the UI displays.  npx tsx scripts/lp-check.ts
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
  buildAddLiquidity,
  buildCreatePosition,
  buildRemoveAllLiquidity,
  deltaA,
  deltaB,
  fetchPool,
  fetchPosition,
  liquidityFromAmountA,
  Q64,
  Rounding,
  ZenithConnection,
} from "../dist/index.js";

const manifest = JSON.parse(
  readFileSync(join(import.meta.dirname, "..", "..", "app", "src", "devnet.json"), "utf8"),
);
const kp = () =>
  Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(readFileSync(join(homedir(), ".config", "solana", "id.json"), "utf8"))),
  );

const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
const payer = kp();
const zc = new ZenithConnection(conn, { commitment: "confirmed" });
const mintA = new PublicKey(manifest.tokenA);
const mintB = new PublicKey(manifest.tokenB);
const pool = new PublicKey(manifest.pool);
const config = new PublicKey(manifest.config);
const ataA = getAssociatedTokenAddressSync(mintA, payer.publicKey, true);
const ataB = getAssociatedTokenAddressSync(mintB, payer.publicKey, true);

const bal = async (a: PublicKey) => {
  try {
    return BigInt((await conn.getTokenAccountBalance(a)).value.amount);
  } catch {
    return 0n;
  }
};
const send = (ixs: any[], extra: Keypair[] = []) =>
  sendAndConfirmTransaction(conn, new Transaction().add(...ixs), [payer, ...extra], { commitment: "confirmed" });

function compUp(L: bigint, p: { sqrtPrice: bigint; sqrtMinPrice: bigint; sqrtMaxPrice: bigint }, r = Rounding.Up) {
  const cur = Q64.fromBits(p.sqrtPrice), min = Q64.fromBits(p.sqrtMinPrice), max = Q64.fromBits(p.sqrtMaxPrice);
  return { a: deltaA(L, cur, max, r) ?? 0n, b: deltaB(L, min, cur, r) ?? 0n };
}

async function main() {
  const p = await fetchPool(zc, pool);
  if (!p) throw new Error("pool not found");

  const amountA = 100_000_000n; // 100 tokenA
  const L = liquidityFromAmountA(amountA, Q64.fromBits(p.sqrtPrice), Q64.fromBits(p.sqrtMaxPrice), Rounding.Down);
  if (!L) throw new Error("L null");
  const need = compUp(L, p);
  console.log("L", L.toString(), "needs A", need.a.toString(), "B", need.b.toString());

  // OPEN
  let beforeA = await bal(ataA), beforeB = await bal(ataB);
  const create = buildCreatePosition({ creator: payer.publicKey, pool });
  const add = buildAddLiquidity({
    owner: payer.publicKey, pool, position: create.derived.position, nftMint: create.derived.nftMint,
    mintA, mintB, liquidityDelta: L, tokenAMax: need.a + 10n, tokenBMax: need.b + 10n,
  });
  await send([...create.instructions, ...add.instructions], [create.signers[0] as Keypair]);
  let spentA = beforeA - (await bal(ataA)), spentB = beforeB - (await bal(ataB));
  const pos1 = await fetchPosition(zc, create.derived.position);
  const okOpen = spentA === need.a && spentB === need.b && pos1?.liquidity === L;
  console.log(okOpen ? "✓ open: spent matches composition, L set" : `✗ open: A ${spentA}/${need.a} B ${spentB}/${need.b} L ${pos1?.liquidity}/${L}`);

  // ADD same L again
  beforeA = await bal(ataA); beforeB = await bal(ataB);
  const add2 = buildAddLiquidity({
    owner: payer.publicKey, pool, position: create.derived.position, nftMint: create.derived.nftMint,
    mintA, mintB, liquidityDelta: L, tokenAMax: need.a + 10n, tokenBMax: need.b + 10n, createAtas: false,
  });
  await send(add2.instructions);
  spentA = beforeA - (await bal(ataA)); spentB = beforeB - (await bal(ataB));
  const pos2 = await fetchPosition(zc, create.derived.position);
  const okAdd = spentA === need.a && spentB === need.b && pos2?.liquidity === 2n * L;
  console.log(okAdd ? "✓ add: spent matches, L doubled" : `✗ add: A ${spentA} B ${spentB} L ${pos2?.liquidity}/${2n * L}`);

  // REMOVE ALL
  const back = compUp(2n * L, p, Rounding.Down);
  beforeA = await bal(ataA); beforeB = await bal(ataB);
  const rm = buildRemoveAllLiquidity({
    owner: payer.publicKey, pool, position: create.derived.position, nftMint: create.derived.nftMint,
    mintA, mintB, tokenAMin: 0n, tokenBMin: 0n, createAtas: false,
  });
  await send(rm.instructions);
  const gotA = (await bal(ataA)) - beforeA, gotB = (await bal(ataB)) - beforeB;
  const pos3 = await fetchPosition(zc, create.derived.position);
  const okRemove = gotA === back.a && gotB === back.b && pos3?.liquidity === 0n;
  console.log(okRemove ? "✓ remove-all: received matches composition, L=0" : `✗ remove: A ${gotA}/${back.a} B ${gotB}/${back.b} L ${pos3?.liquidity}`);

  if (okOpen && okAdd && okRemove) console.log("PASS — open/add/remove == SDK composition");
  else process.exit(1);
}
main().catch((e) => { console.error(e); process.exit(1); });
void config;
