//! Q64.64 fixed-point number.
//!
//! A `Q64x64` is an unsigned fixed-point value backed by a `u128`: the high 64
//! bits are the integer part and the low 64 bits are the fraction. The numeric
//! value is `bits / 2^64`. This is the representation both programs use for
//! sqrt price (AMM) and bin price (DLMM).
//!
//! Design rules:
//! - Multiplication and division go through a 256-bit intermediate so the
//!   `<< 64` scaling never overflows mid-computation.
//! - Every lossy op (`mul`, `div`, `recip`, amount conversions) takes an
//!   explicit [`Rounding`] so callers choose the protocol-favoring direction.
//! - Fallible ops return `Option`; `None` means overflow of the `u128` result
//!   or division by zero. Nothing overflows silently.

use ruint::aliases::U256;

use crate::{Rounding, SCALE_OFFSET};

/// `2^64`, the value of `1.0` in Q64.64.
const ONE_BITS: u128 = 1u128 << SCALE_OFFSET;

/// Unsigned Q64.64 fixed-point number. Value = `bits / 2^64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Q64x64(u128);

/// Narrow a `U256` back to `u128`, returning `None` if it does not fit.
#[inline]
fn to_u128(x: U256) -> Option<u128> {
    let limbs = x.as_limbs();
    if limbs[2] != 0 || limbs[3] != 0 {
        return None;
    }
    Some((limbs[0] as u128) | ((limbs[1] as u128) << 64))
}

impl Q64x64 {
    /// The value `0.0`.
    pub const ZERO: Self = Self(0);
    /// The value `1.0` (`2^64`).
    pub const ONE: Self = Self(ONE_BITS);
    /// The largest representable value (`2^64 - 2^-64`).
    pub const MAX: Self = Self(u128::MAX);

    // --- constructors ---

    /// Build from an integer. `n` becomes `n.0` exactly; never overflows since
    /// `n < 2^64` and the result occupies the high 64 bits.
    #[inline]
    pub const fn from_int(n: u64) -> Self {
        Self((n as u128) << SCALE_OFFSET)
    }

    /// Build from the ratio `a / b`, rounded as requested.
    /// Returns `None` if `b == 0` or the result exceeds `u128`.
    #[inline]
    pub fn from_ratio(a: u128, b: u128, rounding: Rounding) -> Option<Self> {
        if b == 0 {
            return None;
        }
        let num = U256::from(a) << SCALE_OFFSET; // a * 2^64: <= 192 bits, fits U256
        let den = U256::from(b);
        let q = div_round(num, den, rounding);
        to_u128(q).map(Self)
    }

    /// Reinterpret raw Q64.64 bits.
    #[inline]
    pub const fn from_bits(bits: u128) -> Self {
        Self(bits)
    }

    /// The raw Q64.64 bit pattern.
    #[inline]
    pub const fn to_bits(self) -> u128 {
        self.0
    }

    // --- inspection ---

    /// `true` if the value is exactly `0.0`.
    #[inline]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Integer part (floor), dropping the fraction.
    #[inline]
    pub const fn floor_int(self) -> u128 {
        self.0 >> SCALE_OFFSET
    }

    // --- additive ops ---

    /// Checked add. `None` on overflow.
    #[inline]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Checked sub. `None` on underflow (result would be negative).
    #[inline]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// Saturating add: clamps to [`Q64x64::MAX`] instead of overflowing.
    #[inline]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Saturating sub: clamps to [`Q64x64::ZERO`] instead of underflowing.
    #[inline]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    // --- multiplicative ops ---

    /// Multiply two Q64.64 values. The product is computed in 256 bits and
    /// shifted right by 64; the dropped fraction is rounded as requested.
    /// `None` if the scaled result exceeds `u128`.
    #[inline]
    pub fn mul(self, rhs: Self, rounding: Rounding) -> Option<Self> {
        // (a/2^64) * (b/2^64) = (a*b)/2^128, stored as bits = a*b/2^64.
        // (2^128-1)^2 < 2^256, so the U256 product can never overflow/panic.
        let prod = U256::from(self.0) * U256::from(rhs.0);
        let q = shr_round(prod, SCALE_OFFSET, rounding);
        to_u128(q).map(Self)
    }

    /// Divide `self / rhs`, rounded as requested.
    /// `None` if `rhs` is zero or the result exceeds `u128`.
    #[inline]
    pub fn div(self, rhs: Self, rounding: Rounding) -> Option<Self> {
        if rhs.0 == 0 {
            return None;
        }
        let num = U256::from(self.0) << SCALE_OFFSET; // a * 2^64: 128+64 = 192 bits, fits U256
        let den = U256::from(rhs.0);
        let q = div_round(num, den, rounding);
        to_u128(q).map(Self)
    }

    /// Reciprocal `1 / self`, rounded as requested.
    /// `None` if `self` is zero or the result exceeds `u128`.
    #[inline]
    pub fn recip(self, rounding: Rounding) -> Option<Self> {
        if self.0 == 0 {
            return None;
        }
        let num = U256::from(1u128) << (2 * SCALE_OFFSET); // 2^128 (129 bits), fits U256
        let den = U256::from(self.0);
        let q = div_round(num, den, rounding);
        to_u128(q).map(Self)
    }

    // --- token-amount conversions ---

    /// Multiply this value by an integer token `amount`, returning an integer
    /// (e.g. price * base amount -> quote amount). Rounded as requested.
    /// `None` if the result exceeds `u128`.
    #[inline]
    pub fn mul_int(self, amount: u128, rounding: Rounding) -> Option<u128> {
        // (2^128-1)^2 < 2^256, so the U256 product can never overflow/panic.
        let prod = U256::from(self.0) * U256::from(amount);
        let q = shr_round(prod, SCALE_OFFSET, rounding);
        to_u128(q)
    }

    /// Divide an integer token `amount` by this value, returning an integer
    /// (the inverse of [`mul_int`]). Rounded as requested.
    /// `None` if `self` is zero or the result exceeds `u128`.
    #[inline]
    pub fn div_int(self, amount: u128, rounding: Rounding) -> Option<u128> {
        if self.0 == 0 {
            return None;
        }
        let num = U256::from(amount) << SCALE_OFFSET; // amount * 2^64
        let den = U256::from(self.0);
        let q = div_round(num, den, rounding);
        to_u128(q)
    }
}

/// `num / den` over `U256`, applying the rounding direction to a nonzero remainder.
#[inline]
fn div_round(num: U256, den: U256, rounding: Rounding) -> U256 {
    let q = num / den;
    match rounding {
        Rounding::Up if num % den != U256::ZERO => q + U256::from(1u128),
        _ => q,
    }
}

/// `x >> shift` over `U256`, applying the rounding direction to dropped bits.
/// Equivalent to dividing by `2^shift`; shares the single rounding decision in
/// [`div_round`] so there is only one copy of the rounding logic to audit.
#[inline]
fn shr_round(x: U256, shift: u32, rounding: Rounding) -> U256 {
    div_round(x, U256::from(1u128) << (shift as usize), rounding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const D: Rounding = Rounding::Down;
    const U: Rounding = Rounding::Up;

    #[test]
    fn constants() {
        assert_eq!(Q64x64::ZERO.to_bits(), 0);
        assert_eq!(Q64x64::ONE.to_bits(), 1u128 << 64);
        assert_eq!(Q64x64::from_int(1), Q64x64::ONE);
        assert!(Q64x64::ZERO.is_zero());
        assert_eq!(Q64x64::from_int(5).floor_int(), 5);
    }

    #[test]
    fn from_ratio_basic() {
        // 1/2 = 0.5 -> high bit of the fraction set.
        let half = Q64x64::from_ratio(1, 2, D).unwrap();
        assert_eq!(half.to_bits(), 1u128 << 63);
        // 3/4
        let q = Q64x64::from_ratio(3, 4, D).unwrap();
        assert_eq!(q.to_bits(), 3u128 << 62);
        // divide by zero
        assert_eq!(Q64x64::from_ratio(1, 0, D), None);
    }

    #[test]
    fn from_ratio_rounding() {
        // 1/3 is not exact; Up must exceed Down by one ulp.
        let down = Q64x64::from_ratio(1, 3, D).unwrap();
        let up = Q64x64::from_ratio(1, 3, U).unwrap();
        assert_eq!(up.to_bits() - down.to_bits(), 1);
        // exact ratio: Up == Down.
        assert_eq!(
            Q64x64::from_ratio(1, 2, U).unwrap(),
            Q64x64::from_ratio(1, 2, D).unwrap()
        );
    }

    #[test]
    fn add_sub() {
        let a = Q64x64::from_int(3);
        let b = Q64x64::from_int(4);
        assert_eq!(a.checked_add(b).unwrap(), Q64x64::from_int(7));
        assert_eq!(b.checked_sub(a).unwrap(), Q64x64::from_int(1));
        // underflow
        assert_eq!(a.checked_sub(b), None);
        // overflow
        assert_eq!(Q64x64::MAX.checked_add(Q64x64::from_bits(1)), None);
        // saturating
        assert_eq!(Q64x64::MAX.saturating_add(Q64x64::ONE), Q64x64::MAX);
        assert_eq!(Q64x64::ZERO.saturating_sub(Q64x64::ONE), Q64x64::ZERO);
    }

    #[test]
    fn mul_identity_and_values() {
        let x = Q64x64::from_ratio(7, 3, D).unwrap();
        // x * 1 == x
        assert_eq!(x.mul(Q64x64::ONE, D).unwrap(), x);
        // 0.5 * 0.5 == 0.25
        let half = Q64x64::from_ratio(1, 2, D).unwrap();
        let quarter = Q64x64::from_ratio(1, 4, D).unwrap();
        assert_eq!(half.mul(half, D).unwrap(), quarter);
        // 3 * 4 == 12
        assert_eq!(
            Q64x64::from_int(3).mul(Q64x64::from_int(4), D).unwrap(),
            Q64x64::from_int(12)
        );
    }

    #[test]
    fn mul_rounding_and_overflow() {
        // (1/3) * (1/3): inexact, Up exceeds Down by one ulp.
        let third = Q64x64::from_ratio(1, 3, D).unwrap();
        let down = third.mul(third, D).unwrap();
        let up = third.mul(third, U).unwrap();
        assert_eq!(up.to_bits() - down.to_bits(), 1);
        // large * large overflows the u128 result.
        let big = Q64x64::from_int(1u64 << 40);
        // product value 2^80 -> bits 2^144 exceeds u128, narrowing returns None.
        assert_eq!(big.mul(big, D), None);
    }

    #[test]
    fn div_values_and_rounding() {
        // 1 / 2 == 0.5
        assert_eq!(
            Q64x64::ONE.div(Q64x64::from_int(2), D).unwrap(),
            Q64x64::from_ratio(1, 2, D).unwrap()
        );
        // x / x == 1
        let x = Q64x64::from_ratio(7, 9, D).unwrap();
        assert_eq!(x.div(x, D).unwrap(), Q64x64::ONE);
        // div by zero
        assert_eq!(Q64x64::ONE.div(Q64x64::ZERO, D), None);
        // rounding: 1/3 inexact
        let down = Q64x64::ONE.div(Q64x64::from_int(3), D).unwrap();
        let up = Q64x64::ONE.div(Q64x64::from_int(3), U).unwrap();
        assert_eq!(up.to_bits() - down.to_bits(), 1);
    }

    #[test]
    fn recip_roundtrip() {
        // recip(2) == 0.5
        assert_eq!(
            Q64x64::from_int(2).recip(D).unwrap(),
            Q64x64::from_ratio(1, 2, D).unwrap()
        );
        // recip(1) == 1
        assert_eq!(Q64x64::ONE.recip(D).unwrap(), Q64x64::ONE);
        // recip(0) == None
        assert_eq!(Q64x64::ZERO.recip(D), None);
        // recip is div(ONE, x)
        let x = Q64x64::from_int(7);
        assert_eq!(x.recip(D).unwrap(), Q64x64::ONE.div(x, D).unwrap());
    }

    #[test]
    fn amount_conversions() {
        // price 2.5 * amount 100 == 250
        let price = Q64x64::from_ratio(5, 2, D).unwrap();
        assert_eq!(price.mul_int(100, D).unwrap(), 250);
        // inverse: 250 / 2.5 == 100
        assert_eq!(price.div_int(250, D).unwrap(), 100);
        // rounding down vs up on inexact: 1/3 * 1 amount
        let third = Q64x64::from_ratio(1, 3, D).unwrap();
        assert_eq!(third.mul_int(1, D).unwrap(), 0);
        assert_eq!(third.mul_int(1, U).unwrap(), 1);
        // div_int by zero
        assert_eq!(Q64x64::ZERO.div_int(10, D), None);
    }

    #[test]
    fn ordering() {
        assert!(Q64x64::from_int(1) < Q64x64::from_int(2));
        assert!(Q64x64::from_ratio(1, 2, D).unwrap() < Q64x64::ONE);
    }

    #[test]
    fn boundaries() {
        // from_int(u64::MAX) does not truncate.
        let m = Q64x64::from_int(u64::MAX);
        assert_eq!(m.to_bits(), (u64::MAX as u128) << 64);
        assert_eq!(m.floor_int(), u64::MAX as u128);

        // MAX round-trips through mul by ONE.
        assert_eq!(Q64x64::MAX.mul(Q64x64::ONE, D).unwrap(), Q64x64::MAX);

        // recip of the smallest nonzero ulp: 1 / 2^-64 = 2^64 bits -> 2^128, overflows.
        assert_eq!(Q64x64::from_bits(1).recip(D), None);
        // recip of a small-but-representable value stays in range.
        let small = Q64x64::from_int(1u64 << 40); // 2^40
        assert_eq!(small.recip(D).unwrap().to_bits(), 1u128 << (64 - 40));
    }

    #[test]
    fn overflow_paths_return_none() {
        // mul: MAX * MAX overflows.
        assert_eq!(Q64x64::MAX.mul(Q64x64::MAX, D), None);
        // div: MAX / (tiny) overflows.
        assert_eq!(Q64x64::MAX.div(Q64x64::from_bits(1), D), None);
        // from_ratio: huge numerator overflows after the << 64 scaling.
        assert_eq!(Q64x64::from_ratio(u128::MAX, 1, D), None);
        // mul_int: large price * large amount overflows the integer result.
        assert_eq!(Q64x64::from_int(u64::MAX).mul_int(u128::MAX, D), None);
        // div_int: huge amount / tiny price overflows.
        assert_eq!(Q64x64::from_bits(1).div_int(u128::MAX, D), None);
    }

    proptest! {
        // Up is never below Down and differs by at most one ulp, for every op.
        #[test]
        fn mul_rounding_direction(a in any::<u128>(), b in any::<u128>()) {
            let (x, y) = (Q64x64::from_bits(a), Q64x64::from_bits(b));
            if let (Some(d), Some(u)) = (x.mul(y, D), x.mul(y, U)) {
                prop_assert!(u.to_bits() >= d.to_bits());
                prop_assert!(u.to_bits() - d.to_bits() <= 1);
            }
        }

        #[test]
        fn div_rounding_direction(a in any::<u128>(), b in 1u128..=u128::MAX) {
            let (x, y) = (Q64x64::from_bits(a), Q64x64::from_bits(b));
            if let (Some(d), Some(u)) = (x.div(y, D), x.div(y, U)) {
                prop_assert!(u.to_bits() >= d.to_bits());
                prop_assert!(u.to_bits() - d.to_bits() <= 1);
            }
        }

        #[test]
        fn mul_int_rounding_direction(bits in any::<u128>(), amt in any::<u128>()) {
            let x = Q64x64::from_bits(bits);
            if let (Some(d), Some(u)) = (x.mul_int(amt, D), x.mul_int(amt, U)) {
                prop_assert!(u >= d);
                prop_assert!(u - d <= 1);
            }
        }

        // from_ratio is exact <=> Up equals Down.
        #[test]
        fn from_ratio_exactness(a in any::<u128>(), b in 1u128..=u128::MAX) {
            if let (Some(d), Some(u)) = (
                Q64x64::from_ratio(a, b, D),
                Q64x64::from_ratio(a, b, U),
            ) {
                let exact = (U256::from(a) << SCALE_OFFSET) % U256::from(b) == U256::ZERO;
                if exact {
                    prop_assert_eq!(d, u);
                } else {
                    prop_assert_eq!(u.to_bits() - d.to_bits(), 1);
                }
            }
        }
    }
}
