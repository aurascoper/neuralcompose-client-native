# Quantised models on the two linux GGUF rows

Date: 2026-08-03
Status: **findings recorded; no row promoted**

Both `linux/x86_64` rows reached `RuntimeSmokeValidated` on **f32 only**, on a
device reporting `bf16: 0`. Q4_K / Q5_K / Q8_0 are different kernels in both the
CPU and Vulkan backends, and any production model is quantised — so this was the
largest untested surface in the two promoted rows, larger than model variety.

## Fixtures

Produced from the already-validated f32 model with llama.cpp's own quantiser, so
that **only the quantisation changes** between conditions:

```sh
llama-quantize bge-small-en-v1.5-f32.gguf bge-small-en-v1.5-Q4_K_M.gguf Q4_K_M
```

| model | bytes | sha256 (first 16) |
|---|---|---|
| f32 | 133 609 568 | `bf40c42ad7d89382…` |
| Q8_0 | 36 806 944 | `e269574ae150a017…` |
| Q5_K_M | 30 475 552 | `21f2a2fe481021b4…` |
| Q4_K_M | 29 203 744 | `7527bd85f499d44d…` |

Q5_K_M and Q4_K_M differ by only 1.3 MB: on a 33 M-parameter model the token
embedding and output tensors dominate and are not aggressively quantised, so the
size ladder is much flatter than it would be on a 7 B model.

## The code path works

All eight combinations — four quantisations × {CPU, Vulkan} — load and produce
384-dimensional finite embeddings through this repository's own backend, not
just through `llama-embedding`.

## The finding: quantisation degrades cross-backend agreement, not the embedding

| quant | cos vs f32 (cpu) | max diff vs f32 | **cos cpu↔vulkan** | max diff cpu↔vulkan |
|---|---|---|---|---|
| f32 | 1.000000000 | 0 | **0.999999894** | 7.30e-05 |
| Q8_0 | 0.999844 | 2.62e-03 | 0.999897 | 2.63e-03 |
| Q5_K_M | 0.998434 | 8.12e-03 | 0.999884 | 2.41e-03 |
| Q4_K_M | 0.997892 | 1.17e-02 | 0.999868 | 2.57e-03 |

Per-component divergence between the two backends goes from **7.3e-5 at f32 to
~2.5e-3 at every quantisation — roughly 35× worse**, and it is essentially flat
across Q8_0/Q5_K_M/Q4_K_M rather than tracking precision.

**For Q8_0 the cross-backend difference (2.63e-3) is the same size as the
quantisation error itself (2.62e-3).** Choosing a different backend perturbs the
vector as much as quantising it did.

### The practical consequence

**CPU and Vulkan are not interchangeable for a quantised embedding index.** A
vector written by one backend and queried by the other differs by more than
quantisation noise. Any index must record which backend produced it, or be
rebuilt when the backend changes.

An "identical across runtimes" bar stated at six decimal places — cosine
1.000000 — is **met by f32** (0.999999894 rounds to 1.000000) and **failed by
every quantisation measured here**. A pipeline that established a fidelity
result on f32 cannot assume it carries to a quantised deployment.

## What survives quantisation intact

**Determinism is bit-identical within a backend** for every quantisation, on
both CPU and Vulkan. `RuntimeSmokeValidated`'s "a deterministic fixture model
executed successfully" therefore holds for quantised models too.

**Semantic ordering is unaffected.** Related/unrelated cosine margin:

| | f32 | Q8_0 | Q5_K_M | Q4_K_M |
|---|---|---|---|---|
| CPU | 0.3800 | 0.3803 | 0.3804 | 0.3789 |
| Vulkan | 0.3800 | 0.3807 | 0.3819 | 0.3772 |

Every margin sits within 0.005 of f32's. Whatever quantisation perturbs, it is
not the geometry the retrieval task depends on.

## Latency — one observation, not a claim

Measured in a single session, `--bench`, 25 iterations per cell:

| | short | medium | long |
|---|---|---|---|
| f32 CPU | 2.64 | 22.61 | 40.00 |
| Q4_K_M CPU | **4.16** | 21.75 | **29.69** |
| f32 Vulkan | 3.48 | 5.06 | 12.35 |
| Q4_K_M Vulkan | 3.53 | 4.85 | 12.93 |

Quantisation makes the **CPU slower on short inputs** (4.16 vs 2.64 ms —
dequantisation overhead with little work to amortise it against) and faster on
long ones, while **Vulkan is essentially unaffected**.

**These absolutes are not comparable to the numbers in
`vulkan-performance-claim.md`**, which were taken in an earlier session on a
cooler machine — f32 medium reads 22.61 ms here against 18.71 ms there. Only the
within-run f32-vs-Q4_K_M comparison above is valid, and it is recorded as an
observation rather than added to the adopted claim.

## Verification

Five tests in `crates/neuralcompose-llama/tests/quantized_agreement.rs`,
gated by `NC_REQUIRE_QUANT=1` so an absent fixture cannot report a pass that
verifies nothing.

Mutation-checked by pointing the Q4_K_M fixture at the f32 file — the case where
a quantisation is silently not applied. Two tests fail, with the diagnostic
naming the cause:

```
Q4_K_M: cosine 1.000000000 against f32 is suspiciously exact — is the quant applied?
```

The cross-backend test asserts the degradation **exists** (divergence must be
>5× f32's) as well as bounded, so a silently tightened tolerance or a changed
kernel is noticed rather than quietly welcomed.

## A concurrency bug the quantised suite exposed

Adding a third test binary that loads models pushed the workspace suite over a
threshold and surfaced a latent defect: **the full suite crashed with SIGSEGV or
SIGABRT roughly one run in three**, while every binary passed in isolation.

Isolated by manipulation rather than inspection — `vulkan_agreement` crashed
**4 times in 10** with Rust's default parallel test threads and **0 times in 10**
with `--test-threads=1`. Concurrent context creation and teardown mutate
per-device state inside ggml-vulkan that is not guarded there.

Two attempts were needed and the first was wrong:

1. **Serialising backend registration with `Once`** — addressed
   `llama_backend_init` / `ggml_backend_load_all`, which was a real hazard but
   not this one. Crash rate afterwards: 4/8. Kept, because the race it prevents
   is genuine.
2. **A device lock around model load, device probe, context creation and
   teardown.** First version self-deadlocked: `load_tuned` held the lock and
   then called `devices()`, which takes the same non-reentrant mutex, so every
   run hung instead of crashing. Split into a locked public `devices()` and an
   unlocked internal path.

After: **0 crashes in 12 `vulkan_agreement` runs and 0 in 6 full-workspace
runs**, 197 tests each.

`embed` is deliberately outside the lock — each `Embedder` owns its context and
stays on one thread, so inference does not contend, and serialising it would
remove the concurrency the accelerator exists to provide.

This is a real defect in shipped code, not a test artifact. Any application
loading two models concurrently would have hit it.

## No row is promoted

- The two linux rows stay `RuntimeSmokeValidated`. Their `Known limitations`
  column says "BERT/f32 only"; this document extends what has been *exercised*,
  not what is *claimed*, and quantised evidence does not advance a rung.
- **Still one model, one architecture (BERT), one machine, one driver.** Adding
  quantisations does not add model variety.
- **k-quants were tested; legacy quants (Q4_0, Q4_1, Q5_0) were not**, nor were
  IQ-series or any imatrix-calibrated quantisation.
- **No generative model was quantised or run.** Every statement here is about a
  single encoder forward pass with pooled output.
- **Latency is one session's observation**, not part of the adopted performance
  claim.
