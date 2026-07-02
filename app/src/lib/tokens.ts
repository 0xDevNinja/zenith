// Raw <-> human amount conversion for SPL tokens. Raw amounts are integer base
// units (bigint); UI amounts are decimal strings/numbers.

export function parseAmount(value: string, decimals: number): bigint | null {
  const v = value.trim();
  if (!v || !/^\d*\.?\d*$/.test(v)) return null;
  const [whole = "0", frac = ""] = v.split(".");
  if (frac.length > decimals) return null; // more precision than the token has
  const padded = (whole + frac.padEnd(decimals, "0")) || "0";
  try {
    return BigInt(padded);
  } catch {
    return null;
  }
}

// Plain decimal string (no grouping) — safe to feed back into parseAmount,
// e.g. for a "Max" button.
export function formatPlain(raw: bigint, decimals: number): string {
  const base = 10n ** BigInt(decimals);
  const whole = raw / base;
  const frac = (raw % base).toString().padStart(decimals, "0").replace(/0+$/, "");
  return frac ? `${whole}.${frac}` : `${whole}`;
}

export function formatAmount(raw: bigint, decimals: number, maxFrac = 6): string {
  const neg = raw < 0n;
  const abs = neg ? -raw : raw;
  const base = 10n ** BigInt(decimals);
  const whole = abs / base;
  const frac = abs % base;
  let fracStr = frac.toString().padStart(decimals, "0").slice(0, maxFrac).replace(/0+$/, "");
  const wholeStr = whole.toLocaleString("en-US");
  return `${neg ? "-" : ""}${wholeStr}${fracStr ? "." + fracStr : ""}`;
}

// Compact display for very large unitless quantities (e.g. concentrated-liquidity
// L, which lives at Q64 scale and can be 18+ digits). 1_234_567n -> "1.23M".
// Keeps two significant fractional digits; falls back to grouped digits below 1K.
export function formatCompact(raw: bigint): string {
  const neg = raw < 0n;
  const abs = neg ? -raw : raw;
  const sign = neg ? "-" : "";
  const tiers: { v: bigint; s: string }[] = [
    { v: 10n ** 18n, s: "E" },
    { v: 10n ** 15n, s: "Q" },
    { v: 10n ** 12n, s: "T" },
    { v: 10n ** 9n, s: "B" },
    { v: 10n ** 6n, s: "M" },
    { v: 10n ** 3n, s: "K" },
  ];
  for (const t of tiers) {
    if (abs >= t.v) {
      const scaled = (abs * 100n) / t.v; // two extra digits of precision
      const whole = scaled / 100n;
      const frac = (scaled % 100n).toString().padStart(2, "0").replace(/0+$/, "");
      return `${sign}${whole.toLocaleString("en-US")}${frac ? "." + frac : ""}${t.s}`;
    }
  }
  return `${sign}${abs.toString()}`;
}
