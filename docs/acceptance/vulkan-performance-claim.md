# Vulkan performance — adopted claim

Date: 2026-08-03 (revised same day after a token-resolution sweep)
Status: **ADOPTED** as a documented claim in `docs/`. Deliberately **not**
attached to any support-matrix row.

ADR-002's ladder has no performance rung. A row reading `RuntimeSmokeValidated`
must not imply a speed guarantee, so this lives here and nowhere else.

## The claim

> On a GPD with an AMD Ryzen AI 9 HX 370 and Radeon 890M, running
> bge-small-en-v1.5 (33.2 M parameters, f32 GGUF, 384-dim) under llama.cpp
> `d0bfb1981`, offloading a single embedding to the Vulkan backend:
>
> - **1.0×–2.0×** — no reliable benefit — below **32 tokens**
> - **5.4× falling to 1.7×**, monotonically, from **32 to 512 tokens**
>
> The band is stated in **tokens**, not words. The peak is a step at 32 tokens
> and it is a **CPU-side discontinuity**, not a GPU effect.

### The product's own input sits in the no-benefit band

NeuralCompose embeds a partially composed sentence — a handful of words, on the
order of **4–16 tokens**. Measured speedups there are 1.55 / 1.01 / 1.31 / 1.98
at 4 / 8 / 12 / 16 tokens: scattered around unity with no consistent direction.

**Vulkan offload should not be shipped for the carousel path on the strength of
this document.** The 5.4× figure belongs to an input length this application may
rarely produce, and citing it as a reason to enable Vulkan for typing would be a
misreading of the measurement.

## Measurement

`neuralcompose-headless --sweep`, 30 measured iterations per cell after 3
warmup, median of sorted samples. Token counts are exact — text is grown one
word at a time until the model's own tokenizer reports the target, so the
x-axis is the unit the kernels actually see.

| tokens | speedup | cpu ms | | tokens | speedup | cpu ms |
|---|---|---|---|---|---|---|
| 4 | 1.55 | 3.25 | | 64 | 4.54 | 18.47 |
| 8 | 1.01 | 2.47 | | 96 | 3.13 | 21.38 |
| 12 | 1.31 | 3.65 | | 128 | 3.01 | 22.35 |
| 16 | 1.98 | 5.46 | | 192 | 2.65 | 23.74 |
| 24 | 1.86 | 5.57 | | 256 | 2.37 | 26.09 |
| **32** | **5.43** | **16.40** | | 384 | 1.98 | 29.88 |
| 48 | 4.76 | 17.83 | | 512 | 1.71 | 34.12 |

## The non-monotonicity is explained, and it was not what was hypothesised

### The dispatch-boundary hypothesis is refuted structurally

The proposed explanation was a `n_ubatch` boundary: llama.cpp splits a batch
into micro-batches, each submit costing a fixed round-trip the CPU path does not
pay. The test was to sweep `n_ubatch` and see whether a step moved with it.

**It cannot move, because the batch is never split.** llama.cpp asserts:

```
GGML_ASSERT(cparams.n_ubatch >= n_tokens && "encoder requires n_ubatch >= n_tokens")
```

A bidirectional encoder cannot be micro-batched — every token attends to every
other — so requesting a smaller `n_ubatch` does not produce a different dispatch
pattern, it aborts the process. The three legal cells measured before that abort
read 5.41 / 5.45 / 5.47 at 32 tokens for `n_ubatch` 128 / 256 / 512: identical,
as they must be.

This is a stronger refutation than a flat timing curve would have been. A flat
curve is also what an *unapplied* parameter looks like; a compile-time assertion
in the library is not ambiguous.

### The step is on the CPU side

At the 24→32 token step the **GPU is flat** (2.99 → 3.02 ms) while the **CPU
triples** (5.57 → 16.40 ms). The discontinuity is entirely in the CPU baseline.

Sweeping threads across it identifies the mechanism:

| cpu ms | 16 tok | 24 tok | 32 tok | 48 tok |
|---|---|---|---|---|
| 1 thread | 9.81 | 15.68 | 16.56 | 17.85 |
| 2 threads | 5.98 | 8.72 | 16.75 | 17.28 |
| 4 threads | 3.81 | 5.45 | 16.11 | 17.92 |
| 24 threads | 3.33 | 5.10 | **16.66** | 17.90 |

**Below 32 tokens the CPU backend parallelises well** — roughly 3× from 1 to 24
threads. **At and above 32 tokens it stops entirely**: 16.56 ms on one thread
against 16.66 ms on twenty-four. The apparent Vulkan "peak" at 32 tokens is the
CPU baseline falling off a cliff, not the GPU getting faster.

The precise cause inside ggml's CPU backend — a blocked or repacked GEMM path
selected above a size threshold is the obvious suspect — **was not identified**,
and is not claimed here.

### This corrects an earlier statement in this document

A previous revision said "this workload does not scale past roughly 4 threads,"
based on a measurement taken at 60 words (~62 tokens). That is true **only above
the discontinuity**. Below 32 tokens threading helps substantially, and that is
exactly the regime this product operates in. The earlier sentence was measured
in one place and stated generally; it is withdrawn.

The conclusion it supported still holds for the reported band: the comparison
above 32 tokens is not an artifact of an under-threaded CPU, because the CPU is
thread-insensitive there at any count.

## Determinism, settled while measuring

CPU output is **bit-identical from 1 to 24 threads**, so
`RuntimeSmokeValidated`'s "deterministic" needs no qualification about thread
count. Pinned by `cpu_output_is_identical_across_thread_counts`.

## Method notes

- `n_threads` and `n_ubatch` are both **read back** from llama.cpp
  (`llama_n_threads`, `llama_n_ubatch`) rather than assumed to have applied. The
  sweep aborts if a requested `n_ubatch` is not the active one, because a
  silently-ignored parameter produces exactly the flat result that would be read
  as a refuted hypothesis.
- llama.cpp's `n_threads` default is a hard-coded **4**, marked
  `TODO: better default` upstream. An unconfigured baseline uses 4 of 24 cores.
- Median, not mean: one scheduler preemption skews a mean and says nothing about
  a backend.
- The sweep stops at 512 tokens. bge-small's trained context is 512 with learned
  position embeddings, so beyond it the model is out of distribution and the
  timing would not describe anything meaningful.

## What this claim does not say

- **Nothing about generative models.** This is a single encoder forward pass
  with pooled output. Token-by-token generation is memory-bandwidth bound per
  token, and this iGPU shares that bandwidth with the CPU it is compared
  against.
- **Nothing about quantised models.** f32 only, and `bf16: 0` on this device.
  Q4_K/Q5_K kernels are a different code path in both backends and neither row
  has exercised them. This is a larger gap than model variety.
- **Nothing about larger models.** 33.2 M parameters is 127 MiB. Size changes
  the arithmetic-intensity ratio that decides whether offload pays.
- **Nothing about other hardware.** One machine, one driver (RADV), one
  integrated GPU with `uma:1`.
- **Nothing about batching.** Single embeddings, serially. Batching is where an
  accelerator would normally widen its lead and it was not measured.
- **Nothing about power or sustained thermals.** On a handheld both may matter
  more than latency; 30-iteration cells do not surface throttling.
- **Nothing about the cause of the 32-token CPU discontinuity**, only its
  existence, its size, and that it is thread-related rather than a GPU boundary.
- **Nothing comparative about any Mac.** There are no macOS rows in the support
  matrix and no `llama-cpp-metal` backend; a comparison would be measured
  against a hypothetical.

## One-line summary for a reader in a hurry

> Vulkan offload is worth using for paragraph-length embeddings on this machine,
> and pointless for the short inputs this application actually produces.

Re-measure before repeating any of this for another model, quantisation, or
device.
