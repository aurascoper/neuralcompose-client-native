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
//!
//! # The authority boundary, at compile time
//!
//! No field, parameter or constructor on this module's public surface accepts
//! a corpus digest. The doctests below are that proof, and they come in
//! **pairs**: a `compile_fail` block on its own proves nothing, because it
//! passes when the code fails to compile for any reason at all — a typo would
//! satisfy it. Each companion block differs *only* by the digest and must
//! compile, so the pair localises the failure to the thing being asserted.
//!
//! Reading a corpus digest off the declaration does not compile:
//!
//! ```compile_fail
//! use neuralcompose_mobile_core::generation_eval::V6Declaration;
//! fn read(d: &V6Declaration) -> &String {
//!     &d.corpus_sha256_hex
//! }
//! ```
//!
//! Reading the corpus itself does:
//!
//! ```
//! use neuralcompose_mobile_core::generation_eval::{BenchmarkCorpus, V6Declaration};
//! fn read(d: &V6Declaration) -> &BenchmarkCorpus {
//!     &d.corpus
//! }
//! ```
//!
//! Constructing a corpus that carries a digest alongside its content does not
//! compile — this is the v5 shape, kept here as the thing that must stay
//! impossible:
//!
//! ```compile_fail
//! use neuralcompose_mobile_core::generation_eval::{frozen_corpus_v1, BenchmarkCorpus};
//! let c = frozen_corpus_v1();
//! let _ = BenchmarkCorpus {
//!     corpus_id: c.corpus_id,
//!     prompts: c.prompts,
//!     corpus_sha256_hex: String::new(),
//! };
//! ```
//!
//! The same construction without it compiles:
//!
//! ```
//! use neuralcompose_mobile_core::generation_eval::{frozen_corpus_v1, BenchmarkCorpus};
//! let c = frozen_corpus_v1();
//! let _ = BenchmarkCorpus {
//!     corpus_id: c.corpus_id,
//!     prompts: c.prompts,
//! };
//! ```
//!
//! And identity is derived from content ALONE — there is no form of
//! `corpus_identity` that also takes an asserted digest:
//!
//! ```compile_fail
//! use neuralcompose_mobile_core::generation_eval::{corpus_identity, BenchmarkCorpus};
//! let _f: fn(BenchmarkCorpus, String) -> String = corpus_identity;
//! ```
//!
//! ```
//! use neuralcompose_mobile_core::generation_eval::{corpus_identity, BenchmarkCorpus};
//! let _f: fn(BenchmarkCorpus) -> String = corpus_identity;
//! ```
//!
//! No finite compile-fail set can catch every future euphemism for "digest",
//! so this is one of three layers and not the whole answer: the compile-level
//! pairs above, the source-surface scan in `tests/m7b_eval_protocol.rs`, and
//! the runtime check in [`validate_v6_declaration`] that the corpus IS the
//! build-owned one rather than merely a well-shaped one.

use serde::{Deserialize, Serialize};

pub const EVAL_PROTOCOL_DOMAIN: &str = "neuralcompose.generation-eval-protocol.v1";
pub const EVAL_CANDIDATE_DOMAIN: &str = "neuralcompose.generation-eval-candidate.v1";
pub const EVAL_RUN_DOMAIN: &str = "neuralcompose.generation-eval-run.v1";
/// v6: the frozen corpus is a value with an identity, not a caller-asserted
/// digest. Its own domain keeps a corpus hash from ever being mistaken for a
/// protocol, candidate or prompt hash.
pub const EVAL_CORPUS_DOMAIN: &str = "neuralcompose.generation-eval-corpus.v1";
/// v6 bumps this domain because the hashed inputs changed shape: a single
/// free `prompt_profile` string became two closed axes. Prompt hashes recorded
/// under v1 remain exactly what they were; they are simply not comparable with
/// v6 hashes, and a shared domain would have hidden that.
const SEMANTIC_PROMPT_DOMAIN: &str = "neuralcompose.semantic-prompt.v2";
const RENDERED_PROMPT_DOMAIN: &str = "neuralcompose.rendered-prompt.v1";

/// Fixed-point denominator for every quality axis and margin. Axis values are
/// integer millionths in `0..=FIXED_POINT_SCALE`; there is no float anywhere
/// on the path from a scorer's mark to a promotion decision.
pub const FIXED_POINT_SCALE: u32 = 1_000_000;

/// The evaluator schema this module implements. Bound into the declaration so
/// a later evaluator cannot reinterpret identical v6 data under an unchanged
/// declaration digest.
pub const EVALUATOR_SCHEMA_VERSION: u32 = 6;

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

// ---------- prompts: the two-axis taxonomy ----------
//
// v5 carried a single free `prompt_profile: String`. It conflated two
// independent things — WHAT operation is being evaluated, and the interaction
// context it is performed in — and being a free string it could name anything,
// so the hash was honest about its input while the input was not authoritative.
// v6 splits it into two closed enums and permits only an explicit matrix of
// pairs.

/// What operation the prompt asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BenchmarkTaskKind {
    Summarization,
    StructuredExtraction,
    SubstantiveRewrite,
    ReflectiveDialogue,
    DreamAnalysis,
    InstructionRendering,
    Disclosure,
}

/// The interaction context the operation is performed in. Named
/// `PromptContextProfile` rather than `…PresentationProfile` because several
/// values are functional or safety contexts, not presentation styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum PromptContextProfile {
    Neutral,
    Hypnagogic,
    MindfulnessObservation,
    ReturnToTask,
    StopAndDebrief,
    Privacy,
}

/// One allowed cell of the taxonomy. The cross product is NOT permitted:
/// most of the 42 combinations are meaningless, and a corpus that quietly
/// contained them would be measuring something the protocol never declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TaskContextPair {
    pub task_kind: BenchmarkTaskKind,
    pub context_profile: PromptContextProfile,
}

/// The exact nine pairs a v6 corpus may contain, in canonical order. This is
/// normative: a declaration whose allowed matrix differs from this list is
/// invalid, so the matrix cannot be widened by the caller that supplies it.
pub const ALLOWED_TASK_CONTEXT_PAIRS: [(BenchmarkTaskKind, PromptContextProfile); 9] = [
    (
        BenchmarkTaskKind::Summarization,
        PromptContextProfile::Neutral,
    ),
    (
        BenchmarkTaskKind::StructuredExtraction,
        PromptContextProfile::Neutral,
    ),
    (
        BenchmarkTaskKind::SubstantiveRewrite,
        PromptContextProfile::Neutral,
    ),
    (
        BenchmarkTaskKind::ReflectiveDialogue,
        PromptContextProfile::Hypnagogic,
    ),
    (
        BenchmarkTaskKind::DreamAnalysis,
        PromptContextProfile::Neutral,
    ),
    (
        BenchmarkTaskKind::InstructionRendering,
        PromptContextProfile::MindfulnessObservation,
    ),
    (
        BenchmarkTaskKind::InstructionRendering,
        PromptContextProfile::ReturnToTask,
    ),
    (
        BenchmarkTaskKind::InstructionRendering,
        PromptContextProfile::StopAndDebrief,
    ),
    (BenchmarkTaskKind::Disclosure, PromptContextProfile::Privacy),
];

/// The canonical allowed matrix as records.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn allowed_task_context_pairs() -> Vec<TaskContextPair> {
    ALLOWED_TASK_CONTEXT_PAIRS
        .iter()
        .map(|(task_kind, context_profile)| TaskContextPair {
            task_kind: *task_kind,
            context_profile: *context_profile,
        })
        .collect()
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
    /// v6: both taxonomy axes, checked against the frozen corpus at admission
    /// so a result cannot relabel a prompt it actually ran.
    pub task_kind: BenchmarkTaskKind,
    pub context_profile: PromptContextProfile,
    /// Hash of the semantic message — same for every candidate.
    pub semantic_prompt_hash: String,
    /// Hash of the exact bytes this candidate's template rendered.
    pub rendered_prompt_hash: String,
    /// Token ids the runtime actually fed the model, hashed.
    pub input_token_ids_hash: String,
}

/// v6: BOTH axes enter the semantic identity. Two prompts with the same words
/// under different contexts are different benchmark items — averaging them
/// together would be averaging two different questions.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn semantic_prompt_hash(
    task_kind: BenchmarkTaskKind,
    context_profile: PromptContextProfile,
    message: String,
) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        task_kind: BenchmarkTaskKind,
        context_profile: PromptContextProfile,
        message: String,
    }
    crate::audio::sha256_hex(
        serde_json::to_vec(&Doc {
            domain: SEMANTIC_PROMPT_DOMAIN,
            task_kind,
            context_profile,
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

// ---------- v6: the Rust-owned corpus ----------
//
// Through v5 the corpus existed only as `corpus_id` and `corpus_sha256_hex`
// on the protocol, validated for non-emptiness. `git ls-files` returned no
// corpus artifact at all: the digest the protocol claimed to bind was an
// unbacked caller assertion, and the only values it ever held were test
// fixtures. v6 removes both fields. The corpus is a committed, reviewable
// file; Rust compiles it in and derives its identity from its content.
//
// The authority comes from the ABSENCE of an injection point — the same
// discipline as `model_pack::RestoreResult`, which is output-only so that no
// state-changing method can be handed a forged success. There is deliberately
// no constructor, setter or function anywhere in this module that accepts a
// corpus digest.

/// The committed corpus, compiled in. `check-fixtures.sh` regenerates the
/// canonical serialization of the parsed value and diffs it against this same
/// file, so the compiled bytes and the reviewable artifact are proven to agree
/// rather than assumed to.
const CORPUS_V1_BYTES: &[u8] =
    include_bytes!("../../../contracts/generation-eval/m7b-corpus-v1.json");

/// One frozen prompt: the semantic input every candidate shares, plus the two
/// taxonomy axes that make it the benchmark item it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CorpusPrompt {
    pub prompt_id: String,
    pub task_kind: BenchmarkTaskKind,
    pub context_profile: PromptContextProfile,
    pub message: String,
}

/// The frozen corpus as a VALUE. Note what is absent: no digest field. An
/// identity is computed from this, never supplied alongside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct BenchmarkCorpus {
    pub corpus_id: String,
    pub prompts: Vec<CorpusPrompt>,
}

/// The corpus this build owns. Parsing cannot fail on a committed artifact
/// that a test proves parses; a panic here would mean the compiled-in bytes
/// are not the reviewed ones, which is not a recoverable condition.
///
/// Exported: a shell has to be able to OBTAIN the frozen corpus, or it cannot
/// build a valid declaration at all. Note the direction — this hands the
/// corpus out, it never takes one in, so it is not an injection point.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn frozen_corpus_v1() -> BenchmarkCorpus {
    serde_json::from_slice(CORPUS_V1_BYTES).expect("the committed m7b corpus must parse")
}

/// Canonical corpus identity, derived from content. Reordering, renaming or
/// editing any prompt — or changing either taxonomy axis on one — yields a
/// different corpus and therefore a different declaration.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn corpus_identity(corpus: BenchmarkCorpus) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        corpus: BenchmarkCorpus,
    }
    crate::audio::sha256_hex(
        serde_json::to_vec(&Doc {
            domain: EVAL_CORPUS_DOMAIN,
            corpus,
        })
        .expect("serialize"),
    )
}

/// How many prompts one allowed pair must contribute — EXACTLY, not at least.
///
/// A minimum-per-pair rule does not bound weight. Under the frozen
/// equal-prompt macro rule, ten `Summarization × Neutral` prompts and one
/// `ReflectiveDialogue × Hypnagogic` satisfy a minimum of one while giving
/// summarization ten times the influence over the promotion decision. Quota
/// shape IS the weighting, so it is declared exactly and hashed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PairQuota {
    pub pair: TaskContextPair,
    pub exact_count: u32,
}

/// The declared composition of the corpus. Part of the declaration identity,
/// so the effective domain weighting is explicit and reviewable instead of
/// being an emergent property of however many prompts someone wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CorpusCompositionPolicy {
    /// The pairs the corpus may contain. Validated against
    /// `ALLOWED_TASK_CONTEXT_PAIRS`, so this field records the matrix rather
    /// than choosing it.
    pub allowed_task_context_pairs: Vec<TaskContextPair>,
    /// One entry per allowed pair, no more and no fewer.
    pub exact_required_count_per_pair: Vec<PairQuota>,
}

/// v6-1's composition: two prompts per allowed pair.
///
/// Equal counts per PAIR is the choice, and its consequence is stated rather
/// than hidden: because `InstructionRendering` appears in three contexts, it
/// carries 6 of 18 prompts and therefore a third of the macro weight. That is
/// deliberate — instruction rendering under mindfulness, return-to-task and
/// stop-and-debrief is where a wrong output actually reaches someone mid
/// practice. If unequal importance is wanted later, introduce declared
/// fixed-point task weights; never manipulate prompt counts implicitly.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn frozen_composition_policy_v1() -> CorpusCompositionPolicy {
    let pairs = allowed_task_context_pairs();
    CorpusCompositionPolicy {
        exact_required_count_per_pair: pairs
            .iter()
            .map(|pair| PairQuota {
                pair: *pair,
                exact_count: 2,
            })
            .collect(),
        allowed_task_context_pairs: pairs,
    }
}

/// The frozen semantic corpus as the admission layer needs it, DERIVED from
/// the corpus value. v5 carried this as a caller-supplied `expected_prompts`
/// list, which was a second place to assert a prompt identity that no longer
/// had to agree with any prompt text.
pub fn expected_prompts(corpus: &BenchmarkCorpus) -> Vec<ExpectedPrompt> {
    corpus
        .prompts
        .iter()
        .map(|p| ExpectedPrompt {
            prompt_id: p.prompt_id.clone(),
            semantic_prompt_hash: semantic_prompt_hash(
                p.task_kind,
                p.context_profile,
                p.message.clone(),
            ),
        })
        .collect()
}

/// The same derivation across the FFI boundary, where an owned argument is
/// required. A shell needs this to fill in the prompt identities on a result;
/// it computes them from corpus content and cannot be told what they are.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn corpus_expected_prompts(corpus: BenchmarkCorpus) -> Vec<ExpectedPrompt> {
    expected_prompts(&corpus)
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
    Warmup,
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

/// What a single planned run actually measured. The aggregate is derived
/// from these in Rust — the shell never supplies both a ledger and an
/// unrelated authoritative summary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RunMetrics {
    pub load_ms: u64,
    pub time_to_first_token_ms: u64,
    pub prompt_tokens: u32,
    pub prompt_duration_ms: u64,
    pub generated_tokens: u32,
    pub generation_duration_ms: u64,
    pub peak_rss_mb: u64,
    pub model_memory_mb: u64,
    /// Present only on a Cancellation run.
    pub cancellation_latency_ms: Option<u64>,
    pub peak_temperature_celsius_tenths: u32,
    pub throttled: bool,
    pub battery_drop_tenths_percent: u32,
    pub background_foreground_recovered: bool,
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
    /// The artifact this run loaded, as OBSERVED by the runtime when it
    /// opened the file — not copied from the manifest.
    pub loaded_model_sha256: String,
    /// The model-pack layer's verified inventory digest.
    pub verified_inventory_digest: String,
    /// Evidence that revalidation actually happened BEFORE this run. A known
    /// digest can be copied; this cannot be produced without re-verifying.
    pub revalidation_evidence_id: String,
    /// Which process performed that revalidation.
    pub revalidated_process_instance_id: String,
    pub cold_evidence: ColdEvidence,
    pub start_temperature_celsius_tenths: u32,
    pub cooldown_duration_ms: u64,
    pub cooldown_exit_temperature_celsius_tenths: u32,
    pub thermal_sensor_identity: String,
    pub throttling_detector_identity: String,
    pub disposition: RunDisposition,
    /// v4: what this run measured. The promotion aggregate derives from here.
    pub metrics: RunMetrics,
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
    ///
    /// v6: integer millionths, `0..=FIXED_POINT_SCALE`. As an `f64` this was a
    /// float compared against a float aggregate, and the aggregate drifts: the
    /// two-stage average of ten all-`0.8` axes observes `0.8000000000000002`
    /// (`0.8 * 3 / 3` reproduces it directly). Harmless for display, decisive
    /// at an exact boundary — it is the difference between a tier split and a
    /// promotion.
    pub material_quality_margin_millionths: u32,
}

/// Which outputs are eligible to be scored. Enforced against the corpus, the
/// frozen seeds and the run plan — not merely declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum QualitySelectionRule {
    /// Exactly one scored output per candidate × frozen prompt × frozen seed,
    /// drawn from that seed's `Warm` run and cross-referenced against the
    /// unique matching `run_plan` slot.
    ///
    /// v5 validated the plan for non-emptiness and candidate consistency and
    /// nothing more, so a plan could silently score one prompt twice and
    /// another never, and the equal-prompt macro rule would then be averaging
    /// a set nobody declared.
    ExactlyOncePerCandidatePromptSeedFromWarm,
}

/// How many scorers stand behind a mark, and who. v5 carried
/// `scorer_identity` as a free `String` on each observation and never checked
/// it, so one ledger could mix raters and still average cleanly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ScorerPolicy {
    /// The only form v6 admits. Every observation in both ledgers must carry
    /// this exact digest.
    ///
    /// Multi-scorer semantics are deferred rather than guessed: they need
    /// settled answers on coverage, missing raters, weighting, aggregation
    /// order, disagreement, replacement and recusal, and whether inter-rater
    /// reliability is descriptive or promotion-critical. No study here
    /// requires any of them yet, so specifying them now would be invention.
    SingleScorer { scorer_identity_digest: String },
}

/// The versioned identities of the *evaluator*, as distinct from the protocol
/// it evaluates.
///
/// `protocol_version = 6` alone is insufficient: it says what the data means,
/// not how this build interprets it. Without these, a later evaluator can
/// reinterpret identical v6 data under an unchanged declaration digest —
/// which is precisely what happened between `eb9492c` and `3ccf874`. Each id
/// is an explicit versioned identity carrying its own domain separator, not a
/// hash of the source, so a comment change does not invalidate a study.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EvaluatorPolicyIdentities {
    pub selection_policy_id: String,
    pub quality_aggregation_policy_id: String,
    pub promotion_rule_id: String,
    /// Must equal `FIXED_POINT_SCALE`; a study scored in a different unit is
    /// a different study.
    pub fixed_point_scale: u32,
    /// Must equal `EVALUATOR_SCHEMA_VERSION`.
    pub evaluator_schema_version: u32,
}

/// The five identities this build implements.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn frozen_policy_identities_v6() -> EvaluatorPolicyIdentities {
    EvaluatorPolicyIdentities {
        selection_policy_id: "neuralcompose.selection-policy.v6-1".into(),
        quality_aggregation_policy_id: "neuralcompose.quality-aggregation-policy.v6-1".into(),
        promotion_rule_id: "neuralcompose.promotion-rule.v6-1".into(),
        fixed_point_scale: FIXED_POINT_SCALE,
        evaluator_schema_version: EVALUATOR_SCHEMA_VERSION,
    }
}

/// The frozen v6 declaration: what the comparison means, fixed before any
/// candidate is downloaded.
///
/// Renamed from `EvaluationProtocol` because v6 is no longer only a protocol
/// description — it embeds the corpus VALUE and the evaluator identities, and
/// the name should say that the whole thing is the declaration under which a
/// verdict is made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct V6Declaration {
    pub protocol_version: u32,
    /// v6: the corpus itself, by value. There is no digest parameter here or
    /// anywhere else — identity is derived from this field.
    pub corpus: BenchmarkCorpus,
    /// v6: the exact quota map, hashed, so the effective weighting is part of
    /// what was frozen.
    pub composition_policy: CorpusCompositionPolicy,
    /// A drift guard against `corpus`, kept from v3 for the same reason it
    /// existed then: a summary count that disagrees with the thing it
    /// summarizes is a defect worth failing on.
    pub prompt_count: u32,
    pub quality_rubric_id: String,
    /// v6: exactly one scorer, checked.
    pub scorer_policy: ScorerPolicy,
    /// v6: which outputs are scored, enforced against corpus × seeds × plan.
    pub selection_rule: QualitySelectionRule,
    /// v6: the evaluator's own versioned identities.
    pub policies: EvaluatorPolicyIdentities,
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
    /// v3: the exact run schedule, not merely the alternating property.
    pub run_plan: Vec<RunPlanEntry>,
    /// v5: which outputs are scored, declared rather than inferred.
    pub quality_plan: Vec<QualityPlanEntry>,
    /// v5: the blinding manifest scorers worked under.
    pub blinding_manifest_digest: String,
}

/// Canonical declaration identity. Any change to the corpus, quota map,
/// evaluator policy identities, rubric, sampler, seeds, run counts or
/// thresholds yields a different declaration — so a threshold cannot be
/// relaxed after seeing results while still claiming the same study.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn v6_declaration_identity(declaration: V6Declaration) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        protocol: V6Declaration,
    }
    let mut canonical = declaration;
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

/// How candidates are interleaved. A closed policy, validated against the
/// actual plan — not an assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum OrderPolicy {
    /// Timed runs proceed A, B, B, A per seed so ordering and accumulated
    /// heat cannot favour either candidate.
    CounterbalancedAbba,
}

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
    /// v4: the global ordering policy, validated against the plan itself. A
    /// boolean could be true while the plan ran all of A then all of B.
    pub order_policy: OrderPolicy,
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

/// An exact non-negative rational, in millionths of 1.0:
/// `numerator / denominator`.
///
/// The aggregation divides by seed counts and prompt counts, and those
/// divisions do not land on integers — a third of anything does not. Rounding
/// at each step would reintroduce exactly the drift the fixed-point change
/// exists to remove, so the quotient is carried rather than taken. Always
/// reduced to lowest terms with a nonzero denominator, so equal values are
/// structurally equal and `PartialEq` means what it looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ExactMillionths {
    pub numerator: u64,
    pub denominator: u64,
}

fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

impl ExactMillionths {
    /// Reduce and narrow. `None` only on a zero denominator or a value too
    /// large to represent — neither reachable from a valid panel, and both
    /// refused rather than wrapped.
    fn new(numerator: u128, denominator: u128) -> Option<Self> {
        if denominator == 0 {
            return None;
        }
        let d = gcd(numerator.max(denominator), numerator.min(denominator)).max(1);
        Some(Self {
            numerator: u64::try_from(numerator / d).ok()?,
            denominator: u64::try_from(denominator / d).ok()?,
        })
    }

    /// Exact ordering by cross-multiplication. No division, no epsilon.
    fn cmp_exact(&self, other: &Self) -> std::cmp::Ordering {
        let left = self.numerator as u128 * other.denominator as u128;
        let right = other.numerator as u128 * self.denominator as u128;
        left.cmp(&right)
    }

    /// `|self - other|`, exactly.
    fn abs_difference(&self, other: &Self) -> Option<Self> {
        let (hi, lo) = match self.cmp_exact(other) {
            std::cmp::Ordering::Less => (other, self),
            _ => (self, other),
        };
        let denominator = hi.denominator as u128 * lo.denominator as u128;
        let numerator = hi.numerator as u128 * lo.denominator as u128
            - lo.numerator as u128 * hi.denominator as u128;
        Self::new(numerator, denominator)
    }

    /// A whole number of millionths, for thresholds declared as integers.
    fn from_millionths(v: u32) -> Self {
        Self {
            numerator: u64::from(v),
            denominator: 1,
        }
    }
}

/// Blinded quality scores, one per axis, as integer millionths in
/// `0..=FIXED_POINT_SCALE` (so `800_000` is 0.8). The shell collects them; the
/// rubric that produced them is pinned by `quality_rubric_id`.
///
/// v5 held these as `f64`. A scorer's mark is an ordinal judgement against a
/// rubric — it was never a real number, and representing it as one bought
/// nothing while making the aggregate compare inexactly against an exact
/// margin. `NaN` and `Infinity` cease to be representable at all, which is
/// why the malformed-panel checks below now have only one thing left to test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct QualityPanel {
    pub instruction_adherence: u32,
    pub required_output_structure: u32,
    /// Higher is better: `FIXED_POINT_SCALE` means nothing was invented.
    pub avoids_unsupported_invention: u32,
    pub appropriate_uncertainty: u32,
    /// Higher is better: `FIXED_POINT_SCALE` means no false refusals.
    pub avoids_false_refusal: u32,
    pub substantive_position_retention: u32,
    /// Higher is better: `FIXED_POINT_SCALE` means no degenerate repetition.
    pub avoids_repetition: u32,
    pub truncation_behavior: u32,
    pub language_preservation: u32,
    /// Fidelity to the prompt's declared `PromptContextProfile`.
    pub prompt_profile_fidelity: u32,
}

/// The ten axes in canonical order. One list, used by every axis walk in this
/// module, so an axis cannot be added to the record and quietly skipped by the
/// aggregation.
fn axes_of(p: &QualityPanel) -> [u32; 10] {
    [
        p.instruction_adherence,
        p.required_output_structure,
        p.avoids_unsupported_invention,
        p.appropriate_uncertainty,
        p.avoids_false_refusal,
        p.substantive_position_retention,
        p.avoids_repetition,
        p.truncation_behavior,
        p.language_preservation,
        p.prompt_profile_fidelity,
    ]
}

/// The derived panel. Each axis is an exact rational because averaging across
/// seeds and prompts does not generally land on a whole millionth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DerivedQualityPanel {
    pub instruction_adherence: ExactMillionths,
    pub required_output_structure: ExactMillionths,
    pub avoids_unsupported_invention: ExactMillionths,
    pub appropriate_uncertainty: ExactMillionths,
    pub avoids_false_refusal: ExactMillionths,
    pub substantive_position_retention: ExactMillionths,
    pub avoids_repetition: ExactMillionths,
    pub truncation_behavior: ExactMillionths,
    pub language_preservation: ExactMillionths,
    pub prompt_profile_fidelity: ExactMillionths,
}

fn derived_axes_of(p: &DerivedQualityPanel) -> [ExactMillionths; 10] {
    [
        p.instruction_adherence,
        p.required_output_structure,
        p.avoids_unsupported_invention,
        p.appropriate_uncertainty,
        p.avoids_false_refusal,
        p.substantive_position_retention,
        p.avoids_repetition,
        p.truncation_behavior,
        p.language_preservation,
        p.prompt_profile_fidelity,
    ]
}

/// Mean of the ten raw axes, or `None` if any axis exceeds
/// `FIXED_POINT_SCALE` — a malformed panel scores nothing rather than
/// something.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn quality_score(panel: QualityPanel) -> Option<ExactMillionths> {
    let axes = axes_of(&panel);
    if axes.iter().any(|v| *v > FIXED_POINT_SCALE) {
        return None;
    }
    let sum: u128 = axes.iter().map(|v| u128::from(*v)).sum();
    ExactMillionths::new(sum, axes.len() as u128)
}

/// Mean of the ten derived axes, exactly. Every axis shares one denominator by
/// construction, but this does not assume it.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn derived_quality_score(panel: DerivedQualityPanel) -> Option<ExactMillionths> {
    let axes = derived_axes_of(&panel);
    if axes.iter().any(|a| a.denominator == 0) {
        return None;
    }
    let mut denominator = 1u128;
    for a in axes.iter() {
        let d = u128::from(a.denominator);
        denominator = denominator / gcd(denominator.max(d), denominator.min(d)).max(1) * d;
    }
    let mut numerator = 0u128;
    for a in axes.iter() {
        numerator += u128::from(a.numerator) * (denominator / u128::from(a.denominator));
    }
    ExactMillionths::new(numerator, denominator * axes.len() as u128)
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
    /// The installed artifact size — a pack fact, not a per-run measurement.
    pub installed_bytes: u64,
    /// v5: the raw scored-output ledger. The panel is DERIVED from this.
    pub quality_observations: Vec<PromptQualityObservation>,
    /// v2: why this run is or is not a usable measurement.
    pub disposition: RunDisposition,
    /// v3: one entry per planned run, as observed.
    pub observations: Vec<RunObservation>,
}

/// v3: hashing an invalid declaration does not make it valid. Admission and
/// promotion both run this first.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_v6_declaration(protocol: V6Declaration) -> Vec<String> {
    let mut e = validate_run_environment(protocol.environment.clone());
    let mut bad = |m: &str| e.push(m.to_string());
    if protocol.protocol_version != 6 {
        bad("unsupported protocolVersion (v6 expected)");
    }

    // ---- the evaluator's own identities ----
    // A study is reproducible only if the thing that interpreted it is named.
    let pol = &protocol.policies;
    for (name, id, domain) in [
        (
            "selectionPolicyId",
            &pol.selection_policy_id,
            "neuralcompose.selection-policy.",
        ),
        (
            "qualityAggregationPolicyId",
            &pol.quality_aggregation_policy_id,
            "neuralcompose.quality-aggregation-policy.",
        ),
        (
            "promotionRuleId",
            &pol.promotion_rule_id,
            "neuralcompose.promotion-rule.",
        ),
    ] {
        if !id.starts_with(domain) || id.len() <= domain.len() {
            bad(&format!("{name} must be a versioned id under {domain}"));
        }
    }
    if pol.fixed_point_scale != FIXED_POINT_SCALE {
        bad("fixedPointScale is not the scale this evaluator computes in");
    }
    if pol.evaluator_schema_version != EVALUATOR_SCHEMA_VERSION {
        bad("evaluatorSchemaVersion names a different evaluator");
    }
    match &protocol.scorer_policy {
        ScorerPolicy::SingleScorer {
            scorer_identity_digest,
        } if scorer_identity_digest.trim().is_empty() => {
            bad("scorer policy carries no scorer identity digest")
        }
        _ => {}
    }

    // ---- the corpus, and the quota map that weights it ----
    //
    // Two DIFFERENT questions are asked here, and conflating them is what left
    // a gap in the first pass of this work:
    //
    //   1. Is this corpus the one this build owns?      (authority)
    //   2. Is that corpus well formed?                  (shape)
    //
    // Content-addressing answers neither. It only means an identity cannot be
    // forged — a caller can still submit a DIFFERENT eighteen-prompt corpus
    // that satisfies all nine quotas, derive its identity perfectly honestly,
    // and pass. Nothing is forged and the reviewed corpus is replaced anyway.
    //
    // So the authority check is exact equality against the compiled-in value,
    // and the shape checks stay: they are not redundant, because they are what
    // catches a bad edit to the committed artifact ITSELF, where
    // `frozen_corpus_v1()` returns the bad corpus and equality holds.
    //
    // If a later protocol version needs more than one corpus, add a
    // build-owned registry and check membership. Do not reopen arbitrary
    // corpus injection — the caller must never be the one who says what the
    // study is over.
    let corpus = &protocol.corpus;
    if corpus != &frozen_corpus_v1() {
        bad("corpus is not the build-owned m7b corpus");
    }
    if protocol.composition_policy != frozen_composition_policy_v1() {
        bad("composition policy is not the frozen v6 policy");
    }
    // Subsumes the field checks above. Those stay because they name WHICH
    // field is wrong, and because they are what a registry-based successor
    // would keep when this exact check is relaxed.
    if protocol.policies != frozen_policy_identities_v6() {
        bad("evaluator policy identities do not name this evaluator");
    }
    if corpus.corpus_id.trim().is_empty() {
        bad("corpusId must be non-empty");
    }
    if protocol.quality_rubric_id.trim().is_empty() {
        bad("qualityRubricId must be non-empty");
    }
    let allowed = allowed_task_context_pairs();
    // The matrix is normative. This field records it; it does not choose it,
    // so a declaration cannot widen what it is allowed to contain.
    if protocol.composition_policy.allowed_task_context_pairs != allowed {
        bad("allowedTaskContextPairs is not the normative nine-pair matrix");
    }
    let quotas = &protocol.composition_policy.exact_required_count_per_pair;
    if quotas.len() != allowed.len() {
        bad("exactRequiredCountPerPair must carry one entry per allowed pair");
    }
    let pair_of = |p: &CorpusPrompt| TaskContextPair {
        task_kind: p.task_kind,
        context_profile: p.context_profile,
    };
    for pair in &allowed {
        let matching: Vec<&PairQuota> = quotas.iter().filter(|q| q.pair == *pair).collect();
        if matching.len() != 1 {
            bad(&format!(
                "{:?}x{:?} has {} quota entries, expected exactly 1",
                pair.task_kind,
                pair.context_profile,
                matching.len()
            ));
            continue;
        }
        // An allowed pair contributing nothing is not allowed — it is absent,
        // and saying so as a zero quota would hide it from review.
        if matching[0].exact_count == 0 {
            bad(&format!(
                "{:?}x{:?} declares a zero quota",
                pair.task_kind, pair.context_profile
            ));
        }
        let actual = corpus
            .prompts
            .iter()
            .filter(|p| pair_of(p) == *pair)
            .count();
        if actual != matching[0].exact_count as usize {
            bad(&format!(
                "{:?}x{:?}: corpus has {actual} prompts, quota requires exactly {}",
                pair.task_kind, pair.context_profile, matching[0].exact_count
            ));
        }
    }
    let mut prompt_ids = std::collections::HashSet::new();
    let mut duplicate_prompt = false;
    for p in &corpus.prompts {
        if !prompt_ids.insert(p.prompt_id.clone()) {
            duplicate_prompt = true;
        }
        if !allowed.contains(&pair_of(p)) {
            bad(&format!(
                "prompt {} uses a task x context pair outside the allowed matrix",
                p.prompt_id
            ));
        }
        if p.message.trim().is_empty() || p.prompt_id.trim().is_empty() {
            bad(&format!("prompt {} is blank", p.prompt_id));
        }
    }
    if duplicate_prompt {
        bad("duplicate promptId in the corpus");
    }
    if protocol.seeds.is_empty() {
        bad("at least one seed required");
    }
    let mut seen = std::collections::HashSet::new();
    if protocol.seeds.iter().any(|s| !seen.insert(*s)) {
        bad("duplicate seeds");
    }
    if protocol.prompt_count == 0 || corpus.prompts.is_empty() {
        bad("a corpus with no prompts measures nothing");
    }
    if corpus.prompts.len() as u32 != protocol.prompt_count {
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
    // The plan must EXPAND the declared experiment: the summary counts are
    // drift guards, and a plan that quietly omits seeds, the sustained run or
    // the cancellation test would let an incomplete result be admitted.
    if protocol.timed_runs as usize != protocol.seeds.len() {
        bad("timedRuns must equal the number of frozen seeds");
    }
    let candidates: std::collections::BTreeSet<&String> =
        protocol.run_plan.iter().map(|e| &e.candidate_id).collect();
    if candidates.len() != 2 {
        bad("the run plan must cover exactly two candidates");
    }
    for cand in candidates {
        let mine: Vec<&RunPlanEntry> = protocol
            .run_plan
            .iter()
            .filter(|e| &e.candidate_id == cand)
            .collect();
        let count = |m: RunMode| mine.iter().filter(|e| e.mode == m).count();
        if count(RunMode::Warmup) != protocol.warmup_runs as usize {
            bad("run plan warmup entries disagree with warmupRuns");
        }
        for mode in [RunMode::Cold, RunMode::Warm] {
            if count(mode) != protocol.timed_runs as usize {
                bad("run plan timed entries disagree with timedRuns");
            }
            // Every frozen seed must actually be exercised in each mode.
            let seeds: std::collections::BTreeSet<u64> = mine
                .iter()
                .filter(|e| e.mode == mode)
                .map(|e| e.seed)
                .collect();
            let declared: std::collections::BTreeSet<u64> =
                protocol.seeds.iter().copied().collect();
            if seeds != declared {
                bad("run plan does not exercise every frozen seed in each timed mode");
            }
        }
        if count(RunMode::Sustained) != 1 {
            bad("run plan must contain exactly one sustained run per candidate");
        }
        if count(RunMode::Cancellation) != 1 {
            bad("run plan must contain exactly one cancellation run per candidate");
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
    if t.material_quality_margin_millionths > FIXED_POINT_SCALE {
        bad("materialQualityMargin above 1.0 can never be met");
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

    // ---- v6: selection coverage, ENFORCED ----
    //
    // v5 checked the quality plan for non-emptiness and candidate consistency.
    // That left the plan free to score one prompt twice and another never
    // while still looking well-formed, and the equal-prompt macro rule would
    // then have been averaging a set nobody declared. Coverage is now derived
    // from the corpus and the frozen seeds and compared against the plan.
    match protocol.selection_rule {
        QualitySelectionRule::ExactlyOncePerCandidatePromptSeedFromWarm => {
            for (i, entry) in protocol.quality_plan.iter().enumerate() {
                if entry.index as usize != i {
                    bad("quality plan indices must be dense and ordered");
                    break;
                }
            }
            let mut blinded = std::collections::HashSet::new();
            let mut duplicate_output = false;
            for entry in &protocol.quality_plan {
                if !blinded.insert(entry.blinded_output_id.clone()) {
                    duplicate_output = true;
                }
            }
            if duplicate_output {
                bad("a blindedOutputId appears more than once in the quality plan");
            }
            let seeds: std::collections::BTreeSet<u64> = protocol.seeds.iter().copied().collect();
            let plan_candidates: std::collections::BTreeSet<&String> =
                protocol.run_plan.iter().map(|e| &e.candidate_id).collect();
            // Every cell of candidate x prompt x seed, exactly once. Both a
            // duplicated slot and a missing one fail here.
            for cand in &plan_candidates {
                for p in &corpus.prompts {
                    for seed in &seeds {
                        let n = protocol
                            .quality_plan
                            .iter()
                            .filter(|q| {
                                &&q.candidate_id == cand
                                    && q.prompt_id == p.prompt_id
                                    && q.seed == *seed
                            })
                            .count();
                        if n != 1 {
                            bad(&format!(
                                "{cand} x {} x seed {seed}: {n} scored outputs, expected exactly 1",
                                p.prompt_id
                            ));
                        }
                    }
                }
            }
            for entry in &protocol.quality_plan {
                if !plan_candidates.contains(&entry.candidate_id) {
                    bad(&format!(
                        "scored output {} names a candidate absent from the run plan",
                        entry.blinded_output_id
                    ));
                    continue;
                }
                if !prompt_ids.contains(&entry.prompt_id) {
                    bad(&format!(
                        "scored output {} names a prompt absent from the corpus",
                        entry.blinded_output_id
                    ));
                }
                if !seeds.contains(&entry.seed) {
                    bad(&format!(
                        "scored output {} names a seed that was never frozen",
                        entry.blinded_output_id
                    ));
                }
                // The unique matching run-plan slot. Quality is scored from
                // warm runs only, so the slot is (candidate, Warm, seed) and
                // there must be exactly one of it.
                let slots = protocol
                    .run_plan
                    .iter()
                    .filter(|r| {
                        r.candidate_id == entry.candidate_id
                            && r.mode == RunMode::Warm
                            && r.seed == entry.seed
                    })
                    .count();
                if slots != 1 {
                    bad(&format!(
                        "scored output {} has {slots} matching warm run-plan slots, expected 1",
                        entry.blinded_output_id
                    ));
                }
            }
            // One warm run per (candidate, seed) produces all that seed's
            // outputs, and serves no other slot.
            let mut run_of: std::collections::BTreeMap<(&String, u64), &String> =
                std::collections::BTreeMap::new();
            let mut split_run = false;
            for entry in &protocol.quality_plan {
                match run_of.insert((&entry.candidate_id, entry.seed), &entry.run_id) {
                    Some(prior) if prior != &entry.run_id => split_run = true,
                    _ => {}
                }
            }
            if split_run {
                bad("one candidate/seed draws its scored outputs from more than one run");
            }
            let mut run_ids = std::collections::HashSet::new();
            if run_of.values().any(|r| !run_ids.insert(*r)) {
                bad("one run id serves more than one candidate/seed slot");
            }
        }
    }
    e
}

/// Derive the promotion aggregate as a conservative WORST-CASE envelope.
///
/// These fields are compared against `max_*` ceilings and `min_*` floors, so
/// averaging is the wrong operator: a mean lets one bad run be smoothed away,
/// and with only three seeds a median hides it even harder. A candidate
/// passes only if EVERY qualifying run passes.
///
/// Descriptive statistics (mean, median, spread, pooled throughput) belong in
/// a separate report for comparing typical behaviour — they do not decide
/// whether a hard product ceiling is met.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn derive_cost_observation(
    observations: Vec<RunObservation>,
    installed_bytes: u64,
) -> Option<CostObservation> {
    let timed: Vec<&RunObservation> = observations
        .iter()
        .filter(|o| matches!(o.mode, RunMode::Cold | RunMode::Warm))
        .collect();
    let cold: Vec<&&RunObservation> = timed.iter().filter(|o| o.mode == RunMode::Cold).collect();
    let warm: Vec<&&RunObservation> = timed.iter().filter(|o| o.mode == RunMode::Warm).collect();
    if cold.is_empty() || warm.is_empty() {
        return None;
    }
    let rate = |num: u32, ms: u64| {
        if ms == 0 {
            0.0
        } else {
            num as f64 / (ms as f64 / 1000.0)
        }
    };
    // Throughput floors take the WORST (minimum) observed rate.
    let min_rate = |f: &dyn Fn(&RunObservation) -> f64| -> Option<f64> {
        timed
            .iter()
            .map(|o| f(o))
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a: f64| a.min(v)))
            })
    };
    // Battery comes from the SUSTAINED run alone: summing every warmup, timed
    // and cancellation run would make the figure move with plan length rather
    // than with the model.
    let sustained = observations.iter().find(|o| o.mode == RunMode::Sustained)?;
    // Lifecycle recovery must be exercised, not defaulted onto every run.
    let lifecycle_recovered = sustained.metrics.background_foreground_recovered;
    let cancellation = observations
        .iter()
        .find(|o| o.mode == RunMode::Cancellation)
        .and_then(|o| o.metrics.cancellation_latency_ms)?;
    Some(CostObservation {
        cold_load_ms: cold.iter().map(|o| o.metrics.load_ms).max()?,
        warm_load_ms: warm.iter().map(|o| o.metrics.load_ms).max()?,
        time_to_first_token_ms: timed
            .iter()
            .map(|o| o.metrics.time_to_first_token_ms)
            .max()?,
        prompt_tokens_per_second: min_rate(&|o: &RunObservation| {
            rate(o.metrics.prompt_tokens, o.metrics.prompt_duration_ms)
        })?,
        generation_tokens_per_second: min_rate(&|o: &RunObservation| {
            rate(o.metrics.generated_tokens, o.metrics.generation_duration_ms)
        })?,
        peak_rss_mb: observations.iter().map(|o| o.metrics.peak_rss_mb).max()?,
        model_memory_mb: observations
            .iter()
            .map(|o| o.metrics.model_memory_mb)
            .max()?,
        cancellation_latency_ms: cancellation,
        installed_bytes,
        battery_drop_tenths_percent: sustained.metrics.battery_drop_tenths_percent,
        peak_temperature_celsius_tenths: observations
            .iter()
            .map(|o| o.metrics.peak_temperature_celsius_tenths)
            .max()?,
        thermally_throttled: observations.iter().any(|o| o.metrics.throttled),
        background_foreground_recovered: lifecycle_recovered,
    })
}

/// v5: which output is scored. Declared, never inferred from RunMode —
/// otherwise warmups, cancellation and sustained runs would silently gain
/// scoring weight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct QualityPlanEntry {
    pub index: u32,
    pub candidate_id: String,
    pub run_id: String,
    pub prompt_id: String,
    pub seed: u64,
    pub blinded_output_id: String,
}

/// Why a scored output is or is not usable. Hard-invalid events stay OUTSIDE
/// the average rather than becoming a low score that can be averaged away.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum QualityDisposition {
    Admissible,
    ReasoningLeakage { detector: String },
    ParityFailure,
    MissingOutput,
    TimeoutWithoutScorableContinuation,
    MalformedRequiredStructure,
}

/// One scored output. The hashes describe the assembled UTF-8 continuation
/// AFTER runtime stop-token handling and BEFORE any UI trimming, Markdown
/// rewriting or normalization — produced by the runtime, never supplied by
/// the shell as a claim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PromptQualityObservation {
    pub blinded_output_id: String,
    pub run_id: String,
    pub candidate_id: String,
    pub prompt_id: String,
    pub seed: u64,
    pub semantic_prompt_hash: String,
    pub rendered_prompt_hash: String,
    pub input_token_ids_hash: String,
    pub output_text_sha256: String,
    pub output_token_ids_sha256: String,
    pub rubric_id: String,
    pub blinding_manifest_digest: String,
    /// v6: must equal the digest the declaration's `ScorerPolicy` names. A
    /// free string that nothing compared could mix raters inside one average.
    pub scorer_identity_digest: String,
    pub scores: QualityPanel,
    pub disposition: QualityDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum QualityAdmissionFailure {
    PlanCoverageMismatch {
        reason: String,
    },
    DuplicateObservation {
        blinded_output_id: String,
    },
    WrongCandidate {
        blinded_output_id: String,
    },
    RubricMismatch,
    BlindingManifestMismatch,
    ParityMismatch {
        blinded_output_id: String,
    },
    Inadmissible {
        blinded_output_id: String,
    },
    MalformedScores {
        blinded_output_id: String,
    },
    /// v6: this mark was not made by the scorer the declaration names. Two
    /// distinct digests anywhere in one ledger reach this.
    ScorerIdentityMismatch {
        blinded_output_id: String,
    },
    /// The exact aggregation could not be represented. Unreachable from a
    /// valid panel; refused rather than wrapped.
    InexpressibleAggregate,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum QualityDerivation {
    Derived { panel: DerivedQualityPanel },
    Rejected { failure: QualityAdmissionFailure },
}

/// Derive the panel from the raw ledger under the FROZEN macro weighting:
/// average each axis across seeds within one prompt, then average those
/// prompt-level values equally across prompts. Never weighted by output
/// length, token count, run count or prompt frequency — so a prompt with more
/// observations cannot dominate the candidate score.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn derive_quality_panel(
    protocol: V6Declaration,
    candidate: EvaluationCandidate,
    observations: Vec<PromptQualityObservation>,
) -> QualityDerivation {
    let reject = |failure| QualityDerivation::Rejected { failure };
    let expected = expected_prompts(&protocol.corpus);
    let ScorerPolicy::SingleScorer {
        scorer_identity_digest: declared_scorer,
    } = &protocol.scorer_policy;
    let planned: Vec<&QualityPlanEntry> = protocol
        .quality_plan
        .iter()
        .filter(|e| e.candidate_id == candidate.candidate_id)
        .collect();
    if observations.len() != planned.len() {
        return reject(QualityAdmissionFailure::PlanCoverageMismatch {
            reason: format!(
                "expected {} outputs, got {}",
                planned.len(),
                observations.len()
            ),
        });
    }
    let mut seen = std::collections::HashSet::new();
    // prompt_id -> (number of seeds scored, per-axis sums across those seeds)
    let mut per_prompt: std::collections::BTreeMap<String, (u128, [u128; 10])> =
        std::collections::BTreeMap::new();

    for entry in &planned {
        let obs = match observations
            .iter()
            .find(|o| o.blinded_output_id == entry.blinded_output_id)
        {
            None => {
                return reject(QualityAdmissionFailure::PlanCoverageMismatch {
                    reason: format!("no output for {}", entry.blinded_output_id),
                })
            }
            Some(o) => o,
        };
        if !seen.insert(obs.blinded_output_id.clone()) {
            return reject(QualityAdmissionFailure::DuplicateObservation {
                blinded_output_id: obs.blinded_output_id.clone(),
            });
        }
        // The scored output must be the one the plan named, for this
        // candidate, prompt, seed and run.
        if obs.candidate_id != candidate.candidate_id {
            return reject(QualityAdmissionFailure::WrongCandidate {
                blinded_output_id: obs.blinded_output_id.clone(),
            });
        }
        if obs.prompt_id != entry.prompt_id || obs.seed != entry.seed || obs.run_id != entry.run_id
        {
            return reject(QualityAdmissionFailure::PlanCoverageMismatch {
                reason: format!("{} does not match its plan entry", obs.blinded_output_id),
            });
        }
        if obs.rubric_id != protocol.quality_rubric_id {
            return reject(QualityAdmissionFailure::RubricMismatch);
        }
        if obs.blinding_manifest_digest != protocol.blinding_manifest_digest {
            return reject(QualityAdmissionFailure::BlindingManifestMismatch);
        }
        // Every mark in this ledger must come from the one declared scorer.
        // Comparing each against the declaration, rather than against its
        // neighbours, rejects a uniformly-wrong ledger too.
        if &obs.scorer_identity_digest != declared_scorer {
            return reject(QualityAdmissionFailure::ScorerIdentityMismatch {
                blinded_output_id: obs.blinded_output_id.clone(),
            });
        }
        // Parity: the scored output must come from the frozen prompt as this
        // candidate renders it — swapping hashes between candidates,
        // prompts, seeds or runs fails here.
        let expected_semantic = expected
            .iter()
            .find(|e| e.prompt_id == obs.prompt_id)
            .map(|e| e.semantic_prompt_hash.clone());
        let binding = candidate
            .prompt_bindings
            .iter()
            .find(|b| b.prompt_id == obs.prompt_id);
        match (expected_semantic, binding) {
            (Some(sem), Some(b))
                if sem == obs.semantic_prompt_hash
                    && b.rendered_prompt_hash == obs.rendered_prompt_hash
                    && b.input_token_ids_hash == obs.input_token_ids_hash => {}
            _ => {
                return reject(QualityAdmissionFailure::ParityMismatch {
                    blinded_output_id: obs.blinded_output_id.clone(),
                })
            }
        }
        if obs.disposition != QualityDisposition::Admissible {
            return reject(QualityAdmissionFailure::Inadmissible {
                blinded_output_id: obs.blinded_output_id.clone(),
            });
        }
        let axes = axes_of(&obs.scores);
        if axes.iter().any(|v| *v > FIXED_POINT_SCALE) {
            return reject(QualityAdmissionFailure::MalformedScores {
                blinded_output_id: obs.blinded_output_id.clone(),
            });
        }
        let slot = per_prompt
            .entry(obs.prompt_id.clone())
            .or_insert((0, [0; 10]));
        slot.0 += 1;
        for (i, v) in axes.iter().enumerate() {
            slot.1[i] += u128::from(*v);
        }
    }
    if per_prompt.is_empty() {
        return reject(QualityAdmissionFailure::PlanCoverageMismatch {
            reason: "no prompts scored".into(),
        });
    }
    // Seeds within a prompt, then prompts equally — the frozen macro
    // weighting, computed exactly.
    //
    //   axis = (1/P) * SUM_p ( S_p / n_p )
    //
    // Taking each `S_p / n_p` as a quotient would round P times before the
    // final division. Instead every term is put over the common denominator
    // `L = lcm(n_p)` and the whole thing becomes one exact fraction with
    // denominator `P * L` — no rounding anywhere on the path to a verdict.
    let mut l = 1u128;
    for (count, _) in per_prompt.values() {
        l = l / gcd(l.max(*count), l.min(*count)).max(1) * *count;
    }
    let p_count = per_prompt.len() as u128;
    let mut axis = [ExactMillionths {
        numerator: 0,
        denominator: 1,
    }; 10];
    for (i, slot) in axis.iter_mut().enumerate() {
        let mut numerator = 0u128;
        for (count, sums) in per_prompt.values() {
            numerator += sums[i] * (l / count);
        }
        match ExactMillionths::new(numerator, p_count * l) {
            Some(v) => *slot = v,
            None => return reject(QualityAdmissionFailure::InexpressibleAggregate),
        }
    }
    QualityDerivation::Derived {
        panel: DerivedQualityPanel {
            instruction_adherence: axis[0],
            required_output_structure: axis[1],
            avoids_unsupported_invention: axis[2],
            appropriate_uncertainty: axis[3],
            avoids_false_refusal: axis[4],
            substantive_position_retention: axis[5],
            avoids_repetition: axis[6],
            truncation_behavior: axis[7],
            language_preservation: axis[8],
            prompt_profile_fidelity: axis[9],
        },
    }
}

// ---------- admission and promotion ----------

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AdmissionFailure {
    ProtocolMismatch,
    ProtocolInvalid {
        reason: String,
    },
    CandidateIdentityMismatch,
    ResultCandidateMismatch,
    PromptCountMismatch,
    DuplicatePromptId {
        prompt_id: String,
    },
    UnknownPromptId {
        prompt_id: String,
    },
    /// v6: the result labelled a prompt with a different task kind or context
    /// profile than the frozen corpus gives it. Relabelling changes which
    /// benchmark item was answered.
    PromptTaxonomyMismatch {
        prompt_id: String,
    },
    RenderedPromptMismatch {
        prompt_id: String,
    },
    InputTokenMismatch {
        prompt_id: String,
    },
    RunLedgerMismatch {
        reason: String,
    },
    ColdEvidenceMissing {
        run_id: String,
    },
    EnvironmentDeviation {
        reason: String,
    },
    MissingConversionRecord,
    UnexpectedConversionRecord,
    RequantizationNotAllowed,
    ThinkingModeNotDisabled,
    TokenizerIdentityMismatch,
    ChatTemplateIdentityMismatch,
    SemanticPromptMismatch,
    MalformedQualityPanel,
    QualityLedgerRejected {
        reason: String,
    },
    ThermallyThrottled,
    Disqualified {
        reason: String,
    },
}

/// May this result enter the comparison at all? Admission is separate from
/// winning: a result that fails here is not a bad score, it is not evidence.
/// Callers should not need to remember to call this — `evaluate_promotion`
/// runs it internally, and there is no promotion path that skips it.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn admit_result(
    result: CandidateResult,
    candidate: EvaluationCandidate,
    protocol: V6Declaration,
) -> Option<AdmissionFailure> {
    let protocol_errs = validate_v6_declaration(protocol.clone());
    if !protocol_errs.is_empty() {
        return Some(AdmissionFailure::ProtocolInvalid {
            reason: protocol_errs.join("; "),
        });
    }
    let expected = expected_prompts(&protocol.corpus);
    if result.disposition != RunDisposition::Admissible {
        return Some(AdmissionFailure::Disqualified {
            reason: format!("{:?}", result.disposition),
        });
    }
    if result.protocol_identity != v6_declaration_identity(protocol.clone()) {
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
    if result.prompts.len() != expected.len() {
        return Some(AdmissionFailure::PromptCountMismatch);
    }
    let mut seen = std::collections::HashSet::new();
    for p in &result.prompts {
        if !seen.insert(p.prompt_id.clone()) {
            return Some(AdmissionFailure::DuplicatePromptId {
                prompt_id: p.prompt_id.clone(),
            });
        }
        let frozen = match protocol
            .corpus
            .prompts
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
        // v6: the taxonomy the result claims must be the corpus's. Both axes
        // are hashed into the semantic identity, so a relabelling also fails
        // the check below — this reports the actual cause rather than leaving
        // it as an opaque hash mismatch.
        if p.task_kind != frozen.task_kind || p.context_profile != frozen.context_profile {
            return Some(AdmissionFailure::PromptTaxonomyMismatch {
                prompt_id: p.prompt_id.clone(),
            });
        }
        let expected_prompt = match expected.iter().find(|e| e.prompt_id == p.prompt_id) {
            None => {
                return Some(AdmissionFailure::UnknownPromptId {
                    prompt_id: p.prompt_id.clone(),
                })
            }
            Some(e) => e,
        };
        if p.semantic_prompt_hash != expected_prompt.semantic_prompt_hash {
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
    let mut run_ids = std::collections::HashSet::new();
    let mut seen_processes = std::collections::HashSet::new();
    let mut revalidations = std::collections::HashSet::new();
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
        // Identity: what the runtime actually opened must be this candidate.
        if obs.loaded_model_sha256 != candidate.artifact_sha256_hex {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!(
                    "run {} loaded a different artifact than this candidate",
                    entry.index
                ),
            });
        }
        // Freshness: a copied digest proves identity, never revalidation.
        if obs.revalidation_evidence_id.trim().is_empty()
            || obs.verified_inventory_digest.trim().is_empty()
        {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} has no fresh revalidation evidence", entry.index),
            });
        }
        if !revalidations.insert(obs.revalidation_evidence_id.clone()) {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} reused an earlier revalidation", entry.index),
            });
        }
        if obs.revalidated_process_instance_id != obs.process_instance_id {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} was revalidated by a different process", entry.index),
            });
        }
        if obs.run_id.trim().is_empty() || !run_ids.insert(obs.run_id.clone()) {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} has a blank or duplicate run id", entry.index),
            });
        }
        if obs.process_instance_id.trim().is_empty() {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} has no process instance id", entry.index),
            });
        }
        // Cold evidence must actually describe THIS process.
        if let ColdEvidence::ProcessCold {
            process_instance_id,
        } = &obs.cold_evidence
        {
            if process_instance_id != &obs.process_instance_id {
                return Some(AdmissionFailure::ColdEvidenceMissing {
                    run_id: obs.run_id.clone(),
                });
            }
        }
        // Warm means warm: the frozen definition constrains what may be
        // reused, and a "warm" run in a brand-new process is not warm.
        if obs.mode == RunMode::Warm {
            match protocol.environment.warm_definition {
                WarmDefinition::ContextRecreatedModelResident | WarmDefinition::ContextReused => {
                    if !seen_processes.contains(&obs.process_instance_id) {
                        return Some(AdmissionFailure::EnvironmentDeviation {
                            reason: format!(
                                "run {}: warm run in a process with no prior run",
                                entry.index
                            ),
                        });
                    }
                }
            }
        }
        // A required restart must be DEMONSTRATED by a new process instance,
        // not asserted by a protocol boolean.
        if obs.mode == RunMode::Cold
            && protocol.environment.restart_process_between_candidates
            && seen_processes.contains(&obs.process_instance_id)
        {
            return Some(AdmissionFailure::EnvironmentDeviation {
                reason: format!(
                    "run {}: cold run reused a process instance from an earlier run",
                    entry.index
                ),
            });
        }
        seen_processes.insert(obs.process_instance_id.clone());
        // A cancellation run must actually have measured a cancellation.
        if obs.mode == RunMode::Cancellation && obs.metrics.cancellation_latency_ms.is_none() {
            return Some(AdmissionFailure::RunLedgerMismatch {
                reason: format!("run {} is a cancellation run with no latency", entry.index),
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
        } else if obs.observed_brightness_percent != env.screen_brightness_percent {
            Some("screen brightness")
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

    if result.observations.iter().any(|o| o.metrics.throttled) {
        return Some(AdmissionFailure::ThermallyThrottled);
    }
    // v5.1: every scored output must name a run that ACTUALLY HAPPENED, with
    // the right seed, in a scoring-eligible mode. derive_quality_panel only
    // sees the quality ledger, so a plan naming a nonexistent run — or
    // selecting a warmup, sustained or cancellation output — is only
    // detectable here, where both ledgers are in scope.
    for q in &result.quality_observations {
        let run = match result.observations.iter().find(|o| o.run_id == q.run_id) {
            None => {
                return Some(AdmissionFailure::QualityLedgerRejected {
                    reason: format!(
                        "scored output {} names a run that never executed",
                        q.blinded_output_id
                    ),
                })
            }
            Some(o) => o,
        };
        if run.seed != q.seed {
            return Some(AdmissionFailure::QualityLedgerRejected {
                reason: format!(
                    "scored output {} claims a seed the run did not use",
                    q.blinded_output_id
                ),
            });
        }
        if run.candidate_id != candidate.candidate_id {
            return Some(AdmissionFailure::QualityLedgerRejected {
                reason: format!(
                    "scored output {} names another candidate's run",
                    q.blinded_output_id
                ),
            });
        }
        // Scoring eligibility comes from the run's mode, never from its
        // presence in the plan: warmups, cold, sustained and cancellation
        // runs must not acquire scoring weight.
        if run.mode != RunMode::Warm {
            return Some(AdmissionFailure::QualityLedgerRejected {
                reason: format!(
                    "scored output {} came from a {:?} run, which is not scoring-eligible",
                    q.blinded_output_id, run.mode
                ),
            });
        }
        if run.disposition != RunDisposition::Admissible {
            return Some(AdmissionFailure::QualityLedgerRejected {
                reason: format!(
                    "scored output {} came from an inadmissible run",
                    q.blinded_output_id
                ),
            });
        }
    }
    // The panel is derived, never supplied — so admission checks that the
    // raw ledger yields one at all.
    if let QualityDerivation::Rejected { failure } = derive_quality_panel(
        protocol.clone(),
        candidate.clone(),
        result.quality_observations.clone(),
    ) {
        return Some(AdmissionFailure::QualityLedgerRejected {
            reason: format!("{failure:?}"),
        });
    }
    // The aggregate must be derivable from the ledger; if it is not, there is
    // nothing legitimate to evaluate thresholds against.
    if derive_cost_observation(result.observations.clone(), 0).is_none() {
        return Some(AdmissionFailure::RunLedgerMismatch {
            reason: "ledger does not yield a derivable aggregate".into(),
        });
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
        quality_score: ExactMillionths,
        margin: ExactMillionths,
        /// Which evaluator produced this. `protocol_version` alone does not
        /// say how the data was interpreted.
        policies: EvaluatorPolicyIdentities,
    },
    /// Both are admissible and both pass, but neither is materially better —
    /// a split (Basic / Enhanced) is the honest outcome.
    SplitTiers {
        basic_candidate_id: String,
        enhanced_candidate_id: String,
        policies: EvaluatorPolicyIdentities,
    },
    /// Nothing is promoted. This is a legitimate result, not a failure to
    /// decide.
    PromoteNothing {
        reason: String,
        policies: EvaluatorPolicyIdentities,
    },
}

/// Cross-candidate checks that cannot live inside per-candidate admission:
/// the global run ORDER, and process-restart evidence spanning both
/// candidates. `admit_result` rebuilds its process set per candidate, so a
/// restart between A and B is only provable here.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_global_ledger(
    protocol: V6Declaration,
    result_a: CandidateResult,
    result_b: CandidateResult,
) -> Vec<String> {
    let mut errs = Vec::new();
    let mut all: Vec<&RunObservation> = result_a
        .observations
        .iter()
        .chain(result_b.observations.iter())
        .collect();
    all.sort_by_key(|o| o.sequence_index);

    // Indices must be the plan's, once each.
    let planned: Vec<u32> = protocol.run_plan.iter().map(|e| e.index).collect();
    let observed: Vec<u32> = all.iter().map(|o| o.sequence_index).collect();
    if planned != observed {
        errs.push("combined ledger does not cover the frozen plan exactly once".into());
    }

    match protocol.environment.order_policy {
        OrderPolicy::CounterbalancedAbba => {
            // Within each seed, the timed runs must read A, B, B, A.
            let timed: Vec<&&RunObservation> = all
                .iter()
                .filter(|o| matches!(o.mode, RunMode::Cold | RunMode::Warm))
                .collect();
            for chunk in timed.chunks(4) {
                if chunk.len() < 4 {
                    continue;
                }
                let ids: Vec<&String> = chunk.iter().map(|o| &o.candidate_id).collect();
                if !(ids[0] == ids[3] && ids[1] == ids[2] && ids[0] != ids[1]) {
                    errs.push("timed runs are not counterbalanced ABBA".into());
                    break;
                }
            }
        }
    }

    // A process instance may not span both candidates.
    let a_procs: std::collections::HashSet<&String> = result_a
        .observations
        .iter()
        .map(|o| &o.process_instance_id)
        .collect();
    if result_b
        .observations
        .iter()
        .any(|o| a_procs.contains(&o.process_instance_id))
    {
        errs.push("a process instance was shared across candidates".into());
    }
    errs
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
    protocol: V6Declaration,
    candidate_a: EvaluationCandidate,
    result_a: CandidateResult,
    candidate_b: EvaluationCandidate,
    result_b: CandidateResult,
) -> PromotionVerdict {
    let policies = protocol.policies.clone();
    let nothing = |reason: String| PromotionVerdict::PromoteNothing {
        reason,
        policies: policies.clone(),
    };

    let protocol_errs = validate_v6_declaration(protocol.clone());
    if !protocol_errs.is_empty() {
        return nothing(format!("protocol invalid: {}", protocol_errs.join("; ")));
    }
    if let Some(f) = admit_result(result_a.clone(), candidate_a.clone(), protocol.clone()) {
        return nothing(format!("candidate A inadmissible: {f:?}"));
    }
    if let Some(f) = admit_result(result_b.clone(), candidate_b.clone(), protocol.clone()) {
        return nothing(format!("candidate B inadmissible: {f:?}"));
    }
    let global = validate_global_ledger(protocol.clone(), result_a.clone(), result_b.clone());
    if !global.is_empty() {
        return nothing(format!("global ledger invalid: {}", global.join("; ")));
    }

    // Both aggregates are derived here; neither is accepted from the caller.
    let panel_of = |cand: &EvaluationCandidate, r: &CandidateResult| match derive_quality_panel(
        protocol.clone(),
        cand.clone(),
        r.quality_observations.clone(),
    ) {
        QualityDerivation::Derived { panel } => derived_quality_score(panel),
        QualityDerivation::Rejected { .. } => None,
    };
    let (sa, sb) = match (
        panel_of(&candidate_a, &result_a),
        panel_of(&candidate_b, &result_b),
    ) {
        (Some(x), Some(y)) => (x, y),
        _ => return nothing("a quality ledger did not yield a panel".into()),
    };
    let thresholds = protocol.thresholds.clone();
    // The aggregate is DERIVED from each ledger, never accepted from the
    // caller — otherwise a valid-looking ledger could carry favourable
    // unrelated numbers.
    let (cost_a, cost_b) = match (
        derive_cost_observation(result_a.observations.clone(), result_a.installed_bytes),
        derive_cost_observation(result_b.observations.clone(), result_b.installed_bytes),
    ) {
        (Some(a), Some(b)) => (a, b),
        _ => return nothing("a ledger did not yield a derivable aggregate".into()),
    };
    let fa = meets_thresholds(cost_a.clone(), thresholds.clone());
    let fb = meets_thresholds(cost_b.clone(), thresholds.clone());

    // The margin is a difference of two exact rationals and the threshold is
    // a whole number of millionths; the comparison is by cross-multiplication,
    // so there is no epsilon and no representation error to absorb.
    let margin = match sa.abs_difference(&sb) {
        Some(m) => m,
        None => return nothing("the quality margin is not exactly representable".into()),
    };
    match (fa.is_empty(), fb.is_empty()) {
        (false, false) => nothing("neither candidate met the frozen ceilings".into()),
        (true, false) => PromotionVerdict::Promote {
            candidate_id: result_a.candidate_id,
            quality_score: sa,
            margin,
            policies,
        },
        (false, true) => PromotionVerdict::Promote {
            candidate_id: result_b.candidate_id,
            quality_score: sb,
            margin,
            policies,
        },
        (true, true) => {
            let required =
                ExactMillionths::from_millionths(thresholds.material_quality_margin_millionths);
            if margin.cmp_exact(&required) == std::cmp::Ordering::Less {
                let (basic, enhanced) = if cost_a.installed_bytes <= cost_b.installed_bytes {
                    (result_a.candidate_id, result_b.candidate_id)
                } else {
                    (result_b.candidate_id, result_a.candidate_id)
                };
                PromotionVerdict::SplitTiers {
                    basic_candidate_id: basic,
                    enhanced_candidate_id: enhanced,
                    policies,
                }
            } else if sa.cmp_exact(&sb) == std::cmp::Ordering::Greater {
                PromotionVerdict::Promote {
                    candidate_id: result_a.candidate_id,
                    quality_score: sa,
                    margin,
                    policies,
                }
            } else {
                PromotionVerdict::Promote {
                    candidate_id: result_b.candidate_id,
                    quality_score: sb,
                    margin,
                    policies,
                }
            }
        }
    }
}
