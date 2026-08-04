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

## Provenance and scope

- One machine — a GPD G1617-02 (Win Mini 2025), Ryzen AI 9 HX 370 / Radeon 890M, Ubuntu 26.04,
  kernel 7.0.0-28-generic. **Nothing here generalises to other hardware, drivers, or kernels.**
- Findings are dated. Kernel, driver and vendor-package facts go stale; re-verify before relying on
  any of them, and prefer the reproduction commands each document carries over its conclusions.
- **Nothing in this directory has executed a model on the NPU under Linux.** No runtime target, no
  `backend_id`, no ONNX-ABI fixture exists for it.
