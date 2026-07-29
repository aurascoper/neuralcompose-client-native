//! Frozen generation-benchmark protocol (M7-B). Deterministic, effect-free.
//!
//! This module is the *authority* for what the comparison means: candidate
//! identity, protocol identity, prompt identity, metric records, and the
//! promotion verdict. It downloads nothing, executes no model, reads no
//! battery, and never chooses a winner by preference — the shell measures,
//! this decides.
//!
//! The protocol is frozen (hashed) BEFORE any candidate is downloaded, so a
//! threshold cannot be moved after seeing a result.

use serde::{Deserialize, Serialize};

pub const EVAL_PROTOCOL_DOMAIN: &str = "neuralcompose.generation-eval-protocol.v1";
pub const EVAL_CANDIDATE_DOMAIN: &str = "neuralcompose.generation-eval-candidate.v1";
pub const EVAL_RUN_DOMAIN: &str = "neuralcompose.generation-eval-run.v1";
const SEMANTIC_PROMPT_DOMAIN: &str = "neuralcompose.semantic-prompt.v1";
const RENDERED_PROMPT_DOMAIN: &str = "neuralcompose.rendered-prompt.v1";

/// How a candidate's artifact came to exist. The asymmetry between an
/// official download and a local conversion is recorded, never hidden:
/// Qwen2.5 publishes an official Q4_K_M, Qwen3 does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ArtifactProvenance {
    /// Downloaded from the model owner's own repository, unmodified.
    OfficialUpstream,
    /// Quantized locally from pinned official source weights.
    DerivedByConversion,
}

/// What a variant is for. Controls exist to make the conversion asymmetry
/// measurable rather than invisible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CandidateRole {
    /// One of the two Q4_K_M variants the mobile decision rests on.
    PrimaryMobile,
    /// An official artifact held alongside its primary to expose what the
    /// conversion or the quantization step changed.
    Control,
}

/// The reproducible conversion record. Every field is required for a
/// `DerivedByConversion` artifact — an unreproducible artifact is not
/// evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ConversionRecord {
    /// Exact upstream source revision the weights came from.
    pub source_repo: String,
    pub source_revision: String,
    /// One pinned llama.cpp commit does both conversion and quantization.
    pub conversion_commit: String,
    pub quantizer_commit: String,
    pub conversion_command: String,
    pub quantize_command: String,
    /// The unquantized GGUF produced before quantization — hashed so the
    /// chain is auditable end to end, not just at its final link.
    pub intermediate_sha256_hex: String,
    pub output_sha256_hex: String,
    pub quantization: String,
    /// `None` means no importance matrix was used. Never invent a calibration
    /// corpus to fill this field.
    pub importance_matrix: Option<String>,
    pub calibration_dataset: Option<String>,
    /// Requantizing an already-quantized file degrades quality; the pipeline
    /// must refuse it.
    pub allow_requantize: bool,
    pub pure_quantization: bool,
    pub source_precision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EvaluationCandidate {
    pub candidate_id: String,
    pub model_family: String,
    pub model_revision: String,
    pub quantization: String,
    pub variant_id: String,
    pub role: CandidateRole,
    pub provenance: ArtifactProvenance,
    pub artifact_sha256_hex: String,
    pub tokenizer_identity: String,
    pub chat_template_identity: String,
    /// Qwen3 defaults to thinking mode; a non-thinking comparison must prove
    /// it is off rather than trust a runtime flag.
    pub thinking_mode_disabled: bool,
    /// Present exactly when `provenance` is `DerivedByConversion`.
    pub conversion: Option<ConversionRecord>,
}

// ---------- prompts: semantic vs rendered ----------

/// A benchmark prompt carries TWO identities. The semantic input must be
/// identical across models; the rendered bytes may legitimately differ
/// because official chat templates differ — and that difference has to be
/// visible in provenance rather than silently averaged away.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BenchmarkPrompt {
    pub prompt_id: String,
    pub prompt_profile: String,
    /// Hash of the semantic message — same for every candidate.
    pub semantic_prompt_hash: String,
    /// Hash of the exact bytes this candidate's template rendered.
    pub rendered_prompt_hash: String,
    /// Token ids the runtime actually fed the model, hashed.
    pub input_token_ids_hash: String,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn semantic_prompt_hash(prompt_profile: String, message: String) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        prompt_profile: String,
        message: String,
    }
    crate::audio::sha256_hex(
        serde_json::to_vec(&Doc {
            domain: SEMANTIC_PROMPT_DOMAIN,
            prompt_profile,
            message,
        })
        .expect("serialize"),
    )
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn rendered_prompt_hash(rendered: String) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        rendered: String,
    }
    crate::audio::sha256_hex(
        serde_json::to_vec(&Doc {
            domain: RENDERED_PROMPT_DOMAIN,
            rendered,
        })
        .expect("serialize"),
    )
}

// ---------- the frozen protocol ----------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SamplerConfig {
    /// Shared across both models for the primary comparison. Model-specific
    /// recommended settings are a secondary sensitivity analysis, never the
    /// promotion comparison.
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: u32,
    pub repeat_penalty: f64,
    pub max_output_tokens: u32,
    pub context_cap: u32,
}

/// The ceilings a candidate must meet to be promotable. Frozen before any
/// candidate runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PromotionThresholds {
    pub max_cold_load_ms: u64,
    pub max_time_to_first_token_ms: u64,
    pub min_generation_tokens_per_second: f64,
    pub max_peak_rss_mb: u64,
    pub max_installed_bytes: u64,
    pub max_cancellation_latency_ms: u64,
    /// Battery drop across the sustained run, in tenths of a percent, so the
    /// contract stays integral.
    pub max_battery_drop_tenths_percent: u32,
    /// Abort rather than record a thermally-throttled number as if it were
    /// representative.
    pub thermal_cutoff_celsius_tenths: u32,
    /// A candidate must beat the other's quality panel by at least this
    /// margin to justify being larger.
    pub material_quality_margin: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EvaluationProtocol {
    pub protocol_version: u32,
    pub corpus_id: String,
    /// Digest of the committed sanitized corpus file.
    pub corpus_sha256_hex: String,
    pub prompt_count: u32,
    pub quality_rubric_id: String,
    pub sampler: SamplerConfig,
    /// Frozen seeds; multiple, so one lucky sample cannot decide anything.
    pub seeds: Vec<u64>,
    pub warmup_runs: u32,
    pub timed_runs: u32,
    pub sustained_seconds: u32,
    pub per_run_timeout_ms: u64,
    pub thresholds: PromotionThresholds,
    /// v2: the conditions a measurement is taken under. Absent in v1, which
    /// is why v1 could not support an honest battery or cold-load claim.
    pub environment: RunEnvironment,
}

/// Canonical protocol identity. Any change to the corpus, rubric, sampler,
/// seeds, run counts, or thresholds yields a different protocol — so a
/// threshold cannot be relaxed after seeing results while still claiming the
/// same protocol.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn evaluation_protocol_identity(protocol: EvaluationProtocol) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        protocol: EvaluationProtocol,
    }
    let mut canonical = protocol;
    canonical.seeds.sort_unstable();
    crate::audio::sha256_hex(
        serde_json::to_vec(&Doc {
            domain: EVAL_PROTOCOL_DOMAIN,
            protocol: canonical,
        })
        .expect("serialize"),
    )
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn candidate_identity(candidate: EvaluationCandidate) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        candidate: EvaluationCandidate,
    }
    crate::audio::sha256_hex(
        serde_json::to_vec(&Doc {
            domain: EVAL_CANDIDATE_DOMAIN,
            candidate,
        })
        .expect("serialize"),
    )
}

// ---------- protocol v2: the run environment ----------
//
// v1 (commit 72a0367) froze the corpus, sampler, seeds and ceilings but said
// nothing about the conditions a measurement is taken under. It carried a
// battery ceiling with no statement of charging state, which would have let a
// plugged-in run masquerade as a discharge-cost measurement. v1 is preserved
// unchanged in history; this supersedes it and, because every term is hashed,
// necessarily yields a DIFFERENT protocol identity.

/// What "cold" honestly means on Android. An ordinary app cannot drop the OS
/// page cache, so a force-stopped process is process-cold while the model may
/// still be resident in cache — the evidence is named for what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ColdDefinition {
    /// Process and context destroyed; OS page-cache state unknown and NOT
    /// claimed to be cold. This is what Android actually permits.
    ProcessColdPageCacheUnknown,
    /// Page cache demonstrably evicted (e.g. by an external, privileged step).
    /// Only usable when that step is actually performed and recorded.
    FilesystemCacheCold,
}

/// What "warm" reuses. Naming it prevents two candidates being measured under
/// different amounts of retained state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum WarmDefinition {
    /// Same process, model file already loaded, native context recreated.
    ContextRecreatedModelResident,
    /// Same process and the native context is reused as-is.
    ContextReused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ChargingState {
    /// Latency, throughput, memory, temperature and throttling remain
    /// interpretable; battery delta does NOT become a discharge-cost claim.
    PluggedIn,
    OnBattery,
}

/// How a cooldown ends. Elapsed time alone does not prove a device returned
/// to its starting thermal state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CooldownExit {
    /// Wait until the device reports at or below the start ceiling AND a
    /// minimum time has passed.
    TemperatureAtOrBelowStartCeiling,
    /// Elapsed time only — weaker, and recorded as such.
    ElapsedTimeOnly,
}

/// Why a run is not a usable measurement. Kept distinct from a low score:
/// an inadmissible run is not evidence, and must never be averaged in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RunDisposition {
    Admissible,
    Interrupted {
        reason: String,
    },
    ThermallyThrottled,
    CancelledByOperator,
    /// The continuation exposed private stepwise deliberation. Retained
    /// locally for inspection; never scored as a poor answer.
    InadmissibleReasoningLeakage {
        detector: String,
    },
    PromptParityMismatch,
}

/// The conditions every timed run must be taken under.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RunEnvironment {
    pub cold_definition: ColdDefinition,
    pub warm_definition: WarmDefinition,
    pub charging_state: ChargingState,
    pub cooldown_exit: CooldownExit,
    pub cooldown_minimum_seconds: u32,
    /// A run may not START above this temperature.
    pub thermal_start_ceiling_celsius_tenths: u32,
    pub screen_on: bool,
    pub screen_brightness_percent: u32,
    /// Airplane mode proves local execution and removes radio power noise.
    pub airplane_mode: bool,
    /// Process restarted between candidates so neither inherits the other's
    /// warmed state.
    pub restart_process_between_candidates: bool,
    /// Pack integrity re-verified between candidates.
    pub recheck_pack_integrity_between_candidates: bool,
    /// Candidate order alternates rather than running all of A then all of B,
    /// so ordering and accumulated heat do not bias one candidate.
    pub alternating_candidate_order: bool,
}

/// Is a battery figure taken under these conditions interpretable as an
/// energy cost? Plugged in, it is not — the number may be recorded, but it
/// cannot support a discharge-efficiency claim.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn battery_delta_is_energy_evidence(env: RunEnvironment) -> bool {
    env.charging_state == ChargingState::OnBattery
}

/// Does the environment support the cold claim it makes?
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_run_environment(env: RunEnvironment) -> Vec<String> {
    let mut errs = Vec::new();
    if env.cooldown_minimum_seconds == 0 {
        errs.push("cooldownMinimumSeconds must be > 0".into());
    }
    if env.thermal_start_ceiling_celsius_tenths == 0 {
        errs.push("thermalStartCeiling must be set".into());
    }
    if !env.alternating_candidate_order {
        errs.push("candidate order must alternate to control ordering bias".into());
    }
    if !env.restart_process_between_candidates {
        errs.push("process must restart between candidates".into());
    }
    if !env.recheck_pack_integrity_between_candidates {
        errs.push("pack integrity must be rechecked between candidates".into());
    }
    errs
}

// ---------- observations from the shell ----------

/// Measurements only the device can make. Rust receives these; it never
/// reads a battery or a thermal sensor itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CostObservation {
    pub cold_load_ms: u64,
    pub warm_load_ms: u64,
    pub time_to_first_token_ms: u64,
    pub prompt_tokens_per_second: f64,
    pub generation_tokens_per_second: f64,
    pub peak_rss_mb: u64,
    pub model_memory_mb: u64,
    pub cancellation_latency_ms: u64,
    pub installed_bytes: u64,
    pub battery_drop_tenths_percent: u32,
    pub peak_temperature_celsius_tenths: u32,
    /// True when the device throttled during the run — such a run is not a
    /// representative measurement and cannot promote anything.
    pub thermally_throttled: bool,
    pub background_foreground_recovered: bool,
}

/// Blinded quality scores, 0.0–1.0 per axis. The shell collects them; the
/// rubric that produced them is pinned by `quality_rubric_id`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct QualityPanel {
    pub instruction_adherence: f64,
    pub required_output_structure: f64,
    /// Higher is better: 1.0 means nothing was invented.
    pub avoids_unsupported_invention: f64,
    pub appropriate_uncertainty: f64,
    /// Higher is better: 1.0 means no false refusals.
    pub avoids_false_refusal: f64,
    pub substantive_position_retention: f64,
    /// Higher is better: 1.0 means no degenerate repetition.
    pub avoids_repetition: f64,
    pub truncation_behavior: f64,
    pub language_preservation: f64,
    pub prompt_profile_fidelity: f64,
}

impl QualityPanel {
    fn axes(&self) -> [f64; 10] {
        [
            self.instruction_adherence,
            self.required_output_structure,
            self.avoids_unsupported_invention,
            self.appropriate_uncertainty,
            self.avoids_false_refusal,
            self.substantive_position_retention,
            self.avoids_repetition,
            self.truncation_behavior,
            self.language_preservation,
            self.prompt_profile_fidelity,
        ]
    }
}

/// Mean of the ten axes, or `None` if any axis is outside 0.0–1.0 or
/// non-finite — a malformed panel scores nothing rather than something.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn quality_score(panel: QualityPanel) -> Option<f64> {
    let axes = panel.axes();
    if axes.iter().any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0) {
        return None;
    }
    Some(axes.iter().sum::<f64>() / axes.len() as f64)
}

/// One candidate's complete result under one frozen protocol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CandidateResult {
    pub candidate_id: String,
    pub candidate_identity: String,
    pub protocol_identity: String,
    pub device: String,
    pub os_version: String,
    pub runtime_identity: String,
    /// Every prompt actually run, with both prompt identities.
    pub prompts: Vec<BenchmarkPrompt>,
    pub cost: CostObservation,
    pub quality: QualityPanel,
    /// v2: why this run is or is not a usable measurement.
    pub disposition: RunDisposition,
}

// ---------- admission and promotion ----------

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AdmissionFailure {
    ProtocolMismatch,
    MissingConversionRecord,
    UnexpectedConversionRecord,
    RequantizationNotAllowed,
    ThinkingModeNotDisabled,
    TokenizerIdentityMismatch,
    ChatTemplateIdentityMismatch,
    SemanticPromptMismatch,
    MalformedQualityPanel,
    ThermallyThrottled,
    Disqualified { reason: String },
}

/// May this result enter the comparison at all? Admission is separate from
/// winning: a result that fails here is not a bad score, it is not evidence.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn admit_result(
    result: CandidateResult,
    candidate: EvaluationCandidate,
    protocol_identity: String,
) -> Option<AdmissionFailure> {
    if result.disposition != RunDisposition::Admissible {
        return Some(AdmissionFailure::Disqualified {
            reason: format!("{:?}", result.disposition),
        });
    }
    if result.protocol_identity != protocol_identity {
        return Some(AdmissionFailure::ProtocolMismatch);
    }
    match (candidate.provenance, &candidate.conversion) {
        (ArtifactProvenance::DerivedByConversion, None) => {
            return Some(AdmissionFailure::MissingConversionRecord)
        }
        (ArtifactProvenance::OfficialUpstream, Some(_)) => {
            return Some(AdmissionFailure::UnexpectedConversionRecord)
        }
        (ArtifactProvenance::DerivedByConversion, Some(c)) if c.allow_requantize => {
            return Some(AdmissionFailure::RequantizationNotAllowed)
        }
        _ => {}
    }
    if !candidate.thinking_mode_disabled {
        return Some(AdmissionFailure::ThinkingModeNotDisabled);
    }
    if result.cost.thermally_throttled {
        return Some(AdmissionFailure::ThermallyThrottled);
    }
    if quality_score(result.quality).is_none() {
        return Some(AdmissionFailure::MalformedQualityPanel);
    }
    None
}

/// Does a result meet every frozen ceiling?
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn meets_thresholds(cost: CostObservation, t: PromotionThresholds) -> Vec<String> {
    let mut failures = Vec::new();
    let mut check = |ok: bool, name: &str| {
        if !ok {
            failures.push(name.to_string());
        }
    };
    check(cost.cold_load_ms <= t.max_cold_load_ms, "coldLoadMs");
    check(
        cost.time_to_first_token_ms <= t.max_time_to_first_token_ms,
        "timeToFirstTokenMs",
    );
    check(
        cost.generation_tokens_per_second >= t.min_generation_tokens_per_second,
        "generationTokensPerSecond",
    );
    check(cost.peak_rss_mb <= t.max_peak_rss_mb, "peakRssMb");
    check(
        cost.installed_bytes <= t.max_installed_bytes,
        "installedBytes",
    );
    check(
        cost.cancellation_latency_ms <= t.max_cancellation_latency_ms,
        "cancellationLatencyMs",
    );
    check(
        cost.battery_drop_tenths_percent <= t.max_battery_drop_tenths_percent,
        "batteryDrop",
    );
    check(
        cost.peak_temperature_celsius_tenths <= t.thermal_cutoff_celsius_tenths,
        "peakTemperature",
    );
    check(!cost.thermally_throttled, "thermallyThrottled");
    check(
        cost.background_foreground_recovered,
        "backgroundForegroundRecovery",
    );
    failures
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PromotionVerdict {
    /// One candidate is promotable as the mobile default.
    Promote {
        candidate_id: String,
        quality_score: f64,
        margin: f64,
    },
    /// Both are admissible and both pass, but neither is materially better —
    /// a split (Basic / Enhanced) is the honest outcome.
    SplitTiers {
        basic_candidate_id: String,
        enhanced_candidate_id: String,
    },
    /// Nothing is promoted. This is a legitimate result, not a failure to
    /// decide.
    PromoteNothing { reason: String },
}

/// The frozen decision rule. Newer does not win; smaller does not win; a
/// larger candidate must *earn* its size by a material margin. Both failing
/// promotes nothing.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn evaluate_promotion(
    a: CandidateResult,
    b: CandidateResult,
    thresholds: PromotionThresholds,
) -> PromotionVerdict {
    let nothing = |reason: &str| PromotionVerdict::PromoteNothing {
        reason: reason.to_string(),
    };
    let (sa, sb) = match (quality_score(a.quality), quality_score(b.quality)) {
        (Some(x), Some(y)) => (x, y),
        _ => return nothing("a quality panel was malformed"),
    };
    let fa = meets_thresholds(a.cost.clone(), thresholds.clone());
    let fb = meets_thresholds(b.cost.clone(), thresholds.clone());

    match (fa.is_empty(), fb.is_empty()) {
        (false, false) => nothing("neither candidate met the frozen ceilings"),
        (true, false) => PromotionVerdict::Promote {
            candidate_id: a.candidate_id,
            quality_score: sa,
            margin: sa - sb,
        },
        (false, true) => PromotionVerdict::Promote {
            candidate_id: b.candidate_id,
            quality_score: sb,
            margin: sb - sa,
        },
        (true, true) => {
            let margin = (sa - sb).abs();
            if margin < thresholds.material_quality_margin {
                // Neither materially better: cheaper is not a winner, it is a
                // tier. Larger installed size becomes Enhanced.
                let (basic, enhanced) = if a.cost.installed_bytes <= b.cost.installed_bytes {
                    (a.candidate_id, b.candidate_id)
                } else {
                    (b.candidate_id, a.candidate_id)
                };
                PromotionVerdict::SplitTiers {
                    basic_candidate_id: basic,
                    enhanced_candidate_id: enhanced,
                }
            } else if sa > sb {
                PromotionVerdict::Promote {
                    candidate_id: a.candidate_id,
                    quality_score: sa,
                    margin,
                }
            } else {
                PromotionVerdict::Promote {
                    candidate_id: b.candidate_id,
                    quality_score: sb,
                    margin,
                }
            }
        }
    }
}
