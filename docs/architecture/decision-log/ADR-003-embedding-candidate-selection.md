# ADR-003 — Embedding candidate selection

Status: Proposed · 2026-08-04
(No milestone tag: `M7-C` is already earmarked for the `ios`/`apple-system-model`
row in `docs/support-matrix.md`, and this decision is not that work.)
Builds on: ADR-001 (provider selection and model-pack integrity), ADR-002
(runtime targets, property law, and backend conformance)
Scope: names the embedding candidate and records why the choice was a decision
rather than a lookup. This ADR executes no model, measures no retrieval quality,
changes no schema, and **promotes no row**.

## The decision

**`bge-small-en-v1.5` is the embedding candidate.**

`docs/acceptance/m7-a.md` has carried `embedding model selected: no` since M7-A.
That claim is superseded here. The M7-A line itself stays as written — it
correctly records what was claimed on 2026-07-28 — and gains a dated pointer to
this ADR.

## Why this was blocked, and why it is not a lookup

`DeviceValidated` on the two `linux/x86_64` GGUF rows has one outstanding
requirement: `candidate_model_executed`. That is not work waiting for time. It
is a decision waiting for a person, and it sat behind other work while looking
like a formality.

It is not a formality, because the three plausible candidates do not differ only
in weights. Two are drop-in; one is an API change.

| Candidate | Dims | Symmetry | Params | Cost to adopt |
| --- | --- | --- | --- | --- |
| `bge-small-en-v1.5` | 384 | symmetric | 33.2 M | none — already pinned throughout this repo |
| `all-MiniLM-L6-v2` | 384 | symmetric | ~22 M | download + GGUF conversion, per the M7-B template |
| `E5-base-v2` | 768 | **asymmetric** | ~110 M | schema change + re-embed + unmeasured divergence |

### The E5 row is the reason this needed deciding

E5 requires its inputs to be prefixed — `"query: "` for queries, `"passage: "`
for passages. That is not a configuration value. It is an asymmetric encoding
contract, and it does not fit the seam:

- `EmbeddingContract.task_instruction`
  (`crates/neuralcompose-mobile-core/src/model_pack.rs:96`) is a single
  `Option<String>`. There is one slot; E5 needs two.
- That field is hashed into `embedding_space_identity()`
  (`model_pack.rs:328`), so the shape cannot be widened quietly — changing it
  changes every embedding-space identity.
- `Embedder::embed(&mut self, text: &str)`
  (`crates/neuralcompose-llama/src/lib.rs:521`) has no way to say whether *this*
  text is a query or a passage.

Storing one prefix and using it for both is the failure mode that matters:
retrieval degrades and **nothing errors**. Adopting E5 honestly means a contract
change, a `schema_version` bump, and breaking UniFFI checksum `56753`.

E5 is genuinely better at retrieval, and the prefix asymmetry is *why* — query
and passage embeddings occupy deliberately different regions. The honest summary
is that E5 is better at the task and more expensive in every dimension that
constrains a handheld already sharing bandwidth between BLE ingest,
classification and generation. It was not rejected as unsuitable. It was
declined as unaffordable right now, and that is a different record to keep.

### Why bge-small

It is already the de facto candidate. Choosing otherwise orphans committed work:

- `docs/hardware/xdna2-bert-encoder-feasibility.md:3` takes `bge-small-en-v1.5`
  as the audit target for the entire XDNA2 NPU feasibility analysis.
- The cross-backend divergence result in `docs/acceptance/quantized-models.md`
  (~2.5e-3 at every quantisation, ~35× worse than f32's 7.3e-5) is measured with
  this model at 384 dims. Another choice makes that figure unmeasured again.
- Four quantisations exist locally with digests already recorded
  (`docs/acceptance/quantized-models.md:22-25`).
- Every embedding test is pinned to it: `embedding_agreement.rs`,
  `vulkan_agreement.rs`, `quantized_agreement.rs`.

## What this decision does NOT establish

- **bge-small was never selected on retrieval quality.** It was chosen as a
  fixture for size and determinism. Naming it the candidate **defers** the
  retrieval-quality question; it does not answer it. No retrieval benchmark has
  been run in this repository against any candidate.
- **No row is promoted by this ADR.** See below — this is deliberate.
- **No claim at 768 dims.** The divergence characterisation is a 384-dim result
  and does not transfer.
- **Nothing about the NPU path.** The feasibility audit remains an audit.

## The promotion hazard this creates, and how it is handled

`attained_support_status()`
(`crates/neuralcompose-mobile-core/src/runtime_target.rs:177`) checks
`candidate_model_executed` **before** any hardware-naming check — deliberately,
per the rationale at `:148-161` recording the defect PR #27 closed.

bge-small has already executed on both `linux/x86_64` rows. So naming it the
candidate makes those rows **eligible** for `DeviceValidated` on evidence that
already exists. The rung would be attained by relabelling, with nothing new run
and no evidence re-read.

That eligibility is real and is not hidden here. What this ADR declines to do is
**take** it as a side effect of a documentation change:

- `crates/neuralcompose-mobile-core/tests/tonight_evidence.rs` keeps
  `candidate_model_executed: false`. No assertion changes.
- The support-matrix rows stay at `RuntimeSmokeValidated`.
- Promotion is a separate change, with its own evidence read, that must argue
  the 2026-08-03 runs establish what `DeviceValidated` means — that the model
  which will ship ran on named hardware in the configuration it will ship in —
  and not merely that the same file was opened.

The decision is recorded here. Applying it is a deliberate second act. The gap
between them is intentional, and a reviewer finding
`candidate_model_executed: false` alongside this ADR is seeing the design, not
an oversight.

## Consequences

1. `docs/acceptance/m7-a.md` gains a dated superseding note. Its
   `embedding model selected: no` line is **not** edited: it is an accurate record
   of what M7-A claimed on 2026-07-28, and this registry does not retro-edit
   earlier records from later decisions.
2. Standing "fixture, not the candidate" prose across the support matrix, both
   Linux acceptance documents and the evidence test is amended in place — struck
   through, never deleted, per `docs/hardware/README.md:33-35`.
3. `docs/acceptance/embedding-candidate.md` pins the artifacts.
4. The fixture/candidate distinction in `attained_support_status()` is **not**
   weakened. It still separates a smoke run from a device validation. What
   changes is that the two now name the same weights, so the distinction has to
   be carried by the evidence read rather than by the model filename — which is
   a weaker guard than before, and worth knowing.
