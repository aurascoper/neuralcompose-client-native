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

As of M7-A2 every row is `Contracted`. No runtime has executed and no
hardware has been exercised under these contracts.

| OS | Arch | Backend ID | Runtime ABI | Status | Hardware | OS version | Driver/backend version | Pack ID + digest | Evidence commit | Acceptance doc | Known limitations | Last validated |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| android | arm64 | `llama-cpp-cpu` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | No runtime exists yet | 2026-07-29 |
| ios | arm64 | `coreml` | `nc-coreml-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | No runtime exists yet | 2026-07-29 |
| ios | arm64 | `apple-system-model` | `nc-system-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M7-C | 2026-07-29 |
| linux | x86_64 | `llama-cpp-cpu` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M8 | 2026-07-29 |
| linux | x86_64 | `llama-cpp-vulkan` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M8 | 2026-07-29 |
| windows | x86_64 | `llama-cpp-cpu` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M8 | 2026-07-29 |
| windows | x86_64 | `llama-cpp-vulkan` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Awaits M8 | 2026-07-29 |
| windows | x86_64 | `llama-cpp-cuda` | `nc-gguf-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Optional pack; awaits M8-C + physical NVIDIA | 2026-07-29 |
| windows | x86_64 | `windows-ml-openvino` | `nc-onnx-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | CPU/GPU/NPU validate separately; awaits M8-D | 2026-07-29 |
| windows | arm64 | `windows-ml-qnn` | `nc-onnx-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Snapdragon only; never a Windows-x64 claim | 2026-07-29 |
| android | arm64 | `qnn` | `nc-onnx-v1` | Contracted | — | — | — | — | (this PR) | `docs/acceptance/m7-a2.md` | Snapdragon only; Pixel 8a is Tensor G3 and cannot supply this evidence | 2026-07-29 |

## Standing hardware gaps

- **Snapdragon**: no Snapdragon phone and no Windows-on-Snapdragon machine is
  on record, so neither QNN row can reach `DeviceValidated` today.
- **ROCm**: AMD's stable ROCm compatibility matrix does not list Ubuntu 26.04,
  so any ROCm row stays experimental until AMD lists both the exact APU and
  the OS. Absent from this table by design.
- **MIGraphX**: excluded from generative-AI use while Microsoft's own
  documentation excludes that scenario.
