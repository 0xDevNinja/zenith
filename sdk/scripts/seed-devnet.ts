// Seed a live Zenith pool on devnet so the app has something real to trade.
//
// Run from the sdk/ dir (shares one @solana/web3.js with the built SDK):
//   npx tsx scripts/seed-devnet.ts
//
// Prereqs: program deployed to devnet, and ~/.config/solana/id.json funded with
// a few devnet SOL. Emits ../app/src/devnet.json with the addresses the app
// reads for its default market.
import { readFileSync, writeFileSync } from "node:fs";
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
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";
import {
  buildCreateConfig,
  buildInitializePool,
  configPda,
  poolPda,
  sortMints,
  fetchPool,
  ZenithConnection,
  ZENITH_AMM_PROGRAM_ID,
  ONE_Q64,
} from "../dist/index.js";

const RPC = process.env.RPC ?? clusterApiUrl("devnet");
const CONFIG_INDEX = Number(process.env.CONFIG_INDEX ?? 1);

// Two same-decimal test tokens → a clean ~1.0 stable-style market inside the
// 0.5–2.0 price band.
const TOKENS = {
  a: { symbol: "tUSDC", decimals: 6 },
  b: { symbol: "tUSDT", decimals: 6 },
};
const MINT_AMOUNT = 1_000_000_000_000n; // 1,000,000 of each (6 dp)

function loadKeypair(): Keypair {
  const path = join(homedir(), ".config", "solana", "id.json");
  const bytes = Uint8Array.from(JSON.parse(readFileSync(path, "utf8")));
  return Keypair.fromSecretKey(bytes);
}

async function send(conn: Connection, payer: Keypair, ixs: any[], extra: Keypair[] = []) {
  const tx = new Transaction().add(...ixs);
  return sendAndConfirmTransaction(conn, tx, [payer, ...extra], {
    commitment: "confirmed",
  });
}

async function main() {
  const connection = new Connection(RPC, "confirmed");
  const payer = loadKeypair();
  console.log("deployer:", payer.publicKey.toBase58());

  const bal = await connection.getBalance(payer.publicKey);
  console.log("balance :", bal / 1e9, "SOL");
  if (bal < 0.5e9) throw new Error("deployer underfunded — airdrop/faucet devnet SOL first");

  // 1. Mints + funded ATAs.
  console.log("creating mints…");
  const mintA = await createMint(connection, payer, payer.publicKey, null, TOKENS.a.decimals);
  const mintB = await createMint(connection, payer, payer.publicKey, null, TOKENS.b.decimals);
  for (const m of [mintA, mintB]) {
    const acc = await getOrCreateAssociatedTokenAccount(connection, payer, m, payer.publicKey);
    await mintTo(connection, payer, m, acc.address, payer, MINT_AMOUNT);
  }
  console.log("  tUSDC:", mintA.toBase58());
  console.log("  tUSDT:", mintB.toBase58());

  // 2. Config template.
  console.log("create_config…");
  const cfg = buildCreateConfig({
    admin: payer.publicKey,
    params: {
      index: CONFIG_INDEX,
      feeAuthority: payer.publicKey,
      sqrtMinPrice: 1n << 63n, // sqrt 0.5
      sqrtMaxPrice: 1n << 65n, // sqrt 2.0
      baseFeeBps: 30,
      protocolFeeBps: 1000,
      partner: PublicKey.default,
      partnerFeeBps: 0,
      feeSchedulerMode: 0, // constant
      cliffFeeBps: 30,
      reductionFactor: 0,
      feePeriod: 0n,
      maxFeeSteps: 0,
      variableFeeControl: 0, // dynamic fee off → deterministic quotes
      maxVolatilityAccumulator: 0,
      filterPeriod: 10,
      decayPeriod: 100,
      volatilityReductionFactor: 0,
      maxDynamicFeeBps: 0,
    },
  });
  await send(connection, payer, cfg.instructions);
  const config = configPda(CONFIG_INDEX).address;
  console.log("  config:", config.toBase58());

  // 3. Pool + seeded first position (price 1.0, inside the band).
  console.log("initialize_pool (+ seed liquidity)…");
  const init = buildInitializePool({
    creator: payer.publicKey,
    config,
    mintA,
    mintB,
    sqrtPrice: ONE_Q64, // 1.0
    liquidity: 50_000_000_000n, // deep enough that typical swaps show low impact
    tokenAMax: 1n << 60n,
    tokenBMax: 1n << 60n,
  });
  await send(connection, payer, init.instructions, init.signers);

  const [tokenA, tokenB] = sortMints(mintA, mintB);
  const pool = poolPda(config, tokenA, tokenB).address;

  // 4. Read it back to confirm.
  const zc = new ZenithConnection(connection, { commitment: "confirmed" });
  const poolState = await fetchPool(zc, pool);
  console.log("  pool  :", pool.toBase58());
  console.log("  liquidity:", poolState.liquidity.toString(), "sqrtPrice:", poolState.sqrtPrice.toString());

  // 5. Emit the app's default-market manifest.
  const manifest = {
    cluster: "devnet",
    programId: ZENITH_AMM_PROGRAM_ID.toBase58(),
    configIndex: CONFIG_INDEX,
    config: config.toBase58(),
    pool: pool.toBase58(),
    tokenA: tokenA.toBase58(),
    tokenB: tokenB.toBase58(),
    mints: {
      [mintA.toBase58()]: TOKENS.a,
      [mintB.toBase58()]: TOKENS.b,
    },
  };
  const out = join(import.meta.dirname, "..", "..", "app", "src", "devnet.json");
  writeFileSync(out, JSON.stringify(manifest, null, 2) + "\n");
  console.log("wrote", out);
  console.log("done.");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
