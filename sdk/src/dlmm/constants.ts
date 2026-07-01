import { PublicKey } from "@solana/web3.js";

/// On-chain program id for zenith-dlmm (matches `declare_id!`).
export const ZENITH_DLMM_PROGRAM_ID = new PublicKey(
  "7pxn8tEm44gXjfPH9YXsLywuYpAbgbxq9nPwG1XQczsz",
);

/// PDA seed strings — the single source of truth shared with the program's
/// `constants.rs`. Changing one here without the program diverges every PDA.
export const DLMM_SEEDS = {
  lbPair: "lb_pair",
  pairAuthority: "pair_authority",
  reserve: "reserve",
  binArray: "bin_array",
  position: "position",
  oracle: "oracle",
} as const;

/// Bins per `BinArray` account (`bins[i]` is bin id `index * BINS_PER_ARRAY + i`).
export const BINS_PER_ARRAY = 70;
/// Maximum bins a single position may span.
export const BINS_PER_POSITION = 70;
/// Ring-buffer capacity of the TWAP oracle.
export const ORACLE_CAPACITY = 64;
/// Basis-point denominator for fees and rates.
export const BPS_DENOMINATOR = 10_000;
