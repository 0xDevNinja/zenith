import { Card, Eyebrow } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { DepthChart } from "@/components/DepthChart";
import { POSITIONS } from "@/lib/mock";
import { fmtUsd, cn } from "@/lib/utils";

export function Positions() {
  const totalLiq = POSITIONS.reduce((s, p) => s + p.liquidityUsd, 0);
  const totalFees = POSITIONS.reduce((s, p) => s + p.feesEarnedUsd, 0);

  return (
    <div className="mx-auto max-w-5xl px-5 pb-24 pt-10 animate-rise">
      <div className="mb-6">
        <Eyebrow>Your constellations</Eyebrow>
        <h1 className="mt-1 font-display text-4xl text-starlight">Positions</h1>
      </div>

      <div className="mb-6 grid grid-cols-2 gap-3 sm:max-w-md">
        <Stat label="Total liquidity" value={fmtUsd(totalLiq)} />
        <Stat label="Unclaimed fees" value={fmtUsd(totalFees)} accent />
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        {POSITIONS.map((p) => (
          <Card key={p.pair} className="overflow-hidden p-5">
            <div className="mb-4 flex items-start justify-between">
              <div>
                <h3 className="font-display text-2xl text-starlight">{p.pair}</h3>
                <span
                  className={cn(
                    "mt-1 inline-flex items-center gap-1.5 text-xs font-medium",
                    p.inRange ? "text-meridian" : "text-dusk",
                  )}
                >
                  <span
                    className={cn(
                      "h-1.5 w-1.5 rounded-full",
                      p.inRange ? "bg-meridian shadow-[0_0_8px] shadow-meridian" : "bg-dusk",
                    )}
                  />
                  {p.inRange ? "In range" : "Out of range"}
                </span>
              </div>
              <span className="font-mono text-xs text-dusk">{(p.activeAt * 100).toFixed(0)}% across band</span>
            </div>

            <div className="mb-4 h-20 w-full">
              <DepthChart
                activeAt={p.activeAt}
                inRange={p.inRange}
                range={p.inRange ? [0.18, 0.82] : [0.1, 0.62]}
                width={320}
                height={84}
                bins={30}
                sigma={0.24}
                animate={false}
                axis={false}
                className="h-full w-full"
              />
            </div>

            <div className="mb-4 grid grid-cols-2 gap-3 border-t border-line/40 pt-4">
              <Stat label="Liquidity" value={fmtUsd(p.liquidityUsd)} compact />
              <Stat label="Fees earned" value={fmtUsd(p.feesEarnedUsd)} accent compact />
            </div>

            <div className="flex gap-2">
              <Button variant="gold" size="sm" className="flex-1">
                Claim fees
              </Button>
              <Button variant="outline" size="sm" className="flex-1">
                Manage
              </Button>
            </div>
          </Card>
        ))}
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  accent,
  compact,
}: {
  label: string;
  value: string;
  accent?: boolean;
  compact?: boolean;
}) {
  return (
    <div className={cn(!compact && "rounded-xl border border-line/60 bg-panel/60 p-4")}>
      <div className="text-xs text-dusk">{label}</div>
      <div
        className={cn(
          "mt-1 font-mono tnum",
          compact ? "text-lg" : "text-2xl",
          accent ? "text-star" : "text-starlight",
        )}
      >
        {value}
      </div>
    </div>
  );
}
