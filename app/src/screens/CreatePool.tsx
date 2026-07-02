import { useEffect, useMemo, useState } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { useWalletModal } from "@solana/wallet-adapter-react-ui";
import { camm } from "@zenith/sdk";
import { PlusCircle, CheckCircle2 } from "lucide-react";

import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useZenith } from "@/lib/sdk";
import {
  SELECTABLE_MINTS,
  derivePool,
  executeCreatePool,
  poolExists,
  type SelectableMint,
} from "@/lib/cammCreate";
import { formatAmount, parseAmount } from "@/lib/tokens";
import { useToast } from "@/lib/toast";

const short = (s: string) => `${s.slice(0, 4)}…${s.slice(-4)}`;
const optLabel = (m: SelectableMint) => `${m.symbol} · ${short(m.mint.toBase58())}`;

export function CreatePool() {
  const { zenith } = useZenith();
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();
  const { setVisible } = useWalletModal();
  const { notifyTx } = useToast();

  const [aIdx, setAIdx] = useState(0);
  const [bIdx, setBIdx] = useState(1);
  const [feeStr, setFeeStr] = useState("30");
  const [aStr, setAStr] = useState("100");
  const [bStr, setBStr] = useState("100");
  const [busy, setBusy] = useState(false);
  const [exists, setExists] = useState<boolean | null>(null);
  const [created, setCreated] = useState<{ pool: string; reserveA: bigint; reserveB: bigint } | null>(null);

  const tokA = SELECTABLE_MINTS[aIdx];
  const tokB = SELECTABLE_MINTS[bIdx];
  const same = tokA.mint.equals(tokB.mint);
  const feeBps = parseInt(feeStr, 10);
  const amountA = parseAmount(aStr, tokA.decimals) ?? 0n;
  const amountB = parseAmount(bStr, tokB.decimals) ?? 0n;

  // Which of the two is the pool's canonical tokenA (sorted) — for labelling.
  const sorted = useMemo(() => (same ? null : derivePool(tokA.mint, tokB.mint)), [tokA, tokB, same]);

  // Check whether this pair already has a pool whenever the selection changes.
  useEffect(() => {
    if (same) {
      setExists(null);
      return;
    }
    let on = true;
    setExists(null);
    poolExists(zenith, tokA.mint, tokB.mint)
      .then((e) => on && setExists(e))
      .catch(() => on && setExists(null));
    return () => {
      on = false;
    };
  }, [zenith, tokA, tokB, same]);

  const validation = useMemo(() => {
    if (same) return "Pick two different tokens";
    if (!Number.isFinite(feeBps) || feeBps < 0 || feeBps >= 10_000) return "Fee must be 0–9999 bps";
    if (amountA <= 0n || amountB <= 0n) return "Enter an initial amount for both";
    // First deposit must clear the minimum-liquidity floor (sqrt(a*b) > 1000).
    if (camm.initialShares(amountA, amountB) <= camm.MINIMUM_LIQUIDITY)
      return "Initial deposit is too small (raise the amounts)";
    if (exists) return "A pool for this pair already exists";
    return null;
  }, [same, feeBps, amountA, amountB, exists]);

  async function onCreate() {
    if (!connected || !publicKey) return setVisible(true);
    if (validation) return;
    setBusy(true);
    setCreated(null);
    // executeCreatePool sorts the mints; map the entered amounts to sorted order.
    const [sa] = camm.sortMints(tokA.mint, tokB.mint);
    const aIsFirst = sa.equals(tokA.mint);
    const sig = await notifyTx(
      async () => {
        const pool = await executeCreatePool(
          { connection, sendTransaction, owner: publicKey },
          {
            mintA: tokA.mint,
            mintB: tokB.mint,
            baseFeeBps: feeBps,
            protocolFeeRate: 2000,
            amountA: aIsFirst ? amountA : amountB,
            amountB: aIsFirst ? amountB : amountA,
          },
        );
        return pool.toBase58();
      },
      { pending: "Creating pool + seeding…", success: "Pool created" },
    );
    if (sig && sorted) {
      const p = await camm.fetchPool(zenith, sorted.pool);
      if (p) setCreated({ pool: sorted.pool.toBase58(), reserveA: p.reserveA, reserveB: p.reserveB });
      setExists(true);
    }
    setBusy(false);
  }

  return (
    <div className="mx-auto max-w-2xl px-5 pb-24 pt-8 sm:pt-12 animate-rise">
      <div className="mb-5">
        <span className="flex items-center gap-2 text-sm text-dusk">
          <PlusCircle className="h-4 w-4 text-meridian" /> Permissionless
        </span>
        <h1 className="mt-1 font-display text-4xl leading-none text-starlight">Create a pool</h1>
        <p className="mt-2 max-w-lg text-sm text-dusk">
          Pair any two tokens into a new constant-product market and seed it with the first deposit.
          You become the first liquidity provider. Anyone can do this — no gatekeeper.
        </p>
      </div>

      <Card className="flex flex-col gap-4 p-5">
        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-xs text-dusk">Token A</span>
            <select
              value={aIdx}
              onChange={(e) => setAIdx(parseInt(e.target.value, 10))}
              className="rounded-xl border border-line bg-night/40 p-2.5 text-sm text-starlight outline-none"
            >
              {SELECTABLE_MINTS.map((m, i) => (
                <option key={m.mint.toBase58()} value={i}>{optLabel(m)}</option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-dusk">Token B</span>
            <select
              value={bIdx}
              onChange={(e) => setBIdx(parseInt(e.target.value, 10))}
              className="rounded-xl border border-line bg-night/40 p-2.5 text-sm text-starlight outline-none"
            >
              {SELECTABLE_MINTS.map((m, i) => (
                <option key={m.mint.toBase58()} value={i}>{optLabel(m)}</option>
              ))}
            </select>
          </label>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-xs text-dusk">Initial {tokA.symbol}</span>
            <input
              inputMode="decimal"
              value={aStr}
              onChange={(e) => setAStr(e.target.value)}
              className="rounded-xl border border-line bg-night/40 p-2.5 font-mono text-lg text-starlight outline-none tnum"
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs text-dusk">Initial {tokB.symbol}</span>
            <input
              inputMode="decimal"
              value={bStr}
              onChange={(e) => setBStr(e.target.value)}
              className="rounded-xl border border-line bg-night/40 p-2.5 font-mono text-lg text-starlight outline-none tnum"
            />
          </label>
        </div>

        <label className="flex flex-col gap-1">
          <span className="text-xs text-dusk">Swap fee (bps) · 30 = 0.30%</span>
          <input
            inputMode="numeric"
            value={feeStr}
            onChange={(e) => setFeeStr(e.target.value)}
            className="w-32 rounded-xl border border-line bg-night/40 p-2.5 font-mono text-lg text-starlight outline-none tnum"
          />
        </label>

        <p className="text-xs text-dusk">
          The initial deposit sets the starting price (ratio of the two amounts). Protocol fee share
          is 20% of the fee. A minimum-liquidity amount is permanently locked on the first deposit.
        </p>

        {validation && exists && (
          <p className="text-xs text-star">This pair already has a pool — pick a different combination.</p>
        )}
        {validation && !exists && <p className="text-xs text-rose-300">{validation}</p>}

        <Button onClick={onCreate} disabled={busy || (connected && !!validation)}>
          {!connected ? "Connect wallet" : busy ? "Creating…" : "Create pool + seed"}
        </Button>
      </Card>

      {created && (
        <Card className="mt-4 flex flex-col gap-2 p-5">
          <span className="flex items-center gap-2 text-sm text-meridian">
            <CheckCircle2 className="h-4 w-4" /> Pool live on devnet
          </span>
          <dl className="space-y-1 text-xs text-dusk">
            <Row k="Pool" v={created.pool} mono />
            <Row k={`Reserve ${tokA.symbol}`} v={formatAmount(created.reserveA, tokA.decimals)} />
            <Row k={`Reserve ${tokB.symbol}`} v={formatAmount(created.reserveB, tokB.decimals)} />
          </dl>
          <p className="text-xs text-dusk">
            The pool and all its accounts (reserves, LP mint) are PDAs derived from the pair — it's a
            real, tradable market now.
          </p>
        </Card>
      )}
    </div>
  );
}

function Row({ k, v, mono }: { k: string; v: string; mono?: boolean }) {
  return (
    <div className="flex justify-between gap-3">
      <dt>{k}</dt>
      <dd className={mono ? "break-all font-mono text-starlight" : "font-mono text-starlight tnum"}>{v}</dd>
    </div>
  );
}
