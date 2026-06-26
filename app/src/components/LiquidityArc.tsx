import { useId } from "react";
import { useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";

interface LiquidityArcProps {
  /** Where the active price sits along the curve, 0..1 (0.5 = the crest). */
  activeAt?: number;
  /** in-range → cyan meridian, out-of-range → dim. */
  inRange?: boolean;
  width?: number;
  height?: number;
  /** Narrowness of the concentration hump; smaller = more concentrated. */
  sigma?: number;
  className?: string;
  /** Hide the dropped meridian line + label (used for tiny table sparklines). */
  bare?: boolean;
  animate?: boolean;
}

// The signature element: concentrated liquidity drawn as a luminous arc across
// a star-chart, with the active price marked as a bright point — the zenith.
export function LiquidityArc({
  activeAt = 0.5,
  inRange = true,
  width = 520,
  height = 200,
  sigma = 0.17,
  className,
  bare = false,
  animate = true,
}: LiquidityArcProps) {
  const { palette: p } = useTheme();
  const uid = useId().replace(/:/g, "");
  const padX = 6;
  const padTop = 18;
  const baseline = height - 8;
  const amp = baseline - padTop;
  const usable = width - padX * 2;

  const gauss = (t: number) => Math.exp(-((t - 0.5) ** 2) / (2 * sigma * sigma));
  const norm = gauss(0.5); // peak value, so the crest hits padTop exactly
  const xAt = (t: number) => padX + t * usable;
  const yAt = (t: number) => baseline - (gauss(t) / norm) * amp;

  const N = 96;
  const pts: string[] = [];
  for (let i = 0; i <= N; i++) {
    const t = i / N;
    pts.push(`${xAt(t).toFixed(2)},${yAt(t).toFixed(2)}`);
  }
  const line = `M ${pts.join(" L ")}`;
  const area = `${line} L ${xAt(1).toFixed(2)},${baseline} L ${xAt(0).toFixed(2)},${baseline} Z`;

  const mx = xAt(activeAt);
  const my = yAt(activeAt);
  const meridianColor = inRange ? p.rangeIn : p.rangeOut;

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      width="100%"
      preserveAspectRatio="none"
      className={cn("overflow-visible", className)}
      role="img"
      aria-label="Liquidity depth, active price marked at the zenith"
    >
      <defs>
        <linearGradient id={`fill-${uid}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={p.barTop} stopOpacity="0.28" />
          <stop offset="55%" stopColor={p.barTop} stopOpacity="0.08" />
          <stop offset="100%" stopColor={p.barTop} stopOpacity="0" />
        </linearGradient>
        <linearGradient id={`stroke-${uid}`} x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stopColor={p.barBottom} stopOpacity="0.5" />
          <stop offset="50%" stopColor={p.env} stopOpacity="1" />
          <stop offset="100%" stopColor={p.barBottom} stopOpacity="0.5" />
        </linearGradient>
        <filter id={`glow-${uid}`} x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="3" result="b" />
          <feMerge>
            <feMergeNode in="b" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>

      <path d={area} fill={`url(#fill-${uid})`} />

      {!bare && (
        <line
          x1={mx}
          y1={my}
          x2={mx}
          y2={baseline}
          stroke={meridianColor}
          strokeOpacity="0.55"
          strokeWidth="1"
          strokeDasharray="2 4"
        />
      )}

      <path
        d={line}
        fill="none"
        stroke={`url(#stroke-${uid})`}
        strokeWidth="2"
        strokeLinecap="round"
        pathLength={1}
        strokeDasharray={1}
        style={
          animate
            ? { animation: "arc-draw 1.4s ease-out both" }
            : undefined
        }
      />

      {/* The zenith: active price, a pulsing star on the curve. */}
      <g filter={`url(#glow-${uid})`} style={{ transformOrigin: `${mx}px ${my}px` }}>
        <circle
          cx={mx}
          cy={my}
          r={4.5}
          fill={inRange ? p.active : p.activeOut}
          className={animate ? "animate-zenith-pulse" : undefined}
          style={{ transformBox: "fill-box", transformOrigin: "center" }}
        />
        <circle cx={mx} cy={my} r={1.6} fill={p.centerDot} />
      </g>
    </svg>
  );
}
