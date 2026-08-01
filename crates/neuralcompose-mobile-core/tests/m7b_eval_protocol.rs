// M7-B frozen benchmark declaration regressions (v6). Freezing exists so a
// threshold cannot move after a result is seen, an inadmissible run cannot be
// promoted, and a run taken under other conditions cannot borrow the
// declaration's hash. Both polarities throughout.
//
// v6 adds the corpus itself to what is frozen. These helpers build against
// `frozen_corpus_v1()` — the same committed artifact the crate compiles in —
// so no test here can be green against a corpus that does not exist, which is
// how the v5 fixtures managed to look convincing.

use neuralcompose_mobile_core::generation_eval::*;
use neuralcompose_mobile_core::sha256_hex;

const A: &str = "qwen2.5-0.5b-instruct-q4km";
const B: &str = "qwen3-0.6b-q4km-derived";
const SEEDS: [u64; 3] = [11, 22, 33];

fn sha(n: u8) -> String {
    format!("{:02x}", n).repeat(32)
}

/// v6: the corpus is not a fixture any more. These helpers read the same
/// committed artifact the crate compiles in, so a test cannot be green against
/// a corpus that does not exist.
fn corpus() -> BenchmarkCorpus {
    frozen_corpus_v1()
}

fn scorer_digest() -> String {
    sha(0xf0)
}

fn semantic_of(prompt_id: &str) -> String {
    expected_prompts(&corpus())
        .into_iter()
        .find(|e| e.prompt_id == prompt_id)
        .expect("prompt is in the frozen corpus")
        .semantic_prompt_hash
}

fn sampler() -> SamplerConfig {
    SamplerConfig {
        temperature: 0.7,
        top_p: 0.8,
        top_k: 20,
        repeat_penalty: 1.05,
        max_output_tokens: 256,
        context_cap: 4096,
    }
}

fn thresholds() -> PromotionThresholds {
    PromotionThresholds {
        max_cold_load_ms: 4000,
        max_time_to_first_token_ms: 1500,
        min_generation_tokens_per_second: 8.0,
        max_peak_rss_mb: 1600,
        max_installed_bytes: 1_200_000_000,
        max_cancellation_latency_ms: 300,
        // The frozen protocol runs plugged in, so battery is telemetry only.
        battery_policy: BatteryEvidencePolicy::TelemetryOnlyPluggedIn,
        thermal_cutoff_celsius_tenths: 450,
        material_quality_margin_millionths: 50_000,
    }
}

fn environment() -> RunEnvironment {
    RunEnvironment {
        // Android gives an ordinary app no honest way to drop the page cache.
        cold_definition: ColdDefinition::ProcessColdPageCacheUnknown,
        warm_definition: WarmDefinition::ContextRecreatedModelResident,
        charging_state: ChargingState::PluggedIn,
        cooldown_exit: CooldownExit::TemperatureAtOrBelowStartCeiling,
        cooldown_minimum_seconds: 180,
        thermal_start_ceiling_celsius_tenths: 380,
        screen_on: true,
        screen_brightness_percent: 20,
        airplane_mode: true,
        restart_process_between_candidates: true,
        recheck_pack_integrity_between_candidates: true,
        order_policy: OrderPolicy::CounterbalancedAbba,
    }
}

/// The promised sequence, not merely "alternating": A cold, B cold, B warm,
/// A warm.
fn run_plan() -> Vec<RunPlanEntry> {
    // The plan EXPANDS the declaration: one warmup, every frozen seed in each
    // timed mode, one sustained and one cancellation run per candidate, with
    // candidates alternating.
    let mut plan = Vec::new();
    let mut idx = 0u32;
    let push = |plan: &mut Vec<RunPlanEntry>, cand: &str, mode: RunMode, seed: u64, i: &mut u32| {
        plan.push(RunPlanEntry {
            index: *i,
            candidate_id: cand.into(),
            mode,
            seed,
        });
        *i += 1;
    };
    for cand in [A, B] {
        push(&mut plan, cand, RunMode::Warmup, 11, &mut idx);
    }
    for seed in SEEDS {
        push(&mut plan, A, RunMode::Cold, seed, &mut idx);
        push(&mut plan, B, RunMode::Cold, seed, &mut idx);
        push(&mut plan, B, RunMode::Warm, seed, &mut idx);
        push(&mut plan, A, RunMode::Warm, seed, &mut idx);
    }
    for cand in [A, B] {
        push(&mut plan, cand, RunMode::Sustained, 11, &mut idx);
        push(&mut plan, cand, RunMode::Cancellation, 11, &mut idx);
    }
    plan
}

fn metrics(mode: RunMode) -> RunMetrics {
    RunMetrics {
        load_ms: if mode == RunMode::Cold { 2200 } else { 400 },
        time_to_first_token_ms: 700,
        prompt_tokens: 120,
        prompt_duration_ms: 1000,
        generated_tokens: 140,
        generation_duration_ms: 10_000,
        peak_rss_mb: 900,
        model_memory_mb: 500,
        cancellation_latency_ms: if mode == RunMode::Cancellation {
            Some(120)
        } else {
            None
        },
        peak_temperature_celsius_tenths: 400,
        throttled: false,
        battery_drop_tenths_percent: 2,
        background_foreground_recovered: true,
    }
}

fn protocol() -> V6Declaration {
    V6Declaration {
        protocol_version: 6,
        corpus: corpus(),
        composition_policy: frozen_composition_policy_v1(),
        prompt_count: corpus().prompts.len() as u32,
        quality_rubric_id: "m7b-rubric-v1".into(),
        scorer_policy: ScorerPolicy::SingleScorer {
            scorer_identity_digest: scorer_digest(),
        },
        selection_rule: QualitySelectionRule::ExactlyOncePerCandidatePromptSeedFromWarm,
        policies: frozen_policy_identities_v6(),
        sampler: sampler(),
        seeds: SEEDS.to_vec(),
        warmup_runs: 1,
        timed_runs: 3,
        sustained_seconds: 300,
        per_run_timeout_ms: 60_000,
        thresholds: thresholds(),
        environment: environment(),
        run_plan: run_plan(),
        quality_plan: quality_plan(),
        blinding_manifest_digest: sha(0xbd),
    }
}

/// Exactly one scored output per candidate x frozen prompt x frozen seed,
/// taken from that seed's timed warm run. v6 no longer takes this on trust:
/// the same coverage is derived from the corpus and the frozen seeds and
/// compared against whatever the plan actually says.
fn quality_plan() -> Vec<QualityPlanEntry> {
    let plan = run_plan();
    let mut out = Vec::new();
    let mut idx = 0u32;
    for cand in [A, B] {
        for seed in SEEDS {
            let run = plan
                .iter()
                .find(|e| e.candidate_id == cand && e.mode == RunMode::Warm && e.seed == seed)
                .expect("one warm run per candidate and seed");
            for p in &corpus().prompts {
                out.push(QualityPlanEntry {
                    index: idx,
                    candidate_id: cand.into(),
                    run_id: format!("{cand}-{}", run.index),
                    prompt_id: p.prompt_id.clone(),
                    seed,
                    blinded_output_id: format!("out-{idx}"),
                });
                idx += 1;
            }
        }
    }
    out
}

fn quality_observations(id: &str, score: u32) -> Vec<PromptQualityObservation> {
    let cand = candidate(id);
    quality_plan()
        .into_iter()
        .filter(|e| e.candidate_id == id)
        .map(|e| {
            let b = cand
                .prompt_bindings
                .iter()
                .find(|b| b.prompt_id == e.prompt_id)
                .expect("a binding per frozen prompt");
            PromptQualityObservation {
                blinded_output_id: e.blinded_output_id.clone(),
                run_id: e.run_id.clone(),
                candidate_id: id.into(),
                prompt_id: e.prompt_id.clone(),
                seed: e.seed,
                semantic_prompt_hash: semantic_of(&e.prompt_id),
                rendered_prompt_hash: b.rendered_prompt_hash.clone(),
                input_token_ids_hash: b.input_token_ids_hash.clone(),
                output_text_sha256: sha256_hex(
                    format!("text-{}", e.blinded_output_id).into_bytes(),
                ),
                output_token_ids_sha256: sha256_hex(
                    format!("tokens-{}", e.blinded_output_id).into_bytes(),
                ),
                rubric_id: "m7b-rubric-v1".into(),
                blinding_manifest_digest: sha(0xbd),
                scorer_identity_digest: scorer_digest(),
                scores: panel(score),
                disposition: QualityDisposition::Admissible,
            }
        })
        .collect()
}

fn conversion() -> ConversionRecord {
    ConversionRecord {
        source_repo: "Qwen/Qwen3-0.6B".into(),
        source_revision: "c1899de2".into(),
        conversion_commit: "f5b9bd39".into(),
        quantizer_commit: "f5b9bd39".into(),
        conversion_command: "convert_hf_to_gguf.py".into(),
        quantize_command: "llama-quantize Q4_K_M".into(),
        intermediate_sha256_hex: sha(0xb1),
        output_sha256_hex: sha(0xb2),
        quantization: "Q4_K_M".into(),
        importance_matrix: None,
        calibration_dataset: None,
        allow_requantize: false,
        pure_quantization: false,
        source_precision: "bf16".into(),
    }
}

/// One binding per frozen prompt. Rendered bytes and token ids legitimately
/// differ per candidate because the chat templates differ; distinct hashes per
/// (candidate, prompt) is what makes a swap between them detectable.
fn bindings(id: &str) -> Vec<PromptBinding> {
    corpus()
        .prompts
        .iter()
        .map(|p| PromptBinding {
            prompt_id: p.prompt_id.clone(),
            rendered_prompt_hash: sha256_hex(format!("rendered-{id}-{}", p.prompt_id).into_bytes()),
            input_token_ids_hash: sha256_hex(format!("tokens-{id}-{}", p.prompt_id).into_bytes()),
        })
        .collect()
}

fn candidate(id: &str) -> EvaluationCandidate {
    let derived = id == B;
    EvaluationCandidate {
        candidate_id: id.into(),
        model_family: if derived { "qwen3" } else { "qwen2.5" }.into(),
        model_revision: if derived { "0.6b" } else { "0.5b-instruct" }.into(),
        quantization: "Q4_K_M".into(),
        variant_id: "gguf-q4km-android-arm64".into(),
        role: CandidateRole::PrimaryMobile,
        provenance: if derived {
            // Qwen3 publishes no official Q4_K_M, so B is converted.
            ArtifactProvenance::DerivedByConversion
        } else {
            ArtifactProvenance::OfficialUpstream
        },
        artifact_sha256_hex: sha(if derived { 0xc2 } else { 0xc1 }),
        tokenizer_identity: sha(if derived { 0xd2 } else { 0xd1 }),
        chat_template_identity: sha(if derived { 0xe2 } else { 0xe1 }),
        thinking_mode_disabled: true,
        conversion: if derived { Some(conversion()) } else { None },
        prompt_bindings: bindings(id),
    }
}

/// `score` is integer millionths: 800_000 is 0.8.
fn panel(score: u32) -> QualityPanel {
    QualityPanel {
        instruction_adherence: score,
        required_output_structure: score,
        avoids_unsupported_invention: score,
        appropriate_uncertainty: score,
        avoids_false_refusal: score,
        substantive_position_retention: score,
        avoids_repetition: score,
        truncation_behavior: score,
        language_preservation: score,
        prompt_profile_fidelity: score,
    }
}

fn cost() -> CostObservation {
    CostObservation {
        cold_load_ms: 2200,
        warm_load_ms: 400,
        time_to_first_token_ms: 700,
        prompt_tokens_per_second: 120.0,
        generation_tokens_per_second: 14.0,
        peak_rss_mb: 900,
        model_memory_mb: 500,
        cancellation_latency_ms: 120,
        installed_bytes: 400_000_000,
        battery_drop_tenths_percent: 30,
        peak_temperature_celsius_tenths: 400,
        thermally_throttled: false,
        background_foreground_recovered: true,
    }
}

fn observation(idx: u32, cand: &str, mode: RunMode, seed: u64) -> RunObservation {
    // A cold run must be a NEW process; warm runs continue the one before.
    let proc = format!(
        "{cand}-proc-{}",
        if mode == RunMode::Cold { idx } else { 0 }
    );
    RunObservation {
        run_id: format!("{cand}-{idx}"),
        candidate_id: cand.into(),
        seed,
        mode,
        sequence_index: idx,
        started_monotonic_ms: 1000,
        ended_monotonic_ms: 2000,
        observed_charging_state: ChargingState::PluggedIn,
        observed_screen_on: true,
        observed_brightness_percent: 20,
        observed_airplane_mode: true,
        process_instance_id: proc.clone(),
        loaded_model_sha256: candidate(cand).artifact_sha256_hex,
        verified_inventory_digest: sha(0x6b),
        revalidation_evidence_id: format!("reval-{cand}-{idx}"),
        revalidated_process_instance_id: proc.clone(),
        cold_evidence: ColdEvidence::ProcessCold {
            process_instance_id: proc,
        },
        start_temperature_celsius_tenths: 350,
        cooldown_duration_ms: 200_000,
        cooldown_exit_temperature_celsius_tenths: 340,
        thermal_sensor_identity: "thermal_zone0".into(),
        throttling_detector_identity: "detector-v1".into(),
        disposition: RunDisposition::Admissible,
        metrics: metrics(mode),
    }
}

fn result(id: &str, score: u32, installed_bytes: u64) -> CandidateResult {
    let cand = candidate(id);
    let runs: Vec<(u32, RunMode, u64)> = run_plan()
        .iter()
        .filter(|e| e.candidate_id == id)
        .map(|e| (e.index, e.mode, e.seed))
        .collect();
    CandidateResult {
        candidate_id: id.into(),
        candidate_identity: candidate_identity(cand.clone()),
        protocol_identity: v6_declaration_identity(protocol()),
        device: "Pixel 8a".into(),
        os_version: "17".into(),
        runtime_identity: "llama.cpp@f5b9bd39".into(),
        prompts: corpus()
            .prompts
            .iter()
            .map(|p| {
                let b = cand
                    .prompt_bindings
                    .iter()
                    .find(|b| b.prompt_id == p.prompt_id)
                    .expect("a binding per frozen prompt");
                BenchmarkPrompt {
                    prompt_id: p.prompt_id.clone(),
                    task_kind: p.task_kind,
                    context_profile: p.context_profile,
                    semantic_prompt_hash: semantic_of(&p.prompt_id),
                    rendered_prompt_hash: b.rendered_prompt_hash.clone(),
                    input_token_ids_hash: b.input_token_ids_hash.clone(),
                }
            })
            .collect(),
        installed_bytes,
        quality_observations: quality_observations(id, score),
        disposition: RunDisposition::Admissible,
        observations: runs
            .iter()
            .map(|(i, m, sd)| observation(*i, id, *m, *sd))
            .collect(),
    }
}

// ---------- freezing ----------

#[test]
fn the_protocol_identity_pins_every_term() {
    assert!(validate_v6_declaration(protocol()).is_empty());
    let base = v6_declaration_identity(protocol());
    type Mutation = fn(&mut V6Declaration);
    let mutations: Vec<(&str, Mutation)> = vec![
        // v6: there is no corpus digest to mutate. Editing the corpus itself
        // is what changes the declaration, which is the whole point.
        ("corpus text", |p| {
            p.corpus.prompts[0].message = "Summarize this instead.".into()
        }),
        ("corpus id", |p| p.corpus.corpus_id = "other".into()),
        ("corpus taxonomy", |p| {
            p.corpus.prompts[0].context_profile = PromptContextProfile::Privacy
        }),
        ("quota map", |p| {
            p.composition_policy.exact_required_count_per_pair[0].exact_count = 3
        }),
        ("scorer policy", |p| {
            p.scorer_policy = ScorerPolicy::SingleScorer {
                scorer_identity_digest: sha(0x01),
            }
        }),
        ("rubric", |p| p.quality_rubric_id = "other".into()),
        ("temperature", |p| p.sampler.temperature = 0.9),
        ("seeds", |p| p.seeds = vec![11, 22, 34]),
        ("timed runs", |p| p.timed_runs = 5),
        // Relaxing a ceiling after seeing a result must not masquerade as the
        // same protocol.
        ("latency ceiling", |p| {
            p.thresholds.max_time_to_first_token_ms = 9000
        }),
        ("material margin", |p| {
            p.thresholds.material_quality_margin_millionths = 0
        }),
        ("charging state", |p| {
            p.environment.charging_state = ChargingState::OnBattery
        }),
        ("cold definition", |p| {
            p.environment.cold_definition = ColdDefinition::FilesystemCacheCold
        }),
        ("airplane mode", |p| p.environment.airplane_mode = false),
        ("run plan", |p| p.run_plan.truncate(2)),
    ];
    for (name, mutate) in mutations {
        let mut p = protocol();
        mutate(&mut p);
        assert_ne!(
            v6_declaration_identity(p),
            base,
            "changing the {name} must change the declaration identity"
        );
    }
    // Seed ORDER is not a term; the seed SET is.
    let mut reordered = protocol();
    reordered.seeds = vec![33, 11, 22];
    assert_eq!(v6_declaration_identity(reordered), base);
}

// Hashing an invalid protocol does not make it valid.
#[test]
fn an_invalid_protocol_is_rejected_however_it_hashes() {
    type Mutation = fn(&mut V6Declaration);
    let cases: Vec<(&str, Mutation)> = vec![
        // v5 is not merely older; this evaluator will not read it.
        ("unsupported version", |p| p.protocol_version = 5),
        ("empty seeds", |p| p.seeds.clear()),
        ("duplicate seeds", |p| p.seeds = vec![11, 11]),
        ("no prompts", |p| {
            p.corpus.prompts.clear();
            p.prompt_count = 0
        }),
        ("prompt count disagrees", |p| p.prompt_count = 9),
        ("blank corpus id", |p| p.corpus.corpus_id = "  ".into()),
        ("scorer with no identity", |p| {
            p.scorer_policy = ScorerPolicy::SingleScorer {
                scorer_identity_digest: "  ".into(),
            }
        }),
        ("wrong fixed-point scale", |p| {
            p.policies.fixed_point_scale = 1000
        }),
        ("wrong evaluator schema", |p| {
            p.policies.evaluator_schema_version = 5
        }),
        ("undomained policy id", |p| {
            p.policies.promotion_rule_id = "v6-1".into()
        }),
        ("empty run plan", |p| p.run_plan.clear()),
        ("sparse run plan", |p| p.run_plan[0].index = 7),
        ("zero timeout", |p| p.per_run_timeout_ms = 0),
        ("NaN temperature", |p| p.sampler.temperature = f64::NAN),
        ("topP above 1", |p| p.sampler.top_p = 1.5),
        ("zero context cap", |p| p.sampler.context_cap = 0),
        ("zero ceiling", |p| p.thresholds.max_peak_rss_mb = 0),
        ("margin above 1.0", |p| {
            p.thresholds.material_quality_margin_millionths = 1_000_001
        }),
        ("brightness above 100", |p| {
            p.environment.screen_brightness_percent = 150
        }),
        ("screen off but bright", |p| p.environment.screen_on = false),
        ("start ceiling at cutoff", |p| {
            p.environment.thermal_start_ceiling_celsius_tenths = 450
        }),
        // Charging state and battery policy must agree.
        ("plugged in but gating battery", |p| {
            p.thresholds.battery_policy = BatteryEvidencePolicy::OnBattery {
                max_drop_tenths_percent: 60,
            }
        }),
        ("on battery but telemetry-only", |p| {
            p.environment.charging_state = ChargingState::OnBattery
        }),
    ];
    for (name, mutate) in cases {
        let mut p = protocol();
        mutate(&mut p);
        assert!(
            !validate_v6_declaration(p).is_empty(),
            "{name} must be rejected"
        );
    }
    assert!(validate_v6_declaration(protocol()).is_empty(), "polarity");
}

// ---------- the sealed door ----------

// There is no promotion path that skips admission.
#[test]
fn an_inadmissible_run_cannot_be_promoted() {
    let mut leaked = result(B, 990_000, 400_000_000);
    leaked.disposition = RunDisposition::InadmissibleReasoningLeakage {
        detector: "frozen-prose-v1".into(),
    };
    // Despite the highest possible quality panel, it promotes nothing.
    match evaluate_promotion(
        protocol(),
        candidate(A),
        result(A, 500_000, 400_000_000),
        candidate(B),
        leaked,
    ) {
        PromotionVerdict::PromoteNothing { reason, .. } => {
            assert!(reason.contains("inadmissible"), "{reason}")
        }
        other => panic!("a leaked run must not be promoted, got {other:?}"),
    }
    // Every non-admissible disposition behaves the same way.
    for d in [
        RunDisposition::ThermallyThrottled,
        RunDisposition::PromptParityMismatch,
        RunDisposition::CancelledByOperator,
        RunDisposition::Interrupted {
            reason: "call".into(),
        },
    ] {
        let mut r = result(B, 990_000, 400_000_000);
        r.disposition = d;
        assert!(matches!(
            evaluate_promotion(
                protocol(),
                candidate(A),
                result(A, 500_000, 400_000_000),
                candidate(B),
                r
            ),
            PromotionVerdict::PromoteNothing { .. }
        ));
    }
    // Polarity: two admissible results DO produce a verdict.
    assert!(!matches!(
        evaluate_promotion(
            protocol(),
            candidate(A),
            result(A, 500_000, 400_000_000),
            candidate(B),
            result(B, 900_000, 400_000_000)
        ),
        PromotionVerdict::PromoteNothing { .. }
    ));
}

// A result must actually belong to the candidate it is scored against.
#[test]
fn results_are_bound_to_their_candidate_and_prompts() {
    let p = protocol();
    assert_eq!(
        admit_result(result(A, 800_000, 400_000_000), candidate(A), p.clone()),
        None
    );
    // The same result cannot be admitted against the OTHER candidate.
    assert_eq!(
        admit_result(result(A, 800_000, 400_000_000), candidate(B), p.clone()),
        Some(AdmissionFailure::ResultCandidateMismatch)
    );
    // A forged identity is caught even when the id matches.
    let mut forged = result(A, 800_000, 400_000_000);
    forged.candidate_identity = sha(0x01);
    assert_eq!(
        admit_result(forged, candidate(A), p.clone()),
        Some(AdmissionFailure::CandidateIdentityMismatch)
    );
    // An empty prompt list is no longer admissible.
    let mut no_prompts = result(A, 800_000, 400_000_000);
    no_prompts.prompts.clear();
    assert_eq!(
        admit_result(no_prompts, candidate(A), p.clone()),
        Some(AdmissionFailure::PromptCountMismatch)
    );
    let mut dup = result(A, 800_000, 400_000_000);
    dup.prompts.push(dup.prompts[0].clone());
    assert!(matches!(
        admit_result(dup, candidate(A), p.clone()),
        Some(AdmissionFailure::PromptCountMismatch)
    ));
    let mut wrong_semantic = result(A, 800_000, 400_000_000);
    wrong_semantic.prompts[0].semantic_prompt_hash = sha(0x13);
    assert_eq!(
        admit_result(wrong_semantic, candidate(A), p.clone()),
        Some(AdmissionFailure::SemanticPromptMismatch)
    );
    // Rendered and token parity are candidate-specific and declared ahead.
    let mut wrong_render = result(A, 800_000, 400_000_000);
    wrong_render.prompts[0].rendered_prompt_hash = sha(0x99);
    assert!(matches!(
        admit_result(wrong_render, candidate(A), p.clone()),
        Some(AdmissionFailure::RenderedPromptMismatch { .. })
    ));
    let mut wrong_tokens = result(A, 800_000, 400_000_000);
    wrong_tokens.prompts[0].input_token_ids_hash = sha(0x98);
    assert!(matches!(
        admit_result(wrong_tokens, candidate(A), p),
        Some(AdmissionFailure::InputTokenMismatch { .. })
    ));
}

// The ledger must match the frozen plan one-to-one.
#[test]
fn runs_cannot_be_dropped_reordered_or_selectively_discarded() {
    let p = protocol();
    let mut missing = result(A, 800_000, 400_000_000);
    missing.observations.pop();
    assert!(matches!(
        admit_result(missing, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    let mut reordered = result(A, 800_000, 400_000_000);
    reordered.observations.reverse();
    assert!(matches!(
        admit_result(reordered, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    let mut wrong_seed = result(A, 800_000, 400_000_000);
    wrong_seed.observations[0].seed = 99;
    assert!(matches!(
        admit_result(wrong_seed, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    // A run with no integrity receipt is not evidence.
    let mut no_receipt = result(A, 800_000, 400_000_000);
    no_receipt.observations[0].revalidation_evidence_id = "  ".into();
    assert!(matches!(
        admit_result(no_receipt, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    // A single bad run poisons the candidate even if the aggregate looks fine.
    let mut one_bad = result(A, 800_000, 400_000_000);
    one_bad.observations[1].disposition = RunDisposition::ThermallyThrottled;
    assert!(matches!(
        admit_result(one_bad, candidate(A), p),
        Some(AdmissionFailure::Disqualified { .. })
    ));
}

// The frozen conditions must be the observed ones.
#[test]
fn observed_conditions_must_match_the_frozen_environment() {
    let p = protocol();
    type Mutation = fn(&mut RunObservation);
    let deviations: Vec<(&str, Mutation)> = vec![
        ("charging", |o| {
            o.observed_charging_state = ChargingState::OnBattery
        }),
        ("airplane mode", |o| o.observed_airplane_mode = false),
        ("screen", |o| o.observed_screen_on = false),
        ("start temperature", |o| {
            o.start_temperature_celsius_tenths = 440
        }),
        ("cooldown exit", |o| {
            o.cooldown_exit_temperature_celsius_tenths = 430
        }),
        ("cooldown too short", |o| o.cooldown_duration_ms = 1000),
    ];
    for (name, mutate) in deviations {
        let mut r = result(A, 800_000, 400_000_000);
        mutate(&mut r.observations[0]);
        assert!(
            matches!(
                admit_result(r, candidate(A), p.clone()),
                Some(AdmissionFailure::EnvironmentDeviation { .. })
            ),
            "{name} must be caught"
        );
    }
    // A filesystem-cache-cold CLAIM needs external evidence, not an enum.
    let mut fs_cold = protocol();
    fs_cold.environment.cold_definition = ColdDefinition::FilesystemCacheCold;
    let mut r = result(A, 800_000, 400_000_000);
    r.protocol_identity = v6_declaration_identity(fs_cold.clone());
    assert!(
        matches!(
            admit_result(r.clone(), candidate(A), fs_cold.clone()),
            Some(AdmissionFailure::ColdEvidenceMissing { .. })
        ),
        "process-cold evidence cannot support a filesystem-cache-cold claim"
    );
    let mut with_evidence = r;
    for o in with_evidence.observations.iter_mut() {
        o.cold_evidence = ColdEvidence::FilesystemCacheCold {
            external_step_evidence_id: "privileged-drop-caches-001".into(),
        };
    }
    assert_eq!(admit_result(with_evidence, candidate(A), fs_cold), None);
}

// ---------- battery evidence ----------

#[test]
fn a_plugged_in_battery_figure_cannot_gate_promotion() {
    assert!(!battery_delta_is_energy_evidence(environment()));
    // A wild battery number under the plugged-in protocol changes nothing.
    let mut thirsty = cost();
    thirsty.battery_drop_tenths_percent = 9_999;
    assert!(
        meets_thresholds(thirsty.clone(), thresholds()).is_empty(),
        "plugged-in battery telemetry must not fail a candidate"
    );
    // Under an on-battery policy the same figure DOES gate.
    let on_battery = PromotionThresholds {
        battery_policy: BatteryEvidencePolicy::OnBattery {
            max_drop_tenths_percent: 60,
        },
        ..thresholds()
    };
    assert!(meets_thresholds(thirsty, on_battery.clone())
        .iter()
        .any(|f| f == "batteryDrop"));
    assert!(meets_thresholds(cost(), on_battery).is_empty(), "polarity");
    assert!(battery_delta_is_energy_evidence(RunEnvironment {
        charging_state: ChargingState::OnBattery,
        ..environment()
    }));
}

// ---------- the decision rule ----------

#[test]
fn promotion_requires_earning_it_not_being_newer_or_smaller() {
    let small = 300_000_000u64;
    let large = 600_000_000u64;
    // Equal quality, different size -> a tier split, not a winner.
    match evaluate_promotion(
        protocol(),
        candidate(A),
        result(A, 800_000, small),
        candidate(B),
        result(B, 800_000, large),
    ) {
        PromotionVerdict::SplitTiers {
            basic_candidate_id,
            enhanced_candidate_id,
            ..
        } => {
            assert_eq!(basic_candidate_id, A, "smaller becomes Basic");
            assert_eq!(enhanced_candidate_id, B);
        }
        other => panic!("expected a split, got {other:?}"),
    }
    // Below the material margin is still a split — size must be earned.
    assert!(matches!(
        evaluate_promotion(
            protocol(),
            candidate(A),
            result(A, 800_000, small),
            candidate(B),
            result(B, 830_000, large)
        ),
        PromotionVerdict::SplitTiers { .. }
    ));
    // A material margin promotes, in both directions.
    assert!(matches!(
        evaluate_promotion(protocol(), candidate(A), result(A, 800_000, small),
                           candidate(B), result(B, 900_000, large)),
        PromotionVerdict::Promote { ref candidate_id, .. } if candidate_id == B
    ));
    assert!(matches!(
        evaluate_promotion(protocol(), candidate(A), result(A, 950_000, small),
                           candidate(B), result(B, 800_000, large)),
        PromotionVerdict::Promote { ref candidate_id, .. } if candidate_id == A
    ));
}

/// A ceiling can now only be blown through the LEDGER — the aggregate is
/// derived, so there is no way to inject a slow summary directly.
fn slow_result(id: &str, score: u32) -> CandidateResult {
    let mut r = result(id, score, 400_000_000);
    for o in r.observations.iter_mut() {
        o.metrics.time_to_first_token_ms = 9_000;
    }
    r
}

#[test]
fn both_failing_promotes_nothing_and_a_ceiling_beats_a_score() {
    assert!(matches!(
        evaluate_promotion(
            protocol(),
            candidate(A),
            slow_result(A, 900_000),
            candidate(B),
            slow_result(B, 950_000)
        ),
        PromotionVerdict::PromoteNothing { .. }
    ));
    // The passing candidate wins even with the LOWER score.
    assert!(matches!(
        evaluate_promotion(protocol(), candidate(A), result(A, 600_000, 400_000_000),
                           candidate(B), slow_result(B, 990_000)),
        PromotionVerdict::Promote { ref candidate_id, .. } if candidate_id == A
    ));
}

#[test]
fn every_frozen_ceiling_is_actually_checked() {
    assert!(meets_thresholds(cost(), thresholds()).is_empty());
    type Mutation = fn(&mut CostObservation);
    let cases: Vec<(&str, Mutation)> = vec![
        ("coldLoadMs", |c| c.cold_load_ms = 99_000),
        ("timeToFirstTokenMs", |c| c.time_to_first_token_ms = 99_000),
        ("generationTokensPerSecond", |c| {
            c.generation_tokens_per_second = 0.5
        }),
        ("peakRssMb", |c| c.peak_rss_mb = 99_000),
        ("installedBytes", |c| c.installed_bytes = 9_000_000_000),
        ("cancellationLatencyMs", |c| {
            c.cancellation_latency_ms = 9_000
        }),
        ("peakTemperature", |c| {
            c.peak_temperature_celsius_tenths = 900
        }),
        ("thermallyThrottled", |c| c.thermally_throttled = true),
        ("backgroundForegroundRecovery", |c| {
            c.background_foreground_recovered = false
        }),
    ];
    for (name, mutate) in cases {
        let mut c = cost();
        mutate(&mut c);
        let failures = meets_thresholds(c, thresholds());
        assert!(failures.iter().any(|f| f == name), "{name}: {failures:?}");
    }
}

#[test]
fn quality_score_refuses_impossible_panels() {
    // Ten equal axes average to themselves EXACTLY, with denominator 1.
    assert_eq!(
        quality_score(panel(500_000)),
        Some(ExactMillionths {
            numerator: 500_000,
            denominator: 1
        })
    );
    // v6 shrank this test's job, and that is the point: as `u32` millionths,
    // NaN, infinity and a negative score are not representable at all. Only
    // "above 1.0" is still expressible, so only it still needs refusing.
    let mut p = panel(800_000);
    p.avoids_repetition = 1_000_001;
    assert_eq!(quality_score(p), None, "an axis above 1.0 must not score");
    // The boundary itself is legal.
    assert!(quality_score(panel(1_000_000)).is_some());
}

// ---------- provenance ----------

#[test]
fn candidate_identity_records_how_the_artifact_came_to_exist() {
    let official = candidate_identity(candidate(A));
    let derived = candidate_identity(candidate(B));
    assert_ne!(official, derived);
    // Same bytes, different provenance claim -> different identity.
    let mut relabelled = candidate(A);
    relabelled.provenance = ArtifactProvenance::DerivedByConversion;
    relabelled.conversion = Some(conversion());
    assert_ne!(candidate_identity(relabelled), official);
    // Every link of the conversion chain is part of the identity.
    type Mutation = fn(&mut ConversionRecord);
    let links: Vec<(&str, Mutation)> = vec![
        ("source revision", |c| c.source_revision = "other".into()),
        ("llama.cpp commit", |c| c.conversion_commit = "other".into()),
        ("intermediate hash", |c| {
            c.intermediate_sha256_hex = sha(0x99)
        }),
        ("output hash", |c| c.output_sha256_hex = sha(0x98)),
    ];
    for (name, mutate) in links {
        let mut cand = candidate(B);
        let mut conv = conversion();
        mutate(&mut conv);
        cand.conversion = Some(conv);
        assert_ne!(candidate_identity(cand), derived, "the {name} is identity");
    }
}

#[test]
fn unreproducible_or_unproven_candidates_are_inadmissible() {
    let p = protocol();
    // A derived artifact with no conversion record cannot be reproduced.
    let mut no_record = candidate(B);
    no_record.conversion = None;
    let mut r = result(B, 800_000, 400_000_000);
    r.candidate_identity = candidate_identity(no_record.clone());
    assert_eq!(
        admit_result(r, no_record, p.clone()),
        Some(AdmissionFailure::MissingConversionRecord)
    );
    // Requantizing an already-quantized file degrades quality; refused.
    let mut requant = candidate(B);
    let mut conv = conversion();
    conv.allow_requantize = true;
    requant.conversion = Some(conv);
    let mut r2 = result(B, 800_000, 400_000_000);
    r2.candidate_identity = candidate_identity(requant.clone());
    assert_eq!(
        admit_result(r2, requant, p.clone()),
        Some(AdmissionFailure::RequantizationNotAllowed)
    );
    // Qwen3 defaults to thinking mode: it must be PROVEN off.
    let mut thinking = candidate(B);
    thinking.thinking_mode_disabled = false;
    let mut r3 = result(B, 800_000, 400_000_000);
    r3.candidate_identity = candidate_identity(thinking.clone());
    assert_eq!(
        admit_result(r3, thinking, p),
        Some(AdmissionFailure::ThinkingModeNotDisabled)
    );
}

#[test]
fn semantic_and_rendered_prompt_identities_are_separate() {
    let a = semantic_prompt_hash(
        BenchmarkTaskKind::Summarization,
        PromptContextProfile::Neutral,
        "Summarize this.".into(),
    );
    assert_eq!(
        a,
        semantic_prompt_hash(
            BenchmarkTaskKind::Summarization,
            PromptContextProfile::Neutral,
            "Summarize this.".into()
        ),
        "the same semantic input must hash identically for both models"
    );
    // Different templates render different bytes from one input — legitimate,
    // and it must stay visible.
    assert_ne!(
        rendered_prompt_hash("<|im_start|>assistant\n".into()),
        rendered_prompt_hash("<|im_start|>assistant\n<think>\n\n</think>\n\n".into())
    );
    // v6: changing EITHER axis is a different benchmark item.
    assert_ne!(
        semantic_prompt_hash(
            BenchmarkTaskKind::ReflectiveDialogue,
            PromptContextProfile::Neutral,
            "Summarize this.".into()
        ),
        a
    );
    assert_ne!(
        semantic_prompt_hash(
            BenchmarkTaskKind::Summarization,
            PromptContextProfile::Hypnagogic,
            "Summarize this.".into()
        ),
        a
    );
}

#[test]
fn the_environment_requires_its_bias_controls() {
    assert!(validate_run_environment(environment()).is_empty());
    type Mutation = fn(&mut RunEnvironment);
    let required: Vec<(&str, Mutation)> = vec![
        ("process restart", |e| {
            e.restart_process_between_candidates = false
        }),
        ("integrity recheck", |e| {
            e.recheck_pack_integrity_between_candidates = false
        }),
        ("cooldown minimum", |e| e.cooldown_minimum_seconds = 0),
        ("thermal start ceiling", |e| {
            e.thermal_start_ceiling_celsius_tenths = 0
        }),
    ];
    for (name, mutate) in required {
        let mut e = environment();
        mutate(&mut e);
        assert!(!validate_run_environment(e).is_empty(), "{name} required");
    }
}

// v4: ceilings are compared against the WORST run, never an average.
#[test]
fn one_bad_run_cannot_be_averaged_away() {
    // Two good cold runs and one slow one: the mean would pass, the envelope
    // must not.
    let mut r = result(A, 900_000, 400_000_000);
    let slow_idx = r
        .observations
        .iter()
        .position(|o| o.mode == RunMode::Cold)
        .unwrap();
    r.observations[slow_idx].metrics.time_to_first_token_ms = 9_000;
    let derived = derive_cost_observation(r.observations.clone(), 400_000_000).unwrap();
    assert_eq!(
        derived.time_to_first_token_ms, 9_000,
        "the envelope must take the worst run, not the mean"
    );
    assert!(meets_thresholds(derived, thresholds())
        .iter()
        .any(|f| f == "timeToFirstTokenMs"));

    // Throughput floors take the MINIMUM observed rate.
    let mut slowgen = result(B, 900_000, 400_000_000);
    let i = slowgen
        .observations
        .iter()
        .position(|o| o.mode == RunMode::Warm)
        .unwrap();
    slowgen.observations[i].metrics.generation_duration_ms = 1_000_000;
    let d = derive_cost_observation(slowgen.observations, 400_000_000).unwrap();
    assert!(
        d.generation_tokens_per_second < 1.0,
        "a single slow run must drag the floor down, got {}",
        d.generation_tokens_per_second
    );

    // Battery comes from the sustained run alone, so it does not grow with
    // the length of the run plan.
    let base = result(A, 900_000, 400_000_000);
    let d = derive_cost_observation(base.observations.clone(), 400_000_000).unwrap();
    assert_eq!(d.battery_drop_tenths_percent, 2, "sustained run only");
}

// The global order is validated against the plan, not asserted.
#[test]
fn a_non_counterbalanced_order_is_rejected_globally() {
    let p = protocol();
    assert!(
        validate_global_ledger(p.clone(), result(A, 800_000, 1), result(B, 800_000, 1)).is_empty(),
        "the frozen ABBA plan validates"
    );
    // A process instance shared across candidates is refused.
    let a = result(A, 800_000, 1);
    let mut b = result(B, 800_000, 1);
    let shared = a.observations[0].process_instance_id.clone();
    b.observations[0].process_instance_id = shared;
    assert!(!validate_global_ledger(p.clone(), a, b).is_empty());
    // A ledger that does not cover the plan exactly once is refused.
    let mut short = result(A, 800_000, 1);
    short.observations.pop();
    assert!(!validate_global_ledger(p, short, result(B, 800_000, 1)).is_empty());
}

// Fresh revalidation evidence cannot be a copied digest.
#[test]
fn revalidation_evidence_must_be_fresh_per_run() {
    let p = protocol();
    let mut reused = result(A, 800_000, 400_000_000);
    let first = reused.observations[0].revalidation_evidence_id.clone();
    reused.observations[1].revalidation_evidence_id = first;
    assert!(matches!(
        admit_result(reused, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    // A run that loaded a different artifact is not this candidate's run.
    let mut wrong_artifact = result(A, 800_000, 400_000_000);
    wrong_artifact.observations[0].loaded_model_sha256 = sha(0x77);
    assert!(matches!(
        admit_result(wrong_artifact, candidate(A), p),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
}

// v5: quality is derived from the raw ledger, not supplied.
#[test]
fn the_quality_panel_is_derived_and_cannot_be_asserted() {
    let p = protocol();
    // The frozen plan derives the expected macro panel.
    match derive_quality_panel(p.clone(), candidate(A), quality_observations(A, 800_000)) {
        QualityDerivation::Derived { panel } => assert_eq!(
            derived_quality_score(panel),
            Some(ExactMillionths {
                numerator: 800_000,
                denominator: 1
            }),
            "a uniform ledger must derive back to exactly its own value"
        ),
        other => panic!("expected a derived panel, got {other:?}"),
    }
    // Declaration order does not change the derived panel.
    let mut shuffled = quality_observations(A, 800_000);
    shuffled.reverse();
    let ordered = derive_quality_panel(p.clone(), candidate(A), quality_observations(A, 800_000));
    assert_eq!(
        derive_quality_panel(p.clone(), candidate(A), shuffled),
        ordered
    );
    // One candidate's ledger cannot be admitted for the other.
    assert!(matches!(
        derive_quality_panel(p.clone(), candidate(B), quality_observations(A, 800_000)),
        QualityDerivation::Rejected { .. }
    ));
    // Missing, duplicated or extra observations reject.
    let mut short = quality_observations(A, 800_000);
    short.pop();
    assert!(matches!(
        derive_quality_panel(p.clone(), candidate(A), short),
        QualityDerivation::Rejected {
            failure: QualityAdmissionFailure::PlanCoverageMismatch { .. }
        }
    ));
    let mut extra = quality_observations(A, 800_000);
    extra.push(extra[0].clone());
    assert!(matches!(
        derive_quality_panel(p.clone(), candidate(A), extra),
        QualityDerivation::Rejected { .. }
    ));
    // Swapped output parity, wrong rubric, wrong blinding manifest all reject.
    type M = fn(&mut PromptQualityObservation);
    let cases: Vec<(&str, M)> = vec![
        ("rendered parity", |o| o.rendered_prompt_hash = sha(0x01)),
        ("token parity", |o| o.input_token_ids_hash = sha(0x02)),
        ("semantic parity", |o| o.semantic_prompt_hash = sha(0x03)),
        ("rubric", |o| o.rubric_id = "other".into()),
        ("blinding manifest", |o| {
            o.blinding_manifest_digest = sha(0x04)
        }),
        ("seed", |o| o.seed = 99),
        ("run", |o| o.run_id = "elsewhere".into()),
    ];
    for (name, mutate) in cases {
        let mut obs = quality_observations(A, 800_000);
        mutate(&mut obs[0]);
        assert!(
            matches!(
                derive_quality_panel(p.clone(), candidate(A), obs),
                QualityDerivation::Rejected { .. }
            ),
            "{name} must reject"
        );
    }
    // A hard-invalid generation cannot enter the average as a low score.
    for d in [
        QualityDisposition::ReasoningLeakage {
            detector: "frozen-prose-v1".into(),
        },
        QualityDisposition::ParityFailure,
        QualityDisposition::MissingOutput,
        QualityDisposition::TimeoutWithoutScorableContinuation,
        QualityDisposition::MalformedRequiredStructure,
    ] {
        let mut obs = quality_observations(A, 800_000);
        obs[0].disposition = d;
        assert!(matches!(
            derive_quality_panel(p.clone(), candidate(A), obs),
            QualityDerivation::Rejected {
                failure: QualityAdmissionFailure::Inadmissible { .. }
            }
        ));
    }
    // An axis above 1.0 rejects rather than skewing the mean. Under fixed
    // point that is the only malformed value still expressible.
    let mut obs = quality_observations(A, 800_000);
    obs[0].scores.avoids_repetition = 1_000_001;
    assert!(matches!(
        derive_quality_panel(p.clone(), candidate(A), obs),
        QualityDerivation::Rejected {
            failure: QualityAdmissionFailure::MalformedScores { .. }
        }
    ));
    // Altering one raw score moves only the legitimately derived result — and
    // the result it moves to is a value no rounding would have produced.
    let mut nudged = quality_observations(A, 800_000);
    nudged[0].scores.instruction_adherence = 200_000;
    match derive_quality_panel(p, candidate(A), nudged) {
        QualityDerivation::Derived { panel } => {
            // One of three seeds on one of eighteen prompts:
            //   that prompt averages (200_000 + 800_000 + 800_000)/3 = 600_000
            //   the axis averages (600_000 + 17*800_000)/18 = 14_200_000/18
            // which reduces to 7_100_000/9 — 788_888.88... millionths. Carried
            // as a fraction, so nothing is lost before the margin comparison.
            assert_eq!(
                panel.instruction_adherence,
                ExactMillionths {
                    numerator: 7_100_000,
                    denominator: 9
                }
            );
            assert_eq!(
                panel.avoids_repetition,
                ExactMillionths {
                    numerator: 800_000,
                    denominator: 1
                }
            );
        }
        other => panic!("expected a derived panel, got {other:?}"),
    }
}

// Scoring eligibility comes from the EXECUTION ledger, not the plan alone.
#[test]
fn a_scored_output_must_name_a_run_that_actually_happened() {
    let p = protocol();
    assert_eq!(
        admit_result(result(A, 800_000, 400_000_000), candidate(A), p.clone()),
        None
    );
    // A plan may name a run that never executed; admission must catch it.
    let mut ghost = result(A, 800_000, 400_000_000);
    ghost.quality_observations[0].run_id = "never-ran".into();
    assert!(matches!(
        admit_result(ghost, candidate(A), p.clone()),
        Some(AdmissionFailure::QualityLedgerRejected { .. })
    ));
    // A warmup, cold, sustained or cancellation output cannot be scored,
    // even when the plan points at it.
    for mode in [
        RunMode::Warmup,
        RunMode::Cold,
        RunMode::Sustained,
        RunMode::Cancellation,
    ] {
        let mut r = result(A, 800_000, 400_000_000);
        let ineligible = r
            .observations
            .iter()
            .find(|o| o.mode == mode)
            .map(|o| o.run_id.clone())
            .unwrap();
        r.quality_observations[0].run_id = ineligible;
        assert!(
            matches!(
                admit_result(r, candidate(A), p.clone()),
                Some(AdmissionFailure::QualityLedgerRejected { .. })
            ),
            "{mode:?} must not be scoring-eligible"
        );
    }
    // The seed claimed by the scored output must be the seed the run used.
    let mut wrong_seed = result(A, 800_000, 400_000_000);
    wrong_seed.quality_observations[0].seed = 99;
    assert!(matches!(
        admit_result(wrong_seed, candidate(A), p.clone()),
        Some(AdmissionFailure::QualityLedgerRejected { .. })
    ));
    // A run id belonging to the other candidate is absent from THIS
    // candidate's admitted execution ledger, so it is refused. Note this
    // exercises the "run never executed" branch, not the candidate-mismatch
    // branch: the ledger walk above has already required every observation to
    // belong to the admitted candidate, which makes that comparison
    // defence-in-depth rather than a reachable path.
    let mut borrowed = result(A, 800_000, 400_000_000);
    borrowed.quality_observations[0].run_id = result(B, 800_000, 1).observations[0].run_id.clone();
    assert!(matches!(
        admit_result(borrowed, candidate(A), p),
        Some(AdmissionFailure::QualityLedgerRejected { .. })
    ));
}

// ============================================================================
// v6-1 adversarial regressions (issue #7)
//
// Each one exists because the v5 declaration layer would have passed it. They
// are named for the defect, not the feature, and every one asserts both
// polarities — a rejection test that would also reject the valid case has
// demonstrated nothing.
// ============================================================================

fn extra_prompt(id: &str, task: BenchmarkTaskKind, ctx: PromptContextProfile) -> CorpusPrompt {
    CorpusPrompt {
        prompt_id: id.into(),
        task_kind: task,
        context_profile: ctx,
        message: format!("An extra prompt, {id}, that the quota did not ask for."),
    }
}

fn errors_mentioning(d: V6Declaration, needle: &str) -> Vec<String> {
    validate_v6_declaration(d)
        .into_iter()
        .filter(|e| e.contains(needle))
        .collect()
}

// 1. Quota shape IS the weighting, so an off-quota corpus is not a smaller
//    study — it is a differently weighted one wearing the same declaration.
#[test]
fn a_corpus_that_misses_or_overshoots_its_quota_is_rejected() {
    assert!(
        validate_v6_declaration(protocol()).is_empty(),
        "the frozen 2-per-pair corpus satisfies its own quota"
    );

    // A surplus prompt in an ALLOWED pair. v5 had no quota at all; a minimum
    // rule would also have let this through, and it silently gives
    // summarization 3/19 of the macro weight instead of 2/18.
    let mut surplus = protocol();
    surplus.corpus.prompts.push(extra_prompt(
        "p19",
        BenchmarkTaskKind::Summarization,
        PromptContextProfile::Neutral,
    ));
    surplus.prompt_count = surplus.corpus.prompts.len() as u32;
    assert!(
        !errors_mentioning(surplus, "quota requires exactly 2").is_empty(),
        "a surplus prompt must be named as a quota violation"
    );

    // A missing pair. The corpus still parses, still has unique ids, and every
    // pair it does contain is allowed.
    let mut missing = protocol();
    let doomed = missing
        .corpus
        .prompts
        .iter()
        .position(|p| p.context_profile == PromptContextProfile::Privacy)
        .expect("the corpus declares a privacy pair");
    missing.corpus.prompts.remove(doomed);
    missing.prompt_count = missing.corpus.prompts.len() as u32;
    let errs = errors_mentioning(missing, "quota requires exactly 2");
    assert!(
        errs.iter().any(|e| e.contains("Disclosure")),
        "a missing prompt must name the pair that came up short, got {errs:?}"
    );

    // Removing an entire pair from the quota map is likewise not a smaller
    // study: the map must cover the matrix.
    let mut short_map = protocol();
    short_map
        .composition_policy
        .exact_required_count_per_pair
        .pop();
    assert!(!validate_v6_declaration(short_map).is_empty());

    // A zero quota is an absent pair pretending to be a present one.
    let mut zeroed = protocol();
    zeroed.composition_policy.exact_required_count_per_pair[0].exact_count = 0;
    assert!(!errors_mentioning(zeroed, "zero quota").is_empty());
}

// 2. The cross product is not the matrix. 42 pairs are expressible; 9 are
//    declared, and the declaration cannot widen its own permission.
#[test]
fn a_task_context_pair_outside_the_allowed_matrix_is_rejected() {
    assert_eq!(allowed_task_context_pairs().len(), 9);

    // A prompt relabelled into a combination nobody declared.
    let mut smuggled = protocol();
    smuggled.corpus.prompts[0].context_profile = PromptContextProfile::Privacy;
    assert!(
        !errors_mentioning(smuggled, "outside the allowed matrix").is_empty(),
        "Summarization x Privacy is not a benchmark item this protocol defines"
    );

    // Widening the declared matrix does not widen the normative one.
    let mut widened = protocol();
    widened
        .composition_policy
        .allowed_task_context_pairs
        .push(TaskContextPair {
            task_kind: BenchmarkTaskKind::DreamAnalysis,
            context_profile: PromptContextProfile::Privacy,
        });
    assert!(
        !errors_mentioning(widened, "normative nine-pair matrix").is_empty(),
        "the allowed matrix is recorded by the declaration, not chosen by it"
    );

    // Reordering it is also a different declaration: the list is canonical so
    // that the identity is.
    let mut reordered = protocol();
    reordered
        .composition_policy
        .allowed_task_context_pairs
        .swap(0, 1);
    assert!(!errors_mentioning(reordered, "normative nine-pair matrix").is_empty());
}

// 3. Context is not presentation. Two prompts with identical words under
//    different contexts are different questions, and averaging them together
//    would be averaging two different questions.
#[test]
fn the_context_profile_alone_changes_the_semantic_prompt_identity() {
    let message = "Note it as thinking and return to the breath.";
    let base = semantic_prompt_hash(
        BenchmarkTaskKind::InstructionRendering,
        PromptContextProfile::MindfulnessObservation,
        message.into(),
    );
    for other in [
        PromptContextProfile::ReturnToTask,
        PromptContextProfile::StopAndDebrief,
        PromptContextProfile::Neutral,
    ] {
        assert_ne!(
            semantic_prompt_hash(
                BenchmarkTaskKind::InstructionRendering,
                other,
                message.into()
            ),
            base,
            "{other:?} must not share an identity with MindfulnessObservation"
        );
    }
    // Polarity: the same pair and the same message is the same item.
    assert_eq!(
        base,
        semantic_prompt_hash(
            BenchmarkTaskKind::InstructionRendering,
            PromptContextProfile::MindfulnessObservation,
            message.into()
        )
    );
    // And the corpus really does exercise this: the same task kind appears
    // under three contexts, so the axis is load-bearing rather than decorative.
    let contexts: std::collections::BTreeSet<PromptContextProfile> = corpus()
        .prompts
        .iter()
        .filter(|p| p.task_kind == BenchmarkTaskKind::InstructionRendering)
        .map(|p| p.context_profile)
        .collect();
    assert_eq!(contexts.len(), 3);
}

// 4. protocol_version says what the data means; these say how it was read.
//    Without them a later evaluator reinterprets identical v6 data under an
//    unchanged digest — which is what happened between eb9492c and 3ccf874.
#[test]
fn each_evaluator_policy_identity_is_part_of_the_declaration_digest() {
    let base = v6_declaration_identity(protocol());
    type Mutation = fn(&mut EvaluatorPolicyIdentities);
    let identities: Vec<(&str, Mutation)> = vec![
        ("selectionPolicyId", |p| {
            p.selection_policy_id = "neuralcompose.selection-policy.v6-2".into()
        }),
        ("qualityAggregationPolicyId", |p| {
            p.quality_aggregation_policy_id = "neuralcompose.quality-aggregation-policy.v6-2".into()
        }),
        ("promotionRuleId", |p| {
            p.promotion_rule_id = "neuralcompose.promotion-rule.v6-2".into()
        }),
        ("fixedPointScale", |p| p.fixed_point_scale = 1_000),
        ("evaluatorSchemaVersion", |p| p.evaluator_schema_version = 7),
    ];
    for (name, mutate) in identities {
        let mut d = protocol();
        mutate(&mut d.policies);
        assert_ne!(
            v6_declaration_identity(d),
            base,
            "changing {name} must change the declaration digest"
        );
    }
}

// 5. The defect this replaces was not theoretical: it decided verdicts.
#[test]
fn an_exact_margin_comparison_has_no_epsilon_drift() {
    // What the f64 path actually did, reproduced rather than asserted. Both
    // of these are true of IEEE-754 doubles and are why the aggregate could
    // not be compared against an exact margin.
    assert_ne!(0.8f64 * 3.0 / 3.0, 0.8f64);
    assert_ne!(0.9f64 - 0.8f64, 0.1f64);

    // A ledger of all-0.8 axes derives back to exactly 0.8, denominator 1.
    match derive_quality_panel(protocol(), candidate(A), quality_observations(A, 800_000)) {
        QualityDerivation::Derived { panel } => {
            let exact = ExactMillionths {
                numerator: 800_000,
                denominator: 1,
            };
            assert_eq!(panel.instruction_adherence, exact);
            assert_eq!(derived_quality_score(panel), Some(exact));
        }
        other => panic!("expected a derived panel, got {other:?}"),
    }

    // The consequence, end to end. Scores of 0.9 and 0.8 are a margin of
    // exactly 0.1 against a material margin of exactly 0.1, so the larger
    // candidate HAS earned its size, and the boundary decides which of two
    // different verdicts is returned. The two assertions above show the
    // subtraction and the two-stage average both drift under f64; what the
    // old code returned for this specific 18-prompt ledger is not reproduced
    // here and is not claimed.
    let exact_margin = PromotionThresholds {
        material_quality_margin_millionths: 100_000,
        ..thresholds()
    };
    let mut d = protocol();
    d.thresholds = exact_margin;
    let mut a = result(A, 800_000, 300_000_000);
    let mut b = result(B, 900_000, 600_000_000);
    a.protocol_identity = v6_declaration_identity(d.clone());
    b.protocol_identity = v6_declaration_identity(d.clone());
    match evaluate_promotion(d.clone(), candidate(A), a.clone(), candidate(B), b.clone()) {
        PromotionVerdict::Promote {
            candidate_id,
            margin,
            ..
        } => {
            assert_eq!(candidate_id, B);
            assert_eq!(
                margin,
                ExactMillionths {
                    numerator: 100_000,
                    denominator: 1
                }
            );
        }
        other => panic!("a margin exactly at the requirement must promote, got {other:?}"),
    }
    // Polarity: one millionth short of the requirement is still a split.
    d.thresholds.material_quality_margin_millionths = 100_001;
    a.protocol_identity = v6_declaration_identity(d.clone());
    b.protocol_identity = v6_declaration_identity(d.clone());
    assert!(matches!(
        evaluate_promotion(d, candidate(A), a, candidate(B), b),
        PromotionVerdict::SplitTiers { .. }
    ));
}

// 6. v5 validated the quality plan for non-emptiness and candidate
//    consistency. That let a plan score one prompt twice and another never
//    while the equal-prompt macro rule averaged a set nobody declared.
#[test]
fn selection_coverage_is_enforced_against_the_corpus_and_the_run_plan() {
    assert!(validate_v6_declaration(protocol()).is_empty(), "polarity");

    // Two outputs in the same candidate x prompt x seed slot. Note this is
    // exactly the shape v5 could not see: every entry still names a real
    // candidate, a real prompt and a real seed.
    let mut collided = protocol();
    let stolen = collided.quality_plan[0].prompt_id.clone();
    collided.quality_plan[1].prompt_id = stolen.clone();
    let errs = validate_v6_declaration(collided);
    assert!(
        errs.iter()
            .any(|e| e.contains(&stolen) && e.contains("2 scored outputs")),
        "a doubled slot must be named, got {errs:?}"
    );
    assert!(
        errs.iter().any(|e| e.contains("0 scored outputs")),
        "and so must the prompt it was stolen from, got {errs:?}"
    );

    // An output with no matching run-plan slot.
    let mut orphan = protocol();
    orphan.quality_plan[0].seed = 44;
    assert!(
        !errors_mentioning(orphan, "matching warm run-plan slots").is_empty(),
        "a scored output must correspond to a run the plan actually schedules"
    );

    // One (candidate, seed) draws from ONE warm run.
    let mut split = protocol();
    split.quality_plan[0].run_id = "somewhere-else".into();
    assert!(!errors_mentioning(split, "more than one run").is_empty());

    // A blinded output id cannot appear twice, however well-formed the rest is.
    let mut dup = protocol();
    dup.quality_plan[1].blinded_output_id = dup.quality_plan[0].blinded_output_id.clone();
    assert!(!errors_mentioning(dup, "more than once").is_empty());
}

// 7. scorer_identity was a free String that nothing compared, so one ledger
//    could mix raters and still average cleanly.
#[test]
fn a_ledger_mixing_scorer_identities_is_rejected() {
    let d = protocol();
    assert!(
        matches!(
            derive_quality_panel(d.clone(), candidate(A), quality_observations(A, 800_000)),
            QualityDerivation::Derived { .. }
        ),
        "polarity: one scorer throughout derives normally"
    );

    // One mark by someone else.
    let mut mixed = quality_observations(A, 800_000);
    mixed[0].scorer_identity_digest = sha(0x02);
    assert!(matches!(
        derive_quality_panel(d.clone(), candidate(A), mixed),
        QualityDerivation::Rejected {
            failure: QualityAdmissionFailure::ScorerIdentityMismatch { .. }
        }
    ));

    // A UNIFORMLY wrong ledger is rejected too. Each mark is compared against
    // the declaration rather than against its neighbours, so a ledger scored
    // end to end by an undeclared panel cannot pass by being self-consistent.
    let mut wholesale = quality_observations(A, 800_000);
    for o in wholesale.iter_mut() {
        o.scorer_identity_digest = sha(0x03);
    }
    assert!(matches!(
        derive_quality_panel(d.clone(), candidate(A), wholesale),
        QualityDerivation::Rejected {
            failure: QualityAdmissionFailure::ScorerIdentityMismatch { .. }
        }
    ));

    // And it is caught at admission, not only by the derivation helper.
    let mut r = result(A, 800_000, 400_000_000);
    r.quality_observations[2].scorer_identity_digest = sha(0x04);
    assert!(matches!(
        admit_result(r, candidate(A), d),
        Some(AdmissionFailure::QualityLedgerRejected { .. })
    ));
}

// 8. Authority comes from the ABSENCE of an injection point — the same reason
//    model_pack::RestoreResult is output-only.
//
//    This is ONE OF THREE LAYERS, and on its own it is the weakest:
//
//      * paired compile_fail/compile doctests in the module header, which
//        prove the field and the two-argument identity function do not exist
//        and cannot be written;
//      * this scan, which catches a digest reintroduced under a different
//        spelling that a fixed compile-fail set would not name;
//      * the runtime check in validate_v6_declaration that the corpus IS the
//        build-owned one, because a caller can otherwise submit a different
//        shape-valid corpus and identify it perfectly honestly.
//
//    None of the three subsumes the others. The scan below deliberately
//    ignores comment text: the compile-fail fixtures have to NAME
//    `corpus_sha256_hex` in order to prove it will not compile, and a scan
//    that tripped over its own evidence would be unmaintainable.
const MODULE_SOURCE: &str = include_str!("../src/generation_eval.rs");

#[test]
fn no_public_api_accepts_a_corpus_digest_alongside_corpus_content() {
    // `corpus_identity` is deliberately absent from this list: deriving an
    // identity FROM content is the point. What must not exist is a way to
    // supply one.
    let banned = [
        "corpus_sha256",
        "corpusSha256",
        "corpus_digest",
        "corpusDigest",
        "corpus_hash",
        "expected_corpus",
    ];
    for (n, line) in MODULE_SOURCE.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for needle in banned {
            assert!(
                !code.contains(needle),
                "generation_eval.rs:{}: `{needle}` reintroduces a corpus digest the caller can assert",
                n + 1
            );
        }
    }

    // The declaration's serialized shape carries the corpus by value and
    // nothing else about it. A digest field would appear here.
    let value = serde_json::to_value(protocol()).expect("declaration serializes");
    let corpus = value
        .get("corpus")
        .expect("the corpus is embedded by value");
    let mut keys: Vec<&String> = corpus.as_object().expect("object").keys().collect();
    keys.sort();
    assert_eq!(keys, vec!["corpusId", "prompts"]);
    for key in value.as_object().expect("object").keys() {
        assert!(
            !key.to_lowercase().contains("corpussha")
                && !key.to_lowercase().contains("corpusdigest"),
            "{key} is a corpus digest on the declaration"
        );
    }

    // Polarity: the identity IS derived, and it moves with the content.
    let mut edited = corpus_from_test_edit();
    assert_ne!(
        corpus_identity(edited.clone()),
        corpus_identity(frozen_corpus_v1())
    );
    edited.prompts = frozen_corpus_v1().prompts;
    assert_eq!(corpus_identity(edited), corpus_identity(frozen_corpus_v1()));
}

fn corpus_from_test_edit() -> BenchmarkCorpus {
    let mut c = frozen_corpus_v1();
    c.prompts[0].message.push_str(" (edited)");
    c
}

// ============================================================================
// v6-1 hardening: content-addressed is not the same as build-owned.
//
// The first pass of #7 removed the forgeable digest and stopped there. That
// left a real gap: a caller could submit a DIFFERENT eighteen-prompt corpus
// satisfying all nine quotas, derive its identity perfectly honestly, and pass
// validation. Nothing forged, reviewed corpus replaced anyway.
//
// Every test below is written so that the SHAPE checks pass and only the
// AUTHORITY check fires. A test that let both fire would not distinguish the
// new check from the ones that already existed.
// ============================================================================

/// Shape-valid and not the build-owned corpus: same ids, same taxonomy, same
/// nine pair counts, different words.
fn shape_valid_impostor() -> BenchmarkCorpus {
    let mut c = frozen_corpus_v1();
    c.corpus_id = "m7b-impostor-v1".into();
    for p in c.prompts.iter_mut() {
        p.message = format!(
            "A different question for {}, under the same pair.",
            p.prompt_id
        );
    }
    c
}

fn assert_shape_checks_are_silent(errs: &[String]) {
    for shape in [
        "quota requires exactly",
        "outside the allowed matrix",
        "normative nine-pair matrix",
        "promptCount disagrees",
        "duplicate promptId",
        "one entry per allowed pair",
        "zero quota",
        "is blank",
        "must be non-empty",
    ] {
        assert!(
            !errs.iter().any(|e| e.contains(shape)),
            "the shape check {shape:?} fired, so this case does not isolate the \
             authority check; errors were {errs:?}"
        );
    }
}

// 1. One word changed. Every quota still satisfied.
#[test]
fn editing_a_single_prompt_message_is_rejected_even_though_the_quotas_still_hold() {
    let mut edited = protocol();
    edited.corpus.prompts[0]
        .message
        .push_str(" Also mention the weather.");
    let errs = validate_v6_declaration(edited);
    assert!(
        errs.iter()
            .any(|e| e.contains("not the build-owned m7b corpus")),
        "got {errs:?}"
    );
    assert_shape_checks_are_silent(&errs);
}

// 2. A wholesale replacement that is shape-valid in every respect.
#[test]
fn a_shape_valid_replacement_corpus_is_rejected_as_not_build_owned() {
    let impostor = shape_valid_impostor();
    assert_ne!(impostor, frozen_corpus_v1());
    assert_eq!(impostor.prompts.len(), 18);

    let mut swapped = protocol();
    swapped.corpus = impostor.clone();
    swapped.prompt_count = impostor.prompts.len() as u32;
    let errs = validate_v6_declaration(swapped);
    assert!(
        errs.iter()
            .any(|e| e.contains("not the build-owned m7b corpus")),
        "a corpus can be honestly identified and still not be ours; got {errs:?}"
    );
    assert_shape_checks_are_silent(&errs);

    // The impostor's identity is derived just as honestly as the real one's —
    // which is exactly why content-addressing alone was not enough.
    assert_ne!(
        corpus_identity(impostor),
        corpus_identity(frozen_corpus_v1())
    );

    // Polarity: the build-owned corpus validates.
    assert!(validate_v6_declaration(protocol()).is_empty());
}

// 3. Correct domain prefix, different version. The prefix check passes; the
//    identity is still not this evaluator's.
#[test]
fn a_correctly_domained_but_unknown_policy_id_is_rejected() {
    for (name, mutate) in [
        ("selectionPolicyId", 0usize),
        ("qualityAggregationPolicyId", 1),
        ("promotionRuleId", 2),
    ] {
        let mut d = protocol();
        match mutate {
            0 => d.policies.selection_policy_id = "neuralcompose.selection-policy.v6-2".into(),
            1 => {
                d.policies.quality_aggregation_policy_id =
                    "neuralcompose.quality-aggregation-policy.v7-0".into()
            }
            _ => d.policies.promotion_rule_id = "neuralcompose.promotion-rule.v6-1-hotfix".into(),
        }
        let errs = validate_v6_declaration(d);
        assert!(
            errs.iter()
                .any(|e| e.contains("do not name this evaluator")),
            "{name}: got {errs:?}"
        );
        // The prefix check must stay SILENT here, or this test would pass
        // without the exact check existing at all.
        assert!(
            !errs
                .iter()
                .any(|e| e.contains("must be a versioned id under")),
            "{name}: the prefix check fired, so this case does not isolate the \
             exact identity check; errors were {errs:?}"
        );
    }
}

// 4. The sharpest one: quota map and corpus edited TOGETHER so they remain
//    mutually consistent. Every shape check is satisfied by construction.
#[test]
fn a_mutually_consistent_corpus_and_quota_pair_is_still_rejected_if_it_is_not_ours() {
    let mut d = protocol();
    let widened = d.composition_policy.exact_required_count_per_pair[0].pair;
    d.composition_policy.exact_required_count_per_pair[0].exact_count = 3;
    d.corpus.prompts.push(CorpusPrompt {
        prompt_id: "p19".into(),
        task_kind: widened.task_kind,
        context_profile: widened.context_profile,
        message: "A third prompt for a pair the quota was widened to want three of.".into(),
    });
    d.prompt_count = d.corpus.prompts.len() as u32;

    let errs = validate_v6_declaration(d);
    assert!(
        errs.iter()
            .any(|e| e.contains("not the build-owned m7b corpus")),
        "got {errs:?}"
    );
    assert!(
        errs.iter().any(|e| e.contains("not the frozen v6 policy")),
        "got {errs:?}"
    );
    // Nothing about the SHAPE is wrong: three required, three present, and the
    // count agrees. Before the authority check existed, this declaration was
    // accepted — a reweighted study wearing a valid-looking declaration.
    assert_shape_checks_are_silent(&errs);
}

// The shape checks are not made redundant by the authority check: they are
// what catches a bad edit to the committed artifact ITSELF, where
// frozen_corpus_v1() returns the bad corpus and equality holds.
#[test]
fn the_shape_checks_still_guard_the_committed_artifact_itself() {
    // Simulate the artifact having been edited badly: the "frozen" corpus and
    // the declaration's corpus agree, so authority passes and only shape can
    // catch it. Constructed by making the QUOTA disagree with a corpus that is
    // genuinely the build-owned one.
    let mut d = protocol();
    d.composition_policy.exact_required_count_per_pair[0].exact_count = 5;
    let errs = validate_v6_declaration(d);
    assert!(
        errs.iter().any(|e| e.contains("quota requires exactly 5")),
        "a quota that the build-owned corpus does not satisfy must still be \
         named by the shape check, got {errs:?}"
    );
}
