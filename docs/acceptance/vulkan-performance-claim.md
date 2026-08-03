# Vulkan performance — a proposed claim, and its boundaries

Date: 2026-08-03
Status: **PROPOSED**, not adopted

ADR-002's ladder has no performance rung. `RuntimeSmokeValidated` says a fixture
model executed; it says nothing about speed, and neither does any rung above it.
So this is a **separate, explicitly bounded claim** rather than a promotion, and
adopting it is a decision about what the project is willing to assert.

## The proposed claim

> On a GPD with an AMD Ryzen AI 9 HX 370 and Radeon 890M, running
> bge-small-en-v1.5 (33.2 M parameters, f32 GGUF, 384-dim) under llama.cpp
> `d0bfb1981`, offloading to the Vulkan backend reduces single-embedding
> latency by **2.2×–4.6×** for inputs between 60 and 300 words, and confers
> **no benefit** (0.96×–1.02×) on inputs of a few words.

That is the whole claim. It is about one model on one machine, and every clause
is load-bearing.

## Measurement

`neuralcompose-headless --bench`, 40 measured iterations per cell after 3
warmup, median of sorted samples, p90 reported alongside so a tail is visible
rather than hidden.

Three independent runs:

| case | words | cpu ms | accel ms | speedup |
|---|---|---|---|---|
| short | 3 | 2.156 / 2.253 / 2.189 | 2.237 / 2.198 / 2.163 | 0.96× / 1.02× / 1.01× |
| medium | 60 | 18.711 / 18.824 / 18.779 | 4.111 / 4.113 / 4.110 | 4.55× / 4.58× / 4.57× |
| long | 300 | 27.495 / 27.224 / 27.216 | 12.417 / 12.415 / 12.466 | 2.21× / 2.19× / 2.18× |

Run-to-run spread is under 1%. Median rather than mean because a single
scheduler preemption skews a mean and says nothing about the backend.

## The CPU baseline was checked for a handicap, and the first numbers were wrong

llama.cpp's `n_threads` default is `GGML_DEFAULT_N_THREADS` — a hard-coded **4**,
marked "TODO: better default" upstream. The first benchmark ran against that, on
a 24-core machine, which would have measured a handicap rather than a backend.

`n_threads` was plumbed through the shim, verified applied by reading it back
with `llama_n_threads()` (requesting 1 and 24 reports 1 and 24), and swept:

| threads | 4 | 8 | 12 | 16 | 24 |
|---|---|---|---|---|---|
| cpu ms, medium case | 17.78 | 17.94 | 18.17 | 18.66 | 18.43 |

**More threads do not help.** At 24 threads the process burns 4.17 s of CPU for
2.36 s of wall time — threads spawn and spin without reducing latency. For a
33 M-parameter f32 model this workload does not scale past roughly 4 threads.

So the comparison is fair at any thread count, and the speedups are not an
artifact of an under-threaded baseline. The numbers above use all cores.

A related question was settled while doing this: **CPU output is bit-identical
from 1 to 24 threads**, so `RuntimeSmokeValidated`'s "deterministic" does not
need qualifying as "deterministic at a fixed thread count". Pinned by
`cpu_output_is_identical_across_thread_counts`.

## An unexplained result, reported rather than smoothed

**The speedup is not monotonic in input length.** It peaks at the medium case
(4.6×) and falls at the long case (2.2×). The CPU curve is the odd one: 3 words
→ 2.2 ms, 60 words → 18.8 ms, 300 words → 27.2 ms. That is 8.6× for 20× the
tokens, then only 1.45× for a further 5×.

Attention is quadratic, so the long case should widen the gap, not narrow it. A
plausible cause is a different kernel or ubatch path being selected at larger
sizes, but **that is a guess and was not investigated.** The non-monotonicity is
reproducible across all three runs, so it is a real property of this
configuration and not noise.

Anyone extrapolating a single speedup number from this table will be wrong. That
is the main reason the claim states a range tied to an input-length band rather
than one figure.

## What this claim does not say

- **Nothing about generative models.** This measures a single encoder forward
  pass with pooled output. Token-by-token generation has a completely different
  profile — it is memory-bandwidth bound per token, and an iGPU shares that
  bandwidth with the CPU it is being compared against.
- **Nothing about larger models.** 33.2 M parameters at f32 is 127 MiB. Model
  size changes the arithmetic-intensity ratio that decides whether offload pays,
  and the 890M has no dedicated memory to hide it in.
- **Nothing about quantised models.** f32 only. Quantised kernels differ
  substantially between the CPU and Vulkan backends.
- **Nothing about other hardware.** One machine, one driver (RADV), one
  integrated GPU. A discrete card with dedicated memory is a different question
  entirely, and so is a different Vulkan driver on the same silicon.
- **Nothing about throughput or batching.** Single embeddings, serially. Batched
  inference is where an accelerator would normally be expected to widen its lead
  and it was not measured.
- **Nothing about power.** On a handheld this may matter more than latency, and
  no power measurement was taken.
- **Nothing about first-token or load latency.** Model load time is excluded by
  the warmup.

## Recommendation

Adopt it, or don't, as a documented claim in `docs/` — but **do not attach it to
a support-matrix row.** The matrix tracks correctness rungs, and mixing a
performance figure into a row would invite reading `RuntimeSmokeValidated` as
carrying a speed guarantee it does not.

If adopted, the honest one-line summary for a reader is:

> Vulkan offload is worth using for sentence-to-paragraph embeddings on this
> machine, and pointless for very short strings.

Re-measure before repeating it for any other model, quantisation, or device.
