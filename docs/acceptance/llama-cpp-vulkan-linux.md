# `linux/x86_64/llama-cpp-vulkan` — runtime smoke acceptance

Date: 2026-08-03
Branch: `feat/llama-cpp-vulkan-row`
Base: `5b39d2a`

The second executing backend, and the first to run a model somewhere other than
the CPU.

## Claim

| Claim | State |
|---|---|
| A GGUF model executes on the Vulkan backend on `linux/x86_64` | **ACHIEVED** |
| The accelerator is identified, not assumed | **ACHIEVED — `Vulkan0`, IGPU, RADV STRIX1** |
| Output agrees with the Vulkan reference binary | **ACHIEVED — <2e-6 across sampled dimensions** |
| Requesting offload with no accelerator reports CPU, not Vulkan | **ACHIEVED — asserted in both directions** |
| `linux/x86_64/llama-cpp-vulkan` → `RuntimeSmokeValidated` | **PROPOSED — see Scope** |
| Any other row promoted | **NO** |

## Named hardware and software

| Field | Value |
|---|---|
| Machine | GPD, AMD Ryzen AI 9 HX 370 w/ Radeon 890M (Strix Point) |
| OS | Ubuntu 26.04 LTS, kernel 7.0.0-28-generic |
| Accelerator | `Vulkan0` — AMD Radeon 890M Graphics (RADV STRIX1) |
| ggml device type | `IGPU` (integrated, `uma: 1`) — **not** `GPU` |
| Device caps | fp16: 1, bf16: 0, warp 64, shared mem 65536, `KHR_coopmat` |
| Vulkan | libvulkan 1.4.341, API 1.4.335, driver `radv` |
| Shader compiler | `glslc` (system) |
| llama.cpp | `d0bfb1981266c271cd0536a8aa7c5e863e7cdf61` |
| Build flags | `-DGGML_VULKAN=ON -DBUILD_SHARED_LIBS=ON -DGGML_NATIVE=ON` |
| Backend ID | `llama-cpp-vulkan` |
| Runtime ABI | `nc-gguf-v1` |
| Fixture model | bge-small-en-v1.5, f32 GGUF, 384-dim, 33.2 M params |
| Model sha256 | `bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65` |

**The device is an integrated GPU and is recorded as one.** ggml distinguishes
`GGML_BACKEND_DEVICE_TYPE_IGPU` from `GPU`, and the 890M reports the former with
`uma: 1` — it shares host memory. Collapsing the two would misdescribe the
machine, and the memory envelope of an iGPU is the host's, which matters for any
future claim about model size on this hardware.

## The backend ID is derived from the outcome, never the request

llama.cpp runs on CPU **without complaint** when offload is requested and no
accelerator is found. A backend identifier taken from configuration would
therefore file a CPU run as Vulkan evidence in exactly the case that matters.

So `Embedder::backend_id()` reports `llama-cpp-vulkan` only when layers were
requested *and* an accelerator was actually enumerated. Both directions are
asserted:

```
# CPU-only build, 99 layers requested
backend llama-cpp-cpu — 99 layers requested but NO accelerator enumerated

# Vulkan build, 99 layers requested
backend llama-cpp-vulkan on Vulkan0 (AMD Radeon 890M Graphics (RADV STRIX1))

# Vulkan build, 0 layers requested (control)
backend llama-cpp-cpu
```

The third line matters as much as the second: the same binary against the same
libraries falls back to a correctly-labelled CPU run when offload is not asked
for.

## Agreement — and why this row needs its own reference

**The Vulkan row cannot reuse the CPU row's numbers.** The same model and input
through the two backends differ by up to **9.06e-05** per component, far outside
the 2e-6 tolerance `embedding_agreement.rs` asserts. Loosening that test to
accommodate this one would silently stop checking the CPU path, so each backend
is pinned against its own reference binary.

| text | max abs diff | mean abs diff | cosine(cpu, vulkan) |
|---|---|---|---|
| "the headband is on" | 7.300e-05 | 1.885e-05 | 0.999999894 |
| "neural compose" | 7.390e-05 | 2.054e-05 | 0.999999877 |
| "quarterly tax filing deadline" | 9.060e-05 | 1.995e-05 | 0.999999884 |

The divergence is expected — GPU kernels reduce in a different order and may use
fp16 intermediates, so bit-identical output across backends was never available.

**And the divergence is itself the evidence that computation moved.** If the two
backends had agreed bit-for-bit, the likeliest explanation would be that Vulkan
was never used. The test therefore asserts *both* that the outputs differ
(> 1e-7) and that they are semantically identical (cosine > 0.9999) — identity
would be evidence against this row's claim, not for it.

Against the Vulkan reference binary itself, agreement is <2e-6 across eight
sampled dimensions on each of two strings.

## Verification matrix

| build | tests | clippy |
|---|---|---|
| stub, `LLAMA_CPP_DIR` unset (what CI sees) | **188 passed** | 0 errors |
| CPU, `NC_REQUIRE_BACKEND=1` | **188 passed** | 0 errors |
| Vulkan, `NC_REQUIRE_BACKEND=1 NC_REQUIRE_VULKAN=1` | **188 passed** | 0 errors |

`NC_REQUIRE_VULKAN=1` converts every skip into a hard failure, for the reason
`NC_REQUIRE_BACKEND` exists: a skipped Rust test reports `ok`, so a CPU-only
build would otherwise produce the identical "all passed" line as a real Vulkan
run and make an unexercised backend look verified.

## Scope — what this does and does not promote

**Proposed:** `linux/x86_64` / `llama-cpp-vulkan` / `nc-gguf-v1`,
`Contracted → RuntimeSmokeValidated`.

**Not promoted:**

- **`DeviceValidated` for this row.** ~~That rung needs the *real candidate
  model*; bge-small-en-v1.5 is a fixture chosen for size and determinism.~~
  **Amended 2026-08-04 ([ADR-003](../architecture/decision-log/ADR-003-embedding-candidate-selection.md)):**
  bge-small-en-v1.5 *is* the candidate as of that decision, so this row is now
  eligible for the rung on the evidence below. It is still not promoted — ADR-003
  declines to take a rung by relabelling, and the model was selected for size and
  determinism, never for retrieval quality. Promotion needs a deliberate evidence
  read, not this document plus a rename.
- **Any Windows row**, including the two `llama-cpp-vulkan` ones. Vulkan is
  portable and this evidence is not: different driver, different OS, never run.
- **Anything about the EEG path.** `--embed` opens no socket and touches no
  headband.
- **Any performance claim.** Latency and throughput were not measured, and on an
  iGPU sharing host memory bandwidth there is no reason to assume Vulkan is
  faster than 24 CPU cores for a 33 M-parameter model. **Nothing here says this
  is quicker** — only that it runs and is correct.

## Non-claims

- **One accelerator, one driver.** RADV on a Radeon 890M. AMDVLK, NVIDIA, Intel
  and discrete AMD are all untested; the `IGpu` assertion in the test would
  legitimately need relaxing on a discrete card.
- **`bf16: 0` on this device**, so any future model requiring bf16 is outside
  what this run establishes.
- **The `llama` CLI target failed to link** in the Vulkan build (`bin/llama`,
  link error). Every library used here built cleanly and the failure is in a
  tool this repository does not consume, but the build is not wholly green and
  that is recorded rather than glossed.
- **No `model_pack` verification.** The digest above is recorded by hand; the
  core's verification contract is still not consulted by either backend.
- **Determinism was checked within a backend, not across builds.** Two runs on
  Vulkan agree; a rebuilt shader cache or a driver update is untested.
- One model, one architecture (BERT), one quantisation (f32).
