import { useMemo, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import { camm } from "@zenith/sdk";
import { ArrowDownUp, Coins, Loader2, Sprout } from "lucide-react";

import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import {
  CAMM_MARKET,
  executeCammAdd,
  executeCammHarvest,
  executeCammRemove,
  executeCammSwap,
  pendingYield,
  shareValue,
  useCammLpBalance,
  useCammPool,
  type CammState,
} from "@/lib/camm";
import { formatAmount, parseAmount } from "@/lib/tokens";
import { useToast } from "@/lib/toast";
import { cn } from "@/lib/utils";

const { tokenA, tokenB } = CAMM_MARKET;

// Human price of A in units of B, from the raw reserves and decimals.
function priceOf(pool: camm.Pool): number {
  if (pool.reserveA === 0n) return 0;
  const a = Number(pool.reserveA) / 10 ** tokenA.decimals;
  const b = Number(pool.reserveB) / 10 ** tokenB.decimals;
  return b / a;
}

export function Camm() {
  const state = useCammPool();
  const { pool, loading, error, refetch } = state;
  const [view, setView] = useState<"swap" | "liquidity">("swap");

  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-8 sm:pt-12 animate-rise">
      <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
        <div>
          <span className="flex items-center gap-2 text-sm text-dusk">
            <Coins className="h-4 w-4 text-meridian" /> Constant-Product · Yield
          </span>
          <h1 className="mt-1 font-display text-4xl leading-none text-starlight">
            {tokenA.symbol} / {tokenB.symbol}
          </h1>
        </div>
        {pool && (
          <div className="flex items-center gap-6 text-sm">
            <Stat label="Price" value={priceOf(pool).toFixed(4)} />
            <Stat label="Base fee" value={`${(pool.baseFeeBps / 100).toFixed(2)}%`} />
            <Stat label={`${tokenA.symbol} reserve`} value={formatAmount(pool.reserveA, tokenA.decimals)} />
            <Stat label={`${tokenB.symbol} reserve`} value={formatAmount(pool.reserveB, tokenB.decimals)} />
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

      {loading && <Card className="p-8 text-center text-dusk">Loading the pool…</Card>}
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

      {pool && !error && (
        <div className="grid gap-4 lg:grid-cols-[1fr_1fr]">
          {view === "swap" ? (
            <SwapPanel pool={pool} onDone={refetch} />
          ) : (
            <LiquidityPanel state={state} onDone={refetch} />
          )}
          <YieldCard state={state} onDone={refetch} />
        </div>
      )}
    </div>
  );
}

function SwapPanel({ pool, onDone }: { pool: camm.Pool; onDone: () => void }) {
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { notifyTx } = useToast();

  const [aToB, setAToB] = useState(true);
  const [amountStr, setAmountStr] = useState("1");
  const [busy, setBusy] = useState(false);

  const inputTok = aToB ? tokenA : tokenB;
  const outputTok = aToB ? tokenB : tokenA;
  const amount = useMemo(() => parseAmount(amountStr, inputTok.decimals), [amountStr, inputTok]);

  const quote = useMemo(() => {
    if (!amount || amount <= 0n) return null;
    try {
      return {
        q: camm.quoteSwap({
          pool,
          direction: aToB ? camm.Direction.AtoB : camm.Direction.BtoA,
          mode: camm.SwapMode.ExactIn,
          amount,
        }),
        err: null as string | null,
      };
    } catch (e) {
      return { q: null, err: e instanceof Error ? e.message : String(e) };
    }
  }, [pool, amount, aToB]);

  async function onSwap() {
    if (!connected || !publicKey) return setVisible(true);
    if (!quote?.q || !amount) return;
    setBusy(true);
    const sig = await notifyTx(
      () =>
        executeCammSwap(
          { connection, sendTransaction, owner: publicKey },
          {
            direction: aToB ? camm.Direction.AtoB : camm.Direction.BtoA,
            mode: camm.SwapMode.ExactIn,
            amount,
            otherAmountThreshold: quote.q!.otherAmountThreshold,
          },
        ),
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
          onClick={() => setAToB((v) => !v)}
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
          <Row k="Fee" v={`${formatAmount(quote.q.fee, inputTok.decimals)} ${inputTok.symbol}`} />
          <Row
            k="Min received"
            v={`${formatAmount(quote.q.minAmountOut, outputTok.decimals)} ${outputTok.symbol}`}
          />
        </dl>
      )}
      <Button onClick={onSwap} disabled={busy || (connected && (!quote?.q || !amount))} className="mt-1">
        {!connected ? "Connect wallet" : busy ? "Swapping…" : "Swap"}
      </Button>
    </Card>
  );
}

function LiquidityPanel({ state, onDone }: { state: CammState; onDone: () => void }) {
  const { pool, supply } = state;
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { notifyTx } = useToast();
  const { balance, refetch: refetchLp } = useCammLpBalance();

  const [aStr, setAStr] = useState("100");
  const [bStr, setBStr] = useState("100");
  const [pct, setPct] = useState(50);
  const [busy, setBusy] = useState(false);

  const desiredA = parseAmount(aStr, tokenA.decimals) ?? 0n;
  const desiredB = parseAmount(bStr, tokenB.decimals) ?? 0n;
  const mine = pool ? shareValue(pool, supply, balance) : { a: 0n, b: 0n };
  const shareBps = supply > 0n ? Number((balance * 10_000n) / supply) : 0;

  const done = () => {
    onDone();
    refetchLp();
  };

  async function onAdd() {
    if (!connected || !publicKey) return setVisible(true);
    if (desiredA <= 0n || desiredB <= 0n) return;
    setBusy(true);
    // Slippage: the program trims to the pool ratio; allow the matched side to
    // dip to 99% of what was typed.
    const sig = await notifyTx(
      () =>
        executeCammAdd(
          { connection, sendTransaction, owner: publicKey },
          { desiredA, desiredB, minA: (desiredA * 99n) / 100n, minB: (desiredB * 99n) / 100n },
        ),
      { pending: "Adding liquidity…", success: "Liquidity added" },
    );
    if (sig) done();
    setBusy(false);
  }

  async function onRemove() {
    if (!connected || !publicKey) return setVisible(true);
    const shares = (balance * BigInt(pct)) / 100n;
    if (shares <= 0n || !pool) return;
    setBusy(true);
    const out = shareValue(pool, supply, shares);
    const sig = await notifyTx(
      () =>
        executeCammRemove(
          { connection, sendTransaction, owner: publicKey },
          { shares, minA: (out.a * 99n) / 100n, minB: (out.b * 99n) / 100n },
        ),
      { pending: "Removing liquidity…", success: "Liquidity removed" },
    );
    if (sig) done();
    setBusy(false);
  }

  return (
    <Card className="flex flex-col gap-3 p-5">
      <span className="text-sm text-dusk">Add liquidity</span>
      <label className="rounded-2xl border border-line bg-night/40 p-3">
        <span className="text-xs text-dusk">{tokenA.symbol}</span>
        <input
          inputMode="decimal"
          value={aStr}
          onChange={(e) => setAStr(e.target.value)}
          className="w-full bg-transparent font-mono text-xl text-starlight outline-none tnum"
          placeholder="0.0"
        />
      </label>
      <label className="rounded-2xl border border-line bg-night/40 p-3">
        <span className="text-xs text-dusk">{tokenB.symbol}</span>
        <input
          inputMode="decimal"
          value={bStr}
          onChange={(e) => setBStr(e.target.value)}
          className="w-full bg-transparent font-mono text-xl text-starlight outline-none tnum"
          placeholder="0.0"
        />
      </label>
      <p className="text-xs text-dusk">
        Deposits are trimmed to the pool ratio; the leftover side stays in your wallet.
      </p>
      <Button onClick={onAdd} disabled={busy || (connected && (desiredA <= 0n || desiredB <= 0n))}>
        {!connected ? "Connect wallet" : busy ? "Working…" : "Add liquidity"}
      </Button>

      <div className="mt-2 rounded-xl bg-night/30 p-3 text-xs text-dusk">
        <div className="mb-2 flex items-center justify-between">
          <span>Your position</span>
          <span className="text-starlight">{(shareBps / 100).toFixed(2)}% of pool</span>
        </div>
        <Row k={`${tokenA.symbol}`} v={formatAmount(mine.a, tokenA.decimals)} />
        <Row k={`${tokenB.symbol}`} v={formatAmount(mine.b, tokenB.decimals)} />
        <Row k="LP shares" v={formatAmount(balance, 9)} />
      </div>

      {balance > 0n && (
        <div className="rounded-xl bg-night/30 p-3">
          <div className="mb-2 flex items-center justify-between text-xs text-dusk">
            <span>Withdraw</span>
            <span className="text-starlight">{pct}%</span>
          </div>
          <input
            type="range"
            min={1}
            max={100}
            value={pct}
            onChange={(e) => setPct(parseInt(e.target.value, 10))}
            className="w-full accent-meridian"
          />
          <Button variant="outline" className="mt-2 w-full" onClick={onRemove} disabled={busy}>
            {busy ? "Working…" : `Remove ${pct}%`}
          </Button>
        </div>
      )}
    </Card>
  );
}

function YieldCard({ state, onDone }: { state: CammState; onDone: () => void }) {
  const { pool, slot } = state;
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { notifyTx } = useToast();
  const [busy, setBusy] = useState(false);

  if (!pool) return null;
  const pending = pendingYield(pool, slot);
  const enabled = pool.yieldRate > 0n;
  // Rate shown as yield per 1000 slots (yieldRate/1e9 * 1000 * 100%).
  const ratePct = (Number(pool.yieldRate) / 1e9) * 1000 * 100;

  async function onHarvest() {
    if (!connected || !publicKey) return setVisible(true);
    setBusy(true);
    const sig = await notifyTx(
      () => executeCammHarvest({ connection, sendTransaction, owner: publicKey }),
      { pending: "Harvesting yield…", success: "Yield harvested" },
    );
    if (sig) onDone();
    setBusy(false);
  }

  return (
    <Card className="flex flex-col gap-3 p-5">
      <span className="flex items-center gap-2 text-sm text-dusk">
        <Sprout className="h-4 w-4 text-star" /> Idle-reserve yield
      </span>
      {!enabled ? (
        <p className="text-sm text-dusk">Yield is not configured for this pool.</p>
      ) : (
        <>
          <p className="text-xs text-dusk">
            Reserves above the buffer are deployed to earn yield; harvesting pays it into the pool,
            raising every LP&apos;s share value. Mock lending market (devnet).
          </p>
          <dl className="space-y-1 rounded-xl bg-night/30 p-3 text-xs text-dusk">
            <Row k="Rate" v={`${ratePct.toFixed(3)}% / 1000 slots`} />
            <Row k="Buffer" v={`${(Number(pool.bufferBps) / 100).toFixed(1)}%`} />
            <Row
              k={`Deployed ${tokenA.symbol}`}
              v={formatAmount(pool.deployedA, tokenA.decimals)}
            />
            <Row
              k={`Deployed ${tokenB.symbol}`}
              v={formatAmount(pool.deployedB, tokenB.decimals)}
            />
            <Row
              k="Pending yield"
              v={`${formatAmount(pending.a, tokenA.decimals)} ${tokenA.symbol} · ${formatAmount(pending.b, tokenB.decimals)} ${tokenB.symbol}`}
            />
          </dl>
          <Button onClick={onHarvest} disabled={busy} variant="outline">
            {busy ? (
              <span className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" /> Harvesting…
              </span>
            ) : !connected ? (
              "Connect wallet"
            ) : (
              "Harvest yield"
            )}
          </Button>
        </>
      )}
    </Card>
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
      <dd className="font-mono text-starlight tnum">{v}</dd>
    </div>
  );
}
