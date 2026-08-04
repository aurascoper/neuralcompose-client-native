# `docs/hardware/` — machine facts that promote nothing

Documents here record **what a particular machine is and does**. They are deliberately *not* in
`docs/acceptance/`, and the distinction is the point.

| | `docs/acceptance/` | `docs/hardware/` |
|---|---|---|
| Answers | "does this row earn a rung?" | "what is this hardware?" |
| Can promote a support-matrix row | yes | **never** |
| Bound to ADR-002's ladder | yes | no |

ADR-002's ladder is `Contracted → BuildValidated → RuntimeSmokeValidated → DeviceValidated →
ReleaseSupported`, and **promotion is never by implication**. A hardware fact — a device enumerates,
a driver binds, a firmware image loads, an ioctl handler exists — is not a rung and does not imply
one. `attained_support_status()` is the machine-checkable form of that rule, and it returns `None`
for everything described in this directory.

Filing these under `acceptance/` would quietly assert the opposite: that establishing what the
silicon is constitutes evidence that a backend works. It does not. The XDNA documents here verify a
great deal about the NPU and **claim nothing** from it — that refusal is the most important thing in
them, and the directory layout should not undercut it.

## What is here

| file | subject |
|---|---|
| `xdna-npu-detection-evidence.md` | XDNA 2 NPU on `ok-cyberdeck`: device, driver bind, firmware selection, access model. |
| `xrt-userspace-intree-amdxdna-audit.md` | Whether AMD's XRT userspace can drive the in-tree `amdxdna`, whether a BF16 encoder compiles on Linux, and what Ubuntu version is actually required. |
| `xdna2-bert-encoder-feasibility.md` | Whether `bge-small-en-v1.5` can be compiled and run on XDNA 2 under Linux: operator support, BF16 against an f32-referenced index, Linux prior art, acquisition, artifact lifetime, and daemon access. **Supersedes the acquisition conclusion of the XRT audit** — Ubuntu 26.04 ships XRT itself, so that step needs no AMD `.deb` at all. |
| `rocm-gfx1150-jacc-amdgpu.md` | Ubuntu 26.04's archive ROCm 7.1 on the gfx1150 iGPU: native enumeration (no `HSA_OVERRIDE_GFX_VERSION`), AMDGPU.jl functional, JACC.jl suite 249/249 in Float64, three distinct vendor-math-library breakages (rocRAND/rocBLAS/rocSPARSE), install snapshots and an empirically proven rollback. |

Both were produced on the machine they describe and are copied here verbatim, including their
withdrawal notices and corrections. **Corrections are struck through in place rather than deleted**,
because a claim that was withdrawn and the reasoning that withdrew it are more useful than a
document that reads as if it had always been right.

## These files entered version control already corrected

**Both documents were written and revised as loose files outside any repository, and entered git on
2026-08-03 in their corrected state. Revisions made before that date are not recoverable.**

`git log` is therefore *not* the full history of the claims in them. Some corrections survive only
as strike-throughs in the text, and only because someone chose to strike rather than delete — the
`libboost-filesystem1.74.0` figure is the clearest example, and it is visible today purely as an
authoring choice, not because version control preserved it.

That gap is the reason this directory exists. Compare `docs/acceptance/vulkan-performance-claim.md`,
which was withdrawn the same day: it could retain its superseded claim verbatim *because the file
was already in git*, so the withdrawal is a diff and not an assertion. **Version control was the
difference between a withdrawal and a quiet edit**, and everything added here from now on gets that
property by default.

## Every GPU measurement records its power state

**A GPU measurement taken here records power source, CPU governor and thermal
state alongside kernel, driver and Mesa versions.** `scripts/capture-power-state.sh`
produces the record; redirect it into the run's `.evidence/` directory and hash it
with everything else.

This is a provenance field, not an experiment. The Radeon 890M power-gates between
samples, so the machine's power state changes a number by more than the thing being
measured usually does — and unlike a driver version, it cannot be recovered
afterwards from the artifact.

Two results were already distorted by it, and the shape is the same both times:

- A prompt-throughput pair where the **battery** figure (159.96 t/s) beat the **AC**
  figure (147.91 t/s). The AC number was adopted as citable and the battery one
  superseded. That picked a side; it did not explain the inversion, and it is
  exactly what would happen if AC-plugged runs sat idle longer between repetitions
  and gated down further.
- A reducer benchmark whose ~11× headline was mostly clock-ramp artifact. Arms
  tuned to one evaluation per sample paid a ~500–800 µs ramp that arms tuned to
  many evaluations amortised. Measured back-to-back the real figures are 1.58× and
  1.76×. Those were held and never published.

**Retrospective note.** Measurements predating this convention — including the
corrected 1.58×/1.76× reducer numbers — have back-to-back control over *inter-call*
gating but no established power profile for the session. The 2026-08-04 reducer
session provably straddles an AC-to-battery transition at 14:05 local
(`/var/lib/upower/history-charge-*.dat`), and the two inline probes that produced
the corrected figures left no timestamps, so they cannot be placed on either side
of it. Those numbers are steady-state with respect to call spacing and unestablished
with respect to power. Do not cite them as steady-state without qualification.

The general lesson is recorded because it inverts an existing one: the 1-in-3
segfault needed repetition for a defect to **appear**; power-gating needed
repetition for a defect to **disappear**. Same remedy, opposite directions — a
single run can manufacture a finding as easily as it can hide one.

## Provenance and scope

- One machine — a GPD G1617-02 (Win Mini 2025), Ryzen AI 9 HX 370 / Radeon 890M, Ubuntu 26.04,
  kernel 7.0.0-28-generic. **Nothing here generalises to other hardware, drivers, or kernels.**
- Findings are dated. Kernel, driver and vendor-package facts go stale; re-verify before relying on
  any of them, and prefer the reproduction commands each document carries over its conclusions.
- **Nothing in this directory has executed a model on the NPU under Linux.** No runtime target, no
  `backend_id`, no ONNX-ABI fixture exists for it.
