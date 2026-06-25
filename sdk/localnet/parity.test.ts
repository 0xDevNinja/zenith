import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMint2Instruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  Keypair,
  PublicKey,
  type Signer,
  SystemProgram,
  Transaction,
  type TransactionInstruction,
} from "@solana/web3.js";
import { type BanksClient, type ProgramTestContext, start } from "solana-bankrun";
import { beforeAll, describe, expect, it } from "vitest";
import {
  type Built,
  buildCreateConfig,
  buildInitializePool,
  buildSwap,
  configPda,
  decodeConfig,
  decodePool,
  effectiveFeeBps,
  type Pool,
  poolPda,
  quoteSwap,
  sortMints,
  SwapDirection,
  SwapMode,
  ZENITH_AMM_PROGRAM_ID,
} from "../src/index.js";

// Point bankrun at the cargo-build-sbf output so `{ name: "zenith_amm" }`
// resolves the program .so.
const deployDir = resolve(fileURLToPath(new URL("../../target/deploy", import.meta.url)));
process.env.BPF_OUT_DIR = deployDir;

const DECIMALS = 6;
const ONE = 1n << 64n;

let ctx: ProgramTestContext;
let client: BanksClient;
let payer: Keypair;
let mintA: Keypair;
let mintB: Keypair;

async function send(ixs: TransactionInstruction[], signers: Signer[] = []): Promise<void> {
  const tx = new Transaction();
  for (const ix of ixs) tx.add(ix);
  const latest = await client.getLatestBlockhash();
  if (!latest) throw new Error("no blockhash");
  tx.recentBlockhash = latest[0];
  tx.feePayer = payer.publicKey;
  tx.sign(payer, ...signers);
  await client.processTransaction(tx);
}

async function sendBuilt(b: Built): Promise<void> {
  await send(b.instructions, b.signers);
}

async function createMint(mint: Keypair): Promise<void> {
  const rent = await client.getRent();
  const lamports = Number(rent.minimumBalance(BigInt(MINT_SIZE)));
  await send(
    [
      SystemProgram.createAccount({
        fromPubkey: payer.publicKey,
        newAccountPubkey: mint.publicKey,
        lamports,
        space: MINT_SIZE,
        programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeMint2Instruction(mint.publicKey, DECIMALS, payer.publicKey, null),
    ],
    [mint],
  );
}

async function fundAta(mint: PublicKey, amount: bigint): Promise<PublicKey> {
  const acc = getAssociatedTokenAddressSync(mint, payer.publicKey, true);
  await send([
    createAssociatedTokenAccountIdempotentInstruction(payer.publicKey, acc, payer.publicKey, mint),
    createMintToInstruction(mint, acc, payer.publicKey, amount),
  ]);
  return acc;
}

async function tokenAmount(account: PublicKey): Promise<bigint> {
  const info = await client.getAccount(account);
  if (!info) return 0n;
  // SPL token account: amount is a u64 LE at offset 64.
  const dv = new DataView(info.data.buffer, info.data.byteOffset, info.data.byteLength);
  return dv.getBigUint64(64, true);
}

async function fetchPool(addr: PublicKey): Promise<Pool> {
  const info = await client.getAccount(addr);
  if (!info) throw new Error("pool missing");
  return decodePool(Uint8Array.from(info.data));
}

beforeAll(async () => {
  ctx = await start([{ name: "zenith_amm", programId: ZENITH_AMM_PROGRAM_ID }], []);
  client = ctx.banksClient;
  payer = ctx.payer;

  // Two SPL mints, funded into the payer's ATAs.
  mintA = Keypair.generate();
  mintB = Keypair.generate();
  await createMint(mintA);
  await createMint(mintB);
  await fundAta(mintA.publicKey, 1_000_000_000_000n);
  await fundAta(mintB.publicKey, 1_000_000_000_000n);
}, 120_000);

describe("localnet parity: SDK quote vs on-chain swap", () => {
  const index = 0;
  const config = configPda(index).address;

  it("creates a config and initializes a pool", async () => {
    await sendBuilt(
      buildCreateConfig({
        admin: payer.publicKey,
        params: {
          index,
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
          variableFeeControl: 0, // dynamic fee disabled -> deterministic
          maxVolatilityAccumulator: 0,
          filterPeriod: 10,
          decayPeriod: 100,
          volatilityReductionFactor: 0,
          maxDynamicFeeBps: 0,
        },
      }),
    );

    const init = buildInitializePool({
      creator: payer.publicKey,
      config,
      mintA: mintA.publicKey,
      mintB: mintB.publicKey,
      sqrtPrice: ONE, // price 1.0, inside the band
      liquidity: 1_000_000_000n,
      tokenAMax: 1n << 60n,
      tokenBMax: 1n << 60n,
    });
    await sendBuilt(init);

    const pool = await fetchPool(poolPda(config, mintA.publicKey, mintB.publicKey).address);
    expect(pool.liquidity).toBe(1_000_000_000n);
    expect(pool.sqrtPrice).toBe(ONE);
  });

  it("ExactIn swap output matches quoteSwap exactly", async () => {
    const poolAddr = poolPda(config, mintA.publicKey, mintB.publicKey).address;
    const [tokenA, tokenB] = sortMints(mintA.publicKey, mintB.publicKey);
    const ataA = getAssociatedTokenAddressSync(tokenA, payer.publicKey, true);
    const ataB = getAssociatedTokenAddressSync(tokenB, payer.publicKey, true);

    const pool = await fetchPool(poolAddr);
    const cfgInfo = await client.getAccount(config);
    const cfg = decodeConfig(Uint8Array.from(cfgInfo!.data));
    const slot = (await client.getClock()).slot;

    const amount = 1_000_000n;
    const quote = quoteSwap({
      pool,
      config: cfg,
      slot,
      direction: SwapDirection.BToA, // sell B, receive A
      mode: SwapMode.ExactIn,
      amount,
      slippageBps: 50,
    });

    // Fee derivation matches the program's constant scheduler (no dynamic fee).
    expect(effectiveFeeBps(cfg, pool, slot).totalFeeBps).toBe(30);
    expect(quote.amountOut > 0n).toBe(true);

    const beforeA = await tokenAmount(ataA);
    const beforeB = await tokenAmount(ataB);

    await sendBuilt(
      buildSwap({
        owner: payer.publicKey,
        pool: poolAddr,
        config,
        mintA: mintA.publicKey,
        mintB: mintB.publicKey,
        direction: SwapDirection.BToA,
        mode: SwapMode.ExactIn,
        amount,
        otherAmountThreshold: quote.otherAmountThreshold,
      }),
    );

    const afterA = await tokenAmount(ataA);
    const afterB = await tokenAmount(ataB);

    // Output (token A) received and input (token B) spent match the quote.
    expect(afterA - beforeA).toBe(quote.amountOut);
    expect(beforeB - afterB).toBe(quote.amountIn);

    // Pool price advanced exactly to the quoted next price.
    const after = await fetchPool(poolAddr);
    expect(after.sqrtPrice).toBe(quote.nextSqrtPrice);
  });

  it("ExactOut swap input matches quoteSwap exactly", async () => {
    const poolAddr = poolPda(config, mintA.publicKey, mintB.publicKey).address;
    const [tokenA, tokenB] = sortMints(mintA.publicKey, mintB.publicKey);
    const ataA = getAssociatedTokenAddressSync(tokenA, payer.publicKey, true);
    const ataB = getAssociatedTokenAddressSync(tokenB, payer.publicKey, true);

    const pool = await fetchPool(poolAddr);
    const cfg = decodeConfig(Uint8Array.from((await client.getAccount(config))!.data));
    const slot = (await client.getClock()).slot;

    const wantOut = 500_000n;
    const quote = quoteSwap({
      pool,
      config: cfg,
      slot,
      direction: SwapDirection.BToA,
      mode: SwapMode.ExactOut,
      amount: wantOut,
      slippageBps: 50,
    });

    const beforeA = await tokenAmount(ataA);
    const beforeB = await tokenAmount(ataB);

    await sendBuilt(
      buildSwap({
        owner: payer.publicKey,
        pool: poolAddr,
        config,
        mintA: mintA.publicKey,
        mintB: mintB.publicKey,
        direction: SwapDirection.BToA,
        mode: SwapMode.ExactOut,
        amount: wantOut,
        otherAmountThreshold: quote.otherAmountThreshold, // max-in
      }),
    );

    const afterA = await tokenAmount(ataA);
    const afterB = await tokenAmount(ataB);

    expect(afterA - beforeA).toBe(wantOut);
    expect(afterA - beforeA).toBe(quote.amountOut);
    expect(beforeB - afterB).toBe(quote.amountIn);
    // The spent input respects the slippage ceiling.
    expect(beforeB - afterB <= quote.otherAmountThreshold).toBe(true);
  });
});
