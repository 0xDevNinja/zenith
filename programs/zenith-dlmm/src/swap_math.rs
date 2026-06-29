//! Single-bin constant-sum swap math.
//!
//! Inside a bin the price `P` (token Y per token X) is fixed, so a trade is a
//! constant-sum exchange with **zero slippage**: every unit of X is worth `P`
//! units of Y until one side of the bin is exhausted. These pure helpers fill
//! one bin; the handler walks bins, crossing to the next when a bin drains.
//!
//! Rounding always favors the protocol: the input needed for a given output
//! rounds **up**, the output for a given input rounds **down**.

use zenith_math::{Q64x64, Rounding};

/// Swap direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Direction {
    /// Sell X for Y. Consumes a bin's Y reserve; the active bin moves **down**.
    XtoY = 0,
    /// Sell Y for X. Consumes a bin's X reserve; the active bin moves **up**.
    YtoX = 1,
}

impl Direction {
    /// Decode from the wire byte, or `None` for an unknown value.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Direction::XtoY),
            1 => Some(Direction::YtoX),
            _ => None,
        }
    }
}

/// Whether the caller fixed the input or the output amount.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum SwapMode {
    /// Spend exactly `amount` of input, maximize output.
    ExactIn = 0,
    /// Receive exactly `amount` of output, minimize input.
    ExactOut = 1,
}

impl SwapMode {
    /// Decode from the wire byte, or `None` for an unknown value.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(SwapMode::ExactIn),
            1 => Some(SwapMode::ExactOut),
            _ => None,
        }
    }
}

/// Output produced by spending `input` in a bin at `price`.
///
/// `X->Y`: `out_y = in_x * P`. `Y->X`: `out_x = in_y / P`.
fn out_for_in(input: u64, price: Q64x64, dir: Direction, rounding: Rounding) -> Option<u64> {
    let out = match dir {
        Direction::XtoY => price.mul_int(input as u128, rounding)?,
        Direction::YtoX => price.div_int(input as u128, rounding)?,
    };
    u64::try_from(out).ok()
}

/// Input required to produce `output` in a bin at `price`.
///
/// `X->Y`: `in_x = out_y / P`. `Y->X`: `in_y = out_x * P`.
fn in_for_out(output: u64, price: Q64x64, dir: Direction, rounding: Rounding) -> Option<u64> {
    let inp = match dir {
        Direction::XtoY => price.div_int(output as u128, rounding)?,
        Direction::YtoX => price.mul_int(output as u128, rounding)?,
    };
    u64::try_from(inp).ok()
}

/// The result of filling one bin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinFill {
    /// Input consumed by this bin.
    pub in_used: u64,
    /// Output produced by this bin.
    pub out: u64,
    /// `true` if the bin's out reserve was fully drained (cross to the next).
    pub drained: bool,
}

/// Fill a bin with up to `in_avail` input. `reserve_out` is the bin's reserve
/// of the output token (Y for `XtoY`, X for `YtoX`).
///
/// Either the bin drains (and `in_avail` may be left over for the next bin) or
/// all of `in_avail` is spent here. Output rounds down; the drain input rounds
/// up — both in the protocol's favor.
pub fn fill_exact_in(
    in_avail: u64,
    reserve_out: u64,
    price: Q64x64,
    dir: Direction,
) -> Option<BinFill> {
    if in_avail == 0 || reserve_out == 0 {
        return Some(BinFill {
            in_used: 0,
            out: 0,
            drained: reserve_out == 0,
        });
    }
    // Input that would drain the bin's whole out reserve.
    let in_to_drain = in_for_out(reserve_out, price, dir, Rounding::Up)?;
    if in_avail >= in_to_drain {
        Some(BinFill {
            in_used: in_to_drain,
            out: reserve_out,
            drained: true,
        })
    } else {
        let out = out_for_in(in_avail, price, dir, Rounding::Down)?;
        Some(BinFill {
            in_used: in_avail,
            out,
            drained: false,
        })
    }
}

/// Fill a bin to produce up to `out_need` output. `reserve_out` is the bin's
/// reserve of the output token. Input rounds up (protocol-favoring).
pub fn fill_exact_out(
    out_need: u64,
    reserve_out: u64,
    price: Q64x64,
    dir: Direction,
) -> Option<BinFill> {
    let out_take = out_need.min(reserve_out);
    if out_take == 0 {
        return Some(BinFill {
            in_used: 0,
            out: 0,
            drained: reserve_out == 0,
        });
    }
    let in_used = in_for_out(out_take, price, dir, Rounding::Up)?;
    Some(BinFill {
        in_used,
        out: out_take,
        drained: out_take == reserve_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // price 2.0 (Y per X): 1 X <-> 2 Y.
    fn p2() -> Q64x64 {
        Q64x64::from_int(2)
    }

    #[test]
    fn exact_in_partial_within_bin() {
        // X->Y, plenty of Y reserve, spend 10 X -> 20 Y, bin not drained.
        let f = fill_exact_in(10, 1_000, p2(), Direction::XtoY).unwrap();
        assert_eq!(
            f,
            BinFill {
                in_used: 10,
                out: 20,
                drained: false
            }
        );
        // Y->X, spend 10 Y -> 5 X.
        let f = fill_exact_in(10, 1_000, p2(), Direction::YtoX).unwrap();
        assert_eq!(
            f,
            BinFill {
                in_used: 10,
                out: 5,
                drained: false
            }
        );
    }

    #[test]
    fn exact_in_drains_bin() {
        // X->Y, only 20 Y in the bin; draining needs 10 X, leftover stays.
        let f = fill_exact_in(1_000, 20, p2(), Direction::XtoY).unwrap();
        assert_eq!(
            f,
            BinFill {
                in_used: 10,
                out: 20,
                drained: true
            }
        );
    }

    #[test]
    fn exact_out_within_and_drain() {
        // Want 20 Y out (X->Y), bin has 1000 -> need 10 X, not drained.
        let f = fill_exact_out(20, 1_000, p2(), Direction::XtoY).unwrap();
        assert_eq!(
            f,
            BinFill {
                in_used: 10,
                out: 20,
                drained: false
            }
        );
        // Want 50 Y but bin only has 20 -> take 20, drained, need 10 X.
        let f = fill_exact_out(50, 20, p2(), Direction::XtoY).unwrap();
        assert_eq!(
            f,
            BinFill {
                in_used: 10,
                out: 20,
                drained: true
            }
        );
    }

    #[test]
    fn empty_bin_is_drained_noop() {
        let f = fill_exact_in(100, 0, p2(), Direction::XtoY).unwrap();
        assert_eq!(
            f,
            BinFill {
                in_used: 0,
                out: 0,
                drained: true
            }
        );
        let f = fill_exact_out(100, 0, p2(), Direction::XtoY).unwrap();
        assert_eq!(
            f,
            BinFill {
                in_used: 0,
                out: 0,
                drained: true
            }
        );
    }

    proptest! {
        /// A bin fill never produces more output than the reserve and never
        /// rounds output in the trader's favor.
        #[test]
        fn fill_never_over_produces(
            in_avail in 0u64..=u64::MAX,
            reserve_out in 0u64..=u64::MAX,
            price_bits in 1u128..=(u128::MAX >> 1),
        ) {
            let price = Q64x64::from_bits(price_bits);
            for dir in [Direction::XtoY, Direction::YtoX] {
                if let Some(f) = fill_exact_in(in_avail, reserve_out, price, dir) {
                    prop_assert!(f.out <= reserve_out);
                    prop_assert!(f.in_used <= in_avail);
                    if f.drained {
                        prop_assert_eq!(f.out, reserve_out);
                    }
                }
            }
        }

        /// ExactOut never takes more than requested or more than the reserve.
        #[test]
        fn exact_out_bounded(
            out_need in 0u64..=u64::MAX,
            reserve_out in 0u64..=u64::MAX,
            price_bits in 1u128..=(u128::MAX >> 1),
        ) {
            let price = Q64x64::from_bits(price_bits);
            for dir in [Direction::XtoY, Direction::YtoX] {
                if let Some(f) = fill_exact_out(out_need, reserve_out, price, dir) {
                    prop_assert!(f.out <= reserve_out);
                    prop_assert!(f.out <= out_need);
                }
            }
        }
    }
}
