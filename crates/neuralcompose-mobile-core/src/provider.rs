//! Provider selection semantics (M7-A, ADR-001; review-amended).
//! Deterministic and effect-free. No implicit provider substitution; no
//! fabricated facts (an unknown provider has NO transport); fail closed on
//! every configuration inconsistency.

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
    NotVerified {
        reason: String,
    },
    /// Duplicate descriptors/availability rows, provider_id disagreement,
    /// or an inconsistent credential report — fail closed, never guess.
    InconsistentConfiguration {
        reason: String,
    },
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

/// The provider-specific readiness contract's observed status. A single
/// boolean cannot distinguish not-checked, checking, and failed — this can.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AvailabilityProbe {
    NotChecked,
    Checking,
    Verified,
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProviderAvailability {
    pub provider_id: String,
    pub credential_state: CredentialState,
    pub probe: AvailabilityProbe,
}

/// Explicitly registered, PROVIDER-SCOPED model equivalence (model names
/// are not globally unique).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModelAlias {
    pub provider_id: String,
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
    /// None for an unknown provider — the transport is unknowable and is
    /// never fabricated (telemetry must not contain false facts).
    pub transport: Option<ProviderTransport>,
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
/// NEVER substitutes a provider; fail closed on inconsistent configuration.
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
    let matching_desc: Vec<&ProviderDescriptor> = descriptors
        .iter()
        .filter(|d| d.provider_id == requested_provider_id)
        .collect();
    let matching_avail: Vec<&ProviderAvailability> = availability
        .iter()
        .filter(|a| a.provider_id == requested_provider_id)
        .collect();

    let inconsistent = |reason: &str| ProviderReadiness::Unavailable {
        reason: ProviderFailure::InconsistentConfiguration {
            reason: reason.to_string(),
        },
    };

    let (transport, locality, caps, readiness) = if matching_desc.len() > 1 {
        (
            None,
            ProviderLocality::Unresolved,
            empty_caps(),
            inconsistent("duplicate descriptors"),
        )
    } else if let Some(d) = matching_desc.first() {
        if matching_avail.len() > 1 {
            (
                Some(d.transport),
                d.locality,
                d.capabilities.clone(),
                inconsistent("duplicate availability rows"),
            )
        } else {
            let readiness = match matching_avail.first() {
                None => ProviderReadiness::Unconfigured,
                Some(a) => {
                    if a.provider_id != d.provider_id {
                        inconsistent("availability provider_id disagreement")
                    } else if d.credential_requirement == CredentialRequirement::NotRequired
                        && a.credential_state == CredentialState::Missing
                    {
                        inconsistent("credential state Missing for NotRequired provider")
                    } else if d.credential_requirement == CredentialRequirement::Required
                        && a.credential_state != CredentialState::Available
                    {
                        ProviderReadiness::Unavailable {
                            reason: ProviderFailure::MissingCredentials,
                        }
                    } else {
                        match &a.probe {
                            AvailabilityProbe::Verified => ProviderReadiness::Ready,
                            AvailabilityProbe::Checking => ProviderReadiness::Verifying,
                            AvailabilityProbe::NotChecked => match d.transport {
                                // No usable active pack → unavailable, no
                                // cloud request.
                                ProviderTransport::OnDeviceModelPack => {
                                    ProviderReadiness::Unavailable {
                                        reason: ProviderFailure::LocalPackNotReady,
                                    }
                                }
                                _ => ProviderReadiness::Configured,
                            },
                            AvailabilityProbe::Failed { reason } => {
                                ProviderReadiness::Unavailable {
                                    reason: ProviderFailure::NotVerified {
                                        reason: reason.clone(),
                                    },
                                }
                            }
                        }
                    }
                }
            };
            (
                Some(d.transport),
                d.locality,
                d.capabilities.clone(),
                readiness,
            )
        }
    } else {
        (
            None, // unknown transport stays absent — never fabricated
            ProviderLocality::Unresolved,
            empty_caps(),
            ProviderReadiness::Unavailable {
                reason: ProviderFailure::UnknownProvider,
            },
        )
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

/// Structural alias validation: no self-aliases, no alias mapped to more
/// than one canonical within a provider, no alias id reused as a canonical
/// within the same provider (cycles). Empty vec = valid.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_model_aliases(aliases: Vec<ModelAlias>) -> Vec<String> {
    let mut errs = Vec::new();
    let mut alias_to_canonical: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    for a in &aliases {
        if a.alias_model_id == a.canonical_model_id {
            errs.push(format!(
                "self-alias '{}' ({})",
                a.alias_model_id, a.provider_id
            ));
        }
        let key = (a.provider_id.clone(), a.alias_model_id.clone());
        if let Some(existing) = alias_to_canonical.get(&key) {
            if existing != &a.canonical_model_id {
                errs.push(format!(
                    "alias '{}' maps to multiple canonicals ({})",
                    a.alias_model_id, a.provider_id
                ));
            }
        } else {
            alias_to_canonical.insert(key, a.canonical_model_id.clone());
        }
    }
    for a in &aliases {
        if aliases
            .iter()
            .any(|b| b.provider_id == a.provider_id && b.alias_model_id == a.canonical_model_id)
        {
            errs.push(format!(
                "canonical '{}' is itself aliased ({})",
                a.canonical_model_id, a.provider_id
            ));
        }
    }
    errs
}

/// Substitution disclosure: provider mismatch is always substitution; model
/// mismatch is substitution unless a VALID, provider-scoped alias proves
/// equivalence. An invalid alias set authorizes nothing (fail closed).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn is_substitution(identity: ResolvedProviderIdentity, aliases: Vec<ModelAlias>) -> bool {
    if identity.requested_provider_id != identity.resolved_provider_id {
        return true;
    }
    if identity.requested_model_id == identity.resolved_model_id {
        return false;
    }
    if !validate_model_aliases(aliases.clone()).is_empty() {
        return true; // invalid alias set can never authorize equivalence
    }
    let equivalent = aliases.iter().any(|a| {
        a.provider_id == identity.resolved_provider_id
            && ((a.alias_model_id == identity.requested_model_id
                && a.canonical_model_id == identity.resolved_model_id)
                || (a.alias_model_id == identity.resolved_model_id
                    && a.canonical_model_id == identity.requested_model_id))
    });
    !equivalent
}

/// Unknown locality must present conservatively: possible egress.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn presents_as_possible_egress(locality: ProviderLocality) -> bool {
    !matches!(locality, ProviderLocality::OnDevice)
}
