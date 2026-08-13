//! How a value came to exist — the wire vocabulary shared across the four
//! NeuralCompose repositories (ADR-004). Deterministic, effect-free.
//!
//! Every repository here already answers "how was this produced" somewhere, and
//! each answers it differently: `capture.rs` stamps a build identity onto a
//! recording, `conformance.rs` insists an unmeasured tolerance is not a
//! satisfied one, `property_law.rs` refuses to share an index across embedding
//! spaces — and `neuralcompose-hypnagogic`'s turn log answers it not at all.
//! This module is the one shape all of them can carry.
//!
//! **It is vocabulary, not a reasoner.** Nothing here derives a new claim from
//! existing ones. It records what a producer asserts and under which method, so
//! that a later reader can tell a measurement from a guess. That distinction is
//! the entire product.
//!
//! Two rules earn their own types below, because both have already been got
//! wrong in this codebase and its siblings:
//!
//! - **Absence is not a value.** `docs/reviews/absent-as-legitimate-value-2026-08-12.md`
//!   records `cosineSimilarity` returning `0` for "incomparable" — a legitimate
//!   similarity standing in for "no answer". [`comparable`] exists so that two
//!   *unknown* embedding spaces never read as one matching space.
//! - **A decision is not an oversight.** [`EvidenceMapping`] distinguishes "this
//!   kind deliberately has no evidence-store representation" from "nobody has
//!   decided yet", which an `Option` would render identically.

use serde::{Deserialize, Serialize};

pub const PROVENANCE_ENVELOPE_SCHEMA: &str = "neuralcompose.provenance-envelope.v1";

/// Forces an `Option` field to be *present*, even when null.
///
/// serde's derive treats a missing `Option<T>` as `None` unless the field names
/// a custom deserializer — so without this, `{"method": null}` and an envelope
/// with no `method` key at all parse identically. They are different facts: one
/// producer recorded that there is no method, the other recorded nothing. This
/// module exists to keep exactly that pair apart, so it cannot afford the
/// default in its own wire format.
///
/// Found by `every_required_field_is_rejected_when_missing`, which is the only
/// reason it is not still here.
fn present_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}

/// How the value this envelope describes came to be asserted.
///
/// Five of the six are spelled exactly as `neural-memory-server`'s
/// `EvidenceClass` serializes them (`crates/neural-memory-domain/src/terms.rs:49-63`
/// at commit `5da4a5c`); [`evidence_mapping`] is where that correspondence is
/// written down and `scripts/check-evidence-class-drift.sh` is what re-checks it.
///
/// The sixth, [`AssertionKind::HeuristicAnnotation`], has **no** counterpart
/// there, on purpose. A threshold that a source file itself calls unvalidated —
/// `channel_health.rs:24`, `SpectralState.honestyCaveat` — is not evidence at
/// any confidence, and giving it an evidence class would be the laundering this
/// vocabulary exists to prevent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssertionKind {
    Observed,
    DerivedDeterministically,
    HumanDecision,
    AgentInference,
    ExternalClaim,
    HeuristicAnnotation,
}

impl AssertionKind {
    /// Every kind, so a caller can exhaust the space without matching by hand.
    pub const ALL: [AssertionKind; 6] = [
        AssertionKind::Observed,
        AssertionKind::DerivedDeterministically,
        AssertionKind::HumanDecision,
        AssertionKind::AgentInference,
        AssertionKind::ExternalClaim,
        AssertionKind::HeuristicAnnotation,
    ];
}

/// What a kind becomes if it reaches the evidence store.
///
/// Deliberately not `Option<_>`: `None` would give a permanent decision the same
/// shape as an undecided one, and every call site would have to guess which it
/// was looking at. There is also no `NotYetDecided` variant — it would have no
/// member today, and its existence is what would let a future kind sit
/// undecided indefinitely. Adding a variant to [`AssertionKind`] must force the
/// decision at that moment, and an inexhaustive match is how it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceMapping {
    /// Spelled as `neural-memory-domain::EvidenceClass` serializes it.
    Ingestible(&'static str),
    /// No evidence-store representation, by decision rather than by omission.
    NeverIngestible,
}

/// The correspondence to `neural-memory-server`'s evidence classes.
///
/// **This is vocabulary alignment, not a pipe.** Four of the five ingestible
/// mappings have no live ingestion path today: `crates/neural-memory-store/src/write.rs:266-269`
/// clamps every `WriteChannel::Agent` write to `agentInference` *before* the
/// digest is computed, and the MCP surface exposes no other door. Reading this
/// function as a capability would be reading it wrong.
pub fn evidence_mapping(kind: AssertionKind) -> EvidenceMapping {
    use AssertionKind::*;
    match kind {
        Observed => EvidenceMapping::Ingestible("observed"),
        DerivedDeterministically => EvidenceMapping::Ingestible("derivedDeterministically"),
        HumanDecision => EvidenceMapping::Ingestible("humanDecision"),
        AgentInference => EvidenceMapping::Ingestible("agentInference"),
        ExternalClaim => EvidenceMapping::Ingestible("externalClaim"),
        HeuristicAnnotation => EvidenceMapping::NeverIngestible,
    }
}

/// Something a method consumed or produced, named by digest.
///
/// `resource_kind` is a free label rather than an enum for the same reason
/// ADR-002 decision 1 keeps `backend_id` configuration: a new kind of resource
/// must not require a core release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceRef {
    pub resource_kind: String,
    pub sha256_hex: String,
    /// Where inside the resource. `None` means the whole of it — a fact about
    /// the reference, not a missing field.
    #[serde(deserialize_with = "present_option")]
    pub locator: Option<String>,
}

/// The producing software and the frozen parameters it ran under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MethodIdentity {
    /// Dotted and versioned, the convention `CAPTURE_LINE_SCHEMA` and
    /// `TURN_LINE_SCHEMA` already use.
    pub method_id: String,
    pub software_id: String,
    pub software_version: String,
    /// `None` means this build did not come from a git checkout. Never treated
    /// as a pinned build — the same discipline as an unsigned pack.
    #[serde(deserialize_with = "present_option")]
    pub git_commit: Option<String>,
    /// sha256 over the canonical parameter document.
    ///
    /// Not `Option`: an empty parameter set still has a digest, so an optional
    /// field here could only ever encode "nobody recorded it", which is the
    /// hole this whole module exists to close.
    pub parameters_digest: String,
}

/// How one recorded value came to be asserted.
///
/// Embedded in the record it describes — there is no `subject` field, because
/// the host record *is* the subject, exactly as `schema_id` is already the host
/// record's own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvenanceEnvelope {
    pub schema_id: String,
    pub assertion_kind: AssertionKind,
    #[serde(deserialize_with = "present_option")]
    pub method: Option<MethodIdentity>,
    /// What the method consumed, in the order it consumed them.
    pub inputs: Vec<ResourceRef>,
    /// `None` means unscored. Never promotes a kind; see [`validate`].
    #[serde(deserialize_with = "present_option")]
    pub confidence: Option<f64>,
    /// Names the single embedding space every compared vector came from.
    /// `None` means this assertion rests on no embedding comparison at all.
    #[serde(deserialize_with = "present_option")]
    pub comparison_embedding_space: Option<String>,
}

impl ProvenanceEnvelope {
    /// An envelope for a kind that rests on no inputs and no embedding
    /// comparison — the common case for a heuristic or an agent inference.
    pub fn new(assertion_kind: AssertionKind, method: MethodIdentity) -> Self {
        ProvenanceEnvelope {
            schema_id: PROVENANCE_ENVELOPE_SCHEMA.to_string(),
            assertion_kind,
            method: Some(method),
            inputs: Vec::new(),
            confidence: None,
            comparison_embedding_space: None,
        }
    }
}

/// May these two assertions' scores be compared?
///
/// Only when both name the **same** embedding space. Two absences are not a
/// match: `a == b` would return `true` for `(None, None)` and quietly licence
/// exactly the comparison this function exists to refuse.
///
/// That failure is not hypothetical. `claude-mind-mcp`'s Core Data recall path
/// (`Sources/ClaudeMindCore/MemoryStore.swift:1178-1183`) gates cosine on
/// `q.count == stored.count` and never reads the row's `embeddingProfile`, so
/// two different 384-dimension encoders are compared as though they shared a
/// space — while its Postgres path checks the profile strictly. Dimension is
/// not identity.
///
/// Compare [`crate::property_law::shares_index`], which needs no such guard
/// because its embedding-space field cannot be absent.
pub fn comparable(a: &ProvenanceEnvelope, b: &ProvenanceEnvelope) -> bool {
    match (&a.comparison_embedding_space, &b.comparison_embedding_space) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Why an envelope is not well formed. A typed enum rather than strings so the
/// contract test can assert *which* rule fired — substring-matching a message
/// is a test that passes for the wrong reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvenanceDefect {
    /// The envelope declares a schema this build does not implement.
    SchemaIdMismatch,
    /// A kind that must name its method did not.
    MethodRequired,
    /// A deterministic derivation that named no inputs cannot be recomputed,
    /// and an unrecomputable derivation is just an assertion.
    DerivationInputsRequired,
    /// A measured, recomputed or decided claim carries no confidence. Scoring
    /// one is the first step toward a score promoting it.
    ConfidenceNotApplicable,
    ConfidenceOutOfRange,
    EmptyField(String),
    MalformedDigest(String),
}

/// Kinds whose confidence is meaningful. An observation is measured, a
/// derivation is recomputed, and a human decision is a decision — none of the
/// three is scored.
fn takes_confidence(kind: AssertionKind) -> bool {
    matches!(
        kind,
        AssertionKind::AgentInference
            | AssertionKind::ExternalClaim
            | AssertionKind::HeuristicAnnotation
    )
}

/// Kinds that must name the software and parameters that produced them.
fn requires_method(kind: AssertionKind) -> bool {
    matches!(
        kind,
        AssertionKind::DerivedDeterministically
            | AssertionKind::AgentInference
            | AssertionKind::HeuristicAnnotation
    )
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Every way this envelope is malformed. Empty means well formed.
///
/// Confidence is read only to range-check it and to reject it where it does not
/// belong — never to choose or adjust a kind. That is the invariant behind
/// "confidence never promotes an inference into an observation".
pub fn validate(env: &ProvenanceEnvelope) -> Vec<ProvenanceDefect> {
    let mut out = Vec::new();

    if env.schema_id != PROVENANCE_ENVELOPE_SCHEMA {
        out.push(ProvenanceDefect::SchemaIdMismatch);
    }

    match &env.method {
        None if requires_method(env.assertion_kind) => out.push(ProvenanceDefect::MethodRequired),
        None => {}
        Some(m) => {
            for (name, value) in [
                ("methodId", &m.method_id),
                ("softwareId", &m.software_id),
                ("softwareVersion", &m.software_version),
            ] {
                if value.trim().is_empty() {
                    out.push(ProvenanceDefect::EmptyField(name.to_string()));
                }
            }
            if !is_lower_hex(&m.parameters_digest, 64) {
                out.push(ProvenanceDefect::MalformedDigest("parametersDigest".into()));
            }
            // 40 hex, the full object name — an abbreviated commit is ambiguous
            // and this field exists so a run can be reproduced exactly.
            if m.git_commit.as_ref().is_some_and(|c| !is_lower_hex(c, 40)) {
                out.push(ProvenanceDefect::MalformedDigest("gitCommit".into()));
            }
        }
    }

    if env.assertion_kind == AssertionKind::DerivedDeterministically && env.inputs.is_empty() {
        out.push(ProvenanceDefect::DerivationInputsRequired);
    }

    for input in &env.inputs {
        if input.resource_kind.trim().is_empty() {
            out.push(ProvenanceDefect::EmptyField("resourceKind".into()));
        }
        if !is_lower_hex(&input.sha256_hex, 64) {
            out.push(ProvenanceDefect::MalformedDigest("sha256Hex".into()));
        }
    }

    if let Some(c) = env.confidence {
        if !takes_confidence(env.assertion_kind) {
            out.push(ProvenanceDefect::ConfidenceNotApplicable);
        } else if !(0.0..=1.0).contains(&c) {
            // NaN and both infinities fall here too: `contains` is false for
            // every non-finite value, which is the answer we want.
            out.push(ProvenanceDefect::ConfidenceOutOfRange);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method() -> MethodIdentity {
        MethodIdentity {
            method_id: "neuralcompose.test.v1".into(),
            software_id: "neuralcompose-mobile-core".into(),
            software_version: "0.1.0".into(),
            git_commit: None,
            parameters_digest: "a".repeat(64),
        }
    }

    fn envelope(kind: AssertionKind) -> ProvenanceEnvelope {
        ProvenanceEnvelope::new(kind, method())
    }

    #[test]
    fn a_well_formed_heuristic_envelope_has_no_defects() {
        assert_eq!(validate(&envelope(AssertionKind::HeuristicAnnotation)), []);
    }

    /// NaN is the case a `<`/`>` pair would let through: every comparison
    /// against it is false, so a hand-rolled range check reads it as in range.
    #[test]
    fn non_finite_confidence_is_out_of_range() {
        for c in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut env = envelope(AssertionKind::AgentInference);
            env.confidence = Some(c);
            assert_eq!(
                validate(&env),
                [ProvenanceDefect::ConfidenceOutOfRange],
                "confidence {c} was accepted"
            );
        }
    }

    /// Both ends of the range are inclusive — 0.0 and 1.0 are real scores.
    #[test]
    fn the_confidence_bounds_are_inclusive() {
        for c in [0.0, 1.0] {
            let mut env = envelope(AssertionKind::AgentInference);
            env.confidence = Some(c);
            assert_eq!(validate(&env), [], "confidence {c} was rejected");
        }
    }

    #[test]
    fn an_abbreviated_commit_is_malformed() {
        let mut env = envelope(AssertionKind::HeuristicAnnotation);
        env.method.as_mut().unwrap().git_commit = Some("deadbee".into());
        assert_eq!(
            validate(&env),
            [ProvenanceDefect::MalformedDigest("gitCommit".into())]
        );
    }
}
