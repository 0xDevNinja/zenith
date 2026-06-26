import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRightLeft, Loader2, Plus, Search } from "lucide-react";
import { useConnection } from "@solana/wallet-adapter-react";
import { fetchConfig, type Config, type Pool } from "@zenith/sdk";
import { Card, Eyebrow } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { LiquidityArc } from "@/components/LiquidityArc";
import { usePools, type DiscoveredPool } from "@/lib/usePools";
import { useZenith } from "@/lib/sdk";
import { humanPrice, isMarketPool, resolveSymbol } from "@/lib/market";
import { explorerAddress, explorerTx } from "@/lib/config";
import { formatAmount } from "@/lib/tokens";
import type { Screen } from "@/components/Nav";
import { cn } from "@/lib/utils";

function bandActiveAt(pool: Pool): number {
  const span = pool.sqrtMaxPrice - pool.sqrtMinPrice;
  if (span <= 0n) return 0.5;
  return Math.min(0.95, Math.max(0.05, Number(pool.sqrtPrice - pool.sqrtMinPrice) / Number(span)));
}

export function Pools({ onNavigate }: { onNavigate: (s: Screen) => void }) {
  const { pools, loading, error } = usePools();
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<DiscoveredPool | null>(null);

  if (selected) {
    return <PoolDetail entry={selected} onBack={() => setSelected(null)} onNavigate={onNavigate} />;
  }

  const filtered = pools.filter((p) => {
    if (!query.trim()) return true;
    const q = query.toLowerCase();
    return (
      resolveSymbol(p.pool.tokenAMint).toLowerCase().includes(q) ||
      resolveSymbol(p.pool.tokenBMint).toLowerCase().includes(q) ||
      p.pool.tokenAMint.toBase58().toLowerCase().includes(q) ||
      p.pool.tokenBMint.toBase58().toLowerCase().includes(q)
    );
  });

  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-10 animate-rise">
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <Eyebrow>Star catalog</Eyebrow>
          <h1 className="mt-1 font-display text-4xl text-starlight">Pools</h1>
        </div>
        <label className="flex h-10 items-center gap-2 rounded-xl border border-line bg-panel/60 px-3 text-sm text-dusk focus-within:border-meridian/40">
          <Search className="h-4 w-4" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search token"
            className="w-32 bg-transparent text-starlight outline-none placeholder:text-dusk/60"
          />
        </label>
      </div>

      <Card className="overflow-hidden">
        <div className="grid grid-cols-[1.5fr_1fr_1fr_0.7fr_1.1fr] items-center gap-4 border-b border-line/50 px-5 py-3 text-[11px] uppercase tracking-[0.16em] text-dusk">
          <span>Pair</span>
          <span className="text-right">Price</span>
          <span className="text-right">Liquidity</span>
          <span className="text-right">Fee</span>
          <span className="text-right">Depth</span>
        </div>

        {loading ? (
          <div className="flex items-center justify-center py-12 text-dusk">
            <Loader2 className="h-5 w-5 animate-spin" />
          </div>
        ) : error ? (
          <div className="py-12 text-center text-sm text-star">{error}</div>
        ) : filtered.length === 0 ? (
          <div className="py-12 text-center text-sm text-dusk">No pools match.</div>
        ) : (
          filtered.map((p) => (
            <button
              key={p.address.toBase58()}
              onClick={() => setSelected(p)}
              className="grid w-full grid-cols-[1.5fr_1fr_1fr_0.7fr_1.1fr] items-center gap-4 border-b border-line/30 px-5 py-4 text-left transition-colors last:border-0 hover:bg-panel-2/40"
            >
              <span className="flex items-center gap-3">
                <PairGlyph a={resolveSymbol(p.pool.tokenAMint)} b={resolveSymbol(p.pool.tokenBMint)} />
                <span className="flex flex-col">
                  <span className="font-medium text-starlight">
                    {resolveSymbol(p.pool.tokenAMint)} / {resolveSymbol(p.pool.tokenBMint)}
                  </span>
                  {isMarketPool(p.address) && <span className="text-[10px] font-medium uppercase tracking-wide text-meridian">Active market</span>}
                </span>
              </span>
              <span className="text-right font-mono tnum text-starlight">
                {humanPrice(p.pool.sqrtPrice, p.pool.tokenAMint, p.pool.tokenBMint).toFixed(4)}
              </span>
              <span className="text-right font-mono tnum text-dusk">{formatAmount(p.pool.liquidity, 0, 0)}</span>
              <span className="text-right font-mono tnum text-star">{(p.pool.baseFeeBps / 100).toFixed(2)}%</span>
              <span className="ml-auto h-9 w-28">
                <LiquidityArc activeAt={bandActiveAt(p.pool)} width={120} height={40} sigma={0.2} bare animate={false} className="h-full w-full" />
              </span>
            </button>
          ))
        )}
      </Card>
    </div>
  );
}

function PoolDetail({
  entry,
  onBack,
  onNavigate,
}: {
  entry: DiscoveredPool;
  onBack: () => void;
  onNavigate: (s: Screen) => void;
}) {
  const { pool, address } = entry;
  const { connection } = useConnection();
  const { zenith } = useZenith();
  const [config, setConfig] = useState<Config | null>(null);
  const [reserves, setReserves] = useState<{ a: string; b: string } | null>(null);
  const [activity, setActivity] = useState<{ sig: string; slot: number; err: boolean }[]>([]);
  const tradable = isMarketPool(address);
  const symA = resolveSymbol(pool.tokenAMint);
  const symB = resolveSymbol(pool.tokenBMint);

  useEffect(() => {
    let active = true;
    (async () => {
      const [cfg, ra, rb, sigs] = await Promise.all([
        fetchConfig(zenith, pool.config).catch(() => null),
        connection.getTokenAccountBalance(pool.tokenAVault).catch(() => null),
        connection.getTokenAccountBalance(pool.tokenBVault).catch(() => null),
        connection.getSignaturesForAddress(address, { limit: 6 }).catch(() => []),
      ]);
      if (!active) return;
      setConfig(cfg);
      setReserves(ra && rb ? { a: ra.value.uiAmountString ?? "0", b: rb.value.uiAmountString ?? "0" } : null);
      setActivity(sigs.map((s) => ({ sig: s.signature, slot: s.slot, err: s.err !== null })));
    })();
    return () => {
      active = false;
    };
  }, [connection, zenith, address, pool]);

  return (
    <div className="mx-auto max-w-4xl px-5 pb-24 pt-10 animate-rise">
      <button onClick={onBack} className="mb-5 flex items-center gap-1.5 text-sm text-dusk transition-colors hover:text-starlight">
        <ArrowLeft className="h-4 w-4" /> All pools
      </button>

      <div className="mb-6 flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <PairGlyph a={symA} b={symB} />
          <div>
            <h1 className="font-display text-3xl leading-none text-starlight">{symA} / {symB}</h1>
            <a href={explorerAddress(address.toBase58())} target="_blank" rel="noreferrer" className="font-mono text-xs text-dusk hover:text-meridian">
              {address.toBase58().slice(0, 8)}… ↗
            </a>
          </div>
        </div>
        <div className="flex gap-2">
          <Button size="md" disabled={!tradable} onClick={() => onNavigate("swap")}>
            <ArrowRightLeft className="h-4 w-4" /> Swap
          </Button>
          <Button size="md" variant="outline" disabled={!tradable} onClick={() => onNavigate("positions")}>
            <Plus className="h-4 w-4" /> Add liquidity
          </Button>
        </div>
      </div>
      {!tradable && (
        <p className="mb-4 text-xs text-dusk">Trading and LP in this app run on the default market; this pool is read-only here.</p>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <Card className="p-5">
          <div className="mb-3 font-display text-xl text-starlight">Market</div>
          <Detail label="Current price" value={humanPrice(pool.sqrtPrice, pool.tokenAMint, pool.tokenBMint).toFixed(6)} accent />
          <Detail label="Liquidity (L)" value={formatAmount(pool.liquidity, 0, 0)} />
          <Detail label={`Reserve ${symA}`} value={reserves ? reserves.a : "…"} />
          <Detail label={`Reserve ${symB}`} value={reserves ? reserves.b : "…"} />
          <Detail label="Status" value={poolStatus(pool.status)} />
        </Card>

        <Card className="p-5">
          <div className="mb-3 font-display text-xl text-starlight">Fee configuration</div>
          <Detail label="Base fee" value={`${(pool.baseFeeBps / 100).toFixed(2)}%`} accent />
          {config ? (
            <>
              <Detail label="Protocol share" value={`${(config.protocolFeeBps / 100).toFixed(2)}%`} />
              <Detail label="Partner share" value={`${(config.partnerFeeBps / 100).toFixed(2)}%`} />
              <Detail label="Max dynamic fee" value={`${(config.maxDynamicFeeBps / 100).toFixed(2)}%`} />
              <Detail label="Config index" value={String(config.index)} />
            </>
          ) : (
            <div className="py-2 text-sm text-dusk">Config unavailable</div>
          )}
        </Card>
      </div>

      <Card className="mt-4 p-5">
        <div className="mb-3 font-display text-xl text-starlight">Recent activity</div>
        {activity.length === 0 ? (
          <div className="py-4 text-center text-sm text-dusk">No recent transactions.</div>
        ) : (
          <div className="divide-y divide-line/30">
            {activity.map((a) => (
              <a key={a.sig} href={explorerTx(a.sig)} target="_blank" rel="noreferrer" className="flex items-center justify-between py-2.5 text-sm transition-colors hover:text-meridian">
                <span className="font-mono text-dusk">{a.sig.slice(0, 10)}…{a.sig.slice(-8)}</span>
                <span className="flex items-center gap-3">
                  <span className="font-mono text-xs text-dusk tnum">slot {a.slot}</span>
                  <span className={cn("text-xs", a.err ? "text-star" : "text-meridian")}>{a.err ? "failed" : "ok"}</span>
                </span>
              </a>
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}

function poolStatus(status: number): string {
  return ["Uninitialized", "Active", "Disabled"][status] ?? `#${status}`;
}

function Detail({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className="flex items-center justify-between border-t border-line/30 py-2.5 first:border-0">
      <span className="text-sm text-dusk">{label}</span>
      <span className={cn("font-mono tnum", accent ? "text-meridian" : "text-starlight")}>{value}</span>
    </div>
  );
}

function PairGlyph({ a, b }: { a: string; b: string }) {
  return (
    <span className="flex">
      <span className="grid h-7 w-7 place-items-center rounded-full border border-night bg-gradient-to-br from-star/40 to-star/10 font-mono text-[10px] font-semibold text-starlight">
        {a.slice(0, 2)}
      </span>
      <span className="-ml-2 grid h-7 w-7 place-items-center rounded-full border border-night bg-gradient-to-br from-meridian/40 to-meridian/10 font-mono text-[10px] font-semibold text-starlight">
        {b.slice(0, 2)}
      </span>
    </span>
  );
}
