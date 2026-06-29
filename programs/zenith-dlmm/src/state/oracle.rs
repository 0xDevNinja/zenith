//! Price oracle — a TWAP ring buffer over the active bin.
//!
//! Each swap records an [`Observation`] holding a running **cumulative active
//! bin**: `Σ active_bin * Δslots`. The time-weighted average bin over a window
//! is then `(cumulative(now) - cumulative(now - window)) / window` — the
//! Uniswap-style accumulator, adapted to the discrete active bin. The TWAP
//! *price* is `bin_price(bin_step, twap_bin)`.
//!
//! Observations live in a fixed-capacity ring (`[Observation; ORACLE_CAPACITY]`)
//! so the account size is bounded; the configured `length` (`1..=capacity`) sets
//! how far back the window can reach before the oldest sample is overwritten.

use anchor_lang::prelude::*;

use crate::constants::ORACLE_CAPACITY;

/// One oracle sample.
#[zero_copy]
#[repr(C)]
#[derive(Default)]
pub struct Observation {
    /// Running `Σ active_bin * Δslots` up to `timestamp`.
    pub cumulative_active_bin: i128,
    /// Slot this observation was written at.
    pub timestamp: u64,
    /// 1 once written.
    pub initialized: u8,
    /// Padding to a 16-byte multiple (no Pod hole).
    pub padding: [u8; 7],
}

#[account(zero_copy)]
#[repr(C)]
pub struct Oracle {
    /// The ring buffer.
    pub observations: [Observation; ORACLE_CAPACITY],
    /// The pair this oracle belongs to.
    pub lb_pair: Pubkey,
    /// Configured ring length (`1..=ORACLE_CAPACITY`).
    pub length: u16,
    /// Number of observations written so far (`<= length`).
    pub active_size: u16,
    /// Index of the most recent observation.
    pub last_index: u16,
    /// PDA bump.
    pub bump: u8,
    /// Padding to a 16-byte multiple.
    pub padding: [u8; 9],
}

impl Oracle {
    /// On-chain byte length including the 8-byte account discriminator.
    pub const LEN: usize = 8 + core::mem::size_of::<Oracle>();

    /// Index of the oldest live observation.
    fn oldest_index(&self) -> usize {
        if self.active_size < self.length {
            0
        } else {
            ((self.last_index + 1) % self.length) as usize
        }
    }

    /// Record the active bin in effect since the last observation. Call on swap
    /// with the **pre-swap** active bin and the current slot — that bin was the
    /// price over `[last.timestamp, now]`. A repeated slot is ignored.
    pub fn record(&mut self, active_bin: i32, now: u64) {
        if self.length == 0 {
            return;
        }
        if self.active_size == 0 {
            self.observations[0] = Observation {
                cumulative_active_bin: 0,
                timestamp: now,
                initialized: 1,
                padding: [0; 7],
            };
            self.last_index = 0;
            self.active_size = 1;
            return;
        }
        let last = self.observations[self.last_index as usize];
        if now <= last.timestamp {
            return;
        }
        let cumulative =
            last.cumulative_active_bin + active_bin as i128 * (now - last.timestamp) as i128;
        let next = ((self.last_index + 1) % self.length) as usize;
        self.observations[next] = Observation {
            cumulative_active_bin: cumulative,
            timestamp: now,
            initialized: 1,
            padding: [0; 7],
        };
        self.last_index = next as u16;
        if self.active_size < self.length {
            self.active_size += 1;
        }
    }

    /// Cumulative active bin interpolated at slot `t` (assumed within the live
    /// window `[oldest.timestamp, now]`). The open segment after the last
    /// observation runs at `current_active_bin` up to `now`.
    fn cumulative_at(&self, t: u64, current_active_bin: i32, _now: u64) -> i128 {
        let oldest = self.oldest_index();
        let n = self.active_size as usize;
        for k in 0..n.saturating_sub(1) {
            let a = self.observations[(oldest + k) % self.length as usize];
            let b = self.observations[(oldest + k + 1) % self.length as usize];
            if t < b.timestamp {
                let seg = (b.timestamp - a.timestamp) as i128;
                if seg == 0 || t <= a.timestamp {
                    return a.cumulative_active_bin;
                }
                let gap = b.cumulative_active_bin - a.cumulative_active_bin;
                return a.cumulative_active_bin + gap * (t - a.timestamp) as i128 / seg;
            }
        }
        // `t` is at or after the last observation: the open segment runs at the
        // current active bin.
        let last = self.observations[self.last_index as usize];
        last.cumulative_active_bin + current_active_bin as i128 * (t - last.timestamp) as i128
    }

    /// Time-weighted average active bin over the last `window` slots, given the
    /// current active bin and slot. If the recorded history is shorter than
    /// `window` the window is clamped to the oldest sample, so the result is the
    /// average over the *available* span (a caller needing the exact window
    /// should check the history depth). Rounds toward negative infinity (active
    /// bins can be negative). Returns `None` if there are no samples or no time
    /// has elapsed.
    pub fn twap(&self, current_active_bin: i32, now: u64, window: u64) -> Option<i64> {
        if self.active_size == 0 || window == 0 {
            return None;
        }
        let last = self.observations[self.last_index as usize];
        let cum_now = last.cumulative_active_bin
            + current_active_bin as i128 * now.saturating_sub(last.timestamp) as i128;

        let oldest = self.observations[self.oldest_index()];
        let start = now.saturating_sub(window).max(oldest.timestamp);
        if now <= start {
            return None;
        }
        let cum_start = self.cumulative_at(start, current_active_bin, now);
        let span = (now - start) as i128;
        // Floor division (div_euclid) so a negative average rounds consistently
        // toward -inf instead of toward zero.
        Some((cum_now - cum_start).div_euclid(span) as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oracle(length: u16) -> Oracle {
        let mut o: Oracle = bytemuck::Zeroable::zeroed();
        o.length = length;
        o
    }

    #[test]
    fn twap_matches_reference_constant_bin() {
        // Active bin held at 5 the whole time -> TWAP is 5.
        let mut o = oracle(8);
        o.record(5, 100); // first sample (cumulative 0)
        o.record(5, 110); // bin 5 over [100,110]
                          // now = 120, current bin still 5, window 20 -> avg 5.
        assert_eq!(o.twap(5, 120, 20), Some(5));
    }

    #[test]
    fn twap_matches_reference_step_change() {
        // bin 10 for [0,10], then bin 20 for [10,30]. Over the last 30 slots:
        // (10*10 + 20*20) / 30 = (100 + 400)/30 = 500/30 = 16 (floor).
        let mut o = oracle(8);
        o.record(10, 0); // seed at t=0
        o.record(10, 10); // bin 10 over [0,10] -> cumulative 100
                          // current bin becomes 20 from t=10; query at now=30.
        assert_eq!(o.twap(20, 30, 30), Some(16));
        // shorter window [20,30] is entirely bin 20 -> 20.
        assert_eq!(o.twap(20, 30, 10), Some(20));
    }

    #[test]
    fn negative_bin_twap_floors() {
        // bin -10 over [0,10], bin -20 over [10,30]: avg = -(100+400)/30 =
        // -16.67 -> floor -17 (not -16, which toward-zero would give).
        let mut o = oracle(8);
        o.record(-10, 0);
        o.record(-10, 10); // cumulative -100
        assert_eq!(o.twap(-20, 30, 30), Some(-17));
    }

    #[test]
    fn buffer_wraps_and_keeps_recent() {
        // length 3: after 5 records the 3 newest survive.
        let mut o = oracle(3);
        for (bin, t) in [(1, 0), (2, 10), (3, 20), (4, 30), (5, 40)] {
            o.record(bin, t);
        }
        assert_eq!(o.active_size, 3);
        // oldest live sample is the one written at t=20 (bin 3 recorded then).
        let oldest = o.observations[o.oldest_index()];
        assert_eq!(oldest.timestamp, 20);
        // window longer than history clamps to the oldest sample.
        assert!(o.twap(5, 50, 1000).is_some());
    }

    #[test]
    fn repeated_slot_is_ignored() {
        let mut o = oracle(8);
        o.record(7, 100);
        o.record(7, 100); // same slot
        o.record(7, 100); // same slot
        assert_eq!(o.active_size, 1);
    }

    #[test]
    fn empty_or_zero_window_is_none() {
        let o = oracle(8);
        assert_eq!(o.twap(5, 100, 10), None); // no samples
        let mut o2 = oracle(8);
        o2.record(5, 100);
        assert_eq!(o2.twap(5, 100, 0), None); // zero window
    }
}
