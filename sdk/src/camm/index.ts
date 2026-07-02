//! zenith-camm SDK surface — PDA derivation, byte-exact account decoding, the
//! bit-exact math port, swap quoting, and transaction builders for the
//! full-range constant-product engine. Exposed under the `camm` namespace from
//! the package root (`import { camm } from "@zenith/sdk"`).

export {
  ZENITH_CAMM_PROGRAM_ID,
  CAMM_SEEDS,
  BPS_DENOMINATOR,
  MINIMUM_LIQUIDITY,
  YIELD_SCALE,
  LP_MINT_DECIMALS,
} from "./constants.js";

export {
  type Pda,
  sortMints,
  poolPda,
  poolAuthorityPda,
  reservePda,
  lpMintPda,
  lockedLpPda,
  yieldSourcePda,
} from "./pda.js";

export {
  CAMM_DISCRIMINATORS,
  CAMM_ACCOUNT_LEN,
  PoolStatus,
  CammTokenFlavor,
  type Pool,
  decodePool,
} from "./accounts.js";

export { fetchPool } from "./fetch.js";

export {
  outGivenIn,
  inGivenOut,
  initialShares,
  sharesFromDeposit,
  tokensForShares,
  matchingAmount,
  feeOnInput,
  grossInputForNet,
  splitProtocolFee,
  accruedYield,
  deployable,
  Direction,
  SwapMode,
  CammQuoteError,
  type CammQuoteErrorCode,
  type SwapResult,
  computeSwap,
} from "./math.js";

export {
  type CammSwapQuote,
  type QuoteSwapParams,
  quoteSwap,
} from "./quote.js";

export {
  CAMM_INSTRUCTION_DISCRIMINATORS,
  type InitializePoolParams,
  type AddLiquidityParams,
  type RemoveLiquidityParams,
  type SwapParams,
  type InitializeYieldParams,
  type YieldAccrueParams,
  buildInitializePool,
  buildAddLiquidity,
  buildRemoveLiquidity,
  buildSwap,
  buildInitializeYield,
  buildHarvestYield,
  buildRebalanceToVault,
} from "./instructions.js";
