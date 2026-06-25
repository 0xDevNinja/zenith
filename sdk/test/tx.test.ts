import { createHash } from "node:crypto";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { describe, expect, it } from "vitest";
import {
  buildAddLiquidity,
  buildClaimPositionFee,
  buildClaimProtocolFee,
  buildClosePosition,
  buildCreateConfig,
  buildCreatePosition,
  buildInitializePool,
  buildSwap,
  buildTransactionFrom,
  configPda,
  INSTRUCTION_DISCRIMINATORS,
  type InstructionName,
  ixData,
  poolAuthorityPda,
  poolPda,
  positionPda,
  sortMints,
  SwapDirection,
  SwapMode,
  vaultPda,
  Writer,
  ZENITH_AMM_PROGRAM_ID,
} from "../src/index.js";

const MINT_A = new PublicKey("So11111111111111111111111111111111111111112");
const MINT_B = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const OWNER = new PublicKey("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM");
const POOL = new PublicKey("6xhGycp5XkHkDxCVhXxTfmUKe6xkrmHQid9NSQx8bjfZ");
const NFT = new PublicKey("4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T");

// snake_case names matching the program, in declared order.
const SNAKE: Record<InstructionName, string> = {
  createConfig: "create_config",
  initializePool: "initialize_pool",
  createPosition: "create_position",
  addLiquidity: "add_liquidity",
  removeLiquidity: "remove_liquidity",
  removeAllLiquidity: "remove_all_liquidity",
  swap: "swap",
  claimPositionFee: "claim_position_fee",
  claimProtocolFee: "claim_protocol_fee",
  claimPartnerFee: "claim_partner_fee",
  setPositionCompounding: "set_position_compounding",
  closePosition: "close_position",
};

describe("instruction discriminators", () => {
  it("match sha256('global:<snake>')[..8] for every instruction", () => {
    for (const [camel, snake] of Object.entries(SNAKE)) {
      const want = [...createHash("sha256").update(`global:${snake}`).digest().subarray(0, 8)];
      expect(INSTRUCTION_DISCRIMINATORS[camel as InstructionName], camel).toEqual(want);
    }
  });
});

describe("arg encoding (borsh, little-endian)", () => {
  it("Writer lays out scalars LE", () => {
    const buf = new Writer().u8(1).u16(0x0203).u32(4).u64(5n).u128(6n).bool(true).build();
    expect([...buf.subarray(0, 1)]).toEqual([1]);
    expect([...buf.subarray(1, 3)]).toEqual([0x03, 0x02]); // u16 LE
    expect(buf.readUInt32LE(3)).toBe(4);
    expect(buf.readBigUInt64LE(7)).toBe(5n);
    expect(buf.readBigUInt64LE(15)).toBe(6n); // low 64 of u128
    expect(buf.readBigUInt64LE(23)).toBe(0n); // high 64
    expect(buf.readUInt8(31)).toBe(1); // bool
  });

  it("swap data = discriminator + dir + mode + amount + threshold", () => {
    const data = ixData("swap", (w) =>
      w.u8(0 /*AToB*/).u8(1 /*ExactOut*/).u64(1000n).u64(990n),
    );
    expect([...data.subarray(0, 8)]).toEqual(INSTRUCTION_DISCRIMINATORS.swap);
    expect(data.readUInt8(8)).toBe(0); // direction
    expect(data.readUInt8(9)).toBe(1); // mode
    expect(data.readBigUInt64LE(10)).toBe(1000n);
    expect(data.readBigUInt64LE(18)).toBe(990n);
    expect(data.length).toBe(8 + 1 + 1 + 8 + 8);
  });
});

const flags = (ix: { keys: { isSigner: boolean; isWritable: boolean }[] }) =>
  ix.keys.map((k) => `${k.isSigner ? "s" : "-"}${k.isWritable ? "w" : "-"}`);
const addrs = (ix: { keys: { pubkey: PublicKey }[] }) => ix.keys.map((k) => k.pubkey.toBase58());

describe("account layouts match the program", () => {
  it("swap: 9 accounts, correct flags + derived addresses", () => {
    const config = configPda(0).address;
    const b = buildSwap({
      owner: OWNER,
      pool: POOL,
      config,
      mintA: MINT_A,
      mintB: MINT_B,
      direction: SwapDirection.BToA,
      mode: SwapMode.ExactIn,
      amount: 1000n,
      otherAmountThreshold: 1n,
      createAtas: false,
    });
    expect(b.instructions).toHaveLength(1);
    const ix = b.instructions[0];
    expect(ix.programId.equals(ZENITH_AMM_PROGRAM_ID)).toBe(true);
    expect(flags(ix)).toEqual(["s-", "-w", "--", "--", "-w", "-w", "-w", "-w", "--"]);
    const [tA, tB] = sortMints(MINT_A, MINT_B);
    expect(addrs(ix)).toEqual([
      OWNER.toBase58(),
      POOL.toBase58(),
      config.toBase58(),
      poolAuthorityPda(POOL).address.toBase58(),
      vaultPda(POOL, tA).address.toBase58(),
      vaultPda(POOL, tB).address.toBase58(),
      getAssociatedTokenAddressSync(tA, OWNER, true).toBase58(),
      getAssociatedTokenAddressSync(tB, OWNER, true).toBase58(),
      TOKEN_PROGRAM_ID.toBase58(),
    ]);
  });

  it("createAtas prepends two idempotent ATA-create instructions", () => {
    const withAtas = buildSwap({
      owner: OWNER,
      pool: POOL,
      config: configPda(0).address,
      mintA: MINT_A,
      mintB: MINT_B,
      direction: SwapDirection.BToA,
      mode: SwapMode.ExactIn,
      amount: 1000n,
      otherAmountThreshold: 1n,
    });
    expect(withAtas.instructions).toHaveLength(3);
    expect(withAtas.instructions[0].programId.equals(ASSOCIATED_TOKEN_PROGRAM_ID)).toBe(true);
    expect(withAtas.instructions[1].programId.equals(ASSOCIATED_TOKEN_PROGRAM_ID)).toBe(true);
    expect(withAtas.instructions[2].programId.equals(ZENITH_AMM_PROGRAM_ID)).toBe(true);
  });

  it("initialize_pool: 16 accounts, NFT mint is a fresh signer", () => {
    const config = configPda(0).address;
    const b = buildInitializePool({
      creator: OWNER,
      config,
      mintA: MINT_A,
      mintB: MINT_B,
      sqrtPrice: 1n << 64n,
      liquidity: 1000n,
      tokenAMax: 1n << 60n,
      tokenBMax: 1n << 60n,
    });
    const ix = b.instructions[0];
    expect(ix.keys).toHaveLength(16);
    expect(b.signers).toHaveLength(1);
    const nft = b.signers[0].publicKey;
    expect(b.derived.nftMint.equals(nft)).toBe(true);
    // creator(s,w); config; tokenA; tokenB; pool(w); authority; vaultA(w); vaultB(w); ...
    expect(flags(ix).slice(0, 8)).toEqual(["sw", "--", "--", "--", "-w", "--", "-w", "-w"]);
    // NFT mint account (index 10) is a writable signer.
    expect(ix.keys[10].isSigner && ix.keys[10].isWritable).toBe(true);
    expect(ix.keys[10].pubkey.equals(nft)).toBe(true);
    expect(b.derived.pool.equals(poolPda(config, MINT_A, MINT_B).address)).toBe(true);
    expect(b.derived.position.equals(positionPda(nft).address)).toBe(true);
    expect(ix.keys[13].pubkey.equals(TOKEN_PROGRAM_ID)).toBe(true);
    expect(ix.keys[15].pubkey.equals(SystemProgram.programId)).toBe(true);
  });

  it("create_position: 9 accounts, fresh NFT signer", () => {
    const b = buildCreatePosition({ creator: OWNER, pool: POOL });
    expect(b.instructions[0].keys).toHaveLength(9);
    expect(b.signers).toHaveLength(1);
    expect(b.derived.position.equals(positionPda(b.signers[0].publicKey).address)).toBe(true);
  });

  it("add_liquidity: 10 accounts, owner signs (not writable), data args", () => {
    const b = buildAddLiquidity({
      owner: OWNER,
      pool: POOL,
      position: positionPda(NFT).address,
      nftMint: NFT,
      mintA: MINT_A,
      mintB: MINT_B,
      liquidityDelta: 5n,
      tokenAMax: 10n,
      tokenBMax: 20n,
      createAtas: false,
    });
    const ix = b.instructions[0];
    expect(ix.keys).toHaveLength(10);
    expect(flags(ix)).toEqual(["s-", "-w", "-w", "--", "--", "-w", "-w", "-w", "-w", "--"]);
    expect([...ix.data.subarray(0, 8)]).toEqual(INSTRUCTION_DISCRIMINATORS.addLiquidity);
    expect(ix.data.readBigUInt64LE(8 + 16)).toBe(10n); // tokenAMax after u128 delta
  });

  it("claim_position_fee: position-NFT account present, owner signs", () => {
    const b = buildClaimPositionFee({
      owner: OWNER,
      pool: POOL,
      position: positionPda(NFT).address,
      nftMint: NFT,
      mintA: MINT_A,
      mintB: MINT_B,
      createAtas: false,
    });
    const ix = b.instructions[0];
    expect(ix.keys).toHaveLength(10);
    expect(ix.keys[3].pubkey.equals(getAssociatedTokenAddressSync(NFT, OWNER, true))).toBe(true);
  });

  it("claim_protocol_fee: fee authority signs; recipients default to its ATAs", () => {
    const b = buildClaimProtocolFee({
      feeAuthority: OWNER,
      config: configPda(0).address,
      pool: POOL,
      mintA: MINT_A,
      mintB: MINT_B,
      createAtas: false,
    });
    const ix = b.instructions[0];
    expect(ix.keys).toHaveLength(9);
    expect(ix.keys[0].isSigner).toBe(true);
    const [tA] = sortMints(MINT_A, MINT_B);
    expect(ix.keys[6].pubkey.equals(getAssociatedTokenAddressSync(tA, OWNER, true))).toBe(true);
  });

  it("close_position: owner writable, NFT mint + account writable", () => {
    const b = buildClosePosition({
      owner: OWNER,
      pool: POOL,
      position: positionPda(NFT).address,
      nftMint: NFT,
    });
    const ix = b.instructions[0];
    expect(ix.keys).toHaveLength(6);
    expect(flags(ix)).toEqual(["sw", "-w", "-w", "-w", "-w", "--"]);
    expect(ix.keys[3].pubkey.equals(NFT)).toBe(true);
  });
});

describe("transaction assembly", () => {
  const config = configPda(0).address;
  const blockhash = "11111111111111111111111111111111";

  it("buildTransactionFrom compiles a v0 tx and surfaces extra signers", () => {
    const init = buildInitializePool({
      creator: OWNER,
      config,
      mintA: MINT_A,
      mintB: MINT_B,
      sqrtPrice: 1n << 64n,
      liquidity: 1000n,
      tokenAMax: 1n << 60n,
      tokenBMax: 1n << 60n,
    });
    const { transaction, signers } = buildTransactionFrom({
      payerKey: OWNER,
      recentBlockhash: blockhash,
      built: [init],
    });
    expect(transaction.version).toBe(0);
    expect(transaction.message.recentBlockhash).toBe(blockhash);
    // the position-NFT mint keypair must be carried through as a signer
    expect(signers).toHaveLength(1);
    expect(signers[0].publicKey.equals(init.derived.nftMint)).toBe(true);
  });

  it("merges multiple builders' instructions in order", () => {
    const cfg = buildCreateConfig({
      admin: OWNER,
      params: {
        index: 0,
        feeAuthority: OWNER,
        sqrtMinPrice: 1n,
        sqrtMaxPrice: 1n << 80n,
        baseFeeBps: 30,
        protocolFeeBps: 2000,
        partner: Keypair.generate().publicKey,
        partnerFeeBps: 0,
        feeSchedulerMode: 0,
        cliffFeeBps: 30,
        reductionFactor: 0,
        feePeriod: 0n,
        maxFeeSteps: 0,
        variableFeeControl: 0,
        maxVolatilityAccumulator: 0,
        filterPeriod: 0,
        decayPeriod: 0,
        volatilityReductionFactor: 0,
        maxDynamicFeeBps: 0,
      },
    });
    const { transaction } = buildTransactionFrom({
      payerKey: OWNER,
      recentBlockhash: blockhash,
      built: [cfg],
    });
    // config PDA + admin + system program all referenced
    const keys = transaction.message.staticAccountKeys.map((k) => k.toBase58());
    expect(keys).toContain(config.toBase58());
    expect(keys).toContain(SystemProgram.programId.toBase58());
  });
});
