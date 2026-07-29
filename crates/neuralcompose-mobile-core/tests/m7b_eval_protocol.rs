// M7-B frozen benchmark protocol regressions. The point of freezing is that
// a threshold cannot move after a result is seen, and that "newer" and
// "smaller" are not arguments. Both polarities throughout.

use neuralcompose_mobile_core::generation_eval::*;

fn sha(n: u8) -> String {
    format!("{:02x}", n).repeat(32)
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
        max_battery_drop_tenths_percent: 60,
        thermal_cutoff_celsius_tenths: 450,
        material_quality_margin: 0.05,
    }
}

fn protocol() -> EvaluationProtocol {
    EvaluationProtocol {
        protocol_version: 1,
        corpus_id: "m7b-sanitized-v1".into(),
        corpus_sha256_hex: sha(0xa1),
        prompt_count: 24,
        quality_rubric_id: "m7b-rubric-v1".into(),
        sampler: sampler(),
        seeds: vec![11, 22, 33],
        warmup_runs: 1,
        timed_runs: 3,
        sustained_seconds: 300,
        per_run_timeout_ms: 60_000,
        thresholds: thresholds(),
    }
}

fn conversion() -> ConversionRecord {
    ConversionRecord {
        source_repo: "Qwen/Qwen3-0.6B".into(),
        source_revision: "pinned-revision-sha".into(),
        conversion_commit: "llamacpp-pinned-sha".into(),
        quantizer_commit: "llamacpp-pinned-sha".into(),
        conversion_command: "python convert_hf_to_gguf.py <src> --outfile <int> --outtype auto"
            .into(),
        quantize_command: "llama-quantize <int> <out> Q4_K_M".into(),
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

fn qwen25_q4() -> EvaluationCandidate {
    EvaluationCandidate {
        candidate_id: "qwen2.5-0.5b-instruct-q4km".into(),
        model_family: "qwen2.5".into(),
        model_revision: "0.5b-instruct".into(),
        quantization: "Q4_K_M".into(),
        variant_id: "gguf-q4km-android-arm64".into(),
        role: CandidateRole::PrimaryMobile,
        provenance: ArtifactProvenance::OfficialUpstream,
        artifact_sha256_hex: sha(0xc1),
        tokenizer_identity: sha(0xd1),
        chat_template_identity: sha(0xe1),
        thinking_mode_disabled: true,
        conversion: None,
    }
}

fn qwen3_q4() -> EvaluationCandidate {
    EvaluationCandidate {
        candidate_id: "qwen3-0.6b-q4km-derived".into(),
        model_family: "qwen3".into(),
        model_revision: "0.6b".into(),
        quantization: "Q4_K_M".into(),
        variant_id: "gguf-q4km-android-arm64".into(),
        role: CandidateRole::PrimaryMobile,
        // No official Q4_K_M exists for Qwen3-0.6B, so this one is derived.
        provenance: ArtifactProvenance::DerivedByConversion,
        artifact_sha256_hex: sha(0xc2),
        tokenizer_identity: sha(0xd2),
        chat_template_identity: sha(0xe2),
        thinking_mode_disabled: true,
        conversion: Some(conversion()),
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

fn result(id: &str, score: f64, c: CostObservation) -> CandidateResult {
    CandidateResult {
        candidate_id: id.into(),
        candidate_identity: sha(0x01),
        protocol_identity: evaluation_protocol_identity(protocol()),
        device: "Pixel 8a".into(),
        os_version: "17".into(),
        runtime_identity: "llama.cpp@pinned".into(),
        prompts: vec![],
        cost: c,
        quality: panel(score),
        disqualified_reason: None,
    }
}

// The whole point of freezing: no term can move without changing identity.
#[test]
fn the_protocol_identity_pins_every_term() {
    let base = evaluation_protocol_identity(protocol());
    type Mutation = fn(&mut EvaluationProtocol);
    let mutations: Vec<(&str, Mutation)> = vec![
        ("corpus digest", |p| p.corpus_sha256_hex = sha(0xff)),
        ("rubric", |p| p.quality_rubric_id = "other".into()),
        ("temperature", |p| p.sampler.temperature = 0.9),
        ("max output tokens", |p| p.sampler.max_output_tokens = 512),
        ("seeds", |p| p.seeds = vec![11, 22, 34]),
        ("timed runs", |p| p.timed_runs = 5),
        ("sustained seconds", |p| p.sustained_seconds = 60),
        // The one that matters most: relaxing a ceiling after seeing a
        // result must not be able to masquerade as the same protocol.
        ("latency ceiling", |p| {
            p.thresholds.max_time_to_first_token_ms = 9000
        }),
        ("memory ceiling", |p| p.thresholds.max_peak_rss_mb = 8000),
        ("material margin", |p| {
            p.thresholds.material_quality_margin = 0.0
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
    // Seed ORDER is not a term — the same set is the same protocol.
    let mut reordered = protocol();
    reordered.seeds = vec![33, 11, 22];
    assert_eq!(evaluation_protocol_identity(reordered), base);
    assert_eq!(evaluation_protocol_identity(protocol()), base, "stable");
}

// The conversion asymmetry is recorded, never hidden.
#[test]
fn candidate_identity_records_how_the_artifact_came_to_exist() {
    let official = candidate_identity(qwen25_q4());
    let derived = candidate_identity(qwen3_q4());
    assert_ne!(official, derived);
    // Same bytes, different provenance claim → different identity. An
    // official download and a local conversion are not interchangeable.
    let mut relabelled = qwen25_q4();
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
        ("quantize command", |c| c.quantize_command = "Q5_K_M".into()),
    ];
    for (name, mutate) in links {
        let mut cand = qwen3_q4();
        let mut conv = conversion();
        mutate(&mut conv);
        cand.conversion = Some(conv);
        assert_ne!(
            candidate_identity(cand),
            derived,
            "the {name} is part of the derived artifact's identity"
        );
    }
}

// Admission is separate from winning: an inadmissible run is not evidence.
#[test]
fn admission_rejects_unreproducible_or_unproven_candidates() {
    let pid = evaluation_protocol_identity(protocol());
    let ok = result("a", 0.8, cost());
    assert_eq!(admit_result(ok.clone(), qwen25_q4(), pid.clone()), None);
    assert_eq!(admit_result(ok.clone(), qwen3_q4(), pid.clone()), None);

    // A derived artifact with no conversion record cannot be reproduced.
    let mut no_record = qwen3_q4();
    no_record.conversion = None;
    assert_eq!(
        admit_result(ok.clone(), no_record, pid.clone()),
        Some(AdmissionFailure::MissingConversionRecord)
    );
    // An "official" artifact carrying a conversion record is incoherent.
    let mut official_but_converted = qwen25_q4();
    official_but_converted.conversion = Some(conversion());
    assert_eq!(
        admit_result(ok.clone(), official_but_converted, pid.clone()),
        Some(AdmissionFailure::UnexpectedConversionRecord)
    );
    // Requantizing an already-quantized file degrades quality; refused.
    let mut requantized = qwen3_q4();
    let mut conv = conversion();
    conv.allow_requantize = true;
    requantized.conversion = Some(conv);
    assert_eq!(
        admit_result(ok.clone(), requantized, pid.clone()),
        Some(AdmissionFailure::RequantizationNotAllowed)
    );
    // Qwen3 defaults to thinking mode: it must be PROVEN off.
    let mut thinking = qwen3_q4();
    thinking.thinking_mode_disabled = false;
    assert_eq!(
        admit_result(ok.clone(), thinking, pid.clone()),
        Some(AdmissionFailure::ThinkingModeNotDisabled)
    );
    // A result produced under a different protocol is not comparable.
    let mut other_protocol = ok.clone();
    other_protocol.protocol_identity = sha(0x77);
    assert_eq!(
        admit_result(other_protocol, qwen25_q4(), pid.clone()),
        Some(AdmissionFailure::ProtocolMismatch)
    );
    // A throttled run is not a representative measurement.
    let mut hot = cost();
    hot.thermally_throttled = true;
    assert_eq!(
        admit_result(result("a", 0.8, hot), qwen25_q4(), pid.clone()),
        Some(AdmissionFailure::ThermallyThrottled)
    );
    // A malformed panel scores nothing rather than something.
    let mut bad_panel = ok.clone();
    bad_panel.quality.instruction_adherence = 1.4;
    assert_eq!(
        admit_result(bad_panel, qwen25_q4(), pid),
        Some(AdmissionFailure::MalformedQualityPanel)
    );
}

#[test]
fn quality_score_refuses_impossible_panels() {
    assert_eq!(quality_score(panel(0.5)), Some(0.5));
    assert_eq!(quality_score(panel(1.0)), Some(1.0));
    for bad in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        let mut p = panel(0.8);
        p.avoids_repetition = bad;
        assert_eq!(quality_score(p), None, "{bad} must not score");
    }
}

// "Newer wins" and "smaller wins" are both refused.
#[test]
fn promotion_requires_earning_it_not_being_newer_or_smaller() {
    let t = thresholds();
    // Equal quality, different size → a split, not a winner.
    let mut small = cost();
    small.installed_bytes = 300_000_000;
    let mut large = cost();
    large.installed_bytes = 600_000_000;
    match evaluate_promotion(
        result("qwen2.5", 0.80, small.clone()),
        result("qwen3", 0.80, large.clone()),
        t.clone(),
    ) {
        PromotionVerdict::SplitTiers {
            basic_candidate_id,
            enhanced_candidate_id,
        } => {
            assert_eq!(basic_candidate_id, "qwen2.5", "smaller becomes Basic");
            assert_eq!(enhanced_candidate_id, "qwen3");
        }
        other => panic!("expected a split, got {other:?}"),
    }
    // A margin below the frozen threshold is still a split — a larger model
    // must EARN its size.
    assert!(matches!(
        evaluate_promotion(
            result("qwen2.5", 0.80, small.clone()),
            result("qwen3", 0.83, large.clone()),
            t.clone()
        ),
        PromotionVerdict::SplitTiers { .. }
    ));
    // A material margin does promote.
    match evaluate_promotion(
        result("qwen2.5", 0.80, small.clone()),
        result("qwen3", 0.90, large.clone()),
        t.clone(),
    ) {
        PromotionVerdict::Promote {
            candidate_id,
            margin,
            ..
        } => {
            assert_eq!(candidate_id, "qwen3");
            assert!((margin - 0.10).abs() < 1e-9);
        }
        other => panic!("expected promotion, got {other:?}"),
    }
    // ...and in the other direction, so the rule is not one-sided.
    assert!(matches!(
        evaluate_promotion(
            result("qwen2.5", 0.95, small),
            result("qwen3", 0.80, large),
            t
        ),
        PromotionVerdict::Promote { candidate_id, .. } if candidate_id == "qwen2.5"
    ));
}

// "No model promoted" is a legitimate outcome.
#[test]
fn both_failing_promotes_nothing() {
    let t = thresholds();
    let mut slow = cost();
    slow.time_to_first_token_ms = 9_000;
    assert!(matches!(
        evaluate_promotion(
            result("qwen2.5", 0.9, slow.clone()),
            result("qwen3", 0.95, slow.clone()),
            t.clone()
        ),
        PromotionVerdict::PromoteNothing { .. }
    ));
    // A high quality score cannot rescue a candidate that blew a ceiling:
    // the one that passed wins even with the LOWER score.
    match evaluate_promotion(
        result("qwen2.5", 0.60, cost()),
        result("qwen3", 0.99, slow),
        t.clone(),
    ) {
        PromotionVerdict::Promote { candidate_id, .. } => assert_eq!(candidate_id, "qwen2.5"),
        other => panic!("expected the passing candidate, got {other:?}"),
    }
    // A malformed panel promotes nothing rather than defaulting to a winner.
    let mut broken = result("qwen3", 0.9, cost());
    broken.quality.language_preservation = f64::NAN;
    assert!(matches!(
        evaluate_promotion(result("qwen2.5", 0.9, cost()), broken, t),
        PromotionVerdict::PromoteNothing { .. }
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
        ("batteryDrop", |c| c.battery_drop_tenths_percent = 900),
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
        assert!(
            failures.iter().any(|f| f == name),
            "{name} must be reported as a failure, got {failures:?}"
        );
    }
}

// The semantic input is shared; the rendered bytes may differ by template.
#[test]
fn semantic_and_rendered_prompt_identities_are_separate() {
    let semantic_a = semantic_prompt_hash("focused".into(), "Summarize this.".into());
    let semantic_b = semantic_prompt_hash("focused".into(), "Summarize this.".into());
    assert_eq!(
        semantic_a, semantic_b,
        "the same semantic input must hash identically for both models"
    );
    // Different chat templates render different bytes from that one input —
    // legitimate, and it must remain visible.
    let rendered_qwen25 = rendered_prompt_hash(
        "<|im_start|>user\nSummarize this.<|im_end|>\n<|im_start|>assistant\n".into(),
    );
    let rendered_qwen3 = rendered_prompt_hash(
        "<|im_start|>user\nSummarize this.<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
            .into(),
    );
    assert_ne!(rendered_qwen25, rendered_qwen3);
    // A changed semantic message is a different benchmark, not a rendering
    // detail.
    assert_ne!(
        semantic_prompt_hash("focused".into(), "Summarize this".into()),
        semantic_a
    );
    assert_ne!(
        semantic_prompt_hash("reflective".into(), "Summarize this.".into()),
        semantic_a
    );
}
