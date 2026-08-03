//! Per-channel RMS → health classification.
//!
//! A port of `Sources/BCICore/Preprocessing/ChannelHealthThresholds.swift`
//! from the macOS repository, whose own docstring says it is "intentionally
//! pure: no actor isolation, no I/O, no dependencies" and that future
//! revisions "should remain pure functions". Being pure is exactly what made
//! it portable, so it moves here unchanged in behaviour.
//!
//! WHY IT MOVED
//!
//! The core already carries `ChannelHealthState`, a frozen
//! `contracts/fixtures/health.json`, a schema whose `status` enum is
//! `healthy | saturated | dead | unknown`, and a golden test asserting the
//! fixture's AF8 reads `saturated`. What it did not have was the rule that
//! decides which of those four a measurement is. That lived only in Swift, so
//! any non-Apple shell could compute an RMS and had no way to interpret it.
//!
//! Measured live on 2026-08-03 (Muse S over BLE, Ubuntu 26.04, Ryzen AI 9
//! HX 370): a poorly seated headband gave TP9 513 µV and AF7 881 µV, both
//! saturated by these thresholds, and a headless runtime printing bare numbers
//! reported them as merely "high". After adjustment the same channels read
//! 54 µV and 86 µV. The classification was the missing part, not the arithmetic.
//!
//! THE THRESHOLDS ARE HEURISTICS AND THE SOURCE SAYS SO
//!
//! 2 µV dead / 200 µV saturated were "derived for the Muse S validation
//! subject on 2026-07-10 and are NOT physiologically validated cutoffs …
//! intended to be tuned as more recordings become available." That caveat
//! travels with them. Two independent corroborations exist and neither is
//! validation: the frozen fixture reads 162.5 / 176.6 / 146.4 as healthy and
//! 971.0 as saturated, and the 2026-08-03 session above sits inside the band
//! at rest on all four channels.
//!
//! `saturated` does not mean one thing. The Swift enum documents it as
//! "typically muscle / motion contamination or 50/60 Hz interference,
//! sometimes an analog front-end fault", and the 2026-08-03 session showed
//! both: two channels pinned for 30 s (contact, fixed by adjustment) and a
//! single instant where ALL FOUR jumped to ~300-350 together and recovered
//! (motion). One threshold, two causes — only the pattern across channels
//! separates them, which is why this classifies per channel and never
//! aggregates.

use serde::{Deserialize, Serialize};

/// The four states in `contracts/api-schema/health.schema.json`'s `status`
/// enum. `as_str` is the wire spelling and must stay in sync with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelHealthStatus {
    /// Not enough samples yet to form a stable estimate.
    Unknown,
    /// RMS in the configured healthy range.
    Healthy,
    /// RMS above the saturated threshold; typically muscle / motion
    /// contamination or 50/60 Hz interference, sometimes an analog
    /// front-end fault.
    Saturated,
    /// RMS below the dead threshold; typically an electrode lifted off
    /// the scalp or a dry pad.
    Dead,
}

impl ChannelHealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Healthy => "healthy",
            Self::Saturated => "saturated",
            Self::Dead => "dead",
        }
    }
}

/// Classifies a per-channel RMS measurement. Pure: no I/O, no clock, no state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelHealthThresholds {
    /// RMS below this is `Dead`, in microvolts.
    pub dead_rms: f64,
    /// RMS above this is `Saturated`, in microvolts.
    pub saturated_rms: f64,
    /// Below this many samples the result is `Unknown` regardless of RMS,
    /// because very short windows are dominated by a single oscillation and
    /// do not represent the long-run amplitude of the channel.
    pub minimum_samples: u64,
}

impl Default for ChannelHealthThresholds {
    fn default() -> Self {
        Self {
            dead_rms: 2.0,
            saturated_rms: 200.0,
            minimum_samples: 32,
        }
    }
}

impl ChannelHealthThresholds {
    /// Mirrors the Swift `precondition`s rather than panicking, so a bad
    /// configuration is a value a caller can handle instead of a crash in a
    /// long-running shell.
    pub fn new(dead_rms: f64, saturated_rms: f64, minimum_samples: u64) -> Option<Self> {
        // Finiteness is checked FIRST so the comparisons below are total.
        // Writing them as negated comparisons (`!(x >= 0.0)`) would also
        // reject NaN, but clippy's neg_cmp_op_on_partial_ord is right that it
        // hides the intent: the reader cannot tell whether NaN-rejection is
        // deliberate or accidental. Here it is deliberate and separate.
        if !dead_rms.is_finite() || !saturated_rms.is_finite() {
            return None;
        }
        if dead_rms < 0.0 || saturated_rms <= dead_rms {
            return None;
        }
        if minimum_samples == 0 {
            return None;
        }
        Some(Self {
            dead_rms,
            saturated_rms,
            minimum_samples,
        })
    }

    /// Classify one channel.
    ///
    /// Boundary semantics are the Swift original's, and the ported tests pin
    /// each one: the sample floor is INCLUSIVE (`samples >= minimum_samples`),
    /// `saturated` is strict (`rms > saturated_rms`, so exactly 200 is
    /// healthy), and `dead` is strict (`rms < dead_rms`, so exactly 2 is
    /// healthy). A non-finite RMS is `Unknown`, not healthy — a NaN must never
    /// read as a working electrode.
    pub fn status(&self, rms: f64, samples: u64) -> ChannelHealthStatus {
        if samples < self.minimum_samples || !rms.is_finite() {
            return ChannelHealthStatus::Unknown;
        }
        if rms > self.saturated_rms {
            ChannelHealthStatus::Saturated
        } else if rms < self.dead_rms {
            ChannelHealthStatus::Dead
        } else {
            ChannelHealthStatus::Healthy
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported one-for-one from ChannelHealthThresholdsTests.swift so the two
    // implementations cannot silently diverge.

    #[test]
    fn default_thresholds_classify_healthy_range() {
        let t = ChannelHealthThresholds::default();
        for rms in [10.0, 50.0, 199.0] {
            assert_eq!(
                t.status(rms, 256),
                ChannelHealthStatus::Healthy,
                "rms {rms}"
            );
        }
    }

    #[test]
    fn default_thresholds_classify_saturated() {
        let t = ChannelHealthThresholds::default();
        for rms in [201.0, 900.0] {
            assert_eq!(
                t.status(rms, 256),
                ChannelHealthStatus::Saturated,
                "rms {rms}"
            );
        }
    }

    #[test]
    fn default_thresholds_classify_dead() {
        let t = ChannelHealthThresholds::default();
        for rms in [0.0, 1.99] {
            assert_eq!(t.status(rms, 256), ChannelHealthStatus::Dead, "rms {rms}");
        }
    }

    #[test]
    fn below_the_sample_floor_is_unknown_and_the_floor_is_inclusive() {
        let t = ChannelHealthThresholds::default();
        assert_eq!(t.status(10.0, 0), ChannelHealthStatus::Unknown);
        assert_eq!(t.status(10.0, 31), ChannelHealthStatus::Unknown);
        // Exactly at the minimum returns the real classification.
        assert_eq!(t.status(10.0, 32), ChannelHealthStatus::Healthy);
    }

    #[test]
    fn custom_thresholds_can_be_constructed() {
        let t = ChannelHealthThresholds::new(5.0, 500.0, 64).expect("valid");
        assert_eq!(t.status(3.0, 256), ChannelHealthStatus::Dead);
        assert_eq!(t.status(50.0, 256), ChannelHealthStatus::Healthy);
        assert_eq!(t.status(600.0, 256), ChannelHealthStatus::Saturated);
        assert_eq!(t.status(50.0, 32), ChannelHealthStatus::Unknown);
        assert_eq!(t.status(50.0, 64), ChannelHealthStatus::Healthy);
    }

    #[test]
    fn the_exact_threshold_values_are_healthy_not_their_own_category() {
        // Both comparisons are strict in the Swift original. Pinned in BOTH
        // directions so flipping either to >= or <= fails here.
        let t = ChannelHealthThresholds::default();
        assert_eq!(t.status(200.0, 256), ChannelHealthStatus::Healthy);
        assert_eq!(t.status(200.001, 256), ChannelHealthStatus::Saturated);
        assert_eq!(t.status(2.0, 256), ChannelHealthStatus::Healthy);
        assert_eq!(t.status(1.999, 256), ChannelHealthStatus::Dead);
    }

    #[test]
    fn a_non_finite_rms_is_unknown_not_healthy() {
        let t = ChannelHealthThresholds::default();
        assert_eq!(t.status(f64::NAN, 256), ChannelHealthStatus::Unknown);
        assert_eq!(t.status(f64::INFINITY, 256), ChannelHealthStatus::Unknown);
    }

    #[test]
    fn invalid_configurations_are_rejected_rather_than_panicking() {
        assert!(ChannelHealthThresholds::new(-1.0, 200.0, 32).is_none());
        assert!(ChannelHealthThresholds::new(200.0, 200.0, 32).is_none()); // not >
        assert!(ChannelHealthThresholds::new(300.0, 200.0, 32).is_none());
        assert!(ChannelHealthThresholds::new(2.0, 200.0, 0).is_none());
        assert!(ChannelHealthThresholds::new(2.0, f64::NAN, 32).is_none());
    }

    #[test]
    fn the_frozen_health_fixture_agrees_with_these_thresholds() {
        // contracts/fixtures/health.json is contract-frozen and its statuses
        // were written by the Swift implementation. If this port disagreed
        // with it, one of the two is wrong — this is the cross-check that
        // says which.
        let t = ChannelHealthThresholds::default();
        let cases = [
            ("TP9", 162.5, ChannelHealthStatus::Healthy),
            ("AF7", 176.6, ChannelHealthStatus::Healthy),
            ("AF8", 971.0, ChannelHealthStatus::Saturated),
            ("TP10", 146.4, ChannelHealthStatus::Healthy),
        ];
        for (ch, rms, want) in cases {
            assert_eq!(t.status(rms, 77_966), want, "{ch} at {rms} µV");
        }
    }

    #[test]
    fn the_2026_08_03_live_session_classifies_as_observed() {
        // Real Muse S over BLE on Ubuntu 26.04. Before adjustment two channels
        // were pinned; after, all four sat in the healthy band. Recorded here
        // so a threshold change has to confront a real session, not only the
        // synthetic fixture.
        let t = ChannelHealthThresholds::default();
        let before = [("TP9", 513.0), ("AF7", 881.0)];
        for (ch, rms) in before {
            assert_eq!(t.status(rms, 7688), ChannelHealthStatus::Saturated, "{ch}");
        }
        let after = [("TP9", 54.0), ("AF7", 86.0), ("AF8", 132.0), ("TP10", 62.0)];
        for (ch, rms) in after {
            assert_eq!(t.status(rms, 7688), ChannelHealthStatus::Healthy, "{ch}");
        }
        // The all-channel excursion at t=15s reads saturated on every channel,
        // which is what distinguishes motion from an electrode fault.
        for rms in [305.0, 344.0, 352.0, 310.0] {
            assert_eq!(t.status(rms, 1286), ChannelHealthStatus::Saturated);
        }
    }
}
