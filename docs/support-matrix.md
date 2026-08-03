# Support matrix

Every (OS, architecture, backend) row carries exactly **one** support status
from ADR-002. Promotion is never by implication: each rung requires its own
evidence, and **compiling is not running**.

| Status | Means |
| --- | --- |
| `Contracted` | Schema and deterministic Rust behaviour exist. |
| `BuildValidated` | Code compiles and packages on the named OS/architecture. |
| `RuntimeSmokeValidated` | A deterministic fixture model executed successfully. |
| `DeviceValidated` | The real candidate model executed on named physical hardware. |
| `ReleaseSupported` | Signed packaging, install, upgrade, removal, and acceptance gates pass. |

`attained_support_status()` in `runtime_target.rs` is the machine-checkable
form of this table; a row may never claim more than that function returns
for its evidence.

## Rows

As of 2026-08-03 the two `linux/x86_64` GGUF rows are `RuntimeSmokeValidated`
— a deterministic fixture model executed on named hardware. Every other row
remains `Contracted`: no runtime has executed under those contracts.

**The two promoted rows ran a FIXTURE model, not the candidate**, which is why
they stop at `RuntimeSmokeValidated` — and, since 2026-08-03, why
`attained_support_status()` stops them there too. See "The checker distinguishes
a fixture from the candidate" below.

| OS | Arch | Backend ID | Runtime ABI | Status | Hardware | OS version | Driver/backend version | Pack ID + digest | Evidence commit | Acceptance doc | Known limitations | Last validated |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| android | arm64 | `llama-cpp-cpu` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | No runtime exists yet | 2026-07-29 |
| ios | arm64 | `coreml` | `nc-coreml-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | No runtime exists yet | 2026-07-29 |
| ios | arm64 | `apple-system-model` | `nc-system-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M7-C | 2026-07-29 |
| linux | x86_64 | `llama-cpp-cpu` | `nc-gguf-v1` | RuntimeSmokeValidated | GPD, AMD Ryzen AI 9 HX 370 w/ Radeon 890M | Ubuntu 26.04 LTS, kernel 7.0.0-28-generic | llama.cpp `d0bfb1981` | no pack; fixture bge-small-en-v1.5-f32 sha256 `bf40c42a…49b65` | `5b39d2a` | `docs/acceptance/llama-cpp-cpu-linux.md` | Fixture model only, not the candidate; BERT/f32 only; no `model_pack` verification; llama.cpp not vendored | 2026-08-03 |
| linux | x86_64 | `llama-cpp-vulkan` | `nc-gguf-v1` | RuntimeSmokeValidated | GPD, Radeon 890M (RADV STRIX1), ggml `IGPU`, `uma:1` | Ubuntu 26.04 LTS, kernel 7.0.0-28-generic | llama.cpp `d0bfb1981`, libvulkan 1.4.341, driver `radv` | no pack; fixture bge-small-en-v1.5-f32 sha256 `bf40c42a…49b65` | `8d14547` | `docs/acceptance/llama-cpp-vulkan-linux.md` | Fixture model only; one driver (RADV) on one iGPU; `bf16:0`; `bin/llama` target failed to link in that build | 2026-08-03 |
| windows | x86_64 | `llama-cpp-cpu` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M8 | 2026-07-29 |
| windows | x86_64 | `llama-cpp-vulkan` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M8 | 2026-07-29 |
| windows | x86_64 | `llama-cpp-cuda` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Optional pack; awaits M8-C + physical NVIDIA | 2026-07-29 |
| windows | x86_64 | `windows-ml-openvino` | `nc-onnx-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | CPU/GPU/NPU validate separately; awaits M8-D | 2026-07-29 |
| windows | arm64 | `windows-ml-qnn` | `nc-onnx-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Snapdragon only; never a Windows-x64 claim | 2026-07-29 |
| android | arm64 | `qnn` | `nc-onnx-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Snapdragon only; Pixel 8a is Tensor G3 and cannot supply this evidence | 2026-07-29 |

## The checker distinguishes a fixture from the candidate

`attained_support_status()` is described above as "the machine-checkable form of
this table", and until 2026-08-03 it was more permissive than the table it
enforces. The function had no notion of *which* model ran: it took
`fixture_runtime_executed` plus a named device, OS and backend version and
returned `DeviceValidated` — the rung this table reserves for "the real
candidate model executed on named physical hardware". A fixture run on named
hardware satisfied the checker while failing the definition, which is exactly
the evidence behind the two `RuntimeSmokeValidated` rows above.

`SupportEvidence` now carries `candidate_model_executed`, checked **before** the
hardware-naming conditions, because no amount of hardware detail turns a fixture
run into a device validation. The two linux rows name their device, OS and
backend and still attain `RuntimeSmokeValidated`, which is what they claim.

The field defaults to `false`, so the change can only ever **lower** an attained
rung, never raise one — an unmigrated caller loses a rung it was not entitled
to rather than silently gaining one. That direction is asserted by
`the_new_field_can_only_lower_an_attained_rung_never_raise_it`.

## CI jobs that are NOT matrix evidence

`swift-bindings-syntax-linux` compiles the UniFFI-generated Swift on a Linux
runner. It is a fast fail for a broken regeneration — seconds instead of a Mac
round-trip — and it is **not evidence for any row in this table**.

A green run there means the generated bindings parse and type-check under Linux
Swift. The `ios/arm64` rows are `Contracted`, and evidence produced on Linux can
never promote them: a Linux Swift compile is a fact about Linux Swift. Reading
that job as iOS readiness is the promotion-by-implication this document exists
to forbid, and the same shape as a canary named `ubuntu-26.04-preview` that was
in fact running 24.04.

The job is `continue-on-error` for the same reason: a Linux toolchain regression
must not gate a merge for a target it cannot speak to. It builds no SwiftUI
shell, no XCFramework, and runs no XCTest — none of which exist on Linux.

## Standing hardware gaps

- **Snapdragon**: no Snapdragon phone and no Windows-on-Snapdragon machine is
  on record, so neither QNN row can reach `DeviceValidated` today.
- **ROCm**: the gap is the GPU, not the OS. Canonical now ships ROCm in the
  Ubuntu archive (`sudo apt install rocm`, 26.04 universe), so the previous
  reason given here — that AMD's matrix does not list Ubuntu 26.04 — is no
  longer true. What remains true is that **the APU's iGPU is not an officially
  supported ROCm target**: gfx1150 (Radeon 880M/890M, RDNA 3.5) is absent from
  AMD's supported-hardware list, and running it needs
  `HSA_OVERRIDE_GFX_VERSION` to borrow another target's kernels. An override is
  a workaround, not support, and cannot be evidence for a row. Any ROCm row
  stays experimental until AMD lists the exact APU. Absent from this table by
  design.
- **MIGraphX**: excluded from generative-AI use while Microsoft's own
  documentation excludes that scenario.
