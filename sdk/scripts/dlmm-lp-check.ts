// End-to-end devnet proof for DLMM liquidity: open a fresh position, add a Spot
// deposit, remove all of it, and close — asserting the reserves move by exactly
// the deposit and the tokens come back (minus floor dust).
//
//   npx tsx scripts/dlmm-lp-check.ts
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
const send = (conn: Connection, ixs: Transaction, signers: Keypair[]) =>
  sendAndConfirmTransaction(conn, ixs, signers, { commitment: "confirmed" });

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

  const base = Keypair.generate();
  const position = dlmm.positionPda(base.publicKey, programId).address;

  // Deposit X-only into bins strictly ABOVE the active bin (range [1,5]). Those
  // bins hold only token X, so removing returns X (composition preserved) — a
  // clean token-conservation check. (A deposit into the shared active bin would
  // come back in the bin's blended X:Y ratio: value-, not composition-, stable.)
  const amountX = 5_000_000n;
  const amountY = 0n;
  const lowerBin = 1;
  const width = 5;

  const rxBefore = await bal(conn, reserveX);
  const ryBefore = await bal(conn, reserveY);
  const uxBefore = await bal(conn, userX);
  const uyBefore = await bal(conn, userY);

  // open position [0,10] (array 0) + add a Spot deposit.
  const openTx = new Transaction()
    .add(
      dlmm.buildInitializePosition({
        owner: payer.publicKey,
        base: base.publicKey,
        lbPair,
        position,
        lowerBinId: lowerBin,
        width,
        programId,
      }),
    )
    .add(
      dlmm.buildAddLiquidityByStrategy({
        owner: payer.publicKey,
        lbPair,
        position,
        binArray: binArray0,
        reserveX,
        reserveY,
        userTokenX: userX,
        userTokenY: userY,
        amountX,
        amountY,
        strategy: 0, // Spot
        expectedActiveBinId: 0,
        activeIdSlippage: 5,
        programId,
      }),
    );
  console.log(`open+add: ${await send(conn, openTx, [payer, base])}`);

  const rxAdd = await bal(conn, reserveX);
  const ryAdd = await bal(conn, reserveY);
  let ok = true;
  if (rxAdd - rxBefore !== amountX || ryAdd - ryBefore !== amountY) {
    console.error(
      `✗ reserves moved by (${rxAdd - rxBefore}, ${ryAdd - ryBefore}), expected (${amountX}, ${amountY})`,
    );
    ok = false;
  } else console.log("✓ add: reserves increased by exactly the deposit");

  // remove all + close.
  const closeTx = new Transaction()
    .add(
      dlmm.buildRemoveLiquidity({
        owner: payer.publicKey,
        lbPair,
        position,
        binArray: binArray0,
        pairAuthority,
        reserveX,
        reserveY,
        userTokenX: userX,
        userTokenY: userY,
        bps: 10_000,
        programId,
      }),
    )
    .add(dlmm.buildClosePosition({ owner: payer.publicKey, position, programId }));
  console.log(`remove+close: ${await send(conn, closeTx, [payer])}`);

  const uxAfter = await bal(conn, userX);
  const uyAfter = await bal(conn, userY);
  const netX = uxBefore - uxAfter; // tokens not returned (floor dust)
  const netY = uyBefore - uyAfter;
  console.log(`round-trip token loss to dust: X ${netX}, Y ${netY}`);
  // X-only deposit into non-active bins: same token returns; floor dust is at
  // most a couple of units per bin (share rounding on add + token rounding on
  // remove), and Y is untouched.
  if (netX < 0n || netX > 2n * BigInt(width) || netY !== 0n) {
    console.error("✗ round-trip did not conserve tokens within floor dust");
    ok = false;
  } else console.log("✓ remove returned the deposit within floor dust");

  if (!ok) process.exit(1);
  console.log("PASS — DLMM open/add/remove/close conserves tokens");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
