//! Backend semantic conformance (M7-A2, ADR-002). Deterministic, effect-free.
//!
//! Running one model through CUDA, Vulkan, OpenVINO, QNN, Metal, or CPU is
//! **not** mathematical equivalence: floating-point kernels, quantization,
//! and sampling differ. What must hold is *semantic conformance* — identical
//! model/tokenizer/template identity, identical prompt bytes and token ids,
//! identical stop configuration, declared shapes, and numerical agreement
//! only within a **pre-registered** tolerance.
//!
//! Two backends that agree within tolerance may share a numerical-contract
//! identity. Two that do not are not "broken" — they are a *different
//! contract*, and must be published as such rather than declared equal.

use serde::{Deserialize, Serialize};

const NUMERICAL_CONTRACT_DOMAIN: &str = "neuralcompose.numerical-contract.v1";
const PROMPT_DOMAIN: &str = "neuralcompose.prompt-bytes.v1";

/// How sampled text is judged. Byte equality is only ever legitimate under
/// deterministic greedy decoding on a backend that supports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum GeneratedTokenPolicy {
    /// Greedy decode, deterministic on this backend: exact token ids required.
    ExactUnderGreedy,
    /// Sampling permitted: evaluate as distributions, never string equality.
    DistributionOnly,
}

/// The frozen agreement a backend must satisfy to claim a numerical contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BackendConformancePolicy {
    pub numerical_contract_id: String,
    pub tokenizer_identity: String,
    pub prompt_byte_identity: String,
    pub stop_token_identity: String,
    pub context_cap: u32,
    pub deterministic_test_mode: bool,
    /// Maximum absolute logit divergence. Must be finite and >= 0.
    pub logits_tolerance: f64,
    /// Maximum embedding divergence under the declared metric.
    pub embedding_tolerance: f64,
    pub generated_token_policy: GeneratedTokenPolicy,
}

/// What a backend actually did, as reported by the shell running it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BackendObservation {
    pub backend_id: String,
    pub tokenizer_identity: String,
    pub prompt_byte_identity: String,
    pub stop_token_identity: String,
    pub context_cap: u32,
    pub output_shape_declared: bool,
    /// `None` when the backend does not expose logits — absence is not
    /// agreement, and cannot satisfy a logits tolerance.
    pub max_logit_divergence: Option<f64>,
    pub max_embedding_divergence: Option<f64>,
    pub greedy_determinism_available: bool,
    pub generated_tokens_match_reference: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ConformanceFailure {
    TokenizerIdentityMismatch,
    PromptByteMismatch,
    StopTokenMismatch,
    ContextCapMismatch,
    OutputShapeUndeclared,
    GreedyDeterminismUnavailable,
    GeneratedTokensDiverged,
    MissingMeasurement { measurement: String },
    MalformedPolicy { reason: String },
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ConformanceVerdict {
    /// Every identity matches and every measurement is inside tolerance:
    /// this backend may carry the declared numerical-contract id.
    Conformant,
    /// Identities match but measurements exceed tolerance. NOT a failure to
    /// hide — the backend must publish under its own contract identity.
    RequiresSeparateNumericalContract {
        measurement: String,
        observed: f64,
        tolerance: f64,
    },
    /// A semantic identity mismatch. No tolerance can excuse this.
    NonConformant { failure: ConformanceFailure },
}

/// Canonical prompt identity: exact bytes, hashed with a domain string.
/// Providers transmit prompts unchanged, so this must survive transport.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn prompt_byte_identity(prompt_profile: String, prompt: String) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        prompt_profile: String,
        prompt: String,
    }
    let doc = Doc {
        domain: PROMPT_DOMAIN,
        prompt_profile,
        prompt,
    };
    crate::audio::sha256_hex(serde_json::to_vec(&doc).expect("serialize"))
}

/// Canonical identity of a numerical contract. Any tolerance, policy, or
/// identity change yields a different contract — silently loosening a
/// tolerance cannot preserve the id.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn numerical_contract_identity(policy: BackendConformancePolicy) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        policy: BackendConformancePolicy,
    }
    let doc = Doc {
        domain: NUMERICAL_CONTRACT_DOMAIN,
        policy,
    };
    crate::audio::sha256_hex(serde_json::to_vec(&doc).expect("serialize"))
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_conformance_policy(policy: BackendConformancePolicy) -> Vec<String> {
    let mut errs = Vec::new();
    for (name, value) in [
        ("numericalContractId", &policy.numerical_contract_id),
        ("tokenizerIdentity", &policy.tokenizer_identity),
        ("promptByteIdentity", &policy.prompt_byte_identity),
        ("stopTokenIdentity", &policy.stop_token_identity),
    ] {
        if value.trim().is_empty() {
            errs.push(format!("{name} must not be empty"));
        }
    }
    if policy.context_cap == 0 {
        errs.push("contextCap must be >= 1".into());
    }
    for (name, t) in [
        ("logitsTolerance", policy.logits_tolerance),
        ("embeddingTolerance", policy.embedding_tolerance),
    ] {
        if !t.is_finite() || t < 0.0 {
            errs.push(format!("{name} must be finite and non-negative"));
        }
    }
    // Exact-token claims are only meaningful in deterministic mode.
    if policy.generated_token_policy == GeneratedTokenPolicy::ExactUnderGreedy
        && !policy.deterministic_test_mode
    {
        errs.push("exactUnderGreedy requires deterministicTestMode".into());
    }
    errs
}

/// Judge one backend against a frozen policy. Identity mismatches fail hard;
/// numerical divergence beyond tolerance demands a separate contract rather
/// than a claim of equivalence.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn evaluate_backend_conformance(
    policy: BackendConformancePolicy,
    observation: BackendObservation,
) -> ConformanceVerdict {
    let policy_errs = validate_conformance_policy(policy.clone());
    if !policy_errs.is_empty() {
        return ConformanceVerdict::NonConformant {
            failure: ConformanceFailure::MalformedPolicy {
                reason: policy_errs.join("; "),
            },
        };
    }
    let fail = |failure| ConformanceVerdict::NonConformant { failure };

    if observation.tokenizer_identity != policy.tokenizer_identity {
        return fail(ConformanceFailure::TokenizerIdentityMismatch);
    }
    if observation.prompt_byte_identity != policy.prompt_byte_identity {
        return fail(ConformanceFailure::PromptByteMismatch);
    }
    if observation.stop_token_identity != policy.stop_token_identity {
        return fail(ConformanceFailure::StopTokenMismatch);
    }
    if observation.context_cap != policy.context_cap {
        return fail(ConformanceFailure::ContextCapMismatch);
    }
    if !observation.output_shape_declared {
        return fail(ConformanceFailure::OutputShapeUndeclared);
    }

    if policy.generated_token_policy == GeneratedTokenPolicy::ExactUnderGreedy {
        if !observation.greedy_determinism_available {
            return fail(ConformanceFailure::GreedyDeterminismUnavailable);
        }
        match observation.generated_tokens_match_reference {
            None => {
                return fail(ConformanceFailure::MissingMeasurement {
                    measurement: "generatedTokensMatchReference".into(),
                })
            }
            Some(false) => return fail(ConformanceFailure::GeneratedTokensDiverged),
            Some(true) => {}
        }
    }

    // A tolerance the backend never measured is unsatisfied, not satisfied.
    for (measurement, observed, tolerance) in [
        (
            "logits",
            observation.max_logit_divergence,
            policy.logits_tolerance,
        ),
        (
            "embeddings",
            observation.max_embedding_divergence,
            policy.embedding_tolerance,
        ),
    ] {
        match observed {
            None => {
                return fail(ConformanceFailure::MissingMeasurement {
                    measurement: measurement.to_string(),
                })
            }
            Some(v) if !v.is_finite() || v < 0.0 => {
                return fail(ConformanceFailure::MissingMeasurement {
                    measurement: format!("{measurement} (non-finite)"),
                })
            }
            Some(v) if v > tolerance => {
                return ConformanceVerdict::RequiresSeparateNumericalContract {
                    measurement: measurement.to_string(),
                    observed: v,
                    tolerance,
                }
            }
            Some(_) => {}
        }
    }

    ConformanceVerdict::Conformant
}

/// May two backends share one numerical-contract identity? Only when both
/// are Conformant against the same policy — never by assertion.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn may_share_numerical_contract(
    policy: BackendConformancePolicy,
    a: BackendObservation,
    b: BackendObservation,
) -> bool {
    a.backend_id != b.backend_id
        && evaluate_backend_conformance(policy.clone(), a) == ConformanceVerdict::Conformant
        && evaluate_backend_conformance(policy, b) == ConformanceVerdict::Conformant
}
