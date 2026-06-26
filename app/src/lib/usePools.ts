import { useCallback, useEffect, useState } from "react";
import { PublicKey } from "@solana/web3.js";
import { useConnection } from "@solana/wallet-adapter-react";
import { decodePool, type Pool } from "@zenith/sdk";
import { PROGRAM_ID } from "./config";

export interface DiscoveredPool {
  address: PublicKey;
  pool: Pool;
}

// Pool accounts are a fixed 440 bytes, distinct from Config (196) and Position
// (233), so a dataSize filter cleanly enumerates every pool the program owns.
const POOL_ACCOUNT_SIZE = 440;

interface PoolsState {
  pools: DiscoveredPool[];
  loading: boolean;
  error: string | null;
  refetch: () => void;
}

export function usePools(): PoolsState {
  const { connection } = useConnection();
  const [pools, setPools] = useState<DiscoveredPool[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);

  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    let active = true;
    setLoading(true);
    (async () => {
      try {
        const accounts = await connection.getProgramAccounts(PROGRAM_ID, {
          filters: [{ dataSize: POOL_ACCOUNT_SIZE }],
        });
        const decoded = accounts
          .map((a) => {
            try {
              return { address: a.pubkey, pool: decodePool(a.account.data) } satisfies DiscoveredPool;
            } catch {
              return null;
            }
          })
          .filter((p): p is DiscoveredPool => p !== null)
          .sort((a, b) => (b.pool.liquidity > a.pool.liquidity ? 1 : -1));
        if (!active) return;
        setPools(decoded);
        setError(null);
      } catch (e) {
        if (active) setError(e instanceof Error ? e.message : "Failed to load pools");
      } finally {
        if (active) setLoading(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [connection, nonce]);

  return { pools, loading, error, refetch };
}
