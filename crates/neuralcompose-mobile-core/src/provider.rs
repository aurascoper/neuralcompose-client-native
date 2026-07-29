//! Provider selection semantics (M7-A, ADR-001). Deterministic and
//! effect-free: no network, no SDKs, no credentials — the shell reports
//! availability facts; this module decides identity, readiness, and
//! substitution visibility. No implicit provider substitution, ever.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ProviderTransport {
    OnDeviceModelPack,
    SystemModel,
    HttpEndpoint,
    BrokeredCloud,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ProviderLocality {
    OnDevice,
    LocalNetwork,
    RemoteEndpoint,
    Cloud,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CredentialRequirement {
    NotRequired,
    Required,
}

/// The core never sees secrets — only whether one exists where required.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CredentialState {
    NotRequired,
    Missing,
    Available,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProviderCapabilities {
    pub generation: bool,
    pub embeddings: bool,
    pub streaming: bool,
    pub cancellation: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProviderDescriptor {
    pub provider_id: String,
    pub transport: ProviderTransport,
    pub locality: ProviderLocality,
    pub credential_requirement: CredentialRequirement,
    pub capabilities: ProviderCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ProviderFailure {
    UnknownProvider,
    MissingCredentials,
    LocalPackNotReady,
    NotVerified { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ProviderReadiness {
    Unconfigured,
    Configured,
    Verifying,
    Ready,
    Unavailable { reason: ProviderFailure },
}

/// Facts the SHELL reports about one configured provider. `verified_ready`
/// means the provider-specific readiness contract was actually checked
/// (Configured is merely structural presence).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProviderAvailability {
    pub provider_id: String,
    pub credential_state: CredentialState,
    /// For OnDeviceModelPack: is the pack phase Ready? Others: transport
    /// probe passed.
    pub verified_ready: bool,
}

/// Explicitly registered model-equivalence: resolving `alias` to
/// `canonical` is NOT substitution.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModelAlias {
    pub canonical_model_id: String,
    pub alias_model_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ResolvedProviderIdentity {
    pub requested_provider_id: String,
    pub requested_model_id: String,
    pub resolved_provider_id: String,
    pub resolved_model_id: String,
    pub model_digest: Option<String>,
    pub transport: ProviderTransport,
    pub locality: ProviderLocality,
    pub readiness: ProviderReadiness,
    pub prompt_profile: Option<String>,
    pub prompt_hash: Option<String>,
    pub capabilities: ProviderCapabilities,
}

fn empty_caps() -> ProviderCapabilities {
    ProviderCapabilities {
        generation: false,
        embeddings: false,
        streaming: false,
        cancellation: false,
    }
}

/// Resolve a request against configuration + shell-reported availability.
/// NEVER substitutes a provider: the resolved provider is always the
/// requested one; unavailability is expressed, not routed around.
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[allow(clippy::too_many_arguments)]
pub fn resolve_provider_identity(
    requested_provider_id: String,
    requested_model_id: String,
    resolved_model_id: String,
    model_digest: Option<String>,
    descriptors: Vec<ProviderDescriptor>,
    availability: Vec<ProviderAvailability>,
    prompt_profile: Option<String>,
    prompt_hash: Option<String>,
) -> ResolvedProviderIdentity {
    let descriptor = descriptors
        .iter()
        .find(|d| d.provider_id == requested_provider_id);
    let avail = availability
        .iter()
        .find(|a| a.provider_id == requested_provider_id);

    let (transport, locality, caps, readiness) = match descriptor {
        None => (
            ProviderTransport::HttpEndpoint, // placeholder; readiness blocks use
            ProviderLocality::Unresolved,    // unknown → conservative egress
            empty_caps(),
            ProviderReadiness::Unavailable {
                reason: ProviderFailure::UnknownProvider,
            },
        ),
        Some(d) => {
            let readiness = match avail {
                None => ProviderReadiness::Unconfigured,
                Some(a) => {
                    let cred_missing = d.credential_requirement == CredentialRequirement::Required
                        && a.credential_state != CredentialState::Available;
                    if cred_missing {
                        // Missing credentials → unavailable, no fallback.
                        ProviderReadiness::Unavailable {
                            reason: ProviderFailure::MissingCredentials,
                        }
                    } else if !a.verified_ready {
                        match d.transport {
                            // Missing local pack → unavailable, no cloud request.
                            ProviderTransport::OnDeviceModelPack => {
                                ProviderReadiness::Unavailable {
                                    reason: ProviderFailure::LocalPackNotReady,
                                }
                            }
                            _ => ProviderReadiness::Configured,
                        }
                    } else {
                        ProviderReadiness::Ready
                    }
                }
            };
            (d.transport, d.locality, d.capabilities.clone(), readiness)
        }
    };

    ResolvedProviderIdentity {
        requested_provider_id: requested_provider_id.clone(),
        requested_model_id,
        resolved_provider_id: requested_provider_id,
        resolved_model_id,
        model_digest,
        transport,
        locality,
        readiness,
        prompt_profile,
        prompt_hash,
        capabilities: caps,
    }
}

/// Substitution disclosure: provider mismatch is always substitution; model
/// mismatch is substitution unless an explicitly registered alias proves
/// equivalence (in either direction toward the same canonical).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn is_substitution(identity: ResolvedProviderIdentity, aliases: Vec<ModelAlias>) -> bool {
    if identity.requested_provider_id != identity.resolved_provider_id {
        return true;
    }
    if identity.requested_model_id == identity.resolved_model_id {
        return false;
    }
    let equivalent = aliases.iter().any(|a| {
        (a.alias_model_id == identity.requested_model_id
            && a.canonical_model_id == identity.resolved_model_id)
            || (a.alias_model_id == identity.resolved_model_id
                && a.canonical_model_id == identity.requested_model_id)
    });
    !equivalent
}

/// Unknown locality must present conservatively: possible egress.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn presents_as_possible_egress(locality: ProviderLocality) -> bool {
    !matches!(locality, ProviderLocality::OnDevice)
}
