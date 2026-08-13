# Decision log (native client)

Architecture Decision Records for `neuralcompose-client-native`. Numbering is
local to this repository (the macOS NeuralCompose repo has its own registry).
Reconstructed or amended decisions get new, dated ADRs — records are never
retro-edited from later conversations.

| ADR | Title | Status |
|---|---|---|
| [ADR-001](ADR-001-provider-selection-and-model-pack-integrity.md) | Provider selection and model-pack integrity | Proposed (M7-A) |
| [ADR-002](ADR-002-runtime-targets-property-law-and-conformance.md) | Runtime targets, property law, and backend conformance | Proposed (M7-A2) |
| ADR-003 | *(reserved — see below)* | Unwritten |
| [ADR-004](ADR-004-provenance-vocabulary.md) | Provenance vocabulary: how a value came to exist | Proposed |

**ADR-003 is a reserved gap, not a numbering mistake.** A stored claim
(`neural-memory` record `dc1377b4…`, 2026-08-04, `agentInference`) asserts that
"ADR-003 records the eligibility rather than taking it" about the
promotion-by-relabel hazard in `attained_support_status()` — that because
`candidate_model_executed` is checked before any hardware-naming check and
bge-small has executed on both `linux/x86_64` rows, those rows are *eligible*
for `DeviceValidated` on evidence nobody re-examined. No such ADR was ever
written here. The number is held so that writing it later satisfies the record,
rather than being silently consumed by an unrelated decision.
