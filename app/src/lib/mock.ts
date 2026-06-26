// Placeholder data so the UI can be designed and reviewed before the SDK is
// wired in. Shapes mirror what the @zenith/sdk decoders return, so swapping
// real fetches in later is a drop-in.

export interface TokenMeta {
  symbol: string;
  name: string;
  decimals: number;
}

export const TOKENS: Record<string, TokenMeta> = {
  SOL: { symbol: "SOL", name: "Solana", decimals: 9 },
  USDC: { symbol: "USDC", name: "USD Coin", decimals: 6 },
  USDT: { symbol: "USDT", name: "Tether", decimals: 6 },
  ZEN: { symbol: "ZEN", name: "Zenith", decimals: 9 },
};

export interface PoolRow {
  pair: string;
  base: string;
  quote: string;
  price: number;
  liquidityUsd: number;
  volume24hUsd: number;
  feeBps: number;
  // Where the active price sits inside the depth curve, 0..1 (0.5 = centered).
  activeAt: number;
}

export const POOLS: PoolRow[] = [
  { pair: "SOL/USDC", base: "SOL", quote: "USDC", price: 94.86, liquidityUsd: 1_240_000, volume24hUsd: 842_000, feeBps: 30, activeAt: 0.52 },
  { pair: "ZEN/SOL", base: "ZEN", quote: "SOL", price: 0.0142, liquidityUsd: 410_000, volume24hUsd: 188_000, feeBps: 100, activeAt: 0.38 },
  { pair: "USDC/USDT", base: "USDC", quote: "USDT", price: 1.0001, liquidityUsd: 2_010_000, volume24hUsd: 1_120_000, feeBps: 5, activeAt: 0.5 },
  { pair: "ZEN/USDC", base: "ZEN", quote: "USDC", price: 1.35, liquidityUsd: 96_000, volume24hUsd: 41_000, feeBps: 100, activeAt: 0.66 },
];

export interface PositionRow {
  pair: string;
  base: string;
  quote: string;
  inRange: boolean;
  // Active price position inside the position's own range, 0..1.
  activeAt: number;
  liquidityUsd: number;
  feesEarnedUsd: number;
}

export const POSITIONS: PositionRow[] = [
  { pair: "SOL/USDC", base: "SOL", quote: "USDC", inRange: true, activeAt: 0.46, liquidityUsd: 12_400, feesEarnedUsd: 84.2 },
  { pair: "ZEN/SOL", base: "ZEN", quote: "SOL", inRange: false, activeAt: 0.92, liquidityUsd: 3_100, feesEarnedUsd: 12.05 },
  { pair: "USDC/USDT", base: "USDC", quote: "USDT", inRange: true, activeAt: 0.51, liquidityUsd: 48_900, feesEarnedUsd: 6.41 },
];
