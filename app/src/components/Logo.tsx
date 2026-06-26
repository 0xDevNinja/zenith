import { useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";

// Logo: an apex caret — a peak rising to its highest point, crowned by the
// active-price star. "Zenith" read at a glance; the aqua peak is the curve,
// the gold star is the price at its summit.
export function Zenithmark({ size = 30, animate = true }: { size?: number; animate?: boolean }) {
  const { palette } = useTheme();
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" fill="none" aria-hidden>
      <path
        d="M8 46 L32 21 L56 46"
        fill="none"
        stroke={palette.zenith}
        strokeWidth="7"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="32" cy="13" r="6" fill={palette.logoTop} />
      {animate && (
        <circle
          cx="32"
          cy="13"
          r="6"
          fill={palette.logoTop}
          opacity="0.4"
          className="animate-zenith-pulse"
          style={{ transformBox: "fill-box", transformOrigin: "center" }}
        />
      )}
    </svg>
  );
}

export function Wordmark({ className, size = 30 }: { className?: string; size?: number }) {
  return (
    <span className={cn("flex items-center gap-2.5", className)}>
      <Zenithmark size={size} />
      <span className="font-display text-2xl leading-none tracking-wide text-starlight">Zenith</span>
    </span>
  );
}
