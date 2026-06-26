import { useId } from "react";
import { useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";

interface DepthChartProps {
  /** Active price position across the band, 0..1. */
  activeAt?: number;
  /** Optional provided-range shading [lo, hi] in 0..1, for position views. */
  range?: [number, number];
  inRange?: boolean;
  width?: number;
  height?: number;
  bins?: number;
  /** Concentration tightness; smaller = liquidity piled tighter at price. */
  sigma?: number;
  className?: string;
  animate?: boolean;
  /** Show the bottom price axis label. */
  axis?: boolean;
}

// Deterministic jitter so bars look like real, uneven liquidity (no RNG → the
// chart is stable across renders and screenshots).
function jitter(i: number): number {
  const s = Math.sin(i * 12.9898) * 43758.5453;
  return s - Math.floor(s); // 0..1
}

// The hero: concentrated liquidity as a heatmap depth chart. Bars are taller
// and hotter (gold) where liquidity piles up; the active price is the bright
// cyan meridian — the zenith.
export function DepthChart({
  activeAt = 0.5,
  range,
  inRange = true,
  width = 560,
  height = 230,
  bins = 56,
  sigma = 0.18,
  className,
  animate = true,
  axis = true,
}: DepthChartProps) {
  const { palette: p } = useTheme();
  const uid = useId().replace(/:/g, "");
  const padX = 4;
  const padTop = 16;
  const baseline = height - (axis ? 22 : 8);
  const amp = baseline - padTop;
  const usable = width - padX * 2;
  const gap = 2;
  const bw = usable / bins - gap;

  const gauss = (t: number) => Math.exp(-((t - 0.5) ** 2) / (2 * sigma * sigma));
  const norm = gauss(0.5);

  const bars = Array.from({ length: bins }, (_, i) => {
    const t = (i + 0.5) / bins;
    const base = gauss(t) / norm; // 0..1
    const h = Math.max(0.04, base * (0.78 + 0.22 * jitter(i)));
    return { t, x: padX + i * (bw + gap), h, heat: base };
  });

  const ax = padX + activeAt * usable;

  // Smooth curve over the bar tops for a crisp liquidity envelope.
  const curve = bars
    .map((b, i) => `${i === 0 ? "M" : "L"} ${(b.x + bw / 2).toFixed(1)} ${(baseline - b.h * amp).toFixed(1)}`)
    .join(" ");

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      width="100%"
      className={cn("overflow-visible", className)}
      preserveAspectRatio="none"
      role="img"
      aria-label="Liquidity depth by price; active price marked at the zenith"
    >
      <defs>
        <linearGradient id={`bar-${uid}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={p.barTop} />
          <stop offset="100%" stopColor={p.barBottom} stopOpacity="0.25" />
        </linearGradient>
        <linearGradient id={`env-${uid}`} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stopColor={p.barBottom} stopOpacity="0.4" />
          <stop offset="50%" stopColor={p.env} />
          <stop offset="100%" stopColor={p.barBottom} stopOpacity="0.4" />
        </linearGradient>
        <filter id={`g-${uid}`} x="-40%" y="-40%" width="180%" height="180%">
          <feGaussianBlur stdDeviation="2.4" result="b" />
          <feMerge>
            <feMergeNode in="b" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      {/* provided range shading */}
      {range && (
        <rect
          x={padX + range[0] * usable}
          y={padTop - 6}
          width={(range[1] - range[0]) * usable}
          height={baseline - padTop + 6}
          fill={inRange ? p.rangeIn : p.rangeOut}
          fillOpacity="0.08"
        />
      )}

      {/* liquidity bars — opacity tracks concentration heat */}
      {bars.map((b, i) => (
        <rect
          key={i}
          x={b.x}
          y={baseline - b.h * amp}
          width={bw}
          height={b.h * amp}
          rx={Math.min(1.5, bw / 3)}
          fill={`url(#bar-${uid})`}
          opacity={0.25 + b.heat * 0.7}
          style={
            animate
              ? {
                  transformBox: "fill-box",
                  transformOrigin: "bottom",
                  animation: `rise 0.5s ${(i / bins) * 0.4}s ease-out both`,
                }
              : undefined
          }
        />
      ))}

      {/* envelope curve */}
      <path d={curve} fill="none" stroke={`url(#env-${uid})`} strokeWidth="1.5" strokeLinejoin="round" opacity="0.9" />

      {/* baseline */}
      <line x1={padX} y1={baseline} x2={width - padX} y2={baseline} stroke={p.baseline} strokeWidth="1" />

      {/* active price meridian + zenith point */}
      <line
        x1={ax}
        y1={padTop - 8}
        x2={ax}
        y2={baseline}
        stroke={inRange ? p.active : p.activeOut}
        strokeWidth="1.25"
        strokeDasharray="3 4"
        opacity="0.8"
      />
      <g filter={`url(#g-${uid})`}>
        <circle
          cx={ax}
          cy={baseline - bars[Math.min(bins - 1, Math.round(activeAt * bins))].h * amp}
          r="4.5"
          fill={inRange ? p.active : p.activeOut}
          className={animate ? "animate-zenith-pulse" : undefined}
          style={{ transformBox: "fill-box", transformOrigin: "center" }}
        />
      </g>

      {/* price axis ticks */}
      {axis && [0.06, 0.5, 0.94].map((t, i) => (
        <text
          key={i}
          x={padX + t * usable}
          y={height - 5}
          textAnchor={i === 0 ? "start" : i === 2 ? "end" : "middle"}
          fill={p.axis}
          fontSize="9"
          fontFamily="Geist Mono, monospace"
        >
          {i === 1 ? "price" : ""}
        </text>
      ))}
    </svg>
  );
}
