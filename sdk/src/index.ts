// @zenith/sdk — public entry point.
//
// M2 foundation: program id + PDA helpers (mirror the on-chain seeds), an RPC
// connection wrapper, and the committed IDL. Account decoders, the exact-math
// quote engine, and transaction builders land in the following M2 issues.

export const VERSION = "0.1.0";

export { ZENITH_AMM_PROGRAM_ID, SEEDS } from "./constants.js";
export {
  type Pda,
  sortMints,
  configPda,
  poolPda,
  poolAuthorityPda,
  vaultPda,
  positionPda,
  positionNftCustodyPda,
} from "./pda.js";
export { ZenithConnection, type ZenithConnectionOptions } from "./connection.js";
export {
  ZENITH_AMM_IDL,
  type ZenithAmmIdl,
  type ZenithInstructionName,
  type ZenithAccountName,
} from "./idl.js";
export {
  Rounding,
  SCALE_OFFSET,
  ONE_Q64,
  U128_MAX,
  U64_MAX,
  mulDiv,
  mulShr,
  shlDiv,
  Q64,
  isqrt,
  sqrtPriceFromPrice,
  priceFromSqrtPrice,
  deltaA,
  deltaB,
  liquidityFromAmountA,
  liquidityFromAmountB,
  nextSqrtPriceFromAmountX,
  nextSqrtPriceFromAmountY,
  SwapDirection,
  SwapMode,
  SwapError,
  type SwapErrorCode,
  type SwapStep,
  computeSwapStep,
} from "./math/index.js";
export {
  Reader,
  DISCRIMINATORS,
  PoolStatus,
  TokenFlavor,
  FeeSchedulerMode,
  decodePool,
  decodePosition,
  decodeConfig,
  type Pool,
  type Position,
  type Config,
  fetchPool,
  fetchPosition,
  fetchConfig,
} from "./coder/index.js";
