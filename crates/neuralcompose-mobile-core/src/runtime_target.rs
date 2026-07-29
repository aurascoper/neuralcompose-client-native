//! Runtime targets, runtime packs, and model variants (M7-A2, ADR-002).
//! Deterministic and effect-free: this module loads no library, executes no
//! model, and links no accelerator.
//!
//! Two dimensions stay separate. An **OS target** (os + architecture) is not
//! a **hardware backend** (`backend_id` + accelerator class), and a **logical
//! model identity** is not a **backend artifact variant**. `local-qwen` is
//! the same provider whether it runs through CPU, Vulkan, CUDA, or QNN — the
//! variant changes, the provider does not.
//!
//! Backend identifiers are configuration strings, never a closed vendor enum:
//! a new backend must not require a core release.

use serde::{Deserialize, Serialize};

pub const SUPPORTED_RUNTIME_SCHEMA_VERSION: u32 = 1;
const RUNTIME_TARGET_DOMAIN: &str = "neuralcompose.runtime-target.v1";
const RUNTIME_PACK_DOMAIN: &str = "neuralcompose.runtime-pack-manifest.v1";
const MODEL_VARIANT_DOMAIN: &str = "neuralcompose.model-variant.v1";

/// What kind of hardware executes the work. A *class*, not a vendor —
/// vendors live in `backend_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum AcceleratorClass {
    Cpu,
    Gpu,
    Npu,
}

/// Backend-level capabilities. Distinct from `ProviderCapabilities`: a
/// provider may offer cancellation semantically while a specific backend
/// cannot honour it, and callers that *require* a capability must fail
/// closed rather than proceed without it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RuntimeCapabilities {
    pub generation: bool,
    pub embeddings: bool,
    pub streaming: bool,
    pub cancellation: bool,
    pub structured_output: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RuntimeTarget {
    pub os: String,
    pub architecture: String,
    pub accelerator_class: AcceleratorClass,
    /// Configured backend identity, e.g. `llama-cpp-vulkan`, `coreml`,
    /// `windows-ml-qnn`. Never a vendor enum.
    pub backend_id: String,
    pub runtime_abi: String,
    pub model_formats: Vec<String>,
    pub minimum_os_version: Option<String>,
    pub minimum_backend_version: Option<String>,
    pub minimum_driver_version: Option<String>,
    pub capabilities: RuntimeCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RuntimeLibrary {
    pub library_id: String,
    /// Relative to the installed runtime-pack root — never an absolute path,
    /// so identity is invariant under relocation.
    pub relative_path: String,
    pub byte_size: u64,
    pub sha256_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RuntimeEntrypoint {
    pub entrypoint_id: String,
    pub library_id: String,
    pub symbol: String,
}

/// A signed, independently removable bundle of native libraries for exactly
/// one target. Never a base-application dependency.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RuntimePackManifest {
    pub schema_version: u32,
    pub runtime_pack_id: String,
    pub version: String,
    pub target: RuntimeTarget,
    pub libraries: Vec<RuntimeLibrary>,
    pub licenses: Vec<String>,
    pub entrypoints: Vec<RuntimeEntrypoint>,
    pub runtime_abi: String,
    /// Who signed the pack. Absent means unsigned — never treated as trusted.
    pub signing_identity: Option<String>,
}

/// One logical model, published once per backend artifact variant. Distinct
/// variants of the same `logical_model_id` are the *same model* to the user
/// and *different artifacts* to the runtime.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ModelVariant {
    pub schema_version: u32,
    pub logical_model_id: String,
    pub variant_id: String,
    pub model_pack_id: String,
    pub runtime_target: RuntimeTarget,
    pub quantization: Option<String>,
    pub artifact_format: String,
    /// Identity of the numerical contract this variant satisfies. Two
    /// variants sharing it claim results interchangeable within tolerance;
    /// differing ids make no such claim.
    pub numerical_contract_id: String,
}

// ---------- support-level vocabulary ----------

/// Promotion ladder. Never promote by implication: each rung needs its own
/// evidence, and compiling is not running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SupportStatus {
    Contracted,
    BuildValidated,
    RuntimeSmokeValidated,
    DeviceValidated,
    ReleaseSupported,
}

/// Evidence actually in hand for one (target, backend) row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SupportEvidence {
    pub contracts_and_tests_pass: bool,
    pub builds_on_named_target: bool,
    /// A deterministic fixture model actually executed — not "it compiled".
    pub fixture_runtime_executed: bool,
    /// Exact physical hardware. `None` blocks DeviceValidated outright.
    pub physical_device: Option<String>,
    pub os_version: Option<String>,
    pub backend_version: Option<String>,
    pub signed_packaging_accepted: bool,
    pub acceptance_document: Option<String>,
}

fn named(v: &Option<String>) -> bool {
    v.as_ref().is_some_and(|s| !s.trim().is_empty())
}

/// The highest rung this evidence legitimately reaches. Each rung requires
/// every rung below it — a device claim cannot skip a runtime claim.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn attained_support_status(evidence: SupportEvidence) -> Option<SupportStatus> {
    if !evidence.contracts_and_tests_pass {
        return None;
    }
    if !evidence.builds_on_named_target {
        return Some(SupportStatus::Contracted);
    }
    if !evidence.fixture_runtime_executed {
        return Some(SupportStatus::BuildValidated);
    }
    let device_proven = named(&evidence.physical_device)
        && named(&evidence.os_version)
        && named(&evidence.backend_version);
    if !device_proven {
        return Some(SupportStatus::RuntimeSmokeValidated);
    }
    if !(evidence.signed_packaging_accepted && named(&evidence.acceptance_document)) {
        return Some(SupportStatus::DeviceValidated);
    }
    Some(SupportStatus::ReleaseSupported)
}

/// Is a claimed status supported by this evidence? Claiming above the
/// attained rung is always false.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn supports_claim(evidence: SupportEvidence, claimed: SupportStatus) -> bool {
    attained_support_status(evidence).is_some_and(|a| a >= claimed)
}

// ---------- canonical identities ----------

fn sorted_libraries(m: &RuntimePackManifest) -> Vec<RuntimeLibrary> {
    let mut l = m.libraries.clone();
    l.sort_by(|a, b| (&a.library_id, &a.relative_path).cmp(&(&b.library_id, &b.relative_path)));
    l
}

fn sorted_entrypoints(m: &RuntimePackManifest) -> Vec<RuntimeEntrypoint> {
    let mut e = m.entrypoints.clone();
    e.sort_by(|a, b| {
        (&a.entrypoint_id, &a.library_id, &a.symbol).cmp(&(
            &b.entrypoint_id,
            &b.library_id,
            &b.symbol,
        ))
    });
    e
}

/// Canonical target identity. Declaration order never matters; every
/// declared field does.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn runtime_target_identity(target: RuntimeTarget) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        target: RuntimeTarget,
    }
    let mut canonical = target;
    canonical.model_formats.sort();
    let doc = Doc {
        domain: RUNTIME_TARGET_DOMAIN,
        target: canonical,
    };
    crate::audio::sha256_hex(serde_json::to_vec(&doc).expect("serialize"))
}

/// Canonical runtime-pack digest: libraries and entrypoints sorted, paths
/// relative, so neither declaration order nor install root can change it.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn runtime_pack_manifest_digest(manifest: RuntimePackManifest) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        manifest: RuntimePackManifest,
    }
    let mut canonical = manifest;
    canonical.libraries = sorted_libraries(&canonical);
    canonical.entrypoints = sorted_entrypoints(&canonical);
    canonical.licenses.sort();
    let doc = Doc {
        domain: RUNTIME_PACK_DOMAIN,
        manifest: canonical,
    };
    crate::audio::sha256_hex(serde_json::to_vec(&doc).expect("serialize"))
}

/// Canonical variant identity. Changing the target, quantization, format, or
/// numerical contract changes it; reordering declared lists does not.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn model_variant_identity(variant: ModelVariant) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        variant: ModelVariant,
    }
    let mut canonical = variant;
    canonical.runtime_target.model_formats.sort();
    let doc = Doc {
        domain: MODEL_VARIANT_DOMAIN,
        variant: canonical,
    };
    crate::audio::sha256_hex(serde_json::to_vec(&doc).expect("serialize"))
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

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_runtime_target(target: RuntimeTarget) -> Vec<String> {
    let mut errs = Vec::new();
    for (name, value) in [
        ("os", &target.os),
        ("architecture", &target.architecture),
        ("backendId", &target.backend_id),
        ("runtimeAbi", &target.runtime_abi),
    ] {
        if value.trim().is_empty() {
            errs.push(format!("{name} must not be empty"));
        }
    }
    if target.model_formats.is_empty() {
        errs.push("no model formats declared".into());
    }
    let mut seen = std::collections::HashSet::new();
    for f in &target.model_formats {
        if f.trim().is_empty() {
            errs.push("empty model format".into());
        }
        if !seen.insert(f) {
            errs.push(format!("duplicate model format: {f}"));
        }
    }
    for (name, v) in [
        ("minimumOsVersion", &target.minimum_os_version),
        ("minimumBackendVersion", &target.minimum_backend_version),
        ("minimumDriverVersion", &target.minimum_driver_version),
    ] {
        if let Some(s) = v {
            if s.trim().is_empty() {
                errs.push(format!("{name} present but empty"));
            }
        }
    }
    errs
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_runtime_pack_manifest(manifest: RuntimePackManifest) -> Vec<String> {
    let mut errs = validate_runtime_target(manifest.target.clone());
    if manifest.schema_version != SUPPORTED_RUNTIME_SCHEMA_VERSION {
        errs.push(format!(
            "unsupported schemaVersion {}",
            manifest.schema_version
        ));
    }
    if manifest.runtime_pack_id.trim().is_empty() {
        errs.push("runtimePackId must not be empty".into());
    }
    if semver::Version::parse(&manifest.version).is_err() {
        errs.push(format!("malformed version {}", manifest.version));
    }
    // The pack's ABI and its target's ABI are one claim, not two.
    if manifest.runtime_abi != manifest.target.runtime_abi {
        errs.push("runtimeAbi disagrees with target.runtimeAbi".into());
    }
    if manifest.libraries.is_empty() {
        errs.push("no libraries declared".into());
    }
    if manifest.licenses.is_empty() {
        errs.push("no licenses declared".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut paths = std::collections::HashSet::new();
    for l in &manifest.libraries {
        if !ids.insert(l.library_id.clone()) {
            errs.push(format!("duplicate libraryId: {}", l.library_id));
        }
        if !paths.insert(l.relative_path.clone()) {
            errs.push(format!("duplicate library path: {}", l.relative_path));
        }
        if !valid_relative_path(&l.relative_path) {
            errs.push(format!("invalid relative path: {}", l.relative_path));
        }
        if !valid_sha256(&l.sha256_hex) {
            errs.push(format!("invalid sha256 for {}", l.library_id));
        }
        if l.byte_size == 0 {
            errs.push(format!("zero-byte library: {}", l.library_id));
        }
    }
    let mut entry_ids = std::collections::HashSet::new();
    for e in &manifest.entrypoints {
        if !entry_ids.insert(e.entrypoint_id.clone()) {
            errs.push(format!("duplicate entrypointId: {}", e.entrypoint_id));
        }
        if e.symbol.trim().is_empty() {
            errs.push(format!("empty symbol for {}", e.entrypoint_id));
        }
        if !manifest
            .libraries
            .iter()
            .any(|l| l.library_id == e.library_id)
        {
            errs.push(format!(
                "entrypoint '{}' references undeclared library '{}'",
                e.entrypoint_id, e.library_id
            ));
        }
    }
    if let Some(s) = &manifest.signing_identity {
        if s.trim().is_empty() {
            errs.push("signingIdentity present but empty".into());
        }
    }
    errs
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn validate_model_variant(variant: ModelVariant) -> Vec<String> {
    let mut errs = validate_runtime_target(variant.runtime_target.clone());
    if variant.schema_version != SUPPORTED_RUNTIME_SCHEMA_VERSION {
        errs.push(format!(
            "unsupported schemaVersion {}",
            variant.schema_version
        ));
    }
    for (name, value) in [
        ("logicalModelId", &variant.logical_model_id),
        ("variantId", &variant.variant_id),
        ("modelPackId", &variant.model_pack_id),
        ("artifactFormat", &variant.artifact_format),
        ("numericalContractId", &variant.numerical_contract_id),
    ] {
        if value.trim().is_empty() {
            errs.push(format!("{name} must not be empty"));
        }
    }
    // A variant whose artifact format its own target cannot load is a
    // packaging error, not a runtime surprise.
    if !variant.artifact_format.trim().is_empty()
        && !variant
            .runtime_target
            .model_formats
            .contains(&variant.artifact_format)
    {
        errs.push(format!(
            "artifactFormat '{}' not among target model formats",
            variant.artifact_format
        ));
    }
    if let Some(q) = &variant.quantization {
        if q.trim().is_empty() {
            errs.push("quantization present but empty".into());
        }
    }
    errs
}

// ---------- selection: no unsupported backend fallback ----------

/// What the caller asked for. An explicit backend requirement is a hard
/// constraint: a user who chose CUDA is never silently given CPU.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BackendRequirement {
    /// Any variant the device can actually run is acceptable.
    AnySupported,
    /// Only this backend. Absent or unusable → Unavailable, never a swap.
    Explicit { backend_id: String },
}

/// What the device can actually provide right now, as reported by the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DeviceRuntimeProfile {
    pub os: String,
    pub architecture: String,
    /// Runtime packs the shell has verified as installed and usable.
    pub installed_backend_ids: Vec<String>,
    pub supported_runtime_abis: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SelectionFailure {
    NoVariantForOsArchitecture,
    RequestedBackendNotPublished { backend_id: String },
    RequestedBackendNotInstalled { backend_id: String },
    RuntimeAbiUnsupported { backend_id: String },
    RequiredCapabilityUnavailable { capability: String },
    AmbiguousVariants { variant_id: String },
    InvalidVariant { reason: String },
}

// The size asymmetry is inherent to the FFI contract shape: a selection
// either carries the whole variant or a small failure. Boxing would change
// the generated bindings for no runtime benefit at this call rate.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RuntimeSelection {
    Selected { variant: ModelVariant },
    Unavailable { failure: SelectionFailure },
}

/// Capabilities the caller *requires*. Anything true here that the backend
/// cannot do makes the selection fail closed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RequiredCapabilities {
    pub streaming: bool,
    pub cancellation: bool,
    pub structured_output: bool,
}

/// Resolve one logical model to one runnable variant, or explain why not.
/// Never substitutes a backend the caller explicitly required, and never
/// invents support the device did not report.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn select_runtime_variant(
    logical_model_id: String,
    variants: Vec<ModelVariant>,
    device: DeviceRuntimeProfile,
    requirement: BackendRequirement,
    required: RequiredCapabilities,
) -> RuntimeSelection {
    let unavailable = |failure| RuntimeSelection::Unavailable { failure };

    let for_model: Vec<&ModelVariant> = variants
        .iter()
        .filter(|v| v.logical_model_id == logical_model_id)
        .collect();

    // Ambiguity is a configuration error: identical variant ids cannot be
    // resolved by order.
    let mut seen = std::collections::HashSet::new();
    for v in &for_model {
        if !seen.insert(&v.variant_id) {
            return unavailable(SelectionFailure::AmbiguousVariants {
                variant_id: v.variant_id.clone(),
            });
        }
    }

    let on_device: Vec<&&ModelVariant> = for_model
        .iter()
        .filter(|v| {
            v.runtime_target.os == device.os && v.runtime_target.architecture == device.architecture
        })
        .collect();
    if on_device.is_empty() {
        return unavailable(SelectionFailure::NoVariantForOsArchitecture);
    }

    let candidates: Vec<&&ModelVariant> = match &requirement {
        BackendRequirement::AnySupported => on_device,
        BackendRequirement::Explicit { backend_id } => {
            let matching: Vec<&&ModelVariant> = on_device
                .into_iter()
                .filter(|v| &v.runtime_target.backend_id == backend_id)
                .collect();
            if matching.is_empty() {
                // Explicitly required backend has no published variant —
                // that is the answer, not a reason to pick another backend.
                return unavailable(SelectionFailure::RequestedBackendNotPublished {
                    backend_id: backend_id.clone(),
                });
            }
            matching
        }
    };

    let explicit = matches!(requirement, BackendRequirement::Explicit { .. });
    // When several candidates fail for different reasons, report the one that
    // got furthest through the checks: "this backend cannot cancel" is
    // actionable, "some other backend is not installed" is noise.
    fn progress(f: &SelectionFailure) -> u8 {
        match f {
            SelectionFailure::NoVariantForOsArchitecture => 0,
            SelectionFailure::InvalidVariant { .. } => 1,
            SelectionFailure::RequestedBackendNotPublished { .. } => 2,
            SelectionFailure::RequestedBackendNotInstalled { .. } => 3,
            SelectionFailure::RuntimeAbiUnsupported { .. } => 4,
            SelectionFailure::RequiredCapabilityUnavailable { .. } => 5,
            SelectionFailure::AmbiguousVariants { .. } => 6,
        }
    }
    let mut best_failure = SelectionFailure::NoVariantForOsArchitecture;
    let record = |f: SelectionFailure, best: &mut SelectionFailure| {
        if progress(&f) > progress(best) {
            *best = f;
        }
    };
    // Deterministic order: CPU first among equals, then by variant id, so
    // AnySupported never depends on declaration order.
    let mut ordered: Vec<&&ModelVariant> = candidates;
    ordered.sort_by(|a, b| {
        (a.runtime_target.accelerator_class, &a.variant_id)
            .cmp(&(b.runtime_target.accelerator_class, &b.variant_id))
    });

    for v in ordered {
        if !validate_model_variant((**v).clone()).is_empty() {
            record(
                SelectionFailure::InvalidVariant {
                    reason: format!("variant '{}' fails validation", v.variant_id),
                },
                &mut best_failure,
            );
            continue;
        }
        let backend_id = v.runtime_target.backend_id.clone();
        if !device.installed_backend_ids.contains(&backend_id) {
            record(
                SelectionFailure::RequestedBackendNotInstalled { backend_id },
                &mut best_failure,
            );
            continue;
        }
        if !device
            .supported_runtime_abis
            .contains(&v.runtime_target.runtime_abi)
        {
            record(
                SelectionFailure::RuntimeAbiUnsupported { backend_id },
                &mut best_failure,
            );
            continue;
        }
        let caps = &v.runtime_target.capabilities;
        let missing = [
            (required.streaming && !caps.streaming, "streaming"),
            (required.cancellation && !caps.cancellation, "cancellation"),
            (
                required.structured_output && !caps.structured_output,
                "structuredOutput",
            ),
        ]
        .into_iter()
        .find_map(|(missing, name)| missing.then_some(name));
        if let Some(capability) = missing {
            let failure = SelectionFailure::RequiredCapabilityUnavailable {
                capability: capability.to_string(),
            };
            // A required capability the backend lacks is fatal for an
            // explicit request; for AnySupported, keep looking.
            if explicit {
                return unavailable(failure);
            }
            record(failure, &mut best_failure);
            continue;
        }
        return RuntimeSelection::Selected {
            variant: (**v).clone(),
        };
    }
    unavailable(best_failure)
}
