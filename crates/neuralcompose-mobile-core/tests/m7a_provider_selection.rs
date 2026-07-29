// M7-A provider-selection regressions (ADR-001). Both polarities asserted so
// neutering a comparison (mutation) flips at least one test.

use neuralcompose_mobile_core::provider::*;

fn caps() -> ProviderCapabilities {
    ProviderCapabilities {
        generation: true,
        embeddings: false,
        streaming: true,
        cancellation: true,
    }
}

fn local_desc() -> ProviderDescriptor {
    ProviderDescriptor {
        provider_id: "local-qwen".into(),
        transport: ProviderTransport::OnDeviceModelPack,
        locality: ProviderLocality::OnDevice,
        credential_requirement: CredentialRequirement::NotRequired,
        capabilities: caps(),
    }
}

fn cloud_desc() -> ProviderDescriptor {
    ProviderDescriptor {
        provider_id: "neuralcompose-openai".into(),
        transport: ProviderTransport::BrokeredCloud,
        locality: ProviderLocality::Cloud,
        credential_requirement: CredentialRequirement::Required,
        capabilities: caps(),
    }
}

fn resolve(provider: &str, avail: Vec<ProviderAvailability>) -> ResolvedProviderIdentity {
    resolve_provider_identity(
        provider.into(),
        "qwen2.5-0.5b".into(),
        "qwen2.5-0.5b".into(),
        None,
        vec![local_desc(), cloud_desc()],
        avail,
        Some("focused".into()),
        Some("abc123".into()),
    )
}

// 1. Missing/corrupt local pack never selects a cloud provider.
#[test]
fn missing_local_pack_is_unavailable_never_cloud() {
    let id = resolve(
        "local-qwen",
        vec![
            ProviderAvailability {
                provider_id: "local-qwen".into(),
                credential_state: CredentialState::NotRequired,
                probe: AvailabilityProbe::NotChecked,
            },
            ProviderAvailability {
                provider_id: "neuralcompose-openai".into(),
                credential_state: CredentialState::Available,
                probe: AvailabilityProbe::Verified,
            },
        ],
    );
    assert_eq!(id.resolved_provider_id, "local-qwen", "no rerouting, ever");
    assert_eq!(
        id.readiness,
        ProviderReadiness::Unavailable {
            reason: ProviderFailure::LocalPackNotReady
        }
    );
    assert_eq!(id.locality, ProviderLocality::OnDevice);
    // Polarity: a ready pack IS Ready.
    let ok = resolve(
        "local-qwen",
        vec![ProviderAvailability {
            provider_id: "local-qwen".into(),
            credential_state: CredentialState::NotRequired,
            probe: AvailabilityProbe::Verified,
        }],
    );
    assert_eq!(ok.readiness, ProviderReadiness::Ready);
}

// 2/3. Substitution disclosure; alias authorizes equivalence.
#[test]
fn substitution_is_exposed_and_alias_authorizes() {
    let mut id = resolve(
        "local-qwen",
        vec![ProviderAvailability {
            provider_id: "local-qwen".into(),
            credential_state: CredentialState::NotRequired,
            probe: AvailabilityProbe::Verified,
        }],
    );
    assert!(
        !is_substitution(id.clone(), vec![]),
        "same ids: not substitution"
    );

    id.resolved_model_id = "qwen2.5-0.5b-instruct-q4".into();
    assert!(
        is_substitution(id.clone(), vec![]),
        "model mismatch without alias"
    );
    let alias = ModelAlias {
        provider_id: "local-qwen".into(),
        canonical_model_id: "qwen2.5-0.5b-instruct-q4".into(),
        alias_model_id: "qwen2.5-0.5b".into(),
    };
    assert!(
        !is_substitution(id.clone(), vec![alias]),
        "registered alias = equivalence"
    );

    // Requested identity must be preserved for disclosure (mutation guard).
    assert_eq!(id.requested_provider_id, "local-qwen");
    assert_eq!(id.requested_model_id, "qwen2.5-0.5b");
    let mut provider_swapped = id.clone();
    provider_swapped.resolved_provider_id = "neuralcompose-openai".into();
    assert!(
        is_substitution(provider_swapped, vec![]),
        "provider mismatch is always substitution"
    );
}

// Missing credentials → unavailable, no fallback.
#[test]
fn missing_credentials_is_unavailable_no_fallback() {
    let id = resolve(
        "neuralcompose-openai",
        vec![ProviderAvailability {
            provider_id: "neuralcompose-openai".into(),
            credential_state: CredentialState::Missing,
            probe: AvailabilityProbe::Verified,
        }],
    );
    assert_eq!(
        id.readiness,
        ProviderReadiness::Unavailable {
            reason: ProviderFailure::MissingCredentials
        }
    );
    assert_eq!(id.resolved_provider_id, "neuralcompose-openai");
    // Polarity: available credentials → Ready.
    let ok = resolve(
        "neuralcompose-openai",
        vec![ProviderAvailability {
            provider_id: "neuralcompose-openai".into(),
            credential_state: CredentialState::Available,
            probe: AvailabilityProbe::Verified,
        }],
    );
    assert_eq!(ok.readiness, ProviderReadiness::Ready);
}

// Unknown provider/locality → conservative egress presentation.
#[test]
fn unknown_provider_presents_conservative_egress() {
    let id = resolve("nonexistent", vec![]);
    assert_eq!(
        id.readiness,
        ProviderReadiness::Unavailable {
            reason: ProviderFailure::UnknownProvider
        }
    );
    assert_eq!(id.locality, ProviderLocality::Unresolved);
    assert!(presents_as_possible_egress(ProviderLocality::Unresolved));
    assert!(presents_as_possible_egress(ProviderLocality::Cloud));
    assert!(!presents_as_possible_egress(ProviderLocality::OnDevice));
}

// 12. Provider snapshots carry no credential material — only tri-state.
#[test]
fn no_credential_material_in_identity() {
    let id = resolve(
        "neuralcompose-openai",
        vec![ProviderAvailability {
            provider_id: "neuralcompose-openai".into(),
            credential_state: CredentialState::Available,
            probe: AvailabilityProbe::Verified,
        }],
    );
    let dump = format!("{id:?}");
    assert!(!dump.to_lowercase().contains("key"), "{dump}");
    assert!(!dump.contains("sk-"), "{dump}");
    assert!(!dump.to_lowercase().contains("token"), "{dump}");
}

// 15. Explicit cloud selection records cloud locality; never inferred.
#[test]
fn explicit_cloud_selection_is_recorded_not_inferred() {
    let id = resolve(
        "neuralcompose-openai",
        vec![ProviderAvailability {
            provider_id: "neuralcompose-openai".into(),
            credential_state: CredentialState::Available,
            probe: AvailabilityProbe::Verified,
        }],
    );
    assert_eq!(
        id.requested_provider_id, "neuralcompose-openai",
        "user chose cloud explicitly"
    );
    assert_eq!(id.locality, ProviderLocality::Cloud);
    assert!(presents_as_possible_egress(id.locality));
}

// 13. Resolution is pure — identical inputs, identical outputs, no mutation.
#[test]
fn resolution_is_pure() {
    let a = resolve("local-qwen", vec![]);
    let b = resolve("local-qwen", vec![]);
    assert_eq!(a, b);
    assert_eq!(a.readiness, ProviderReadiness::Unconfigured);
}

// ---- Review amendments ----

#[test]
fn unknown_provider_transport_is_absent_not_fabricated() {
    let id = resolve("nonexistent", vec![]);
    assert_eq!(id.transport, None, "transport must never be fabricated");
    assert_eq!(id.locality, ProviderLocality::Unresolved);
    assert!(!id.capabilities.generation && !id.capabilities.embeddings);
    // Polarity: a known provider carries its real transport.
    let known = resolve("local-qwen", vec![]);
    assert_eq!(known.transport, Some(ProviderTransport::OnDeviceModelPack));
}

#[test]
fn probe_enum_produces_all_readiness_states() {
    let mk = |probe| {
        resolve(
            "neuralcompose-openai",
            vec![ProviderAvailability {
                provider_id: "neuralcompose-openai".into(),
                credential_state: CredentialState::Available,
                probe,
            }],
        )
        .readiness
    };
    assert_eq!(
        mk(AvailabilityProbe::NotChecked),
        ProviderReadiness::Configured
    );
    assert_eq!(
        mk(AvailabilityProbe::Checking),
        ProviderReadiness::Verifying
    );
    assert_eq!(mk(AvailabilityProbe::Verified), ProviderReadiness::Ready);
    assert_eq!(
        mk(AvailabilityProbe::Failed {
            reason: "probe timeout".into()
        }),
        ProviderReadiness::Unavailable {
            reason: ProviderFailure::NotVerified {
                reason: "probe timeout".into()
            }
        }
    );
}

#[test]
fn duplicates_and_inconsistencies_fail_closed() {
    // Duplicate descriptors.
    let id = resolve_provider_identity(
        "local-qwen".into(),
        "m".into(),
        "m".into(),
        None,
        vec![local_desc(), local_desc()],
        vec![],
        None,
        None,
    );
    assert!(matches!(
        id.readiness,
        ProviderReadiness::Unavailable {
            reason: ProviderFailure::InconsistentConfiguration { .. }
        }
    ));
    // Duplicate availability rows.
    let row = ProviderAvailability {
        provider_id: "local-qwen".into(),
        credential_state: CredentialState::NotRequired,
        probe: AvailabilityProbe::Verified,
    };
    let id2 = resolve("local-qwen", vec![row.clone(), row]);
    assert!(matches!(
        id2.readiness,
        ProviderReadiness::Unavailable {
            reason: ProviderFailure::InconsistentConfiguration { .. }
        }
    ));
    // NotRequired paired with Missing is an inconsistent report.
    let id3 = resolve(
        "local-qwen",
        vec![ProviderAvailability {
            provider_id: "local-qwen".into(),
            credential_state: CredentialState::Missing,
            probe: AvailabilityProbe::Verified,
        }],
    );
    assert!(matches!(
        id3.readiness,
        ProviderReadiness::Unavailable {
            reason: ProviderFailure::InconsistentConfiguration { .. }
        }
    ));
}

#[test]
fn aliases_are_provider_scoped_and_validated() {
    let mut id = resolve(
        "local-qwen",
        vec![ProviderAvailability {
            provider_id: "local-qwen".into(),
            credential_state: CredentialState::NotRequired,
            probe: AvailabilityProbe::Verified,
        }],
    );
    id.resolved_model_id = "canon".into();
    // Alias registered for ANOTHER provider authorizes nothing.
    let foreign = ModelAlias {
        provider_id: "neuralcompose-openai".into(),
        canonical_model_id: "canon".into(),
        alias_model_id: "qwen2.5-0.5b".into(),
    };
    assert!(is_substitution(id.clone(), vec![foreign]));
    // Same-provider alias authorizes equivalence.
    let scoped = ModelAlias {
        provider_id: "local-qwen".into(),
        canonical_model_id: "canon".into(),
        alias_model_id: "qwen2.5-0.5b".into(),
    };
    assert!(!is_substitution(id.clone(), vec![scoped.clone()]));
    // Invalid alias sets: self-alias, ambiguity, cycle — each rejected and
    // an invalid set authorizes nothing.
    let selfy = ModelAlias {
        provider_id: "p".into(),
        canonical_model_id: "x".into(),
        alias_model_id: "x".into(),
    };
    assert!(!validate_model_aliases(vec![selfy.clone()]).is_empty());
    let ambiguous = vec![
        scoped.clone(),
        ModelAlias {
            provider_id: "local-qwen".into(),
            canonical_model_id: "other".into(),
            alias_model_id: "qwen2.5-0.5b".into(),
        },
    ];
    assert!(!validate_model_aliases(ambiguous.clone()).is_empty());
    assert!(
        is_substitution(id.clone(), ambiguous),
        "invalid set authorizes nothing"
    );
    let cycle = vec![
        scoped.clone(),
        ModelAlias {
            provider_id: "local-qwen".into(),
            canonical_model_id: "qwen2.5-0.5b".into(),
            alias_model_id: "canon".into(),
        },
    ];
    assert!(!validate_model_aliases(cycle).is_empty());
    assert!(
        validate_model_aliases(vec![scoped]).is_empty(),
        "polarity: valid set validates"
    );
}
