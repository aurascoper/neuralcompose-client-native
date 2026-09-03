//! Whether a recorded session may enter the corpus — as a **query over what the
//! session recorded**, not a verdict someone reached afterwards.
//!
//! ## Why this is a query and not a judgement
//!
//! "0 of 4 sessions are eligible" was a sentence in a report. Nothing could ask
//! *why* a particular session was excluded without re-deriving it from the raw
//! signal, and a session that became eligible after a fix could only be noticed
//! by someone who remembered to look. Both of those are properties of a verdict.
//! A query has neither: the reasons are on the record, and re-running it after a
//! fix is free.
//!
//! That is what [`EegRefusal`](crate::eeg::EegRefusal) ids and
//! [`ChannelAnnotation`](crate::turn_log::ChannelAnnotation) are for. This
//! module only counts them.
//!
//! ## The pre-registration, and what it is worth
//!
//! Thresholds live in `contracts/eeg/eligibility-v1.json`, sealed by a sha256
//! beside it, and [`Thresholds::load_sealed`] refuses a file whose digest does
//! not match — the same rule `tools/spoken-loop/dialectic-relay` applies to its
//! own preregistration.
//!
//! **The seal proves the file was not edited. It does not make the numbers
//! right.** They are calibrated on nothing: the 0-of-4 prior comes from
//! 83-second block-design runs where the subject sat still on a timer, and a
//! conversational session involves speech, which moves the jaw and the
//! electrodes. The registration says so in its own text, and lists what a
//! non-discriminating outcome would look like, so that the first surprise is a
//! recorded outcome rather than an argument about whether the number was ever
//! reasonable.
//!
//! A threshold that turns out wrong is replaced by `eligibility-v2.json` with
//! its own seal and date. This file is never edited to fit data.

use crate::turn_log::TurnLine;
use neuralcompose_mobile_core::audio::sha256_hex;
use neuralcompose_mobile_core::channel_health::ChannelHealthStatus;
use serde::{Deserialize, Serialize};

/// The verdict-bearing half of the pre-registration. Deserialized from the
/// sealed contract; the rationale prose in that file is not modelled here
/// because nothing computes on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thresholds {
    pub min_turns_with_reading: u64,
    pub max_absent_fraction: f64,
    pub max_saturated_fraction: f64,
    pub max_mains_pickup_fraction: f64,
}

/// The sealed document, as far as this code reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
    pub schema_id: String,
    pub registered_on: String,
    pub required_channels: Vec<String>,
    pub thresholds: Thresholds,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SealError {
    Unreadable(String),
    /// The digest did not match. Refused rather than warned: a registration
    /// that can be edited without notice is not a registration.
    DigestMismatch {
        expected: String,
        found: String,
    },
    Malformed(String),
}

impl Registration {
    /// Parses the registration only if `bytes` hashes to `expected_sha256_hex`.
    ///
    /// Pure, so it is testable without a filesystem; the shell reads the two
    /// files and hands the contents over.
    pub fn load_sealed(bytes: &[u8], expected_sha256_hex: &str) -> Result<Self, SealError> {
        let found = sha256_hex(bytes.to_vec());
        let expected = expected_sha256_hex.trim().to_lowercase();
        if found != expected {
            return Err(SealError::DigestMismatch { expected, found });
        }
        serde_json::from_slice(bytes).map_err(|e| SealError::Malformed(e.to_string()))
    }
}

/// One channel's tally across a session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelTally {
    pub channel: String,
    pub turns_with_reading: u64,
    pub saturated: u64,
    pub dead: u64,
    pub mains_pickup: u64,
}

impl ChannelTally {
    /// `None` rather than `0.0` when there is nothing to divide by. A session
    /// with no readings has no saturated *fraction*, and reporting 0.0 would
    /// read as "never saturated" — the cleanest possible result — for the case
    /// where nothing was measured at all.
    pub fn saturated_fraction(&self) -> Option<f64> {
        (self.turns_with_reading > 0)
            .then(|| self.saturated as f64 / self.turns_with_reading as f64)
    }

    pub fn mains_pickup_fraction(&self) -> Option<f64> {
        (self.turns_with_reading > 0)
            .then(|| self.mains_pickup as f64 / self.turns_with_reading as f64)
    }
}

/// Everything counted, before any threshold is applied.
///
/// Separate from the verdict so the numbers can be reported for an ineligible
/// session too — "why" is the whole point, and a bare `false` answers nothing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTally {
    pub turns: u64,
    pub turns_with_reading: u64,
    pub turns_absent: u64,
    /// Refusal ids and their counts, in descending order of frequency then
    /// alphabetical — deterministic, so two runs over the same log agree.
    pub absent_reasons: Vec<(String, u64)>,
    pub channels: Vec<ChannelTally>,
}

impl SessionTally {
    pub fn absent_fraction(&self) -> Option<f64> {
        (self.turns > 0).then(|| self.turns_absent as f64 / self.turns as f64)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub eligible: bool,
    /// Why, in both directions. Populated whether or not the session passed:
    /// an eligible session that cleared every bar by a hair is a different fact
    /// from one that cleared them all comfortably.
    pub reasons: Vec<String>,
    pub tally: SessionTally,
}

/// Counts what the turn log recorded. No thresholds here — counting and judging
/// are separated so a threshold change cannot silently alter the counts.
pub fn tally(lines: &[TurnLine], required_channels: &[String]) -> SessionTally {
    let mut turns_with_reading = 0u64;
    let mut turns_absent = 0u64;
    let mut reasons: std::collections::BTreeMap<String, u64> = Default::default();
    let mut channels: Vec<ChannelTally> = required_channels
        .iter()
        .map(|c| ChannelTally {
            channel: c.clone(),
            turns_with_reading: 0,
            saturated: 0,
            dead: 0,
            mains_pickup: 0,
        })
        .collect();

    for line in lines {
        match &line.channel_health {
            Some(records) => {
                turns_with_reading += 1;
                for tally in channels.iter_mut() {
                    let Some(r) = records.iter().find(|r| r.channel == tally.channel) else {
                        // A required channel missing from a present reading is
                        // not a healthy turn for that channel. It is also not
                        // counted as saturated or dead, because nothing was
                        // said about it either way.
                        continue;
                    };
                    tally.turns_with_reading += 1;
                    match r.annotation.status {
                        ChannelHealthStatus::Saturated => tally.saturated += 1,
                        ChannelHealthStatus::Dead => tally.dead += 1,
                        _ => {}
                    }
                    if r.annotation.verdict == "mains-pickup" {
                        tally.mains_pickup += 1;
                    }
                }
            }
            None => {
                turns_absent += 1;
                // An absent reading with no recorded reason predates the reason
                // field or was written by something that did not set it. Named
                // rather than dropped, so the gap is visible in the tally
                // instead of quietly reducing the denominator.
                let id = line
                    .channel_health_absent_reason
                    .clone()
                    .unwrap_or_else(|| "unrecorded".to_string());
                *reasons.entry(id).or_default() += 1;
            }
        }
    }

    let mut absent_reasons: Vec<(String, u64)> = reasons.into_iter().collect();
    absent_reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    SessionTally {
        turns: lines.len() as u64,
        turns_with_reading,
        turns_absent,
        absent_reasons,
        channels,
    }
}

/// Applies the pre-registered thresholds to a tally.
///
/// Every failing check appends a reason and none short-circuits: a session that
/// fails three ways should say so, because fixing only the first would look
/// like progress and change nothing.
pub fn evaluate(tally: SessionTally, thresholds: &Thresholds) -> Verdict {
    let mut reasons = Vec::new();
    let mut eligible = true;

    if tally.turns_with_reading < thresholds.min_turns_with_reading {
        eligible = false;
        reasons.push(format!(
            "only {} turns carried a reading, below the registered minimum of {}",
            tally.turns_with_reading, thresholds.min_turns_with_reading
        ));
    }

    match tally.absent_fraction() {
        Some(f) if f > thresholds.max_absent_fraction => {
            eligible = false;
            reasons.push(format!(
                "{:.0}% of turns had no reading, above the registered {:.0}% ({})",
                f * 100.0,
                thresholds.max_absent_fraction * 100.0,
                describe_reasons(&tally.absent_reasons)
            ));
        }
        Some(_) => {}
        None => {
            eligible = false;
            reasons.push("the session has no turns at all".to_string());
        }
    }

    for c in &tally.channels {
        // A required channel that never produced a reading fails on its own,
        // and says so rather than passing every fraction test vacuously.
        let (Some(sat), Some(mains)) = (c.saturated_fraction(), c.mains_pickup_fraction()) else {
            eligible = false;
            reasons.push(format!(
                "{}: required channel never produced a reading",
                c.channel
            ));
            continue;
        };
        if sat > thresholds.max_saturated_fraction {
            eligible = false;
            reasons.push(format!(
                "{}: saturated on {:.0}% of readings, above the registered {:.0}%",
                c.channel,
                sat * 100.0,
                thresholds.max_saturated_fraction * 100.0
            ));
        }
        if mains > thresholds.max_mains_pickup_fraction {
            eligible = false;
            reasons.push(format!(
                "{}: mains pickup on {:.0}% of readings, above the registered {:.0}%",
                c.channel,
                mains * 100.0,
                thresholds.max_mains_pickup_fraction * 100.0
            ));
        }
        if c.dead > 0 {
            // Not disqualifying on its own — the electrode may have lifted for
            // part of a session — but never silent, because a dead channel is
            // the single most useful thing the log can say.
            reasons.push(format!(
                "{}: read as dead on {} of {} readings",
                c.channel, c.dead, c.turns_with_reading
            ));
        }
    }

    if eligible {
        reasons.push(format!(
            "cleared every registered threshold: {} of {} turns carried a reading",
            tally.turns_with_reading, tally.turns
        ));
    }

    Verdict {
        eligible,
        reasons,
        tally,
    }
}

fn describe_reasons(reasons: &[(String, u64)]) -> String {
    if reasons.is_empty() {
        return "no reasons recorded".to_string();
    }
    reasons
        .iter()
        .map(|(id, n)| format!("{id} x{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn_log::{
        annotation_envelope, derived_envelope, ChannelAnnotation, ChannelDerived, ChannelRecord,
        ChannelWindow,
    };
    use neuralcompose_mobile_core::provenance::{MethodIdentity, ResourceRef};

    const SEALED: &[u8] = include_bytes!("../../../contracts/eeg/eligibility-v1.json");
    const SEAL: &str = include_str!("../../../contracts/eeg/eligibility-v1.json.sha256");

    fn method() -> MethodIdentity {
        MethodIdentity {
            method_id: "test".into(),
            software_id: "test".into(),
            software_version: "0".into(),
            git_commit: None,
            parameters_digest: "0".repeat(64),
        }
    }

    fn rec(channel: &str, status: ChannelHealthStatus, verdict: &str) -> ChannelRecord {
        let w = ResourceRef {
            resource_kind: "eeg-window".into(),
            sha256_hex: "b".repeat(64),
            locator: None,
        };
        ChannelRecord {
            channel: channel.into(),
            window: ChannelWindow {
                sample_count: 1280,
                last_source_timestamp: Some(1.0),
            },
            derived: ChannelDerived {
                rms_microvolts: 40.0,
                mains_power: Some(5.0),
                provenance: derived_envelope(method(), w.clone()),
            },
            annotation: ChannelAnnotation {
                status,
                verdict: verdict.into(),
                mains_line_hz: Some(60.0),
                provenance: annotation_envelope(method(), w),
            },
        }
    }

    /// The registration in the tree must be readable by the code that depends
    /// on it. Committed together, so a divergence is a build failure rather
    /// than a discovery on session night.
    #[test]
    fn the_committed_registration_verifies_against_its_own_seal() {
        let r = Registration::load_sealed(SEALED, SEAL).expect("the committed seal matches");
        assert_eq!(r.schema_id, "neuralcompose.eeg.eligibility.v1");
        assert_eq!(r.required_channels, vec!["TP10".to_string()]);
        assert_eq!(r.thresholds.min_turns_with_reading, 8);
    }

    /// The seal has to actually refuse. A check that only ever passes is the
    /// pre-registration equivalent of a test that asserts nothing.
    #[test]
    fn an_edited_registration_is_refused_rather_than_read() {
        let mut edited = SEALED.to_vec();
        // Flip one digit of one threshold — the smallest possible post-hoc
        // adjustment, and exactly the kind a seal exists to catch.
        let needle = b"\"minTurnsWithReading\": 8";
        let pos = edited
            .windows(needle.len())
            .position(|w| w == needle)
            .expect("threshold present");
        edited[pos + needle.len() - 1] = b'2';

        match Registration::load_sealed(&edited, SEAL) {
            Err(SealError::DigestMismatch { .. }) => {}
            other => panic!("an edited registration was accepted: {other:?}"),
        }
    }

    #[test]
    fn a_clean_session_is_eligible_and_says_why() {
        let lines: Vec<TurnLine> = (0..10)
            .map(|_| {
                crate::turn_log::TurnLine::silent_for_test().with_channel_health(vec![rec(
                    "TP10",
                    ChannelHealthStatus::Healthy,
                    "ok",
                )])
            })
            .collect();
        let t = tally(&lines, &["TP10".to_string()]);
        let v = evaluate(t, &thresholds());
        assert!(v.eligible, "{:?}", v.reasons);
        assert!(!v.reasons.is_empty(), "an eligible session gave no reasons");
    }

    /// The case the whole module exists for: ineligible, with the reason on the
    /// record rather than re-derived.
    #[test]
    fn an_ineligible_session_names_every_failing_threshold_not_just_the_first() {
        let mut lines: Vec<TurnLine> = (0..6)
            .map(|_| {
                crate::turn_log::TurnLine::silent_for_test().with_channel_health(vec![rec(
                    "TP10",
                    ChannelHealthStatus::Saturated,
                    "mains-pickup",
                )])
            })
            .collect();
        lines.extend((0..6).map(|_| {
            crate::turn_log::TurnLine::silent_for_test()
                .with_channel_health_absent("band-exactly-zero")
        }));

        let t = tally(&lines, &["TP10".to_string()]);
        assert_eq!(t.absent_reasons, vec![("band-exactly-zero".to_string(), 6)]);

        let v = evaluate(t, &thresholds());
        assert!(!v.eligible);
        let joined = v.reasons.join(" | ");
        assert!(joined.contains("below the registered minimum"), "{joined}");
        assert!(joined.contains("no reading"), "{joined}");
        assert!(joined.contains("saturated"), "{joined}");
        assert!(joined.contains("mains pickup"), "{joined}");
        assert!(joined.contains("band-exactly-zero"), "{joined}");
    }

    /// A required channel that never reported must fail loudly. With fractions
    /// as `Option` this is a distinct branch; with `0.0` it would have passed
    /// every fraction test as though it were the cleanest session on record.
    #[test]
    fn a_channel_that_never_reported_fails_instead_of_scoring_a_perfect_zero() {
        let lines: Vec<TurnLine> = (0..10)
            .map(|_| {
                crate::turn_log::TurnLine::silent_for_test().with_channel_health(vec![rec(
                    "TP9",
                    ChannelHealthStatus::Healthy,
                    "ok",
                )])
            })
            .collect();
        let t = tally(&lines, &["TP10".to_string()]);
        assert_eq!(t.channels[0].turns_with_reading, 0);
        assert_eq!(t.channels[0].saturated_fraction(), None);

        let v = evaluate(t, &thresholds());
        assert!(!v.eligible);
        assert!(
            v.reasons.iter().any(|r| r.contains("never produced")),
            "{:?}",
            v.reasons
        );
    }

    /// An absence with no recorded reason is named, not dropped. Silently
    /// skipping it would shrink the denominator and make an unexplained session
    /// look cleaner than an explained one.
    #[test]
    fn an_absence_with_no_recorded_reason_is_counted_as_unrecorded() {
        let mut line = crate::turn_log::TurnLine::silent_for_test();
        line.channel_health = None;
        line.channel_health_absent_reason = None;
        let t = tally(&[line], &["TP10".to_string()]);
        assert_eq!(t.turns_absent, 1);
        assert_eq!(t.absent_reasons, vec![("unrecorded".to_string(), 1)]);
    }

    fn thresholds() -> Thresholds {
        Registration::load_sealed(SEALED, SEAL).unwrap().thresholds
    }
}
