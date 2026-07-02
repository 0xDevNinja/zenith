// @zenith/sdk — public entry point.
//
// M2 foundation: program id + PDA helpers (mirror the on-chain seeds), an RPC
// connection wrapper, and the committed IDL. Account decoders, the exact-math
// quote engine, and transaction builders land in the following M2 issues.

export const VERSION = "0.1.0";

// zenith-dlmm (liquidity book) — PDA + account decoders under the `dlmm`
// namespace to avoid clashing with the AMM's flat exports.
export * as dlmm from "./dlmm/index.js";

// zenith-camm (full-range constant-product + yield) — under the `camm`
// namespace, same reason.
export * as camm from "./camm/index.js";

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
  pow,
  BPS_DENOMINATOR,
  DYNAMIC_FEE_DENOMINATOR,
  FeeMode,
  FeeError,
  scheduledBaseFeeBps,
  priceMoveBps,
  decayedVolatilityReference,
  accumulateVolatility,
  dynamicFeeBps,
  computeDynamicFee,
  type DynamicFeeState,
} from "./math/index.js";
export {
  effectiveFeeBps,
  quoteSwap,
  type EffectiveFee,
  type SwapQuote,
} from "./quote.js";
export {
  INSTRUCTION_DISCRIMINATORS,
  type InstructionName,
  Writer,
  ixData,
} from "./instructions/encode.js";
export {
  type Built,
  type CreateConfigParams,
  buildCreateConfig,
  buildInitializePool,
  buildCreatePosition,
  buildAddLiquidity,
  buildRemoveLiquidity,
  buildRemoveAllLiquidity,
  buildSwap,
  buildClaimPositionFee,
  buildClaimProtocolFee,
  buildClaimPartnerFee,
  buildSetPositionCompounding,
  buildClosePosition,
} from "./instructions/builders.js";
export { mergeBuilt, buildTransaction, buildTransactionFrom } from "./tx.js";
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
