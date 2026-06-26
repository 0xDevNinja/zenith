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
