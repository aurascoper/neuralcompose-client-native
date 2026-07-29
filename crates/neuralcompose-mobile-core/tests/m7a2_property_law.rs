// M7-A2 property law (ADR-002): idempotence, equivariance, invariance, and
// the non-invariances that must never collapse. Both polarities throughout.

use neuralcompose_mobile_core::model_pack::*;
use neuralcompose_mobile_core::property_law::*;

type Mutation<T> = Box<dyn Fn(&mut T)>;
use neuralcompose_mobile_core::provider::*;
use neuralcompose_mobile_core::{next_reconnect, SocketEvent, StreamMonitor};

fn sha(n: u8) -> String {
    format!("{:02x}", n).repeat(32)
}

// ---------- idempotence ----------

#[test]
fn provider_resolution_is_idempotent() {
    let descriptors = vec![ProviderDescriptor {
        provider_id: "local-qwen".into(),
        transport: ProviderTransport::OnDeviceModelPack,
        locality: ProviderLocality::OnDevice,
        credential_requirement: CredentialRequirement::NotRequired,
        capabilities: ProviderCapabilities {
            generation: true,
            embeddings: false,
            streaming: true,
            cancellation: true,
        },
    }];
    let availability = vec![ProviderAvailability {
        provider_id: "local-qwen".into(),
        credential_state: CredentialState::NotRequired,
        probe: AvailabilityProbe::Verified,
    }];
    let call = |d: Vec<ProviderDescriptor>, a: Vec<ProviderAvailability>| {
        resolve_provider_identity(
            "local-qwen".into(),
            "m".into(),
            "m".into(),
            None,
            d,
            a,
            Some("focused".into()),
            Some("abc".into()),
        )
    };
    assert_eq!(
        call(descriptors.clone(), availability.clone()),
        call(descriptors.clone(), availability.clone()),
        "f(x) == f(x)"
    );
    // Polarity: a different input genuinely differs.
    let other = resolve_provider_identity(
        "local-qwen".into(),
        "m".into(),
        "different".into(),
        None,
        descriptors.clone(),
        availability.clone(),
        Some("focused".into()),
        Some("abc".into()),
    );
    assert_ne!(call(descriptors, availability), other);
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

#[test]
fn verification_and_publication_are_idempotent() {
    // Verifying the same inventory twice yields the same receipt (proved via
    // the digest recorded on publication) and publishing twice creates no
    // second installation.
    let a = installer(gen_entry());
    assert!(a.on_queued() && a.on_download_complete());
    assert!(a.verify(observed_ok()));
    assert!(a.verify(observed_ok()), "re-verify is legal and stable");
    assert!(a.on_published(1));
    let first = a.active_installation().unwrap();

    assert!(
        !a.on_published(2),
        "second publish is a no-op, not a duplicate"
    );
    let after = a.active_installation().unwrap();
    assert_eq!(first, after, "no duplicate installation, no mutated record");

    // An independent installer over identical inputs reaches an identical
    // inventory digest — the receipt is a function of the inventory.
    let b = installer(gen_entry());
    assert!(b.on_queued() && b.on_download_complete());
    assert!(b.verify(observed_ok()));
    assert!(b.on_published(1));
    assert_eq!(
        first.verified_inventory_digest,
        b.active_installation().unwrap().verified_inventory_digest
    );
    // Polarity: different bytes → different digest.
    let mut changed = gen_entry();
    changed.artifacts[0].sha256_hex = sha(0xcc);
    let c = installer(changed.clone());
    assert!(c.on_queued() && c.on_download_complete());
    let mut obs = observed_ok();
    obs[0].sha256_hex = sha(0xcc);
    assert!(c.verify(obs));
    assert!(c.on_published(1));
    assert_ne!(
        first.verified_inventory_digest,
        c.active_installation().unwrap().verified_inventory_digest
    );
}

#[test]
fn removing_an_absent_pack_stays_absent() {
    let i = installer(gen_entry());
    assert_eq!(i.phase(), ModelPackPhase::NotInstalled);
    assert!(
        !i.on_removal_started(),
        "removal of an absent pack is refused"
    );
    assert_eq!(i.phase(), ModelPackPhase::NotInstalled);
    assert!(i.active_installation().is_none());
    // Polarity: an installed pack CAN start removal.
    let j = installer(gen_entry());
    assert!(j.on_queued() && j.on_download_complete());
    assert!(j.verify(observed_ok()) && j.on_published(1));
    assert!(j.on_removal_started());
}

#[test]
fn indexing_the_same_content_and_space_is_idempotent() {
    let key = IndexEntryKey {
        content_sha256_hex: sha(0x11),
        embedding_space_identity: sha(0x22),
    };
    let deduped = dedupe_index_entries(vec![key.clone(), key.clone(), key.clone()]);
    assert_eq!(deduped.len(), 1, "one entry, however many times indexed");
    // Fixed point: feeding the output back in changes nothing.
    assert_eq!(dedupe_index_entries(deduped.clone()), deduped);

    // Same content, DIFFERENT embedding space → two distinct entries that
    // must never share an index.
    let other_space = IndexEntryKey {
        embedding_space_identity: sha(0x33),
        ..key.clone()
    };
    assert_eq!(
        dedupe_index_entries(vec![key.clone(), other_space.clone()]).len(),
        2
    );
    assert!(!shares_index(key.clone(), other_space));
    assert_ne!(
        index_entry_identity(key.clone()),
        index_entry_identity(IndexEntryKey {
            content_sha256_hex: sha(0x99),
            ..key.clone()
        })
    );
    // Polarity: same space shares an index.
    assert!(shares_index(
        key.clone(),
        IndexEntryKey {
            content_sha256_hex: sha(0x99),
            ..key
        }
    ));
}

// ---------- time-origin equivariance ----------

#[test]
fn stream_phase_is_time_origin_equivariant() {
    let frame = |t: f64| format!(r#"{{"timestamp":{t},"channels":[1.0,2.0,3.0,4.0]}}"#, t = t);
    let run = |origin: u64| {
        let m = StreamMonitor::with_defaults();
        m.on_socket_event(SocketEvent::Opened, origin);
        let mut phases = Vec::new();
        for step in [0u64, 500, 1_000, 3_000, 9_000, 20_000] {
            let now = origin + step;
            if step % 1_000 == 0 && step < 2_000 {
                m.on_frame(frame(step as f64 / 1000.0), now);
            }
            phases.push(format!("{:?}", m.phase(now)));
        }
        phases
    };
    // Shifting every monotonic timestamp by a constant must not change the
    // phase sequence at all.
    let base = run(0);
    for delta in [1u64, 5_000, 86_400_000, 1_000_000_000] {
        assert_eq!(base, run(delta), "phase sequence shifted at Δ={delta}");
    }
    // Polarity: different RELATIVE spacing does change it — the property is
    // equivariance under shift, not blindness to elapsed time.
    let m = StreamMonitor::with_defaults();
    m.on_socket_event(SocketEvent::Opened, 0);
    m.on_frame(frame(0.0), 0);
    assert_ne!(
        format!("{:?}", m.phase(0)),
        format!("{:?}", m.phase(60_000)),
        "elapsed time must still matter"
    );
}

#[test]
fn reconnect_decisions_are_time_origin_independent() {
    // The backoff ladder is a function of attempt count, not wall clock.
    let a: Vec<String> = (0..5).map(|n| format!("{:?}", next_reconnect(n))).collect();
    let b: Vec<String> = (0..5).map(|n| format!("{:?}", next_reconnect(n))).collect();
    assert_eq!(a, b);
    assert_ne!(
        format!("{:?}", next_reconnect(0)),
        format!("{:?}", next_reconnect(2)),
        "attempt count must still matter"
    );
}

// ---------- window-shift equivariance ----------

#[test]
fn event_spans_shift_without_changing_what_was_detected() {
    let span = EventSpan {
        event_kind: "jaw-clench".into(),
        window: SampleWindow {
            start_sample: 1_000,
            end_sample: 1_256,
        },
        detector_parameters_digest: sha(0x44),
    };
    for delta in [0i64, 1, 256, -256, 1_000_000] {
        let shifted = shift_event_span(span.clone(), delta).expect("in range");
        assert!(is_window_shift_equivariant(
            span.clone(),
            shifted.clone(),
            delta
        ));
        assert_eq!(shifted.event_kind, span.event_kind, "kind must not change");
        assert_eq!(
            shifted.detector_parameters_digest, span.detector_parameters_digest,
            "detector parameters must not change"
        );
        assert_eq!(
            shifted.window.end_sample - shifted.window.start_sample,
            span.window.end_sample - span.window.start_sample,
            "duration must not change"
        );
    }
    // Polarity: a span that moved by the wrong amount is not equivariant.
    let wrong = shift_event_span(span.clone(), 100).unwrap();
    assert!(!is_window_shift_equivariant(span.clone(), wrong, 101));
    // A shift that would underflow fabricates nothing — it refuses.
    assert_eq!(shift_event_span(span, -2_000), None);
}

// ---------- channel-permutation equivariance ----------

#[test]
fn channels_permute_only_when_labels_travel_with_values() {
    let canonical = vec!["TP9".to_string(), "AF7".into(), "AF8".into(), "TP10".into()];
    let values = vec![1.0, 2.0, 3.0, 4.0];
    // Identity permutation.
    assert_eq!(
        to_canonical_channel_order(values.clone(), canonical.clone()),
        ChannelOrderResult::Ordered {
            values: values.clone()
        }
    );
    // Permuted values WITH matching labels permute back correspondingly.
    let permuted_labels = vec!["AF8".to_string(), "TP9".into(), "TP10".into(), "AF7".into()];
    let permuted_values = vec![3.0, 1.0, 4.0, 2.0];
    assert_eq!(
        to_canonical_channel_order(permuted_values.clone(), permuted_labels),
        ChannelOrderResult::Ordered {
            values: values.clone()
        },
        "labelled permutation is recoverable"
    );
    // The same values WITHOUT labels are refused, never guessed.
    assert_eq!(
        to_canonical_channel_order(permuted_values.clone(), vec![]),
        ChannelOrderResult::Rejected {
            error: ChannelPermutationError::LabelsMissing
        }
    );
    // Malformed label sets fail closed rather than reorder anatomy.
    assert_eq!(
        to_canonical_channel_order(
            values.clone(),
            vec!["TP9".into(), "TP9".into(), "AF8".into(), "TP10".into()]
        ),
        ChannelOrderResult::Rejected {
            error: ChannelPermutationError::DuplicateLabel {
                label: "TP9".into()
            }
        }
    );
    assert_eq!(
        to_canonical_channel_order(
            values.clone(),
            vec!["F7".into(), "AF7".into(), "AF8".into(), "TP10".into()]
        ),
        ChannelOrderResult::Rejected {
            error: ChannelPermutationError::UnknownLabel { label: "F7".into() }
        }
    );
    assert_eq!(
        to_canonical_channel_order(vec![1.0, 2.0], canonical),
        ChannelOrderResult::Rejected {
            error: ChannelPermutationError::LengthMismatch
        }
    );
}

// ---------- invariance vs non-invariance of embedding identity ----------

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
        task_instruction: Some("retrieve related entries".into()),
    });
    e
}

#[test]
fn embedding_identity_invariances_and_non_invariances() {
    let base = embedding_space_identity(emb_entry()).unwrap();
    // INVARIANT: declaration order of artifacts.
    let mut reordered = emb_entry();
    reordered.artifacts.reverse();
    assert_eq!(
        embedding_space_identity(reordered).unwrap(),
        base,
        "artifact order must not change embedding identity"
    );
    // INVARIANT: fields outside the numerical contract (licence, source).
    let mut relicensed = emb_entry();
    relicensed.license_id = "MIT".into();
    relicensed.source_repository = "elsewhere/mirror".into();
    assert_eq!(
        embedding_space_identity(relicensed).unwrap(),
        base,
        "a mirror URL must not fork the embedding space"
    );
    // NON-INVARIANT: each of these must move the identity.
    let mutations: Vec<(&str, Mutation<ModelPackCatalogEntry>)> = vec![
        (
            "model revision",
            Box::new(|e: &mut ModelPackCatalogEntry| e.model_revision = "0.6b".into()),
        ),
        (
            "model family",
            Box::new(|e: &mut ModelPackCatalogEntry| e.model_family = "bge".into()),
        ),
        (
            "weight shard",
            Box::new(|e: &mut ModelPackCatalogEntry| e.artifacts[0].sha256_hex = sha(0xcd)),
        ),
        (
            "tokenizer",
            Box::new(|e: &mut ModelPackCatalogEntry| e.artifacts[1].sha256_hex = sha(0xce)),
        ),
        (
            "dimensions",
            Box::new(|e: &mut ModelPackCatalogEntry| {
                e.embedding.as_mut().unwrap().dimensions = 384
            }),
        ),
        (
            "pooling",
            Box::new(|e: &mut ModelPackCatalogEntry| {
                e.embedding.as_mut().unwrap().pooling = EmbeddingPooling::Cls
            }),
        ),
        (
            "normalization",
            Box::new(|e: &mut ModelPackCatalogEntry| {
                e.embedding.as_mut().unwrap().normalization = EmbeddingNormalization::None
            }),
        ),
        (
            "task instruction",
            Box::new(|e: &mut ModelPackCatalogEntry| {
                e.embedding.as_mut().unwrap().task_instruction = Some("classify".into())
            }),
        ),
    ];
    for (name, mutate) in mutations {
        let mut e = emb_entry();
        mutate(&mut e);
        assert_ne!(
            embedding_space_identity(e).unwrap(),
            base,
            "{name} must change the embedding-space identity"
        );
    }
}
