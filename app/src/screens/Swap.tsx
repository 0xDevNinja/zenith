import { useEffect, useMemo, useState } from "react";
import { ArrowDown, ChevronDown, Loader2, Settings2, TrendingUp } from "lucide-react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import { quoteSwap, SwapMode, type Pool, type SwapQuote } from "@zenith/sdk";
import { Card, Eyebrow } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { DepthChart } from "@/components/DepthChart";
import { usePoolConfig } from "@/lib/usePoolConfig";
import { useTokenBalance } from "@/lib/useTokenBalance";
import { MARKET, type TokenInfo } from "@/lib/market";
import { directionFor, executeSwap } from "@/lib/swap";
import { formatAmount, formatPlain, parseAmount } from "@/lib/tokens";
import { explorerTx } from "@/lib/config";
import { cn } from "@/lib/utils";

type TxState =
  | { kind: "idle" }
  | { kind: "pending" }
  | { kind: "success"; signature: string }
  | { kind: "error"; message: string };

// Map the pool's price within [sqrtMin, sqrtMax] to 0..1 for the depth marker.
function activeAt(pool: Pool | null): number {
  if (!pool) return 0.5;
  const span = pool.sqrtMaxPrice - pool.sqrtMinPrice;
  if (span <= 0n) return 0.5;
  const num = Number(pool.sqrtPrice - pool.sqrtMinPrice);
  const ratio = num / Number(span);
  return Math.min(0.95, Math.max(0.05, ratio));
}

export function Swap() {
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { pool, config, loading, error, refetch } = usePoolConfig();

  const [inputToken, setInputToken] = useState<TokenInfo>(MARKET.tokenA);
  const [outputToken, setOutputToken] = useState<TokenInfo>(MARKET.tokenB);
  const [amountStr, setAmountStr] = useState("");
  const [slippageBps, setSlippageBps] = useState(50);
  const [slot, setSlot] = useState<bigint | null>(null);
  const [tx, setTx] = useState<TxState>({ kind: "idle" });

  const inBal = useTokenBalance(inputToken.mint);
  const outBal = useTokenBalance(outputToken.mint);

  // Slot drives the fee derivation in the quote; refresh periodically. (The
  // seeded config uses a constant fee, so slot staleness is harmless today —
  // but a dynamic-fee config would need this fresher.)
  useEffect(() => {
    let active = true;
    const load = () => connection.getSlot().then((s) => active && setSlot(BigInt(s))).catch(() => {});
    load();
    const id = setInterval(load, 20_000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [connection]);

  const rawAmount = parseAmount(amountStr, inputToken.decimals);

  const quote = useMemo<SwapQuote | null>(() => {
    if (!pool || !config || slot === null || rawAmount === null || rawAmount <= 0n) return null;
    try {
      return quoteSwap({
        pool,
        config,
        slot,
        direction: directionFor(inputToken.mint, pool),
        mode: SwapMode.ExactIn,
        amount: rawAmount,
        slippageBps,
      });
    } catch {
      return null;
    }
  }, [pool, config, slot, rawAmount, inputToken, slippageBps]);

  const flip = () => {
    setInputToken(outputToken);
    setOutputToken(inputToken);
    setAmountStr("");
    setTx({ kind: "idle" });
  };

  const insufficient = rawAmount !== null && inBal.raw !== null && rawAmount > inBal.raw;
  const quoteFailed = rawAmount !== null && rawAmount > 0n && pool && config && slot !== null && !quote;

  const onSwap = async () => {
    if (!connected || !publicKey) {
      setVisible(true);
      return;
    }
    if (!quote || !pool || insufficient) return;
    setTx({ kind: "pending" });
    try {
      const signature = await executeSwap({
        connection,
        sendTransaction,
        owner: publicKey,
        direction: directionFor(inputToken.mint, pool),
        mode: SwapMode.ExactIn,
        amount: quote.amountIn,
        otherAmountThreshold: quote.otherAmountThreshold,
      });
      setTx({ kind: "success", signature });
      setAmountStr("");
      refetch();
      inBal.refetch();
      outBal.refetch();
    } catch (e) {
      setTx({ kind: "error", message: e instanceof Error ? e.message : "Swap failed" });
    }
  };

  const price = pool ? Number(pool.sqrtPrice) ** 2 / 2 ** 128 : null;

  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-8 sm:pt-12 animate-rise">
      <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
        <div>
          <Eyebrow>Spot · Devnet</Eyebrow>
          <h1 className="mt-1 font-display text-4xl leading-none text-starlight">
            {MARKET.tokenA.symbol} / {MARKET.tokenB.symbol}
          </h1>
        </div>
        <div className="flex items-center gap-6">
          <Stat label="Price" value={price !== null ? price.toFixed(4) : "—"} />
          <Stat label="Fee" value={config ? `${(config.baseFeeBps / 100).toFixed(2)}%` : "—"} tone="star" />
          <Stat label="Status" value={loading ? "Loading" : error ? "Error" : "Live"} tone={error ? undefined : "meridian"} />
        </div>
      </div>

      <div className="grid gap-4 lg:grid-cols-[1.25fr_1fr]">
        {/* Hero: liquidity depth heatmap */}
        <Card className="flex flex-col p-5">
          <div className="mb-2 flex items-center justify-between">
            <span className="flex items-center gap-2 text-sm text-dusk">
              <TrendingUp className="h-4 w-4 text-star" />
              Liquidity depth
            </span>
            <span className="font-mono text-xs text-dusk tnum">concentration · σ band</span>
          </div>
          <div className="flex-1">
            <DepthChart activeAt={activeAt(pool)} width={560} height={250} bins={56} className="h-full min-h-[250px] w-full" />
          </div>
          <div className="mt-3 grid grid-cols-3 gap-3 border-t border-line/40 pt-3">
            <MiniStat label="Active price" value={price !== null ? price.toFixed(4) : "—"} tone="meridian" />
            <MiniStat label="In-range liquidity" value={pool ? formatAmount(pool.liquidity, 0, 0) : "—"} />
            <MiniStat label="Base fee" value={config ? `${(config.baseFeeBps / 100).toFixed(2)}%` : "—"} tone="star" />
          </div>
        </Card>

        {/* Instrument */}
        <div>
          <div className="mb-3 flex items-center justify-between">
            <Eyebrow>Swap</Eyebrow>
            <SlippageControl bps={slippageBps} onChange={setSlippageBps} />
          </div>

          <Card className="p-2">
            <Field
              label="You pay"
              amount={amountStr}
              onAmount={(v) => {
                setAmountStr(v);
                setTx({ kind: "idle" });
              }}
              token={inputToken}
              balanceRaw={inBal.raw}
              editable
            />
            <div className="relative h-0">
              <div className="absolute left-1/2 top-1/2 z-10 -translate-x-1/2 -translate-y-1/2">
                <button
                  onClick={flip}
                  className="grid h-9 w-9 place-items-center rounded-xl border border-line bg-panel-2 text-meridian shadow-lg transition-transform hover:rotate-180"
                  aria-label="Flip direction"
                >
                  <ArrowDown className="h-4 w-4" />
                </button>
              </div>
            </div>
            <Field
              label="You receive"
              amount={quote ? formatPlain(quote.amountOut, outputToken.decimals) : ""}
              token={outputToken}
              balanceRaw={outBal.raw}
            />
          </Card>

          <Card className="mt-2 px-4 py-1 text-sm">
            <div className="divide-y divide-line/30">
              <Row
                label="Rate"
                value={
                  quote && rawAmount && rawAmount > 0n
                    ? `1 ${inputToken.symbol} = ${(
                        Number(formatPlain(quote.amountOut, outputToken.decimals)) /
                        Number(formatPlain(rawAmount, inputToken.decimals))
                      ).toFixed(4)} ${outputToken.symbol}`
                    : "—"
                }
              />
              <Row
                label="Fee"
                value={config ? `${(config.baseFeeBps / 100).toFixed(2)}%` : "—"}
                hint={quote ? `${formatAmount(quote.feeAmount, inputToken.decimals)} ${inputToken.symbol}` : undefined}
              />
              <Row
                label="Price impact"
                value={quote?.priceImpactBps != null ? `${(Number(quote.priceImpactBps) / 100).toFixed(2)}%` : "—"}
                tone="ok"
              />
              <Row
                label="Min. received"
                value={
                  quote?.minAmountOut != null
                    ? `${formatAmount(quote.minAmountOut, outputToken.decimals)} ${outputToken.symbol}`
                    : "—"
                }
              />
            </div>
          </Card>

          <SwapButton
            connected={connected}
            loading={loading}
            marketError={!!error}
            pending={tx.kind === "pending"}
            hasAmount={rawAmount !== null && rawAmount > 0n}
            balanceLoading={connected && inBal.raw === null}
            insufficient={!!insufficient}
            quoteFailed={!!quoteFailed}
            symbol={inputToken.symbol}
            onClick={onSwap}
          />

          <TxBanner tx={tx} />
          {connected && inBal.raw === 0n && (
            <p className="mt-3 text-center text-xs text-dusk">
              No {inputToken.symbol} in this wallet. Test tokens are minted by the devnet seed script.
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

function SwapButton({
  connected,
  loading,
  marketError,
  pending,
  hasAmount,
  balanceLoading,
  insufficient,
  quoteFailed,
  symbol,
  onClick,
}: {
  connected: boolean;
  loading: boolean;
  marketError: boolean;
  pending: boolean;
  hasAmount: boolean;
  balanceLoading: boolean;
  insufficient: boolean;
  quoteFailed: boolean;
  symbol: string;
  onClick: () => void;
}) {
  let label = "Swap";
  let disabled = false;
  if (!connected) label = "Connect wallet";
  else if (loading) (label = "Loading market…"), (disabled = true);
  else if (marketError) (label = "Market unavailable"), (disabled = true);
  else if (!hasAmount) (label = "Enter an amount"), (disabled = true);
  else if (balanceLoading) (label = "Checking balance…"), (disabled = true);
  else if (insufficient) (label = `Insufficient ${symbol}`), (disabled = true);
  else if (quoteFailed) (label = "Can't quote this amount"), (disabled = true);
  else if (pending) (label = "Swapping…"), (disabled = true);

  return (
    <Button size="lg" className="mt-3 w-full text-base" onClick={onClick} disabled={disabled}>
      {pending && <Loader2 className="h-4 w-4 animate-spin" />}
      {label}
    </Button>
  );
}

function TxBanner({ tx }: { tx: TxState }) {
  if (tx.kind === "success") {
    return (
      <a
        href={explorerTx(tx.signature)}
        target="_blank"
        rel="noreferrer"
        className="mt-3 block rounded-2xl border border-meridian/40 bg-meridian/10 px-4 py-3 text-center text-sm text-meridian transition-colors hover:bg-meridian/15"
      >
        Swap confirmed — view on explorer ↗
      </a>
    );
  }
  if (tx.kind === "error") {
    return (
      <div className="mt-3 rounded-2xl border border-star/40 bg-star/10 px-4 py-3 text-center text-sm text-star">
        {tx.message}
      </div>
    );
  }
  return null;
}

function SlippageControl({ bps, onChange }: { bps: number; onChange: (v: number) => void }) {
  const [open, setOpen] = useState(false);
  const presets = [10, 50, 100];
  return (
    <div className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1.5 text-dusk transition-colors hover:text-starlight"
        aria-label="Slippage settings"
      >
        <Settings2 className="h-4 w-4" />
        <span className="font-mono text-xs">{(bps / 100).toFixed(2)}%</span>
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-2 w-56 rounded-2xl border border-line bg-panel p-3 shadow-instrument">
          <div className="mb-2 text-xs text-dusk">Slippage tolerance</div>
          <div className="flex gap-2">
            {presets.map((p) => (
              <button
                key={p}
                onClick={() => onChange(p)}
                className={cn(
                  "flex-1 rounded-lg border px-2 py-1.5 font-mono text-xs transition-colors",
                  bps === p ? "border-meridian/50 bg-meridian/10 text-meridian" : "border-line text-dusk hover:text-starlight",
                )}
              >
                {(p / 100).toFixed(p < 100 ? 2 : 1)}%
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function Field({
  label,
  amount,
  onAmount,
  token,
  balanceRaw,
  editable,
}: {
  label: string;
  amount: string;
  onAmount?: (v: string) => void;
  token: TokenInfo;
  balanceRaw: bigint | null;
  editable?: boolean;
}) {
  return (
    <div className="well rounded-2xl px-4 py-3.5">
      <div className="mb-1 flex items-center justify-between text-xs text-dusk">
        <span>{label}</span>
        <span className="font-mono tnum">
          Balance {balanceRaw !== null ? formatAmount(balanceRaw, token.decimals, 2) : "—"}
          {editable && balanceRaw !== null && balanceRaw > 0n && (
            <button
              className="ml-1.5 text-star hover:brightness-110"
              onClick={() => onAmount?.(formatPlain(balanceRaw, token.decimals))}
            >
              Max
            </button>
          )}
        </span>
      </div>
      <div className="flex items-center justify-between gap-3">
        <input
          value={amount}
          onChange={(e) => onAmount?.(e.target.value)}
          readOnly={!editable}
          inputMode="decimal"
          className={cn(
            "w-full bg-transparent font-mono text-3xl tabular-nums tnum text-starlight outline-none placeholder:text-dusk/50",
            !editable && "text-starlight/90",
          )}
          placeholder="0.0"
        />
        <span className="flex shrink-0 items-center gap-2 rounded-full border border-line bg-panel-2/80 py-1.5 pl-1.5 pr-3">
          <span className="grid h-7 w-7 place-items-center rounded-full bg-gradient-to-br from-star/40 to-meridian/20 font-mono text-[11px] font-semibold text-starlight">
            {token.symbol.slice(1, 3)}
          </span>
          <span className="font-medium">{token.symbol}</span>
          <ChevronDown className="h-4 w-4 text-dusk/40" />
        </span>
      </div>
    </div>
  );
}

function Row({
  label,
  value,
  hint,
  tone = "default",
}: {
  label: string;
  value: string;
  hint?: string;
  tone?: "default" | "ok" | "warn";
}) {
  return (
    <div className="flex items-center justify-between py-2.5">
      <span className="text-dusk">{label}</span>
      <span className="flex items-center gap-2">
        {hint && <span className="font-mono text-xs text-dusk tnum">{hint}</span>}
        <span
          className={cn(
            "font-mono tnum",
            tone === "warn" && "text-star",
            tone === "ok" && "text-meridian",
            tone === "default" && "text-starlight",
          )}
        >
          {value}
        </span>
      </span>
    </div>
  );
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: "meridian" | "star" }) {
  return (
    <div className="text-right">
      <div className="text-[11px] uppercase tracking-wider text-dusk">{label}</div>
      <div
        className={cn(
          "mt-0.5 font-mono text-lg tnum",
          tone === "meridian" ? "text-meridian" : tone === "star" ? "text-star" : "text-starlight",
        )}
      >
        {value}
      </div>
    </div>
  );
}

function MiniStat({ label, value, tone }: { label: string; value: string; tone?: "meridian" | "star" }) {
  return (
    <div>
      <div className="text-[11px] text-dusk">{label}</div>
      <div
        className={cn(
          "mt-0.5 font-mono text-sm tnum",
          tone === "meridian" ? "text-meridian" : tone === "star" ? "text-star" : "text-starlight",
        )}
      >
        {value}
      </div>
    </div>
  );
}
