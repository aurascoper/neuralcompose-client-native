//! Turning a stream of EEG samples into the one thing a turn record can carry:
//! per-channel signal health, or nothing at all.
//!
//! **Pure**, like everything else in this lib. The socket, the thread and the
//! clock live in the binary; what is decided here is *whether there is anything
//! current to report* and *what it says*. That decision is the whole of the
//! interesting logic, and it is the half that can be wrong silently.
//!
//! ## EEG does not touch the dialectic, and this is where that is enforced
//!
//! Nothing in this module returns a weight, a bias or a scalar the competition
//! reads. `gloss_scalar` stays at [`crate::turn_log::NEUTRAL_GLOSS`] whether an
//! EEG is attached or not. The Swift *does* let a `SpectralState` bias
//! selection (`SpectralGloss` → `DialecticalField.advance` → weights →
//! potential → softmax), but that path starts at an MLX estimator with no Linux
//! runtime. Deriving a gloss here from band ratios would be inventing an
//! unvalidated mapping and giving it the authority of a measurement.
//! **The turn log is the only place EEG appears.**
//!
//! ## Absence, again
//!
//! The rule this module exists to hold: *a stale reading is not a current one*.
//! A `StreamMonitor` keeps its cached samples across a reconnect on purpose —
//! a UI should keep drawing the last traces — so the samples are still there
//! when the stream has gone `Stale`. Reporting them into a turn record would
//! date-stamp old signal as this turn's health, and nothing downstream could
//! tell. So every guard below returns `None`, which the log writes as an absent
//! `channelHealth` field, and absent means absent.
//!
//! Thresholds carried from `channel_health.rs` and `electrode_check.rs`, whose
//! own headers say they are heuristics derived from one subject and one mains
//! environment. That is why a turn record is a `heuristicAnnotation` and never
//! ingestible as evidence (ADR-004).

use crate::turn_log::ChannelHealthLine;
use neuralcompose_mobile_core::channel_health::ChannelHealthThresholds;
use neuralcompose_mobile_core::electrode_check::{
    assess_channel, common_mode_hint, MainsThresholds,
};
use neuralcompose_mobile_core::presentation::StreamPhase;
use neuralcompose_mobile_core::types::{CHANNEL_COUNT, CHANNEL_ORDER};

/// The frozen wire sample rate (`contracts/constants.json`). Both the BLE
/// bridge and the fixture server serve at this rate; it is a contract value,
/// not a tuning knob.
pub const SAMPLE_RATE_HZ: f64 = 256.0;

/// What one turn learned from the headband.
///
/// `lines` is what gets persisted. `blocking` and `common_mode` are for the
/// operator's terminal and are deliberately not logged — advice is not a
/// measurement, and a turn record should not carry a sentence telling a future
/// reader to reseat an electrode that was reseated months ago.
#[derive(Clone, Debug, PartialEq)]
pub struct EegTurnReading {
    /// Always all four channels, always in `TP9, AF7, AF8, TP10` order.
    pub lines: Vec<ChannelHealthLine>,
    /// One entry per channel whose verdict blocks a usable recording.
    pub blocking: Vec<String>,
    /// Set when the same fault appears on enough channels to suggest a shared
    /// cause (reference/ground) rather than four separate ones.
    pub common_mode: Option<&'static str>,
}

/// Per-channel health for one turn, or `None` when there is nothing current and
/// complete to report.
///
/// Returns `None` — never a partial or stale answer — when any of these holds:
///
/// - the stream is not [`StreamPhase::Live`]. Cached samples outlive their
///   freshness by design; see the module header;
/// - the montage is not all four channels. `TP9/AF7/AF8/TP10` is anatomy, not
///   slots, so three channels are not a three-quarters answer;
/// - any channel has no samples yet;
/// - any computed RMS is non-finite. This one is not hypothetical: `f64::NAN`
///   serializes to JSON `null`, and [`ChannelHealthLine::rms_microvolts`] is a
///   plain `f64`, so a NaN would write a line that cannot be read back — a
///   corrupt log that passes its own digest check.
pub fn eeg_reading_for_turn(
    phase: StreamPhase,
    channels: &[Vec<f64>],
    sample_rate_hz: f64,
    health: ChannelHealthThresholds,
    mains: MainsThresholds,
) -> Option<EegTurnReading> {
    if !matches!(phase, StreamPhase::Live) {
        return None;
    }
    if channels.len() != CHANNEL_COUNT || channels.iter().any(|c| c.is_empty()) {
        return None;
    }

    let reports: Vec<_> = channels
        .iter()
        .map(|c| assess_channel(c, sample_rate_hz, health, mains))
        .collect();
    if reports.iter().any(|r| !r.rms.is_finite()) {
        return None;
    }

    Some(EegTurnReading {
        lines: CHANNEL_ORDER
            .iter()
            .zip(&reports)
            .map(|(name, r)| ChannelHealthLine {
                channel: (*name).to_string(),
                rms_microvolts: r.rms,
                status: r.health,
            })
            .collect(),
        blocking: CHANNEL_ORDER
            .iter()
            .zip(&reports)
            .filter(|(_, r)| r.verdict.is_blocking())
            .map(|(name, r)| format!("{name}: {}", r.verdict.advice()))
            .collect(),
        common_mode: common_mode_hint(&reports),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use neuralcompose_mobile_core::channel_health::ChannelHealthStatus;

    /// A sinusoid of the given amplitude. RMS is `amplitude / sqrt(2)`, which is
    /// what puts each case either side of the thresholds.
    fn wave(amplitude: f64) -> Vec<f64> {
        (0..512)
            .map(|i| amplitude * (i as f64 * 0.1).sin())
            .collect()
    }

    fn healthy_montage() -> Vec<Vec<f64>> {
        vec![wave(20.0), wave(20.0), wave(20.0), wave(20.0)]
    }

    fn reading(phase: StreamPhase, channels: &[Vec<f64>]) -> Option<EegTurnReading> {
        eeg_reading_for_turn(
            phase,
            channels,
            SAMPLE_RATE_HZ,
            ChannelHealthThresholds::default(),
            MainsThresholds::default(),
        )
    }

    #[test]
    fn a_live_stream_reports_all_four_channels_in_the_frozen_order() {
        let r = reading(StreamPhase::Live, &healthy_montage()).expect("live montage reports");
        assert_eq!(r.lines.len(), CHANNEL_COUNT);
        let order: Vec<&str> = r.lines.iter().map(|l| l.channel.as_str()).collect();
        assert_eq!(order, CHANNEL_ORDER);
        for line in &r.lines {
            assert!(line.rms_microvolts.is_finite());
            assert_eq!(line.status, ChannelHealthStatus::Healthy);
        }
    }

    /// The rule the module exists for. `StreamMonitor` keeps its samples across
    /// a reconnect, so a stale stream still HAS a full montage to report — and
    /// reporting it would date old signal as this turn's.
    #[test]
    fn a_stale_stream_reports_nothing_even_though_it_still_has_samples() {
        let montage = healthy_montage();
        assert!(
            reading(StreamPhase::Live, &montage).is_some(),
            "precondition: this montage is otherwise reportable"
        );
        assert_eq!(reading(StreamPhase::Stale { age_ms: 2001 }, &montage), None);
    }

    /// Every non-Live phase, so no future phase quietly starts reporting.
    #[test]
    fn no_phase_but_live_reports_anything() {
        let montage = healthy_montage();
        for phase in [
            StreamPhase::Connecting,
            StreamPhase::OpenNoData,
            StreamPhase::Stale { age_ms: 1 },
            StreamPhase::Closed,
            StreamPhase::Error,
        ] {
            assert_eq!(reading(phase, &montage), None, "{phase:?} reported health");
        }
        assert!(reading(StreamPhase::Live, &montage).is_some());
    }

    /// Three channels is not a three-quarters answer — the montage is anatomy.
    #[test]
    fn a_partial_montage_is_refused_rather_than_reported_short() {
        let short = vec![wave(20.0), wave(20.0), wave(20.0)];
        assert_eq!(reading(StreamPhase::Live, &short), None);
        let long = vec![wave(20.0); 5];
        assert_eq!(reading(StreamPhase::Live, &long), None);
    }

    #[test]
    fn a_channel_with_no_samples_yet_refuses_the_whole_reading() {
        let mut montage = healthy_montage();
        montage[1].clear();
        assert_eq!(reading(StreamPhase::Live, &montage), None);
    }

    /// A NaN reaching the log would serialize to `null` against an `f64` field —
    /// a line that passes the payload digest and then cannot be parsed back.
    #[test]
    fn a_non_finite_sample_never_reaches_a_turn_line() {
        for poison in [f64::NAN, f64::INFINITY] {
            let mut montage = healthy_montage();
            montage[2][7] = poison;
            assert_eq!(
                reading(StreamPhase::Live, &montage),
                None,
                "{poison} produced a reading"
            );
        }
    }

    /// A bad channel is REPORTED, not dropped. Refusing the whole montage on a
    /// dead electrode would hide the single most useful thing the log can say.
    #[test]
    fn a_dead_or_saturated_channel_is_reported_not_omitted() {
        let mut montage = healthy_montage();
        montage[0] = wave(1.0); // rms ~0.71, below the 2.0 dead floor
        montage[3] = wave(400.0); // rms ~283, above the 200.0 saturation bar
        let r = reading(StreamPhase::Live, &montage).expect("still reportable");
        assert_eq!(r.lines.len(), CHANNEL_COUNT);
        assert_eq!(r.lines[0].status, ChannelHealthStatus::Dead);
        assert_eq!(r.lines[3].status, ChannelHealthStatus::Saturated);
        assert_eq!(r.lines[1].status, ChannelHealthStatus::Healthy);
    }

    /// The persisted half must survive a JSON round trip, because that is what
    /// `verify_turn_log` will do to it.
    #[test]
    fn the_reported_lines_round_trip_through_json() {
        let r = reading(StreamPhase::Live, &healthy_montage()).unwrap();
        let encoded = serde_json::to_string(&r.lines).expect("serializable");
        assert!(
            !encoded.contains("null"),
            "a null reached the health lines: {encoded}"
        );
        let back: Vec<ChannelHealthLine> = serde_json::from_str(&encoded).expect("round trips");
        assert_eq!(back, r.lines);
    }

    /// Advice is for the terminal, never the record.
    #[test]
    fn blocking_advice_is_not_part_of_the_persisted_lines() {
        let mut montage = healthy_montage();
        montage[1] = wave(0.2);
        let r = reading(StreamPhase::Live, &montage).expect("reportable");
        let encoded = serde_json::to_string(&r.lines).unwrap();
        for advice in &r.blocking {
            assert!(!encoded.contains(advice.as_str()));
        }
    }
}
