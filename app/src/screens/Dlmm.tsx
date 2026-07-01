import { useEffect, useMemo, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import { Keypair } from "@solana/web3.js";
import { dlmm } from "@zenith/sdk";
import { ArrowDownUp, Layers, Loader2, TrendingUp } from "lucide-react";

import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  DLMM_MARKET,
  binPriceNumber,
  binReserves,
  dlmmOwedFees,
  executeDlmmClaim,
  executeDlmmClose,
  executeDlmmProvide,
  executeDlmmSwap,
  useDlmmPair,
  useDlmmPositions,
  type OwnedDlmmPosition,
} from "@/lib/dlmm";
import { formatAmount, parseAmount } from "@/lib/tokens";
import { useToast } from "@/lib/toast";
import { cn } from "@/lib/utils";

const LADDER_RADIUS = 8;
const STRATEGIES = [
  { id: 0, label: "Spot", hint: "uniform across the range" },
  { id: 1, label: "Curve", hint: "concentrated at the active bin" },
  { id: 2, label: "BidAsk", hint: "concentrated at the edges" },
];

export function Dlmm() {
  const { pair, binArrays, loading, error, refetch } = useDlmmPair();
  const [view, setView] = useState<"swap" | "liquidity">("swap");

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

      <div className="mb-4 inline-flex rounded-full border border-line/60 bg-panel/50 p-1">
        {(["swap", "liquidity"] as const).map((v) => (
          <button
            key={v}
            onClick={() => setView(v)}
            className={cn(
              "rounded-full px-4 py-1.5 text-sm font-medium capitalize transition-colors",
              view === v ? "bg-panel-2 text-starlight shadow-sm" : "text-dusk hover:text-starlight",
            )}
          >
            {v}
          </button>
        ))}
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
        <>
          <div className="grid gap-4 lg:grid-cols-[1.25fr_1fr]">
            <BinLadder pair={pair} binArrays={binArrays} />
            {view === "swap" ? (
              <SwapPanel pair={pair} binArrays={binArrays} onDone={refetch} />
            ) : (
              <ProvidePanel pair={pair} onDone={refetch} />
            )}
          </div>
          {view === "liquidity" && (
            <PositionsList binArrays={binArrays} onDone={refetch} />
          )}
        </>
      )}
    </div>
  );
}

function BinLadder({ pair, binArrays }: { pair: dlmm.LbPair; binArrays: dlmm.BinArray[] }) {
  const activeBin = pair.activeBinId;
  const bins = useMemo(() => {
    const rows = [];
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
              <div className="flex flex-1 justify-end">
                <div className="h-4 rounded-l bg-meridian/70" style={{ width: pct(b.y) }} />
              </div>
              <div className="flex flex-1 justify-start">
                <div className="h-4 rounded-r bg-star/70" style={{ width: pct(b.x) }} />
              </div>
            </div>
          </div>
        ))}
      </div>
      <p className="mt-3 text-xs text-dusk">
        Each bin is one fixed price — trades inside a bin have zero slippage. Bins below the active
        price hold {DLMM_MARKET.tokenY.symbol}; bins above hold {DLMM_MARKET.tokenX.symbol}.
      </p>
    </Card>
  );
}

function SwapPanel({
  pair,
  binArrays,
  onDone,
}: {
  pair: dlmm.LbPair;
  binArrays: dlmm.BinArray[];
  onDone: () => void;
}) {
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { notifyTx } = useToast();

  const [xToY, setXToY] = useState(true);
  const [amountStr, setAmountStr] = useState("1");
  const [slot, setSlot] = useState<bigint | null>(null);
  const [busy, setBusy] = useState(false);

  const inputTok = xToY ? DLMM_MARKET.tokenX : DLMM_MARKET.tokenY;
  const outputTok = xToY ? DLMM_MARKET.tokenY : DLMM_MARKET.tokenX;

  useEffect(() => {
    let on = true;
    connection.getSlot().then((s) => on && setSlot(BigInt(s))).catch(() => {});
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
    if (!connected || !publicKey) return setVisible(true);
    if (!quote?.q || !amount) return;
    const q = quote.q;
    setBusy(true);
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
    if (sig) onDone();
    setBusy(false);
  }

  return (
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
          {quote?.q ? formatAmount(quote.q.amountOut, outputTok.decimals) : "0.0"}
        </div>
      </div>
      {quote?.err && <p className="text-xs text-rose-300">{quote.err}</p>}
      {quote?.q && (
        <dl className="space-y-1 rounded-xl bg-night/30 p-3 text-xs text-dusk">
          <Row k="Fee" v={`${formatAmount(quote.q.fee, inputTok.decimals)} ${inputTok.symbol} (${(quote.q.feeBps / 100).toFixed(2)}%)`} />
          <Row k="Bins crossed" v={String(quote.q.binsCrossed)} />
          <Row k="Active bin after" v={String(quote.q.endBinId)} />
          <Row k="Min received" v={`${formatAmount(quote.q.otherAmountThreshold, outputTok.decimals)} ${outputTok.symbol}`} />
        </dl>
      )}
      <Button onClick={onSwap} disabled={busy || (connected && (!quote?.q || !amount))} className="mt-1">
        {!connected ? "Connect wallet" : busy ? "Swapping…" : "Swap"}
      </Button>
    </Card>
  );
}

function ProvidePanel({ pair, onDone }: { pair: dlmm.LbPair; onDone: () => void }) {
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { notifyTx } = useToast();

  const active = pair.activeBinId;
  const [strategy, setStrategy] = useState(0);
  const [lowerStr, setLowerStr] = useState(String(active));
  const [widthStr, setWidthStr] = useState("5");
  const [xStr, setXStr] = useState("10");
  const [yStr, setYStr] = useState("10");
  const [busy, setBusy] = useState(false);

  const lower = parseInt(lowerStr, 10);
  const width = parseInt(widthStr, 10);
  const amountX = parseAmount(xStr, DLMM_MARKET.tokenX.decimals) ?? 0n;
  const amountY = parseAmount(yStr, DLMM_MARKET.tokenY.decimals) ?? 0n;

  const validation = useMemo(() => {
    if (!Number.isFinite(lower) || !Number.isFinite(width)) return "Enter a bin range";
    if (width < 1 || width > 70) return "Width must be 1–70";
    const upper = lower + width - 1;
    const ai = Math.floor(lower / dlmm.BINS_PER_ARRAY);
    if (ai !== Math.floor(upper / dlmm.BINS_PER_ARRAY)) return "Range must stay in one bin array";
    if (ai !== 0 && ai !== -1) return "Only bin arrays 0 and -1 exist on this pair";
    if (lower > active && amountY > 0n) return `Range is above the active bin — provide only ${DLMM_MARKET.tokenX.symbol}`;
    if (upper < active && amountX > 0n) return `Range is below the active bin — provide only ${DLMM_MARKET.tokenY.symbol}`;
    if (amountX <= 0n && amountY <= 0n) return "Enter an amount";
    return null;
  }, [lower, width, amountX, amountY, active]);

  async function onProvide() {
    if (!connected || !publicKey) return setVisible(true);
    if (validation) return;
    setBusy(true);
    const sig = await notifyTx(
      () =>
        executeDlmmProvide(
          { connection, sendTransaction, owner: publicKey },
          {
            positionBase: Keypair.generate(),
            lowerBin: lower,
            width,
            amountX,
            amountY,
            strategy,
            activeBin: active,
          },
        ),
      { pending: "Opening position…", success: "Liquidity added" },
    );
    if (sig) onDone();
    setBusy(false);
  }

  return (
    <Card className="flex flex-col gap-3 p-5">
      <span className="text-sm text-dusk">Add liquidity</span>

      <div>
        <div className="mb-1.5 text-xs text-dusk">Strategy</div>
        <div className="grid grid-cols-3 gap-2">
          {STRATEGIES.map((s) => (
            <button
              key={s.id}
              onClick={() => setStrategy(s.id)}
              className={cn(
                "rounded-xl border px-2 py-2 text-center transition-colors",
                strategy === s.id ? "border-meridian/60 bg-meridian/10 text-meridian" : "border-line text-dusk hover:text-starlight",
              )}
            >
              <div className="text-sm font-medium">{s.label}</div>
            </button>
          ))}
        </div>
        <p className="mt-1.5 text-[11px] text-dusk">{STRATEGIES[strategy].hint}</p>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <Field label="Lower bin" value={lowerStr} onChange={setLowerStr} />
        <Field label="Width (bins)" value={widthStr} onChange={setWidthStr} />
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Field label={`${DLMM_MARKET.tokenX.symbol} amount`} value={xStr} onChange={setXStr} />
        <Field label={`${DLMM_MARKET.tokenY.symbol} amount`} value={yStr} onChange={setYStr} />
      </div>

      <p className="text-[11px] text-dusk">
        Range {Number.isFinite(lower) && Number.isFinite(width) ? `[${lower}, ${lower + width - 1}]` : "—"} · array{" "}
        {Number.isFinite(lower) ? Math.floor(lower / dlmm.BINS_PER_ARRAY) : "—"}
      </p>

      {validation && connected && <p className="text-xs text-amber-300">{validation}</p>}

      <Button onClick={onProvide} disabled={busy || (connected && !!validation)}>
        {!connected ? "Connect wallet" : busy ? "Submitting…" : "Add liquidity"}
      </Button>
    </Card>
  );
}

function PositionsList({ binArrays, onDone }: { binArrays: dlmm.BinArray[]; onDone: () => void }) {
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { notifyTx } = useToast();
  const { positions, loading, refetch } = useDlmmPositions();
  const [busy, setBusy] = useState(false);

  const base = () => ({ connection, sendTransaction, owner: publicKey! });
  const done = () => {
    refetch();
    onDone();
  };

  async function onClaim(p: OwnedDlmmPosition) {
    if (!publicKey) return;
    setBusy(true);
    const sig = await notifyTx(() => executeDlmmClaim(base(), p), {
      pending: "Claiming fees…",
      success: "Fees claimed",
    });
    if (sig) done();
    setBusy(false);
  }
  async function onClose(p: OwnedDlmmPosition) {
    if (!publicKey) return;
    setBusy(true);
    const sig = await notifyTx(() => executeDlmmClose(base(), p), {
      pending: "Closing position…",
      success: "Position closed",
    });
    if (sig) done();
    setBusy(false);
  }

  if (!connected) return null;

  return (
    <div className="mt-4">
      <h2 className="mb-3 font-display text-2xl text-starlight">Your positions</h2>
      {loading ? (
        <Card className="flex items-center justify-center p-8 text-dusk">
          <Loader2 className="h-5 w-5 animate-spin" />
        </Card>
      ) : positions.length === 0 ? (
        <Card className="p-8 text-center text-sm text-dusk">
          No DLMM positions yet. Add liquidity above to open one.
        </Card>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2">
          {positions.map((p, i) => (
            <DlmmPositionCard
              key={p.address.toBase58()}
              index={i + 1}
              owned={p}
              binArrays={binArrays}
              pending={busy}
              onClaim={onClaim}
              onClose={onClose}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function DlmmPositionCard({
  index,
  owned,
  binArrays,
  pending,
  onClaim,
  onClose,
}: {
  index: number;
  owned: OwnedDlmmPosition;
  binArrays: dlmm.BinArray[];
  pending: boolean;
  onClaim: (p: OwnedDlmmPosition) => void;
  onClose: (p: OwnedDlmmPosition) => void;
}) {
  const { lowerBinId, upperBinId } = owned.position;
  const owed = dlmmOwedFees(owned.position, binArrays);
  const hasFees = owed.x > 0n || owed.y > 0n;
  const totalShares = owned.position.liquidityShares.reduce((s, v) => s + v, 0n);

  return (
    <Card className="p-5">
      <div className="mb-3 flex items-center justify-between">
        <span className="font-display text-xl text-starlight">Position {index}</span>
        <span className="font-mono text-[11px] text-dusk tnum">
          bins [{lowerBinId}, {upperBinId}]
        </span>
      </div>
      <div className="grid grid-cols-2 gap-3 border-t border-line/40 pt-3">
        <Holding label="Price range" value={`${binPriceNumber(DLMM_MARKET.binStep, lowerBinId).toFixed(4)}–${binPriceNumber(DLMM_MARKET.binStep, upperBinId).toFixed(4)}`} />
        <Holding label="Shares" value={totalShares > 0n ? formatAmount(totalShares, 0, 0) : "0"} />
        <Holding label={`Fees ${DLMM_MARKET.tokenX.symbol}`} value={formatAmount(owed.x, DLMM_MARKET.tokenX.decimals)} accent />
        <Holding label={`Fees ${DLMM_MARKET.tokenY.symbol}`} value={formatAmount(owed.y, DLMM_MARKET.tokenY.decimals)} accent />
      </div>
      <div className="mt-4 flex gap-2">
        <Button variant="gold" size="sm" className="flex-1" disabled={pending || !hasFees} onClick={() => onClaim(owned)}>
          Claim fees
        </Button>
        <Button variant="outline" size="sm" className="flex-1" disabled={pending} onClick={() => onClose(owned)}>
          Close
        </Button>
      </div>
    </Card>
  );
}

function Field({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="rounded-xl border border-line bg-night/40 p-2.5">
      <span className="text-[11px] text-dusk">{label}</span>
      <input
        inputMode="numeric"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full bg-transparent font-mono text-lg text-starlight outline-none tnum"
      />
    </label>
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

function Holding({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div>
      <div className="text-[11px] text-dusk">{label}</div>
      <div className={cn("mt-0.5 font-mono text-sm tnum", accent ? "text-star" : "text-starlight")}>{value}</div>
    </div>
  );
}
