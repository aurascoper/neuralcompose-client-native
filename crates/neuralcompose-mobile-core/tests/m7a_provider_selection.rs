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
                verified_ready: false,
            },
            ProviderAvailability {
                provider_id: "neuralcompose-openai".into(),
                credential_state: CredentialState::Available,
                verified_ready: true,
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
            verified_ready: true,
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
            verified_ready: true,
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
            verified_ready: true,
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
            verified_ready: true,
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
            verified_ready: true,
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
            verified_ready: true,
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
