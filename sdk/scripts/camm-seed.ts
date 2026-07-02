// Seed a zenith-camm pool on devnet: create two test mints, initialize the
// pool, bootstrap liquidity, configure + fund the yield engine, and rebalance.
// Writes the market manifest to app/src/camm-devnet.json.
//
//   npx tsx scripts/camm-seed.ts
import { readFileSync, writeFileSync } from "node:fs";
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
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  getAssociatedTokenAddressSync,
  mintTo,
} from "@solana/spl-token";
import { camm } from "../dist/index.js";

const DECIMALS = 6;
const UNIT = 10n ** BigInt(DECIMALS);
const BASE_FEE_BPS = 30;
const PROTOCOL_FEE_RATE = 2000; // 20% of the fee
const BUFFER_BPS = 1000; // keep 10% of reserves as a swap buffer
const YIELD_RATE = 1000n; // per deployed unit per slot, scaled by 1e9
const BOOTSTRAP = 1_000n * UNIT; // 1000 of each token
const MINT_TO_PAYER = 100_000n * UNIT;
const FUND_YIELD = 500n * UNIT; // pre-fund each yield source

function loadKeypair(): Keypair {
  const path = join(homedir(), ".config", "solana", "id.json");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path, "utf8"))));
}

const send = (conn: Connection, ix: unknown, payer: Keypair) =>
  sendAndConfirmTransaction(conn, new Transaction().add(ix as never), [payer], {
    commitment: "confirmed",
  });

async function main() {
  const conn = new Connection(process.env.RPC ?? clusterApiUrl("devnet"), "confirmed");
  const payer = loadKeypair();
  const pid = camm.ZENITH_CAMM_PROGRAM_ID;
  console.log(`payer ${payer.publicKey.toBase58()}`);

  // 1. Two test mints (payer is the mint authority). Sort so tokenA < tokenB.
  const m0 = await createMint(conn, payer, payer.publicKey, null, DECIMALS);
  const m1 = await createMint(conn, payer, payer.publicKey, null, DECIMALS);
  const [tokenA, tokenB] = camm.sortMints(m0, m1);
  // Symbols follow creation order (tCP first minted, tUSD second) regardless of
  // which sorts to A/B.
  const symbols = new Map<string, string>([
    [m0.toBase58(), "tCP"],
    [m1.toBase58(), "tUSD"],
  ]);
  console.log(`mints A ${tokenA.toBase58()} B ${tokenB.toBase58()}`);

  // Payer token accounts, funded.
  const userA = (await getOrCreateAssociatedTokenAccount(conn, payer, tokenA, payer.publicKey))
    .address;
  const userB = (await getOrCreateAssociatedTokenAccount(conn, payer, tokenB, payer.publicKey))
    .address;
  await mintTo(conn, payer, tokenA, userA, payer, MINT_TO_PAYER);
  await mintTo(conn, payer, tokenB, userB, payer, MINT_TO_PAYER);

  // Derived accounts.
  const pool = camm.poolPda(tokenA, tokenB).address;
  const poolAuthority = camm.poolAuthorityPda(pool).address;
  const reserveA = camm.reservePda(pool, tokenA).address;
  const reserveB = camm.reservePda(pool, tokenB).address;
  const lpMint = camm.lpMintPda(pool).address;
  const lockedLp = camm.lockedLpPda(pool).address;
  const yieldSourceA = camm.yieldSourcePda(pool, tokenA).address;
  const yieldSourceB = camm.yieldSourcePda(pool, tokenB).address;

  // 2. initialize_pool
  await send(
    conn,
    camm.buildInitializePool({
      creator: payer.publicKey,
      tokenAMint: tokenA,
      tokenBMint: tokenB,
      pool,
      poolAuthority,
      reserveAVault: reserveA,
      reserveBVault: reserveB,
      lpMint,
      lockedLp,
      baseFeeBps: BASE_FEE_BPS,
      protocolFeeRate: PROTOCOL_FEE_RATE,
    }),
    payer,
  );
  console.log(`pool ${pool.toBase58()}`);

  // 3. add_liquidity (bootstrap)
  const userLp = (await getOrCreateAssociatedTokenAccount(conn, payer, lpMint, payer.publicKey))
    .address;
  await send(
    conn,
    camm.buildAddLiquidity({
      owner: payer.publicKey,
      pool,
      poolAuthority,
      lpMint,
      lockedLp,
      reserveAVault: reserveA,
      reserveBVault: reserveB,
      userTokenA: userA,
      userTokenB: userB,
      userLp,
      desiredA: BOOTSTRAP,
      desiredB: BOOTSTRAP,
    }),
    payer,
  );
  console.log(`bootstrapped ${BOOTSTRAP} / ${BOOTSTRAP}`);

  // 4. initialize_yield + fund the sources
  await send(
    conn,
    camm.buildInitializeYield({
      creator: payer.publicKey,
      pool,
      tokenAMint: tokenA,
      tokenBMint: tokenB,
      poolAuthority,
      yieldSourceA,
      yieldSourceB,
      yieldRate: YIELD_RATE,
      bufferBps: BUFFER_BPS,
    }),
    payer,
  );
  await mintTo(conn, payer, tokenA, yieldSourceA, payer, FUND_YIELD);
  await mintTo(conn, payer, tokenB, yieldSourceB, payer, FUND_YIELD);
  console.log(`yield configured (rate ${YIELD_RATE}, buffer ${BUFFER_BPS}bps), sources funded`);

  // 5. rebalance_to_vault (mark idle reserve as deployed)
  await send(
    conn,
    camm.buildRebalanceToVault({
      caller: payer.publicKey,
      pool,
      poolAuthority,
      yieldSourceA,
      yieldSourceB,
      reserveAVault: reserveA,
      reserveBVault: reserveB,
      tokenAMint: tokenA,
      tokenBMint: tokenB,
    }),
    payer,
  );
  console.log("rebalanced");

  // 6. manifest
  const manifest = {
    cluster: "devnet",
    programId: pid.toBase58(),
    pool: pool.toBase58(),
    poolAuthority: poolAuthority.toBase58(),
    lpMint: lpMint.toBase58(),
    lockedLp: lockedLp.toBase58(),
    reserveA: reserveA.toBase58(),
    reserveB: reserveB.toBase58(),
    yieldSourceA: yieldSourceA.toBase58(),
    yieldSourceB: yieldSourceB.toBase58(),
    tokenA: tokenA.toBase58(),
    tokenB: tokenB.toBase58(),
    baseFeeBps: BASE_FEE_BPS,
    protocolFeeRate: PROTOCOL_FEE_RATE,
    yieldRate: YIELD_RATE.toString(),
    bufferBps: BUFFER_BPS,
    mints: {
      [tokenA.toBase58()]: { symbol: symbols.get(tokenA.toBase58()), decimals: DECIMALS },
      [tokenB.toBase58()]: { symbol: symbols.get(tokenB.toBase58()), decimals: DECIMALS },
    },
  };
  const out = join(import.meta.dirname, "..", "..", "app", "src", "camm-devnet.json");
  writeFileSync(out, JSON.stringify(manifest, null, 2) + "\n");
  console.log(`wrote ${out}`);
  console.log("SEED DONE");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
