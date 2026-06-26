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
