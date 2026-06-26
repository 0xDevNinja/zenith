import { useMemo, useState } from "react";
import { Loader2, Plus } from "lucide-react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import { Rounding } from "@zenith/sdk";
import { Card, Eyebrow } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { DepthChart } from "@/components/DepthChart";
import { usePoolConfig } from "@/lib/usePoolConfig";
import { usePositions, type OwnedPosition } from "@/lib/usePositions";
import { useTokenBalance } from "@/lib/useTokenBalance";
import { MARKET } from "@/lib/market";
import {
  composition,
  executeAddLiquidity,
  executeClaimFee,
  executeOpenPosition,
  executeRemoveAll,
  executeRemoveLiquidity,
  inRange,
  liquidityForTokenA,
  owedFees,
  slipDown,
  slipUp,
} from "@/lib/liquidity";
import { formatAmount, formatPlain, parseAmount } from "@/lib/tokens";
import { explorerTx } from "@/lib/config";
import { cn } from "@/lib/utils";

type Tx = { kind: "idle" } | { kind: "pending"; what: string } | { kind: "ok"; sig: string } | { kind: "err"; msg: string };

const SLIPPAGE_BPS = 50;
const A = MARKET.tokenA;
const B = MARKET.tokenB;

function activeAt(min: bigint, cur: bigint, max: bigint): number {
  const span = max - min;
  if (span <= 0n) return 0.5;
  return Math.min(0.95, Math.max(0.05, Number(cur - min) / Number(span)));
}

export function Positions() {
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { pool, loading: poolLoading, error } = usePoolConfig();
  const { positions, loading: posLoading, refetch: refetchPositions } = usePositions();

  const balA = useTokenBalance(A.mint);
  const balB = useTokenBalance(B.mint);

  const [depositStr, setDepositStr] = useState("");
  const [target, setTarget] = useState<"new" | string>("new");
  const [tx, setTx] = useState<Tx>({ kind: "idle" });

  const amountA = parseAmount(depositStr, A.decimals);

  // Preview: L from the token-A deposit, then the matching token-B requirement.
  const preview = useMemo(() => {
    if (!pool || amountA === null || amountA <= 0n) return null;
    const L = liquidityForTokenA(pool, amountA);
    if (!L || L <= 0n) return null;
    const comp = composition(pool, L, Rounding.Up);
    return { L, amountA: comp.amountA, amountB: comp.amountB };
  }, [pool, amountA]);

  const insufficient =
    preview && balA.raw !== null && balB.raw !== null
      ? preview.amountA > balA.raw || preview.amountB > balB.raw
      : false;

  const base = () => ({ connection, sendTransaction, owner: publicKey! });

  const refreshAll = () => {
    refetchPositions();
    balA.refetch();
    balB.refetch();
  };

  const onProvide = async () => {
    if (!connected || !publicKey) return setVisible(true);
    if (!preview || insufficient) return;
    setTx({ kind: "pending", what: target === "new" ? "Opening position" : "Adding liquidity" });
    try {
      const args = {
        liquidityDelta: preview.L,
        tokenAMax: slipUp(preview.amountA, SLIPPAGE_BPS),
        tokenBMax: slipUp(preview.amountB, SLIPPAGE_BPS),
      };
      let sig: string;
      if (target === "new") {
        sig = await executeOpenPosition(base(), args);
      } else {
        const p = positions.find((x) => x.address.toBase58() === target)!;
        sig = await executeAddLiquidity(base(), { position: p.address, nftMint: p.nftMint }, args);
      }
      setTx({ kind: "ok", sig });
      setDepositStr("");
      refreshAll();
    } catch (e) {
      setTx({ kind: "err", msg: e instanceof Error ? e.message : "Transaction failed" });
    }
  };

  const onRemove = async (p: OwnedPosition, fraction: number) => {
    if (!pool || !publicKey) return;
    setTx({ kind: "pending", what: "Removing liquidity" });
    try {
      const all = fraction >= 1;
      const delta = all ? p.position.liquidity : (p.position.liquidity * BigInt(Math.round(fraction * 100))) / 100n;
      const comp = composition(pool, delta, Rounding.Down);
      const mins = {
        tokenAMin: slipDown(comp.amountA, SLIPPAGE_BPS),
        tokenBMin: slipDown(comp.amountB, SLIPPAGE_BPS),
      };
      const ref = { position: p.address, nftMint: p.nftMint };
      const sig = all
        ? await executeRemoveAll(base(), ref, mins)
        : await executeRemoveLiquidity(base(), ref, { liquidityDelta: delta, ...mins });
      setTx({ kind: "ok", sig });
      refreshAll();
    } catch (e) {
      setTx({ kind: "err", msg: e instanceof Error ? e.message : "Transaction failed" });
    }
  };

  const onClaim = async (p: OwnedPosition) => {
    if (!publicKey) return;
    setTx({ kind: "pending", what: "Claiming fees" });
    try {
      const sig = await executeClaimFee(base(), { position: p.address, nftMint: p.nftMint });
      setTx({ kind: "ok", sig });
      refreshAll();
    } catch (e) {
      setTx({ kind: "err", msg: e instanceof Error ? e.message : "Claim failed" });
    }
  };

  const totalLiquidity = positions.reduce((s, p) => s + p.position.liquidity, 0n);
  const totalOwed = pool
    ? positions.reduce(
        (acc, p) => {
          const o = owedFees(pool, p.position);
          return { a: acc.a + o.a, b: acc.b + o.b };
        },
        { a: 0n, b: 0n },
      )
    : { a: 0n, b: 0n };

  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-10 animate-rise">
      <div className="mb-6">
        <Eyebrow>Your constellations</Eyebrow>
        <h1 className="mt-1 font-display text-4xl text-starlight">Positions</h1>
      </div>

      <TxBanner tx={tx} />

      <div className="grid gap-4 lg:grid-cols-[1fr_1.1fr]">
        {/* Provide */}
        <Card className="h-fit p-5">
          <div className="mb-4 flex items-center justify-between">
            <span className="font-display text-2xl text-starlight">Provide liquidity</span>
            <span className="font-mono text-xs text-dusk">slippage {(SLIPPAGE_BPS / 100).toFixed(2)}%</span>
          </div>

          {/* range display (pool-level band) */}
          {pool && (
            <div className="mb-4">
              <div className="mb-1 flex justify-between font-mono text-[11px] text-dusk">
                <span>{(Number(pool.sqrtMinPrice) ** 2 / 2 ** 128).toFixed(3)}</span>
                <span className="text-meridian">price {(Number(pool.sqrtPrice) ** 2 / 2 ** 128).toFixed(4)}</span>
                <span>{(Number(pool.sqrtMaxPrice) ** 2 / 2 ** 128).toFixed(3)}</span>
              </div>
              <div className="h-14">
                <DepthChart
                  activeAt={activeAt(pool.sqrtMinPrice, pool.sqrtPrice, pool.sqrtMaxPrice)}
                  width={300}
                  height={56}
                  bins={28}
                  animate={false}
                  axis={false}
                  className="h-full w-full"
                />
              </div>
              <p className="mt-1 text-center text-[11px] text-dusk">Full-band position over the pool's range</p>
            </div>
          )}

          <div className="well rounded-2xl px-4 py-3.5">
            <div className="mb-1 flex justify-between text-xs text-dusk">
              <span>Deposit</span>
              <span className="font-mono tnum">
                Balance {balA.raw !== null ? formatAmount(balA.raw, A.decimals, 2) : "—"}
                {balA.raw !== null && balA.raw > 0n && (
                  <button className="ml-1.5 text-star hover:brightness-110" onClick={() => setDepositStr(formatPlain(balA.raw!, A.decimals))}>
                    Max
                  </button>
                )}
              </span>
            </div>
            <div className="flex items-center justify-between gap-3">
              <input
                value={depositStr}
                onChange={(e) => {
                  setDepositStr(e.target.value);
                  setTx({ kind: "idle" });
                }}
                inputMode="decimal"
                placeholder="0.0"
                className="w-full bg-transparent font-mono text-3xl tabular-nums tnum text-starlight outline-none placeholder:text-dusk/50"
              />
              <span className="shrink-0 rounded-full border border-line bg-panel-2/80 px-3 py-1.5 font-medium">{A.symbol}</span>
            </div>
          </div>

          <div className="mt-2 flex items-center justify-between rounded-2xl border border-line/40 px-4 py-3 text-sm">
            <span className="text-dusk">Paired {B.symbol}</span>
            <span className="font-mono tnum text-starlight">
              {preview ? formatAmount(preview.amountB, B.decimals) : "—"}
            </span>
          </div>

          {positions.length > 0 && (
            <div className="mt-3">
              <div className="mb-1.5 text-xs text-dusk">Target</div>
              <div className="flex flex-wrap gap-2">
                <TargetChip active={target === "new"} onClick={() => setTarget("new")}>
                  New position
                </TargetChip>
                {positions.map((p, i) => (
                  <TargetChip key={p.address.toBase58()} active={target === p.address.toBase58()} onClick={() => setTarget(p.address.toBase58())}>
                    Position {i + 1}
                  </TargetChip>
                ))}
              </div>
            </div>
          )}

          <ProvideButton
            connected={connected}
            poolLoading={poolLoading}
            marketError={!!error}
            pending={tx.kind === "pending"}
            hasAmount={amountA !== null && amountA > 0n}
            balanceLoading={connected && (balA.raw === null || balB.raw === null)}
            previewOk={!!preview}
            insufficient={!!insufficient}
            isNew={target === "new"}
            onClick={onProvide}
          />
        </Card>

        {/* Manage */}
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            <Stat label="Open positions" value={String(positions.length)} />
            <Stat label="Total liquidity" value={totalLiquidity > 0n ? formatAmount(totalLiquidity, 0, 0) : "0"} />
            <Stat
              label="Unclaimed fees"
              value={`${formatAmount(totalOwed.a, A.decimals, 2)} / ${formatAmount(totalOwed.b, B.decimals, 2)}`}
              accent
            />
          </div>

          {!connected ? (
            <Card className="p-8 text-center text-sm text-dusk">Connect a wallet to see your positions.</Card>
          ) : posLoading ? (
            <Card className="flex items-center justify-center p-8 text-dusk">
              <Loader2 className="h-5 w-5 animate-spin" />
            </Card>
          ) : positions.length === 0 ? (
            <Card className="p-8 text-center text-sm text-dusk">
              No positions yet. Provide liquidity to open one.
            </Card>
          ) : (
            positions.map((p, i) => (
              <PositionCard
                key={p.address.toBase58()}
                index={i + 1}
                owned={p}
                pool={pool}
                pending={tx.kind === "pending"}
                onRemove={onRemove}
                onClaim={onClaim}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function PositionCard({
  index,
  owned,
  pool,
  pending,
  onRemove,
  onClaim,
}: {
  index: number;
  owned: OwnedPosition;
  pool: ReturnType<typeof usePoolConfig>["pool"];
  pending: boolean;
  onRemove: (p: OwnedPosition, fraction: number) => void;
  onClaim: (p: OwnedPosition) => void;
}) {
  const comp = pool ? composition(pool, owned.position.liquidity) : null;
  const owed = pool ? owedFees(pool, owned.position) : { a: 0n, b: 0n };
  const hasFees = owed.a > 0n || owed.b > 0n;
  const empty = owned.position.liquidity === 0n;
  const ranged = pool ? inRange(pool) : true;

  return (
    <Card className="p-5">
      <div className="mb-3 flex items-center justify-between">
        <span className="flex items-center gap-2.5">
          <span className="font-display text-xl text-starlight">Position {index}</span>
          <span className={cn("inline-flex items-center gap-1.5 text-xs font-medium", ranged ? "text-meridian" : "text-dusk")}>
            <span className={cn("h-1.5 w-1.5 rounded-full", ranged ? "bg-meridian shadow-[0_0_8px] shadow-meridian" : "bg-dusk")} />
            {ranged ? "In range" : "Out of range"}
          </span>
        </span>
        <span className="font-mono text-[11px] text-dusk">L {formatAmount(owned.position.liquidity, 0, 0)}</span>
      </div>
      <div className="grid grid-cols-2 gap-3 border-t border-line/40 pt-3">
        <Holding label={A.symbol} raw={comp?.amountA ?? 0n} decimals={A.decimals} />
        <Holding label={B.symbol} raw={comp?.amountB ?? 0n} decimals={B.decimals} />
        <Holding label={`Fees ${A.symbol}`} raw={owed.a} decimals={A.decimals} accent />
        <Holding label={`Fees ${B.symbol}`} raw={owed.b} decimals={B.decimals} accent />
      </div>
      <div className="mt-4 flex gap-2">
        <Button variant="gold" size="sm" className="flex-1" disabled={pending || !hasFees} onClick={() => onClaim(owned)}>
          Claim fees
        </Button>
        {!empty && (
          <>
            <Button variant="outline" size="sm" className="flex-1" disabled={pending} onClick={() => onRemove(owned, 0.5)}>
              Remove 50%
            </Button>
            <Button variant="outline" size="sm" className="flex-1" disabled={pending} onClick={() => onRemove(owned, 1)}>
              Remove all
            </Button>
          </>
        )}
      </div>
    </Card>
  );
}

function ProvideButton({
  connected,
  poolLoading,
  marketError,
  pending,
  hasAmount,
  balanceLoading,
  previewOk,
  insufficient,
  isNew,
  onClick,
}: {
  connected: boolean;
  poolLoading: boolean;
  marketError: boolean;
  pending: boolean;
  hasAmount: boolean;
  balanceLoading: boolean;
  previewOk: boolean;
  insufficient: boolean;
  isNew: boolean;
  onClick: () => void;
}) {
  let label = isNew ? "Open position" : "Add liquidity";
  let disabled = false;
  if (!connected) label = "Connect wallet";
  else if (poolLoading) (label = "Loading market…"), (disabled = true);
  else if (marketError) (label = "Market unavailable"), (disabled = true);
  else if (!hasAmount) (label = "Enter an amount"), (disabled = true);
  else if (!previewOk) (label = "Amount out of range"), (disabled = true);
  else if (balanceLoading) (label = "Checking balance…"), (disabled = true);
  else if (insufficient) (label = "Insufficient balance"), (disabled = true);
  else if (pending) (label = "Submitting…"), (disabled = true);

  return (
    <Button size="lg" className="mt-3 w-full" onClick={onClick} disabled={disabled}>
      {pending && <Loader2 className="h-4 w-4 animate-spin" />}
      {!pending && isNew && connected && <Plus className="h-4 w-4" />}
      {label}
    </Button>
  );
}

function TargetChip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "rounded-full border px-3 py-1 text-xs transition-colors",
        active ? "border-meridian/50 bg-meridian/10 text-meridian" : "border-line text-dusk hover:text-starlight",
      )}
    >
      {children}
    </button>
  );
}

function Holding({ label, raw, decimals, accent }: { label: string; raw: bigint; decimals: number; accent?: boolean }) {
  return (
    <div>
      <div className="text-[11px] text-dusk">{label}</div>
      <div className={cn("mt-0.5 font-mono text-sm tnum", accent ? "text-star" : "text-starlight")}>
        {formatAmount(raw, decimals)}
      </div>
    </div>
  );
}

function Stat({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <Card className="px-5 py-4">
      <div className="text-xs text-dusk">{label}</div>
      <div className={cn("mt-1 font-mono text-2xl tnum", accent ? "text-star" : "text-starlight")}>{value}</div>
    </Card>
  );
}

function TxBanner({ tx }: { tx: Tx }) {
  if (tx.kind === "ok") {
    return (
      <a href={explorerTx(tx.sig)} target="_blank" rel="noreferrer" className="mb-4 block rounded-2xl border border-meridian/40 bg-meridian/10 px-4 py-3 text-center text-sm text-meridian hover:bg-meridian/15">
        Confirmed — view on explorer ↗
      </a>
    );
  }
  if (tx.kind === "err") {
    return <div className="mb-4 rounded-2xl border border-star/40 bg-star/10 px-4 py-3 text-center text-sm text-star">{tx.msg}</div>;
  }
  return null;
}
