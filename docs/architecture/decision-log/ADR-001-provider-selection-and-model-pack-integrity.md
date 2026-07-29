# ADR-001 — Provider selection and model-pack integrity

Status: Proposed (M7-A) · 2026-07-28
Scope: provider-neutral, model-neutral, execution-free. This ADR establishes
authority for later lanes; it downloads nothing, executes nothing, links no
provider SDK, and selects no default model.

## Decisions

1. **Capabilities ship with the app; weights do not.** The app ships the
   inference runtime, provider interfaces, prompt profiles, pack schemas,
   download/verification logic, and small deterministic fixtures. Model
   weights install only after an explicit user action that shows model
   name/version, download and installed size, device compatibility, locality
   /privacy status, license, expected battery/thermal impact, a delete
   control, and a Wi-Fi-only default.

2. **Semantic intent is separate from transport.** Prompt profiles and
   embedding task instructions are application resources; providers transmit
   them without provider-specific rewriting (ADR-009 lineage from the macOS
   repo).

3. **A model is configuration behind a transport.** Provider code represents
   transport classes; model identity lives in configuration. Qwen2.5 vs
   Qwen3 is not a new provider type; an Anthropic model change is not a new
   transport.

4. **No implicit provider substitution.** A missing or failed local pack
   never becomes a cloud, system-model, or endpoint request. It produces
   "Local model unavailable — choose another installed model or explicitly
   select a cloud provider."

5. **Requested and resolved identities remain separate.** Substitution is
   visible and normally fail-closed. A requested/resolved model mismatch is
   substitution unless an explicitly registered canonical alias proves
   equivalence.

6. **Generation and embedding are distinct pack kinds.** A generation
   model's hidden states never become the production embedding contract by
   accident. Embedding-space identity pins model revision, weight digest,
   tokenizer digest, dimension, pooling, normalization, and task
   instruction; vectors from differing identities are never mixed.

7. **The shell owns effects.** HTTP, background downloads, files, Keychain/
   Keystore, inference handles, and OS model APIs stay platform-native.
   Rust owns deterministic selection, installation state, verification,
   readiness, and identity semantics. Rust never receives credential
   material — only `NotRequired | Missing | Available`.

8. **Ready means verified.** A pack is Ready only after every required
   artifact matches expected byte size and SHA-256, the schema and runtime
   ABI validate, and the completed directory is atomically promoted. An
   update failure preserves the previously Ready version. Removal reports
   NotInstalled only after platform deletion is confirmed.

## Provider classification

A closed transport taxonomy plus stable configured IDs (no vendor enum — a
new provider must not require a core release):

- `ProviderTransport`: OnDeviceModelPack · SystemModel · HttpEndpoint ·
  BrokeredCloud
- `ProviderLocality`: OnDevice · LocalNetwork · RemoteEndpoint · Cloud ·
  Unresolved (unknown locality presents conservatively as possible egress)

Configured examples (not code): `local-qwen`/OnDeviceModelPack,
`apple-system`/SystemModel, `ollama-lan`/HttpEndpoint+LocalNetwork,
`ollama-cloud`/HttpEndpoint+Cloud (a distinct privacy identity from
ollama-lan), `neuralcompose-openai`/BrokeredCloud. Public-app cloud
credentials live behind the authenticated NeuralCompose broker; BYOK is an
advanced-only mode in platform secure storage; no app-owner key ever ships
in the binary.

## Consequences

`crates/neuralcompose-mobile-core/src/provider.rs` and `src/model_pack.rs`
implement the deterministic halves; `contracts/model-packs/` freezes the
JSON schemas and fixtures; the fifteen M7-A regressions pin the rules above,
with both-polarity assertions so the load-bearing comparisons cannot be
neutered without a test failing.

## Catalog trust root (review amendment)

SHA-256 values are meaningful only under a trusted catalog. v1 catalogs are
**app-bundled** and trusted via the platform code signature. Remote catalogs
are deferred and REQUIRE detached signing with pinned keys plus monotonic
rollback protection before any use. The bundled trust root retains entries
for all SUPPORTED INSTALLED versions (not merely the newest offered), so a
previously installed pack always restores against its own trusted entry —
never against an update target. Installed-record restoration is exact:
pack_id + pack_version + catalog_entry_digest, full inventory equality, ABI
and verification-policy acceptance, and inventory-digest recomputation;
rejected restores are preserved/quarantined with a visible RestoreFailure.

## Round-2 amendments (review 2026-07-29)

**Sealed restoration.** `RestoreResult` is output-only: no constructor or
state-changing API accepts one (a shell-forged `.restored` has no injection
point). `ModelPackInstaller::new` takes raw inputs — persisted record, fresh
on-disk observations, trusted catalog, supported ABIs, accepted policy
versions — and runs restoration internally against its own target pack ID.
Restoration verifies the actual bytes, not just persisted metadata: the
fresh observed inventory must exactly equal the record (well-formed, no
missing/modified/extra/duplicate artifacts), so deleted or tampered weights
never restore as usable. A record whose pack_id differs from the installer's
target is rejected (`TargetPackMismatch`). Observations supplied without a
record are contradictory input and surface visibly. Duplicate trusted
`(pack_id, pack_version)` identities reject as `AmbiguousTrustedCatalog`
before lookup, independent of catalog ordering.

**Usability is an integrity axis.** `has_usable_active_installation` is not
`active.is_some()`: a pack is unusable while removal is in progress and
after a failed removal — acknowledging the failure clears the operation
state but never reactivates the pack. Only a fresh exact integrity
revalidation (`revalidate_active`) restores usability; a failed
revalidation surfaces a specific `ActiveIntegrityFailure` in the snapshot
(distinct evidence class from `RestoreFailure`).

**Transport/locality matrix.** A descriptor's locality is its privacy
claim and must be possible for its transport:

```text
OnDeviceModelPack → OnDevice
SystemModel       → OnDevice
HttpEndpoint      → LocalNetwork | RemoteEndpoint | Cloud
BrokeredCloud     → Cloud
```

Any other pair (including descriptor-declared Unresolved) fails closed:
transport absent, locality Unresolved, capabilities false, readiness
`Unavailable(InconsistentConfiguration)`, presented as possible egress — an
invalid descriptor can never claim a cloud runtime is local.

**Verification-policy registry.** `CURRENT_VERIFICATION_POLICY_VERSION = 1`
and `supported_verification_policy_versions()` are the single Rust
authority. Unsupported versions (0, future, u32::MAX) cannot verify —
therefore cannot mint a receipt or publish — and cannot restore; restore
requires membership in both the Rust-supported and caller-accepted sets.
The JSON Schema's `>= 1` bound is documentation, not the enforcement.

Provider taxonomy note: Ollama local and Ollama Cloud remain separate
providers (unauthenticated localhost API vs authenticated remote API —
distinct privacy identities), and app-owner cloud keys stay server-side
behind the broker.
