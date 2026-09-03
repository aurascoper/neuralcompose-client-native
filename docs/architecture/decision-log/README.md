# Decision log (native client)

Architecture Decision Records for `neuralcompose-client-native`. Numbering is
local to this repository (the macOS NeuralCompose repo has its own registry).
Reconstructed or amended decisions get new, dated ADRs — records are never
retro-edited from later conversations.

| ADR | Title | Status |
|---|---|---|
| [ADR-001](ADR-001-provider-selection-and-model-pack-integrity.md) | Provider selection and model-pack integrity | Proposed (M7-A) |
| [ADR-002](ADR-002-runtime-targets-property-law-and-conformance.md) | Runtime targets, property law, and backend conformance | Proposed (M7-A2) |
| ADR-003 | Embedding candidate selection | Proposed · 2026-08-04 — **on an unmerged branch, see below** |
| [ADR-004](ADR-004-provenance-vocabulary.md) | Provenance vocabulary: how a value came to exist | Proposed |
| [ADR-005](ADR-005-eeg-capture-provenance.md) | Typed EEG capture: what was measured, what was interpreted, what was refused | Proposed |
| [ADR-006](ADR-006-opt-in-cloud-generation.md) | Opt-in cloud generation: one seam leaves the machine, and the record says so | Proposed |

**ADR-003 is taken, not vacant.** `ADR-003-embedding-candidate-selection.md`
exists at commit `471ae61` on `origin/docs/adr-003-embedding-candidate`, pushed
and not yet merged, which is why it is absent from a `main`-based checkout. It
names bge-small the embedding candidate and, at its §"promotion by relabel",
records the hazard that `attained_support_status()` checks
`candidate_model_executed` **before** any hardware-naming check — so naming
bge-small the candidate would make both `linux/x86_64` rows *eligible* for
`DeviceValidated` with nothing new run. It declines to take that rung and keeps
`candidate_model_executed: false`.

The provenance ADR is therefore **004**, and this row is here so the number is
not reused while its branch is in flight. Note that `471ae61` also adds its own
ADR-002 and ADR-003 rows to this table; when it lands, this file conflicts and
the resolution is to keep one row each, not to renumber anything.
