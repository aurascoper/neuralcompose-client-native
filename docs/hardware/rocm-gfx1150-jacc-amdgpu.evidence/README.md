# Raw captures for `rocm-gfx1150-jacc-amdgpu.md`

Every capture that document cites, brought under version control on 2026-08-04.

**Why this directory exists.** The audit was committed at `e8ad1db` while its
evidence lived at `~/outputs/rocm-gfx1150-probes-20260804/` — a path matched by
`.gitignore:34` (`outputs/`, added at `64f2332` for agent scratch) — and at
loose `~/*.txt` files outside the repository entirely. That is a committed
document citing uncommitted evidence: **citable but not findable**, which is the
exact condition `#32` fixed for the XDNA evidence and `composed-error.md` was
written to fix one level up. `docs/hardware/README.md` says version control was
"the difference between a withdrawal and a quiet edit"; this directory gives the
ROCm record that property, matching `xdna2-bert-encoder-feasibility.ops.txt` and
`composed-error.stdout.txt`.

**Provenance caveats, carried from §5 of the parent document:**

- `apt-stage1-install.txt` was **reconstructed from the session transcript** — an
  interactive line-wrap broke the original `tee`. Every other file is a direct
  capture.
- `apt-stage2-install.txt` and `rollback-proof.txt` were written by root during
  `sudo apt` runs; copied here with mode `644`, contents unmodified.
- `pkg-state-preROCm-20260804.txt` doubles as the post-Gate-A+B record, because
  no post-Gate-A+B snapshot was ever taken (2026-07-30 produced only `pre`
  files). That is a real gap in the earlier record, not something this capture
  closes.

**These are captures, not a check.** Nothing re-runs them; §6 of the parent
document has the reproduction commands. Same standing as `tools/onnx/`.

## Manifest

```
a0d71c6201668da0d535649b226c81f41c3d731a229c8f390fd207d21971da86  amdgpu-jl-add.txt
9527ebee48ebc9cc10a34c349881bf459b1414aef022e060f8f2ad1776dd8ae6  amdgpu-jl-float64.txt
f50d4dbcea15d15548a9efede91f61b163006b93cc2e4ed6812e0a5a37fe671a  amdgpu-jl-rocblas.txt
59db21bf294937de4f8f729dbaaa5460ec11186f0877c3f030db0861bd7616aa  amdgpu-jl-rocrand-f32.txt
9f112c71038eec6d0888defb9c931e6a8d8ca2326e96d4d94147f424ac37c8eb  amdgpu-jl-smoke.txt
66dcfcb0b6649801906d1433954d0ce0513dd664f59224b257aa4cfd41027538  amdgpu-jl-versioninfo.txt
8dcab51c3914eb0de25016aeb3302571eda33c77b43922ade835bef52125e33f  apt-dryrun-rocm.txt
0aa5cc45d5116b284d76d2cd3615dd8b8eedec6ed63d95457dfe5fb504ce6e97  apt-manual-preROCm-20260804.txt
52b3beeddfad87678f023551673f1136c136b87db8cd2c1d8d2b2ccea65b97c2  apt-stage1-install.txt
7774a3e0e5ecc3b1b64825520f402c95b86de8d0710c9e7bc27e0333baf19386  apt-stage2-install.txt
35a4635669babfc083b6ae8bce404d63438359682e11d59bcde1ad1c11e8ac7c  graphics-baseline-postROCm-20260804.txt
bc4b6b56661120a84e373e7947dcf720c52b913fea7d4e49f4dd08b452bc937f  graphics-baseline-preROCm-20260804.txt
4e5236404bbed688d9e1c7814431f7d5cc5414234d09fddb37cd07bea922afc4  jacc-suite-amdgpu-canonical.txt
86e34e2c1e7c75d058d8185c0affcb34198f17330add5f2979ffe760029d8c15  jacc-suite-amdgpu.txt
ca7e9c7fd815f00b674ecec18259fb277866ff07b699bb8dde3707bd77500455  pkg-state-postROCm-20260804.txt
d79095e1b528657182cd7ea4c15e68ae99f5ad4842067e18bc1c94522c62a7e7  pkg-state-preROCm-20260804.txt
60e81696e6ea5c87790623e89760c7ebde0db09b5a7232828e0e44c8fc502b13  rocminfo-11.0.0.txt
6d324df87b075307bac49febfee8a45b523d11ca9dcc5e677a6d4c530fa019b1  rocminfo-11.5.0.txt
6d324df87b075307bac49febfee8a45b523d11ca9dcc5e677a6d4c530fa019b1  rocminfo-unset.txt
3a37230968bda2d395a184174dcd0e07e425e2566eb287bede67d22411a9cf85  rocm-smi-11.0.0.txt
49e5975665037cef468249e4f7e349c25fc0a4b05467fe0fadb31bf2af1b8ef1  rocm-smi-11.5.0.txt
35d1b5ec2450a7dbff52272e7e541e964999ed1fea1bf3459888defe02acbfb3  rocm-smi-unset.txt
a031f088f422980fb66debafd494f8e6f8a4fd7dbbe175eef97bb1680cfcbb53  rollback-proof.txt
8cb191a7ace9313ed6fc29a87fa721df8fbc4ba981a0fff13e08381c27fd4074  repro-rocrand.cpp
892788db0fb50bc3e3206d51d380a7198a89957a3995a94a3cf0790cf068d05c  repro-rocblas.cpp
b54b8efcfa8ec3ea7deb0ca7777e709f5134e8f693aac60007163f610a17de90  repro-rocsparse.c
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  repro-rocrand-output.txt
5a674b3139cc0b0e2be6c86a852f589187dabae7e7da33dfd8a182300977486b  repro-rocblas-output.txt
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  repro-rocsparse-output.txt
```

Added 2026-08-04, same day, after the Launchpad decision: three Julia-free C
reproductions (compiled with the distro hipcc, exact commands in each file's
header comment) and their captured stdout/stderr. Two of the output files are
**empty by fact, not by accident** — their sha256 is the empty-file hash because
rocrand and rocsparse segfault (exit 139) before producing any output; the
rocblas run aborts (exit 134) after printing the TensileLibrary error. The full
nine-target lazy `.dat` list in `repro-rocblas-output.txt` is what corrected the
parent document's gfx942-only claim.
