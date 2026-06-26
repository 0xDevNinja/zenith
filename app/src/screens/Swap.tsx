import { useState } from "react";
import { ArrowDown, ChevronDown, Settings2, TrendingUp } from "lucide-react";
import { Card, Eyebrow } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { DepthChart } from "@/components/DepthChart";
import { TOKENS } from "@/lib/mock";
import { cn } from "@/lib/utils";

const RATE = 94.86; // mock SOL→USDC

export function Swap() {
  const [payAmt, setPayAmt] = useState("1.5");
  const pay = parseFloat(payAmt) || 0;
  const receive = pay * RATE;
  const feeBps = 30;
  const feeAmt = receive * (feeBps / 10000);
  const impactBps = 11;
  const minReceive = (receive - feeAmt) * 0.995;

  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-8 sm:pt-12 animate-rise">
      <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
        <div>
          <Eyebrow>Spot · Devnet</Eyebrow>
          <h1 className="mt-1 font-display text-4xl leading-none text-starlight">SOL / USDC</h1>
        </div>
        <div className="flex items-center gap-6">
          <Stat label="Price" value={`$${RATE}`} />
          <Stat label="24h" value="+2.41%" accent="meridian" />
          <Stat label="24h Volume" value="$842K" />
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
            <DepthChart activeAt={0.52} width={560} height={250} className="h-full min-h-[250px] w-full" />
          </div>
          <div className="mt-3 grid grid-cols-3 gap-3 border-t border-line/40 pt-3">
            <MiniStat label="Active tick" value="94.86" tone="meridian" />
            <MiniStat label="In-range TVL" value="$1.24M" />
            <MiniStat label="Bin step" value="0.30%" tone="star" />
          </div>
        </Card>

        {/* Instrument */}
        <div>
          <div className="mb-3 flex items-center justify-between">
            <Eyebrow>Swap</Eyebrow>
            <button className="text-dusk transition-colors hover:text-starlight" aria-label="Settings">
              <Settings2 className="h-4 w-4" />
            </button>
          </div>

          <Card className="p-2">
            <Field label="You pay" amount={payAmt} onAmount={setPayAmt} token="SOL" balance="8.42" editable />
            <div className="relative h-0">
              <div className="absolute left-1/2 top-1/2 z-10 -translate-x-1/2 -translate-y-1/2">
                <button
                  className="grid h-9 w-9 place-items-center rounded-xl border border-line bg-panel-2 text-meridian shadow-lg transition-transform hover:rotate-180"
                  aria-label="Flip direction"
                >
                  <ArrowDown className="h-4 w-4" />
                </button>
              </div>
            </div>
            <Field
              label="You receive"
              amount={receive.toLocaleString("en-US", { maximumFractionDigits: 2 })}
              token="USDC"
              balance="0.00"
            />
          </Card>

          <Card className="mt-2 px-4 py-1 text-sm">
            <div className="divide-y divide-line/30">
              <Row label="Rate" value={`1 SOL = ${RATE} USDC`} />
              <Row label="Network fee" value={`${(feeBps / 100).toFixed(2)}%`} hint={`${feeAmt.toFixed(2)} USDC`} />
              <Row label="Price impact" value={`${(impactBps / 100).toFixed(2)}%`} tone="ok" />
              <Row label="Min. received" value={`${minReceive.toFixed(2)} USDC`} />
            </div>
          </Card>

          <Button size="lg" className="mt-3 w-full text-base">
            Swap
          </Button>
        </div>
      </div>
    </div>
  );
}

function Field({
  label,
  amount,
  onAmount,
  token,
  balance,
  editable,
}: {
  label: string;
  amount: string;
  onAmount?: (v: string) => void;
  token: keyof typeof TOKENS;
  balance: string;
  editable?: boolean;
}) {
  return (
    <div className="well rounded-2xl px-4 py-3.5">
      <div className="mb-1 flex items-center justify-between text-xs text-dusk">
        <span>{label}</span>
        <span className="font-mono tnum">
          Balance {balance}
          {editable && <button className="ml-1.5 text-star hover:text-[#f6d18e]">Max</button>}
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
        <button className="flex shrink-0 items-center gap-2 rounded-full border border-line bg-panel-2/80 py-1.5 pl-1.5 pr-3 transition-colors hover:border-star/40">
          <span className="grid h-7 w-7 place-items-center rounded-full bg-gradient-to-br from-star/40 to-meridian/20 font-mono text-[11px] font-semibold text-starlight">
            {token.slice(0, 2)}
          </span>
          <span className="font-medium">{token}</span>
          <ChevronDown className="h-4 w-4 text-dusk" />
        </button>
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

function Stat({ label, value, accent }: { label: string; value: string; accent?: "meridian" | "star" }) {
  return (
    <div className="text-right">
      <div className="text-[11px] uppercase tracking-wider text-dusk">{label}</div>
      <div
        className={cn(
          "mt-0.5 font-mono text-lg tnum",
          accent === "meridian" ? "text-meridian" : accent === "star" ? "text-star" : "text-starlight",
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
