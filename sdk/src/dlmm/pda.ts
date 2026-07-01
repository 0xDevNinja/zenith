import { PublicKey } from "@solana/web3.js";

import { DLMM_SEEDS, ZENITH_DLMM_PROGRAM_ID } from "./constants.js";

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

/// Canonical ascending order of a pair's two mints (mirrors `sort_mints`), so
/// the pair PDA does not depend on the order the caller passes them.
export function sortMints(a: PublicKey, b: PublicKey): [PublicKey, PublicKey] {
  return Buffer.compare(a.toBuffer(), b.toBuffer()) <= 0 ? [a, b] : [b, a];
}

const u16le = (n: number): Buffer => {
  const b = Buffer.alloc(2);
  b.writeUInt16LE(n);
  return b;
};

const i64le = (n: bigint): Buffer => {
  const b = Buffer.alloc(8);
  b.writeBigInt64LE(BigInt(n));
  return b;
};

/// LbPair PDA for a (unordered) mint pair and bin step
/// (`[lb_pair, min(mints), max(mints), bin_step as u16-le]`).
export function lbPairPda(
  mintA: PublicKey,
  mintB: PublicKey,
  binStep: number,
  programId = ZENITH_DLMM_PROGRAM_ID,
): Pda {
  const [m0, m1] = sortMints(mintA, mintB);
  return derive(
    [seed(DLMM_SEEDS.lbPair), m0.toBuffer(), m1.toBuffer(), u16le(binStep)],
    programId,
  );
}

/// Pair authority PDA — signs for the pair's token reserves.
export function pairAuthorityPda(
  lbPair: PublicKey,
  programId = ZENITH_DLMM_PROGRAM_ID,
): Pda {
  return derive([seed(DLMM_SEEDS.pairAuthority), lbPair.toBuffer()], programId);
}

/// Reserve (vault) PDA for a pair + the mint it holds.
export function reservePda(
  lbPair: PublicKey,
  mint: PublicKey,
  programId = ZENITH_DLMM_PROGRAM_ID,
): Pda {
  return derive(
    [seed(DLMM_SEEDS.reserve), lbPair.toBuffer(), mint.toBuffer()],
    programId,
  );
}

/// BinArray PDA for a pair + signed array index (`index as i64-le`).
export function binArrayPda(
  lbPair: PublicKey,
  index: number | bigint,
  programId = ZENITH_DLMM_PROGRAM_ID,
): Pda {
  return derive(
    [seed(DLMM_SEEDS.binArray), lbPair.toBuffer(), i64le(BigInt(index))],
    programId,
  );
}

/// Position PDA for a caller-supplied base pubkey (the position's unique id).
export function positionPda(base: PublicKey, programId = ZENITH_DLMM_PROGRAM_ID): Pda {
  return derive([seed(DLMM_SEEDS.position), base.toBuffer()], programId);
}

/// Oracle (TWAP ring buffer) PDA for a pair.
export function oraclePda(lbPair: PublicKey, programId = ZENITH_DLMM_PROGRAM_ID): Pda {
  return derive([seed(DLMM_SEEDS.oracle), lbPair.toBuffer()], programId);
}

/// The bin-array index that contains a given bin id (floor division, matching
/// the program's `div_euclid` for negative bins).
export function binArrayIndexOf(binId: number): number {
  return Math.floor(binId / BINS_PER_ARRAY_FOR_INDEX);
}

const BINS_PER_ARRAY_FOR_INDEX = 70;
