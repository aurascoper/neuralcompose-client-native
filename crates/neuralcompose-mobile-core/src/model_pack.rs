//! Model-pack contracts (M7-A, ADR-001). Three distinct records — catalog
//! entry (what the app offers), installation state (deterministic, Rust-
//! owned), installed record (created only after atomic publication).
//! Effect-free: shells download/verify bytes and report facts.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ModelPackKind {
    Generation,
    Embedding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ModelArtifactKind {
    Weights,
    Tokenizer,
    Config,
    Auxiliary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModelArtifact {
    pub artifact_id: String,
    pub kind: ModelArtifactKind,
    /// Validated RELATIVE path — never absolute, never traversing.
    pub relative_path: String,
    pub byte_size: u64,
    pub sha256_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DeviceRequirements {
    pub minimum_ram_mb: u32,
    pub device_class: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum EmbeddingPooling {
    Mean,
    Cls,
    LastToken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum EmbeddingNormalization {
    None,
    L2,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct GenerationContract {
    pub tokenizer_id: String,
    pub context_cap: u32,
    pub prompt_template_id: String,
    pub compatible_prompt_profiles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EmbeddingContract {
    pub tokenizer_id: String,
    pub dimensions: u32,
    pub pooling: EmbeddingPooling,
    pub normalization: EmbeddingNormalization,
    pub task_instruction: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModelPackCatalogEntry {
    pub schema_version: u32,
    pub pack_id: String,
    pub pack_version: String,
    pub kind: ModelPackKind,
    pub model_family: String,
    pub model_revision: String,
    pub quantization: Option<String>,
    pub artifact_format: String,
    pub license_id: String,
    pub source_repository: String,
    pub runtime_abi: String,
    pub minimum_core_version: String,
    pub artifacts: Vec<ModelArtifact>,
    pub requirements: DeviceRequirements,
    pub generation: Option<GenerationContract>,
    pub embedding: Option<EmbeddingContract>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ModelPackFailure {
    DownloadFailed { reason: String },
    SizeMismatch { artifact_id: String },
    DigestMismatch { artifact_id: String },
    UndeclaredArtifact { relative_path: String },
    MissingArtifact { artifact_id: String },
    SchemaInvalid { reason: String },
    RuntimeAbiIncompatible,
    RemovalFailed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ModelPackPhase {
    NotInstalled,
    Queued,
    Downloading {
        received_bytes: u64,
        total_bytes: u64,
    },
    Verifying,
    Ready,
    Failed {
        reason: ModelPackFailure,
    },
    Removing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct VerifiedArtifact {
    pub artifact_id: String,
    pub relative_path: String,
    pub byte_size: u64,
    pub sha256_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct InstalledModelPack {
    pub pack_id: String,
    pub pack_version: String,
    pub installed_at_ms: u64,
    pub artifact_digests: Vec<VerifiedArtifact>,
    pub runtime_abi: String,
    pub verification_policy_version: u32,
}

/// What the shell observed on disk after download, per artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ObservedArtifact {
    pub relative_path: String,
    pub byte_size: u64,
    pub sha256_hex: String,
}

fn valid_sha256(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn valid_relative_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.split('/').any(|c| c.is_empty() || c == "." || c == "..")
}

/// Structural validation of a catalog entry. Empty vec = valid.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_catalog_entry(entry: ModelPackCatalogEntry) -> Vec<String> {
    let mut errs = Vec::new();
    if entry.artifacts.is_empty() {
        errs.push("no artifacts declared".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut paths = std::collections::HashSet::new();
    for a in &entry.artifacts {
        if !ids.insert(a.artifact_id.clone()) {
            errs.push(format!("duplicate artifact id: {}", a.artifact_id));
        }
        if !paths.insert(a.relative_path.clone()) {
            errs.push(format!("duplicate relative path: {}", a.relative_path));
        }
        if !valid_relative_path(&a.relative_path) {
            errs.push(format!("invalid relative path: {}", a.relative_path));
        }
        if !valid_sha256(&a.sha256_hex) {
            errs.push(format!("invalid sha256 for {}", a.artifact_id));
        }
        if a.byte_size == 0
            && matches!(
                a.kind,
                ModelArtifactKind::Weights | ModelArtifactKind::Tokenizer
            )
        {
            errs.push(format!("zero-byte required artifact: {}", a.artifact_id));
        }
    }
    match entry.kind {
        ModelPackKind::Generation => {
            if entry.generation.is_none() {
                errs.push("generation pack missing generation contract".into());
            }
            if entry.embedding.is_some() {
                errs.push("generation pack must not carry embedding contract".into());
            }
        }
        ModelPackKind::Embedding => {
            if entry.embedding.is_none() {
                errs.push("embedding pack missing embedding contract".into());
            }
            if entry.generation.is_some() {
                errs.push("embedding pack must not carry generation contract".into());
            }
        }
    }
    errs
}

/// Embedding-space identity: any change to revision, weight digest,
/// tokenizer digest, dimension, pooling, normalization, or task instruction
/// yields a different identity. Vectors from differing identities must
/// never share an index.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn embedding_space_identity(entry: ModelPackCatalogEntry) -> Option<String> {
    let e = entry.embedding.as_ref()?;
    let weights = entry
        .artifacts
        .iter()
        .find(|a| a.kind == ModelArtifactKind::Weights)
        .map(|a| a.sha256_hex.as_str())
        .unwrap_or("");
    let tokenizer = entry
        .artifacts
        .iter()
        .find(|a| a.kind == ModelArtifactKind::Tokenizer)
        .map(|a| a.sha256_hex.as_str())
        .unwrap_or("");
    let material = format!(
        "{}|{}|{}|{}|{:?}|{:?}|{}",
        entry.model_revision,
        weights,
        tokenizer,
        e.dimensions,
        e.pooling,
        e.normalization,
        e.task_instruction.clone().unwrap_or_default()
    );
    Some(crate::audio::sha256_hex(material.into_bytes()))
}

struct PackInner {
    phase: ModelPackPhase,
    /// Survives failed updates — the previously Ready version.
    installed: Option<InstalledModelPack>,
}

/// Deterministic installation state machine. Shell reports events; Ready is
/// entered only through full verification + explicit atomic publication.
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct ModelPackInstaller {
    entry: ModelPackCatalogEntry,
    supported_abis: Vec<String>,
    verification_policy_version: u32,
    inner: Mutex<PackInner>,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl ModelPackInstaller {
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn new(
        entry: ModelPackCatalogEntry,
        supported_abis: Vec<String>,
        verification_policy_version: u32,
        previously_installed: Option<InstalledModelPack>,
    ) -> Self {
        Self {
            entry,
            supported_abis,
            verification_policy_version,
            inner: Mutex::new(PackInner {
                phase: ModelPackPhase::NotInstalled,
                installed: previously_installed,
            }),
        }
    }

    pub fn phase(&self) -> ModelPackPhase {
        self.inner.lock().unwrap().phase.clone()
    }

    pub fn installed(&self) -> Option<InstalledModelPack> {
        self.inner.lock().unwrap().installed.clone()
    }

    /// Explicit user consent recorded by the shell starts the transaction.
    pub fn on_queued(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.phase {
            ModelPackPhase::NotInstalled | ModelPackPhase::Failed { .. } => {
                g.phase = ModelPackPhase::Queued;
                true
            }
            _ => false,
        }
    }

    pub fn on_download_progress(&self, received_bytes: u64, total_bytes: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.phase {
            ModelPackPhase::Queued | ModelPackPhase::Downloading { .. } => {
                g.phase = ModelPackPhase::Downloading {
                    received_bytes,
                    total_bytes,
                };
                true
            }
            _ => false,
        }
    }

    pub fn on_download_failed(&self, reason: String) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.phase {
            ModelPackPhase::Queued | ModelPackPhase::Downloading { .. } => {
                g.phase = ModelPackPhase::Failed {
                    reason: ModelPackFailure::DownloadFailed { reason },
                };
                true
            }
            _ => false,
        }
    }

    pub fn on_download_complete(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.phase {
            ModelPackPhase::Downloading { .. } | ModelPackPhase::Queued => {
                g.phase = ModelPackPhase::Verifying;
                true
            }
            _ => false,
        }
    }

    /// Full verification against the catalog entry: every declared artifact
    /// present with exact size + digest; no undeclared extras; runtime ABI
    /// supported. Failure preserves any previously Ready version.
    pub fn verify(&self, observed: Vec<ObservedArtifact>) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != ModelPackPhase::Verifying {
            return false;
        }
        if !self.supported_abis.contains(&self.entry.runtime_abi) {
            g.phase = ModelPackPhase::Failed {
                reason: ModelPackFailure::RuntimeAbiIncompatible,
            };
            return false;
        }
        let schema_errs = validate_catalog_entry(self.entry.clone());
        if !schema_errs.is_empty() {
            g.phase = ModelPackPhase::Failed {
                reason: ModelPackFailure::SchemaInvalid {
                    reason: schema_errs.join("; "),
                },
            };
            return false;
        }
        for a in &self.entry.artifacts {
            match observed.iter().find(|o| o.relative_path == a.relative_path) {
                None => {
                    g.phase = ModelPackPhase::Failed {
                        reason: ModelPackFailure::MissingArtifact {
                            artifact_id: a.artifact_id.clone(),
                        },
                    };
                    return false;
                }
                Some(o) => {
                    if o.byte_size != a.byte_size {
                        g.phase = ModelPackPhase::Failed {
                            reason: ModelPackFailure::SizeMismatch {
                                artifact_id: a.artifact_id.clone(),
                            },
                        };
                        return false;
                    }
                    if o.sha256_hex != a.sha256_hex {
                        g.phase = ModelPackPhase::Failed {
                            reason: ModelPackFailure::DigestMismatch {
                                artifact_id: a.artifact_id.clone(),
                            },
                        };
                        return false;
                    }
                }
            }
        }
        for o in &observed {
            if !self
                .entry
                .artifacts
                .iter()
                .any(|a| a.relative_path == o.relative_path)
            {
                g.phase = ModelPackPhase::Failed {
                    reason: ModelPackFailure::UndeclaredArtifact {
                        relative_path: o.relative_path.clone(),
                    },
                };
                return false;
            }
        }
        true // still Verifying; Ready requires explicit atomic publication
    }

    /// Shell atomically promoted the verified directory. Only now Ready.
    pub fn on_published(&self, installed_at_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != ModelPackPhase::Verifying {
            return false;
        }
        g.installed = Some(InstalledModelPack {
            pack_id: self.entry.pack_id.clone(),
            pack_version: self.entry.pack_version.clone(),
            installed_at_ms,
            artifact_digests: self
                .entry
                .artifacts
                .iter()
                .map(|a| VerifiedArtifact {
                    artifact_id: a.artifact_id.clone(),
                    relative_path: a.relative_path.clone(),
                    byte_size: a.byte_size,
                    sha256_hex: a.sha256_hex.clone(),
                })
                .collect(),
            runtime_abi: self.entry.runtime_abi.clone(),
            verification_policy_version: self.verification_policy_version,
        });
        g.phase = ModelPackPhase::Ready;
        true
    }

    pub fn on_removal_started(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.phase {
            ModelPackPhase::Ready | ModelPackPhase::Failed { .. } => {
                g.phase = ModelPackPhase::Removing;
                true
            }
            _ => false,
        }
    }

    /// NotInstalled only after the platform confirmed deletion.
    pub fn on_removal_confirmed(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != ModelPackPhase::Removing {
            return false;
        }
        g.installed = None;
        g.phase = ModelPackPhase::NotInstalled;
        true
    }

    pub fn on_removal_failed(&self, reason: String) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != ModelPackPhase::Removing {
            return false;
        }
        g.phase = ModelPackPhase::Failed {
            reason: ModelPackFailure::RemovalFailed { reason },
        };
        true
    }
}
