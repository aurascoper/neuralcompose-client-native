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
use neuralcompose_mobile_core::channel_health::ChannelHealthThresholds;
use neuralcompose_mobile_core::electrode_check::MainsThresholds;
use neuralcompose_mobile_core::provenance::{
    present_option, validate as validate_provenance, AssertionKind, MethodIdentity,
    ProvenanceEnvelope, ResourceRef, PROVENANCE_ENVELOPE_SCHEMA,
};
use neuralcompose_mobile_core::types::CHANNEL_ORDER;
use serde::{Deserialize, Serialize};

/// v2 added the required `provenance` envelope. v3 replaces the flat
/// `channelHealth` line with [`ChannelRecord`], which separates what was
/// measured from what was interpreted, and adds
/// [`TurnLine::channel_health_absent_reason`].
///
/// Bumped rather than edited in place: `contracts/README.md`'s rule is that
/// contract changes are new versions, and [`verify_turn_log`] already refuses a
/// line whose schema it does not recognize — which is the correct treatment of
/// a v2 log by a v3 reader.
pub const TURN_LINE_SCHEMA: &str = "neuralcompose.hypnagogic.turn.v3";
pub const TURN_MANIFEST_SCHEMA: &str = "neuralcompose.hypnagogic.turnlog.v1";

/// Identifies this crate as the producing software in every turn's envelope.
pub const DIALECTIC_METHOD_ID: &str = "neuralcompose.hypnagogic.dialectic.v1";
pub const SOFTWARE_ID: &str = "neuralcompose-hypnagogic";

/// Identifies the channel-health computation in a [`ChannelRecord`] envelope.
///
/// Separate from [`DIALECTIC_METHOD_ID`] because it seals a different parameter
/// set: sample rate, buffer length, thresholds and band edges, none of which
/// the dialectic's tuning covers.
pub const EEG_HEALTH_METHOD_ID: &str = "neuralcompose.eeg.channel-health.v1";

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

/// Rewrites every non-integer JSON number into a bit-exact hex string, in
/// place, so a digest taken over the document cannot depend on how a float was
/// rendered to decimal or parsed back.
///
/// **Why a digest must not eat a raw float.** `serde_json` writes the shortest
/// round-tripping form but its *default* parser reads it back up to 1 ULP low
/// (measured 2026-08-13; `float_roundtrip` is now enabled here, but nothing
/// obliges a consumer in another crate, repo or language to do the same). Any
/// read-modify-write path would then re-serialize a drifted value and produce a
/// **different digest for the same logical record** — two stores disagreeing
/// about a record they both hold, with no corruption anywhere to point at.
///
/// Excluding floats is not open to us: [`crate::dynamics::Tuning`] is twelve
/// floats and they *are* the parameters this digest exists to seal. So the
/// floats stay and their decimal rendering leaves. `f32` widens to `f64`
/// exactly, so the bits are deterministic.
///
/// This is deliberately narrower than RFC 8785 canonical JSON: it fixes the
/// number encoding only, because that is the part `serde_json` gets wrong. Key
/// ordering is already stable here (serde emits struct fields in declaration
/// order, and `Value`'s map preserves insertion order under the default
/// features). A cross-implementation contract would want the full JCS instead —
/// `neural-memory-server`'s personal crate already vendors one.
pub(crate) fn bit_exact_numbers(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Number(n) => {
            // Integers already survive JSON exactly; leaving them legible keeps
            // a digest document a human can read.
            if n.as_i64().is_none() && n.as_u64().is_none() {
                let bits = n
                    .as_f64()
                    .expect("a JSON number is i64, u64 or f64")
                    .to_bits();
                *v = serde_json::Value::String(format!("f64:{bits:016x}"));
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(bit_exact_numbers),
        serde_json::Value::Object(map) => map.iter_mut().for_each(|(_, x)| bit_exact_numbers(x)),
        _ => {}
    }
}

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
    let mut doc = serde_json::to_value(Params {
        domain: DIALECTIC_METHOD_ID,
        profile: profile.id(),
        tuning: profile.tuning(),
    })
    .expect("Tuning is always serializable");
    bit_exact_numbers(&mut doc);
    let digest = sha256_hex(serde_json::to_vec(&doc).expect("a rewritten document serializes"));
    MethodIdentity {
        method_id: DIALECTIC_METHOD_ID.to_string(),
        software_id: SOFTWARE_ID.to_string(),
        software_version: software_version.into(),
        git_commit,
        parameters_digest: digest,
    }
}

/// Seals the parameters behind a channel-health computation.
///
/// Same recipe as [`dialectic_method_identity`] — serialize a parameter
/// document, rewrite its floats to bit-exact form, digest it — over a different
/// parameter set.
///
/// **The window length is deliberately absent from this digest.** It varies per
/// turn as the `StreamMonitor` ring fills, so it is per-observation data, not
/// configuration; it lives on [`ChannelWindow::sample_count`] instead. Folding
/// it in here would give the same configuration a different method identity on
/// every turn of the same session, which is the opposite of what a method
/// identity is for.
///
/// **The shaping is named honestly.** There is no filter in this pipeline. The
/// only shaping is mean removal plus a Hann window, both inside `band_power`,
/// so the document says so and records `filter: null` rather than naming a
/// stage that does not exist.
pub fn eeg_method_identity(
    sample_rate_hz: f64,
    keep_samples: u32,
    health: ChannelHealthThresholds,
    mains: MainsThresholds,
    software_version: impl Into<String>,
    git_commit: Option<String>,
) -> MethodIdentity {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Params {
        domain: &'static str,
        sample_rate_hz: f64,
        keep_samples: u32,
        detrend: &'static str,
        window: &'static str,
        filter: Option<&'static str>,
        gate_bands: [(f64, f64); 4],
        dead_rms: f64,
        saturated_rms: f64,
        minimum_samples: u64,
        mains_watch: f64,
        mains_high: f64,
    }
    let mut doc = serde_json::to_value(Params {
        domain: EEG_HEALTH_METHOD_ID,
        sample_rate_hz,
        keep_samples,
        detrend: "mean-removal",
        window: "hann",
        filter: None,
        gate_bands: crate::eeg::GATE_BANDS,
        dead_rms: health.dead_rms,
        saturated_rms: health.saturated_rms,
        minimum_samples: health.minimum_samples,
        mains_watch: mains.watch,
        mains_high: mains.high,
    })
    .expect("plain scalars are always serializable");
    bit_exact_numbers(&mut doc);
    let digest = sha256_hex(serde_json::to_vec(&doc).expect("a rewritten document serializes"));
    MethodIdentity {
        method_id: EEG_HEALTH_METHOD_ID.to_string(),
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

/// The envelope on a [`ChannelDerived`]: a named transform over a named window.
///
/// `confidence` is `None` because
/// [`AssertionKind::DerivedDeterministically`] does not take one — a
/// deterministic computation has no confidence, it has inputs and a method.
/// `comparison_embedding_space` is `None` because no embedding is involved.
pub(crate) fn derived_envelope(method: MethodIdentity, window: ResourceRef) -> ProvenanceEnvelope {
    ProvenanceEnvelope {
        schema_id: PROVENANCE_ENVELOPE_SCHEMA.to_string(),
        assertion_kind: AssertionKind::DerivedDeterministically,
        method: Some(method),
        inputs: vec![window],
        confidence: None,
        comparison_embedding_space: None,
    }
}

/// The envelope on a [`ChannelAnnotation`].
///
/// `confidence` is `None` even though
/// [`AssertionKind::HeuristicAnnotation`] permits one. There is no calibrated
/// basis for a number here — the thresholds come from one subject in one mains
/// environment — and a confidence attached to an uncalibrated threshold is the
/// laundering this whole vocabulary exists to prevent.
///
/// The same window is named as the input, so a reader can see that the
/// classification and the measurement are about the same samples.
pub(crate) fn annotation_envelope(
    method: MethodIdentity,
    window: ResourceRef,
) -> ProvenanceEnvelope {
    ProvenanceEnvelope {
        schema_id: PROVENANCE_ENVELOPE_SCHEMA.to_string(),
        assertion_kind: AssertionKind::HeuristicAnnotation,
        method: Some(method),
        inputs: vec![window],
        confidence: None,
        comparison_embedding_space: None,
    }
}

/// Names the window a channel's numbers were computed from.
///
/// The digest is over the **window**, not the whole capture file: a capture's
/// `payloadSha256Hex` only exists at `finish()`, at end of session, and these
/// envelopes are written during it. Digesting the samples actually consumed is
/// both computable at turn time and the stronger claim — it makes the
/// derivation reproducible rather than merely attributed.
pub fn window_resource_ref(samples: &[f64], recording_id: Option<&str>) -> ResourceRef {
    // Bit-exact bytes, so the digest cannot depend on decimal rendering — the
    // same reason `bit_exact_numbers` exists for the method documents.
    let mut bytes = Vec::with_capacity(samples.len() * 8);
    for s in samples {
        bytes.extend_from_slice(&s.to_bits().to_be_bytes());
    }
    ResourceRef {
        resource_kind: "eeg-window".to_string(),
        sha256_hex: sha256_hex(bytes),
        // `None` when the run is not recording a capture: the window was
        // consumed and digested, but no file holds it, and pointing at a file
        // that does not exist would be worse than admitting there is none.
        locator: recording_id.map(|id| format!("{id}.eeg.jsonl")),
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

/// Which samples a channel's numbers were computed from.
///
/// Enough for a reader to find the window inside the session's `.eeg.jsonl` and
/// recompute the derivation. Not itself an assertion — it is the coordinates of
/// one, which is why it carries no envelope of its own.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelWindow {
    /// How many samples were in the buffer. Varies per turn while the ring
    /// fills, which is exactly why it is recorded rather than digested into the
    /// method identity.
    pub sample_count: u64,
    /// Seconds since stream start for the newest sample in the window — the
    /// wire axis, never wall clock. With `sample_count` this locates the window
    /// in the capture.
    ///
    /// `None` when the source supplied none. Not `0.0`: that is a real
    /// timestamp meaning the first sample of the stream, and the two must stay
    /// distinguishable.
    #[serde(deserialize_with = "present_option")]
    pub last_source_timestamp: Option<f64>,
}

/// What was **measured**: numbers a named transform produced from the samples.
///
/// [`AssertionKind::DerivedDeterministically`], not `Observed`. The only
/// observed thing in this pipeline is the raw frame, which lives in the
/// session's `.eeg.jsonl`; an RMS is a computation over it, and the envelope's
/// single input names the window it consumed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDerived {
    pub rms_microvolts: f64,
    /// Power in the winning mains band. `None` when no band was measurable —
    /// never `0.0`, which is a reading meaning "no line noise here".
    #[serde(deserialize_with = "present_option")]
    pub mains_power: Option<f64>,
    pub provenance: ProvenanceEnvelope,
}

/// What was **interpreted**: a classification against thresholds nobody has
/// validated physiologically.
///
/// [`AssertionKind::HeuristicAnnotation`], and therefore `NeverIngestible`.
///
/// This is the naming rule applied to itself. `status` reads `healthy`,
/// `saturated`, `dead` — state words, claims about an electrode rather than
/// about a number — and `channel_health.rs:24-32` says outright that the
/// thresholds behind them are not physiologically validated. A field whose name
/// implies a state cannot be typed as a measurement, so it is not.
///
/// `mains_power` deliberately lives on [`ChannelDerived`] instead: it is a
/// power figure, and bundling it here would put a measurement and an
/// interpretation under one envelope, collapsing two epistemic classes into the
/// looser of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAnnotation {
    pub status: ChannelHealthStatus,
    /// The electrode verdict's stable id — `ok`, `mains-pickup`, `lifted` and
    /// so on. The **enum**, never `ElectrodeVerdict::advice()`.
    ///
    /// Eligibility has to be able to query why a session was excluded, and
    /// mains pickup is an exclusion reason, so the classification must be on
    /// the record. The original decision that kept this out of the log
    /// (`eeg.rs`) was about the advice *sentence* — a turn record should not
    /// carry a line telling a future reader to reseat an electrode that was
    /// reseated months ago — and that reason still holds. The sentence stays
    /// out; the classification comes in.
    pub verdict: String,
    /// Which line frequency carried more power. `None` means *cannot tell* at
    /// this sample rate, never *clean*.
    #[serde(deserialize_with = "present_option")]
    pub mains_line_hz: Option<f64>,
    pub provenance: ProvenanceEnvelope,
}

/// Per-channel signal health at the time of the turn. Absent entirely when no
/// EEG source is attached or the window was refused; when present it is always
/// all four channels in the frozen `TP9, AF7, AF8, TP10` order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRecord {
    pub channel: String,
    pub window: ChannelWindow,
    pub derived: ChannelDerived,
    pub annotation: ChannelAnnotation,
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
    #[serde(deserialize_with = "present_option")]
    pub channel_health: Option<Vec<ChannelRecord>>,
    /// Why `channel_health` is absent, when it is.
    ///
    /// The pair is the point. An absent reading and a dead electrode both
    /// produce `channelHealth: null`, and until this field existed nothing
    /// downstream could tell them apart — so session eligibility had to be a
    /// human verdict reconstructed after the fact instead of a query. Exactly
    /// one of these two fields is `Some` on any line where an EEG was attached;
    /// both are `null` when there was no EEG at all.
    #[serde(deserialize_with = "present_option")]
    pub channel_health_absent_reason: Option<String>,
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
            channel_health_absent_reason: None,
            provenance: turn_envelope(method),
        }
    }

    /// A minimal silent line, for tests in other modules that need a `TurnLine`
    /// to hang channel records off and do not care about the competition.
    ///
    /// `#[cfg(test)]` so it cannot be reached from a real run: a turn line with
    /// no candidates and a placeholder method identity is not a record of
    /// anything, and nothing outside a test should be able to write one.
    #[cfg(test)]
    pub(crate) fn silent_for_test() -> Self {
        Self {
            schema_id: TURN_LINE_SCHEMA.to_string(),
            index: 0,
            mode: "reflective".to_string(),
            heard: String::new(),
            candidates: Vec::new(),
            tension: 0.0,
            margin: 0.0,
            selection_temperature: 1.0,
            gloss_scalar: NEUTRAL_GLOSS,
            spectral_state: None,
            outcome: "silent".to_string(),
            spoken_text: None,
            witness_finding: None,
            witness_distance: None,
            self_similarity: None,
            witness_attempted: None,
            channel_health: None,
            channel_health_absent_reason: None,
            provenance: turn_envelope(MethodIdentity {
                method_id: DIALECTIC_METHOD_ID.to_string(),
                software_id: SOFTWARE_ID.to_string(),
                software_version: "0".to_string(),
                git_commit: None,
                parameters_digest: "0".repeat(64),
            }),
        }
    }

    /// Records a reading. Clears any absent-reason: the two are mutually
    /// exclusive by construction, not by the caller remembering.
    pub fn with_channel_health(mut self, health: Vec<ChannelRecord>) -> Self {
        self.channel_health = Some(health);
        self.channel_health_absent_reason = None;
        self
    }

    /// Records why there was no reading. Clears any health for the same reason.
    pub fn with_channel_health_absent(mut self, reason: &str) -> Self {
        self.channel_health = None;
        self.channel_health_absent_reason = Some(reason.to_string());
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
    /// A channel record's envelope claims a different assertion kind than its
    /// tier. The severity is not cosmetic: `derived` claiming `observed` would
    /// route an uncalibrated computation through
    /// [`neuralcompose_mobile_core::provenance::EvidenceMapping::Ingestible`]
    /// and into a memory store as though it were a measurement.
    ChannelAssertionKindWrong {
        line_number: u64,
        channel: String,
        found: String,
    },
    /// Both a reading and a reason for its absence. The writer contradicted
    /// itself and a reader cannot tell which half to believe.
    ChannelHealthContradiction {
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
        if let Some(defect) = validate_provenance(&line.provenance).first() {
            return fail(TurnLogFailure::ProvenanceDefective {
                line_number,
                defect: format!("{defect:?}"),
            });
        }
        // A reading and a reason for its absence are mutually exclusive. Both
        // present means the writer contradicted itself, and a reader would have
        // to guess which half to believe.
        if line.channel_health.is_some() && line.channel_health_absent_reason.is_some() {
            return fail(TurnLogFailure::ChannelHealthContradiction { line_number });
        }
        if let Some(health) = &line.channel_health {
            let order: Vec<&str> = health.iter().map(|h| h.channel.as_str()).collect();
            if order != CHANNEL_ORDER {
                return fail(TurnLogFailure::ChannelOrderMismatch { line_number });
            }
            // The tiers are enforced here, not left to whoever writes the
            // record. A `derived` envelope that says `observed` would make an
            // uncalibrated computation ingestible as an observation, which is
            // the single failure this vocabulary exists to prevent — and it
            // would be invisible in the JSON to anyone not checking.
            for record in health {
                for (envelope, expected) in [
                    (
                        &record.derived.provenance,
                        AssertionKind::DerivedDeterministically,
                    ),
                    (
                        &record.annotation.provenance,
                        AssertionKind::HeuristicAnnotation,
                    ),
                ] {
                    if envelope.assertion_kind != expected {
                        return fail(TurnLogFailure::ChannelAssertionKindWrong {
                            line_number,
                            channel: record.channel.clone(),
                            found: format!("{:?}", envelope.assertion_kind),
                        });
                    }
                    if let Some(defect) = validate_provenance(envelope).first() {
                        return fail(TurnLogFailure::ProvenanceDefective {
                            line_number,
                            defect: format!("{}: {defect:?}", record.channel),
                        });
                    }
                }
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

    /// A well-formed channel record with both envelopes correctly typed.
    /// Correctly typed on purpose: tests that assert the *wrong* kind is caught
    /// build their own, so this helper cannot be the reason they pass.
    fn channel_record(channel: &str, rms: f64) -> ChannelRecord {
        let window = ResourceRef {
            resource_kind: "eeg-window".into(),
            sha256_hex: "a".repeat(64),
            locator: Some("session-test.eeg.jsonl".into()),
        };
        ChannelRecord {
            channel: channel.to_string(),
            window: ChannelWindow {
                sample_count: 1280,
                last_source_timestamp: Some(5.0),
            },
            derived: ChannelDerived {
                rms_microvolts: rms,
                mains_power: Some(8.11),
                provenance: derived_envelope(test_method(), window.clone()),
            },
            annotation: ChannelAnnotation {
                status: ChannelHealthStatus::Healthy,
                verdict: "ok".into(),
                mains_line_hz: Some(60.0),
                provenance: annotation_envelope(test_method(), window),
            },
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

    /// The single failure this whole vocabulary exists to prevent, asserted at
    /// the verifier rather than trusted at the writer.
    ///
    /// An `rms` envelope that says `observed` maps to `Ingestible`, so an
    /// uncalibrated computation over one unvalidated channel could be ingested
    /// as a measurement — and nothing in the JSON would look wrong to anyone
    /// who was not specifically checking.
    #[test]
    fn a_derived_envelope_claiming_to_be_an_observation_is_refused() {
        let mut records: Vec<ChannelRecord> = CHANNEL_ORDER
            .iter()
            .map(|c| channel_record(c, 12.0))
            .collect();
        records[2].derived.provenance.assertion_kind = AssertionKind::Observed;

        let (payload, manifest) = record(&[spoke_line(0).with_channel_health(records)]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::ChannelAssertionKindWrong {
                    line_number: 1,
                    channel: "AF8".to_string(),
                    found: "Observed".to_string(),
                }
            }
        );
    }

    /// The other direction: a status promoted out of `heuristicAnnotation`
    /// would stop being `NeverIngestible`, which is the only thing standing
    /// between a spectral gloss and a training label.
    #[test]
    fn an_annotation_promoted_out_of_the_heuristic_class_is_refused() {
        let mut records: Vec<ChannelRecord> = CHANNEL_ORDER
            .iter()
            .map(|c| channel_record(c, 12.0))
            .collect();
        records[0].annotation.provenance.assertion_kind = AssertionKind::DerivedDeterministically;

        let (payload, manifest) = record(&[spoke_line(0).with_channel_health(records)]);
        assert!(matches!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::ChannelAssertionKindWrong { .. }
            }
        ));
    }

    /// A reading and a reason for its absence are contradictory, and the
    /// builders make them mutually exclusive — but a hand-written or
    /// hand-edited line can still carry both, so the verifier says so.
    #[test]
    fn a_reading_and_a_reason_for_its_absence_cannot_both_be_present() {
        let mut line = spoke_line(0).with_channel_health(
            CHANNEL_ORDER
                .iter()
                .map(|c| channel_record(c, 12.0))
                .collect(),
        );
        line.channel_health_absent_reason = Some("stream-not-live".to_string());

        let (payload, manifest) = record(&[line]);
        assert_eq!(
            verify_turn_log(&payload, &manifest),
            TurnLogVerdict::Failed {
                failure: TurnLogFailure::ChannelHealthContradiction { line_number: 1 }
            }
        );
    }

    /// The builders are what normally hold that invariant, so they are pinned
    /// too — setting one must clear the other.
    #[test]
    fn the_builders_keep_a_reading_and_an_absence_mutually_exclusive() {
        let health: Vec<ChannelRecord> = CHANNEL_ORDER
            .iter()
            .map(|c| channel_record(c, 12.0))
            .collect();

        let a = spoke_line(0)
            .with_channel_health_absent("stream-not-live")
            .with_channel_health(health.clone());
        assert!(a.channel_health.is_some());
        assert_eq!(a.channel_health_absent_reason, None);

        let b = spoke_line(0)
            .with_channel_health(health)
            .with_channel_health_absent("band-exactly-zero");
        assert_eq!(b.channel_health, None);
        assert_eq!(
            b.channel_health_absent_reason.as_deref(),
            Some("band-exactly-zero")
        );
    }

    /// An absent reading is written as an explicit `null`, never omitted. A
    /// missing key would be indistinguishable from a producer that predates the
    /// field, which is exactly the confusion `present_option` exists to stop.
    #[test]
    fn an_absent_reading_is_a_null_not_a_missing_key() {
        let line = spoke_line(0);
        let encoded = encode_turn_line(&line);
        assert!(encoded.contains("\"channelHealth\":null"), "{encoded}");
        assert!(
            encoded.contains("\"channelHealthAbsentReason\":null"),
            "{encoded}"
        );

        // And a line that simply omits them does not parse.
        let stripped = encoded
            .replace(",\"channelHealth\":null", "")
            .replace(",\"channelHealthAbsentReason\":null", "");
        assert!(
            serde_json::from_str::<TurnLine>(&stripped).is_err(),
            "a line missing both EEG fields parsed as though absent"
        );
    }

    #[test]
    fn channel_health_must_be_all_four_in_the_frozen_order() {
        let health = |channels: &[&str]| -> Vec<ChannelRecord> {
            channels.iter().map(|c| channel_record(c, 12.0)).collect()
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

    /// The read-modify-write property, which is what the byte-level payload
    /// digest does NOT give you. `verify_turn_log` re-hashes the original bytes,
    /// so it stays green even if parsing drifts every float in the file. Any
    /// consumer that parses a record and writes it back — a mirror worker, a
    /// coordinator, a promotion path — would then produce a different digest for
    /// the same logical record, and nothing would look broken.
    ///
    /// This asserts the cycle is byte-identical. It fails if `float_roundtrip`
    /// is ever dropped from `Cargo.toml`.
    #[test]
    fn a_turn_line_survives_a_parse_and_reserialize_cycle_byte_for_byte() {
        let line = spoke_line(0)
            .with_self_similarity(0.9243132)
            .with_channel_health(vec![channel_record("TP9", 14.067217896390133)]);
        let once = encode_turn_line(&line);
        let parsed: TurnLine = serde_json::from_str(&once).expect("parses");
        let twice = encode_turn_line(&parsed);
        assert_eq!(once, twice, "a parse cycle changed the bytes");
        assert_eq!(
            sha256_hex(once.as_bytes().to_vec()),
            sha256_hex(twice.as_bytes().to_vec())
        );
        // The specific value that exposed the default parser.
        assert!(once.contains("14.067217896390133"), "{once}");
    }

    /// `confidence` rides inside the record the payload digest covers, so it is
    /// subject to the same drift as any other float. Pinned explicitly because
    /// it is the field most likely to be added to a future identity document.
    #[test]
    fn a_confidence_value_survives_the_same_cycle() {
        let mut line = spoke_line(0);
        line.provenance.assertion_kind = AssertionKind::AgentInference;
        line.provenance.confidence = Some(0.8414709848078965);
        let once = encode_turn_line(&line);
        let parsed: TurnLine = serde_json::from_str(&once).unwrap();
        assert_eq!(parsed.provenance.confidence, line.provenance.confidence);
        assert_eq!(once, encode_turn_line(&parsed));
    }

    /// The parameters digest must not depend on decimal float rendering at all:
    /// the digest DOCUMENT must survive a text round trip, which is exactly what
    /// a read-modify-write consumer does to it.
    ///
    /// Deliberately NOT "`Tuning` serialized directly equals `Tuning` laundered
    /// through text". `Tuning` is `f32`; `to_value` widens it exactly, while
    /// `to_string` writes the shortest **f32** form and reading that back as
    /// `f64` is a genuinely different number. Those are not the same quantity
    /// and never were. The first version of this test compared them and failed,
    /// correctly — the test was wrong, not the code.
    #[test]
    fn the_parameters_digest_does_not_depend_on_float_rendering() {
        for profile in ContextProfile::ALL {
            let doc = serde_json::to_value(profile.tuning()).unwrap();

            let mut direct = doc.clone();
            bit_exact_numbers(&mut direct);
            let before = sha256_hex(serde_json::to_vec(&direct).unwrap());

            // The read-modify-write cycle. Without `float_roundtrip` this drifts.
            let text = serde_json::to_string(&doc).unwrap();
            let mut round_tripped: serde_json::Value = serde_json::from_str(&text).unwrap();
            bit_exact_numbers(&mut round_tripped);
            let after = sha256_hex(serde_json::to_vec(&round_tripped).unwrap());

            assert_eq!(
                before, after,
                "{profile:?} digest moved across a JSON cycle"
            );
        }
    }

    /// The two properties `parameters_digest` exists for: stable run to run,
    /// and distinct per profile.
    #[test]
    fn the_parameters_digest_is_stable_and_profile_specific() {
        let of = |p| dialectic_method_identity(p, "0.0.0-test", None).parameters_digest;
        for profile in ContextProfile::ALL {
            assert_eq!(of(profile), of(profile), "{profile:?} digest is not stable");
        }
        let all: std::collections::BTreeSet<String> =
            ContextProfile::ALL.iter().map(|p| of(*p)).collect();
        assert_eq!(all.len(), ContextProfile::ALL.len());
    }

    /// The rewrite must actually remove the decimals, and must leave integers
    /// alone — otherwise it is doing nothing and the test above passes for the
    /// wrong reason.
    #[test]
    fn the_rewrite_replaces_floats_and_keeps_integers_legible() {
        let mut doc = serde_json::json!({
            "float": 0.1_f64,
            "int": 7,
            "nested": [1.5_f64, 2, {"deep": 0.25_f64}],
            "text": "0.5"
        });
        bit_exact_numbers(&mut doc);
        assert_eq!(doc["float"], serde_json::json!("f64:3fb999999999999a"));
        assert_eq!(doc["int"], serde_json::json!(7), "an integer was rewritten");
        assert_eq!(doc["nested"][0], serde_json::json!("f64:3ff8000000000000"));
        assert_eq!(doc["nested"][1], serde_json::json!(2));
        assert_eq!(
            doc["nested"][2]["deep"],
            serde_json::json!("f64:3fd0000000000000")
        );
        assert_eq!(
            doc["text"],
            serde_json::json!("0.5"),
            "a string was touched"
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
