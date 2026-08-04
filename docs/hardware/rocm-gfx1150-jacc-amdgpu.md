# ROCm 7.1 (Ubuntu archive) + AMDGPU.jl + JACC.jl on gfx1150 — evidence, ok-cyberdeck

**Date:** 2026-08-04 · **Host:** GPD G1617-02, Ryzen AI 9 HX 370, Radeon 890M (Strix Point)
**OS:** Ubuntu 26.04 LTS (resolute), kernel 7.0.0-28-generic

This records what happened when Ubuntu 26.04's archive ROCm 7.1 was installed on this machine and Julia's AMD GPU stack (AMDGPU.jl, then JACC.jl's test suite) was run against the gfx1150 iGPU. Install steps were snapshotted and the uninstall path was proven empirically before the large package set went on.

> **This document claims no support-matrix row — not even `Contracted`.** It is machine facts in the sense of `docs/hardware/README.md`: it promotes nothing.

---

## 0. Bottom line

1. **gfx1150 enumerates natively under Ubuntu 26.04's archive ROCm 7.1 — no `HSA_OVERRIDE_GFX_VERSION` needed.** `rocminfo` with the variable unset reports agent `gfx1150` with ISAs `amdgcn-amd-amdhsa--gfx1150` and `amdgcn-amd-amdhsa--gfx11-generic`. Setting `11.5.0` is identical; setting `11.0.0` masquerades the device as `gfx1100`. Nobody appears to have written this down for Strix Point + 26.04: the override folklore (11.0.0 vs 11.5.0 threads) is moot on this stack.
2. **AMDGPU.jl v2.7.1 is `functional()` on this stack with zero configuration.** It discovered the Debian-layout ROCm (no `/opt/rocm`) on its own: HIP from `/usr/lib/x86_64-linux-gnu`, LLD from `llvm-21`, device libraries from its own Julia artifact.
3. **The JuliaGPU codegen path works on gfx1150**: elementwise kernels compile and run in both Float32 and Float64.
4. **Ubuntu's ROCm math libraries are broken for gfx1150 in three distinct ways** (packaging gaps, not Julia bugs): rocRAND **segfaults** (ships gfx1151 kernels but not gfx1150), rocBLAS **fails cleanly** (ships only `TensileLibrary_lazy_gfx942.dat` — datacenter-only GEMM kernels), rocSPARSE's version query **crashes its isolated subprocess** (no gfx115x objects at all).
5. **JACC's full test suite passes on gfx1150: 249/249, zero failures**, run in Float64 (the suite's default on AMDGPU), via the canonical CI invocation (`--project` = the JACC clone, `set_backend("AMDGPU")`, `Pkg.test()`). Every compute testset passed: AXPY, reduce, reduce-ND, shared, JACC.BLAS, Add-2D/3D/ND, CG, LBM, rand-Float32/Float64, UnitRange/StepRange, LaunchSpec. This works *because* JACC's portable paths never touch the broken vendor libraries: `JACC.BLAS` is JACC's own kernels (src/blas.jl), and the rand tests call device-side `rand()` inside kernels (GPUCompiler native device RNG), not the rocRAND host API.
6. **JACC.Multi on AMDGPU is not skipped at runtime — it is excluded at the source level**: `if JACC.backend != "amdgpu" && JACC.backend != "metal"` (test/unittests.jl:1054, commit `0dbf990a`). No `@test_skip`, no reason string, no hardware condition. Relevant to JuliaGPU/JACC.jl#381.
7. The first direct observation of gfx1150 on this machine was free and read-only, **before any install**: KFD topology reports `gfx_target_version 110500` (node 1). Every prior document inferred gfx1150 from kernel IP blocks.

## 1. Verified on this machine

Every row is verified-here; the command column produced the fact.

| Fact | Value | Produced by |
|---|---|---|
| KFD GPU target, pre-install, read-only | `gfx_target_version 110500` (node 1; node 0 = CPU, 0) | `grep -H gfx_target_version /sys/class/kfd/kfd/topology/nodes/*/properties` |
| rocminfo GPU agent, `HSA_OVERRIDE_GFX_VERSION` **unset** | `gfx1150`, Marketing "AMD Radeon 890M Graphics", ISAs `amdgcn-amd-amdhsa--gfx1150`, `…--gfx11-generic` | `rocminfo` |
| rocminfo agent under override `11.5.0` | identical to unset (`gfx1150`) | `HSA_OVERRIDE_GFX_VERSION=11.5.0 rocminfo` |
| rocminfo agent under override `11.0.0` | `gfx1100` (masquerade) | `HSA_OVERRIDE_GFX_VERSION=11.0.0 rocminfo` |
| Additional rocminfo agent | XDNA NPU enumerates as HSA agent `aie2` | `rocminfo` |
| rocm-smi | works: device 0 = node 1, DID `0x150e`, live temp/power; benign `map::at` exception + low-power warning on APU N/A fields | `rocm-smi` |
| AMDGPU.jl | v2.7.1, `AMDGPU.functional() == true`, device table: gfx1150, wavefront 32, 11.289 GiB | `AMDGPU.versioninfo()`; `AMDGPU.functional()` |
| AMDGPU.jl ROCm discovery | found Debian layout unaided: HIP 7.1.52801 at `/usr/lib/x86_64-linux-gnu/libamdhip64.so`, LLD `/usr/lib/llvm-21/bin/ld.lld`, device libs from Julia artifact | `AMDGPU.versioninfo()` |
| Elementwise kernel, Float32 | works (`a.*2f0 .+ a` on 1024 elts, correct sum) | julia one-liner, log `amdgpu-jl-smoke.txt` |
| Elementwise kernel, Float64 | works (correct sum; one benign `SharedSignalPool` leak warning at exit) | log `amdgpu-jl-float64.txt` |
| `AMDGPU.rand(Float64, …)` | **SIGSEGV** in `librocrand.so` → `libamdhip64.so`, during `rocrand_initialize_generator` (seeding, before any math) | log `amdgpu-jl-smoke.txt` |
| `AMDGPU.rand(Float32, …)` | **SIGSEGV**, identical stack — the crash is type-independent, it is rocRAND init, not Float64 | log `amdgpu-jl-rocrand-f32.txt` |
| rocBLAS `mul!` | clean error, no crash: `Cannot read /usr/lib/x86_64-linux-gnu/rocblas/5.1.0/library/TensileLibrary.dat: Illegal seek for GPU arch : gfx1150`; only available file is `TensileLibrary_lazy_gfx942.dat` | log `amdgpu-jl-rocblas.txt` |
| rocSPARSE version query | crashed/timed out in AMDGPU.jl's isolated subprocess; AMDGPU.jl warns citing JuliaGPU/AMDGPU.jl#920 | `AMDGPU.versioninfo()` |
| librocrand1 embedded gfx11 targets | `gfx1100 gfx1101 gfx1201 gfx1200 gfx1151` … — **no gfx1150** | `strings librocrand.so \| grep -oE 'gfx[0-9]+…'` (heuristic) |
| librocsparse / librocsolver gfx11 targets | `gfx1100 gfx1102` only — no gfx115x | same heuristic |
| librocfft gfx11 targets | `gfx1100 gfx1101 gfx1102 gfx1151` — no gfx1150 | same heuristic |
| librocblas gfx11 targets (host lib) | includes `gfx1150` — but Tensile GEMM kernel *files* on disk are gfx942-only | heuristic + rocBLAS error text |
| Graphics stack after full ROCm install | **unchanged, byte-identical subset**: libdrm2 2.4.131-1, libegl-mesa0/libgl1-mesa-dri/mesa-vulkan-drivers 26.0.3-1ubuntu1, libvulkan1 1.4.341.0-1 | `diff` of pre/post baselines |
| Vulkan after full ROCm install | RADV STRIX1 still device 0, `driverName = radv`, API 1.4.335 | `vulkaninfo --summary` |
| ROCm install footprint | stage 1: 5 packages; stage 2 (`rocm` metapackage): 127 newly installed, **0 upgraded, 0 removed** | apt logs `apt-stage1-install.txt`, `apt-stage2-install.txt` |
| Uninstall path | **proven empirically, not simulated**: `apt-get remove rocminfo rocm-smi libhsa-runtime64-1` + `autoremove` → zero rocm/hsa packages remained, graphics subset untouched, reinstall restored all 5 | log `rollback-proof.txt` |
| `/dev/kfd` access | logind session ACL (`user:aurascoper:rw-`), **not** group membership; session-scoped — SSH/systemd/container runs need `render` group or explicit ACL. Same daemon constraint as `/dev/accel/accel0`. | `getfacl /dev/kfd`; `id` |

## 2. JACC.jl test suite on the amdgpu backend

Two runs, both captured verbatim; the difference between them is itself a finding.

**Run 1 (non-canonical env) — aborted before any compute test.** Setup: external scratch env, `Pkg.develop(path=clone); Pkg.add("AMDGPU"); JACC.set_backend("amdgpu")`. Pre-adding AMDGPU as a hard dep made JACC record `placement amdgpu = "deps"`; the preferences testset asserts `"weakdeps"` and failed 2 of 46 (test/backend/preferences.jl:16, :59). Because the backend testsets execute eagerly during `include("JACCTests.jl")`, the failure threw `LoadError` at runtests.jl:10 and `retest()` never ran — **all 25 compute testsets were silently skipped**. Run 1's "71 passed" is backend infrastructure only (device array constructors, streams). An early bookkeeping failure masks the entire compute suite; worth knowing before reading any JACC AMDGPU CI/test log. Log: `jacc-suite-amdgpu.txt`.

**Run 2 (canonical, matches ci-gpu-AMD.yaml) — 249/249 passed, exit 0.** Setup exactly as CI (`julia --project` in the clone; `Pkg.instantiate()`; `JACC.set_backend("AMDGPU")` → `placement = "weakdeps"`; `Pkg.test()` in a fresh process). Per-testset: TestBackend 2, array_types 20, stream 5, preferences 46, array 21, transfer! 12, VectorAddLambda 1, AXPY 1, AtomicCounter 1, reduce 15, reduce-ND 20, LaunchSpec 14, shared 26, JACC.BLAS 9, Add-2D 1, Add-2D imbalanced 4, Add-3D 1, Add-ND 4, do 10, macro 6, UnitRange 10, StepRange 14, CG 1, LBM 1, rand-Float32 2, rand-Float64 2. Log: `jacc-suite-amdgpu-canonical.txt`. Recurring benign artifact in both runs: `Resource leak detected by SharedSignalPool, 8 Signals leaked` at process exit.

**Why the suite passes while three vendor libraries are broken:** JACC's portable API never reaches them. `JACC.BLAS` is JACC's own `parallel_for` kernels (src/blas.jl — `_axpy`, `_dot`, …), not rocBLAS. The rand testsets call device-side `rand()` *inside* kernels (unittests.jl:1228 — GPUCompiler/GPUArrays native device RNG), not the rocRAND host API. Anything that did call those libraries — `AMDGPU.rand`, `mul!` on ROCArrays — fails as documented in §1.

### JACC.Multi (relevant to JuliaGPU/JACC.jl#381)

The suite cannot produce an AMDGPU Multi result on any hardware: the entire `Multi` testset is inside `if JACC.backend != "amdgpu" && JACC.backend != "metal"` (test/unittests.jl:1054 at commit `0dbf990a`). There is no runtime skip and no recorded reason; the exclusion is invisible in test output. The #381 thread context: maintainer describes a communication issue on AMD GPUs, tests "skipped … and marked as not supported"; ffrancesco94 reports Multi passing on a full MI250X node with Float32. A consumer-iGPU data point beside that discrete-GPU pass requires removing the source-level gate first.

### Float64 (tracked separately by prior decision)

AMDGPU has no `default_float` override (only Metal does), so `@changeprecision` runs the suite in **Float64** on this backend. Suite-level outcome: **the entire 249-test canonical run is a Float64 exercise on gfx1150, and it passed** — including reduce, reduce-ND, CG, LBM, and the explicit rand-Float64 testset. The Metal precedent (Float64 as the breaking axis) did not reproduce here: gfx1150's FP64 is slow by hardware ratio but functionally correct at suite tolerances. The pre-registered hardware-class failure bucket for Float64 stays **empty**. No Float64-specific throughput was measured; correctness only.

## 3. What is *not* claimed

- No support-matrix row, no rung, nothing promoted. ROCm on gfx1150 remains absent from the table by design (`docs/support-matrix.md` §ROCm); this document *updates the factual basis* of that paragraph — the override sentence is now known to be wrong for this stack — but does not change its conclusion.
- The `strings`-based target inventory is a heuristic (fat-binary target names may live outside the `.so`, e.g. rocBLAS's Tensile `.dat` files). Where it mattered, it was corroborated by observed behavior (rocRAND crash, rocBLAS error text).
- Nothing here is a JACC.jl defect claim. Routing per the pre-registered discipline: rocRAND/rocBLAS/rocSPARSE gaps → ROCm/Ubuntu packaging findings; the rocSPARSE/version-query behavior overlaps JuliaGPU/AMDGPU.jl#920; the Multi gate is a JACC test-suite observation, not a bug report.
- Vendor-stack ≠ distro-package: everything here is about **Ubuntu's** ROCm packaging (7.1.0/7.1.1, Debian layout). AMD's vendor repo for 24.04 may behave differently on every axis.

## 4. Version pins

| component | pinned value |
|---|---|
| Ubuntu | 26.04 LTS (resolute) |
| kernel | 7.0.0-28-generic |
| `rocm` metapackage | 7.1.0-0ubuntu6 (→ rocm-dev, rocm-tests) |
| rocminfo / rocm-smi | 7.1.1-0ubuntu1 |
| libhsa-runtime64-1 / libhsakmt1 | 7.1.0+dfsg-0ubuntu9 |
| libamdhip64-7 | 7.1.0-0ubuntu2 (HIP reports 7.1.52801) |
| hipcc | 7.1.1+dfsg-0ubuntu1 (clang 21.1.8) |
| librocrand1 / librocsparse / librocfft etc. | 7.1.1-0ubuntu1 |
| Julia | 1.12.6 (juliaup, user-local) |
| AMDGPU.jl | v2.7.1 |
| JACC.jl | commit `0dbf990a7a5547518e1c055e70294023ad277513` (main, merge of #398) |
| rocminfo LLVM target | `amdgcn-amd-amdhsa--gfx1150` |
| KFD `gfx_target_version` | 110500 (read-only, pre-install) |
| `HSA_OVERRIDE_GFX_VERSION` | **unset — natively enumerated** (11.5.0 identical; 11.0.0 masquerades as gfx1100) |
| Mesa / Vulkan | unchanged by install: mesa 26.0.3-1ubuntu1, libvulkan1 1.4.341.0-1, RADV STRIX1 verified post-install |

## 5. Provenance notes

**All captures cited below are committed at
`docs/hardware/rocm-gfx1150-jacc-amdgpu.evidence/`, with a sha256 manifest.**

> **This document was committed at `e8ad1db` before its evidence was.** The
> captures then lived at `~/outputs/rocm-gfx1150-probes-20260804/` — matched by
> `.gitignore:34` — and at loose `~/*.txt` files outside the repository. A
> committed document citing uncommitted evidence is *citable but not findable*,
> the same condition `#32` fixed for the XDNA evidence and `composed-error.md`
> was written to fix. Corrected by bringing all 23 files under version control;
> the `~` originals are left in place and are now redundant copies.

Routing (posted 2026-08-04, after review): the harness defect → JuliaGPU/JACC.jl#403; the characterization (insulation framing) → JuliaGPU/JACC.jl#404; the Multi mechanism + single-device question → comment on JuliaGPU/JACC.jl#381. The Ubuntu packaging gaps (rocRAND/rocBLAS/rocSPARSE) are not yet reported to Ubuntu.

- Package snapshots: `pkg-state-preROCm-20260804.txt` (1751→ pre; `dpkg -l` shape, same as Gate A+B), `pkg-state-postROCm-20260804.txt`, `graphics-baseline-{pre,post}ROCm-20260804.txt`, `apt-manual-preROCm-20260804.txt`.
- **No post-GateAB snapshot was ever taken** (2026-07-30 produced only `pre` files). `pkg-state-preROCm-20260804.txt` therefore doubles as the post-GateAB record; the graphics subset was byte-identical between the two dates, so nothing in the gap moved the promoted Vulkan row's provenance. **That gap is in the earlier record and committing these captures does not close it.**
- Raw captures: rocminfo/rocm-smi at three override settings, apt dry-run and both install logs, the empirical rollback proof, AMDGPU.jl logs, and both JACC suite logs.
- The stage-1 apt log was reconstructed from the session transcript (an interactive line-wrap broke the original `tee`); all other logs are direct captures.

## 6. Reproducing this

```sh
# read-only, pre-install
grep -H gfx_target_version /sys/class/kfd/kfd/topology/nodes/*/properties   # 110500
dpkg -l > pkg-state-pre.txt; apt-mark showmanual > manual-pre.txt
apt-get -s install rocm   # gate: no action of any kind on the graphics subset

# staged install
sudo apt install rocminfo rocm-smi libhsa-runtime64-1
rocminfo                                   # gfx1150, no override
HSA_OVERRIDE_GFX_VERSION=11.0.0 rocminfo   # gfx1100 masquerade
HSA_OVERRIDE_GFX_VERSION=11.5.0 rocminfo   # identical to unset
sudo apt remove rocminfo rocm-smi libhsa-runtime64-1 && sudo apt autoremove  # rollback proof
sudo apt install rocminfo rocm-smi libhsa-runtime64-1
sudo apt install rocm

# Julia layer
curl -fsSL https://install.julialang.org | sh -s -- --yes   # 1.12.6
julia --project=amdgpu-env -e 'using Pkg; Pkg.add("AMDGPU")'
julia --project=amdgpu-env -e 'using AMDGPU; AMDGPU.versioninfo(); println(AMDGPU.functional())'

# JACC — canonical invocation (matches .github/workflows/ci-gpu-AMD.yaml)
git clone https://github.com/JuliaGPU/JACC.jl && cd JACC.jl   # 0dbf990a
julia --project -e 'using Pkg; Pkg.instantiate()'
julia --project -e 'using JACC; JACC.set_backend("AMDGPU")'   # placement must come out "weakdeps"
julia --project -e 'using Pkg; Pkg.test()'                    # fresh process; preference read at precompile
# caution: an external env with Pkg.add("AMDGPU") before set_backend records placement
# "deps", fails 2 preference tests, and ABORTS the run before any compute test executes
```

## 7. Verification status

| Finding | Status |
|---|---|
| gfx1150 native enumeration, three override settings | **Verified by me on this machine** |
| AMDGPU.jl functional, elementwise F32/F64 kernels | **Verified by me on this machine** |
| rocRAND segfault, rocBLAS clean error, rocSPARSE query crash | **Verified by me on this machine** |
| Missing gfx1150 targets in Debian math libs | Verified via `strings` heuristic + corroborating behavior |
| Graphics stack unchanged; rollback | **Verified by me on this machine** (empirical remove/reinstall) |
| JACC suite 249/249 pass (canonical CI invocation, Float64) | **Verified by me on this machine** |
| Run-1 abort: early preferences failure skips all compute testsets | **Verified by me on this machine** (both logs retained) |
| JACC.BLAS / rand pass because they bypass vendor libs | **Verified by source read** (src/blas.jl, unittests.jl:1228) + observed behavior |
| JACC.Multi source-level gate | **Verified by source read** at commit `0dbf990a` |
