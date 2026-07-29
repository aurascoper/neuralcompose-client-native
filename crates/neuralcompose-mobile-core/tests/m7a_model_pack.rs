// M7-A model-pack regressions (ADR-001), both polarities asserted.

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

fn drive_to_verifying(i: &ModelPackInstaller) {
    assert!(i.on_queued());
    assert!(i.on_download_progress(10, 1100));
    assert!(i.on_download_complete());
    assert_eq!(i.phase(), ModelPackPhase::Verifying);
}

// 4/5. Digest or size mismatch cannot enter Ready; polarity: exact match can.
#[test]
fn digest_and_size_mismatches_block_ready() {
    let i = installer(gen_entry());
    drive_to_verifying(&i);
    let mut bad = observed_ok();
    bad[0].sha256_hex = sha(0xcc);
    assert!(!i.verify(bad));
    assert!(matches!(
        i.phase(),
        ModelPackPhase::Failed {
            reason: ModelPackFailure::DigestMismatch { .. }
        }
    ));
    assert!(!i.on_published(1), "Failed can never publish");

    let i2 = installer(gen_entry());
    drive_to_verifying(&i2);
    let mut bad2 = observed_ok();
    bad2[1].byte_size = 999;
    assert!(!i2.verify(bad2));
    assert!(matches!(
        i2.phase(),
        ModelPackPhase::Failed {
            reason: ModelPackFailure::SizeMismatch { .. }
        }
    ));

    let i3 = installer(gen_entry());
    drive_to_verifying(&i3);
    assert!(i3.verify(observed_ok()));
    assert!(i3.on_published(123));
    assert_eq!(i3.phase(), ModelPackPhase::Ready);
    let rec = i3.installed().expect("installed record");
    assert_eq!(rec.pack_id, "local-dialogue-basic");
    assert_eq!(rec.artifact_digests.len(), 2);
}

// 6. Runtime ABI mismatch cannot enter Ready.
#[test]
fn abi_mismatch_blocks_ready() {
    let i = ModelPackInstaller::new(gen_entry(), vec!["other-abi".into()], 1, None);
    drive_to_verifying(&i);
    assert!(!i.verify(observed_ok()));
    assert_eq!(
        i.phase(),
        ModelPackPhase::Failed {
            reason: ModelPackFailure::RuntimeAbiIncompatible
        }
    );
}

// 7. A failed update preserves the prior Ready record.
#[test]
fn failed_update_preserves_previous_installed_version() {
    let prior = InstalledModelPack {
        pack_id: "local-dialogue-basic".into(),
        pack_version: "0.9.0".into(),
        installed_at_ms: 5,
        artifact_digests: vec![],
        runtime_abi: "nc-gguf-v1".into(),
        verification_policy_version: 1,
    };
    let i = ModelPackInstaller::new(
        gen_entry(),
        vec!["nc-gguf-v1".into()],
        1,
        Some(prior.clone()),
    );
    drive_to_verifying(&i);
    let mut bad = observed_ok();
    bad[0].sha256_hex = sha(0xdd);
    assert!(!i.verify(bad));
    assert_eq!(
        i.installed(),
        Some(prior),
        "previous ready version survives the failed update"
    );
}

// 8. Path traversal and duplicates rejected.
#[test]
fn traversal_and_duplicates_rejected() {
    let mut e = gen_entry();
    e.artifacts[0].relative_path = "../model.gguf".into();
    assert!(validate_catalog_entry(e.clone())
        .iter()
        .any(|m| m.contains("invalid relative path")));
    e.artifacts[0].relative_path = "/abs/model.gguf".into();
    assert!(!validate_catalog_entry(e.clone()).is_empty());
    let mut d = gen_entry();
    d.artifacts[1].artifact_id = "weights".into();
    assert!(validate_catalog_entry(d)
        .iter()
        .any(|m| m.contains("duplicate artifact id")));
    let mut p = gen_entry();
    p.artifacts[1].relative_path = "model.gguf".into();
    assert!(validate_catalog_entry(p)
        .iter()
        .any(|m| m.contains("duplicate relative path")));
    assert!(
        validate_catalog_entry(gen_entry()).is_empty(),
        "polarity: valid entry validates"
    );
}

// Undeclared extras rejected during publication verification.
#[test]
fn undeclared_extra_files_rejected() {
    let i = installer(gen_entry());
    drive_to_verifying(&i);
    let mut extra = observed_ok();
    extra.push(ObservedArtifact {
        relative_path: "sneaky.bin".into(),
        byte_size: 5,
        sha256_hex: sha(1),
    });
    assert!(!i.verify(extra));
    assert!(matches!(
        i.phase(),
        ModelPackPhase::Failed {
            reason: ModelPackFailure::UndeclaredArtifact { .. }
        }
    ));
}

// 9/10. Kind discriminators are closed.
#[test]
fn kind_cross_contamination_rejected() {
    let mut g = gen_entry();
    g.embedding = emb_entry().embedding;
    assert!(validate_catalog_entry(g)
        .iter()
        .any(|m| m.contains("must not carry embedding")));
    let mut e = emb_entry();
    e.generation = gen_entry().generation;
    assert!(validate_catalog_entry(e)
        .iter()
        .any(|m| m.contains("must not carry generation")));
    assert!(validate_catalog_entry(emb_entry()).is_empty());
}

// 11. Embedding identity changes with every component.
#[test]
fn embedding_identity_changes_with_each_component() {
    let base = embedding_space_identity(emb_entry()).unwrap();
    let mut dim = emb_entry();
    dim.embedding.as_mut().unwrap().dimensions = 128;
    let mut pool = emb_entry();
    pool.embedding.as_mut().unwrap().pooling = EmbeddingPooling::Cls;
    let mut norm = emb_entry();
    norm.embedding.as_mut().unwrap().normalization = EmbeddingNormalization::None;
    let mut instr = emb_entry();
    instr.embedding.as_mut().unwrap().task_instruction = None;
    let mut tok = emb_entry();
    tok.artifacts[1].sha256_hex = sha(0xee);
    let mut wt = emb_entry();
    wt.artifacts[0].sha256_hex = sha(0xef);
    let mut rev = emb_entry();
    rev.model_revision = "0.6b".into();
    for (name, e) in [
        ("dimensions", dim),
        ("pooling", pool),
        ("normalization", norm),
        ("instruction", instr),
        ("tokenizer", tok),
        ("weights", wt),
        ("revision", rev),
    ] {
        let id = embedding_space_identity(e).unwrap();
        assert_ne!(id, base, "{name} change must change the embedding space");
    }
    assert_eq!(
        embedding_space_identity(emb_entry()).unwrap(),
        base,
        "identity is deterministic"
    );
    assert_eq!(
        embedding_space_identity(gen_entry()),
        None,
        "generation packs have no embedding space"
    );
}

// 14. Removal cannot report NotInstalled until platform deletion confirmed.
#[test]
fn removal_requires_platform_confirmation() {
    let i = installer(gen_entry());
    drive_to_verifying(&i);
    assert!(i.verify(observed_ok()));
    assert!(i.on_published(1));
    assert!(i.on_removal_started());
    assert_eq!(i.phase(), ModelPackPhase::Removing);
    assert!(
        i.installed().is_some(),
        "record survives until deletion confirmed"
    );
    assert!(i.on_removal_failed("fs busy".into()));
    assert!(matches!(i.phase(), ModelPackPhase::Failed { .. }));
    assert!(i.on_removal_started());
    assert!(i.on_removal_confirmed());
    assert_eq!(i.phase(), ModelPackPhase::NotInstalled);
    assert!(i.installed().is_none());
}

// 13. Read paths never mutate.
#[test]
fn read_paths_never_mutate() {
    let i = installer(gen_entry());
    drive_to_verifying(&i);
    for _ in 0..20 {
        let _ = i.phase();
        let _ = i.installed();
    }
    assert_eq!(i.phase(), ModelPackPhase::Verifying);
}

// Zero-byte required artifacts rejected.
#[test]
fn zero_byte_required_artifacts_rejected() {
    let mut e = gen_entry();
    e.artifacts[0].byte_size = 0;
    assert!(validate_catalog_entry(e)
        .iter()
        .any(|m| m.contains("zero-byte")));
}
