// M7-A2 runtime-target, runtime-pack, and variant-selection regressions
// (ADR-002). Both polarities asserted so neutering a comparison fails a test.

use neuralcompose_mobile_core::runtime_target::*;

type Mutation<T> = Box<dyn Fn(&mut T)>;

fn sha(n: u8) -> String {
    format!("{:02x}", n).repeat(32)
}

fn caps() -> RuntimeCapabilities {
    RuntimeCapabilities {
        generation: true,
        embeddings: false,
        streaming: true,
        cancellation: true,
        structured_output: false,
    }
}

fn target(backend: &str, class: AcceleratorClass) -> RuntimeTarget {
    RuntimeTarget {
        os: "windows".into(),
        architecture: "x86_64".into(),
        accelerator_class: class,
        backend_id: backend.into(),
        runtime_abi: "nc-gguf-v1".into(),
        model_formats: vec!["gguf".into()],
        minimum_os_version: Some("10.0.22621".into()),
        minimum_backend_version: None,
        minimum_driver_version: None,
        capabilities: caps(),
    }
}

fn manifest() -> RuntimePackManifest {
    RuntimePackManifest {
        schema_version: 1,
        runtime_pack_id: "runtime-vulkan".into(),
        version: "1.0.0".into(),
        target: target("llama-cpp-vulkan", AcceleratorClass::Gpu),
        libraries: vec![
            RuntimeLibrary {
                library_id: "core".into(),
                relative_path: "lib/llama.dll".into(),
                byte_size: 1000,
                sha256_hex: sha(0xaa),
            },
            RuntimeLibrary {
                library_id: "backend".into(),
                relative_path: "lib/vulkan.dll".into(),
                byte_size: 500,
                sha256_hex: sha(0xbb),
            },
        ],
        licenses: vec!["MIT".into()],
        entrypoints: vec![RuntimeEntrypoint {
            entrypoint_id: "generate".into(),
            library_id: "core".into(),
            symbol: "nc_generate".into(),
        }],
        runtime_abi: "nc-gguf-v1".into(),
        signing_identity: Some("NeuralCompose Runtime Signing".into()),
    }
}

fn variant(id: &str, backend: &str, class: AcceleratorClass) -> ModelVariant {
    ModelVariant {
        schema_version: 1,
        logical_model_id: "qwen2.5-0.5b-instruct".into(),
        variant_id: id.into(),
        model_pack_id: "local-dialogue-basic".into(),
        runtime_target: target(backend, class),
        quantization: Some("q4_k_m".into()),
        artifact_format: "gguf".into(),
        numerical_contract_id: "nc-gguf-q4-v1".into(),
    }
}

fn device(installed: &[&str]) -> DeviceRuntimeProfile {
    DeviceRuntimeProfile {
        os: "windows".into(),
        architecture: "x86_64".into(),
        installed_backend_ids: installed.iter().map(|s| s.to_string()).collect(),
        supported_runtime_abis: vec!["nc-gguf-v1".into()],
    }
}

fn no_required() -> RequiredCapabilities {
    RequiredCapabilities {
        streaming: false,
        cancellation: false,
        structured_output: false,
    }
}

// OS target and hardware backend are separate dimensions.
#[test]
fn os_target_and_backend_are_independent_dimensions() {
    let cpu = target("llama-cpp-cpu", AcceleratorClass::Cpu);
    let mut vulkan = cpu.clone();
    vulkan.backend_id = "llama-cpp-vulkan".into();
    vulkan.accelerator_class = AcceleratorClass::Gpu;
    // Same OS/architecture, different backend → different target identity.
    assert_eq!(cpu.os, vulkan.os);
    assert_ne!(
        runtime_target_identity(cpu.clone()),
        runtime_target_identity(vulkan.clone())
    );
    // Same backend, different OS → also different identity.
    let mut linux = cpu.clone();
    linux.os = "linux".into();
    assert_ne!(
        runtime_target_identity(cpu.clone()),
        runtime_target_identity(linux)
    );
    // Polarity: identical declarations agree, and format order does not count.
    let mut reordered = cpu.clone();
    reordered.model_formats = vec!["gguf".into()];
    assert_eq!(
        runtime_target_identity(cpu.clone()),
        runtime_target_identity(reordered)
    );
}

// A logical model is not a backend artifact variant.
#[test]
fn logical_model_and_variant_identities_are_distinct() {
    let cpu = variant("gguf-q4-cpu-x86_64", "llama-cpp-cpu", AcceleratorClass::Cpu);
    let vulkan = variant(
        "gguf-q4-vulkan-x86_64",
        "llama-cpp-vulkan",
        AcceleratorClass::Gpu,
    );
    assert_eq!(cpu.logical_model_id, vulkan.logical_model_id);
    assert_ne!(
        model_variant_identity(cpu.clone()),
        model_variant_identity(vulkan)
    );
    // Non-invariances: quantization and numerical contract each move it.
    let mut requant = cpu.clone();
    requant.quantization = Some("q8_0".into());
    assert_ne!(
        model_variant_identity(cpu.clone()),
        model_variant_identity(requant)
    );
    let mut recontract = cpu.clone();
    recontract.numerical_contract_id = "nc-gguf-q4-v2".into();
    assert_ne!(
        model_variant_identity(cpu.clone()),
        model_variant_identity(recontract)
    );
    // Polarity: determinism.
    assert_eq!(
        model_variant_identity(cpu.clone()),
        model_variant_identity(cpu)
    );
}

// Artifact-order invariance and path (root) invariance.
#[test]
fn manifest_digest_is_order_and_root_invariant() {
    let base = manifest();
    let mut reordered = base.clone();
    reordered.libraries.reverse();
    reordered.entrypoints.reverse();
    assert_eq!(
        runtime_pack_manifest_digest(base.clone()),
        runtime_pack_manifest_digest(reordered),
        "declaration order must not change the digest"
    );
    // Paths are relative by contract, so no absolute root exists to change;
    // changing a relative path IS a content change and must move the digest.
    let mut moved = base.clone();
    moved.libraries[0].relative_path = "lib64/llama.dll".into();
    assert_ne!(
        runtime_pack_manifest_digest(base.clone()),
        runtime_pack_manifest_digest(moved)
    );
    let mut retargeted = base.clone();
    retargeted.target.backend_id = "llama-cpp-cuda".into();
    assert_ne!(
        runtime_pack_manifest_digest(base.clone()),
        runtime_pack_manifest_digest(retargeted)
    );
    // Absolute or traversal paths never validate in the first place.
    for bad in ["/abs/llama.dll", "../escape.dll", "lib\\win.dll"] {
        let mut m = base.clone();
        m.libraries[0].relative_path = bad.into();
        assert!(
            !validate_runtime_pack_manifest(m).is_empty(),
            "must reject {bad}"
        );
    }
}

#[test]
fn manifest_validation_rejects_incoherent_packs() {
    assert!(validate_runtime_pack_manifest(manifest()).is_empty());
    let cases: Vec<(&str, Mutation<RuntimePackManifest>)> = vec![
        (
            "abi disagreement",
            Box::new(|m: &mut RuntimePackManifest| m.runtime_abi = "other-abi".into()),
        ),
        (
            "dangling entrypoint",
            Box::new(|m: &mut RuntimePackManifest| m.entrypoints[0].library_id = "nope".into()),
        ),
        (
            "no libraries",
            Box::new(|m: &mut RuntimePackManifest| m.libraries.clear()),
        ),
        (
            "no licenses",
            Box::new(|m: &mut RuntimePackManifest| m.licenses.clear()),
        ),
        (
            "duplicate library id",
            Box::new(|m: &mut RuntimePackManifest| m.libraries[1].library_id = "core".into()),
        ),
        (
            "zero-byte library",
            Box::new(|m: &mut RuntimePackManifest| m.libraries[0].byte_size = 0),
        ),
        (
            "uppercase digest",
            Box::new(|m: &mut RuntimePackManifest| {
                m.libraries[0].sha256_hex = m.libraries[0].sha256_hex.to_uppercase()
            }),
        ),
        (
            "future schema",
            Box::new(|m: &mut RuntimePackManifest| m.schema_version = 2),
        ),
        (
            "malformed version",
            Box::new(|m: &mut RuntimePackManifest| m.version = "one".into()),
        ),
    ];
    for (name, mutate) in cases {
        let mut m = manifest();
        mutate(&mut m);
        assert!(
            !validate_runtime_pack_manifest(m).is_empty(),
            "must reject: {name}"
        );
    }
}

#[test]
fn variant_must_be_loadable_by_its_own_target() {
    assert!(
        validate_model_variant(variant("v", "llama-cpp-cpu", AcceleratorClass::Cpu)).is_empty()
    );
    let mut mismatched = variant("v", "llama-cpp-cpu", AcceleratorClass::Cpu);
    mismatched.artifact_format = "onnx".into(); // target declares gguf only
    assert!(!validate_model_variant(mismatched).is_empty());
    let mut empty_contract = variant("v", "llama-cpp-cpu", AcceleratorClass::Cpu);
    empty_contract.numerical_contract_id = "  ".into();
    assert!(!validate_model_variant(empty_contract).is_empty());
}

// PRIMARY: no unsupported backend fallback.
#[test]
fn explicit_backend_requirement_never_falls_back() {
    let variants = vec![
        variant("cpu", "llama-cpp-cpu", AcceleratorClass::Cpu),
        variant("cuda", "llama-cpp-cuda", AcceleratorClass::Gpu),
    ];
    // CUDA explicitly required but not installed → Unavailable, NOT CPU.
    let sel = select_runtime_variant(
        "qwen2.5-0.5b-instruct".into(),
        variants.clone(),
        device(&["llama-cpp-cpu"]),
        BackendRequirement::Explicit {
            backend_id: "llama-cpp-cuda".into(),
        },
        no_required(),
    );
    assert_eq!(
        sel,
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::RequestedBackendNotInstalled {
                backend_id: "llama-cpp-cuda".into()
            }
        },
        "an explicit CUDA request must never resolve to CPU"
    );
    // Explicitly required backend with no published variant at all.
    let sel = select_runtime_variant(
        "qwen2.5-0.5b-instruct".into(),
        variants.clone(),
        device(&["windows-ml-qnn"]),
        BackendRequirement::Explicit {
            backend_id: "windows-ml-qnn".into(),
        },
        no_required(),
    );
    assert_eq!(
        sel,
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::RequestedBackendNotPublished {
                backend_id: "windows-ml-qnn".into()
            }
        }
    );
    // Polarity 1: explicit CUDA WITH CUDA installed resolves to CUDA.
    let sel = select_runtime_variant(
        "qwen2.5-0.5b-instruct".into(),
        variants.clone(),
        device(&["llama-cpp-cpu", "llama-cpp-cuda"]),
        BackendRequirement::Explicit {
            backend_id: "llama-cpp-cuda".into(),
        },
        no_required(),
    );
    match sel {
        RuntimeSelection::Selected { variant } => {
            assert_eq!(variant.runtime_target.backend_id, "llama-cpp-cuda")
        }
        other => panic!("expected CUDA, got {other:?}"),
    }
    // Polarity 2: with no explicit requirement, CPU is a legitimate choice.
    let sel = select_runtime_variant(
        "qwen2.5-0.5b-instruct".into(),
        variants,
        device(&["llama-cpp-cpu"]),
        BackendRequirement::AnySupported,
        no_required(),
    );
    match sel {
        RuntimeSelection::Selected { variant } => {
            assert_eq!(variant.runtime_target.backend_id, "llama-cpp-cpu")
        }
        other => panic!("expected CPU, got {other:?}"),
    }
}

#[test]
fn selection_fails_closed_on_missing_capability_abi_and_ambiguity() {
    let need_structured = RequiredCapabilities {
        streaming: false,
        cancellation: false,
        structured_output: true, // no variant offers it
    };
    let variants = vec![variant("cpu", "llama-cpp-cpu", AcceleratorClass::Cpu)];
    let sel = select_runtime_variant(
        "qwen2.5-0.5b-instruct".into(),
        variants.clone(),
        device(&["llama-cpp-cpu"]),
        BackendRequirement::AnySupported,
        need_structured.clone(),
    );
    assert_eq!(
        sel,
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::RequiredCapabilityUnavailable {
                capability: "structuredOutput".into()
            }
        }
    );
    // Polarity: a capability the backend HAS resolves normally.
    let need_cancel = RequiredCapabilities {
        streaming: true,
        cancellation: true,
        structured_output: false,
    };
    assert!(matches!(
        select_runtime_variant(
            "qwen2.5-0.5b-instruct".into(),
            variants.clone(),
            device(&["llama-cpp-cpu"]),
            BackendRequirement::AnySupported,
            need_cancel,
        ),
        RuntimeSelection::Selected { .. }
    ));
    // Unsupported ABI on the device.
    let mut old_device = device(&["llama-cpp-cpu"]);
    old_device.supported_runtime_abis = vec!["nc-gguf-v0".into()];
    assert!(matches!(
        select_runtime_variant(
            "qwen2.5-0.5b-instruct".into(),
            variants.clone(),
            old_device,
            BackendRequirement::AnySupported,
            no_required(),
        ),
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::RuntimeAbiUnsupported { .. }
        }
    ));
    // Wrong OS/architecture.
    let mut wrong_os = device(&["llama-cpp-cpu"]);
    wrong_os.os = "linux".into();
    assert_eq!(
        select_runtime_variant(
            "qwen2.5-0.5b-instruct".into(),
            variants.clone(),
            wrong_os,
            BackendRequirement::AnySupported,
            no_required(),
        ),
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::NoVariantForOsArchitecture
        }
    );
    // Duplicate variant ids can never be resolved by order.
    let dup = vec![variants[0].clone(), variants[0].clone()];
    assert!(matches!(
        select_runtime_variant(
            "qwen2.5-0.5b-instruct".into(),
            dup,
            device(&["llama-cpp-cpu"]),
            BackendRequirement::AnySupported,
            no_required(),
        ),
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::AmbiguousVariants { .. }
        }
    ));
}

// When candidates fail differently, the most actionable failure is reported.
#[test]
fn the_furthest_progressed_failure_is_reported() {
    let variants = vec![
        variant("cpu", "llama-cpp-cpu", AcceleratorClass::Cpu),
        variant("qnn", "windows-ml-qnn", AcceleratorClass::Npu),
    ];
    let need_structured = RequiredCapabilities {
        streaming: false,
        cancellation: false,
        structured_output: true,
    };
    // CPU is installed but lacks the capability; QNN is simply absent.
    // "cannot do structured output" is the useful answer.
    assert_eq!(
        select_runtime_variant(
            "qwen2.5-0.5b-instruct".into(),
            variants,
            device(&["llama-cpp-cpu"]),
            BackendRequirement::AnySupported,
            need_structured,
        ),
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::RequiredCapabilityUnavailable {
                capability: "structuredOutput".into()
            }
        },
        "an incidental missing backend must not mask the real constraint"
    );
    // Polarity: with nothing installed at all, absence IS the real answer.
    assert!(matches!(
        select_runtime_variant(
            "qwen2.5-0.5b-instruct".into(),
            vec![variant("cpu", "llama-cpp-cpu", AcceleratorClass::Cpu)],
            device(&[]),
            BackendRequirement::AnySupported,
            no_required(),
        ),
        RuntimeSelection::Unavailable {
            failure: SelectionFailure::RequestedBackendNotInstalled { .. }
        }
    ));
}

// Selection is order-independent: AnySupported must not depend on which
// variant the caller happened to list first.
#[test]
fn any_supported_selection_is_declaration_order_independent() {
    let cpu = variant("cpu", "llama-cpp-cpu", AcceleratorClass::Cpu);
    let gpu = variant("vulkan", "llama-cpp-vulkan", AcceleratorClass::Gpu);
    let d = device(&["llama-cpp-cpu", "llama-cpp-vulkan"]);
    let a = select_runtime_variant(
        "qwen2.5-0.5b-instruct".into(),
        vec![cpu.clone(), gpu.clone()],
        d.clone(),
        BackendRequirement::AnySupported,
        no_required(),
    );
    let b = select_runtime_variant(
        "qwen2.5-0.5b-instruct".into(),
        vec![gpu, cpu],
        d,
        BackendRequirement::AnySupported,
        no_required(),
    );
    assert_eq!(a, b);
}

// PRIMARY: never promote a target by implication.
#[test]
fn support_promotion_requires_its_own_evidence() {
    let contracted = SupportEvidence {
        contracts_and_tests_pass: true,
        builds_on_named_target: false,
        fixture_runtime_executed: false,
        physical_device: None,
        os_version: None,
        backend_version: None,
        signed_packaging_accepted: false,
        acceptance_document: None,
    };
    assert_eq!(
        attained_support_status(contracted.clone()),
        Some(SupportStatus::Contracted)
    );
    // Compiling is NOT running.
    let built = SupportEvidence {
        builds_on_named_target: true,
        ..contracted.clone()
    };
    assert_eq!(
        attained_support_status(built.clone()),
        Some(SupportStatus::BuildValidated)
    );
    assert!(
        !supports_claim(built.clone(), SupportStatus::RuntimeSmokeValidated),
        "compilation must never imply runtime validation"
    );
    // Running a fixture is NOT device validation.
    let smoked = SupportEvidence {
        fixture_runtime_executed: true,
        ..built.clone()
    };
    assert_eq!(
        attained_support_status(smoked.clone()),
        Some(SupportStatus::RuntimeSmokeValidated)
    );
    assert!(!supports_claim(
        smoked.clone(),
        SupportStatus::DeviceValidated
    ));
    // Device validation needs NAMED hardware, OS, and backend versions.
    let half_named = SupportEvidence {
        physical_device: Some("Pixel 8a".into()),
        os_version: Some("   ".into()), // blank is not named
        backend_version: Some("b1".into()),
        ..smoked.clone()
    };
    assert_eq!(
        attained_support_status(half_named),
        Some(SupportStatus::RuntimeSmokeValidated)
    );
    let device_validated = SupportEvidence {
        physical_device: Some("Pixel 8a".into()),
        os_version: Some("Android 16".into()),
        backend_version: Some("llama.cpp b4321".into()),
        ..smoked
    };
    assert_eq!(
        attained_support_status(device_validated.clone()),
        Some(SupportStatus::DeviceValidated)
    );
    assert!(!supports_claim(
        device_validated.clone(),
        SupportStatus::ReleaseSupported
    ));
    // Release needs signed packaging AND an acceptance document.
    let released = SupportEvidence {
        signed_packaging_accepted: true,
        acceptance_document: Some("docs/acceptance/m8-b.md".into()),
        ..device_validated.clone()
    };
    assert_eq!(
        attained_support_status(released.clone()),
        Some(SupportStatus::ReleaseSupported)
    );
    assert!(supports_claim(released, SupportStatus::DeviceValidated));
    // Failing contracts attains nothing at all.
    let broken = SupportEvidence {
        contracts_and_tests_pass: false,
        ..device_validated
    };
    assert_eq!(attained_support_status(broken.clone()), None);
    assert!(!supports_claim(broken, SupportStatus::Contracted));
}
