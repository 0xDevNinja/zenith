import { PublicKey } from "@solana/web3.js";
import { SEEDS, ZENITH_AMM_PROGRAM_ID } from "./constants.js";

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

/// Config PDA for an index (`[CONFIG_SEED, index as u16-le]`).
export function configPda(index: number, programId = ZENITH_AMM_PROGRAM_ID): Pda {
  const indexLe = Buffer.alloc(2);
  indexLe.writeUInt16LE(index);
  return derive([seed(SEEDS.config), indexLe], programId);
}

/// Pool PDA for a config and its (unordered) token mints.
export function poolPda(
  config: PublicKey,
  mintA: PublicKey,
  mintB: PublicKey,
  programId = ZENITH_AMM_PROGRAM_ID,
): Pda {
  const [m0, m1] = sortMints(mintA, mintB);
  return derive(
    [seed(SEEDS.pool), config.toBuffer(), m0.toBuffer(), m1.toBuffer()],
    programId,
  );
}

/// Pool authority PDA — signs for the pool's vaults and the NFT mint.
export function poolAuthorityPda(pool: PublicKey, programId = ZENITH_AMM_PROGRAM_ID): Pda {
  return derive([seed(SEEDS.poolAuthority), pool.toBuffer()], programId);
}

/// Token vault PDA for a pool + the mint it holds.
export function vaultPda(pool: PublicKey, mint: PublicKey, programId = ZENITH_AMM_PROGRAM_ID): Pda {
  return derive([seed(SEEDS.vault), pool.toBuffer(), mint.toBuffer()], programId);
}

/// Position PDA for a position-NFT mint.
export function positionPda(nftMint: PublicKey, programId = ZENITH_AMM_PROGRAM_ID): Pda {
  return derive([seed(SEEDS.position), nftMint.toBuffer()], programId);
}

/// Position-NFT custody PDA (token account holding a locked NFT).
export function positionNftCustodyPda(
  nftMint: PublicKey,
  programId = ZENITH_AMM_PROGRAM_ID,
): Pda {
  return derive([seed(SEEDS.positionNft), nftMint.toBuffer()], programId);
}
