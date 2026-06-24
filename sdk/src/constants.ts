import { PublicKey } from "@solana/web3.js";

/// On-chain program id for zenith-amm (matches `declare_id!`).
export const ZENITH_AMM_PROGRAM_ID = new PublicKey(
  "AA8cKcHQj63GEHRaLrrT87W1efRZ44U147JTCXC2Rmkq",
);

/// PDA seed strings — the single source of truth shared with the program's
/// `constants.rs`. Changing one here without the program diverges every PDA.
export const SEEDS = {
  config: "config",
  pool: "pool",
  poolAuthority: "pool_authority",
  vault: "vault",
  position: "position",
  positionNft: "position_nft",
} as const;
