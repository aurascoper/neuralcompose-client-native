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
    ModelPackInstaller::new(
        entry,
        vec!["nc-gguf-v1".into()],
        1,
        None,
        vec![],
        vec![],
        vec![1],
    )
}

/// Sealed restoration through the constructor: raw persisted record +
/// fresh observations + trusted catalog. No RestoreResult is accepted.
fn restored_installer(
    target: ModelPackCatalogEntry,
    record: InstalledModelPack,
    observed: Vec<ObservedArtifact>,
    trusted: Vec<ModelPackCatalogEntry>,
) -> ModelPackInstaller {
    ModelPackInstaller::new(
        target,
        vec!["nc-gguf-v1".into()],
        1,
        Some(record),
        observed,
        trusted,
        vec![1],
    )
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
    let mut old = gen_entry();
    old.pack_version = "0.9.0".into();
    let v09 = published_record(&old);
    // Sealed restore of v0.9 (its own trusted entry retained) while the
    // installer targets 1.0.0.
    let i = restored_installer(
        gen_entry(), // targets 1.0.0
        v09.clone(),
        observed_ok(),
        vec![old.clone(), gen_entry()],
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

    let restore = |rec: InstalledModelPack,
                   obs: Vec<ObservedArtifact>,
                   cat: Vec<ModelPackCatalogEntry>,
                   accepted: Vec<u32>,
                   target: &str| {
        restore_installed_record(
            rec,
            obs,
            cat,
            vec!["nc-gguf-v1".into()],
            accepted,
            target.into(),
        )
    };
    match restore(
        record.clone(),
        observed_ok(),
        trusted.clone(),
        vec![1],
        "local-dialogue-basic",
    ) {
        RestoreResult::Restored { record: r } => assert_eq!(r.pack_version, "0.9.0"),
        other => panic!("expected restore, got {other:?}"),
    }
    // Missing trusted entry → visible failure, not silence.
    match restore(
        record.clone(),
        observed_ok(),
        vec![gen_entry()],
        vec![1],
        "local-dialogue-basic",
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
        restore(
            tampered,
            observed_ok(),
            trusted.clone(),
            vec![1],
            "local-dialogue-basic"
        ),
        RestoreResult::Rejected {
            failure: RestoreFailure::CatalogDigestMismatch
        }
    ));
    // Tampered inventory (well-formed hash, wrong content).
    let mut swapped = record.clone();
    swapped.artifact_digests[0].sha256_hex = sha(0x22);
    assert!(matches!(
        restore(
            swapped,
            observed_ok(),
            trusted.clone(),
            vec![1],
            "local-dialogue-basic"
        ),
        RestoreResult::Rejected {
            failure: RestoreFailure::InstalledInventoryMismatch
        }
    ));
    // Policy version outside the caller-accepted set.
    assert!(matches!(
        restore(
            record.clone(),
            observed_ok(),
            trusted.clone(),
            vec![2],
            "local-dialogue-basic"
        ),
        RestoreResult::Rejected {
            failure: RestoreFailure::UnsupportedVerificationPolicy
        }
    ));
    // Record targeting a different pack than the installer's entry.
    assert!(matches!(
        restore(
            record.clone(),
            observed_ok(),
            trusted.clone(),
            vec![1],
            "some-other-pack"
        ),
        RestoreResult::Rejected {
            failure: RestoreFailure::TargetPackMismatch
        }
    ));
    // Rejected restore surfaces visibly in the installer snapshot: sealed
    // constructor, empty trusted catalog.
    let i = restored_installer(gen_entry(), record.clone(), observed_ok(), vec![]);
    let snap = i.snapshot();
    assert!(!snap.has_usable_active_installation);
    assert_eq!(
        snap.restore_failure,
        Some(RestoreFailure::TrustedCatalogEntryMissing)
    );
    assert_eq!(snap.operation_phase, ModelPackPhase::NotInstalled);
    // Observations without a persisted record are contradictory input —
    // visible failure, no restore attempt.
    let j = ModelPackInstaller::new(
        gen_entry(),
        vec!["nc-gguf-v1".into()],
        1,
        None,
        observed_ok(),
        vec![gen_entry()],
        vec![1],
    );
    let jsnap = j.snapshot();
    assert!(!jsnap.has_usable_active_installation);
    assert!(matches!(
        jsnap.restore_failure,
        Some(RestoreFailure::InvalidInstalledRecord { .. })
    ));
}

// ROUND-2 BLOCKER: restoration verifies the actual bytes, not just the
// persisted metadata record.
#[test]
fn restore_rejects_missing_or_modified_on_disk_bytes() {
    let record = published_record(&gen_entry());
    let trusted = vec![gen_entry()];
    let attempt = |obs: Vec<ObservedArtifact>| {
        restore_installed_record(
            record.clone(),
            obs,
            trusted.clone(),
            vec!["nc-gguf-v1".into()],
            vec![1],
            "local-dialogue-basic".into(),
        )
    };
    // Polarity: exact bytes restore.
    assert!(matches!(
        attempt(observed_ok()),
        RestoreResult::Restored { .. }
    ));
    let rejected = |obs: Vec<ObservedArtifact>| {
        matches!(
            attempt(obs),
            RestoreResult::Rejected {
                failure: RestoreFailure::OnDiskInventoryMismatch { .. }
            }
        )
    };
    // Deleted weights.
    let mut missing = observed_ok();
    missing.remove(0);
    assert!(rejected(missing));
    // Modified weights bytes.
    let mut modified = observed_ok();
    modified[0].sha256_hex = sha(0x99);
    assert!(rejected(modified));
    // Wrong size.
    let mut resized = observed_ok();
    resized[0].byte_size = 999;
    assert!(rejected(resized));
    // Extra file next to the pack.
    let mut extra = observed_ok();
    extra.push(ObservedArtifact {
        relative_path: "sneaky.bin".into(),
        byte_size: 5,
        sha256_hex: sha(0x77),
    });
    assert!(rejected(extra));
    // Duplicate observed path.
    let mut dup = observed_ok();
    dup.push(dup[0].clone());
    assert!(rejected(dup));
    // Malformed observations: traversal path, uppercase sha.
    let mut traversal = observed_ok();
    traversal[0].relative_path = "../model.gguf".into();
    assert!(rejected(traversal));
    let mut upper = observed_ok();
    upper[0].sha256_hex = upper[0].sha256_hex.to_uppercase();
    assert!(rejected(upper));
    // Through the sealed constructor: tampered bytes → visible failure,
    // no usable installation.
    let mut modified = observed_ok();
    modified[0].sha256_hex = sha(0x99);
    let i = restored_installer(gen_entry(), record.clone(), modified, trusted.clone());
    let snap = i.snapshot();
    assert!(!snap.has_usable_active_installation);
    assert!(snap.active_installation.is_none());
    assert!(matches!(
        snap.restore_failure,
        Some(RestoreFailure::OnDiskInventoryMismatch { .. })
    ));
}

// ROUND-2 BLOCKER: duplicate trusted identities are rejected regardless of
// catalog vector order.
#[test]
fn ambiguous_trusted_catalog_rejected_in_both_orders() {
    let record = published_record(&gen_entry());
    let genuine = gen_entry();
    let mut impostor = gen_entry(); // same (pack_id, pack_version), different content
    impostor.artifacts[0].sha256_hex = sha(0xee);
    for cat in [
        vec![genuine.clone(), impostor.clone()],
        vec![impostor.clone(), genuine.clone()],
    ] {
        assert!(matches!(
            restore_installed_record(
                record.clone(),
                observed_ok(),
                cat,
                vec!["nc-gguf-v1".into()],
                vec![1],
                "local-dialogue-basic".into(),
            ),
            RestoreResult::Rejected {
                failure: RestoreFailure::AmbiguousTrustedCatalog
            }
        ));
    }
    // Polarity: unambiguous catalog restores.
    assert!(matches!(
        restore_installed_record(
            record,
            observed_ok(),
            vec![genuine],
            vec!["nc-gguf-v1".into()],
            vec![1],
            "local-dialogue-basic".into(),
        ),
        RestoreResult::Restored { .. }
    ));
}

// ROUND-2 BLOCKER: verification-policy versions come from one Rust
// authority; zero and future versions are rejected at verify and restore.
#[test]
fn verification_policy_registry_rejects_zero_and_future_versions() {
    assert_eq!(supported_verification_policy_versions(), vec![1]);
    for bad in [0u32, 2, u32::MAX] {
        let i = ModelPackInstaller::new(
            gen_entry(),
            vec!["nc-gguf-v1".into()],
            bad,
            None,
            vec![],
            vec![],
            vec![bad],
        );
        to_verifying(&i);
        assert!(!i.verify(observed_ok()), "policy {bad} must not verify");
        assert!(matches!(
            i.snapshot().operation_failure.unwrap().reason,
            ModelPackFailure::UnsupportedVerificationPolicy { version } if version == bad
        ));
        assert!(!i.on_published(1), "policy {bad} must not publish");

        // Restore: record stamped with an unsupported policy is rejected
        // even when the caller claims to accept it.
        let mut record = published_record(&gen_entry());
        record.verification_policy_version = bad;
        assert!(matches!(
            restore_installed_record(
                record,
                observed_ok(),
                vec![gen_entry()],
                vec!["nc-gguf-v1".into()],
                vec![bad],
                "local-dialogue-basic".into(),
            ),
            RestoreResult::Rejected {
                failure: RestoreFailure::UnsupportedVerificationPolicy
            }
        ));
    }
    // Polarity: version 1 verifies, publishes, and restores end-to-end.
    let record = published_record(&gen_entry());
    assert_eq!(record.verification_policy_version, 1);
    assert!(matches!(
        restore_installed_record(
            record,
            observed_ok(),
            vec![gen_entry()],
            vec!["nc-gguf-v1".into()],
            vec![1],
            "local-dialogue-basic".into(),
        ),
        RestoreResult::Restored { .. }
    ));
}

// ROUND-2 BLOCKER: a pack is not usable while being removed or after a
// failed removal; only fresh exact revalidation restores usability.
#[test]
fn removal_and_failed_removal_report_unusable_until_revalidated() {
    let i = installer(gen_entry());
    to_verifying(&i);
    assert!(i.verify(observed_ok()));
    assert!(i.on_published(1));
    assert!(i.snapshot().has_usable_active_installation);

    assert!(i.on_removal_started());
    let snap = i.snapshot();
    assert!(snap.active_installation.is_some(), "record retained");
    assert!(
        !snap.has_usable_active_installation,
        "not usable while files may be disappearing"
    );

    assert!(i.on_removal_failed("fs busy".into()));
    assert!(!i.snapshot().has_usable_active_installation);
    assert!(i.acknowledge_operation_failure());
    assert!(
        !i.snapshot().has_usable_active_installation,
        "dismissing the error must not reactivate the pack"
    );

    // Tampered revalidation → still unusable, specific reason visible.
    let mut bad = observed_ok();
    bad[0].sha256_hex = sha(0x55);
    assert!(!i.revalidate_active(bad));
    let snap = i.snapshot();
    assert!(!snap.has_usable_active_installation);
    assert!(matches!(
        snap.active_integrity_failure,
        Some(ActiveIntegrityFailure::DigestMismatch { .. })
    ));
    // Other mismatch classes surface their own variants.
    let mut short = observed_ok();
    short.remove(1);
    assert!(!i.revalidate_active(short));
    assert!(matches!(
        i.snapshot().active_integrity_failure,
        Some(ActiveIntegrityFailure::MissingArtifact { .. })
    ));
    let mut extra = observed_ok();
    extra.push(ObservedArtifact {
        relative_path: "sneaky.bin".into(),
        byte_size: 5,
        sha256_hex: sha(0x66),
    });
    assert!(!i.revalidate_active(extra));
    assert!(matches!(
        i.snapshot().active_integrity_failure,
        Some(ActiveIntegrityFailure::UndeclaredArtifact { .. })
    ));

    // Fresh exact revalidation restores usability and clears the reason.
    assert!(i.revalidate_active(observed_ok()));
    let snap = i.snapshot();
    assert!(snap.has_usable_active_installation);
    assert_eq!(snap.active_integrity_failure, None);
    assert_eq!(snap.operation_phase, ModelPackPhase::Ready);
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
    let abi = ModelPackInstaller::new(
        gen_entry(),
        vec!["other".into()],
        1,
        None,
        vec![],
        vec![],
        vec![1],
    );
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
