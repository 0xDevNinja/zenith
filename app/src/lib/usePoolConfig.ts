import { useCallback, useEffect, useState } from "react";
import { fetchConfig, fetchPool, type Config, type Pool } from "@zenith/sdk";
import { useZenith } from "./sdk";
import { MARKET } from "./market";

interface PoolConfigState {
  pool: Pool | null;
  config: Config | null;
  loading: boolean;
  error: string | null;
  /// Re-read both accounts (call after a swap to reflect new reserves/price).
  refetch: () => void;
}

// Refresh cadence — the pool's price/reserves move as others trade, so a quote
// computed off a long-stale pool would mislead (funds are still protected by the
// on-chain min-out threshold, but the displayed numbers would be wrong).
const POLL_MS = 15_000;

// Live Pool + Config for the default market. Re-reads on an interval and on
// demand. Transient poll failures keep the last good state rather than blanking
// the UI; only the very first load surfaces an error.
export function usePoolConfig(): PoolConfigState {
  const { zenith } = useZenith();
  const [pool, setPool] = useState<Pool | null>(null);
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);

  const refetch = useCallback(() => setNonce((n) => n + 1), []);

  useEffect(() => {
    let active = true;

    const load = async (initial: boolean) => {
      try {
        const [p, c] = await Promise.all([
          fetchPool(zenith, MARKET.pool),
          fetchConfig(zenith, MARKET.config),
        ]);
        if (!active) return;
        if (!p) throw new Error("Pool not found on devnet — is it seeded?");
        if (!c) throw new Error("Config not found on devnet");
        setPool(p);
        setConfig(c);
        setError(null);
      } catch (e) {
        if (!active) return;
        // Only fail loudly if we have nothing to show; otherwise keep last good.
        if (initial) setError(e instanceof Error ? e.message : "Failed to load pool");
      } finally {
        if (active && initial) setLoading(false);
      }
    };

    load(true);
    const id = setInterval(() => load(false), POLL_MS);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [zenith, nonce]);

  return { pool, config, loading, error, refetch };
}
