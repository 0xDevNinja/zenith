import { Plus, Search } from "lucide-react";
import { Card, Eyebrow } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { LiquidityArc } from "@/components/LiquidityArc";
import { POOLS } from "@/lib/mock";
import { fmtUsd } from "@/lib/utils";

export function Pools() {
  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-10 animate-rise">
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <Eyebrow>Star catalog</Eyebrow>
          <h1 className="mt-1 font-display text-4xl text-starlight">Pools</h1>
        </div>
        <div className="flex items-center gap-2">
          <label className="flex h-10 items-center gap-2 rounded-xl border border-line bg-panel/60 px-3 text-sm text-dusk focus-within:border-star/40">
            <Search className="h-4 w-4" />
            <input
              placeholder="Search pair"
              className="w-32 bg-transparent text-starlight outline-none placeholder:text-dusk/60"
            />
          </label>
          <Button size="md">
            <Plus className="h-4 w-4" />
            New pool
          </Button>
        </div>
      </div>

      <Card className="overflow-hidden">
        <div className="grid grid-cols-[1.4fr_1fr_1fr_0.7fr_1.2fr] items-center gap-4 border-b border-line/50 px-5 py-3 text-[11px] uppercase tracking-[0.16em] text-dusk">
          <span>Pair</span>
          <span className="text-right">Liquidity</span>
          <span className="text-right">24h Volume</span>
          <span className="text-right">Fee</span>
          <span className="text-right">Depth</span>
        </div>

        {POOLS.map((p) => (
          <button
            key={p.pair}
            className="grid w-full grid-cols-[1.4fr_1fr_1fr_0.7fr_1.2fr] items-center gap-4 border-b border-line/30 px-5 py-4 text-left transition-colors last:border-0 hover:bg-panel-2/40"
          >
            <span className="flex items-center gap-3">
              <PairGlyph base={p.base} quote={p.quote} />
              <span className="font-medium text-starlight">{p.pair}</span>
            </span>
            <span className="text-right font-mono tnum text-starlight">{fmtUsd(p.liquidityUsd)}</span>
            <span className="text-right font-mono tnum text-dusk">{fmtUsd(p.volume24hUsd)}</span>
            <span className="text-right font-mono tnum text-star">{(p.feeBps / 100).toFixed(2)}%</span>
            <span className="ml-auto h-9 w-28">
              <LiquidityArc
                activeAt={p.activeAt}
                width={120}
                height={40}
                sigma={0.2}
                bare
                animate={false}
                className="h-full w-full"
              />
            </span>
          </button>
        ))}
      </Card>
    </div>
  );
}

function PairGlyph({ base, quote }: { base: string; quote: string }) {
  return (
    <span className="flex">
      <span className="grid h-7 w-7 place-items-center rounded-full border border-night bg-gradient-to-br from-star/40 to-star/10 font-mono text-[10px] font-semibold text-starlight">
        {base.slice(0, 2)}
      </span>
      <span className="-ml-2 grid h-7 w-7 place-items-center rounded-full border border-night bg-gradient-to-br from-meridian/40 to-meridian/10 font-mono text-[10px] font-semibold text-starlight">
        {quote.slice(0, 2)}
      </span>
    </span>
  );
}
