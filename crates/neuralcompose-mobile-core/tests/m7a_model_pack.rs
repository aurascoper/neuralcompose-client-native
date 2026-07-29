// M7-A model-pack regressions (ADR-001, review-amended). Both polarities.

use neuralcompose_mobile_core::model_pack::*;

fn sha(n: u8) -> String {
    format!("{:02x}", n).repeat(32)
}

fn gen_entry() -> ModelPackCatalogEntry {
    ModelPackCatalogEntry {
        schema_version: 1,
        pack_id: "local-dialogue-basic".into(),
        pack_version: "1.0.0".into(),
        kind: ModelPackKind::Generation,
        model_family: "qwen2.5".into(),
        model_revision: "0.5b-instruct".into(),
        quantization: Some("q4_k_m".into()),
        artifact_format: "gguf".into(),
        license_id: "Apache-2.0".into(),
        source_repository: "example/qwen2.5-0.5b".into(),
        runtime_abi: "nc-gguf-v1".into(),
        minimum_core_version: "0.1.0".into(),
        artifacts: vec![
            ModelArtifact {
                artifact_id: "weights".into(),
                kind: ModelArtifactKind::Weights,
                relative_path: "model.gguf".into(),
                byte_size: 1000,
                sha256_hex: sha(0xaa),
            },
            ModelArtifact {
                artifact_id: "tokenizer".into(),
                kind: ModelArtifactKind::Tokenizer,
                relative_path: "tokenizer.json".into(),
                byte_size: 100,
                sha256_hex: sha(0xbb),
            },
        ],
        requirements: DeviceRequirements {
            minimum_ram_mb: 3072,
            device_class: "modern-phone".into(),
        },
        generation: Some(GenerationContract {
            tokenizer_id: "tokenizer".into(),
            context_cap: 4096,
            prompt_template_id: "chatml".into(),
            compatible_prompt_profiles: vec!["focused".into()],
        }),
        embedding: None,
    }
}

fn emb_entry() -> ModelPackCatalogEntry {
    let mut e = gen_entry();
    e.pack_id = "semantic-search-basic".into();
    e.kind = ModelPackKind::Embedding;
    e.generation = None;
    e.embedding = Some(EmbeddingContract {
        tokenizer_id: "tokenizer".into(),
        dimensions: 256,
        pooling: EmbeddingPooling::Mean,
        normalization: EmbeddingNormalization::L2,
        task_instruction: Some("retrieve related journal entries".into()),
    });
    e
}

fn observed_ok() -> Vec<ObservedArtifact> {
    vec![
        ObservedArtifact {
            relative_path: "model.gguf".into(),
            byte_size: 1000,
            sha256_hex: sha(0xaa),
        },
        ObservedArtifact {
            relative_path: "tokenizer.json".into(),
            byte_size: 100,
            sha256_hex: sha(0xbb),
        },
    ]
}

fn installer(entry: ModelPackCatalogEntry) -> ModelPackInstaller {
    ModelPackInstaller::new(entry, vec!["nc-gguf-v1".into()], 1, None)
}

fn to_verifying(i: &ModelPackInstaller) {
    assert!(i.on_queued());
    assert!(i.on_download_progress(10, 1100));
    assert!(i.on_download_complete());
    assert_eq!(i.phase(), ModelPackPhase::Verifying);
}

fn published_record(entry: &ModelPackCatalogEntry) -> InstalledModelPack {
    let i = installer(entry.clone());
    to_verifying(&i);
    assert!(i.verify(observed_ok()));
    assert!(i.on_published(123));
    i.active_installation().expect("installed")
}

// PRIMARY BLOCKER: verification cannot be skipped or staled.
#[test]
fn publish_requires_a_fresh_verification_receipt() {
    // publish without verify → rejected
    let i = installer(gen_entry());
    to_verifying(&i);
    assert!(!i.on_published(1), "verification skipped");
    assert_eq!(i.phase(), ModelPackPhase::Verifying);
    assert!(i.active_installation().is_none());

    // successful verify → failed reverify → publish rejected
    let i2 = installer(gen_entry());
    to_verifying(&i2);
    assert!(i2.verify(observed_ok()));
    let mut bad = observed_ok();
    bad[0].sha256_hex = sha(0xcc);
    assert!(!i2.verify(bad), "reverify fails");
    assert!(
        !i2.on_published(1),
        "receipt must be cleared by the failed attempt"
    );

    // Download events are REJECTED from Verifying (no state corruption):
    // the receipt stays bound to the unchanged, already-verified inventory.
    let i3 = installer(gen_entry());
    to_verifying(&i3);
    assert!(i3.verify(observed_ok()));
    assert!(
        !i3.on_download_progress(5, 10),
        "no re-download from Verifying"
    );
    assert!(!i3.on_queued(), "no requeue from Verifying");
    assert!(
        i3.on_published(1),
        "receipt valid for the unchanged inventory"
    );

    // polarity: verify → publish succeeds, receipt consumed (second publish fails)
    let i4 = installer(gen_entry());
    to_verifying(&i4);
    assert!(i4.verify(observed_ok()));
    assert!(i4.on_published(9));
    assert!(!i4.on_published(10), "receipt consumed");
    let rec = i4.active_installation().unwrap();
    assert_eq!(rec.verified_inventory_digest.len(), 64);
    assert_eq!(rec.catalog_entry_digest, catalog_entry_digest(gen_entry()));
}

#[test]
fn duplicate_observed_paths_rejected_before_verification() {
    let i = installer(gen_entry());
    to_verifying(&i);
    let mut dup = observed_ok();
    dup.push(dup[0].clone());
    assert!(!i.verify(dup));
    assert!(matches!(
        i.snapshot().operation_failure.unwrap().reason,
        ModelPackFailure::DuplicateObservedPath { .. }
    ));
}

// PRIMARY BLOCKER: active vs operation independence.
#[test]
fn failed_update_keeps_active_version_usable() {
    let v09 = {
        let mut e = gen_entry();
        e.pack_version = "0.9.0".into();
        published_record(&e)
    };
    let i = ModelPackInstaller::new(
        gen_entry(), // targets 1.0.0
        vec!["nc-gguf-v1".into()],
        1,
        Some(RestoreResult::Restored {
            record: v09.clone(),
        }),
    );
    assert_eq!(
        i.phase(),
        ModelPackPhase::Ready,
        "restored active presents Ready"
    );
    assert!(i.on_queued(), "updates start from Ready");
    assert!(i.on_download_complete());
    let mut bad = observed_ok();
    bad[0].sha256_hex = sha(0xdd);
    assert!(!i.verify(bad));
    let snap = i.snapshot();
    assert!(matches!(
        snap.operation_phase,
        ModelPackPhase::Failed { .. }
    ));
    assert!(
        snap.has_usable_active_installation,
        "v0.9 stays usable through failed update"
    );
    assert_eq!(snap.active_installation.unwrap().pack_version, "0.9.0");
    assert_eq!(
        snap.operation_failure.as_ref().unwrap().operation,
        OperationKind::Update
    );

    // No direct Failed → Queued.
    assert!(!i.on_queued());
    assert!(i.acknowledge_operation_failure());
    assert!(i.on_queued(), "requeue after acknowledgement");
}

// PRIMARY BLOCKER: trusted-catalog restoration.
#[test]
fn restore_resolves_against_trusted_catalog_only() {
    let mut old = gen_entry();
    old.pack_version = "0.9.0".into();
    let record = published_record(&old);
    let trusted = vec![old.clone(), gen_entry()]; // retains supported installed versions

    match restore_installed_record(
        record.clone(),
        trusted.clone(),
        vec!["nc-gguf-v1".into()],
        vec![1],
    ) {
        RestoreResult::Restored { record: r } => assert_eq!(r.pack_version, "0.9.0"),
        other => panic!("expected restore, got {other:?}"),
    }
    // Missing trusted entry → visible failure, not silence.
    match restore_installed_record(
        record.clone(),
        vec![gen_entry()],
        vec!["nc-gguf-v1".into()],
        vec![1],
    ) {
        RestoreResult::Rejected { failure } => {
            assert_eq!(failure, RestoreFailure::TrustedCatalogEntryMissing)
        }
        _ => panic!("must reject"),
    }
    // Tampered catalog digest.
    let mut tampered = record.clone();
    tampered.catalog_entry_digest = sha(0x11);
    assert!(matches!(
        restore_installed_record(
            tampered,
            trusted.clone(),
            vec!["nc-gguf-v1".into()],
            vec![1]
        ),
        RestoreResult::Rejected {
            failure: RestoreFailure::CatalogDigestMismatch
        }
    ));
    // Tampered inventory (well-formed hash, wrong content).
    let mut swapped = record.clone();
    swapped.artifact_digests[0].sha256_hex = sha(0x22);
    assert!(matches!(
        restore_installed_record(swapped, trusted.clone(), vec!["nc-gguf-v1".into()], vec![1]),
        RestoreResult::Rejected {
            failure: RestoreFailure::InstalledInventoryMismatch
        }
    ));
    // Unsupported policy version.
    assert!(matches!(
        restore_installed_record(
            record.clone(),
            trusted.clone(),
            vec!["nc-gguf-v1".into()],
            vec![2]
        ),
        RestoreResult::Rejected {
            failure: RestoreFailure::UnsupportedVerificationPolicy
        }
    ));
    // Rejected restore surfaces visibly in the installer snapshot.
    let i = ModelPackInstaller::new(
        gen_entry(),
        vec!["nc-gguf-v1".into()],
        1,
        Some(RestoreResult::Rejected {
            failure: RestoreFailure::TrustedCatalogEntryMissing,
        }),
    );
    let snap = i.snapshot();
    assert!(!snap.has_usable_active_installation);
    assert_eq!(
        snap.restore_failure,
        Some(RestoreFailure::TrustedCatalogEntryMissing)
    );
    assert_eq!(snap.operation_phase, ModelPackPhase::NotInstalled);
}

// PRIMARY BLOCKER: compatibility validation.
#[test]
fn core_version_and_schema_compatibility() {
    let ok = |v: &str| validate_catalog_for_core(gen_entry(), v).is_empty();
    assert!(ok("0.1.0"), "minimum == core accepted");
    assert!(ok("1.2.3"), "older minimum accepted");
    assert!(!ok("0.0.9"), "minimum > core rejected");
    let mut malformed = gen_entry();
    malformed.minimum_core_version = "not-a-version".into();
    assert!(!validate_catalog_for_core(malformed, "0.1.0").is_empty());
    // SemVer pre-release ordering: 0.1.0-alpha < 0.1.0
    let mut pre = gen_entry();
    pre.minimum_core_version = "0.1.0-alpha".into();
    assert!(validate_catalog_for_core(pre, "0.1.0").is_empty());
    let mut future = gen_entry();
    future.schema_version = 2;
    assert!(!validate_catalog_for_core(future, "0.1.0").is_empty());
    // Dangling tokenizer_id and missing tokenizer artifact.
    let mut dangling = gen_entry();
    dangling.generation.as_mut().unwrap().tokenizer_id = "nope".into();
    assert!(!validate_catalog_entry(dangling).is_empty());
    let mut no_tok = gen_entry();
    no_tok
        .artifacts
        .retain(|a| a.kind != ModelArtifactKind::Tokenizer);
    assert!(!validate_catalog_entry(no_tok).is_empty());
    // Public wrapper uses the real core version (0.1.0) — same verdicts.
    assert!(validate_catalog_entry(gen_entry()).is_empty());
}

// PRIMARY BLOCKER: embedding identity completeness + canonical encoding.
#[test]
fn embedding_identity_is_complete_and_injection_resistant() {
    let base = embedding_space_identity(emb_entry()).unwrap();
    // Second weight shard changes the identity.
    let mut sharded = emb_entry();
    sharded.artifacts.push(ModelArtifact {
        artifact_id: "weights-2".into(),
        kind: ModelArtifactKind::Weights,
        relative_path: "model-00002.gguf".into(),
        byte_size: 500,
        sha256_hex: sha(0x33),
    });
    assert_ne!(embedding_space_identity(sharded.clone()).unwrap(), base);
    let mut shard_changed = sharded.clone();
    shard_changed.artifacts.last_mut().unwrap().sha256_hex = sha(0x34);
    assert_ne!(
        embedding_space_identity(shard_changed).unwrap(),
        embedding_space_identity(sharded).unwrap(),
        "changing only the second shard must change the identity"
    );
    // Delimiter/quote/Unicode content cannot collide.
    let mut tricky_a = emb_entry();
    tricky_a.embedding.as_mut().unwrap().task_instruction = Some("a|b".into());
    let mut tricky_b = emb_entry();
    tricky_b.embedding.as_mut().unwrap().task_instruction = Some("a".into());
    tricky_b.model_revision = "0.5b-instruct|b".into();
    assert_ne!(
        embedding_space_identity(tricky_a).unwrap(),
        embedding_space_identity(tricky_b).unwrap()
    );
    let mut unicode = emb_entry();
    unicode.embedding.as_mut().unwrap().task_instruction = Some("导出 \"quotes\" ok".into());
    assert_ne!(embedding_space_identity(unicode).unwrap(), base);
    // Invalid entry → no identity at all.
    let mut invalid = emb_entry();
    invalid.artifacts[0].byte_size = 0;
    assert_eq!(embedding_space_identity(invalid), None);
    assert_eq!(embedding_space_identity(gen_entry()), None);
    assert_eq!(
        embedding_space_identity(emb_entry()).unwrap(),
        base,
        "deterministic"
    );
}

#[test]
fn digest_size_abi_and_extras_block_ready() {
    let run = |obs: Vec<ObservedArtifact>| {
        let i = installer(gen_entry());
        to_verifying(&i);
        assert!(!i.verify(obs));
        let reason = i.snapshot().operation_failure.unwrap().reason;
        assert!(!i.on_published(1));
        reason
    };
    let mut a = observed_ok();
    a[0].sha256_hex = "e".repeat(64);
    assert!(matches!(run(a), ModelPackFailure::DigestMismatch { .. }));
    let mut b = observed_ok();
    b[1].byte_size = 7;
    assert!(matches!(run(b), ModelPackFailure::SizeMismatch { .. }));
    let mut c = observed_ok();
    c.push(ObservedArtifact {
        relative_path: "sneaky.bin".into(),
        byte_size: 5,
        sha256_hex: "f".repeat(64),
    });
    assert!(matches!(
        run(c),
        ModelPackFailure::UndeclaredArtifact { .. }
    ));
    let abi = ModelPackInstaller::new(gen_entry(), vec!["other".into()], 1, None);
    to_verifying(&abi);
    assert!(!abi.verify(observed_ok()));
    assert!(matches!(
        abi.snapshot().operation_failure.unwrap().reason,
        ModelPackFailure::RuntimeAbiIncompatible
    ));
}

#[test]
fn traversal_duplicates_and_kind_rules_rejected() {
    let mut t = gen_entry();
    t.artifacts[0].relative_path = "../model.gguf".into();
    assert!(!validate_catalog_entry(t).is_empty());
    let mut d = gen_entry();
    d.artifacts[1].artifact_id = "weights".into();
    assert!(!validate_catalog_entry(d).is_empty());
    let mut g = gen_entry();
    g.embedding = emb_entry().embedding;
    assert!(!validate_catalog_entry(g).is_empty());
    let mut e = emb_entry();
    e.generation = gen_entry().generation;
    assert!(!validate_catalog_entry(e).is_empty());
    assert!(validate_catalog_entry(emb_entry()).is_empty());
}

#[test]
fn removal_requires_platform_confirmation_and_read_paths_pure() {
    let i = installer(gen_entry());
    to_verifying(&i);
    assert!(i.verify(observed_ok()));
    assert!(i.on_published(1));
    for _ in 0..20 {
        let _ = i.phase();
        let _ = i.snapshot();
    }
    assert_eq!(i.phase(), ModelPackPhase::Ready);
    assert!(i.on_removal_started());
    assert!(
        i.snapshot().active_installation.is_some(),
        "survives until confirmed"
    );
    assert!(i.on_removal_failed("fs busy".into()));
    assert_eq!(
        i.snapshot().operation_failure.unwrap().operation,
        OperationKind::Removal
    );
    assert!(!i.on_removal_started(), "must acknowledge first");
    assert!(i.acknowledge_operation_failure());
    assert!(i.on_removal_started());
    assert!(i.on_removal_confirmed());
    assert_eq!(i.phase(), ModelPackPhase::NotInstalled);
    assert!(i.active_installation().is_none());
}
