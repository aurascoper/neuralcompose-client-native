# ADR-002 — Runtime targets, property law, and backend conformance

Status: Proposed (M7-A2) · 2026-07-29
Builds on: ADR-001 (provider selection and model-pack integrity)
Scope: contracts, schemas, tests, CI skeleton, and documentation only. This
ADR executes no model, links no accelerator library, ships no runtime pack,
and promotes no target.

## Decisions

1. **An OS target and a hardware backend are separate dimensions.** `os` +
   `architecture` say where the app runs; `backend_id` + `accelerator_class`
   say what executes the math. Neither implies the other, and neither is a
   vendor enum — `backend_id` is configuration, so a new backend never
   requires a core release.

2. **A logical model identity and a backend artifact variant are separate.**
   One `logical_model_id` is published as many `ModelVariant`s. The provider
   does not change when the backend does: `local-qwen` on Vulkan and
   `local-qwen` on CUDA are the same provider, different variants. (ADR-001
   decision 3, extended to backends.)

3. **Runtime packs carry their own signed manifests.** A `RuntimePackManifest`
   declares its target, libraries (relative paths + digests), licences,
   entrypoints, ABI, and signing identity. Packs are independently
   installable and removable and are never a base-application dependency.
   An absent `signing_identity` means unsigned — never treated as trusted.

4. **No unsupported backend fallback.** `select_runtime_variant` distinguishes
   `BackendRequirement::Explicit` from `AnySupported`. An explicit request for
   a backend that is unpublished, uninstalled, ABI-incompatible, or missing a
   required capability resolves to `Unavailable` — never to a different
   backend. Only `AnySupported` may choose among eligible variants, and it
   does so in a declaration-order-independent way. When several candidates
   fail differently, the furthest-progressed failure is reported, so an
   incidental uninstalled backend never masks the real constraint.

5. **Backend equality is semantic conformance, not mathematical
   equivalence.** Identical results across CUDA, Vulkan, OpenVINO, QNN,
   Metal, and CPU are not claimed and not required. What is required:
   identical tokenizer identity, exact prompt bytes, identical stop-token
   configuration and context cap, a declared output shape, deterministic
   greedy behaviour where the policy demands it, and numerical divergence
   within a **pre-registered** tolerance. Sampled output is evaluated as a
   distribution, never by string equality.

6. **Exceeding tolerance is a different contract, not a failed test.**
   `RequiresSeparateNumericalContract` is a first-class verdict: the backend
   publishes under its own `numerical_contract_id` rather than being
   declared equal or quietly dropped. An **unmeasured** tolerance is never a
   satisfied tolerance — a missing or non-finite measurement is
   non-conformance.

7. **Idempotence, equivariance, and invariance are executable properties,
   not prose.** They are pinned by tests with both polarities, so a mutation
   that neuters a comparison fails at least one test:
   - *Idempotent*: provider resolution; re-verifying an identical inventory;
     publishing twice (no duplicate installation); removing an absent pack;
     indexing the same content hash under the same embedding identity.
   - *Time-origin equivariant*: adding a constant Δ to every monotonic
     timestamp leaves the phase sequence, elapsed durations, staleness, and
     retry decisions unchanged — while relative spacing still matters.
   - *Window-shift equivariant*: shifting a span by N moves the range by N
     and changes neither the event kind nor the detector parameters; an
     out-of-range shift refuses rather than wrapping.
   - *Channel-permutation equivariant*: values permute **only** when labels
     travel with them. Bare values in a new order are rejected —
     TP9/AF7/AF8/TP10 are anatomy, not slots.
   - *Invariant*: artifact declaration order and filesystem root never change
     a manifest digest or a content identity.
   - *Not invariant*: model family or revision, any weight shard, tokenizer,
     quantization, dimensions, pooling, normalization, task instruction,
     channel identity, and the numerical contract each **must** change the
     corresponding identity.

8. **Target promotion follows the support-level vocabulary, never
   implication.** `Contracted → BuildValidated → RuntimeSmokeValidated →
   DeviceValidated → ReleaseSupported`. Each rung requires every rung below
   it plus its own evidence: compiling is not running, a fixture run is not
   device validation, and `DeviceValidated` requires *named* hardware, OS,
   and backend versions. `docs/support-matrix.md` is the register, and no row
   is promoted by preference.

## Review amendments (PR #3, 2026-07-29)

**A1 — The workload is a required capability.** `RequiredCapabilities` gains
`generation` and `embeddings`. Without them an embedding-only backend could
be selected to generate text: the target declared both capability flags but
selection only consulted streaming, cancellation, and structured output.

**A2 — Two variants of one backend must be named.** `AnySupported` and an
explicit *backend* requirement both refuse to choose between two eligible
variants of the same backend (`AmbiguousVariantsForBackend`): q4 vs q8, or
two numerical contracts, are the caller's decision and were previously
settled by a lexicographic sort on `variant_id`. The new
`BackendRequirement::ExplicitVariant` is the only way to disambiguate. The
CPU-first preference survives only *across* backends, where one variant per
backend makes it a documented preference rather than a hidden tie-break.

**A3 — The numerical-contract identity is sealed, not asserted.**
`numerical_contract_identity` now hashes the substantive terms and
**excludes** the declared `numerical_contract_id`; hashing the caller's own
label into its digest made the identity self-referential. A policy is valid
only when its declared id **equals** the seal its terms derive, so a
tolerance cannot be loosened while keeping the name. `ModelVariant`
`numerical_contract_id` must be a 64-hex seal rather than a free label, and
`variant_binds_to_contract` is the only legitimate way for a variant to
claim conformance to a policy.

**A4 — Measurement scope follows the contract kind.** `logits_tolerance` and
`embedding_tolerance` are `Option`; at least one must be declared. A
declared tolerance must be measured (missing or non-finite → non-conformant),
an **undeclared** one must not be measured
(`MeasurementOutOfScope`, visible rather than silently dropped), and
`ExactUnderGreedy` requires a logits scope to be meaningful. Previously every
conformance run demanded both, which contradicted the generation-only vs
embedding-only separation this ADR establishes.

## Consequences

`crates/neuralcompose-mobile-core/src/runtime_target.rs`,
`src/conformance.rs`, and `src/property_law.rs` implement the deterministic
halves; `contracts/runtime/` freezes the JSON schemas and fixtures; the
M7-A2 regressions pin every rule above with both-polarity assertions, and
the golden test asserts each invalid case's schema verdict in **both**
directions so the marker table cannot drift.

## Non-claims

This ADR does not claim that any backend works, that any target is
supported, or that any two backends agree numerically. No model executed, no
accelerator library was linked, and no support-matrix row advanced beyond
`Contracted`.
