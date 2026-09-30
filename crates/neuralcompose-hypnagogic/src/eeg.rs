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
//! tell. So every guard below refuses, the log writes an absent
//! `channelHealth` field, and absent means absent.
//!
//! ## The refusal carries its reason
//!
//! Refusing used to be a bare `None`. It is now an [`EegRefusal`], because
//! session eligibility must be a **query over recorded annotations** rather
//! than a human verdict reconstructed later. "No reading on this turn" and "a
//! dead electrode on this turn" both produce an absent `channelHealth`, and
//! nothing downstream could previously tell them apart. The turn record now
//! carries the reason beside the absence, so a session excluded for saturation
//! is distinguishable from one excluded for a stream that never went live —
//! and one that becomes eligible after a fix is detectable rather than
//! remembered.
//!
//! Thresholds carried from `channel_health.rs` and `electrode_check.rs`, whose
//! own headers say they are heuristics derived from one subject and one mains
//! environment. That is why a turn record is a `heuristicAnnotation` and never
//! ingestible as evidence (ADR-004).

use crate::turn_log::{
    annotation_envelope, derived_envelope, window_resource_ref, ChannelAnnotation, ChannelDerived,
    ChannelRecord, ChannelWindow,
};
use neuralcompose_mobile_core::band_power::band_power;
use neuralcompose_mobile_core::channel_health::ChannelHealthThresholds;
use neuralcompose_mobile_core::electrode_check::{
    assess_channel, common_mode_hint, MainsThresholds,
};
use neuralcompose_mobile_core::presentation::StreamPhase;
use neuralcompose_mobile_core::provenance::MethodIdentity;
use neuralcompose_mobile_core::types::{CHANNEL_COUNT, CHANNEL_ORDER};

/// The frozen wire sample rate (`contracts/constants.json`). Both the BLE
/// bridge and the fixture server serve at this rate; it is a contract value,
/// not a tuning knob.
pub const SAMPLE_RATE_HZ: f64 = 256.0;

/// The canonical bands, used **only** as a gate.
///
/// Nothing computed from these is persisted. They exist to answer one
/// question — is this window measurable at all — and the values are discarded
/// immediately afterwards. Persisting them would start a hand-engineered
/// feature pipeline, which is a different project from this one and the
/// alternative to a learned representation rather than a step toward it.
pub const GATE_BANDS: [(f64, f64); 4] = [
    (0.5, 4.0),   // delta
    (4.0, 8.0),   // theta
    (8.0, 13.0),  // alpha
    (13.0, 30.0), // beta
];

/// Shortest window that can resolve the lowest band, in samples.
///
/// `lag = fs / centre_hz / 2`. With `GATE_BANDS[0]` centred at 2.25 Hz and
/// `fs = 256`, that is 57 samples. (A 2.5 Hz centre gives the 51 quoted in the
/// design note; the exact figure follows the band edges, so it is derived here
/// rather than written down.)
///
/// This is **not** the binding constraint — `band_power` refuses anything under
/// one second, which at 256 Hz is 256 samples, four times stricter. It is
/// asserted anyway because that floor lives in another crate's function and a
/// precondition that is currently implied is the cheapest thing to lose.
fn minimum_window_samples(sample_rate_hz: f64) -> usize {
    let (lo, hi) = GATE_BANDS[0];
    let centre = (lo + hi) / 2.0;
    (sample_rate_hz / centre / 2.0).ceil() as usize
}

/// Why a window was refused. Distinct variants because the reasons are not
/// interchangeable: one says the instrument could not look, the other says it
/// looked and found a channel that is not alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EegRefusal {
    /// Cached samples outlive their freshness by design; a stale reading is not
    /// a current one.
    StreamNotLive,
    /// `TP9/AF7/AF8/TP10` is anatomy, not slots — three channels are not a
    /// three-quarters answer.
    MontageIncomplete,
    /// A channel has no samples yet.
    ChannelEmpty,
    /// A NaN would serialize to JSON `null` against an `f64` field: a log line
    /// that passes its own digest check and cannot be read back.
    NonFiniteRms,
    /// The window is shorter than the lowest band can resolve.
    WindowTooShort,
    /// `band_power` returned `None` — insufficient samples, a non-finite
    /// sample, or a band this rate cannot resolve. The instrument could not
    /// look.
    BandNotMeasurable,
    /// `band_power` returned exactly `Some(0.0)`. Every detrended sample was
    /// exactly zero, so the window is constant: a dead or railed channel, never
    /// a reading. See `band_power`'s
    /// `a_constant_window_is_exactly_zero_not_merely_small`.
    ///
    /// This is a deterministic fact about the input, not a probabilistic one.
    /// It only became a usable signal when `band_power` stopped returning `0.0`
    /// as its refusal sentinel — before that, this case and
    /// `BandNotMeasurable` were the same value.
    BandExactlyZero,
}

impl EegRefusal {
    /// Stable identifier for the turn record. Never the `Debug` spelling: this
    /// goes on disk and a rename must be a deliberate schema change.
    pub fn id(self) -> &'static str {
        match self {
            EegRefusal::StreamNotLive => "stream-not-live",
            EegRefusal::MontageIncomplete => "montage-incomplete",
            EegRefusal::ChannelEmpty => "channel-empty",
            EegRefusal::NonFiniteRms => "non-finite-rms",
            EegRefusal::WindowTooShort => "window-too-short",
            EegRefusal::BandNotMeasurable => "band-not-measurable",
            EegRefusal::BandExactlyZero => "band-exactly-zero",
        }
    }
}

/// What the shell knows and this module cannot: which build computed the
/// numbers, which capture holds the samples, and where in it they sit.
///
/// Bundled into one struct rather than three more parameters because they are
/// one fact — the provenance context of this turn — and splitting them invites
/// a caller to supply two of the three.
#[derive(Clone, Debug)]
pub struct EegRecordContext<'a> {
    /// Seals the sample rate, buffer length, thresholds and band edges behind
    /// every number in the record. Built once per run, not per turn.
    pub method: MethodIdentity,
    /// The recording this window belongs to, when one is being written.
    /// `None` when the run is not capturing: the window was still digested, but
    /// no file holds it, and naming a file that does not exist would be worse
    /// than admitting there is none.
    pub recording_id: Option<&'a str>,
    /// Seconds since stream start for the newest sample in the window. `None`
    /// when the source has not supplied one — never `0.0`, which is a real
    /// timestamp meaning "the first sample of the stream".
    pub last_source_timestamp: Option<f64>,
}

/// What one turn learned from the headband.
///
/// `lines` is what gets persisted. `blocking` and `common_mode` are for the
/// operator's terminal and are deliberately not logged — advice is not a
/// measurement, and a turn record should not carry a sentence telling a future
/// reader to reseat an electrode that was reseated months ago.
#[derive(Clone, Debug, PartialEq)]
pub struct EegTurnReading {
    /// Always all four channels, always in `TP9, AF7, AF8, TP10` order.
    pub lines: Vec<ChannelRecord>,
    /// One entry per channel whose verdict blocks a usable recording.
    pub blocking: Vec<String>,
    /// Set when the same fault appears on enough channels to suggest a shared
    /// cause (reference/ground) rather than four separate ones.
    pub common_mode: Option<&'static str>,
}

/// Per-channel health for one turn, or the reason there is nothing current and
/// complete to report.
///
/// Refuses — never a partial or stale answer — for any [`EegRefusal`]. The
/// refusal is returned rather than swallowed because session eligibility has to
/// be a query over recorded reasons; "no reading this turn" and "a dead
/// electrode this turn" are not the same fact, and a later reader cannot
/// re-derive which one happened.
///
/// The order of the checks is the order of the guards below, and it is
/// deliberate: cheap structural refusals come before anything that touches the
/// samples, so a stale stream never pays for four periodograms.
pub fn eeg_reading_for_turn(
    phase: StreamPhase,
    channels: &[Vec<f64>],
    sample_rate_hz: f64,
    health: ChannelHealthThresholds,
    mains: MainsThresholds,
    ctx: EegRecordContext<'_>,
) -> Result<EegTurnReading, EegRefusal> {
    if !matches!(phase, StreamPhase::Live) {
        return Err(EegRefusal::StreamNotLive);
    }
    if channels.len() != CHANNEL_COUNT {
        return Err(EegRefusal::MontageIncomplete);
    }
    if channels.iter().any(|c| c.is_empty()) {
        return Err(EegRefusal::ChannelEmpty);
    }

    // Asserted rather than relied upon. `band_power`'s one-second floor is
    // stricter and would catch this anyway today; that floor is in another
    // crate and this is the cheapest way to notice if it stops holding.
    let minimum = minimum_window_samples(sample_rate_hz);
    if channels.iter().any(|c| c.len() < minimum) {
        return Err(EegRefusal::WindowTooShort);
    }

    // The gate. Every band of every channel must be measurable AND non-zero.
    // The two failures are distinguished, because `None` means the instrument
    // could not look and `Some(0.0)` means it looked at a channel that is not
    // alive — and only the second is evidence about the electrode.
    //
    // Nothing computed here is kept. See `GATE_BANDS`.
    for channel in channels {
        for band in GATE_BANDS {
            match band_power(channel, sample_rate_hz, band) {
                None => return Err(EegRefusal::BandNotMeasurable),
                // Matches `-0.0` as well, which is correct: negative zero is
                // still a constant window, and float patterns compare by value.
                Some(0.0) => return Err(EegRefusal::BandExactlyZero),
                Some(_) => {}
            }
        }
    }

    let reports: Vec<_> = channels
        .iter()
        .map(|c| assess_channel(c, sample_rate_hz, health, mains))
        .collect();
    if reports.iter().any(|r| !r.rms.is_finite()) {
        return Err(EegRefusal::NonFiniteRms);
    }

    Ok(EegTurnReading {
        lines: CHANNEL_ORDER
            .iter()
            .zip(channels)
            .zip(&reports)
            .map(|((name, samples), r)| {
                // One ResourceRef per channel, digesting the samples that
                // channel's numbers were actually computed from. Both tiers
                // name the same window, so a reader can see the measurement and
                // the interpretation are about the same data.
                let window_ref = window_resource_ref(samples, ctx.recording_id);
                ChannelRecord {
                    channel: (*name).to_string(),
                    window: ChannelWindow {
                        sample_count: samples.len() as u64,
                        last_source_timestamp: ctx.last_source_timestamp,
                    },
                    derived: ChannelDerived {
                        rms_microvolts: r.rms,
                        mains_power: r.mains_power,
                        provenance: derived_envelope(ctx.method.clone(), window_ref.clone()),
                    },
                    annotation: ChannelAnnotation {
                        status: r.health,
                        verdict: r.verdict.as_str().to_string(),
                        mains_line_hz: r.line_hz,
                        provenance: annotation_envelope(ctx.method.clone(), window_ref),
                    },
                }
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
    use neuralcompose_mobile_core::provenance::{
        evidence_mapping, validate, AssertionKind, EvidenceMapping,
    };

    /// A sinusoid of the given amplitude. RMS is `amplitude / sqrt(2)`, which is
    /// what puts each case either side of the thresholds.
    fn wave(amplitude: f64) -> Vec<f64> {
        wave_n(amplitude, 512)
    }

    /// The same wave at an explicit length, for the window-length guards.
    fn wave_n(amplitude: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| amplitude * (i as f64 * 0.1).sin()).collect()
    }

    fn healthy_montage() -> Vec<Vec<f64>> {
        vec![wave(20.0), wave(20.0), wave(20.0), wave(20.0)]
    }

    fn test_method() -> MethodIdentity {
        MethodIdentity {
            method_id: "test.eeg".into(),
            software_id: "test".into(),
            software_version: "0".into(),
            git_commit: None,
            parameters_digest: "0".repeat(64),
        }
    }

    fn reading(phase: StreamPhase, channels: &[Vec<f64>]) -> Result<EegTurnReading, EegRefusal> {
        eeg_reading_for_turn(
            phase,
            channels,
            SAMPLE_RATE_HZ,
            ChannelHealthThresholds::default(),
            MainsThresholds::default(),
            EegRecordContext {
                method: test_method(),
                recording_id: Some("session-test"),
                last_source_timestamp: Some(4.25),
            },
        )
    }

    /// The refusal reason, or a panic naming what was reported instead.
    fn refusal(phase: StreamPhase, channels: &[Vec<f64>]) -> EegRefusal {
        match reading(phase, channels) {
            Err(e) => e,
            Ok(r) => panic!("expected a refusal, got a reading: {r:?}"),
        }
    }

    #[test]
    fn a_live_stream_reports_all_four_channels_in_the_frozen_order() {
        let r = reading(StreamPhase::Live, &healthy_montage()).expect("live montage reports");
        assert_eq!(r.lines.len(), CHANNEL_COUNT);
        let order: Vec<&str> = r.lines.iter().map(|l| l.channel.as_str()).collect();
        assert_eq!(order, CHANNEL_ORDER);
        for line in &r.lines {
            assert!(line.derived.rms_microvolts.is_finite());
            assert_eq!(line.annotation.status, ChannelHealthStatus::Healthy);
        }
    }

    /// The three tiers, asserted where they are produced rather than only where
    /// they are verified. A measurement and an interpretation must not share an
    /// assertion kind, because the mapping to `Ingestible` is what decides
    /// whether either can ever become a training label.
    #[test]
    fn the_measurement_and_the_interpretation_carry_different_assertion_kinds() {
        let r = reading(StreamPhase::Live, &healthy_montage()).expect("reportable");
        for line in &r.lines {
            assert_eq!(
                line.derived.provenance.assertion_kind,
                AssertionKind::DerivedDeterministically,
                "{} rms is not a derivation",
                line.channel
            );
            assert_eq!(
                line.annotation.provenance.assertion_kind,
                AssertionKind::HeuristicAnnotation,
                "{} status is not an annotation",
                line.channel
            );
            // The one that actually matters: a status must never be ingestible.
            assert_eq!(
                evidence_mapping(line.annotation.provenance.assertion_kind),
                EvidenceMapping::NeverIngestible
            );
            // Neither envelope may be malformed, or the kind is decoration.
            assert!(validate(&line.derived.provenance).is_empty());
            assert!(validate(&line.annotation.provenance).is_empty());
        }
    }

    /// Both tiers name the same window, so a reader can tell the measurement
    /// and the interpretation are about the same samples — and can recompute
    /// the first from the bytes the digest names.
    #[test]
    fn both_tiers_name_the_same_window_and_the_windows_differ_per_channel() {
        let mut montage = healthy_montage();
        montage[1] = wave(31.0); // a different signal, so a different digest
        let r = reading(StreamPhase::Live, &montage).expect("reportable");

        for line in &r.lines {
            let d = &line.derived.provenance.inputs;
            let a = &line.annotation.provenance.inputs;
            assert_eq!(d.len(), 1, "{} has no window input", line.channel);
            assert_eq!(d, a, "{} tiers name different windows", line.channel);
            assert_eq!(d[0].resource_kind, "eeg-window");
            assert_eq!(d[0].locator.as_deref(), Some("session-test.eeg.jsonl"));
            assert_eq!(line.window.sample_count, 512);
            assert_eq!(line.window.last_source_timestamp, Some(4.25));
        }

        // A digest that did not depend on the samples would make every channel
        // look identical, and the reference would be decorative.
        assert_ne!(
            r.lines[0].derived.provenance.inputs[0].sha256_hex,
            r.lines[1].derived.provenance.inputs[0].sha256_hex,
            "two different channels produced the same window digest"
        );
    }

    /// Without a capture there is no file to point at, and the locator says so
    /// rather than naming one that was never written.
    #[test]
    fn a_run_without_a_capture_digests_the_window_but_names_no_file() {
        let r = eeg_reading_for_turn(
            StreamPhase::Live,
            &healthy_montage(),
            SAMPLE_RATE_HZ,
            ChannelHealthThresholds::default(),
            MainsThresholds::default(),
            EegRecordContext {
                method: test_method(),
                recording_id: None,
                last_source_timestamp: None,
            },
        )
        .expect("reportable");
        let input = &r.lines[0].derived.provenance.inputs[0];
        assert_eq!(input.locator, None);
        assert_eq!(input.sha256_hex.len(), 64, "the window is still digested");
        assert_eq!(r.lines[0].window.last_source_timestamp, None);
    }

    /// The rule the module exists for. `StreamMonitor` keeps its samples across
    /// a reconnect, so a stale stream still HAS a full montage to report — and
    /// reporting it would date old signal as this turn's.
    #[test]
    fn a_stale_stream_reports_nothing_even_though_it_still_has_samples() {
        let montage = healthy_montage();
        assert!(
            reading(StreamPhase::Live, &montage).is_ok(),
            "precondition: this montage is otherwise reportable"
        );
        assert_eq!(
            refusal(StreamPhase::Stale { age_ms: 2001 }, &montage),
            EegRefusal::StreamNotLive
        );
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
            assert_eq!(
                refusal(phase, &montage),
                EegRefusal::StreamNotLive,
                "{phase:?} reported health"
            );
        }
        assert!(reading(StreamPhase::Live, &montage).is_ok());
    }

    /// Three channels is not a three-quarters answer — the montage is anatomy.
    #[test]
    fn a_partial_montage_is_refused_rather_than_reported_short() {
        let short = vec![wave(20.0), wave(20.0), wave(20.0)];
        assert_eq!(
            refusal(StreamPhase::Live, &short),
            EegRefusal::MontageIncomplete
        );
        let long = vec![wave(20.0); 5];
        assert_eq!(
            refusal(StreamPhase::Live, &long),
            EegRefusal::MontageIncomplete
        );
    }

    #[test]
    fn a_channel_with_no_samples_yet_refuses_the_whole_reading() {
        let mut montage = healthy_montage();
        montage[1].clear();
        assert_eq!(
            refusal(StreamPhase::Live, &montage),
            EegRefusal::ChannelEmpty
        );
    }

    /// A NaN reaching the log would serialize to `null` against an `f64` field —
    /// a line that passes the payload digest and then cannot be parsed back.
    ///
    /// The reason is `BandNotMeasurable` rather than `NonFiniteRms` because the
    /// band gate runs first and `band_power` refuses any window containing a
    /// non-finite sample. Both refuse; the ordering decides which reason is
    /// recorded, and it is pinned here so a later reordering is visible rather
    /// than silently relabelling every such session.
    #[test]
    fn a_non_finite_sample_never_reaches_a_turn_line() {
        for poison in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut montage = healthy_montage();
            montage[2][7] = poison;
            assert_eq!(
                refusal(StreamPhase::Live, &montage),
                EegRefusal::BandNotMeasurable,
                "{poison} produced a reading"
            );
        }
    }

    /// The `NonFiniteRms` guard, reached the only way it still can be.
    ///
    /// A mutation campaign found this guard SURVIVING: deleting it changed
    /// nothing, because every NaN or infinity in the *input* is now caught
    /// earlier by the band gate, which refuses any window `band_power` will not
    /// measure. The guard was not redundant, though — it was merely no longer
    /// reachable by the case its own test used.
    ///
    /// It is still reachable by **overflow**: samples that are individually
    /// finite but whose squares are not. `1e200` squared is `inf`, so the sum
    /// of squares is `inf` and the RMS with it, while `band_power` returns a
    /// large-but-`Some` value and lets the window through. Alternating sign
    /// matters — a *constant* huge signal detrends to exactly zero and is
    /// caught by `BandExactlyZero` instead, which is a different guard.
    ///
    /// Without this test the guard would be deleted by anyone tidying up on
    /// the evidence that nothing depends on it.
    #[test]
    fn an_rms_that_overflows_to_infinity_is_refused_even_though_every_sample_is_finite() {
        let huge: Vec<f64> = (0..512)
            .map(|i| if i % 2 == 0 { 1e200 } else { -1e200 })
            .collect();
        assert!(
            huge.iter().all(|v| v.is_finite()),
            "precondition: every sample is finite"
        );

        let mut montage = healthy_montage();
        montage[2] = huge;
        assert_eq!(
            refusal(StreamPhase::Live, &montage),
            EegRefusal::NonFiniteRms
        );
    }

    /// The gate's whole point: a flat channel is refused, and refused for the
    /// reason that says the electrode is dead rather than the one that says the
    /// instrument could not look.
    ///
    /// This is the case the Swift path gets wrong — `JEPATransition` guards
    /// only `isFinite`, so a constant window passes as a reading of zero.
    #[test]
    fn a_flat_channel_is_refused_as_exactly_zero_not_as_unmeasurable() {
        let mut montage = healthy_montage();
        montage[0] = vec![0.0; 512];
        assert_eq!(
            refusal(StreamPhase::Live, &montage),
            EegRefusal::BandExactlyZero
        );

        // A railed channel — constant but far from zero — is the same fault.
        let mut railed = healthy_montage();
        railed[3] = vec![-1800.0; 512];
        assert_eq!(
            refusal(StreamPhase::Live, &railed),
            EegRefusal::BandExactlyZero
        );
    }

    /// A window shorter than the lowest band can resolve is refused before any
    /// periodogram runs.
    ///
    /// `band_power`'s one-second floor (256 samples here) is stricter than the
    /// lag bound (57), so this asserts the stricter of the two actually fires
    /// and that a sub-second window never yields a reading by either route.
    #[test]
    fn a_window_shorter_than_the_lowest_band_can_resolve_is_refused() {
        assert_eq!(minimum_window_samples(SAMPLE_RATE_HZ), 57);

        let too_short = vec![
            wave_n(20.0, 40),
            wave_n(20.0, 40),
            wave_n(20.0, 40),
            wave_n(20.0, 40),
        ];
        assert_eq!(
            refusal(StreamPhase::Live, &too_short),
            EegRefusal::WindowTooShort
        );

        // Past the lag bound but still under one second: `band_power` refuses.
        let sub_second = vec![
            wave_n(20.0, 200),
            wave_n(20.0, 200),
            wave_n(20.0, 200),
            wave_n(20.0, 200),
        ];
        assert_eq!(
            refusal(StreamPhase::Live, &sub_second),
            EegRefusal::BandNotMeasurable
        );
    }

    /// Every refusal id is distinct and stable. These strings go on disk, so a
    /// collision would merge two exclusion reasons in every eligibility query
    /// ever run against the corpus.
    #[test]
    fn refusal_ids_are_distinct_and_kebab_case() {
        let all = [
            EegRefusal::StreamNotLive,
            EegRefusal::MontageIncomplete,
            EegRefusal::ChannelEmpty,
            EegRefusal::NonFiniteRms,
            EegRefusal::WindowTooShort,
            EegRefusal::BandNotMeasurable,
            EegRefusal::BandExactlyZero,
        ];
        let mut ids: Vec<&str> = all.iter().map(|r| r.id()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two refusals share an id");
        for id in ids {
            assert!(
                !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{id} is not kebab-case"
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
        assert_eq!(r.lines[0].annotation.status, ChannelHealthStatus::Dead);
        assert_eq!(r.lines[3].annotation.status, ChannelHealthStatus::Saturated);
        assert_eq!(r.lines[1].annotation.status, ChannelHealthStatus::Healthy);
    }

    /// The persisted half must survive a JSON round trip, because that is what
    /// `verify_turn_log` will do to it.
    ///
    /// The original form of this test asserted the encoding contained no `null`
    /// at all. That is now wrong rather than merely strict: an unpinned build's
    /// `gitCommit`, an unscored `confidence` and an unmeasured `mainsPower` are
    /// all legitimately `null`, and the record is *required* to write them
    /// rather than omit them. What the test was actually guarding is narrower
    /// and is what it checks now — a NaN RMS serializing to `null` against an
    /// `f64` field, producing a line that passes the payload digest and then
    /// cannot be parsed back.
    #[test]
    fn the_reported_lines_round_trip_through_json() {
        let r = reading(StreamPhase::Live, &healthy_montage()).unwrap();
        let encoded = serde_json::to_string(&r.lines).expect("serializable");
        assert!(
            !encoded.contains("\"rmsMicrovolts\":null"),
            "a NaN reached an rms field: {encoded}"
        );
        let back: Vec<ChannelRecord> = serde_json::from_str(&encoded).expect("round trips");
        assert_eq!(back, r.lines);
        for line in &back {
            assert!(line.derived.rms_microvolts.is_finite());
        }
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
