import { ONE_Q64, Rounding, SCALE_OFFSET, U128_MAX } from "./rounding.js";
import { mulShr, shlDiv } from "./u256.js";

/// Unsigned Q64.64 fixed-point number, value = `bits / 2^64`. Bit-exact port of
/// `zenith_math::Q64x64`: every lossy op takes an explicit [`Rounding`], and
/// fallible ops return `null` (overflow of the `u128` result or div-by-zero).
export class Q64 {
  readonly bits: bigint;

  private constructor(bits: bigint) {
    this.bits = bits;
  }

  /// The value `0.0`.
  static readonly ZERO = new Q64(0n);
  /// The value `1.0` (`2^64`).
  static readonly ONE = new Q64(ONE_Q64);
  /// The largest representable value (`2^64 - 2^-64`).
  static readonly MAX = new Q64(U128_MAX);

  /// Reinterpret raw Q64.64 bits. Throws if outside the `u128` range.
  static fromBits(bits: bigint): Q64 {
    if (bits < 0n || bits > U128_MAX) {
      throw new RangeError(`Q64 bits out of u128 range: ${bits}`);
    }
    return new Q64(bits);
  }

  /// Build from an integer `n` (`n.0`). `n` must fit `u64`.
  static fromInt(n: bigint): Q64 {
    return new Q64(n << SCALE_OFFSET);
  }

  /// Build from the ratio `a / b`, rounded as requested. `null` if `b == 0` or
  /// the result exceeds `u128`.
  static fromRatio(a: bigint, b: bigint, rounding: Rounding): Q64 | null {
    const v = shlDiv(a, SCALE_OFFSET, b, rounding);
    return v === null ? null : new Q64(v);
  }

  /// The raw Q64.64 bit pattern.
  toBits(): bigint {
    return this.bits;
  }

  /// `true` if exactly `0.0`.
  isZero(): boolean {
    return this.bits === 0n;
  }

  /// Integer part (floor).
  floorInt(): bigint {
    return this.bits >> SCALE_OFFSET;
  }

  /// Checked add. `null` on `u128` overflow.
  checkedAdd(rhs: Q64): Q64 | null {
    const s = this.bits + rhs.bits;
    return s > U128_MAX ? null : new Q64(s);
  }

  /// Checked sub. `null` on underflow.
  checkedSub(rhs: Q64): Q64 | null {
    return rhs.bits > this.bits ? null : new Q64(this.bits - rhs.bits);
  }

  /// Saturating add: clamps to [`Q64.MAX`].
  saturatingAdd(rhs: Q64): Q64 {
    const s = this.bits + rhs.bits;
    return new Q64(s > U128_MAX ? U128_MAX : s);
  }

  /// Saturating sub: clamps to [`Q64.ZERO`].
  saturatingSub(rhs: Q64): Q64 {
    return new Q64(rhs.bits > this.bits ? 0n : this.bits - rhs.bits);
  }

  /// Multiply two Q64.64 values (256-bit intermediate, `>> 64`). `null` if the
  /// scaled result exceeds `u128`.
  mul(rhs: Q64, rounding: Rounding): Q64 | null {
    const v = mulShr(this.bits, rhs.bits, SCALE_OFFSET, rounding);
    return v === null ? null : new Q64(v);
  }

  /// Divide `self / rhs`. `null` if `rhs` is zero or the result exceeds `u128`.
  div(rhs: Q64, rounding: Rounding): Q64 | null {
    const v = shlDiv(this.bits, SCALE_OFFSET, rhs.bits, rounding);
    return v === null ? null : new Q64(v);
  }

  /// Reciprocal `1 / self`. `null` if `self` is zero or the result exceeds `u128`.
  recip(rounding: Rounding): Q64 | null {
    const v = shlDiv(1n, 2n * SCALE_OFFSET, this.bits, rounding);
    return v === null ? null : new Q64(v);
  }

  /// `self * amount` as an integer. `null` on overflow.
  mulInt(amount: bigint, rounding: Rounding): bigint | null {
    return mulShr(this.bits, amount, SCALE_OFFSET, rounding);
  }

  /// `amount / self` as an integer (inverse of [`mulInt`]). `null` if `self` is
  /// zero or the result exceeds `u128`.
  divInt(amount: bigint, rounding: Rounding): bigint | null {
    return shlDiv(amount, SCALE_OFFSET, this.bits, rounding);
  }

  /// Value equality.
  eq(other: Q64): boolean {
    return this.bits === other.bits;
  }
}
