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
