//! zenith-dlmm SDK surface — PDA derivation and byte-exact account decoders for
//! the liquidity-book program. Exposed under the `dlmm` namespace from the
//! package root (`import { dlmm } from "@zenith/sdk"`).

export {
  ZENITH_DLMM_PROGRAM_ID,
  DLMM_SEEDS,
  BINS_PER_ARRAY,
  BINS_PER_POSITION,
  ORACLE_CAPACITY,
  BPS_DENOMINATOR,
} from "./constants.js";

export {
  type Pda,
  sortMints,
  lbPairPda,
  pairAuthorityPda,
  reservePda,
  binArrayPda,
  positionPda,
  oraclePda,
  binArrayIndexOf,
} from "./pda.js";

export {
  DLMM_DISCRIMINATORS,
  DLMM_ACCOUNT_LEN,
  PairStatus,
  DlmmTokenFlavor,
  type LbPair,
  type Bin,
  type BinArray,
  type PositionBinData,
  type Position,
  type Observation,
  type Oracle,
  decodeLbPair,
  decodeBinArray,
  decodePosition,
  decodeOracle,
  binIdAt,
} from "./accounts.js";

export {
  fetchLbPair,
  fetchBinArray,
  fetchDlmmPosition,
  fetchOracle,
} from "./fetch.js";

export {
  Direction,
  SwapMode,
  type BinFill,
  type VariableFeeState,
  binPrice,
  fillExactIn,
  fillExactOut,
  binMoveBps,
  decayedVolatilityReference,
  accumulateVolatility,
  variableFeeBps,
  totalFeeBps,
  splitProtocolFee,
  computeVariableFee,
} from "./math.js";

export {
  DlmmQuoteError,
  type DlmmSwapQuote,
  type QuoteSwapParams,
  quoteSwap,
} from "./quote.js";

export {
  DLMM_INSTRUCTION_DISCRIMINATORS,
  type InitializeLbPairParams,
  type AddLiquidityParams,
  type RemoveLiquidityParams,
  type SwapParams,
  buildInitializeLbPair,
  buildInitializeBinArray,
  buildInitializeOracle,
  buildInitializePosition,
  buildAddLiquidityByStrategy,
  buildRemoveLiquidity,
  buildSwap,
  buildClaimFee,
  buildClaimProtocolFee,
  buildClosePosition,
} from "./instructions.js";
