import { PublicKey } from "@solana/web3.js";

import { CAMM_SEEDS, ZENITH_CAMM_PROGRAM_ID } from "./constants.js";

/// A derived address with its bump.
export interface Pda {
  address: PublicKey;
  bump: number;
}

const seed = (s: string): Buffer => Buffer.from(s, "utf8");

function derive(seeds: Array<Buffer | Uint8Array>, programId: PublicKey): Pda {
  const [address, bump] = PublicKey.findProgramAddressSync(seeds, programId);
  return { address, bump };
}

/// Canonical ascending order of a pool's two mints (mirrors `sort_mints`), so
/// the pool PDA does not depend on the order the caller passes them.
export function sortMints(a: PublicKey, b: PublicKey): [PublicKey, PublicKey] {
  return Buffer.compare(a.toBuffer(), b.toBuffer()) <= 0 ? [a, b] : [b, a];
}

/// Pool PDA for a (unordered) mint pair (`[cp_pool, min(mints), max(mints)]`).
export function poolPda(
  mintA: PublicKey,
  mintB: PublicKey,
  programId = ZENITH_CAMM_PROGRAM_ID,
): Pda {
  const [m0, m1] = sortMints(mintA, mintB);
  return derive([seed(CAMM_SEEDS.pool), m0.toBuffer(), m1.toBuffer()], programId);
}

/// Pool authority PDA — signs for the reserves and the LP mint.
export function poolAuthorityPda(pool: PublicKey, programId = ZENITH_CAMM_PROGRAM_ID): Pda {
  return derive([seed(CAMM_SEEDS.poolAuthority), pool.toBuffer()], programId);
}

/// Reserve (vault) PDA for a pool + the mint it holds.
export function reservePda(
  pool: PublicKey,
  mint: PublicKey,
  programId = ZENITH_CAMM_PROGRAM_ID,
): Pda {
  return derive([seed(CAMM_SEEDS.reserve), pool.toBuffer(), mint.toBuffer()], programId);
}

/// LP-share mint PDA for a pool.
export function lpMintPda(pool: PublicKey, programId = ZENITH_CAMM_PROGRAM_ID): Pda {
  return derive([seed(CAMM_SEEDS.lpMint), pool.toBuffer()], programId);
}

/// Locked-liquidity token account PDA for a pool.
export function lockedLpPda(pool: PublicKey, programId = ZENITH_CAMM_PROGRAM_ID): Pda {
  return derive([seed(CAMM_SEEDS.lockedLp), pool.toBuffer()], programId);
}

/// Yield-source vault PDA for a pool + the mint it pays yield in.
export function yieldSourcePda(
  pool: PublicKey,
  mint: PublicKey,
  programId = ZENITH_CAMM_PROGRAM_ID,
): Pda {
  return derive([seed(CAMM_SEEDS.yieldSource), pool.toBuffer(), mint.toBuffer()], programId);
}
