//! Opt-in local record of one dialectical turn, and the envelope that makes a
//! session file checkable.
//!
//! Port of `Sources/BCICore/Telemetry/DialecticalTurnEvent.swift`. The envelope
//! (one record per line, a manifest, a sha256 over the payload, a `verify_*`
//! that re-reads rather than trusts) deliberately reuses the shape
//! `mobile-core`'s `capture.rs` already established rather than inventing a
//! second one.
//!
//! What this records is "the history of transformations": not just the chosen
//! reply, but both candidates, their scored energies, the tension, and which
//! basin — or silence, or synthesis — resolved it. A trajectory of these is a
//! path through the dialogue's semantic space.
//!
//! Privacy contract, carried from ADR-005: this is **local, opt-in and never
//! transmitted**, off unless the caller passes `--log`. It carries text and
//! scalar scores only — never a raw embedding. That is the same line the Swift
//! drew, and for the same reason: logging a continuous vector reads as more
//! authoritative than the spectral estimator's own honesty caveat allows.

use crate::dynamics::{DialecticalOutcome, Resolution, ScoredCandidate};
use neuralcompose_mobile_core::audio::sha256_hex;
use neuralcompose_mobile_core::channel_health::ChannelHealthStatus;
use neuralcompose_mobile_core::types::CHANNEL_ORDER;
use serde::{Deserialize, Serialize};

pub const TURN_LINE_SCHEMA: &str = "neuralcompose.hypnagogic.turn.v1";
pub const TURN_MANIFEST_SCHEMA: &str = "neuralcompose.hypnagogic.turnlog.v1";

/// The neutral gloss. `SpectralState` comes from a Core ML classifier with no
/// Linux runtime, so on this platform the scalar is always neutral and
/// [`TurnLine::spectral_state`] is always `None`.
///
/// It is recorded as a real value rather than omitted **on purpose**: 0.5 means
/// both "the estimator said neutral" and "there was no estimator", so an absent
/// field could not later be told apart from a genuine neutral reading. Present
/// and neutral is a fact; absent is a hole.
pub const NEUTRAL_GLOSS: f32 = 0.5;

/// One scored candidate as persisted — text and scalars, never the embedding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateLine {
    pub text: String,
    pub role_id: String,
    pub coherence: f32,
    pub resonance: f32,
    pub novelty: f32,
    pub potential: f32,
}

/// Per-channel signal health at the time of the turn. Absent entirely when no
/// EEG source is attached; when present it is always all four channels in the
/// frozen `TP9, AF7, AF8, TP10` order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHealthLine {
    pub channel: String,
    pub rms_microvolts: f64,
    pub status: ChannelHealthStatus,
}

/// One turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnLine {
    pub schema_id: String,
    pub index: u64,
    pub mode: String,
    pub heard: String,
    pub candidates: Vec<CandidateLine>,
    pub tension: f32,
    pub margin: f32,
    pub selection_temperature: f32,
    /// See [`NEUTRAL_GLOSS`]. Always present, never `null`.
    pub gloss_scalar: f32,
    /// The estimator's badge label, or `None` when no estimator produced one —
    /// which on Linux is always. Disambiguates a real neutral from an absent
    /// one, which `gloss_scalar` alone cannot.
    pub spectral_state: Option<String>,
    /// `"spoke:<roleID>"`, `"synthesized:<roleID>"`, or `"silent"`.
    pub outcome: String,
    /// The words actually voiced; `None` on a silent turn.
    pub spoken_text: Option<String>,
    /// The Witness's observation — Reflective only. Text only, and never the
    /// spoken output.
    pub witness_finding: Option<String>,
    /// How far the witness's observation sits from what got voiced.
    pub witness_distance: Option<f32>,
    /// How much this turn's utterance collapsed toward the running average of
    /// the dialogue's own replies. Logged for every profile.
    pub self_similarity: Option<f32>,
    /// Whether the Witness RAN — true even if the call failed or produced
    /// nothing, so a broken witness is distinguishable from a disabled one.
    pub witness_attempted: Option<bool>,
    pub channel_health: Option<Vec<ChannelHealthLine>>,
}

impl TurnLine {
    /// Builds a record from a resolved competition.
    pub fn new(
        index: u64,
        mode: &str,
        heard: &str,
        scored: &[ScoredCandidate],
        resolution: &Resolution,
    ) -> Self {
        let (outcome, spoken_text) = match &resolution.outcome {
            DialecticalOutcome::Spoke(c) => (format!("spoke:{}", c.role_id), Some(c.text.clone())),
            DialecticalOutcome::Synthesized(c) => {
                (format!("synthesized:{}", c.role_id), Some(c.text.clone()))
            }
            DialecticalOutcome::Silent => ("silent".to_string(), None),
        };
        Self {
            schema_id: TURN_LINE_SCHEMA.to_string(),
            index,
            mode: mode.to_string(),
            heard: heard.to_string(),
            candidates: scored
                .iter()
                .map(|s| CandidateLine {
                    text: s.candidate.text.clone(),
                    role_id: s.candidate.role_id.clone(),
                    coherence: s.energy.coherence,
                    resonance: s.energy.resonance,
                    novelty: s.energy.novelty,
                    potential: s.potential,
                })
                .collect(),
            tension: resolution.tension,
            margin: resolution.margin,
            selection_temperature: resolution.selection_temperature,
            gloss_scalar: NEUTRAL_GLOSS,
            spectral_state: None,
            outcome,
            spoken_text,
            witness_finding: None,
            witness_distance: None,
            self_similarity: None,
            witness_attempted: None,
            channel_health: None,
        }
    }

    pub fn with_channel_health(mut self, health: Vec<ChannelHealthLine>) -> Self {
        self.channel_health = Some(health);
        self
    }

    pub fn with_witness(
        mut self,
        attempted: bool,
        finding: Option<String>,
        distance: Option<f32>,
    ) -> Self {
        self.witness_attempted = Some(attempted);
        self.witness_finding = finding;
        self.witness_distance = distance;
        self
    }

    pub fn with_self_similarity(mut self, value: f32) -> Self {
        self.self_similarity = Some(value);
        self
    }

    pub fn is_silent(&self) -> bool {
        self.outcome == "silent"
    }
}

/// Encode one line exactly as it must appear in the `.jsonl` file: compact
/// JSON, no trailing newline — the shell appends the separator.
pub fn encode_turn_line(line: &TurnLine) -> String {
    serde_json::to_string(line).expect("TurnLine is always serializable")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnLogManifest {
    pub schema_id: String,
    pub line_schema_id: String,
    pub session_id: String,
    pub mode: String,
    pub turn_count: u64,
    pub silent_turn_count: u64,
    pub payload_byte_size: u64,
    pub payload_sha256_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TurnLogFailure {
    ManifestSchemaMismatch,
    PayloadSizeMismatch,
    PayloadDigestMismatch,
    MalformedLine {
        line_number: u64,
    },
    LineSchemaMismatch {
        line_number: u64,
    },
    IndexOutOfOrder {
        line_number: u64,
    },
    ModeMismatch {
        line_number: u64,
    },
    /// `gloss_scalar` outside `[0, 1]` — a corrupt record, since the field is a
    /// normalized bias scalar.
    GlossOutOfRange {
        line_number: u64,
    },
    /// Channel health present but not all four channels in the frozen order.
    ChannelOrderMismatch {
        line_number: u64,
    },
    TurnCountMismatch,
    SilentCountMismatch,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TurnLogVerdict {
    Verified {
        turn_count: u64,
        silent_turn_count: u64,
    },
    Failed {
        failure: TurnLogFailure,
    },
}

/// Accumulates a session. Effect-free: returns the exact bytes the shell must
/// append and the manifest it must publish, and never touches a file.
#[derive(Debug)]
pub struct TurnLogRecorder {
    session_id: String,
    mode: String,
    payload: String,
    turn_count: u64,
    silent_turn_count: u64,
}

impl TurnLogRecorder {
    pub fn new(session_id: impl Into<String>, mode: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            mode: mode.into(),
            payload: String::new(),
            turn_count: 0,
            silent_turn_count: 0,
        }
    }

    /// Records one turn and returns the line the shell appends (without its
    /// newline separator).
    pub fn on_turn(&mut self, line: &TurnLine) -> String {
        let encoded = encode_turn_line(line);
        self.payload.push_str(&encoded);
        self.payload.push('\n');
        self.turn_count += 1;
        if line.is_silent() {
            self.silent_turn_count += 1;
        }
        encoded
    }

    pub fn turn_count(&self) -> u64 {
        self.turn_count
    }

    pub fn silent_turn_count(&self) -> u64 {
        self.silent_turn_count
    }

    /// The manifest describing what has been recorded so far. The shell must
    /// publish this only *after* the payload it describes is durable.
    pub fn manifest(&self) -> TurnLogManifest {
        TurnLogManifest {
            schema_id: TURN_MANIFEST_SCHEMA.to_string(),
            line_schema_id: TURN_LINE_SCHEMA.to_string(),
            session_id: self.session_id.clone(),
            mode: self.mode.clone(),
            turn_count: self.turn_count,
            silent_turn_count: self.silent_turn_count,
            payload_byte_size: self.payload.len() as u64,
            payload_sha256_hex: sha256_hex(self.payload.as_bytes().to_vec()),
        }
    }
}

/// Re-reads a session file and checks every claim its manifest makes, rather
/// than trusting the manifest. The bytes are checked before any interpretation.
pub fn verify_turn_log(jsonl: &str, manifest: &TurnLogManifest) -> TurnLogVerdict {
    let fail = |failure| TurnLogVerdict::Failed { failure };

    if manifest.schema_id != TURN_MANIFEST_SCHEMA || manifest.line_schema_id != TURN_LINE_SCHEMA {
        return fail(TurnLogFailure::ManifestSchemaMismatch);
    }
    let bytes = jsonl.as_bytes();
    if bytes.len() as u64 != manifest.payload_byte_size {
        return fail(TurnLogFailure::PayloadSizeMismatch);
    }
    if sha256_hex(bytes.to_vec()) != manifest.payload_sha256_hex {
        return fail(TurnLogFailure::PayloadDigestMismatch);
    }

    let mut turns = 0u64;
    let mut silent = 0u64;
    let mut expected_index = 0u64;

    for (idx, raw) in jsonl.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let line_number = idx as u64 + 1;
        let line: TurnLine = match serde_json::from_str(raw) {
            Ok(l) => l,
            Err(_) => return fail(TurnLogFailure::MalformedLine { line_number }),
        };
        if line.schema_id != TURN_LINE_SCHEMA {
            return fail(TurnLogFailure::LineSchemaMismatch { line_number });
        }
        if line.index != expected_index {
            return fail(TurnLogFailure::IndexOutOfOrder { line_number });
        }
        expected_index += 1;
        if line.mode != manifest.mode {
            return fail(TurnLogFailure::ModeMismatch { line_number });
        }
        if !(0.0..=1.0).contains(&line.gloss_scalar) || !line.gloss_scalar.is_finite() {
            return fail(TurnLogFailure::GlossOutOfRange { line_number });
        }
        if let Some(health) = &line.channel_health {
            let order: Vec<&str> = health.iter().map(|h| h.channel.as_str()).collect();
            if order != CHANNEL_ORDER {
                return fail(TurnLogFailure::ChannelOrderMismatch { line_number });
            }
        }
        turns += 1;
        if line.is_silent() {
            silent += 1;
        }
    }

    if turns != manifest.turn_count {
        return fail(TurnLogFailure::TurnCountMismatch);
    }
    if silent != manifest.silent_turn_count {
        return fail(TurnLogFailure::SilentCountMismatch);
    }
    TurnLogVerdict::Verified {
        turn_count: turns,
        silent_turn_count: silent,
    }
}

pub fn turn_log_payload_filename(session_id: &str) -> String {
    format!("{session_id}.turns.jsonl")
}

pub fn turn_log_manifest_filename(session_id: &str) -> String {
    format!("{session_id}.manifest.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamics::{
        DialecticalCandidate, DialecticalEnergy, DialecticalOutcome, Resolution, ScoredCandidate,
    };
    use crate::embedding::Embedding;

    fn candidate(role: &str) -> DialecticalCandidate {
        DialecticalCandidate {
            text: format!("{role} text"),
            embedding: Embedding::new(vec![1.0, 0.0], "fixture"),
            role_id: role.to_string(),
        }
    }

    fn scored(role: &str, potential: f32) -> ScoredCandidate {
        ScoredCandidate {
            candidate: candidate(role),
            energy: DialecticalEnergy {
                coherence: 0.6,
                resonance: 0.5,
                novelty: 0.4,
            },
            potential,
            role_fulfillment: Some(0.1),
        }
    }

    fn resolution(outcome: DialecticalOutcome) -> Resolution {
        Resolution {
            outcome,
            tension: 0.4,
            margin: 0.2,
            selection_temperature: 0.36,
            decisive: false,
        }
    }

    fn spoke_line(index: u64) -> TurnLine {
        TurnLine::new(
            index,
            "reflective",
            "what did you mean",
            &[scored("coherence", 1.2), scored("displacement", 1.0)],
            &resolution(DialecticalOutcome::Spoke(candidate("coherence"))),
        )
    }

    fn record(lines: &[TurnLine]) -> (String, TurnLogManifest) {
        let mut r = TurnLogRecorder::new("s1", "reflective");
        for l in lines {
            r.on_turn(l);
        }
        let manifest = r.manifest();
        let payload = lines
            .iter()
            .map(|l| encode_turn_line(l) + "\n")
            .collect::<String>();
        (payload, manifest)
    }

    #[test]
    fn outcomes_encode_with_their_role_provenance() {
        assert_eq!(spoke_line(0).outcome, "spoke:coherence");
        assert_eq!(spoke_line(0).spoken_text.as_deref(), Some("coherence text"));

        let silent = TurnLine::new(
            0,
            "reflective",
            "…",
            &[],
            &resolution(DialecticalOutcome::Silent),
        );
        assert_eq!(silent.outcome, "silent");
        assert_eq!(silent.spoken_text, None);
        assert!(silent.is_silent());

        let synth = TurnLine::new(
            0,
            "reflective",
            "…",
            &[],
            &resolution(DialecticalOutcome::Synthesized(candidate("third"))),
        );
        assert_eq!(synth.outcome, "synthesized:third");
    }

    /// The whole point of the field: a neutral gloss is recorded, not omitted.
    #[test]
    fn gloss_is_present_and_neutral_rather_than_absent() {
        let line = spoke_line(0);
        assert_eq!(line.gloss_scalar, NEUTRAL_GLOSS);
        assert_eq!(line.spectral_state, None);
        let json = encode_turn_line(&line);
        assert!(json.contains("\"glossScalar\":0.5"));
        assert!(json.contains("\"spectralState\":null"));
    }

    /// Text and scalars only — an embedding must never reach the file.
    #[test]
    fn no_embedding_reaches_the_payload() {
        let json = encode_turn_line(&spoke_line(0));
        assert!(!json.contains("embedding"));
        assert!(!json.contains("values"));
    }

    #[test]
    fn a_clean_session_verifies() {
        let (payload, manifest) = record(&[spoke_line(0), spoke_line(1)]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Verified {
                turn_count: 2,
                silent_turn_count: 0,
            }
        );
    }

    #[test]
    fn silent_turns_are_counted() {
        let silent = TurnLine::new(
            1,
            "reflective",
            "…",
            &[],
            &resolution(DialecticalOutcome::Silent),
        );
        let (payload, manifest) = record(&[spoke_line(0), silent]);
        assert_eq!(manifest.silent_turn_count, 1);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Verified {
                turn_count: 2,
                silent_turn_count: 1,
            }
        );
    }

    /// A single edited byte must be caught before any field is interpreted.
    #[test]
    fn a_tampered_payload_fails_on_the_digest() {
        let (payload, manifest) = record(&[spoke_line(0)]);
        let tampered = payload.replace("coherence text", "coherence texT");
        assert_eq!(tampered.len(), payload.len(), "same length on purpose");
        assert_eq!(
            verify_turn_log(&tampered, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::PayloadDigestMismatch
            }
        );
    }

    #[test]
    fn a_dropped_turn_fails_rather_than_verifying_short() {
        let (payload, manifest) = record(&[spoke_line(0), spoke_line(1)]);
        let truncated = payload.lines().next().unwrap().to_string() + "\n";
        assert!(matches!(
            verify_turn_log(&truncated, &manifest),
            TurnLogVerdict::Failed { .. }
        ));
    }

    #[test]
    fn out_of_order_indices_fail() {
        let (payload, manifest) = record(&[spoke_line(0), spoke_line(5)]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::IndexOutOfOrder { line_number: 2 }
            }
        );
    }

    #[test]
    fn a_record_from_another_mode_fails() {
        let mut mixed = spoke_line(1);
        mixed.mode = "contemplative".into();
        let (payload, manifest) = record(&[spoke_line(0), mixed]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::ModeMismatch { line_number: 2 }
            }
        );
    }

    #[test]
    fn an_out_of_range_gloss_fails() {
        let mut bad = spoke_line(0);
        bad.gloss_scalar = 1.5;
        let (payload, manifest) = record(&[bad]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::GlossOutOfRange { line_number: 1 }
            }
        );
    }

    #[test]
    fn channel_health_must_be_all_four_in_the_frozen_order() {
        let health = |channels: &[&str]| -> Vec<ChannelHealthLine> {
            channels
                .iter()
                .map(|c| ChannelHealthLine {
                    channel: c.to_string(),
                    rms_microvolts: 12.0,
                    status: ChannelHealthStatus::Healthy,
                })
                .collect()
        };

        let good = spoke_line(0).with_channel_health(health(&CHANNEL_ORDER));
        let (payload, manifest) = record(&[good]);
        assert!(matches!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Verified { .. }
        ));

        // Reordered — the same four channels in the wrong order would silently
        // mislabel every reading.
        let swapped = spoke_line(0).with_channel_health(health(&["AF7", "TP9", "AF8", "TP10"]));
        let (payload, manifest) = record(&[swapped]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::ChannelOrderMismatch { line_number: 1 }
            }
        );

        // Short.
        let partial = spoke_line(0).with_channel_health(health(&["TP9", "AF7"]));
        let (payload, manifest) = record(&[partial]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::ChannelOrderMismatch { line_number: 1 }
            }
        );
    }

    /// A disabled witness and a broken one must not look alike.
    #[test]
    fn witness_attempted_distinguishes_broken_from_disabled() {
        let disabled = spoke_line(0);
        assert_eq!(disabled.witness_attempted, None);

        let broken = spoke_line(0).with_witness(true, None, None);
        assert_eq!(broken.witness_attempted, Some(true));
        assert_eq!(broken.witness_finding, None);

        let worked = spoke_line(0).with_witness(true, Some("what both avoided".into()), Some(0.7));
        assert_eq!(worked.witness_attempted, Some(true));
        assert!(worked.witness_finding.is_some());
    }

    #[test]
    fn an_empty_session_verifies_as_empty() {
        let r = TurnLogRecorder::new("s1", "mirror");
        let manifest = r.manifest();
        assert_eq!(
            verify_turn_log("", &manifest),
            TurnLogVerdict::Verified {
                turn_count: 0,
                silent_turn_count: 0,
            }
        );
    }

    #[test]
    fn filenames_are_derived_from_the_session_id() {
        assert_eq!(
            turn_log_payload_filename("night-01"),
            "night-01.turns.jsonl"
        );
        assert_eq!(
            turn_log_manifest_filename("night-01"),
            "night-01.manifest.json"
        );
    }
}
