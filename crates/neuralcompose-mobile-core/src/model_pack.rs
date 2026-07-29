//! Model-pack contracts (M7-A, ADR-001; review-amended). Effect-free.
//! All digests are canonical serde_json documents with domain strings —
//! never delimiter concatenation. Ready is reachable only through an
//! inventory-bound verification receipt + explicit atomic publication.
//! Installed records restore ONLY against the trusted catalog set, and
//! the active installation is independent of the current operation.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const VERIFICATION_DOMAIN: &str = "neuralcompose.model-pack-verification.v1";
const EMBEDDING_DOMAIN: &str = "neuralcompose.embedding-space.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ModelPackKind {
    Generation,
    Embedding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    DuplicateObservedPath { relative_path: String },
    SchemaInvalid { reason: String },
    RuntimeAbiIncompatible,
    RemovalFailed { reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum OperationKind {
    Install,
    Update,
    Removal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OperationFailure {
    pub operation: OperationKind,
    pub reason: ModelPackFailure,
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
    pub catalog_entry_digest: String,
    pub verified_inventory_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ObservedArtifact {
    pub relative_path: String,
    pub byte_size: u64,
    pub sha256_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RestoreFailure {
    TrustedCatalogEntryMissing,
    CatalogDigestMismatch,
    InstalledInventoryMismatch,
    UnsupportedRuntimeAbi,
    UnsupportedVerificationPolicy,
    InvalidInstalledRecord { reason: String },
}

/// Independent observability of the usable installation vs the operation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModelPackSnapshot {
    pub operation_phase: ModelPackPhase,
    pub active_installation: Option<InstalledModelPack>,
    /// Provider availability for local inference derives from THIS,
    /// never from `operation_phase == Ready`.
    pub has_usable_active_installation: bool,
    pub operation_failure: Option<OperationFailure>,
    pub restore_failure: Option<RestoreFailure>,
}

// ---------- canonical digests (no delimiter concatenation) ----------

fn sorted_artifacts(entry: &ModelPackCatalogEntry) -> Vec<ModelArtifact> {
    let mut a = entry.artifacts.clone();
    a.sort_by(|x, y| {
        (&x.artifact_id, x.kind, &x.relative_path).cmp(&(&y.artifact_id, y.kind, &y.relative_path))
    });
    a
}

/// Digest of the full catalog entry (artifacts canonically sorted).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn catalog_entry_digest(entry: ModelPackCatalogEntry) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        entry: ModelPackCatalogEntry,
    }
    let mut canonical = entry;
    canonical.artifacts = sorted_artifacts(&canonical);
    let doc = Doc {
        domain: "neuralcompose.model-pack-catalog-entry.v1",
        entry: canonical,
    };
    crate::audio::sha256_hex(serde_json::to_vec(&doc).expect("serialize"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationMaterial {
    domain: &'static str,
    catalog_entry_digest: String,
    pack_id: String,
    pack_version: String,
    schema_version: u32,
    runtime_abi: String,
    verification_policy_version: u32,
    artifacts: Vec<ModelArtifact>,
}

fn inventory_digest(entry: &ModelPackCatalogEntry, policy: u32) -> String {
    let material = VerificationMaterial {
        domain: VERIFICATION_DOMAIN,
        catalog_entry_digest: catalog_entry_digest(entry.clone()),
        pack_id: entry.pack_id.clone(),
        pack_version: entry.pack_version.clone(),
        schema_version: entry.schema_version,
        runtime_abi: entry.runtime_abi.clone(),
        verification_policy_version: policy,
        artifacts: sorted_artifacts(entry),
    };
    crate::audio::sha256_hex(serde_json::to_vec(&material).expect("serialize"))
}

/// Embedding-space identity: canonical document over ALL weight and
/// tokenizer artifacts (sorted), with a domain string. Requires the entry
/// to pass full catalog validation first.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn embedding_space_identity(entry: ModelPackCatalogEntry) -> Option<String> {
    if !validate_catalog_entry(entry.clone()).is_empty() {
        return None;
    }
    let e = entry.embedding.as_ref()?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Art {
        artifact_id: String,
        relative_path: String,
        sha256_hex: String,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        model_family: String,
        model_revision: String,
        weights: Vec<Art>,
        tokenizers: Vec<Art>,
        dimensions: u32,
        pooling: EmbeddingPooling,
        normalization: EmbeddingNormalization,
        task_instruction: Option<String>,
    }
    let pick = |k: ModelArtifactKind| {
        let mut v: Vec<Art> = entry
            .artifacts
            .iter()
            .filter(|a| a.kind == k)
            .map(|a| Art {
                artifact_id: a.artifact_id.clone(),
                relative_path: a.relative_path.clone(),
                sha256_hex: a.sha256_hex.clone(),
            })
            .collect();
        v.sort_by(|x, y| x.relative_path.cmp(&y.relative_path));
        v
    };
    let doc = Doc {
        domain: EMBEDDING_DOMAIN,
        model_family: entry.model_family.clone(),
        model_revision: entry.model_revision.clone(),
        weights: pick(ModelArtifactKind::Weights),
        tokenizers: pick(ModelArtifactKind::Tokenizer),
        dimensions: e.dimensions,
        pooling: e.pooling,
        normalization: e.normalization,
        task_instruction: e.task_instruction.clone(),
    };
    Some(crate::audio::sha256_hex(
        serde_json::to_vec(&doc).expect("serialize"),
    ))
}

// ---------- validation ----------

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

/// Public validation pinned to the RUNNING core version — never
/// caller-supplied.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_catalog_entry(entry: ModelPackCatalogEntry) -> Vec<String> {
    validate_catalog_for_core(entry, env!("CARGO_PKG_VERSION"))
}

/// Testable seam; not exported over FFI.
pub fn validate_catalog_for_core(entry: ModelPackCatalogEntry, core_version: &str) -> Vec<String> {
    let mut errs = Vec::new();
    if entry.schema_version != SUPPORTED_SCHEMA_VERSION {
        errs.push(format!(
            "unsupported schemaVersion {}",
            entry.schema_version
        ));
    }
    match (
        semver::Version::parse(&entry.minimum_core_version),
        semver::Version::parse(core_version),
    ) {
        (Ok(min), Ok(core)) => {
            if min > core {
                errs.push(format!(
                    "minimumCoreVersion {min} exceeds running core {core}"
                ));
            }
        }
        _ => errs.push(format!(
            "malformed minimumCoreVersion {}",
            entry.minimum_core_version
        )),
    }
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
    let has_kind = |k| entry.artifacts.iter().any(|a| a.kind == k);
    if !has_kind(ModelArtifactKind::Weights) {
        errs.push("no weights artifact declared".into());
    }
    if !has_kind(ModelArtifactKind::Tokenizer) {
        errs.push("no tokenizer artifact declared".into());
    }
    let tokenizer_declared = |id: &str| {
        entry
            .artifacts
            .iter()
            .any(|a| a.artifact_id == id && a.kind == ModelArtifactKind::Tokenizer)
    };
    match entry.kind {
        ModelPackKind::Generation => {
            match &entry.generation {
                None => errs.push("generation pack missing generation contract".into()),
                Some(g) => {
                    if !tokenizer_declared(&g.tokenizer_id) {
                        errs.push(format!(
                            "tokenizer_id '{}' not a declared tokenizer",
                            g.tokenizer_id
                        ));
                    }
                }
            }
            if entry.embedding.is_some() {
                errs.push("generation pack must not carry embedding contract".into());
            }
        }
        ModelPackKind::Embedding => {
            match &entry.embedding {
                None => errs.push("embedding pack missing embedding contract".into()),
                Some(e) => {
                    if !tokenizer_declared(&e.tokenizer_id) {
                        errs.push(format!(
                            "tokenizer_id '{}' not a declared tokenizer",
                            e.tokenizer_id
                        ));
                    }
                }
            }
            if entry.generation.is_some() {
                errs.push("embedding pack must not carry generation contract".into());
            }
        }
    }
    errs
}

// ---------- trusted-catalog restoration ----------

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RestoreResult {
    Restored { record: InstalledModelPack },
    Rejected { failure: RestoreFailure },
}

/// Resolve a persisted installed record against the TRUSTED catalog set.
/// The record is never compared against an update target; it must match its
/// own trusted entry exactly. Rejection is visible — callers preserve/
/// quarantine the record for diagnosis, never silently drop it.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn restore_installed_record(
    record: InstalledModelPack,
    trusted_catalog: Vec<ModelPackCatalogEntry>,
    supported_abis: Vec<String>,
    accepted_policy_versions: Vec<u32>,
) -> RestoreResult {
    for a in &record.artifact_digests {
        if !valid_sha256(&a.sha256_hex) || !valid_relative_path(&a.relative_path) {
            return RestoreResult::Rejected {
                failure: RestoreFailure::InvalidInstalledRecord {
                    reason: format!("malformed artifact record {}", a.artifact_id),
                },
            };
        }
    }
    let entry = match trusted_catalog
        .iter()
        .find(|e| e.pack_id == record.pack_id && e.pack_version == record.pack_version)
    {
        None => {
            return RestoreResult::Rejected {
                failure: RestoreFailure::TrustedCatalogEntryMissing,
            }
        }
        Some(e) => e,
    };
    if !validate_catalog_entry(entry.clone()).is_empty() {
        return RestoreResult::Rejected {
            failure: RestoreFailure::InvalidInstalledRecord {
                reason: "trusted entry fails catalog validation".into(),
            },
        };
    }
    if catalog_entry_digest(entry.clone()) != record.catalog_entry_digest {
        return RestoreResult::Rejected {
            failure: RestoreFailure::CatalogDigestMismatch,
        };
    }
    if record.runtime_abi != entry.runtime_abi || !supported_abis.contains(&record.runtime_abi) {
        return RestoreResult::Rejected {
            failure: RestoreFailure::UnsupportedRuntimeAbi,
        };
    }
    if !accepted_policy_versions.contains(&record.verification_policy_version) {
        return RestoreResult::Rejected {
            failure: RestoreFailure::UnsupportedVerificationPolicy,
        };
    }
    // Exact inventory equality: no missing, duplicate, or extra artifacts.
    if record.artifact_digests.len() != entry.artifacts.len() {
        return RestoreResult::Rejected {
            failure: RestoreFailure::InstalledInventoryMismatch,
        };
    }
    let mut seen = std::collections::HashSet::new();
    for rec in &record.artifact_digests {
        if !seen.insert(rec.artifact_id.clone()) {
            return RestoreResult::Rejected {
                failure: RestoreFailure::InstalledInventoryMismatch,
            };
        }
        let matched = entry.artifacts.iter().any(|a| {
            a.artifact_id == rec.artifact_id
                && a.relative_path == rec.relative_path
                && a.byte_size == rec.byte_size
                && a.sha256_hex == rec.sha256_hex
        });
        if !matched {
            return RestoreResult::Rejected {
                failure: RestoreFailure::InstalledInventoryMismatch,
            };
        }
    }
    if inventory_digest(entry, record.verification_policy_version)
        != record.verified_inventory_digest
    {
        return RestoreResult::Rejected {
            failure: RestoreFailure::InstalledInventoryMismatch,
        };
    }
    RestoreResult::Restored { record }
}

// ---------- installer ----------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Idle,
    Queued,
    Downloading { received: u64, total: u64 },
    Verifying,
    Removing,
}

struct PackInner {
    op: Op,
    active: Option<InstalledModelPack>,
    operation_failure: Option<OperationFailure>,
    restore_failure: Option<RestoreFailure>,
    verification_receipt: Option<String>,
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct ModelPackInstaller {
    entry: ModelPackCatalogEntry,
    supported_abis: Vec<String>,
    verification_policy_version: u32,
    inner: Mutex<PackInner>,
}

fn op_kind(active: bool, removing: bool) -> OperationKind {
    if removing {
        OperationKind::Removal
    } else if active {
        OperationKind::Update
    } else {
        OperationKind::Install
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl ModelPackInstaller {
    /// `restored` comes from `restore_installed_record` — Restored's record
    /// becomes the active installation; a Rejected failure is surfaced
    /// visibly in the snapshot (the shell preserves/quarantines the record).
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn new(
        entry: ModelPackCatalogEntry,
        supported_abis: Vec<String>,
        verification_policy_version: u32,
        restored: Option<RestoreResult>,
    ) -> Self {
        let (active, restore_failure) = match restored {
            Some(RestoreResult::Restored { record }) => (Some(record), None),
            Some(RestoreResult::Rejected { failure }) => (None, Some(failure)),
            None => (None, None),
        };
        Self {
            entry,
            supported_abis,
            verification_policy_version,
            inner: Mutex::new(PackInner {
                op: Op::Idle,
                active,
                operation_failure: None,
                restore_failure,
                verification_receipt: None,
            }),
        }
    }

    /// Derived presentation phase. Failed/op states describe the OPERATION;
    /// the active installation stays independently observable.
    pub fn phase(&self) -> ModelPackPhase {
        let g = self.inner.lock().unwrap();
        if let Some(f) = &g.operation_failure {
            return ModelPackPhase::Failed {
                reason: f.reason.clone(),
            };
        }
        match &g.op {
            Op::Queued => ModelPackPhase::Queued,
            Op::Downloading { received, total } => ModelPackPhase::Downloading {
                received_bytes: *received,
                total_bytes: *total,
            },
            Op::Verifying => ModelPackPhase::Verifying,
            Op::Removing => ModelPackPhase::Removing,
            Op::Idle => {
                if g.active.is_some() {
                    ModelPackPhase::Ready
                } else {
                    ModelPackPhase::NotInstalled
                }
            }
        }
    }

    pub fn snapshot(&self) -> ModelPackSnapshot {
        let phase = self.phase();
        let g = self.inner.lock().unwrap();
        ModelPackSnapshot {
            operation_phase: phase,
            active_installation: g.active.clone(),
            has_usable_active_installation: g.active.is_some(),
            operation_failure: g.operation_failure.clone(),
            restore_failure: g.restore_failure.clone(),
        }
    }

    pub fn active_installation(&self) -> Option<InstalledModelPack> {
        self.inner.lock().unwrap().active.clone()
    }

    /// Legal only from Idle (derived Ready/NotInstalled — updates start
    /// from Ready). A Failed operation must be acknowledged first: a failed
    /// removal and a failed update have different filesystem implications.
    pub fn on_queued(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.op != Op::Idle || g.operation_failure.is_some() {
            return false;
        }
        g.verification_receipt = None;
        g.op = Op::Queued;
        true
    }

    pub fn acknowledge_operation_failure(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.operation_failure.is_none() {
            return false;
        }
        g.operation_failure = None;
        g.op = Op::Idle;
        true
    }

    pub fn on_download_progress(&self, received_bytes: u64, total_bytes: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.operation_failure.is_some() {
            return false;
        }
        match g.op {
            Op::Queued | Op::Downloading { .. } => {
                g.verification_receipt = None;
                g.op = Op::Downloading {
                    received: received_bytes,
                    total: total_bytes,
                };
                true
            }
            _ => false,
        }
    }

    pub fn on_download_failed(&self, reason: String) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.operation_failure.is_some() {
            return false;
        }
        match g.op {
            Op::Queued | Op::Downloading { .. } => {
                g.verification_receipt = None;
                let kind = op_kind(g.active.is_some(), false);
                g.operation_failure = Some(OperationFailure {
                    operation: kind,
                    reason: ModelPackFailure::DownloadFailed { reason },
                });
                g.op = Op::Idle;
                true
            }
            _ => false,
        }
    }

    pub fn on_download_complete(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.operation_failure.is_some() {
            return false;
        }
        match g.op {
            Op::Queued | Op::Downloading { .. } => {
                g.verification_receipt = None;
                g.op = Op::Verifying;
                true
            }
            _ => false,
        }
    }

    /// Full verification. The receipt is cleared at the START of every
    /// attempt, set only on complete success, and bound to the exact
    /// verified inventory via the canonical VerificationMaterial digest.
    pub fn verify(&self, observed: Vec<ObservedArtifact>) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.op != Op::Verifying || g.operation_failure.is_some() {
            return false;
        }
        g.verification_receipt = None;
        let kind = op_kind(g.active.is_some(), false);
        let fail = |g: &mut PackInner, reason: ModelPackFailure| {
            g.operation_failure = Some(OperationFailure {
                operation: kind,
                reason,
            });
            g.op = Op::Idle;
            false
        };
        // Duplicate observed paths rejected before anything else.
        let mut seen = std::collections::HashSet::new();
        for o in &observed {
            if !seen.insert(o.relative_path.clone()) {
                return fail(
                    &mut g,
                    ModelPackFailure::DuplicateObservedPath {
                        relative_path: o.relative_path.clone(),
                    },
                );
            }
        }
        if !self.supported_abis.contains(&self.entry.runtime_abi) {
            return fail(&mut g, ModelPackFailure::RuntimeAbiIncompatible);
        }
        let schema_errs = validate_catalog_entry(self.entry.clone());
        if !schema_errs.is_empty() {
            return fail(
                &mut g,
                ModelPackFailure::SchemaInvalid {
                    reason: schema_errs.join("; "),
                },
            );
        }
        for a in &self.entry.artifacts {
            match observed.iter().find(|o| o.relative_path == a.relative_path) {
                None => {
                    return fail(
                        &mut g,
                        ModelPackFailure::MissingArtifact {
                            artifact_id: a.artifact_id.clone(),
                        },
                    )
                }
                Some(o) => {
                    if o.byte_size != a.byte_size {
                        return fail(
                            &mut g,
                            ModelPackFailure::SizeMismatch {
                                artifact_id: a.artifact_id.clone(),
                            },
                        );
                    }
                    if o.sha256_hex != a.sha256_hex {
                        return fail(
                            &mut g,
                            ModelPackFailure::DigestMismatch {
                                artifact_id: a.artifact_id.clone(),
                            },
                        );
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
                return fail(
                    &mut g,
                    ModelPackFailure::UndeclaredArtifact {
                        relative_path: o.relative_path.clone(),
                    },
                );
            }
        }
        g.verification_receipt = Some(inventory_digest(
            &self.entry,
            self.verification_policy_version,
        ));
        true // remains Verifying until explicit atomic publication
    }

    /// Requires the inventory-bound receipt; consumes it. The published
    /// installation becomes the active one (replacing any prior version).
    pub fn on_published(&self, installed_at_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.op != Op::Verifying || g.operation_failure.is_some() {
            return false;
        }
        let receipt = match g.verification_receipt.take() {
            None => return false, // verification was skipped or failed
            Some(r) => r,
        };
        g.active = Some(InstalledModelPack {
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
            catalog_entry_digest: catalog_entry_digest(self.entry.clone()),
            verified_inventory_digest: receipt,
        });
        g.restore_failure = None;
        g.op = Op::Idle;
        true
    }

    pub fn on_removal_started(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.op != Op::Idle || g.operation_failure.is_some() || g.active.is_none() {
            return false;
        }
        g.op = Op::Removing;
        true
    }

    /// NotInstalled only after the platform confirmed deletion.
    pub fn on_removal_confirmed(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.op != Op::Removing {
            return false;
        }
        g.active = None;
        g.op = Op::Idle;
        true
    }

    pub fn on_removal_failed(&self, reason: String) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.op != Op::Removing {
            return false;
        }
        g.operation_failure = Some(OperationFailure {
            operation: OperationKind::Removal,
            reason: ModelPackFailure::RemovalFailed { reason },
        });
        g.op = Op::Idle;
        true
    }
}
