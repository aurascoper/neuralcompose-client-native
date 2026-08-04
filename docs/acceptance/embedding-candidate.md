# Embedding candidate pinning

Decision: [ADR-003](../architecture/decision-log/ADR-003-embedding-candidate-selection.md)
Date: 2026-08-04
Machine: GPD, AMD Ryzen AI 9 HX 370 w/ Radeon 890M, Ubuntu 26.04 LTS, kernel
7.0.0-28-generic

**No model bytes are in this repository.** Only revisions, commands, and
digests. The artifacts live in `~/models/`, outside the repo.

## The candidate

`bge-small-en-v1.5` — BERT-family, 33.2 M parameters, 384 dimensions, mean
pooling, 512 max tokens, symmetric (no task prefix).

**Licence: UNVERIFIED.** No document in this repository records this model's
licence, and it was not confirmed from upstream while writing this. Since
`ModelPackCatalogEntry.license_id` is a required field, this must be established
from the pinned revision before any catalog entry is written. It is recorded as
a gap rather than asserted from recollection.

This model has been present in this repository since M7-A as a **fixture**.
ADR-003 designates it the candidate. Nothing about the artifacts changed; the
designation did.

## Pinned inputs

| Repository | Revision | Status |
| --- | --- | --- |
| `BAAI/bge-small-en-v1.5` | `5c38ec7c405ec4b44b94cc5a9bb96e735b38267a` | **pinned for the ONNX path only** — see the gap below |
| `ggml-org/llama.cpp` (quantisation) | `d0bfb1981266c271cd0536a8aa7c5e863e7cdf61` | verified — the runtime commit in both Linux rows |

### GAP — the f32 GGUF's conversion provenance is ABSENT

The revision above is pinned in `tools/onnx/README.md:45` and
`docs/hardware/xdna2-bert-encoder-feasibility.md:1883` for the **ONNX export**
path. Nothing in this repository records where `bge-small-en-v1.5-f32.gguf`
came from — whether it was converted locally from that same revision, converted
from another, or downloaded pre-built from a community repository.

The digest below is stable and every result in this repository is reproducible
against it, so no measurement is in question. What is unestablished is the chain
from upstream weights to this file. **Closing this gap is a prerequisite for
`ReleaseSupported`**, which requires signed packaging, and it should be closed
before any promotion argument leans on upstream identity rather than on the
digest.

Until then the f32 GGUF is pinned **by digest, not by provenance.**

## Artifact digests

Verified with `sha256sum` on 2026-08-04 against `~/models/`. All four match the
table recorded in `quantized-models.md:22-25`.

| Artifact | SHA-256 | Bytes |
| --- | --- | --- |
| `bge-small-en-v1.5-f32.gguf` | `bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65` | 133 609 568 |
| `bge-small-en-v1.5-Q8_0.gguf` | `e269574ae150a0170ccf68c86dc37b744757d76d96a1ab05df4d3fdf55d5db5e` | 36 806 944 |
| `bge-small-en-v1.5-Q5_K_M.gguf` | `21f2a2fe481021b4dcdc596e25ca5854bf3aa3a214e629362bbbf564a65cdca3` | 30 475 552 |
| `bge-small-en-v1.5-Q4_K_M.gguf` | `7527bd85f499d44df03c8ac4397042d90c0284c0343aaa0af871502825da4769` | 29 203 744 |

The three quantised artifacts are derived from the f32 file, so **only the
quantisation differs** between conditions:

```sh
llama-quantize bge-small-en-v1.5-f32.gguf bge-small-en-v1.5-Q4_K_M.gguf Q4_K_M
```

## The embedding contract this candidate implies

```
EmbeddingContract {
    tokenizer_id:     "bge-small-en-v1.5",
    dimensions:       384,
    pooling:          Mean,
    normalization:    L2,
    task_instruction: None,      // symmetric — no query/passage asymmetry
}
```

`task_instruction: None` is the substantive part. A candidate needing an
asymmetric prefix (E5's `"query: "` / `"passage: "`) could not be expressed
here — there is one slot, and it is hashed into `embedding_space_identity()`.
See ADR-003.

**No catalog entry is committed by this document.** Only fixtures with
placeholder digests exist under `contracts/model-packs/fixtures/`. Writing a
real `ModelPackCatalogEntry` is packaging work and belongs with `model_pack`
verification, which no Linux row has.

## Which backend produced an index still matters

`quantized-models.md` establishes that CPU and Vulkan are **not
interchangeable** for a quantised embedding index: cross-backend divergence goes
from 7.30e-05 at f32 to ~2.5e-03 at every quantisation, roughly 35× worse.

Naming the candidate does not soften this. Any index built with this model must
record which backend produced it, or be rebuilt when the backend changes. That
constraint is a property of the pair (model, backend), and it survives the
fixture-to-candidate relabelling unchanged.

## Scope — what this does and does not promote

**Not promoted:**

- **`DeviceValidated` for any row.** bge-small has already executed on both
  `linux/x86_64` rows, so naming it the candidate makes those rows *eligible* on
  existing evidence. ADR-003 declines to take that rung as a side effect of a
  documentation change. `tonight_evidence.rs` still carries
  `candidate_model_executed: false`, and promotion is a separate change with its
  own evidence read.
- **Any retrieval-quality claim.** This model was chosen for size and
  determinism as a fixture, and no retrieval benchmark has been run against it
  or against any alternative in this repository. ADR-003 defers the question; it
  does not answer it.
- **Any upstream-identity claim.** See the provenance gap above.
- **Any 768-dimension result.** The divergence characterisation is 384-dim.
- **Anything about the NPU path.** `xdna2-bert-encoder-feasibility.md` remains
  an audit; nothing has executed on the NPU under Linux.
