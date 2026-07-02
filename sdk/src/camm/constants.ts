import { PublicKey } from "@solana/web3.js";

/// On-chain program id for zenith-camm (matches `declare_id!`).
export const ZENITH_CAMM_PROGRAM_ID = new PublicKey(
  "CjjcK3rnskHswBpTgZquLGgS7P2QyzeaNwwe98FUUdy7",
);

/// PDA seed strings — the single source of truth shared with the program's
/// `constants.rs`. Changing one here without the program diverges every PDA.
export const CAMM_SEEDS = {
  pool: "cp_pool",
  poolAuthority: "cp_authority",
  reserve: "cp_reserve",
  lpMint: "cp_lp_mint",
  lockedLp: "cp_locked_lp",
  yieldSource: "cp_yield_source",
} as const;

/// Basis-point denominator for fees and rates.
export const BPS_DENOMINATOR = 10_000n;
/// Minimum liquidity permanently locked on the first deposit (Uniswap-v2 style).
export const MINIMUM_LIQUIDITY = 1_000n;
/// Fixed-point scale for the yield rate (yield per deployed unit per slot).
export const YIELD_SCALE = 1_000_000_000n;
/// Decimals of the fungible LP-share mint.
export const LP_MINT_DECIMALS = 9;
