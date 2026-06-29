//! Liquidity distribution strategies.
//!
//! When an LP adds liquidity across a range of bins, a *strategy* decides how
//! much of each token goes into each bin. All three shapes are defined by a
//! per-bin weight as a function of the bin's distance `d` from the active edge
//! of its side (`d = 0` is the bin nearest the current price):
//!
//! - **Spot** — uniform: every bin gets the same weight. A flat book.
//! - **Curve** — weight is highest at the active edge and falls linearly to the
//!   far edge: liquidity is concentrated around the current price (tight
//!   spreads, more fees while the price stays put).
//! - **BidAsk** — the inverse: weight is lowest at the active edge and highest
//!   at the far edge, concentrating liquidity away from the price (good for
//!   catching volatility / scaling in and out).
//!
//! [`distribute`] turns a token total + a strategy into exact per-bin amounts
//! that sum back to the total (the flooring remainder is handed out one unit at
//! a time from the active edge outward).

use zenith_math::{mul_div, MathError, Rounding};

use crate::constants::MAX_BINS_PER_POSITION;

/// How liquidity is shaped across a bin range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Strategy {
    /// Uniform across all bins.
    Spot = 0,
    /// Concentrated at the active edge, falling off to the far edge.
    Curve = 1,
    /// Concentrated at the far edge, lightest at the active edge.
    BidAsk = 2,
}

impl Strategy {
    /// Decode from the wire byte, or `None` for an unknown value.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Strategy::Spot),
            1 => Some(Strategy::Curve),
            2 => Some(Strategy::BidAsk),
            _ => None,
        }
    }

    /// Weight of the bin at distance `d` within a side spanning `count` bins
    /// (`0 <= d < count`). Always >= 1, so no in-range bin is starved.
    pub fn weight(self, d: u32, count: u32) -> u64 {
        match self {
            Strategy::Spot => 1,
            // count at the active edge (d=0) down to 1 at the far edge.
            Strategy::Curve => (count - d) as u64,
            // 1 at the active edge up to `count` at the far edge.
            Strategy::BidAsk => (d + 1) as u64,
        }
    }
}

/// Split `total` token units across `count` bins by `strategy`, returning the
/// per-bin amounts indexed by distance from the active edge (`[0]` is nearest
/// the price). The result always sums to exactly `total`.
///
/// `count` is bounded by [`MAX_BINS_PER_POSITION`]; callers pass the width of
/// one token side of the deposit.
pub fn distribute(total: u64, count: u32, strategy: Strategy) -> Result<Vec<u64>, MathError> {
    debug_assert!(count as usize <= MAX_BINS_PER_POSITION);
    if count == 0 {
        return Ok(Vec::new());
    }

    let weights: Vec<u64> = (0..count).map(|d| strategy.weight(d, count)).collect();
    let weight_sum: u128 = weights.iter().map(|&w| w as u128).sum();

    // Floor each share, then hand the rounding remainder out one unit at a time
    // from the active edge outward, so the parts sum back to `total` exactly.
    let mut amounts: Vec<u64> = Vec::with_capacity(count as usize);
    let mut allocated: u128 = 0;
    for &w in &weights {
        let part = mul_div(total as u128, w as u128, weight_sum, Rounding::Down)?;
        allocated += part;
        amounts.push(part as u64);
    }

    let mut remainder = total as u128 - allocated;
    let mut i = 0;
    while remainder > 0 {
        amounts[i] += 1;
        remainder -= 1;
        i += 1;
    }

    Ok(amounts)
}

/// Per-bin token amounts produced by [`plan_deposit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinDeposit {
    /// Bin id.
    pub bin_id: i32,
    /// Token X going into this bin.
    pub x: u64,
    /// Token Y going into this bin.
    pub y: u64,
}

/// Why a deposit could not be planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// A token was supplied that the bin range cannot hold (e.g. token X for a
    /// range entirely below the active bin).
    TokenSideMismatch,
    /// Arithmetic overflow while distributing.
    Math,
}

/// Plan a deposit of `amount_x` / `amount_y` across the inclusive bin range
/// `[lower, upper]` given the pair's `active` bin and a `strategy`.
///
/// Bins below `active` hold only Y, bins above hold only X, and the active bin
/// holds both. X is distributed over the `[active, upper]` side and Y over the
/// `[lower, active]` side, each measuring distance from the active edge, so the
/// strategy shape is centered on the current price. Returns one [`BinDeposit`]
/// per bin in the range (amounts may be zero), summing to exactly the inputs.
pub fn plan_deposit(
    lower: i32,
    upper: i32,
    active: i32,
    amount_x: u64,
    amount_y: u64,
    strategy: Strategy,
) -> Result<Vec<BinDeposit>, PlanError> {
    let has_x_side = upper >= active;
    let has_y_side = lower <= active;
    if amount_x > 0 && !has_x_side {
        return Err(PlanError::TokenSideMismatch);
    }
    if amount_y > 0 && !has_y_side {
        return Err(PlanError::TokenSideMismatch);
    }

    let x_start = lower.max(active);
    let y_end = upper.min(active);
    let count_x = if has_x_side {
        (upper - x_start + 1) as u32
    } else {
        0
    };
    let count_y = if has_y_side {
        (y_end - lower + 1) as u32
    } else {
        0
    };

    let dist_x = if amount_x > 0 {
        distribute(amount_x, count_x, strategy).map_err(|_| PlanError::Math)?
    } else {
        Vec::new()
    };
    let dist_y = if amount_y > 0 {
        distribute(amount_y, count_y, strategy).map_err(|_| PlanError::Math)?
    } else {
        Vec::new()
    };

    let mut out = Vec::with_capacity((upper - lower + 1) as usize);
    for id in lower..=upper {
        let x = if !dist_x.is_empty() && id >= x_start {
            dist_x[(id - x_start) as usize]
        } else {
            0
        };
        let y = if !dist_y.is_empty() && id <= y_end {
            dist_y[(y_end - id) as usize]
        } else {
            0
        };
        out.push(BinDeposit { bin_id: id, x, y });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_decodes() {
        assert_eq!(Strategy::from_u8(0), Some(Strategy::Spot));
        assert_eq!(Strategy::from_u8(1), Some(Strategy::Curve));
        assert_eq!(Strategy::from_u8(2), Some(Strategy::BidAsk));
        assert_eq!(Strategy::from_u8(3), None);
    }

    #[test]
    fn spot_is_uniform_and_exact() {
        // 100 over 4 bins: 25 each, exact.
        assert_eq!(
            distribute(100, 4, Strategy::Spot).unwrap(),
            vec![25, 25, 25, 25]
        );
        // 10 over 4 bins: 2 each + remainder 2 to the first two bins.
        assert_eq!(distribute(10, 4, Strategy::Spot).unwrap(), vec![3, 3, 2, 2]);
    }

    #[test]
    fn curve_concentrates_at_active_edge() {
        // weights for count=4 are [4,3,2,1], sum 10. total 100 -> [40,30,20,10].
        assert_eq!(
            distribute(100, 4, Strategy::Curve).unwrap(),
            vec![40, 30, 20, 10]
        );
        // strictly non-increasing from the active edge.
        let d = distribute(1_000, 8, Strategy::Curve).unwrap();
        assert!(d.windows(2).all(|w| w[0] >= w[1]));
        assert!(d[0] > *d.last().unwrap());
    }

    #[test]
    fn bidask_concentrates_at_far_edge() {
        // weights for count=4 are [1,2,3,4], sum 10. total 100 -> [10,20,30,40].
        assert_eq!(
            distribute(100, 4, Strategy::BidAsk).unwrap(),
            vec![10, 20, 30, 40]
        );
        // strictly non-decreasing from the active edge.
        let d = distribute(1_000, 8, Strategy::BidAsk).unwrap();
        assert!(d.windows(2).all(|w| w[0] <= w[1]));
        assert!(*d.last().unwrap() > d[0]);
    }

    #[test]
    fn distribution_always_sums_to_total() {
        for strat in [Strategy::Spot, Strategy::Curve, Strategy::BidAsk] {
            for &total in &[0u64, 1, 7, 100, 999, 1_000_000, u64::MAX / 2] {
                for count in 1u32..=20 {
                    let parts = distribute(total, count, strat).unwrap();
                    assert_eq!(parts.len(), count as usize);
                    let sum: u128 = parts.iter().map(|&p| p as u128).sum();
                    assert_eq!(sum, total as u128, "{strat:?} total {total} count {count}");
                }
            }
        }
    }

    #[test]
    fn single_bin_takes_everything() {
        for strat in [Strategy::Spot, Strategy::Curve, Strategy::BidAsk] {
            assert_eq!(distribute(12_345, 1, strat).unwrap(), vec![12_345]);
        }
    }

    #[test]
    fn zero_count_is_empty() {
        assert!(distribute(100, 0, Strategy::Spot).unwrap().is_empty());
    }

    // ---- plan_deposit ----

    fn sums(plan: &[BinDeposit]) -> (u64, u64) {
        plan.iter().fold((0, 0), |(sx, sy), b| (sx + b.x, sy + b.y))
    }

    #[test]
    fn balanced_deposit_splits_on_the_active_bin() {
        // range [-2, 2], active 0, Spot. X funds [0,2] (3 bins), Y funds
        // [-2,0] (3 bins). 90 each side -> 30 per bin on each side.
        let plan = plan_deposit(-2, 2, 0, 90, 90, Strategy::Spot).unwrap();
        assert_eq!(plan.len(), 5);
        // bins below active: Y only
        assert_eq!(
            plan[0],
            BinDeposit {
                bin_id: -2,
                x: 0,
                y: 30
            }
        );
        assert_eq!(
            plan[1],
            BinDeposit {
                bin_id: -1,
                x: 0,
                y: 30
            }
        );
        // active bin: both
        assert_eq!(
            plan[2],
            BinDeposit {
                bin_id: 0,
                x: 30,
                y: 30
            }
        );
        // bins above active: X only
        assert_eq!(
            plan[3],
            BinDeposit {
                bin_id: 1,
                x: 30,
                y: 0
            }
        );
        assert_eq!(
            plan[4],
            BinDeposit {
                bin_id: 2,
                x: 30,
                y: 0
            }
        );
        assert_eq!(sums(&plan), (90, 90));
    }

    #[test]
    fn one_sided_x_above_active_places_only_x() {
        // range entirely above the active bin -> only token X allowed.
        let plan = plan_deposit(5, 8, 0, 100, 0, Strategy::Spot).unwrap();
        assert!(plan.iter().all(|b| b.y == 0));
        assert_eq!(sums(&plan), (100, 0));
        // supplying Y for an all-X range is rejected.
        assert_eq!(
            plan_deposit(5, 8, 0, 100, 1, Strategy::Spot),
            Err(PlanError::TokenSideMismatch)
        );
    }

    #[test]
    fn one_sided_y_below_active_places_only_y() {
        // range entirely below the active bin -> only token Y allowed.
        let plan = plan_deposit(-8, -5, 0, 0, 100, Strategy::Spot).unwrap();
        assert!(plan.iter().all(|b| b.x == 0));
        assert_eq!(sums(&plan), (0, 100));
        assert_eq!(
            plan_deposit(-8, -5, 0, 1, 100, Strategy::Spot),
            Err(PlanError::TokenSideMismatch)
        );
    }

    #[test]
    fn curve_concentrates_each_side_at_the_active_bin() {
        // active inside the range; Curve weights are highest nearest active.
        let plan = plan_deposit(-3, 3, 0, 1_000, 1_000, Strategy::Curve).unwrap();
        // X side [0,3]: weights [4,3,2,1] over 100 of total -> active bin most.
        let active = plan.iter().find(|b| b.bin_id == 0).unwrap();
        let far_x = plan.iter().find(|b| b.bin_id == 3).unwrap();
        assert!(active.x > far_x.x);
        // Y side [-3,0]: active bin gets the most Y too.
        let far_y = plan.iter().find(|b| b.bin_id == -3).unwrap();
        assert!(active.y > far_y.y);
        assert_eq!(sums(&plan), (1_000, 1_000));
    }

    #[test]
    fn deposit_always_conserves_input_totals() {
        for strat in [Strategy::Spot, Strategy::Curve, Strategy::BidAsk] {
            for &(lo, hi, act) in &[(-5i32, 5i32, 0i32), (-10, -3, 0), (2, 9, 0), (0, 0, 0)] {
                let (ax, ay) = (777u64, 333u64);
                // skip combinations that are token-side invalid
                let plan = match plan_deposit(lo, hi, act, ax, ay, strat) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                assert_eq!(sums(&plan), (ax, ay), "{strat:?} [{lo},{hi}] act {act}");
            }
        }
    }
}
