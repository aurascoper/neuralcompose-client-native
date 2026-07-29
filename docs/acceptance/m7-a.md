# Acceptance record — M7-A: provider + model-pack contracts (2026-07-28)

Branch `m7-a/provider-model-pack-contracts` from `main @ 8132105`.
Provider-neutral, model-neutral, execution-free (ADR-001).

- ADR registry created; ADR-001 records the eight decisions (capabilities
  ship / weights don't; intent vs transport; model = configuration; no
  implicit substitution; requested vs resolved identities; generation vs
  embedding pack kinds; shell owns effects; Ready means verified).
- `src/provider.rs`: closed transport/locality taxonomy + configured IDs,
  tri-state CredentialState (no secrets in core), resolve_provider_identity
  (never reroutes), is_substitution with explicit alias equivalence,
  conservative-egress presentation.
- `src/model_pack.rs`: catalog entry / ModelPackPhase machine / installed
  record; structural validation (paths, digests, duplicates, zero-byte,
  kind discriminators); full verification (size+sha per artifact, no
  undeclared extras, runtime ABI); Ready only via explicit atomic
  publication; failed update preserves the prior Ready version; removal
  confirms platform deletion; embedding_space_identity pins revision +
  weight/tokenizer digests + dims + pooling + normalization + instruction.
- `contracts/model-packs/`: 3 JSON Schemas + 5 fixtures; golden tests
  validate + round-trip every fixture; invalid-cases table drives Rust
  rejection.
- All 15 required regressions covered with both-polarity assertions
  (mutation-guard style: digest-always-true, cloud-fallback, erased
  requested identity, ../ paths, mixed embedding identities each flip a
  test).

Evidence: Rust 83 tests both feature sets; clippy -D warnings + fmt clean;
fixture drift clean; Swift sim smoke 3/3; Kotlin JVM smoke 3/3 against
regenerated bindings.

Non-claims: model bytes downloaded: no · model executed: no · network
request made: no · Apple model API linked: no · Ollama/OpenAI/Anthropic SDK
linked: no · provider credential stored: no · default model selected: no ·
Qwen2.5 vs Qwen3 decided: no · embedding model selected: no.

## Review-response addendum (2026-07-28)

All five primary blockers + additional corrections resolved:
receipt-gated publication (canonical VerificationMaterial, domain-tagged,
cleared per attempt, consumed on publish; bypass/stale/duplicate-path
regressions incl. |/quote/Unicode paths); trusted-catalog restoration with
catalog_entry_digest + verified_inventory_digest on InstalledModelPack and
visible RestoreFailure (never silent); active/operation split with
ModelPackSnapshot.has_usable_active_installation driving provider
availability (failed v1.0 update keeps v0.9 usable; Failed→acknowledge→Idle
with OperationKind recorded); env!-pinned core version via semver (public
wrapper + testable seam; six version cases pinned); ≥1 weights + tokenizer
and tokenizer_id reference integrity; complete canonical embedding identity
(all shards, domain, injection-resistant); provider transport Option (no
fabrication), AvailabilityProbe (all five readiness states producible),
fail-closed duplicates/disagreement/NotRequired+Missing, provider-scoped
validated aliases; schema if/then kind pairing + schemaVersion const 1 with
dual-direction schemaInvalid assertions in the golden test.
Evidence: Rust 86 tests both feature sets; clippy/fmt/drift clean; Swift
sim 4/4 and Kotlin JVM 4/4 incl. publish-before-verify rejection, invalid-
restore surfacing, and unknown-provider transport nil/null.

## Round-2 review addendum (2026-07-29)

Five trust-boundary findings resolved:
1. Sealed restoration — constructor accepts only raw inputs (persisted
   record + fresh observed inventory + trusted catalog + accepted policy
   versions); RestoreResult is output-only with no injection point; on-disk
   bytes verified against the record (missing/modified/extra/duplicate/
   malformed each rejected with visible failures); pack_id must match the
   installer target (TargetPackMismatch). Tests:
   restore_rejects_missing_or_modified_on_disk_bytes,
   restore_resolves_against_trusted_catalog_only (extended), Swift/Kotlin
   m7aContractsThroughBindings (tampered-restore + polarity).
2. Removal usability — usable=false during Removing and after failed
   removal; acknowledge never reactivates; revalidate_active with exact
   bytes restores usability, mismatches surface specific
   ActiveIntegrityFailure variants in ModelPackSnapshot. Tests:
   removal_and_failed_removal_report_unusable_until_revalidated,
   Swift/Kotlin m7aRemovalIntegrityThroughBindings.
3. Ambiguous trusted catalog — duplicate (pack_id, pack_version) rejected
   before lookup in both vector orders. Test:
   ambiguous_trusted_catalog_rejected_in_both_orders.
4. Transport/locality matrix — is_valid_transport_locality pins the full
   ADR matrix both polarities; impossible pairs resolve transport=None,
   locality=Unresolved, caps=false, InconsistentConfiguration, egress=true.
   Test: impossible_transport_locality_pairs_fail_closed.
5. Policy-version registry — CURRENT_VERIFICATION_POLICY_VERSION=1 single
   authority; 0/2/u32::MAX rejected at verify (UnsupportedVerificationPolicy
   operation failure, no receipt, no publish) and at restore (both-set
   membership); 1 accepted end-to-end. Test:
   verification_policy_registry_rejects_zero_and_future_versions.

Evidence: 91 Rust tests green on default features, uniffi feature set
green, clippy -D warnings + fmt clean on both feature sets, fixture drift
clean, Swift 5/5 on iPhone 17 simulator, Kotlin 5/5 JVM through regenerated
bindings (dylib + XCFramework + Swift bindings rebuilt together).
