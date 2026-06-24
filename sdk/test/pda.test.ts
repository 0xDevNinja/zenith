import { describe, expect, it } from "vitest";
import { PublicKey } from "@solana/web3.js";
import {
  configPda,
  poolAuthorityPda,
  poolPda,
  positionPda,
  sortMints,
  vaultPda,
} from "../src/pda.js";

const MINT_A = new PublicKey("So11111111111111111111111111111111111111112");
const MINT_B = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

describe("PDA on-chain parity", () => {
  // Golden vectors emitted by the program's own pda.rs (same seeds + algorithm).
  // If the SDK seeds drift from the program, these break.
  const config = configPda(0);
  const pool = poolPda(config.address, MINT_A, MINT_B);

  it("config PDA matches the program", () => {
    expect(config.address.toBase58()).toBe("CjhWAYbfECu3NGTjtScmdu1v8NduPBoHMRrWAmziVStK");
  });

  it("pool PDA matches the program", () => {
    expect(pool.address.toBase58()).toBe("6xhGycp5XkHkDxCVhXxTfmUKe6xkrmHQid9NSQx8bjfZ");
  });

  it("pool authority PDA matches the program", () => {
    expect(poolAuthorityPda(pool.address).address.toBase58()).toBe(
      "6wcRgxVcx3t3pVkKrNEUZjZwYYfsJRrS2bgNtBZWSdHj",
    );
  });

  it("vault PDA matches the program", () => {
    expect(vaultPda(pool.address, MINT_A).address.toBase58()).toBe(
      "G8KSKMGcLZZfqzozv8L1se97HTHnhQvN8pQDqJN6ZE64",
    );
  });

  it("position PDA matches the program", () => {
    expect(positionPda(MINT_A).address.toBase58()).toBe(
      "6G8J1h8ufBFxKmfnByB5APmhyRYfKRMyqNB4fQaQaCGN",
    );
  });
});

describe("PDA structural invariants", () => {
  it("pool PDA is mint-order independent", () => {
    const c = configPda(0).address;
    expect(poolPda(c, MINT_A, MINT_B).address.toBase58()).toBe(
      poolPda(c, MINT_B, MINT_A).address.toBase58(),
    );
  });

  it("sortMints orders ascending by bytes", () => {
    const [m0, m1] = sortMints(MINT_B, MINT_A);
    expect(Buffer.compare(m0.toBuffer(), m1.toBuffer())).toBeLessThanOrEqual(0);
  });

  it("config index is encoded little-endian (distinct per index)", () => {
    expect(configPda(0).address.toBase58()).not.toBe(configPda(1).address.toBase58());
    expect(configPda(256).address.toBase58()).not.toBe(configPda(1).address.toBase58());
  });

  it("distinct seeds give distinct addresses for the same key", () => {
    const c = configPda(0).address;
    const pool = poolPda(c, MINT_A, MINT_B).address;
    expect(vaultPda(pool, MINT_A).address.toBase58()).not.toBe(
      poolAuthorityPda(pool).address.toBase58(),
    );
    expect(positionPda(MINT_A).address.toBase58()).not.toBe(
      vaultPda(pool, MINT_A).address.toBase58(),
    );
  });
});
