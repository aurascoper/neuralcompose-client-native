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
    /// v3: declared in advance, so a harness cannot render something else
    /// and still be admitted.
    pub prompt_bindings: Vec<PromptBinding>,
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

/// Whether a battery figure may gate promotion at all. Under a plugged-in
/// protocol the delta is recorded but carries no energy claim, so letting it
/// disqualify a candidate would be inventing evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BatteryEvidencePolicy {
    TelemetryOnlyPluggedIn,
    OnBattery { max_drop_tenths_percent: u32 },
}

/// v3: one timed run, as OBSERVED. The protocol says what conditions were
/// required; this says what actually happened, so a run taken under other
/// conditions cannot simply carry the protocol hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RunMode {
    Cold,
    Warm,
    Sustained,
    Cancellation,
}

/// Evidence for a cold claim. `FilesystemCacheCold` requires a non-empty
/// external privileged-step id — an enum variant cannot by itself establish
/// that the page cache was evicted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ColdEvidence {
    ProcessCold { process_instance_id: String },
    FilesystemCacheCold { external_step_evidence_id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RunObservation {
    pub run_id: String,
    pub candidate_id: String,
    pub seed: u64,
    pub mode: RunMode,
    pub sequence_index: u32,
    pub started_monotonic_ms: u64,
    pub ended_monotonic_ms: u64,
    pub observed_charging_state: ChargingState,
    pub observed_screen_on: bool,
    pub observed_brightness_percent: u32,
    pub observed_airplane_mode: bool,
    pub process_instance_id: String,
    /// Digest proving the pack was re-verified before this run.
    pub pack_integrity_receipt: String,
    pub cold_evidence: ColdEvidence,
    pub start_temperature_celsius_tenths: u32,
    pub cooldown_duration_ms: u64,
    pub cooldown_exit_temperature_celsius_tenths: u32,
    pub thermal_sensor_identity: String,
    pub throttling_detector_identity: String,
    pub disposition: RunDisposition,
}

/// v3: the exact schedule, frozen. `alternating_candidate_order: true` is a
/// property; this is the promised sequence, so runs cannot be reordered,
/// dropped, duplicated, or selectively discarded after the fact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RunPlanEntry {
    pub index: u32,
    pub candidate_id: String,
    pub mode: RunMode,
    pub seed: u64,
}

/// A frozen corpus entry: the semantic input every candidate shares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ExpectedPrompt {
    pub prompt_id: String,
    pub semantic_prompt_hash: String,
}

/// The candidate-specific rendering of one frozen prompt. Rendered bytes and
/// token ids legitimately differ per template — but they are declared in
/// advance, so the harness cannot silently render something else.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PromptBinding {
    pub prompt_id: String,
    pub rendered_prompt_hash: String,
    pub input_token_ids_hash: String,
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
    /// v3: battery only gates promotion when the run was actually on
    /// battery. Plugged in, the figure is telemetry and cannot pass or fail
    /// a candidate.
    pub battery_policy: BatteryEvidencePolicy,
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
    /// v3: the frozen semantic corpus every candidate shares.
    pub expected_prompts: Vec<ExpectedPrompt>,
    /// v3: the exact run schedule, not merely the alternating property.
    pub run_plan: Vec<RunPlanEntry>,
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
    /// v3: one entry per planned run, as observed.
    pub observations: Vec<RunObservation>,
}

/// v3: hashing an invalid protocol does not make it valid. Admission and
/// promotion both run this first.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_evaluation_protocol(protocol: EvaluationProtocol) -> Vec<String> {
    let mut e = validate_run_environment(protocol.environment.clone());
    let mut bad = |m: &str| e.push(m.to_string());
    if protocol.protocol_version != 3 {
        bad("unsupported protocolVersion (v3 expected)");
    }
    if protocol.corpus_id.trim().is_empty() || protocol.quality_rubric_id.trim().is_empty() {
        bad("corpusId and qualityRubricId must be non-empty");
    }
    if protocol.seeds.is_empty() {
        bad("at least one seed required");
    }
    let mut seen = std::collections::HashSet::new();
    if protocol.seeds.iter().any(|s| !seen.insert(*s)) {
        bad("duplicate seeds");
    }
    if protocol.prompt_count == 0 || protocol.expected_prompts.is_empty() {
        bad("a corpus with no prompts measures nothing");
    }
    if protocol.expected_prompts.len() as u32 != protocol.prompt_count {
        bad("promptCount disagrees with the frozen corpus");
    }
    if protocol.run_plan.is_empty() {
        bad("run plan must be frozen, not merely 'alternating'");
    }
    for (i, entry) in protocol.run_plan.iter().enumerate() {
        if entry.index as usize != i {
            bad("run plan indices must be dense and ordered");
            break;
        }
    }
    if protocol.timed_runs == 0
        || protocol.sustained_seconds == 0
        || protocol.per_run_timeout_ms == 0
    {
        bad("zero run count, duration or timeout");
    }
    let sp = &protocol.sampler;
    for (name, v) in [
        ("temperature", sp.temperature),
        ("topP", sp.top_p),
        ("repeatPenalty", sp.repeat_penalty),
    ] {
        if !v.is_finite() || v < 0.0 {
            bad(&format!("{name} must be finite and non-negative"));
        }
    }
    if sp.top_p > 1.0 {
        bad("topP must be within 0..=1");
    }
    if sp.max_output_tokens == 0 || sp.context_cap == 0 {
        bad("zero context or output cap");
    }
    let t = &protocol.thresholds;
    if !t.material_quality_margin.is_finite() || t.material_quality_margin < 0.0 {
        bad("materialQualityMargin must be finite and non-negative");
    }
    if !t.min_generation_tokens_per_second.is_finite() {
        bad("minGenerationTokensPerSecond must be finite");
    }
    if t.max_cold_load_ms == 0 || t.max_peak_rss_mb == 0 || t.max_installed_bytes == 0 {
        bad("a zero ceiling can never be met");
    }
    let env = &protocol.environment;
    if env.screen_brightness_percent > 100 {
        bad("brightness above 100%");
    }
    if !env.screen_on && env.screen_brightness_percent > 0 {
        bad("screen off but a nonzero brightness claimed");
    }
    // A plugged-in protocol may not carry an on-battery gate, and vice versa.
    match (&env.charging_state, &t.battery_policy) {
        (ChargingState::PluggedIn, BatteryEvidencePolicy::OnBattery { .. }) => {
            bad("plugged-in protocol cannot gate on battery discharge")
        }
        (ChargingState::OnBattery, BatteryEvidencePolicy::TelemetryOnlyPluggedIn) => {
            bad("on-battery protocol declares a plugged-in battery policy")
        }
        _ => {}
    }
    if env.thermal_start_ceiling_celsius_tenths >= t.thermal_cutoff_celsius_tenths {
        bad("thermal start ceiling is at or above the abort cutoff");
    }
    e
}

// ---------- admission and promotion ----------

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AdmissionFailure {
    ProtocolMismatch,
    ProtocolInvalid { reason: String },
    CandidateIdentityMismatch,
    ResultCandidateMismatch,
    PromptCountMismatch,
    DuplicatePromptId { prompt_id: String },
    UnknownPromptId { prompt_id: String },
    RenderedPromptMismatch { prompt_id: String },
    InputTokenMismatch { prompt_id: String },
    RunLedgerMismatch { reason: String },
    ColdEvidenceMissing { run_id: String },
    EnvironmentDeviation { reason: String },
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
/// Callers should not need to remember to call this — `evaluate_promotion`
/// runs it internally, and there is no promotion path that skips it.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn admit_result(
    result: CandidateResult,
    candidate: EvaluationCandidate,
    protocol: EvaluationProtocol,
) -> Option<AdmissionFailure> {
    let protocol_errs = validate_evaluation_protocol(protocol.clone());
    if !protocol_errs.is_empty() {
        return Some(AdmissionFailure::ProtocolInvalid {
            reason: protocol_errs.join("; "),
        });
    }
    if result.disposition != RunDisposition::Admissible {
        return Some(AdmissionFailure::Disqualified {
            reason: format!("{:?}", result.disposition),
        });
    }
    if result.protocol_identity != evaluation_protocol_identity(protocol.clone()) {
        return Some(AdmissionFailure::ProtocolMismatch);
    }
    // The result must actually belong to this candidate.
    if result.candidate_id != candidate.candidate_id {
        return Some(AdmissionFailure::ResultCandidateMismatch);
    }
    if result.candidate_identity != candidate_identity(candidate.clone()) {
        return Some(AdmissionFailure::CandidateIdentityMismatch);
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

    // Prompt parity: exactly the frozen corpus, once each, rendered and
    // tokenized as this candidate declared in advance.
    if result.prompts.len() != protocol.expected_prompts.len() {
        return Some(AdmissionFailure::PromptCountMismatch);
    }
    let mut seen = std::collections::HashSet::new();
    for p in &result.prompts {
        if !seen.insert(p.prompt_id.clone()) {
            return Some(AdmissionFailure::DuplicatePromptId {
                prompt_id: p.prompt_id.clone(),
            });
        }
        let expected = match protocol
            .expected_prompts
            .iter()
            .find(|e| e.prompt_id == p.prompt_id)
        {
            None => {
                return Some(AdmissionFailure::UnknownPromptId {
                    prompt_id: p.prompt_id.clone(),
                })
            }
            Some(e) => e,
        };
        if p.semantic_prompt_hash != expected.semantic_prompt_hash {
            return Some(AdmissionFailure::SemanticPromptMismatch);
        }
        let binding = match candidate
            .prompt_bindings
            .iter()
            .find(|b| b.prompt_id == p.prompt_id)
        {
            None => {
                return Some(AdmissionFailure::UnknownPromptId {
                    prompt_id: p.prompt_id.clone(),
                })
            }
            Some(b) => b,
        };
        if p.rendered_prompt_hash != binding.rendered_prompt_hash {
            return Some(AdmissionFailure::RenderedPromptMismatch {
                prompt_id: p.prompt_id.clone(),
            });
        }
        if p.input_token_ids_hash != binding.input_token_ids_hash {
            return Some(AdmissionFailure::InputTokenMismatch {
                prompt_id: p.prompt_id.clone(),
            });
        }
    }

    // Run ledger: one observation per planned run for this candidate, in
    // order. Missing, duplicated, reordered or discarded runs cannot yield a
    // verdict.
    let planned: Vec<&RunPlanEntry> = protocol
        .run_plan
        .iter()
        .filter(|e| e.candidate_id == candidate.candidate_id)
        .collect();
    if result.observations.len() != planned.len() {
        return Some(AdmissionFailure::RunLedgerMismatch {
            reason: format!(
                "expected {} runs, ledger has {}",
                planned.len(),
                result.observations.len()
            ),
        });
    }
    for (entry, obs) in planned.iter().zip(result.observations.iter()) {
        if obs.sequence_index != entry.index || obs.mode != entry.mode || obs.seed != entry.seed {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} does not match the frozen plan", entry.index),
            });
        }
        if obs.candidate_id != candidate.candidate_id {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} belongs to another candidate", entry.index),
            });
        }
        if obs.disposition != RunDisposition::Admissible {
            return Some(AdmissionFailure::Disqualified {
                reason: format!("run {}: {:?}", entry.index, obs.disposition),
            });
        }
        if obs.pack_integrity_receipt.trim().is_empty() {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} has no pack-integrity receipt", entry.index),
            });
        }
        // A cold CLAIM needs cold EVIDENCE.
        match (&protocol.environment.cold_definition, &obs.cold_evidence) {
            (
                ColdDefinition::FilesystemCacheCold,
                ColdEvidence::FilesystemCacheCold {
                    external_step_evidence_id,
                },
            ) if external_step_evidence_id.trim().is_empty() => {
                return Some(AdmissionFailure::ColdEvidenceMissing {
                    run_id: obs.run_id.clone(),
                })
            }
            (ColdDefinition::FilesystemCacheCold, ColdEvidence::ProcessCold { .. }) => {
                return Some(AdmissionFailure::ColdEvidenceMissing {
                    run_id: obs.run_id.clone(),
                })
            }
            _ => {}
        }
        // The observed conditions must be the frozen ones.
        let env = &protocol.environment;
        let deviation = if obs.observed_charging_state != env.charging_state {
            Some("charging state")
        } else if obs.observed_airplane_mode != env.airplane_mode {
            Some("airplane mode")
        } else if obs.observed_screen_on != env.screen_on {
            Some("screen state")
        } else if obs.start_temperature_celsius_tenths > env.thermal_start_ceiling_celsius_tenths {
            Some("start temperature above the ceiling")
        } else if env.cooldown_exit == CooldownExit::TemperatureAtOrBelowStartCeiling
            && obs.cooldown_exit_temperature_celsius_tenths
                > env.thermal_start_ceiling_celsius_tenths
        {
            Some("cooldown did not reach the start ceiling")
        } else if obs.cooldown_duration_ms < env.cooldown_minimum_seconds as u64 * 1000 {
            Some("cooldown shorter than the frozen minimum")
        } else {
            None
        };
        if let Some(reason) = deviation {
            return Some(AdmissionFailure::EnvironmentDeviation {
                reason: format!("run {}: {reason}", entry.index),
            });
        }
    }

    if result.cost.thermally_throttled {
        return Some(AdmissionFailure::ThermallyThrottled);
    }
    if quality_score(result.quality).is_none() {
        return Some(AdmissionFailure::MalformedQualityPanel);
    }
    None
}

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
    // Only a genuine on-battery run may gate on battery. Plugged in, the
    // figure is telemetry — using it to pass or fail would invent evidence.
    if let BatteryEvidencePolicy::OnBattery {
        max_drop_tenths_percent,
    } = t.battery_policy
    {
        check(
            cost.battery_drop_tenths_percent <= max_drop_tenths_percent,
            "batteryDrop",
        );
    }
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

/// The frozen decision rule, behind a sealed door. There is deliberately no
/// promotion path that accepts already-assumed-valid results: this admits
/// both candidates itself, so an inadmissible run cannot be promoted by
/// skipping a call.
///
/// Newer does not win; smaller does not win; a larger candidate must beat the
/// other by a material margin or the honest outcome is a tier split; and both
/// failing promotes nothing.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn evaluate_promotion(
    protocol: EvaluationProtocol,
    candidate_a: EvaluationCandidate,
    result_a: CandidateResult,
    candidate_b: EvaluationCandidate,
    result_b: CandidateResult,
) -> PromotionVerdict {
    let nothing = |reason: String| PromotionVerdict::PromoteNothing { reason };

    let protocol_errs = validate_evaluation_protocol(protocol.clone());
    if !protocol_errs.is_empty() {
        return nothing(format!("protocol invalid: {}", protocol_errs.join("; ")));
    }
    if let Some(f) = admit_result(result_a.clone(), candidate_a.clone(), protocol.clone()) {
        return nothing(format!("candidate A inadmissible: {f:?}"));
    }
    if let Some(f) = admit_result(result_b.clone(), candidate_b.clone(), protocol.clone()) {
        return nothing(format!("candidate B inadmissible: {f:?}"));
    }

    let (sa, sb) = match (
        quality_score(result_a.quality),
        quality_score(result_b.quality),
    ) {
        (Some(x), Some(y)) => (x, y),
        _ => return nothing("a quality panel was malformed".into()),
    };
    let thresholds = protocol.thresholds.clone();
    let fa = meets_thresholds(result_a.cost.clone(), thresholds.clone());
    let fb = meets_thresholds(result_b.cost.clone(), thresholds.clone());

    match (fa.is_empty(), fb.is_empty()) {
        (false, false) => nothing("neither candidate met the frozen ceilings".into()),
        (true, false) => PromotionVerdict::Promote {
            candidate_id: result_a.candidate_id,
            quality_score: sa,
            margin: sa - sb,
        },
        (false, true) => PromotionVerdict::Promote {
            candidate_id: result_b.candidate_id,
            quality_score: sb,
            margin: sb - sa,
        },
        (true, true) => {
            let margin = (sa - sb).abs();
            if margin < thresholds.material_quality_margin {
                let (basic, enhanced) =
                    if result_a.cost.installed_bytes <= result_b.cost.installed_bytes {
                        (result_a.candidate_id, result_b.candidate_id)
                    } else {
                        (result_b.candidate_id, result_a.candidate_id)
                    };
                PromotionVerdict::SplitTiers {
                    basic_candidate_id: basic,
                    enhanced_candidate_id: enhanced,
                }
            } else if sa > sb {
                PromotionVerdict::Promote {
                    candidate_id: result_a.candidate_id,
                    quality_score: sa,
                    margin,
                }
            } else {
                PromotionVerdict::Promote {
                    candidate_id: result_b.candidate_id,
                    quality_score: sb,
                    margin,
                }
            }
        }
    }
}
