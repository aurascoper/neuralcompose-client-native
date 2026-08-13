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
//! Privacy contract, carried from **the macOS repository's ADR-005** (that
//! registry is separate from this one — the local decision log runs ADR-001,
//! ADR-002, ADR-004): this is **local, opt-in and never transmitted**, off
//! unless the caller passes `--log`. It carries text and scalar scores only —
//! never a raw embedding. That is the same line the Swift drew, and for the
//! same reason: logging a continuous vector reads as more authoritative than
//! the spectral estimator's own honesty caveat allows.
//!
//! Every line carries a [`ProvenanceEnvelope`] naming the software, commit and
//! frozen tuning that produced it (ADR-004). Before that, "which build wrote
//! this record?" had no answer at all — `capture.rs`'s manifest beside it has
//! carried a build identity since M4.

use crate::dynamics::{DialecticalOutcome, Resolution, ScoredCandidate};
use crate::profile::ContextProfile;
use neuralcompose_mobile_core::audio::sha256_hex;
use neuralcompose_mobile_core::channel_health::ChannelHealthStatus;
use neuralcompose_mobile_core::provenance::{
    validate as validate_provenance, AssertionKind, MethodIdentity, ProvenanceEnvelope,
    PROVENANCE_ENVELOPE_SCHEMA,
};
use neuralcompose_mobile_core::types::CHANNEL_ORDER;
use serde::{Deserialize, Serialize};

/// v2 adds the required `provenance` envelope. Bumped rather than edited in
/// place: `contracts/README.md`'s rule is that contract changes are new
/// versions, and [`verify_turn_log`] already refuses a line whose schema it does
/// not recognize — which is the correct treatment of a v1 log by a v2 reader.
pub const TURN_LINE_SCHEMA: &str = "neuralcompose.hypnagogic.turn.v2";
pub const TURN_MANIFEST_SCHEMA: &str = "neuralcompose.hypnagogic.turnlog.v1";

/// Identifies this crate as the producing software in every turn's envelope.
pub const DIALECTIC_METHOD_ID: &str = "neuralcompose.hypnagogic.dialectic.v1";
pub const SOFTWARE_ID: &str = "neuralcompose-hypnagogic";

/// The neutral gloss. `SpectralState` comes from an **MLX**-backed estimator
/// (`Sources/BCILLM/SpectralStateEstimator.swift:23` in the macOS repository,
/// not a Core ML classifier as this comment claimed until 2026-08-13) which has
/// no Linux runtime, so on this platform the scalar is always neutral and
/// [`TurnLine::spectral_state`] is always `None`.
///
/// It is recorded as a real value rather than omitted **on purpose**: 0.5 means
/// both "the estimator said neutral" and "there was no estimator", so an absent
/// field could not later be told apart from a genuine neutral reading. Present
/// and neutral is a fact; absent is a hole.
///
/// Note what this ceiling rests on. In the Swift the gloss is *not* inert: four
/// of `SpectralGloss.scalar`'s five states are non-neutral, and the scalar
/// reaches selection through `DialecticalField.advance` → weights → potential →
/// softmax. It stays neutral here because **no estimator exists on Linux**, not
/// because the gloss cannot bias anything. Ship an estimator and this ceiling is
/// gone.
pub const NEUTRAL_GLOSS: f32 = 0.5;

/// The envelope for a turn record: which build, under which frozen tuning.
///
/// `parameters_digest` is a sha256 over the profile's own [`crate::dynamics::Tuning`],
/// so changing any knob changes the digest — no hand-maintained field list to
/// fall out of date.
///
/// `software_version` and `git_commit` come from the caller because only the
/// shell knows them, the same division of labour as `CaptureBuildIdentity`.
/// `None` for the commit means the build did not come from a checkout; it is
/// never treated as a pinned build.
pub fn dialectic_method_identity(
    profile: ContextProfile,
    software_version: impl Into<String>,
    git_commit: Option<String>,
) -> MethodIdentity {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Params {
        domain: &'static str,
        profile: &'static str,
        tuning: crate::dynamics::Tuning,
    }
    let digest = sha256_hex(
        serde_json::to_vec(&Params {
            domain: DIALECTIC_METHOD_ID,
            profile: profile.id(),
            tuning: profile.tuning(),
        })
        .expect("Tuning is always serializable"),
    );
    MethodIdentity {
        method_id: DIALECTIC_METHOD_ID.to_string(),
        software_id: SOFTWARE_ID.to_string(),
        software_version: software_version.into(),
        git_commit,
        parameters_digest: digest,
    }
}

/// A turn record is a [`AssertionKind::HeuristicAnnotation`], and therefore
/// [`neuralcompose_mobile_core::provenance::EvidenceMapping::NeverIngestible`].
///
/// That is the honest class and it is worth being blunt about why. The scores in
/// a turn line come from a tension-sharpened *sample* over weights that a
/// documented-heuristic gloss biases; `channel_health.rs:24` says its own
/// thresholds are unvalidated; and the Swift attaches `honestyCaveat` verbatim
/// wherever a `SpectralState` reaches a human. None of that is an observation,
/// and a confidence number would not make it one.
fn turn_envelope(method: MethodIdentity) -> ProvenanceEnvelope {
    ProvenanceEnvelope {
        schema_id: PROVENANCE_ENVELOPE_SCHEMA.to_string(),
        assertion_kind: AssertionKind::HeuristicAnnotation,
        method: Some(method),
        // The candidate embeddings are deliberately not logged (see the privacy
        // note above), so there is nothing to name as an input digest.
        inputs: Vec::new(),
        confidence: None,
        // A turn's outcome is not an embedding comparison; the per-candidate
        // scores inside it are, but those are not what this envelope covers.
        comparison_embedding_space: None,
    }
}

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
    /// Which build, under which frozen tuning, produced this line (ADR-004).
    pub provenance: ProvenanceEnvelope,
}

impl TurnLine {
    /// Builds a record from a resolved competition.
    pub fn new(
        index: u64,
        mode: &str,
        heard: &str,
        scored: &[ScoredCandidate],
        resolution: &Resolution,
        method: MethodIdentity,
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
            provenance: turn_envelope(method),
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
    /// The line's provenance envelope is malformed — an unusable answer to
    /// "which build produced this?" is not better than no answer, it is worse,
    /// because it looks like one.
    ProvenanceDefective {
        line_number: u64,
        defect: String,
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
        if let Some(defect) = validate_provenance(&line.provenance).first() {
            return fail(TurnLogFailure::ProvenanceDefective {
                line_number,
                defect: format!("{defect:?}"),
            });
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

    /// A fixed build identity for the record tests. Real runs get theirs from
    /// the shell; these tests only need it to be well formed.
    fn test_method() -> MethodIdentity {
        dialectic_method_identity(ContextProfile::Reflective, "0.0.0-test", None)
    }
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
            test_method(),
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
            test_method(),
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
            test_method(),
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
            test_method(),
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

    // ---- ADR-004 competency questions, made executable ----

    /// CQ1 and CQ7: *which software, at which commit, under which frozen
    /// parameters, produced this line?* Before v2 this had no answer at all.
    #[test]
    fn a_turn_line_names_its_producing_software() {
        let line = spoke_line(0);
        assert_eq!(line.schema_id, TURN_LINE_SCHEMA);
        let m = line
            .provenance
            .method
            .as_ref()
            .expect("a turn line must name its method");
        assert_eq!(m.method_id, DIALECTIC_METHOD_ID);
        assert_eq!(m.software_id, SOFTWARE_ID);
        assert!(!m.software_version.trim().is_empty());
        assert_eq!(m.parameters_digest.len(), 64);
        assert_eq!(validate_provenance(&line.provenance), []);
    }

    /// CQ8: a *pinned* build names a full 40-character commit, or names none at
    /// all. An abbreviated or dirty-tree commit is refused rather than recorded
    /// as if it identified the build.
    #[test]
    fn an_unpinned_build_says_so_rather_than_guessing() {
        let unpinned = dialectic_method_identity(ContextProfile::Reflective, "0.0.0-test", None);
        assert!(unpinned.git_commit.is_none());

        let pinned = dialectic_method_identity(
            ContextProfile::Reflective,
            "0.0.0-test",
            Some("0".repeat(40)),
        );
        let mut env = turn_envelope(pinned);
        assert_eq!(validate_provenance(&env), []);

        env.method.as_mut().unwrap().git_commit = Some("0123456".into());
        assert!(
            !validate_provenance(&env).is_empty(),
            "an abbreviated commit was accepted as a pinned build"
        );
    }

    /// The digest must actually depend on the tuning. A stubbed or constant
    /// digest satisfies "is 64 hex characters" perfectly well — this is what
    /// separates a real parameter seal from a decorative one.
    #[test]
    fn the_parameters_digest_distinguishes_the_profiles() {
        let digests: std::collections::BTreeSet<String> = ContextProfile::ALL
            .iter()
            .map(|p| dialectic_method_identity(*p, "0.0.0-test", None).parameters_digest)
            .collect();
        assert_eq!(
            digests.len(),
            ContextProfile::ALL.len(),
            "two profiles share a parameters digest — the tuning is not sealed"
        );
    }

    /// CQ2 and CQ5: a neutral gloss is an *annotation*, not a measurement, and
    /// therefore has no evidence-store representation at all. This is the test
    /// that would fail if some later path tried to ingest a defaulted gloss as
    /// an observation.
    #[test]
    fn a_neutral_gloss_is_a_heuristic_annotation_not_a_measurement() {
        use neuralcompose_mobile_core::provenance::{evidence_mapping, EvidenceMapping};

        let line = spoke_line(0);
        assert_eq!(line.gloss_scalar, NEUTRAL_GLOSS);
        assert_eq!(
            line.spectral_state, None,
            "no estimator ran on this platform"
        );
        assert_eq!(
            line.provenance.assertion_kind,
            AssertionKind::HeuristicAnnotation
        );
        assert_eq!(
            evidence_mapping(line.provenance.assertion_kind),
            EvidenceMapping::NeverIngestible,
            "a turn record must never be ingestible as evidence"
        );
        assert_eq!(
            line.provenance.confidence, None,
            "a heuristic annotation is not rescued by a confidence score"
        );
    }

    /// A malformed envelope is worse than an absent one, because it looks like
    /// an answer. Verification must refuse the file.
    #[test]
    fn verification_rejects_a_line_whose_provenance_is_defective() {
        let mut line = spoke_line(0);
        line.provenance.method.as_mut().unwrap().parameters_digest = "not-a-digest".into();
        let (payload, manifest) = record(&[line]);
        match verify_turn_log(&payload, &manifest) {
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::ProvenanceDefective { line_number, .. },
            } => assert_eq!(line_number, 1),
            other => panic!("a defective envelope was accepted: {other:?}"),
        }
    }
}
