import { useEffect, useMemo, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import { dlmm } from "@zenith/sdk";
import { ArrowDownUp, Layers, TrendingUp } from "lucide-react";

import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  DLMM_MARKET,
  binPriceNumber,
  binReserves,
  executeDlmmSwap,
  useDlmmPair,
} from "@/lib/dlmm";
import { formatAmount, parseAmount } from "@/lib/tokens";
import { useToast } from "@/lib/toast";
import { cn } from "@/lib/utils";

const LADDER_RADIUS = 8; // bins shown on each side of the active bin

export function Dlmm() {
  const { pair, binArrays, loading, error, refetch } = useDlmmPair();
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { notifyTx } = useToast();

  const [xToY, setXToY] = useState(true); // sell tBIN for tUSD
  const [amountStr, setAmountStr] = useState("1");
  const [slot, setSlot] = useState<bigint | null>(null);
  const [busy, setBusy] = useState(false);

  const inputTok = xToY ? DLMM_MARKET.tokenX : DLMM_MARKET.tokenY;
  const outputTok = xToY ? DLMM_MARKET.tokenY : DLMM_MARKET.tokenX;

  useEffect(() => {
    let on = true;
    connection
      .getSlot()
      .then((s) => on && setSlot(BigInt(s)))
      .catch(() => {});
    return () => {
      on = false;
    };
  }, [connection, pair]);

  const amount = useMemo(() => parseAmount(amountStr, inputTok.decimals), [amountStr, inputTok]);

  const quote = useMemo(() => {
    if (!pair || binArrays.length === 0 || slot === null || !amount || amount <= 0n) return null;
    try {
      return {
        q: dlmm.quoteSwap({
          pair,
          binArrays,
          slot,
          direction: xToY ? dlmm.Direction.XtoY : dlmm.Direction.YtoX,
          mode: dlmm.SwapMode.ExactIn,
          amount,
        }),
        err: null as string | null,
      };
    } catch (e) {
      return { q: null, err: e instanceof Error ? e.message : String(e) };
    }
  }, [pair, binArrays, slot, amount, xToY]);

  async function onSwap() {
    if (!connected || !publicKey) {
      setVisible(true);
      return;
    }
    if (!quote?.q || !amount) return;
    setBusy(true);
    const q = quote.q;
    const sig = await notifyTx(
      () =>
        executeDlmmSwap({
          connection,
          sendTransaction,
          owner: publicKey,
          direction: xToY ? dlmm.Direction.XtoY : dlmm.Direction.YtoX,
          mode: dlmm.SwapMode.ExactIn,
          amount,
          otherAmountThreshold: q.otherAmountThreshold,
        }),
      { pending: "Swapping…", success: "Swap confirmed" },
    );
    if (sig) refetch();
    setBusy(false);
  }

  const activeBin = pair?.activeBinId ?? 0;
  const bins = useMemo(() => {
    if (!pair) return [];
    const rows = [];
    // High price at top: iterate from active+radius down to active-radius.
    for (let id = activeBin + LADDER_RADIUS; id >= activeBin - LADDER_RADIUS; id--) {
      const r = binReserves(binArrays, id);
      rows.push({ id, price: binPriceNumber(pair.binStep, id), x: r.x, y: r.y });
    }
    return rows;
  }, [pair, binArrays, activeBin]);

  const maxReserve = useMemo(() => {
    let m = 1n;
    for (const b of bins) {
      if (b.x > m) m = b.x;
      if (b.y > m) m = b.y;
    }
    return m;
  }, [bins]);

  const pct = (v: bigint) => `${Number((v * 1000n) / maxReserve) / 10}%`;

  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-8 sm:pt-12 animate-rise">
      <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
        <div>
          <span className="flex items-center gap-2 text-sm text-dusk">
            <Layers className="h-4 w-4 text-meridian" /> Liquidity Book
          </span>
          <h1 className="mt-1 font-display text-4xl leading-none text-starlight">
            {DLMM_MARKET.tokenX.symbol} / {DLMM_MARKET.tokenY.symbol}
          </h1>
        </div>
        {pair && (
          <div className="flex items-center gap-6 text-sm">
            <Stat label="Active bin" value={String(pair.activeBinId)} />
            <Stat label="Price" value={binPriceNumber(pair.binStep, pair.activeBinId).toFixed(4)} />
            <Stat label="Bin step" value={`${pair.binStep} bps`} />
            <Stat label="Base fee" value={`${(pair.baseFeeBps / 100).toFixed(2)}%`} />
          </div>
        )}
      </div>

      {loading && <Card className="p-8 text-center text-dusk">Loading the book…</Card>}
      {error && (
        <Card className="p-8 text-center text-rose-300">
          {error}
          <div className="mt-3">
            <Button variant="outline" size="sm" onClick={refetch}>
              Retry
            </Button>
          </div>
        </Card>
      )}

      {pair && !error && (
        <div className="grid gap-4 lg:grid-cols-[1.25fr_1fr]">
          {/* bin ladder */}
          <Card className="flex flex-col p-5">
            <div className="mb-3 flex items-center justify-between">
              <span className="flex items-center gap-2 text-sm text-dusk">
                <TrendingUp className="h-4 w-4 text-star" /> Bins around the active price
              </span>
              <span className="font-mono text-xs text-dusk">
                <span className="text-meridian">■</span> {DLMM_MARKET.tokenY.symbol}{" "}
                <span className="text-star">■</span> {DLMM_MARKET.tokenX.symbol}
              </span>
            </div>
            <div className="flex flex-col gap-1">
              {bins.map((b) => (
                <div
                  key={b.id}
                  className={cn(
                    "flex items-center gap-2 rounded-lg px-2 py-1",
                    b.id === activeBin && "bg-panel-2/60 ring-1 ring-meridian/40",
                  )}
                >
                  <span className="w-14 shrink-0 font-mono text-[11px] text-dusk tnum">
                    {b.price.toFixed(4)}
                  </span>
                  <span className="w-8 shrink-0 font-mono text-[11px] text-dusk tnum">{b.id}</span>
                  <div className="flex h-4 flex-1 items-center gap-px overflow-hidden">
                    {/* Y reserve grows left, X reserve grows right */}
                    <div className="flex flex-1 justify-end">
                      <div
                        className="h-4 rounded-l bg-meridian/70"
                        style={{ width: pct(b.y) }}
                      />
                    </div>
                    <div className="flex flex-1 justify-start">
                      <div className="h-4 rounded-r bg-star/70" style={{ width: pct(b.x) }} />
                    </div>
                  </div>
                </div>
              ))}
            </div>
            <p className="mt-3 text-xs text-dusk">
              Each bin is one fixed price — trades inside a bin have zero slippage. Bins below the
              active price hold {DLMM_MARKET.tokenY.symbol}; bins above hold {DLMM_MARKET.tokenX.symbol}.
            </p>
          </Card>

          {/* swap */}
          <Card className="flex flex-col gap-3 p-5">
            <span className="text-sm text-dusk">Swap</span>

            <label className="rounded-2xl border border-line bg-night/40 p-3">
              <span className="text-xs text-dusk">You pay · {inputTok.symbol}</span>
              <input
                inputMode="decimal"
                value={amountStr}
                onChange={(e) => setAmountStr(e.target.value)}
                className="w-full bg-transparent font-mono text-2xl text-starlight outline-none tnum"
                placeholder="0.0"
              />
            </label>

            <div className="flex justify-center">
              <button
                onClick={() => setXToY((v) => !v)}
                className="rounded-xl border border-line bg-panel-2/60 p-2 text-dusk hover:text-meridian"
                aria-label="flip direction"
              >
                <ArrowDownUp className="h-4 w-4" />
              </button>
            </div>

            <div className="rounded-2xl border border-line bg-night/40 p-3">
              <span className="text-xs text-dusk">You receive · {outputTok.symbol}</span>
              <div className="font-mono text-2xl text-starlight tnum">
                {quote?.q
                  ? formatAmount(quote.q.amountOut, outputTok.decimals)
                  : "0.0"}
              </div>
            </div>

            {quote?.err && <p className="text-xs text-rose-300">{quote.err}</p>}

            {quote?.q && (
              <dl className="space-y-1 rounded-xl bg-night/30 p-3 text-xs text-dusk">
                <Row k="Fee" v={`${formatAmount(quote.q.fee, inputTok.decimals)} ${inputTok.symbol} (${(quote.q.feeBps / 100).toFixed(2)}%)`} />
                <Row k="Bins crossed" v={String(quote.q.binsCrossed)} />
                <Row k="Active bin after" v={String(quote.q.endBinId)} />
                <Row
                  k="Min received"
                  v={`${formatAmount(quote.q.otherAmountThreshold, outputTok.decimals)} ${outputTok.symbol}`}
                />
              </dl>
            )}

            <Button
              onClick={onSwap}
              disabled={busy || (connected && (!quote?.q || !amount))}
              className="mt-1"
            >
              {!connected ? "Connect wallet" : busy ? "Swapping…" : "Swap"}
            </Button>
          </Card>
        </div>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="text-right">
      <div className="text-xs text-dusk">{label}</div>
      <div className="font-mono text-starlight tnum">{value}</div>
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between">
      <dt>{k}</dt>
      <dd className="font-mono text-starlight/90 tnum">{v}</dd>
    </div>
  );
}
