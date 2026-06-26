import { PublicKey } from "@solana/web3.js";
import manifest from "@/devnet.json";

// The default market the app trades, seeded on devnet by scripts/seed-devnet.ts.
export interface TokenInfo {
  mint: PublicKey;
  symbol: string;
  decimals: number;
}

export interface Market {
  programId: PublicKey;
  config: PublicKey;
  pool: PublicKey;
  /// Sorted pool tokens (tokenA = base, tokenB = quote).
  tokenA: TokenInfo;
  tokenB: TokenInfo;
}

function token(mint: string): TokenInfo {
  const meta = (manifest.mints as Record<string, { symbol: string; decimals: number }>)[mint];
  if (!meta) throw new Error(`devnet.json missing metadata for mint ${mint}`);
  return { mint: new PublicKey(mint), symbol: meta.symbol, decimals: meta.decimals };
}

export const MARKET: Market = {
  programId: new PublicKey(manifest.programId),
  config: new PublicKey(manifest.config),
  pool: new PublicKey(manifest.pool),
  tokenA: token(manifest.tokenA),
  tokenB: token(manifest.tokenB),
};

export function tokenByMint(mint: PublicKey): TokenInfo {
  if (mint.equals(MARKET.tokenA.mint)) return MARKET.tokenA;
  if (mint.equals(MARKET.tokenB.mint)) return MARKET.tokenB;
  throw new Error(`mint ${mint.toBase58()} is not in the market`);
}

// Known mints (from the seeded manifest) → metadata; unknown mints fall back to
// a short address so the pool list still renders foreign pairs.
const KNOWN = manifest.mints as Record<string, { symbol: string; decimals: number }>;

export function tokenMeta(mint: PublicKey): { symbol: string; decimals: number } | undefined {
  return KNOWN[mint.toBase58()];
}

export function resolveSymbol(mint: PublicKey): string {
  const b58 = mint.toBase58();
  return KNOWN[b58]?.symbol ?? `${b58.slice(0, 4)}…${b58.slice(-4)}`;
}

export function isMarketPool(pool: PublicKey): boolean {
  return pool.equals(MARKET.pool);
}

// Human price from raw Q64.64 sqrt-price, adjusting for token decimals when both
// are known (raw price is in base units).
export function humanPrice(sqrtPrice: bigint, mintA: PublicKey, mintB: PublicKey): number {
  const raw = Number(sqrtPrice) ** 2 / 2 ** 128;
  const a = tokenMeta(mintA)?.decimals;
  const b = tokenMeta(mintB)?.decimals;
  if (a === undefined || b === undefined) return raw;
  return raw * 10 ** (a - b);
}
