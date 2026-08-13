# ADR-004 — Provenance vocabulary: how a value came to exist

Status: Proposed · 2026-08-13
Builds on: ADR-002 (measured-not-declared; an unmeasured tolerance is never a
satisfied tolerance)
Scope: one new module, one new contract subdirectory, one turn-log schema
version, one developer script. **Executes no reasoner, emits no RDF, promotes
no support-matrix row, and opens no ingestion path into any memory store.**

## Context

Four repositories each answer "how was this value produced" and each answers it
differently. `capture.rs` stamps a build identity onto a recording;
`conformance.rs` refuses to call an unmeasured tolerance satisfied;
`property_law.rs` will not share an index across embedding spaces;
`neural-memory-server` recomputes a derivation and rejects the write if it
cannot. And `neuralcompose-hypnagogic`'s turn log answered it **not at all** —
`TurnLine` carried no producing-software identity while the `CaptureManifest`
beside it had carried one since M4.

Two source analyses of this question both recommended *deferring* a shared
interchange profile until an external consumer and written competency questions
exist. The decision was taken to build the vocabulary now regardless. This ADR
records that as a deliberate call made with the deferral argument in view, and
answers the deferral's real objection by writing the competency questions down
and making each one an executable test (see *Verification*).

## Decisions

1. **Vocabulary, not inference.** Nothing in `provenance.rs` derives a new claim
   from existing ones. It records what a producer asserts and under which
   method. No general reasoner enters any canonical write path, here or in the
   sibling repositories. A future reasoner's output would be a separately typed,
   separately stored inference bundle, never merged into asserted records.

2. **Six assertion kinds, five of them borrowed exactly.**
   `observed`, `derivedDeterministically`, `humanDecision`, `agentInference` and
   `externalClaim` are spelled as `neural-memory-server`'s `EvidenceClass`
   serializes them. `heuristicAnnotation` is ours alone and maps to nothing
   there, deliberately: a threshold whose own source calls it unvalidated
   (`channel_health.rs:24`, `SpectralState.honestyCaveat`) is not evidence at any
   confidence.

3. **A missing evidence mapping is a decision, not a hole.**
   `EvidenceMapping::{Ingestible, NeverIngestible}` rather than
   `Option<EvidenceClass>`, so "this kind must never be ingested" cannot be read
   as "nobody has decided yet". There is no `NotYetDecided` variant — it would
   have no member today, and its existence is what would let a future kind sit
   undecided.

4. **Absence never matches absence.** `comparable()` returns `false` for two
   unknown embedding spaces. `a == b` would return `true`, licensing exactly the
   comparison the function exists to refuse.

5. **The turn-log line schema goes to v2 rather than being edited at v1.**
   `contracts/README.md`'s rule is that contract changes are new versions, and
   `verify_turn_log` already refuses an unrecognized line schema — the correct
   treatment of a v1 log by a v2 reader.

6. **A dirty tree yields no commit.** `build.rs` emits `NC_HYPNAGOGIC_COMMIT`
   only when `git status --porcelain=v2 --branch` shows a clean tree. A commit
   does not describe a build with uncommitted changes in it, and a present
   `gitCommit` is a reproducibility claim.

7. **Bounded contexts, integrated by reference.** `neural-memory-server` is the
   evidence authority; `claude-mind-mcp` is the personal-memory authority;
   `NeuralCompose` and this repository are typed producers owning no
   cross-project ontology. A cross-store reference never copies or upgrades the
   referenced record's kind. This matches policy already written in
   `claude-mind-mcp`'s `docs/sync/BANKING_POLICY.md`, which states that evidence
   stays canonical in `neural-memory-server` and that the personal coordinator
   must reject that domain — this ADR does not propose a change there.

## What this is not

**The mapping is vocabulary alignment, not a pipe.** Four of the five ingestible
mappings have no live ingestion path at all: `neural-memory-store/src/write.rs:266-269`
clamps every `WriteChannel::Agent` write to `agentInference` *before* the digest
is computed, and the MCP surface exposes no other door. Reading
`evidence_mapping` as a capability would be reading it wrong.

**Deferred, and named as deferred:** JSON-LD emission, PROV-O/SOSA/QUDT/SKOS
emitters, SHACL shapes, BFO alignment, `@context`, IRIs of any kind, and a
`provenance_envelope_identity()` seal (nothing keys off envelope identity yet).

### PROV-O correspondence

Carried because PROV-O is the only one of the candidate standards with terms for
what these types actually are. Five rows, and the last two are the valuable ones
— they record that the mapping is **not total**, which is what a future emitter
would need to know and what a tidy complete-looking table would hide.

| here | PROV-O |
|---|---|
| `ResourceRef` | `prov:Entity` |
| `MethodIdentity` | `prov:Activity` + `prov:SoftwareAgent` |
| `inputs` | `prov:used` / `prov:wasDerivedFrom` |
| `assertionKind` | **no PROV term**; nearest is `prov:wasAttributedTo` on the agent |
| `confidence` | **none** — PROV deliberately has no confidence |

SOSA, QUDT, SKOS, SHACL and BFO were considered and no per-term mapping is
written: a mapping with no emitter and no test is prose that rots, and
`contracts/README.md`'s versioning rule would make an unversioned aspirational
mapping the one artifact here nobody could safely change later.

## Four corrections this ADR lands with

All four were verified against source, not against prose describing it.

1. **The evidence taxonomy this work started from was wrong.** It named eight
   kinds. `neural-memory-server` at `5da4a5c` defines **five**
   (`crates/neural-memory-domain/src/terms.rs:49-63`): `humanAssertion`,
   `heuristicAnnotation` and `reasonerInference` appear nowhere in that
   repository, and `deterministicDerivation` is really `DerivedDeterministically`.
   Three inventions and one misspelling. `contracts/provenance/fixtures/evidence-class-names.json`
   plus `scripts/check-evidence-class-drift.sh` exist so this cannot recur
   silently.

2. **The "EEG does not bias the dialectic" ceiling was right for the wrong
   reason.** `SpectralGloss.scalar` maps four of five states to non-0.5, and the
   scalar reaches selection: gloss → `DialecticalField.advance` → weights →
   `potential` → softmax. On Linux the ceiling holds because **no estimator
   exists**, not because the gloss is inert. Corrected at `turn_log.rs`'s
   `NEUTRAL_GLOSS`.

3. **That estimator is MLX-backed, not Core ML** (`Sources/BCILLM/SpectralStateEstimator.swift:23`).
   `turn_log.rs:30` claimed Core ML and was wrong.

4. **ADR numbering is local to this repository.** `turn_log.rs` cited "ADR-005"
   and `embedding.rs` cites "ADR-010" and `docs/architecture/embedding_contract.md`;
   those are macOS-repository numbers and one absent document. The `turn_log.rs`
   citation is now labelled foreign; `embedding.rs`'s is not yet, and is the
   known remaining instance.

   **This ADR is 004 because 003 is taken by unmerged work**, not because 003 is
   free. `ADR-003-embedding-candidate-selection.md` is committed at `471ae61` on
   `origin/docs/adr-003-embedding-candidate`. It is invisible from a `main`-based
   checkout, which is exactly how a number gets reused: *absent from this
   working tree* is not *never written*. The first draft of this ADR asserted
   the latter and was wrong. Check `git log --all` before claiming a number is
   free.

Related, and still open in the Swift repository: `Embedding.cosineSimilarity`
returns `0` for incomparable operands
(`docs/reviews/embedding-comparability-unenforced-2026-08-12.md`), unpatched at
`01681b5`. `claude-mind-mcp` has the same root defect in a different shape — its
Core Data recall path (`Sources/ClaudeMindCore/MemoryStore.swift:1178-1183`)
gates cosine on dimension alone and never reads the row's `embeddingProfile`,
while its Postgres path checks it strictly. Dimension is not identity. Decision 4
is the direct response.

## Relaxing an earlier promise

The hypnagogic plan stated that `contracts/` and
`crates/neuralcompose-mobile-core/src/` would stay untouched. That was a fence
around the hypnagogic port — "this milestone adds no shared-contract surface" —
and it held for that milestone. It was never a permanent freeze, and
`contracts/README.md` already describes the legitimate way to change contracts:
a new subdirectory and a new version, never an edit in place.

This ADR adds `mobile-core/src/provenance.rs` and `contracts/provenance/`. It
modifies no existing module except a `pub mod` line in `lib.rs`, a `Serialize`
derive on `Tuning` (which the parameters digest needs), and the turn-log line
schema, which goes to v2 rather than being edited at v1.

**No uniffi derives yet.** Six mobile-core modules already have zero binding
surface. Adding permanent FFI for a type no shell constructs — on a machine that
cannot run the Swift side — is cost for nothing; the derives go on the day a
shell builds an envelope, which is when it will need to, since only a shell knows
its own version.

## Verification

Competency questions, each an executable test rather than a paragraph:

| # | Question | Test |
|---|---|---|
| 1 | Which software produced this turn-log line? | `a_turn_line_names_its_producing_software` |
| 2 | Is a recorded gloss of 0.5 a measurement or an absence? | `a_neutral_gloss_is_a_heuristic_annotation_not_a_measurement` |
| 3 | May these two similarity scores be compared at all? | `comparable_refuses_two_absences` |
| 4 | This number is derived — from what, and recomputable? | `derivation_requires_method_and_inputs_both_ways` |
| 5 | Which records may never enter the evidence store, and why? | `heuristic_annotation_is_the_only_never_ingestible` |
| 6 | Did a confidence value ever change how a claim is classified? | `confidence_never_promotes` |
| 7 | Under which frozen parameters did the dialectic run? | `the_parameters_digest_distinguishes_the_profiles` |
| 8 | Is this build pinned to a commit, or an unpinned tree? | `an_unpinned_build_says_so_rather_than_guessing` |
| 9 | Which channel-health status is a classification, not a default? | `a_neutral_gloss_is_a_heuristic_annotation_not_a_measurement` |

Question 7's test is the one worth naming: it asserts the three profiles produce
three *different* digests. "Is 64 hex characters" is satisfied perfectly well by
a constant, and that is the version of this feature that would otherwise ship.

```sh
cargo +1.97.1 fmt --check
cargo +1.97.1 clippy --all-targets --all-features -- -D warnings
cargo +1.97.1 test
./scripts/check-no-secrets.sh
./scripts/check-evidence-class-drift.sh   # fails if the sibling checkout is absent
```

The drift script is a **developer gate, not CI** — the runner has no sibling
checkout. Run it before changing the mapping. It is deliberately unable to pass
by skipping.

## Consequences

- A v1 turn log is unreadable by this build, by design. There are no v1 logs in
  circulation: logging is opt-in and off by default (macOS ADR-005).
- `evidence_mapping` will drift from upstream between runs of the drift script.
  That window is the honest cost of not having a runtime oracle, and it is
  written into the fixture's own comment rather than left implied.
- **`serde_json`'s default parser does not round-trip `f64`.** Found 2026-08-13
  while logging channel-health RMS: it writes the correct shortest form and
  reads it back up to 1 ULP low (`14.067217896390133` → `…131`). The payload
  digest still verifies, because that is over bytes — which is precisely why
  this would never have surfaced on its own, and why a record that cannot
  reproduce its own values is a provenance problem rather than a rounding one.
  The `float_roundtrip` feature is now enabled in both crates that persist
  records. No new dependency; a feature flag on one already present.
- Promotes no support-matrix row. `attained_support_status()` returns exactly
  what it returned before; per ADR-002 there is no promotion by implication.
