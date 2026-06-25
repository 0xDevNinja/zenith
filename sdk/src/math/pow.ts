import { Q64 } from "./q64.js";
import { Rounding } from "./rounding.js";

/// Raise a Q64.64 `base` to a signed integer power via binary exponentiation.
/// `exp === 0` yields exactly `1.0`; negative exponents return the reciprocal of
/// the positive power. The same `rounding` is applied to every intermediate
/// multiply and the final reciprocal. `null` if any intermediate or the result
/// overflows Q64.64. Bit-exact port of `zenith_math::pow`.
export function pow(base: Q64, exp: number, rounding: Rounding): Q64 | null {
  let result: Q64 = Q64.ONE;
  let b = base;
  let e = Math.abs(exp);
  while (e > 0) {
    if (e & 1) {
      const r = result.mul(b, rounding);
      if (r === null) return null;
      result = r;
    }
    e >>>= 1;
    if (e > 0) {
      const sq = b.mul(b, rounding);
      if (sq === null) return null;
      b = sq;
    }
  }
  if (exp < 0) return result.recip(rounding);
  return result;
}
