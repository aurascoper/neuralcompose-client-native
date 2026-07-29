// M7-B frozen benchmark protocol regressions (v3). Freezing exists so a
// threshold cannot move after a result is seen, an inadmissible run cannot be
// promoted, and a run taken under other conditions cannot borrow the
// protocol's hash. Both polarities throughout.

use neuralcompose_mobile_core::generation_eval::*;

const A: &str = "qwen2.5-0.5b-instruct-q4km";
const B: &str = "qwen3-0.6b-q4km-derived";

fn sha(n: u8) -> String {
    format!("{:02x}", n).repeat(32)
}

fn corpus_hash() -> String {
    semantic_prompt_hash("focused".into(), "Summarize.".into())
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
        material_quality_margin: 0.05,
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
        alternating_candidate_order: true,
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
    for seed in [11u64, 22, 33] {
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

fn protocol() -> EvaluationProtocol {
    EvaluationProtocol {
        protocol_version: 3,
        corpus_id: "m7b-sanitized-v1".into(),
        corpus_sha256_hex: sha(0xa1),
        prompt_count: 1,
        quality_rubric_id: "m7b-rubric-v1".into(),
        sampler: sampler(),
        seeds: vec![11, 22, 33],
        warmup_runs: 1,
        timed_runs: 3,
        sustained_seconds: 300,
        per_run_timeout_ms: 60_000,
        thresholds: thresholds(),
        environment: environment(),
        expected_prompts: vec![ExpectedPrompt {
            prompt_id: "p1".into(),
            semantic_prompt_hash: corpus_hash(),
        }],
        run_plan: run_plan(),
    }
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

fn bindings(tag: u8) -> Vec<PromptBinding> {
    vec![PromptBinding {
        prompt_id: "p1".into(),
        rendered_prompt_hash: sha(tag),
        input_token_ids_hash: sha(tag.wrapping_add(1)),
    }]
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
        prompt_bindings: bindings(if derived { 0x80 } else { 0x70 }),
    }
}

fn panel(score: f64) -> QualityPanel {
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
        pack_integrity_receipt: candidate(cand).artifact_sha256_hex,
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

fn result(id: &str, score: f64, installed_bytes: u64) -> CandidateResult {
    let cand = candidate(id);
    let tag = if id == B { 0x80u8 } else { 0x70u8 };
    let runs: Vec<(u32, RunMode, u64)> = run_plan()
        .iter()
        .filter(|e| e.candidate_id == id)
        .map(|e| (e.index, e.mode, e.seed))
        .collect();
    CandidateResult {
        candidate_id: id.into(),
        candidate_identity: candidate_identity(cand),
        protocol_identity: evaluation_protocol_identity(protocol()),
        device: "Pixel 8a".into(),
        os_version: "17".into(),
        runtime_identity: "llama.cpp@f5b9bd39".into(),
        prompts: vec![BenchmarkPrompt {
            prompt_id: "p1".into(),
            prompt_profile: "focused".into(),
            semantic_prompt_hash: corpus_hash(),
            rendered_prompt_hash: sha(tag),
            input_token_ids_hash: sha(tag.wrapping_add(1)),
            output_hash: sha(tag.wrapping_add(2)),
        }],
        installed_bytes,
        quality_rubric_id: "m7b-rubric-v1".into(),
        quality: panel(score),
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
    assert!(validate_evaluation_protocol(protocol()).is_empty());
    let base = evaluation_protocol_identity(protocol());
    type Mutation = fn(&mut EvaluationProtocol);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("corpus digest", |p| p.corpus_sha256_hex = sha(0xff)),
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
            p.thresholds.material_quality_margin = 0.0
        }),
        ("charging state", |p| {
            p.environment.charging_state = ChargingState::OnBattery
        }),
        ("cold definition", |p| {
            p.environment.cold_definition = ColdDefinition::FilesystemCacheCold
        }),
        ("airplane mode", |p| p.environment.airplane_mode = false),
        ("run plan", |p| p.run_plan.truncate(2)),
        ("corpus prompts", |p| {
            p.expected_prompts[0].semantic_prompt_hash = sha(0x12)
        }),
    ];
    for (name, mutate) in mutations {
        let mut p = protocol();
        mutate(&mut p);
        assert_ne!(
            evaluation_protocol_identity(p),
            base,
            "changing the {name} must change the protocol identity"
        );
    }
    // Seed ORDER is not a term; the seed SET is.
    let mut reordered = protocol();
    reordered.seeds = vec![33, 11, 22];
    assert_eq!(evaluation_protocol_identity(reordered), base);
}

// Hashing an invalid protocol does not make it valid.
#[test]
fn an_invalid_protocol_is_rejected_however_it_hashes() {
    type Mutation = fn(&mut EvaluationProtocol);
    let cases: Vec<(&str, Mutation)> = vec![
        ("unsupported version", |p| p.protocol_version = 2),
        ("empty seeds", |p| p.seeds.clear()),
        ("duplicate seeds", |p| p.seeds = vec![11, 11]),
        ("no prompts", |p| {
            p.expected_prompts.clear();
            p.prompt_count = 0
        }),
        ("prompt count disagrees", |p| p.prompt_count = 9),
        ("empty run plan", |p| p.run_plan.clear()),
        ("sparse run plan", |p| p.run_plan[0].index = 7),
        ("zero timeout", |p| p.per_run_timeout_ms = 0),
        ("NaN temperature", |p| p.sampler.temperature = f64::NAN),
        ("topP above 1", |p| p.sampler.top_p = 1.5),
        ("zero context cap", |p| p.sampler.context_cap = 0),
        ("zero ceiling", |p| p.thresholds.max_peak_rss_mb = 0),
        ("negative margin", |p| {
            p.thresholds.material_quality_margin = -1.0
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
            !validate_evaluation_protocol(p).is_empty(),
            "{name} must be rejected"
        );
    }
    assert!(
        validate_evaluation_protocol(protocol()).is_empty(),
        "polarity"
    );
}

// ---------- the sealed door ----------

// There is no promotion path that skips admission.
#[test]
fn an_inadmissible_run_cannot_be_promoted() {
    let mut leaked = result(B, 0.99, 400_000_000);
    leaked.disposition = RunDisposition::InadmissibleReasoningLeakage {
        detector: "frozen-prose-v1".into(),
    };
    // Despite the highest possible quality panel, it promotes nothing.
    match evaluate_promotion(
        protocol(),
        candidate(A),
        result(A, 0.5, 400_000_000),
        candidate(B),
        leaked,
    ) {
        PromotionVerdict::PromoteNothing { reason } => {
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
        let mut r = result(B, 0.99, 400_000_000);
        r.disposition = d;
        assert!(matches!(
            evaluate_promotion(
                protocol(),
                candidate(A),
                result(A, 0.5, 400_000_000),
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
            result(A, 0.5, 400_000_000),
            candidate(B),
            result(B, 0.9, 400_000_000)
        ),
        PromotionVerdict::PromoteNothing { .. }
    ));
}

// A result must actually belong to the candidate it is scored against.
#[test]
fn results_are_bound_to_their_candidate_and_prompts() {
    let p = protocol();
    assert_eq!(
        admit_result(result(A, 0.8, 400_000_000), candidate(A), p.clone()),
        None
    );
    // The same result cannot be admitted against the OTHER candidate.
    assert_eq!(
        admit_result(result(A, 0.8, 400_000_000), candidate(B), p.clone()),
        Some(AdmissionFailure::ResultCandidateMismatch)
    );
    // A forged identity is caught even when the id matches.
    let mut forged = result(A, 0.8, 400_000_000);
    forged.candidate_identity = sha(0x01);
    assert_eq!(
        admit_result(forged, candidate(A), p.clone()),
        Some(AdmissionFailure::CandidateIdentityMismatch)
    );
    // An empty prompt list is no longer admissible.
    let mut no_prompts = result(A, 0.8, 400_000_000);
    no_prompts.prompts.clear();
    assert_eq!(
        admit_result(no_prompts, candidate(A), p.clone()),
        Some(AdmissionFailure::PromptCountMismatch)
    );
    let mut dup = result(A, 0.8, 400_000_000);
    dup.prompts.push(dup.prompts[0].clone());
    assert!(matches!(
        admit_result(dup, candidate(A), p.clone()),
        Some(AdmissionFailure::PromptCountMismatch)
    ));
    let mut wrong_semantic = result(A, 0.8, 400_000_000);
    wrong_semantic.prompts[0].semantic_prompt_hash = sha(0x13);
    assert_eq!(
        admit_result(wrong_semantic, candidate(A), p.clone()),
        Some(AdmissionFailure::SemanticPromptMismatch)
    );
    // Rendered and token parity are candidate-specific and declared ahead.
    let mut wrong_render = result(A, 0.8, 400_000_000);
    wrong_render.prompts[0].rendered_prompt_hash = sha(0x99);
    assert!(matches!(
        admit_result(wrong_render, candidate(A), p.clone()),
        Some(AdmissionFailure::RenderedPromptMismatch { .. })
    ));
    let mut wrong_tokens = result(A, 0.8, 400_000_000);
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
    let mut missing = result(A, 0.8, 400_000_000);
    missing.observations.pop();
    assert!(matches!(
        admit_result(missing, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    let mut reordered = result(A, 0.8, 400_000_000);
    reordered.observations.reverse();
    assert!(matches!(
        admit_result(reordered, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    let mut wrong_seed = result(A, 0.8, 400_000_000);
    wrong_seed.observations[0].seed = 99;
    assert!(matches!(
        admit_result(wrong_seed, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    // A run with no integrity receipt is not evidence.
    let mut no_receipt = result(A, 0.8, 400_000_000);
    no_receipt.observations[0].pack_integrity_receipt = "  ".into();
    assert!(matches!(
        admit_result(no_receipt, candidate(A), p.clone()),
        Some(AdmissionFailure::RunLedgerMismatch { .. })
    ));
    // A single bad run poisons the candidate even if the aggregate looks fine.
    let mut one_bad = result(A, 0.8, 400_000_000);
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
        let mut r = result(A, 0.8, 400_000_000);
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
    let mut r = result(A, 0.8, 400_000_000);
    r.protocol_identity = evaluation_protocol_identity(fs_cold.clone());
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
        result(A, 0.80, small),
        candidate(B),
        result(B, 0.80, large),
    ) {
        PromotionVerdict::SplitTiers {
            basic_candidate_id,
            enhanced_candidate_id,
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
            result(A, 0.80, small),
            candidate(B),
            result(B, 0.83, large)
        ),
        PromotionVerdict::SplitTiers { .. }
    ));
    // A material margin promotes, in both directions.
    assert!(matches!(
        evaluate_promotion(protocol(), candidate(A), result(A, 0.80, small),
                           candidate(B), result(B, 0.90, large)),
        PromotionVerdict::Promote { ref candidate_id, .. } if candidate_id == B
    ));
    assert!(matches!(
        evaluate_promotion(protocol(), candidate(A), result(A, 0.95, small),
                           candidate(B), result(B, 0.80, large)),
        PromotionVerdict::Promote { ref candidate_id, .. } if candidate_id == A
    ));
}

/// A ceiling can now only be blown through the LEDGER — the aggregate is
/// derived, so there is no way to inject a slow summary directly.
fn slow_result(id: &str, score: f64) -> CandidateResult {
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
            slow_result(A, 0.9),
            candidate(B),
            slow_result(B, 0.95)
        ),
        PromotionVerdict::PromoteNothing { .. }
    ));
    // The passing candidate wins even with the LOWER score.
    assert!(matches!(
        evaluate_promotion(protocol(), candidate(A), result(A, 0.60, 400_000_000),
                           candidate(B), slow_result(B, 0.99)),
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
    assert_eq!(quality_score(panel(0.5)), Some(0.5));
    for bad in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        let mut p = panel(0.8);
        p.avoids_repetition = bad;
        assert_eq!(quality_score(p), None, "{bad} must not score");
    }
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
    let mut r = result(B, 0.8, 400_000_000);
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
    let mut r2 = result(B, 0.8, 400_000_000);
    r2.candidate_identity = candidate_identity(requant.clone());
    assert_eq!(
        admit_result(r2, requant, p.clone()),
        Some(AdmissionFailure::RequantizationNotAllowed)
    );
    // Qwen3 defaults to thinking mode: it must be PROVEN off.
    let mut thinking = candidate(B);
    thinking.thinking_mode_disabled = false;
    let mut r3 = result(B, 0.8, 400_000_000);
    r3.candidate_identity = candidate_identity(thinking.clone());
    assert_eq!(
        admit_result(r3, thinking, p),
        Some(AdmissionFailure::ThinkingModeNotDisabled)
    );
}

#[test]
fn semantic_and_rendered_prompt_identities_are_separate() {
    let a = semantic_prompt_hash("focused".into(), "Summarize this.".into());
    assert_eq!(
        a,
        semantic_prompt_hash("focused".into(), "Summarize this.".into()),
        "the same semantic input must hash identically for both models"
    );
    // Different templates render different bytes from one input — legitimate,
    // and it must stay visible.
    assert_ne!(
        rendered_prompt_hash("<|im_start|>assistant\n".into()),
        rendered_prompt_hash("<|im_start|>assistant\n<think>\n\n</think>\n\n".into())
    );
    assert_ne!(
        semantic_prompt_hash("reflective".into(), "Summarize this.".into()),
        a
    );
}

#[test]
fn the_environment_requires_its_bias_controls() {
    assert!(validate_run_environment(environment()).is_empty());
    type Mutation = fn(&mut RunEnvironment);
    let required: Vec<(&str, Mutation)> = vec![
        ("alternating order", |e| {
            e.alternating_candidate_order = false
        }),
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
