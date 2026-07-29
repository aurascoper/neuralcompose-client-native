// M7-A2 backend semantic conformance (ADR-002). Backends are never declared
// mathematically equivalent; they either satisfy a pre-registered contract or
// publish under a different one.

use neuralcompose_mobile_core::conformance::*;

type Mutation<T> = Box<dyn Fn(&mut T)>;

fn policy() -> BackendConformancePolicy {
    BackendConformancePolicy {
        numerical_contract_id: "nc-gguf-q4-v1".into(),
        tokenizer_identity: "tok-abc".into(),
        prompt_byte_identity: prompt_byte_identity("focused".into(), "hello".into()),
        stop_token_identity: "stop-v1".into(),
        context_cap: 4096,
        deterministic_test_mode: true,
        logits_tolerance: 1e-3,
        embedding_tolerance: 1e-4,
        generated_token_policy: GeneratedTokenPolicy::ExactUnderGreedy,
    }
}

fn observation() -> BackendObservation {
    let p = policy();
    BackendObservation {
        backend_id: "llama-cpp-vulkan".into(),
        tokenizer_identity: p.tokenizer_identity,
        prompt_byte_identity: p.prompt_byte_identity,
        stop_token_identity: p.stop_token_identity,
        context_cap: p.context_cap,
        output_shape_declared: true,
        max_logit_divergence: Some(1e-5),
        max_embedding_divergence: Some(1e-6),
        greedy_determinism_available: true,
        generated_tokens_match_reference: Some(true),
    }
}

#[test]
fn conformant_backend_may_carry_the_contract() {
    assert!(validate_conformance_policy(policy()).is_empty());
    assert_eq!(
        evaluate_backend_conformance(policy(), observation()),
        ConformanceVerdict::Conformant
    );
    // Two independently conformant backends may share one contract id.
    let cuda = BackendObservation {
        backend_id: "llama-cpp-cuda".into(),
        max_logit_divergence: Some(9e-4), // inside tolerance
        ..observation()
    };
    assert!(may_share_numerical_contract(
        policy(),
        observation(),
        cuda.clone()
    ));
    // A backend cannot "share" a contract with itself — that claims nothing.
    assert!(!may_share_numerical_contract(
        policy(),
        observation(),
        observation()
    ));
    // Nor may a non-conformant one be quietly admitted.
    let drifted = BackendObservation {
        backend_id: "windows-ml-openvino".into(),
        max_logit_divergence: Some(0.5),
        ..observation()
    };
    assert!(!may_share_numerical_contract(
        policy(),
        observation(),
        drifted
    ));
}

// PRIMARY: exceeding tolerance is a DIFFERENT contract, not a passed test
// and not a silent failure.
#[test]
fn tolerance_excess_demands_a_separate_numerical_contract() {
    let drifted = BackendObservation {
        max_logit_divergence: Some(0.25),
        ..observation()
    };
    match evaluate_backend_conformance(policy(), drifted) {
        ConformanceVerdict::RequiresSeparateNumericalContract {
            measurement,
            observed,
            tolerance,
        } => {
            assert_eq!(measurement, "logits");
            assert_eq!(observed, 0.25);
            assert_eq!(tolerance, 1e-3);
        }
        other => panic!("expected separate-contract verdict, got {other:?}"),
    }
    let emb = BackendObservation {
        max_embedding_divergence: Some(0.1),
        ..observation()
    };
    assert!(matches!(
        evaluate_backend_conformance(policy(), emb),
        ConformanceVerdict::RequiresSeparateNumericalContract {
            measurement,
            ..
        } if measurement == "embeddings"
    ));
    // Polarity: exactly at tolerance is still conformant.
    let at_limit = BackendObservation {
        max_logit_divergence: Some(1e-3),
        ..observation()
    };
    assert_eq!(
        evaluate_backend_conformance(policy(), at_limit),
        ConformanceVerdict::Conformant
    );
}

// An unmeasured tolerance is not a satisfied tolerance.
#[test]
fn absent_measurements_never_count_as_agreement() {
    for (name, mutate) in [
        (
            "logits",
            Box::new(|o: &mut BackendObservation| o.max_logit_divergence = None)
                as Box<dyn Fn(&mut BackendObservation)>,
        ),
        (
            "embeddings",
            Box::new(|o: &mut BackendObservation| o.max_embedding_divergence = None),
        ),
    ] {
        let mut o = observation();
        mutate(&mut o);
        assert!(
            matches!(
                evaluate_backend_conformance(policy(), o),
                ConformanceVerdict::NonConformant {
                    failure: ConformanceFailure::MissingMeasurement { .. }
                }
            ),
            "missing {name} must not pass"
        );
    }
    // NaN is not a measurement either.
    let nan = BackendObservation {
        max_logit_divergence: Some(f64::NAN),
        ..observation()
    };
    assert!(matches!(
        evaluate_backend_conformance(policy(), nan),
        ConformanceVerdict::NonConformant {
            failure: ConformanceFailure::MissingMeasurement { .. }
        }
    ));
}

// Identity mismatches are fatal: no tolerance can excuse a different
// tokenizer, prompt bytes, stop configuration, or context cap.
#[test]
fn identity_mismatches_are_never_tolerable() {
    let cases: Vec<(ConformanceFailure, Mutation<BackendObservation>)> = vec![
        (
            ConformanceFailure::TokenizerIdentityMismatch,
            Box::new(|o: &mut BackendObservation| o.tokenizer_identity = "tok-other".into()),
        ),
        (
            ConformanceFailure::PromptByteMismatch,
            Box::new(|o: &mut BackendObservation| {
                o.prompt_byte_identity = prompt_byte_identity("focused".into(), "hello ".into())
            }),
        ),
        (
            ConformanceFailure::StopTokenMismatch,
            Box::new(|o: &mut BackendObservation| o.stop_token_identity = "stop-v2".into()),
        ),
        (
            ConformanceFailure::ContextCapMismatch,
            Box::new(|o: &mut BackendObservation| o.context_cap = 2048),
        ),
        (
            ConformanceFailure::OutputShapeUndeclared,
            Box::new(|o: &mut BackendObservation| o.output_shape_declared = false),
        ),
        (
            ConformanceFailure::GreedyDeterminismUnavailable,
            Box::new(|o: &mut BackendObservation| o.greedy_determinism_available = false),
        ),
        (
            ConformanceFailure::GeneratedTokensDiverged,
            Box::new(|o: &mut BackendObservation| o.generated_tokens_match_reference = Some(false)),
        ),
    ];
    for (expected, mutate) in cases {
        let mut o = observation();
        mutate(&mut o);
        assert_eq!(
            evaluate_backend_conformance(policy(), o),
            ConformanceVerdict::NonConformant {
                failure: expected.clone()
            },
            "expected {expected:?}"
        );
    }
}

// Sampled output is judged as a distribution, never by string equality.
#[test]
fn sampling_mode_does_not_require_token_equality() {
    let sampling = BackendConformancePolicy {
        generated_token_policy: GeneratedTokenPolicy::DistributionOnly,
        deterministic_test_mode: false,
        ..policy()
    };
    let sampled = BackendObservation {
        greedy_determinism_available: false,
        generated_tokens_match_reference: Some(false), // irrelevant here
        ..observation()
    };
    assert_eq!(
        evaluate_backend_conformance(sampling.clone(), sampled),
        ConformanceVerdict::Conformant,
        "distribution-only policy must not demand identical prose"
    );
    // Polarity: under greedy policy the same observation fails.
    assert!(matches!(
        evaluate_backend_conformance(
            policy(),
            BackendObservation {
                greedy_determinism_available: false,
                ..observation()
            }
        ),
        ConformanceVerdict::NonConformant { .. }
    ));
    // A greedy claim without deterministic mode is a malformed policy.
    let incoherent = BackendConformancePolicy {
        generated_token_policy: GeneratedTokenPolicy::ExactUnderGreedy,
        deterministic_test_mode: false,
        ..policy()
    };
    assert!(!validate_conformance_policy(incoherent.clone()).is_empty());
    assert!(matches!(
        evaluate_backend_conformance(incoherent, observation()),
        ConformanceVerdict::NonConformant {
            failure: ConformanceFailure::MalformedPolicy { .. }
        }
    ));
}

#[test]
fn numerical_contract_identity_moves_with_every_term() {
    let base = numerical_contract_identity(policy());
    let mutations: Vec<(&str, Mutation<BackendConformancePolicy>)> = vec![
        (
            "logits tolerance",
            Box::new(|p: &mut BackendConformancePolicy| p.logits_tolerance = 1e-2),
        ),
        (
            "embedding tolerance",
            Box::new(|p: &mut BackendConformancePolicy| p.embedding_tolerance = 1e-3),
        ),
        (
            "tokenizer",
            Box::new(|p: &mut BackendConformancePolicy| p.tokenizer_identity = "tok-x".into()),
        ),
        (
            "context cap",
            Box::new(|p: &mut BackendConformancePolicy| p.context_cap = 8192),
        ),
        (
            "token policy",
            Box::new(|p: &mut BackendConformancePolicy| {
                p.generated_token_policy = GeneratedTokenPolicy::DistributionOnly
            }),
        ),
    ];
    for (name, mutate) in mutations {
        let mut p = policy();
        mutate(&mut p);
        assert_ne!(
            numerical_contract_identity(p),
            base,
            "loosening/altering {name} must not preserve the contract id"
        );
    }
    assert_eq!(numerical_contract_identity(policy()), base, "deterministic");
}

// Exact prompt bytes: providers transmit them unchanged, so the hash must
// distinguish anything a rewriter might "helpfully" alter.
#[test]
fn prompt_byte_identity_is_exact() {
    let base = prompt_byte_identity("focused".into(), "Explain the gate.".into());
    for altered in [
        "Explain the gate. ",
        "explain the gate.",
        "Explain  the gate.",
        "Explain the gate.\n",
        "Explain the gate",
    ] {
        assert_ne!(
            prompt_byte_identity("focused".into(), altered.into()),
            base,
            "rewriting '{altered}' must be visible"
        );
    }
    // The profile is part of the identity, and delimiter injection cannot
    // collide two different (profile, prompt) pairs.
    assert_ne!(
        prompt_byte_identity("reflective".into(), "Explain the gate.".into()),
        base
    );
    assert_ne!(
        prompt_byte_identity("a".into(), "b|c".into()),
        prompt_byte_identity("a|b".into(), "c".into())
    );
    assert_eq!(
        prompt_byte_identity("focused".into(), "Explain the gate.".into()),
        base
    );
    // Unicode survives byte-exactly.
    assert_ne!(
        prompt_byte_identity("focused".into(), "导出 \"quotes\"".into()),
        base
    );
}
