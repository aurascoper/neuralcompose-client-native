# Audit — can a BERT-family embedding encoder run on XDNA 2 under Linux?

**Audit target:** `bge-small-en-v1.5` (BERT-family, 33.2M params, 384-dim, mean
pooling) on the XDNA 2 NPU of `ok-cyberdeck`, and what the toolchain requires.
**Host under audit:** GPD G1617-02, Ryzen AI 9 HX 370, Ubuntu 26.04, kernel
`7.0.0-28-generic`.
**Method:** read-only. Nothing installed, no module built or loaded, no group
membership changed. The one dependency-resolution check was `apt-get install -s`,
an unprivileged simulation that writes nothing.

**This document promotes nothing.** It lives in `docs/hardware/` for the reason
that directory's README gives: establishing what the silicon is, and what a
vendor toolchain demands of it, is **not** evidence that a backend works.
Nothing here advances a support-matrix row on ADR-002's ladder, and
`attained_support_status()` returns `None` for everything described below.
Nothing in this document has executed a model on the NPU.

**Supersedes** the acquisition conclusion of
`xrt-userspace-intree-amdxdna-audit.md` §4.4 — see §6.1. That document's other
findings stand and are cited here rather than repeated.

---

> **Document status:** final audit artifact. Retrieval date for all external
> citations: **2026-08-04**. Current vendor release at retrieval:
> **AMD Ryzen AI Software 1.8.0**, tag `v1.8.0` published `2026-07-23T04:37:16Z`
> (https://api.github.com/repos/amd/RyzenAI-SW/releases, re-fetched 2026-08-04;
> https://ryzenai.docs.amd.com/en/latest/ self-identifies as "Ryzen AI Software
> 1.8.0 documentation", retrieved 2026-08-04). **No 1.8.1 and no 1.9 exists.**
> Ubuntu's Debian-packaged XRT is a *different product with a different version*:
> `xrt 1:2.21.75+dfsg-4` in Ubuntu 26.04 "resolute". These two are never
> interchanged in this document; where they are compared, they are named.

---

## §0 — Bottom line

**No — not today, and not from the Ubuntu archive alone**, because nothing in any
distro archive can turn an ONNX graph into NPU code: Ubuntu 26.04 ships the XRT
*runtime* but its ONNX Runtime package contains only the DNNL execution provider,
with no Vitis AI EP (https://packages.ubuntu.com/resolute/amd64/libonnxruntime-providers/filelist,
xrt/onnxruntime as packaged in Ubuntu 26.04 resolute, re-verified 2026-08-04 —
the package contains exactly `libonnxruntime_providers_dnnl.so` and
`libonnxruntime_providers_shared.so`).

**There are two present-tense blockers, not one**, and they are independent —
fixing either leaves the other standing.

**Blocker 1 — the compiler.** The VAIML / Vitis AI EP exists only inside AMD's
EULA-gated `ryzen_ai-1.8.0.tgz`, whose documented Linux path targets Ubuntu
24.04 / Python 3.12 / `libboost-filesystem1.74.0`, and whose companion
`xrt_plugin…amdxdna.deb` **displaces the in-tree amdxdna driver this machine is
currently running** (§6.2). On the vendor path this is the binding constraint.

**Blocker 2 — device access, which has nothing to do with AMD.** `/dev/accel/accel0`
is reachable today *only* through a udev `uaccess` ACL granted to the active seat
session. A systemd **system** service has no seat session, is not in `render`, and
gets `EACCES` at `open(2)` — today, as configured, with no vendor software
involved (§8.5). For the long-running memory server this audit was commissioned
to evaluate, that blocker arrives before any compiler question does.

**The blocker is "single" only under a vendor-path premise this audit itself
refuted.** §5.1 documents a third party compiling and running a 384-dim
`all-MiniLM-L6-v2` forward pass on XDNA 2 under Linux via hand-written `aie2p`
kernels, MLIR-AIE/IRON and raw `pyxrt` dispatch, **bypassing the Vitis AI EP
entirely** — and Ubuntu's own archive ships `aiebu-asm` and `xrt-runner` (§6.1).
That path is not turnkey here (it wants XRT ≥ 2.23.0; resolute has 2.21.75), but
it means "no compiler exists outside AMD's EULA" is false as stated.

**On the runtime half.** Ubuntu 26.04's archive supplies the XRT runtime at zero
cost and zero driver risk, and this **supersedes the prior audit's "blocked on
acquisition" conclusion** (§6.1). The honest word is **resolvable, not solved**:
no vendor, maintainer, or field report covers the pairing of Ubuntu's
`2.21.75+dfsg` shim with this in-tree `amdxdna`, so the prior audit's ABI finding
remains a load-bearing *assumption* for the packaged build (§11 G9, §12).

And even with a compiler in hand, whether a BERT encoder compiles **to the NPU**
rather than silently partitioning to CPU is **UNVERIFIED in AMD's own 1.8.0
documentation, which contradicts itself on exactly that point** (§3.2).

---

## §1 — Scope, method, and what "verified" means here

**Question under audit.** Can `bge-small-en-v1.5` (a 12-layer, 384-dim BERT
encoder) be made to produce embedding vectors on the NPU of `ok-cyberdeck`
(Ryzen AI 9 HX 370, Strix Point, XDNA2) running Ubuntu 26.04 with the in-tree
`amdxdna` driver — and at what cost to the numerics, the operational model, and
the driver currently bound to the device.

**Method.** Six research questions were investigated independently, each then
attacked by a separate skeptic whose refutations are applied throughout and
recorded in §9. Nothing was run on the host during this synthesis. The host
facts in §2 were produced by read-only commands before this document was
written and are treated as ground truth. During synthesis I independently
re-fetched four load-bearing external sources (the RyzenAI-SW releases API, the
1.8.0 operator table, the Ubuntu `libonnxruntime-providers` file list, and the
GitHub API record for `jyatesdotdev/npu-embeddings`); those are marked
"re-verified 2026-08-04".

**Status classes.** Every finding in this document carries exactly one:

| Class | Meaning |
|---|---|
| **VERIFIED-HERE** | Measured on this machine by a read-only command (§2). |
| **VERIFIED-DOC** | ≥2 independent sources, or one primary vendor document, cited by URL + product version + retrieval date. |
| **SINGLE-SOURCE** | Exactly one source. Tagged explicitly, and enumerated again in §10. |
| **UNVERIFIED** | Could not be established either way. §11 names what would close it. |
| **ABSENT** | Searched for; it does not exist. The searches are given so the absence is auditable. An absence is a result. |

**Where the body departs from this contract, and why that is recorded rather
than tidied away.** The critic pass (§13) found the body using five labels that
are not in the table above: `DOCUMENTED-BUT-UNMEASURED` and `ANALYST INFERENCE`
(§5.3), `UNVERIFIED-BUT-FINDABLE` (§4.3), `Derived arithmetic` (§4.3), and
`FROM MEMORY, UNVERIFIED` (§6.2, §6.3). Read them as refinements of the five:
the first two and the fourth are species of **UNVERIFIED** that say *why* it is
unverified, and `FROM MEMORY, UNVERIFIED` is the honest label the evidence rules
require for a claim carried without a source. They are left in place because
each is more informative than the class it belongs to, and because silently
rewriting them to fit the table would be the tidier and less honest option. The
contract is therefore: **every finding carries a status, and the five above are
the ones that count for §10 and §11.**

**Two provenance rules applied throughout.**

1. *RyzenAI-SW ≠ Ubuntu XRT.* AMD's stack for 1.8.0 is XRT `202620.2.25.37` plus
   `xrt_plugin.2.25.260102.56…amdxdna.deb`
   (https://ryzenai.docs.amd.com/en/latest/linux.html, RyzenAI 1.8.0, retrieved
   2026-08-04). Ubuntu's is `1:2.21.75+dfsg-4`, a DFSG-repacked build of upstream
   Xilinx/XRT tag `2.21.75` (https://sources.debian.org/src/xrt/,
   https://github.com/Xilinx/XRT/releases, retrieved 2026-08-04). Same upstream
   version *number* as the XRT that shipped with RyzenAI 1.7.x; **not** the same
   product as the 1.8.0 stack, and **not byte-identical** to AMD's tree.
2. *Read-only.* This document names no command that installs, builds, loads a
   module, or changes group membership. Where such a change is the only thing
   that would work (§8), it is **described together with what it does and what it
   risks**, and explicitly not recommended for execution.

---

## §2 — VERIFIED ON THIS MACHINE

Every row below is VERIFIED-HERE. Nothing read from the internet appears in this
table. The command column is the read-only command that produced the fact.

| Fact | Value | Produced by |
|---|---|---|
| Hostname / model | `ok-cyberdeck`, GPD G1617-02 | `hostnamectl` |
| OS | Ubuntu 26.04 LTS "Resolute Raccoon", `VERSION_ID=26.04`, codename `resolute` | `cat /etc/os-release` |
| Kernel | `7.0.0-28-generic #28-Ubuntu SMP PREEMPT_DYNAMIC` | `uname -a` |
| glibc | 2.43 (Ubuntu GLIBC 2.43-2ubuntu2.3) | `ldd --version` |
| gcc | 15.2.0 | `gcc --version` |
| python3 | 3.14.4 | `python3 --version` |
| systemd | 259 (`259.5-0ubuntu3`) | `systemctl --version` |
| RAM | `MemTotal` = **22.58 GiB** | `awk '/MemTotal/' /proc/meminfo` |
| **boost installed** | `libboost-iostreams1.90.0`, `libboost-locale1.90.0`, `libboost-thread1.90.0` — and **no `libboost-filesystem` at any version**; `ldconfig` resolves no `libboost_filesystem` | `dpkg -l \| awk '$1=="ii" && $2 ~ /boost/'`; `ldconfig -p \| grep boost_filesystem` |
| CPU | Ryzen AI 9 HX 370 (Strix Point) | `lscpu` |
| NPU device | PCI `0000:c5:00.1`, `[1022:17f0]` rev `0x10`, "Strix/Krackan/Strix Halo Neural Processing Unit" | `lspci -nnk -s c5:00.1` |
| Driver bound | `/sys/bus/pci/drivers/amdxdna` | `readlink /sys/bus/pci/devices/0000:c5:00.1/driver` |
| Module provenance | `/lib/modules/7.0.0-28-generic/kernel/drivers/accel/amdxdna/amdxdna.ko.zst`, `intree: Y`, `srcversion E4FC5AFFF53D21BC6AFDAE3` | `modinfo amdxdna` |
| Module loaded | yes, refcount 0 | `lsmod \| grep amdxdna` |
| sysfs `vbnv` | `RyzenAI-npu4` | `cat /sys/…/0000:c5:00.1/vbnv` |
| sysfs `fw_version` | `1.1.2.64` | `cat /sys/…/0000:c5:00.1/fw_version` |
| Firmware on disk | `/lib/firmware/amdnpu/17f0_10/npu.sbin.{1.0.0.63,1.1.2.64}.zst`; `17f0_11/{1.0.0.166,1.1.2.65}`; `1502_00/{1.5.2.380,1.5.5.391}` | `ls /lib/firmware/amdnpu/*/` |
| Device node | `/dev/accel/accel0`, `crw-rw----+ root:render 261,0` | `ls -l /dev/accel/accel0` |
| The `+` | POSIX ACL `user:aurascoper:rw-` | `getfacl /dev/accel/accel0` |
| ACL source | `SUBSYSTEM=="accel", KERNEL=="accel*", TAG+="uaccess"` | `cat /usr/lib/udev/rules.d/70-uaccess.rules` |
| Base perms source | `SUBSYSTEM=="accel", GROUP="render", MODE="0660"` | `grep accel /usr/lib/udev/rules.d/50-udev-default.rules` |
| User groups | `aurascoper adm cdrom sudo dip plugdev users lpadmin lxd` — **not** `render`, **not** `video` | `id` |
| Access mechanism | the uaccess ACL is the **only** thing granting device access today | composition of the three rows above |
| logind | session 2, seat0, tty2, `class=user` (interactive seat) | `loginctl list-sessions`, `loginctl show-session 2` |
| sudo | in group `sudo`; passwordless sudo **not** available | `id`; `sudo -n true` (non-zero) |
| systemd | 259 (259.5-0ubuntu3) | `systemctl --version` |
| AMD tooling present | **none**: no `xrt-smi`, `xbutil`, `xclbinutil`, `xrt_server`, `aie-opt`, `vaip`; no `/opt/xilinx`, `/opt/amd`, `/usr/local/xrt` | `command -v …`; `ls` |
| AMD packages installed | **none** matching `xrt\|xdna\|vitis\|vaip\|ryzen` | `dpkg -l` |
| AMD `.deb` on filesystem | **none anywhere** | filesystem search |
| Python ORT | `ModuleNotFoundError: No module named 'onnxruntime'` | `python3 -c "import onnxruntime"` |
| **Archive: XRT** | source package `xrt`, `1:2.21.75+dfsg-4`, resolute/universe, amd64: `libxrt2`, `libxrt-npu2`, `libxrt-utils`, `libxrt-utils-npu`, `libxrt-dev`, `python3-xrt`, `libxrt-alveo2`, `libxrt-utils-alveo`, `xrt-xocl-dkms` | `apt-cache show`, `apt-cache policy` |
| **Archive: resolution** | `apt-get install -s libxrt-utils-npu libxrt-npu2` → **6 new packages** (`libboost-filesystem1.90.0`, `libboost-program-options1.90.0`, `libxrt2`, `libxrt-npu2`, `libxrt-utils`, `libxrt-utils-npu`), all from Ubuntu:26.04/resolute, 0 removed, 0 held; grep for `dkms\|xocl\|amdxdna` in the resolved set: **NO MATCH** | `apt-get install -s` (simulation only; nothing installed, no root) |
| **Archive: python3-xrt** | resolves, pulls only `libxrt2`; host python 3.14.4 satisfies `>=3.14~, <<3.15` | `apt-get install -s` |
| **Archive: ONNX Runtime** | `libonnxruntime1.23`, `libonnxruntime-dev`, `libonnxruntime-providers`, `python3-onnxruntime`, `onnxruntime-tools`, `python3-onnx`, `libonnx1t64` present | `apt-cache show` |

**Explicitly NOT established on this host** (do not assert either way): what
binaries `libxrt-utils-npu` ships (`apt-file` unavailable here — resolved from the
archive's published file lists in §6 instead); whether Ubuntu's XRT 2.21.75 speaks
the same ioctl ABI as this in-tree `amdxdna`; whether Ubuntu's ONNX Runtime has a
Vitis AI EP (**answered NO in §6, from the archive's file list, not from this
host**).

---

## §3 — Operator support and opset (Q1)

### §3.1 The operator table, verbatim

Re-verified by direct fetch of https://ryzenai.docs.amd.com/en/latest/ops_support.html
(page self-identifies as **Ryzen AI Software 1.8.0**, retrieved 2026-08-04; source
of truth https://raw.githubusercontent.com/amd/ryzen-ai-documentation/main/docs/ops_support.rst).
Columns are `Ops | BF16 | A16W8 | A8W8 | XINT8`.

| Op | BF16 | A16W8 | A8W8 | XINT8 |
|---|---|---|---|---|
| MatMul | Y | Y | Y | Y |
| Add | Y | Y | Y | Y |
| Transpose | Y | Y | Y | Y |
| Reshape | Y | Y | Y | Y |
| Slice | Y | Y | Y | Y |
| Gemm | Y | Y | Y | Y |
| Sqrt / Sub / Div | Y | Y | Y | Y |
| Erf | Y | — | — | Y |
| **Gather** | **Y** | — | — | — |
| **LayerNormalization** | — | — | — | **Y** |
| **Softmax** | — | Y | Y | Y |
| **Gelu** | — | Y | Y | Y |

**VERIFIED-DOC.** The four rows in bold are the ones a BERT encoder needs.
**No single quantization mode has `Y` for all four.** BF16 — the mode AMD
prescribes for transformers — lacks LayerNormalization, Softmax and Gelu.
XINT8 has all three but lacks Gather, the token-embedding lookup.

**VERIFIED-DOC (absence within a document): no fused transformer operator is
listed anywhere.** `Attention`, `MultiHeadAttention`, `FastGelu`, `BiasGelu`,
`SkipLayerNormalization`, `EmbedLayerNormalization`, `QAttention` appear in no
column. The only attention-family op in the whole document is
`GroupQueryAttention`, and it sits in a separate "LLM Operator support" list
scoped to the ONNX Runtime GenAI decoder flow (causal, not bidirectional).
Practical consequence: an ONNX graph produced by
`onnxruntime.transformers.optimizer` — which fuses BERT into
`com.microsoft::Attention` + `SkipLayerNormalization` — **has no documented
support path**. Only a plain `torch.onnx.export` graph is covered.
(ops_support.rst lines 9–238, RyzenAI 1.8.0, retrieved 2026-08-04.)

### §3.2 The central unresolved contradiction — DOWNGRADED to UNVERIFIED

The research phase reported "LayerNorm/Softmax/Gelu have no BF16 → a BERT encoder
falls back to CPU" as a **BLOCKER**. **That inference is refuted and does not
survive** (see §9.1). What survives:

- **VERIFIED-DOC:** the table reads as shown above.
- **VERIFIED-DOC:** AMD explicitly claims the opposite capability. The 1.8.0
  Linux page states the release "supports STX and KRK platforms" and that users
  "can now compile and run AI models using the following formats: CNN models in
  INT8; CNN models in BF16; **NLP models (e.g., BERT, encoder-based) in BF16**;
  LLMs (NPU-only flow)". The Model Compatibility Table checkmarks "NLP BF16" for
  STX/KRK and not for PHX/HPT, and AMD's own Linux detection snippet maps PCI
  `1022:17f0` → STX/KRK — **this host's exact device ID**.
  (https://raw.githubusercontent.com/amd/ryzen-ai-documentation/main/docs/linux.rst
  lines 6–14, 199–214; https://ryzenai.docs.amd.com/en/latest/relnotes.html;
  RyzenAI 1.8.0, retrieved 2026-08-04.)
- **VERIFIED-DOC:** AMD ships BERT-family encoder examples through the BF16/VAIML
  path — `Transformer-examples/DistilBERT_text_classification_bf16` and
  `LLM-examples/RAG-OGA/custom_embedding/export_bge_onnx.py`
  (`BAAI/bge-large-en-v1.5`, `torch.onnx.export`, `opset_version=17`, static
  `(1,512)`, `VitisAIExecutionProvider` + `vaiml_config.json` with
  `vaip-pass_vaiml_partition`). Both feed the **unquantized fp32** ONNX to the
  VAIML compiler; the BF16 conversion is done by the compiler.
  (https://raw.githubusercontent.com/amd/RyzenAI-SW/main/LLM-examples/RAG-OGA/custom_embedding/,
  .../Transformer-examples/DistilBERT_text_classification_bf16/, RyzenAI-SW
  v1.8.0, retrieved 2026-08-04.)
- **VERIFIED-DOC:** AMD's own 1.8.0 Known Issues carry a **BF16 Models**
  subsection whose entry concerns `openai-whisper-medium-encoder` — a transformer
  encoder built from LayerNorm + Softmax + GELU, shipped as a working BF16 NPU
  model — and 1.8.0 adds "New embedding model support: embeddinggemma-300m".
  (https://ryzenai.docs.amd.com/en/latest/relnotes.html, RyzenAI 1.8.0, retrieved
  2026-08-04.) **If a blank BF16 cell were a hard per-op gate, neither could
  exist.**
- **UNVERIFIED — this is the central gap of the entire audit.** The document
  defines only what `Y` means ("broad coverage for that operator and for that
  specific quantization type for CNN and NLP models" — quoted verbatim,
  re-verified 2026-08-04). **It never defines what a blank cell means.** Either
  (i) the BF16 column is incomplete/stale with respect to the VAIML compiler flow,
  which fuses whole subgraphs rather than gating op-by-op, or (ii) those nodes are
  silently partitioned to CPU inside AMD's own shipped examples. **Documentation
  cannot resolve this.** Closing action in §11.

### §3.3 Opset

- **VERIFIED-DOC:** opset **17 is recommended, not required**: "Models with ONNX
  opset 17 are recommended. If your model uses a different opset version, consider
  converting it using the ONNX Version Converter"
  (https://raw.githubusercontent.com/amd/ryzen-ai-documentation/main/docs/modelrun.rst
  line 14 = https://ryzenai.docs.amd.com/en/latest/modelrun.html, RyzenAI 1.8.0,
  retrieved 2026-08-04). AMD's BGE and DistilBERT export scripts both hard-code
  `opset_version=17`.
- **ABSENT — no maximum opset is published by anybody.** A grep for `opset` across
  all 18 `.rst` files of the current docs repo returns exactly 7 hits: the opset-17
  recommendation (modelrun.rst:14, getstartex.rst:107) and historical release-note
  entries (relnotes.rst:764/849/945, "Resize OP in ONNX opset 10 or lower is not
  supported by VOE" — a *floor*; relnotes.rst:913 "Supports ONNX Opset version 18"
  from v0.7, long superseded). Web searches run 2026-08-04: *"Ryzen AI Vitis AI EP
  maximum ONNX opset supported ceiling opset limit 21 22 23"*; *"AMD Quark ONNX
  quantizer supported opset version maximum requirement 2026"*; *"'Vitis AI'
  execution provider ONNX 'opset 17' OR 'opset 18' OR 'opset version' requirement
  maximum"*. **No ceiling exists in any AMD document.**
- **VERIFIED-DOC, and AMD contradicts itself:** AMD Quark 0.12.post1 states
  "Models with opset 21 or higher are recommended" alongside "Models must use
  opset 10 or higher to be quantized"
  (https://quark.docs.amd.com/latest/onnx/optional_utilities.html, AMD Quark
  0.12.post1, retrieved 2026-08-04). So AMD's quantizer docs recommend ≥21 while
  the RyzenAI docs recommend 17. Opset 17 is the only value backed by the RyzenAI
  docs *and* AMD's own encoder examples; it is not the only vendor-backed value.

### §3.4 What happens when an op is unsupported — the honest answer

**VERIFIED-DOC.** Partitioning is automatic and AMD frames it as a feature,
verbatim: *"The ONNX graph is automatically partitioned into multiple subgraphs by
the Vitis AI Execution Provider (EP). During deployment, the subgraph(s)
containing operators supported by the NPU are executed on the NPU. The remaining
subgraph(s) are executed on the CPU. This graph partitioning and deployment
technique across CPU and NPU is fully automated by the VAI EP and is totally
transparent to the end-user."*
(https://ryzenai.docs.amd.com/en/latest/modelrun.html, modelrun.rst line 58,
RyzenAI 1.8.0, retrieved 2026-08-04.)

The EP raises **no error, no exception, and no warning** for unsupported ops.
What you *do* get at default settings — this is the corrected form of an
overreach, see §9.2 — is one line from ONNX Runtime core:

> `[W:onnxruntime:, session_state.cc VerifyEachNodeIsAssignedToAnEp] Some nodes were not assigned to the preferred execution providers which may or may not have an negative impact on performance.`

That line is **WARNING severity, which is at the default threshold**, so you are
told *something*. It is operationally useless: it names no ops, gives no counts,
and **fires in perfectly healthy sessions too**. **Total CPU fallback and normal
operation are indistinguishable at default settings.**

The informative signal is suppressed. The `[Vitis AI EP] No. of Operators : NPU
398 / VITIS_EP_CPU 2` banner carries the `[I:onnxruntime:, stat.cpp:198]` prefix —
**INFO severity**, below the documented default of `Warning (Default) … 2`
(modelrun.rst lines 146–170; inst.rst lines 110–160, RyzenAI 1.8.0, retrieved
2026-08-04). *Caveat, applied from §9.7: `inst.rst` is the **Windows** install
page (it uses `%RYZEN_AI_INSTALLATION_PATH%`, `findstr`, `set`). The banner text
is confirmed appearing on Linux via RyzenAI-SW issue #318; its INFO-severity
suppression on Linux is inferred from the Windows doc plus ORT's
platform-independent severity table, not from a Linux-specific AMD document.*

Getting the truth requires **three separate opt-ins, none of them default**:
`enable_cache_file_io_in_mem=0` (documented default `1`), the environment
variable `XLNX_ONNX_EP_REPORT_FILE=<name>`, and a lowered
`SessionOptions.log_severity_level` for the console banner (modelrun.rst lines
107–113, 516–571, RyzenAI 1.8.0, retrieved 2026-08-04). Since RyzenAI 1.5 the
`vitisai_ep_report.json` is **no longer generated automatically**
(https://ryzenai.docs.amd.com/en/latest/relnotes.html, 1.5 entry, retrieved
2026-08-04).

**SINGLE-SOURCE, real-world, on Linux.** A user on Ubuntu 22.04 with
`ryzen_ai-1.6.1` changed one Resize layer from `mode=linear` to `mode=nearest` in
a CV model. Before: `CPU 8 NPU 2356 VITIS_EP_CPU 13 / Subgraphs: NPU 2`. After:
`CPU 713 VITIS_EP_CPU 1664` — **zero NPU operators**. The only diagnostic was the
generic ORT warning above. AI Analyzer produced no logs anywhere. AMD's first
response came ~5 weeks later asking for details; still open.
(https://github.com/amd/RyzenAI-SW/issues/318, opened 2025-12-25, state open,
retrieved 2026-08-04.)

**SINGLE-SOURCE, disqualified for this host but instructive.** BERT-base-uncased
at opset 17, in **both** INT8 and BF16, hangs indefinitely at
`InferenceSession` creation with no log output and no timeout, while ResNet50
compiles fine (https://github.com/amd/RyzenAI-SW/issues/312 and
https://github.com/microsoft/onnxruntime/issues/26755, both opened 2025-12-09,
both open, retrieved 2026-08-04). **Disqualifier:** the reporter corrected their
hardware in-thread from "Ryzen AI 9 HX 370" to "AMD Ryzen 7 8700G" — Phoenix
(XDNA1), which AMD's Model Compatibility Table gives **no** checkmark for BF16
NLP. It is *not* evidence against this STX host. It *is* evidence that the
failure mode on an unsupported configuration is an **unkillable silent hang, not
a clean error**. (The ORT twin was auto-staled 2026-01-08 and **de-staled and
assigned to a maintainer 2026-01-12** — correcting a nit, §9.8.)

### §3.5 The cost of partition boundaries

**ABSENT.** Nobody has published a quantification of partition-boundary cost for a
transformer encoder on Ryzen AI: no partition counts, no host↔NPU round-trip
latency, no DDR transfer cost per boundary, no break-even partition count.
Searches run 2026-08-04: *"Ryzen AI NPU BERT embedding model bge benchmark latency
vs CPU milliseconds 2026"*; *"VitisAIExecutionProvider BERT encoder subgraph CPU
fallback partition count benchmark NPU slower"*; *"Ryzen AI VAIML vaiml_partition
number of subgraphs transformer encoder round-trip overhead DDR"*; *"Ryzen AI NPU
BERT encoder benchmark latency subgraph partition CPU fallback measurement"*.
Sites covered: `ryzenai.docs.amd.com` (all versions 1.0–1.8), `amd.com/en/blogs`,
`github.com/amd/RyzenAI-SW` issues, `github.com/microsoft/onnxruntime` issues,
`onnxruntime.ai`, arXiv/alphaXiv-indexed results, and the `npu-benchmark` repo
(checked directly: Stable Diffusion only, Windows only). **Zero results.**

The strongest vendor signal is qualitative: set
`preferred_data_storage="unvectorized"` because "models dominated by GEMMs (e.g.,
Transformers) perform better with unvectorized data"
(https://ryzenai.docs.amd.com/en/latest/modelrun.html, RyzenAI 1.8.0, retrieved
2026-08-04). Nearest published quantitative datapoint, on adjacent workload:
TileFuse reports AMD-NPU driver and reconfiguration overheads "in the millisecond
range" against sub-millisecond GEMV kernels (https://arxiv.org/pdf/2606.11357,
LLM-decode context, retrieved 2026-08-04) — **not** an answer for an encoder.

**Also undocumented, and it matters:** nothing establishes the semantic difference
between the banner's `CPU` count and its `VITIS_EP_CPU` count. Issue #318 shows
both simultaneously with different values, strongly implying `VITIS_EP_CPU` = nodes
the Vitis EP claims but runs on CPU internally (cheap) while `CPU` = nodes handed
back to the ORT CPU EP (real boundary, real tensor copy). **This is not documented
and is not asserted here.** 37 of one and 37 of the other have very different
costs.

---

## §4 — Precision, quantisation, and BF16 against an f32-referenced index (Q2)

> **Scope warning, applied from §9.3 and consistent with §6:** everything in this
> section describes AMD's RyzenAI 1.8.0 Linux stack (XRT 2.25.37 + `xrt_plugin`
> amdxdna DKMS deb + AMD's bundled `libonnxruntime_providers_vitisai.so`). **None
> of it describes anything installable on this host today.** Ubuntu's
> `libxrt-npu2 1:2.21.75+dfsg-4` contains no execution provider (§6). The
> precision analysis below is therefore conditional on obtaining the compiler.

### §4.1 No pre-quantisation is required

**VERIFIED-DOC.** *"When FP32 models are provided as input, the VitisAI EP
automatically converts them to bfloat16 (BF16) precision and processes them
through the optimized BF16 compilation pipeline."*
(https://ryzenai.docs.amd.com/en/latest/model_quantization.html, RyzenAI 1.8.0,
retrieved 2026-08-04.) Corroborated by the landing page: "CNN and Transformer
models in floating-point 32 format as input models without quantization"
(https://ryzenai.docs.amd.com/en/latest/, RyzenAI 1.8.0, retrieved 2026-08-04).

**Struck as a fabricated citation (§9.4):** the phrase "recommended … for better
control over accuracy" is **not on that page** and is not quoted here.

### §4.2 BF16 is a cast path, not a calibrated path

**VERIFIED-DOC.** *"The bfloat16 conversion is implemented by inserting Cast
operations to convert from float32/float16 to bfloat16."* The basic
`convert_fp32_to_bf16` flow takes **no calibration data reader**; AdaQuant
fast-finetuning is explicitly optional.
(https://quark.docs.amd.com/latest/onnx/tutorial_bf16_quantization.html and
.../supported_accelerators/ryzenai/tutorial_convert_fp32_or_fp16_to_bf16.html,
AMD Quark 0.12.post1, retrieved 2026-08-04.) This is a genuine practical
difference from INT8, which requires a calibration dataset.

**VERIFIED-DOC.** BF16 is the **only** precision AMD prescribes for transformers:
CNN → "INT8 or BF16"; **Transformer → "BF16"**; LLM → "INT4 or BF16"
(https://quark.docs.amd.com/latest/supported_accelerators/ryzenai/index.html,
AMD Quark 0.12.post1, retrieved 2026-08-04). There is **no recommended INT8 path
for an encoder**.

**VERIFIED-DOC, but re-scoped (§9.5):** Quark 0.12.post1 ships a universal
`py3-none-any` wheel plus CPU/CUDA wheels for Linux and Windows and Linux-only
ROCm wheels (https://quark.docs.amd.com/latest/install.html, retrieved
2026-08-04). **Quark is not a Windows blocker — but nobody suspected it was.**
The Linux risk sits downstream, in the EP.

### §4.3 What BF16 does to the numbers

- **VERIFIED-DOC → downgraded to SINGLE-SOURCE (cross-generation), §9.6.**
  BF16 multiply-accumulate accumulates in FP32: "bfloat16 operations use the
  `accfloat` format as accumulator registers", `accfloat` being single-precision
  (https://download.amd.com/docnav/aiengine/xilinx2024_1/aiengine_ml_intrinsics/intrinsics/group__group__datatype__accumulator.html,
  **AI Engine-ML (AIE2) Intrinsics User Guide, Vitis 2024.1**, retrieved
  2026-08-04). **That document is for AIE2/Versal and never mentions AIE2P.**
  Strix Point is **AIE2P**. The only architecture-matched corroboration is a
  third-party reverse-engineering site (https://tnzr.org/xdna/isa.html, "BF16
  matrix multiplications accumulate into FP32", retrieved 2026-08-04). Two
  documents describing different hardware generations are not two independent
  sources for this hardware.
- **UNVERIFIED-BUT-FINDABLE (§9.6).** An **AIE-MLv2 / AM020** architecture manual
  covering `aie2p` exists and was not consulted; the correct vendor search terms
  are "AIE-MLv2" and "AM020", not "AIE2P". Secondary sources put `aie2p` at 8
  accumulator registers.
- **UNVERIFIED, and unaddressed by this audit.** XDNA2's marquee MAC format is
  **BFP16 (block floating point, 8×8 MAC)**, not plain BF16. If the 50-TOPS
  datapath is BFP16, "BF16 MACs accumulate in FP32" may not describe the datapath
  actually used. Nobody established which applies to a VAIML-compiled encoder.
- **Derived arithmetic, explicitly not a measurement.** BF16 unit roundoff
  `u = 2⁻⁸ ≈ 3.9e-03`; FP32 `u = 2⁻²⁴ ≈ 6.0e-08` — BF16 rounding is ~65,536×
  coarser per value. *If* accumulation is in FP32, a length-384 dot product's
  worst-case relative error is bounded by ~`2·u_bf16` (input rounding, ~7.8e-03)
  plus `n·u_fp32` (~2.3e-05 at n=384): **input rounding dominates and the error
  does not grow with depth**. Had accumulation been in BF16 the bound would be
  ~`n·u_bf16 ≈ 1.5` — destroyed. This is a bound on one dot product, not a
  prediction of 12-layer embedding drift, and per §4.3 above the FP32-accumulation
  premise is SINGLE-SOURCE for this silicon.

### §4.4 Effect on an f32-referenced index

- **SINGLE-SOURCE.** The only published measurement of a BERT-family **encoder**
  run at BF16 vs FP32: *"BF16 is the most distinct, exhibiting the largest drift
  from all other formats (e.g., an L2 distance of 6.31e-03 from FP32)"*, against
  FP16-vs-FP32 `5.74e-04` and TF32-vs-FP32 `4.09e-04`
  (https://arxiv.org/pdf/2509.18869, "On The Reproducibility Limitations of RAG
  Systems", v1 2025-09-23, §4.4.2, retrieved 2026-08-04).
- **Caveats that limit transfer, all material:** (1) models are `bge-base-en-v1.5`
  (768-dim), E5 and Qwen3-0.6B — **not** `bge-small` (384-dim); (2) platform is
  the TAMU ACES NVIDIA/cuDNN HPC stack, **not an AMD NPU** — the NPU adds op
  fusion, tiling and CPU/NPU partition differences on top of the precision change;
  (3) the metric is whole-vector **L2 distance**, **not** the max per-component
  diff previously measured for Vulkan Q8_0 (2.63e-03) — *these two numbers must
  not be compared*; (4) the paper never states which of its three models the
  6.31e-03 heatmap belongs to, so attributing it to bge-base is an inference; (5)
  it is an unrefereed preprint with placeholder venue metadata ("Workshop '25,
  xxxxxx") and typos in the cited tables ("BF15-non-det."). For L2-normalised
  vectors, `cos = 1 − d²/2`, so 6.31e-03 ⇒ a cosine deficit of ~2.0e-05 —
  **my arithmetic, assuming normalisation, not the paper's**.
- **ABSENT.** Nobody has published a cosine or MTEB/NDCG delta for
  `bge-small-en-v1.5` — or **any** 384-dim BGE — run as a BF16 **encoder** vs an
  FP32 reference, and **no numerical-agreement measurement of any Ryzen AI NPU
  BF16 embedding model against a CPU FP32 baseline exists at all**. Searches run
  2026-08-04 across `ryzenai.docs.amd.com`, `quark.docs.amd.com`, amd.com blogs,
  `blog.vespa.ai`, `sbert.net`, huggingface.co blog, arxiv.org: *"bfloat16 vs fp32
  sentence embeddings cosine similarity difference MTEB bge-small"*; *"BERT encoder
  inference bfloat16 vs float32 embedding output cosine similarity drift
  measurement"*; *"MTEB bfloat16 inference accuracy delta embedding model fp32
  benchmark comparison"*; *"Ryzen AI NPU BF16 bge embedding model cosine similarity
  vs CPU fp32 measured benchmark"*; *"sentence-transformers bf16 vs fp32 embeddings
  retrieval quality benchmark NDCG difference"*. Adjacent material that **does**
  exist and does not close the gap: sentence-transformers efficiency docs benchmark
  bf16 but only across MiniLM-L6-v2 / bge-base / mxbai-large / bge-m3 with
  aggregated charts and no per-precision accuracy table
  (https://sbert.net/docs/sentence_transformer/usage/efficiency.html); several
  cross-encoder **reranker** model cards publish fp32-vs-bf16 NanoBEIR NDCG@10.
- **Do not be misled by the widely-cited "bfloat16 costs almost nothing" embedding
  numbers.** Vespa's bge study casts the **stored document tensor** and keeps the
  query tensor float — verbatim "We do not change the type of the query tensor.
  Vespa casts the bfloat16 field representation to float at search time" — and
  reports NDCG@10 0.7346 (bf16) vs 0.7395 (float) for bge-small-en
  (https://blog.vespa.ai/bge-embedding-models-in-vespa-using-bfloat16/, retrieved
  2026-08-04). **That is BF16 storage, not BF16 encoder compute.** It is not
  evidence for an NPU BF16 backend.
- **Consequence, stated plainly:** an NPU-produced index would be a **third,
  non-interchangeable backend** alongside CPU-f32 and Vulkan-Q8_0. Its vectors are
  not drop-in comparable with an f32-built index, and the magnitude of the
  difference for this specific model is unmeasured by anyone.

### §4.5 Reproducibility

- **ABSENT.** AMD publishes **no** statement that Ryzen AI NPU inference is
  bit-reproducible run to run. The "deterministic" language around AI Engines
  refers to timing/latency determinism and static scheduling, not numerical
  bit-exactness. Searched 2026-08-04: *"Ryzen AI NPU deterministic output run to
  run reproducibility bit-exact VitisAI EP"*; *"AIE AI Engine deterministic
  execution bit-exact repeatable results NPU scheduling"*; *"'Vitis AI' OR 'Ryzen
  AI' NPU inference results differ from CPU numerical accuracy mismatch
  tolerance"*. Pages checked: modelrun/relnotes/ops_support/linux on
  `ryzenai.docs.amd.com`, the ORT Vitis AI EP page, amd.com AI Engine technology
  page. **No such statement exists. The absence of a guarantee is the finding.**
- **VERIFIED-DOC.** What AMD *does* say pins the numerics to a version tuple:
  *"Cache directories generated by the Vitis AI Execution Provider should not be
  reused across different versions of the Vitis AI EP or across different version
  of the NPU drivers."* Combined with `vaiml_config` knobs `optimize_level` (1/2/3,
  default 1) and `preferred_data_storage` (auto/vectorized/unvectorized), the
  compiled artifact — and therefore the fusion, tiling and reduction order that
  determine the output bits — is a function of **(EP version, NPU driver/firmware
  version, config)**, not of "the NPU"
  (https://ryzenai.docs.amd.com/en/latest/modelrun.html, RyzenAI 1.8.0, retrieved
  2026-08-04). See §7.
- **SINGLE-SOURCE, and about CPU/CUDA not the NPU.** ReproRAG found all 8
  precision × determinism-flag configurations (including BF16) perfectly
  reproducible over 5 runs — "mean pairwise L2 distance of 0.0 and cosine
  similarity of 1.0" — and that cuDNN determinism flags "did not influence the
  final embedding vectors" (https://arxiv.org/pdf/2509.18869, Tables 3–5, retrieved
  2026-08-04). Evidence that BF16 *per se* adds no run-to-run noise. **No bearing
  on amdxdna / VitisAI EP.**
- **VERIFIED-DOC.** AMD ships first-party tooling to measure this yourself: Quark
  supports "cosine similarity, L2 loss, PSNR" for comparing float32/float16
  against bfloat16 inference results
  (https://quark.docs.amd.com/latest/supported_accelerators/ryzenai/tutorial_convert_fp32_or_fp16_to_bf16.html,
  Quark 0.12.post1, retrieved 2026-08-04). **The missing number is obtainable; it
  simply has not been published.** Critically, a **CPU-simulated BF16 comparison
  can be done with Quark on Linux without touching the NPU or the driver** — which
  decouples "does BF16 hurt my index" from "does the NPU work here", two risks
  currently entangled.

---

## §5 — Prior art on Linux (Q3)

### §5.1 The corrected result

The research phase reported a flat absence: *nobody has produced text embeddings
on AMD XDNA under Linux.* **That is refuted (§9.9).** The corrected finding, split
into the two claims that behave differently:

**(1) ABSENT — via AMD's supported path.** No one has published a text-embedding
or BERT-family encoder result on XDNA under Linux **through the Vitis AI EP /
VAIML path**. This narrower absence survived re-testing.

**(2) SINGLE-SOURCE — around AMD's path.** `jyatesdotdev/npu-embeddings` reports a
complete `all-MiniLM-L6-v2` forward pass producing 384-dim vectors on XDNA2 under
Ubuntu 25.10, via hand-written `aie2p` kernels + MLIR-AIE/IRON designs + raw XRT
`pyxrt` dispatch, **bypassing the Vitis AI EP entirely**. Repo metadata
independently re-verified by me on 2026-08-04 via
https://api.github.com/repos/jyatesdotdev/npu-embeddings: exists, description
*"Text embeddings on AMD XDNA NPU (Strix Halo) — OpenAI-compatible /v1/embeddings
endpoint powered by all-MiniLM-L6-v2 running on the NPU tile array"*, created
2026-06-30, pushed 2026-07-01, **1 star**.

**Weigh it honestly.** `docs/tasks.md` Milestone 3 is checked complete —
"Full forward pass: embedding lookup → 6 layers → mean pooling → L2 norm",
"Verified: 0.95 cosine similarity vs float model", "Latency: 50ms per embedding" —
but those figures exist **only as checked boxes**: no committed logs, no CI, no
benchmark artifacts. The repo is internally inconsistent (README "Current Status"
still says "Next: Host-orchestrated full model inference at MiniLM dimensions";
Milestone 4 "Benchmark & Optimize", including the STS-B accuracy comparison, is
entirely Todo). Two commits, one author, unreplicated. Hardware is **Strix Halo
(Ryzen AI MAX+ 395)**, not this host's HX 370 Strix Point — both XDNA2, but
portability is an inference nobody has published. It requires XRT ≥2.23.0 and an
MLIR-AIE toolchain built from source; **Ubuntu 26.04's archive ships XRT
2.21.75+dfsg-4, so this path is not turnkey here.**

"Weak published result" and "nobody has published this" are different findings.
The correct one is the former.

### §5.2 The searches that establish the narrowed absence

Authenticated GitHub code search: `VitisAIExecutionProvider bge` (25 hits),
`… MiniLM` (8), `… sentence-transformers` (10), `… embed_query` (6),
`… pooler_output` (3 — two are AMD's own BGE example, one Windows),
`vaiml_partition bert linux` (1), `ryzen_ai-1.8.0 embedding linux` (0),
plus the corrective queries the first pass missed: `all-MiniLM-L6-v2 npu xdna`
(56), `pyxrt embedding minilm` (7), `MiniLM xclbin NPU` (22),
`mlir-aie bert matmul xclbin embedding` (3). GitHub issue search across
`amd/xdna-driver`, `Xilinx/mlir-aie`, `amd/RyzenAI-SW`, `lemonade-sdk/lemonade`,
`FastFlowLM/FastFlowLM`. HuggingFace API: `author=amd&search=bert` → **0 results**;
`author=amd&search=embed` → 2; `search=ryzenai&limit=100` regex-filtered for
`bert|embed|minilm|bge|e5|gte|mpnet|encoder|sentence` → **0 matches**. Tree
enumeration: `amd/RyzenAI-SW` main (669 paths), `Xilinx/mlir-aie` main (4004
paths) regexed for `bert|embed|minilm|bge|sentence|transformer|attention|encoder`
→ **0 matches**. Twelve web searches. All 2026-08-04.

**Search gaps that make this absence weaker than it looks:** two Reddit-targeted
queries returned **zero reddit.com results** — the tool surfaced only vendor pages
and SEO blogspam. I cannot claim r/LocalLLaMA or r/Amd contain nothing; only that
my searches did not reach them. Likewise no `community.amd.com` thread was
retrieved. Both closers are in §11.

### §5.3 What the Linux record actually contains

- **VERIFIED-DOC — AMD advertises the capability and ships zero Linux examples of
  it.** `linux.rst` (RyzenAI 1.8.0) lists "NLP models (e.g., BERT, encoder-based)
  in BF16" as Linux-supported for STX/KRK. Its "Examples, Demos, Tutorials"
  section names **exactly three**, all CNN: ResNet BF16, ResNet INT8, YOLOv8m. AMD
  points Linux users at **no** encoder example. The DistilBERT and BGE examples
  that exist are Windows-shaped: `conda create --name bert --clone
  ryzen-ai-<version>` and `cd <RyzenAI-SW>\Transformer-examples\…` (backslash),
  while AMD's own Linux page says "Skip conda environment instruction as they are
  Windows specific only". Neither README reports a Linux run.
  (https://raw.githubusercontent.com/amd/ryzen-ai-documentation/main/docs/linux.rst;
  https://raw.githubusercontent.com/amd/RyzenAI-SW/main/Transformer-examples/DistilBERT_text_classification_bf16/README.md;
  RyzenAI 1.8.0 / RyzenAI-SW v1.8.0, retrieved 2026-08-04.)
- **SINGLE-SOURCE — the only published Linux test of a transformer encoder through
  the supported path returned numerically WRONG values.** RyzenAI-SW issue #369
  (opened 2026-05-02, **open**): Ryzen AI Max+ 395, Ubuntu 24.04.4, kernel 6.18.13,
  SDK 1.7.1 Linux venv, XRT 2.23.0, FW 1.1.2.65. `nn.TransformerEncoderLayer` × N
  + 2 Linear heads at 165/332/499/1000 nodes — **all four compiled, all four ran on
  NPU, all four returned cosines uncorrelated with the CPU reference**.
  Op-bisection narrowed it to a 7-node `LayerNorm → Linear → 2 outputs`
  (cos = NaN, 0.045) and a 7-node `Linear → Softmax → 2 outputs` (cos = NaN,
  −0.272). Every **single**-output control returned cos ≥ 0.99.
  `preferred_data_storage: unvectorized` — the exact setting in AMD's own DistilBERT
  config — flipped **0 of 4** broken topologies. **Round 4, which the first pass
  missed (§9.10):** AMD's own YOLOv8s-WorldV2 Conv-CNN, converted to 2 outputs with
  AMD's own `split_concat_output()` helper, returns bbox cos 0.9752 but **cls
  cos 0.000004**; the issue states this "invalidates the prior framing of Conv-CNN
  multi-output works in general". The failure is **silent**: it compiles, runs at
  expected NPU latency, and returns plausible shapes and dtypes.
  (https://github.com/amd/RyzenAI-SW/issues/369, retrieved 2026-08-04.)
  **Unreplicated, one author, no AMD confirmation, no retest against 1.8.0.**
- **VERIFIED-DOC artifact fact + ANALYST INFERENCE for the linkage (§9.11).** AMD's
  own BGE example exports `output_names=["last_hidden_state","pooler_output"]` and
  reads `embedding = outputs[1][0]  # pooler_output` — a **two-output graph**,
  the topology class #369 reports as corrupted. **Nobody in #369 tested BGE or any
  real embedding checkpoint** (the failing cases are randomly-initialised toy
  graphs plus YOLOv8s-WorldV2). This is a well-grounded **risk hypothesis**, not
  an established defect on the embedding path.
- **SINGLE-SOURCE — the closest Linux encoder result is audio, not text.**
  `pir0c0pter0/NPUamd` **reports** (not proves — 0 stars, one author, unreplicated)
  a Whisper `tiny.en` **encoder** on the NPU on Linux/Strix Halo: CachyOS/Arch,
  kernel 7.0.0-rc5, PCI 1022:17f0, Quark XINT8, EP report **NPU 416 / CPU 122
  (77.3% offload)**, with `LayerNormalization(9)`, `Gelu(6)`, `Conv(2)` staying on
  CPU. Measured 2026-03-26: **CPU warm ~0.061 s vs NPU warm ~0.26–0.32 s**, NPU
  cold ~7.75 s. The author's own verdict: the gain is proof of offload, **not**
  speedup; accuracy unresolved (best transcript "I'm"), with XINT8 encoder
  fidelity named as the blocker. The same report states the **VAIML path did not
  work in their tree** — `libvaip-pass_vaiml_partition.so` absent, only
  `libvaip-pass_vaiml.so`, which does not partition — though they used a
  hand-assembled local mirror of AMD libs, not the official installer, so that
  specific gate may not apply to the official tarball.
  (https://github.com/pir0c0pter0/NPUamd/blob/main/STATUS.md, retrieved
  2026-08-04.)
- **SINGLE-SOURCE (relabelled from VERIFIED-DOC) — AMD's own ecosystem says no
  runtime runs encoders.** Lemonade maintainer, 2026-07-07: *"No current backend
  can: llama.cpp does embeddings/reranking but not classification heads;
  FastFlowLM/RyzenAI/vLLM are decoder LLMs; Moonshine is STT."* The fix is a new
  generic C++ ORT server whose v1 EP is **CPU**; `vitisai` is Phase 2 and
  `/embed` + `/rerank` are Phase 3. (https://github.com/lemonade-sdk/lemonade/issues/2592,
  created 2026-07-07, **closed**, retrieved 2026-08-04 — the closed state means
  the roadmap may have moved; no follow-up published.)
- **Corrected (§9.12):** Lemonade serves embeddings through its `llamacpp`
  backend, which on Strix-class hardware means the **iGPU, not the NPU**
  (SINGLE-SOURCE, https://github.com/boxwrench/xdna-top/blob/main/docs/npu/runtime-landscape.md,
  retrieved 2026-08-04). But FastFlowLM was **wrongly** described as a
  non-functional stub. FastFlowLM's own docs describe a working NPU2 embedding
  path — `flm serve <llm> --embed 1`, `POST /v1/embeddings`, with limitations only
  that the embedding model must be loaded concurrently with an LLM and does not
  work in CLI mode — and ship real artifacts:
  `FastFlowLM/Embedding-Gemma-300M-NPU2` (ungated, `attn_full_mask.xclbin`,
  `mm.xclbin`, `mv.xclbin`, `sliding_attn.xclbin`, `model.q4nx`, 521 downloads,
  lastModified 2025-10-24). FastFlowLM Linux requires XDNA2 + kernel ≥7.0 with
  `amdxdna` — **which this host satisfies exactly**.
  (https://fastflowlm.com/docs/models/embeddinggemma/,
  https://fastflowlm.com/docs/install_lin/,
  https://huggingface.co/FastFlowLM/Embedding-Gemma-300M-NPU2, retrieved
  2026-08-04.)
- **ABSENT.** No published **measured** Linux run of FastFlowLM EmbeddingGemma on
  the NPU: no vector output, no latency, no accuracy figure, from anyone. The HF
  card is an inherited copy of Google's (its MTEB scores are Google's, not
  FastFlowLM NPU measurements). This is **DOCUMENTED-BUT-UNMEASURED** — precisely
  the "supported table" category this question was meant to exclude, so it counts
  as neither prior art nor as a non-functional stub.
- **Windows encoder prior art, reported separately and not encouraging.**
  RyzenAI-SW #312 (BERT-base hangs indefinitely, §3.4). FastFlowLM #647 (opened
  2026-08-02, open): `flm serve gemma3:4b --embed 1` with embed-gemma:300m on a
  **Ryzen AI 9 HX 370** returns **non-deterministic** 768-d vectors — 8 distinct
  vectors across 60 identical requests, cos as low as 0.31, and embedding a longer
  document permanently changes the vectors of shorter ones (Windows 11 Pro).
  `leerobber/GH05T3` and `r2d2butbetter/Naive-NPU-RAG` wire BGE-large / MiniLM to
  `VitisAIExecutionProvider` — both Windows, neither reports a measured NPU result.
  (retrieved 2026-08-04.)
- **VERIFIED-DOC — the custom-kernel path is no shortcut either.**
  `Xilinx/mlir-aie` `programming_examples/ml` contains: block_datatypes,
  bottleneck, conv2d, conv2d_14x14, eltwise, eltwise_unary, magika, mobilenet,
  norm, resnet, rope, scale_shift, softmax, swiglu. **No BERT, no attention, no
  assembled encoder.** Building blocks exist (softmax, layer_norm, recent `aie2p`
  layer_norm PRs) but nothing assembled.
  (https://api.github.com/repos/Xilinx/mlir-aie/git/trees/main?recursive=1, 4004
  paths, retrieved 2026-08-04.)

---

## §6 — Acquisition (Q4)

### §6.1 The Ubuntu-archive finding, and what it supersedes

**This supersedes the prior audit's conclusion that the work was "blocked on
acquisition — no AMD `.deb` anywhere on this filesystem."** That conclusion was
correct about the filesystem (§2 confirms it) and wrong about the archive.

**VERIFIED-DOC + VERIFIED-HERE.** Ubuntu 26.04 "resolute" universe ships XRT
itself, Debian-packaged from a **combined tree** whose top level contains both
`xrt/` (Xilinx/XRT tag 2.21.75) and `xdna/xdna-driver/` (amd/xdna-driver tag
2.21.75) — so **Ubuntu's NPU shim is AMD's `xdna-driver` userspace, not a Debian
reimplementation** (https://sources.debian.org/src/xrt/1%3A2.21.75+dfsg-9/,
retrieved 2026-08-04).

**Debian builds the xdna userspace ONLY.** `debian/rules` configures
`-DCMAKE_BUILD_TYPE=Release -DXRT_NPU=1 -DXRT_ALVEO=1 -DXRT_ENABLE_HIP=ON
-DXRT_ENABLE_TRACER=OFF` and contains **no** amdxdna kernel-module build; the only
`dh_dkms` invocation is `dh_dkms -pxrt-xocl-dkms`. There is no amdxdna `.install`
file and no amdxdna DKMS binary package anywhere in `debian/`. The bundled kernel
driver source is simply not built.
(https://sources.debian.org/src/xrt/1%3A2.21.75+dfsg-9/debian/rules/, retrieved
2026-08-04.) **This is corroborated on this host** by the apt simulation in §2:
6 packages resolved, `grep dkms|xocl|amdxdna` → NO MATCH.

**Finding about the archive (not an instruction):** the package set
`libxrt-utils-npu` + `libxrt-npu2` resolves cleanly on this host from
Ubuntu:26.04/resolute, is **userspace only**, compiles no kernel module, loads no
module, writes no firmware, and leaves `/lib/modules` untouched — the in-tree
`amdxdna` at `/lib/modules/7.0.0-28-generic/kernel/drivers/accel/amdxdna/amdxdna.ko.zst`
would stay bound. AMD upstream asserts the same runtime/driver separation:
"Ubuntu 25.04 includes Linux kernel 6.14 that incorporates the amdxdna driver …
the XRT SHIM library is still needed"
(https://github.com/amd/xdna-driver/blob/main/README.md, retrieved 2026-08-04).

**What that set actually delivers** (from the archive's published file lists,
retrieved 2026-08-04 — `apt-file` is unavailable on this host, so these come from
`packages.ubuntu.com`, not from the machine):

| Package | Ships |
|---|---|
| `libxrt-utils-npu` | `/usr/bin/aiebu-asm`, `/usr/bin/aiebu-dump`, `/usr/bin/xrt-runner` (+3 manpages) — **no `xrt-smi`** |
| `libxrt-utils` (hard Depends) | `/usr/bin/xrt-smi`, `/usr/bin/xclbinutil`, bash-completion, `/etc/OpenCL/vendors/amdxrt.icd` |
| `libxrt-npu2` (hard Depends) | `libxrt_driver_xdna.so.2{,.21.75}`, `libxdp_core.so.2`, and five XDP plugins (`aie_profile`, `aie_trace`, `ml_timeline`, `native`, `user`) |
| any of them | **no udev rules, no systemd units, no daemons** |

`xbutil` is shipped by **no** Ubuntu package (legacy Alveo tool, superseded by
`xrt-smi`). There is **no `xrt_server`, `msd`, or `mpd`** anywhere: grepping the
whole `amd/xdna-driver` main tree for `xrt_server|XRT_SERVER` returns zero hits.
The runtime is a pure in-process library stack talking ioctls to
`/dev/accel/accel0`. **Nothing needs root at runtime and nothing needs a daemon.**

**Version mapping, resolved.** Ubuntu's `2.21.75` is the same **upstream version**
(Xilinx/XRT tag `2.21.75`) that RyzenAI **1.7.0/1.7.1** ships as
`xrt_202610.2.21.75_24.04-amd64-{base,base-dev,npu}.deb`
(https://ryzenai.docs.amd.com/en/1.7.1/linux.html;
https://github.com/Xilinx/XRT/releases; retrieved 2026-08-04). It is **DFSG-repacked,
not byte-identical** (§9.13). RyzenAI **1.8.0** has moved on to
`xrt_202620.2.25.37_24.04` + `xrt_plugin.2.25.260102.56`
(https://ryzenai.docs.amd.com/en/latest/linux.html, RyzenAI 1.8.0, retrieved
2026-08-04). **Ubuntu is one XRT generation behind current.** Anything in the
1.8.0 compiler requiring a 2.25-era shim ABI is not satisfied by Ubuntu's runtime.

**Boost is clean.** `libboost-filesystem1.90.0` and `libboost-program-options1.90.0`
exist in resolute at `1.90.0-6ubuntu1`, depend only on libc6/libgcc-s1/libstdc++6,
and declare no Conflicts/Breaks/Replaces
(https://packages.ubuntu.com/resolute/libboost-filesystem1.90.0, retrieved
2026-08-04). **The prior audit's boost worry came from AMD's prerequisite
`libboost-filesystem1.74.0`, which is the Ubuntu 24.04 version and does not exist
in resolute at all** (name search: "Sorry, your search gave no results",
retrieved 2026-08-04). That is a real blocker for AMD's `.deb` path and a
**non-issue** for Ubuntu's.

**No kernel-version constraint.** `libxrt-npu2` and `libxrt-utils-npu` Depends are
`${shlibs:Depends}` + `${misc:Depends}` + the sibling packages — **no
`linux-image`, no headers, no dkms** (`debian/control`, retrieved 2026-08-04).
The kernel patches in the Debian package (`6.18.patch`, `6.19.patch`) touch only
XOCL/ZOCL DKMS headers, never amdxdna, and `xrt-xocl-dkms` is not pulled in.
**Kernel 7.0 being "too new" is a non-issue for the NPU path.**

**Two caveats about what Ubuntu actually ships:**
- **SINGLE-SOURCE.** Ubuntu carries `-4` (20 Mar 2026); Debian sid has advanced to
  `-9`. Ubuntu therefore lacks `-5` (gcc16 build fix), `-6`, **`-7` ("pyxrt patch
  for scratchpad and xdna mmap patch")**, `-8`, and **`-9` (xclbin parsing
  vulnerability fix)**. The `-7` xdna mmap patch and the `-9` security fix are
  both NPU-relevant and absent (https://sources.debian.org/src/xrt/1%3A2.21.75+dfsg-9/debian/changelog/,
  retrieved 2026-08-04).
- **SINGLE-SOURCE.** `xrt-smi` on Ryzen devices needs a device "archive" the
  Debian package does not ship; Debian's `xrt-smi.patch` adds a fallback error
  path directing the user to download it from GitHub. Expect `xrt-smi examine` to
  work and archive-dependent subcommands (notably `xrt-smi validate`, which runs
  precompiled test kernels) to fail with a download prompt
  (https://sources.debian.org/src/xrt/1%3A2.21.75+dfsg-9/debian/patches/xrt-smi.patch/,
  retrieved 2026-08-04).

### §6.2 The compiler — where the blocker actually is

**VERIFIED-DOC, re-verified by me 2026-08-04.** Ubuntu's ONNX Runtime has **no
Vitis AI execution provider**. `libonnxruntime-providers` in resolute contains
exactly `libonnxruntime_providers_dnnl.so` and `libonnxruntime_providers_shared.so`
(https://packages.ubuntu.com/resolute/amd64/libonnxruntime-providers/filelist).
**This closes the host block's open question with a hard negative.**

**ABSENT.** There is **no `vitis` package of any kind** in Ubuntu 26.04 — a name
search across all sections of resolute returns zero results, as does a `vaip`
search (https://packages.ubuntu.com/search?keywords=vitis&searchon=names&suite=resolute&section=all,
retrieved 2026-08-04). No `vaip`, no `voe`, no `vai_q_onnx`, no VAIML.

**VERIFIED-DOC.** The compiler is obtainable **only** from `ryzen_ai-1.8.0.tgz`,
downloaded behind AMD's EULA form
(`account.amd.com/en/forms/downloads/ryzenai-eula-public-xef.html`) and installed
by `./install_ryzen_ai.sh -a yes -p <path>/venv` into a Python venv, governed by
the AMD EULA plus a separate "Linux — Third Party End User License Agreement"
(https://ryzenai.docs.amd.com/en/latest/linux.html,
https://ryzenai.docs.amd.com/en/latest/licenses.html, RyzenAI 1.8.0, retrieved
2026-08-04). It is not in Debian, not in Ubuntu, and not redistributable.
*Nuance (§9.14):* the ORT-side Vitis AI EP **glue** is open source and
self-buildable (`--use_vitisai`), but it is **inert** without AMD's proprietary
VAIP/`voe`/xcompiler backend. AMD's own ORT-hosted EP page still lists Ryzen AI
targets as **Windows-only**
(https://onnxruntime.ai/docs/execution-providers/Vitis-AI-ExecutionProvider.html,
unversioned page, retrieved 2026-08-04) — a provenance conflict with AMD's own
`linux.rst`, which the ORT page appears simply to lag.

**VERIFIED-DOC + VERIFIED-HERE — AMD's Linux path does not fit this host, on four
independent axes.** RyzenAI 1.8.0's Linux prerequisites are Ubuntu **24.04 LTS**,
kernel ≥6.10, Python **3.12.x**, `libboost-filesystem**1.74.0**`, and `dkms`; the
debs are all named `*_24.04-amd64-*`. This host is Ubuntu **26.04**, kernel
**7.0.0-28**, Python **3.14.4**. Forcing it means `dpkg --force`, a foreign
boost, and a mismatched Python.
(https://ryzenai.docs.amd.com/en/latest/linux.html + `linux.rst`, RyzenAI 1.8.0,
retrieved 2026-08-04; host facts §2.)

> **⚠ Correction applied before publication — the third libboost error in this
> project's history, caught by the contradiction critic.** An earlier draft of
> this paragraph read "boost **1.90**" as a *host* fact in a list of host facts.
> It is not one. **VERIFIED-HERE (`dpkg -l | awk '$1=="ii" && $2 ~ /boost/'`,
> `ldconfig -p | grep boost_filesystem`, 2026-08-04): this host has
> `libboost-iostreams1.90.0`, `libboost-locale1.90.0` and
> `libboost-thread1.90.0` installed, and NO `libboost-filesystem` at any
> version — `ldconfig` resolves no `libboost_filesystem` at all.** `1.90.0` is
> the version the *archive* offers (`1.90.0-6ubuntu1`), which is why §6.1's
> simulation lists `libboost-filesystem1.90.0` among six packages it *would*
> install. Converting an archive version into installed host state, inside a
> four-axis "does not fit this host" argument, is precisely the move that
> produced the `libboost-filesystem1.74.0` figure the previous audit had to
> strike. The distro-mismatch argument survives on its other three axes; this
> axis is withdrawn as a *host* claim and restated as an archive claim.

**VERIFIED-DOC — and the AMD path takes the driver with it.** The
`xrt_plugin.*-amdxdna.deb` "includes the XDNA driver source and DKMS script that
build, install, and load the primary `amdxdna.ko`", installs both `amdxdna.ko` and
`amdxdna_legacy.ko`, and writes firmware into `/usr/lib/firmware/amdnpu`. Its
`postinst` writes verbatim
`KERNEL=="accel*",DRIVERS=="amdxdna",MODE="0666"` into `udev/rules.d`, writes
`omit_drivers+=" amdxdna "` into `dracut.conf.d`, then `rmmod`s and `modprobe`s its
own DKMS build. AMD's README explicitly recommends "the driver built and shipped
from this repo (the staging `amdxdna.ko`) … rather than an older in-tree module".
(https://github.com/amd/xdna-driver/blob/main/README.md,
https://raw.githubusercontent.com/amd/xdna-driver/main/CMake/config/postinst.in
lines 25–50, retrieved 2026-08-04.) **Installing AMD's stack is a deliberate
replacement of this host's driver, its firmware, and its device permissions — not
an addition.** This **confirms and strengthens** the prior audit's warning. *(The
specific `/lib/modules/<ver>/updates/dkms` depmod-precedence mechanism is standard
behaviour stated FROM MEMORY, UNVERIFIED — the displacement itself is documented
by AMD's own postinst and FAQ.)*

### §6.3 The consequence, stated without softening

Ubuntu gives you `libxrt2` + `libxrt-npu2` + `xrt-smi` + `python3-xrt` at zero
cost and zero driver risk. **Which pre-compiled artifacts that is enough to run
depends on the artifact class, and the distinction is load-bearing:**

| Artifact class | Runnable on Ubuntu's XRT alone? | Why |
|---|---|---|
| Raw `.xclbin` / `.pdi` + control code, dispatched through `pyxrt` (the §5.1 MLIR-AIE path) | **Yes** — this is what XRT *is* | needs only the shim and the driver |
| Vitis AI EP cache directory, or an ORT **EP-context** `.onnx` (the artifact §7 is about) | **No** | an EP-context model cannot be loaded without the EP, and the Vitis AI EP lives in AMD's package under `${RYZEN_AI_INSTALLATION_PATH}/onnxruntime/lib/`, not in XRT (§9.9.2) |

So Ubuntu's XRT is **not** enough to compile a BERT encoder, and it is **not**
enough to *run* the artifact AMD's own toolchain produces for one. It is enough
to run an artifact built the §5.1 way. *(An earlier draft said flatly that this
was "enough to open the device and run a pre-compiled artifact", which is true
only of the first row — corrected per §13.)*

Without accepting AMD's EULA and installing their proprietary stack, an ONNX BERT
model on this host **runs on the CPU**. Code that does not name a provider gets
**no indication the NPU was never involved**. Code that explicitly requests
`VitisAIExecutionProvider` will get an unavailable-provider error or warning from
recent ONNX Runtime — *(exact behaviour of Ubuntu's ORT 1.23 build:
FROM MEMORY, UNVERIFIED; closable by reading
`onnxruntime_inference_collection.py` in the `python3-onnxruntime` source, §9.15)*.

**The acquisition blocker did not dissolve. It moved from the runtime to the
compiler.**

---

## §7 — The compiled artifact and its lifetime (Q5)

> **Conditional section.** Everything here presupposes the compiler from §6.2 has
> been obtained under EULA. It is included because the artifact lifetime governs
> whether this is a one-time cost or a recurring one — which is the deciding
> factor for whether the EULA route is worth taking at all. It does **not** imply
> the compiler is available on this host; §6 says it is not.

### §7.1 What the artifact is

**VERIFIED-DOC.** There is no single monolithic artifact. Compilation produces two
deployable forms: (1) the **VitisAI EP cache directory**, and (2) an **ONNX Runtime
EP-context model** — a `.onnx` wrapper carrying an `EPContext` node whose
`ep_cache_context` attribute names or embeds the compiled binary, produced via
session options `ep.context_enable`, `ep.context_file_path`, `ep.context_embed_mode`.
AMD's guidance, verbatim: *"The VitisAI EP Cache mechanism is most convenient to
quickly iterate during the development cycle. The OnnxRuntime EP Context Cache
mechanism is recommended for the final version of the application."*
(https://ryzenai.docs.amd.com/en/latest/modelrun.html + `modelrun.rst` line 380,
RyzenAI 1.8.0, retrieved 2026-08-04.)

**UNVERIFIED (§9.16).** The internal cache tree (`vaiml_par_0/`,
`vaiml_partition_fe.flexml/`, a hash-keyed `cache/` subdir) is **not in any cited
AMD document**; it was attributed to `modelrun.rst`, which does not contain it,
and the AMD-published HF cache zips were never opened. Do not rely on it.

**VERIFIED-DOC.** The `.xclbin` is **not** a per-model output on XDNA2 — it is a
fixed hardware overlay shipped with the EP (`AMD_AIE2P_4x4_Overlay.xclbin` for
STX/KRK), and in 1.8.0 "For running INT8 models on STX/KRK or newer devices, the
`xclbin` provider option is no longer supported"; the user-supplied option
survives only for PHX/HPT
(https://ryzenai.docs.amd.com/en/latest/relnotes.html and `.../modelrun.html`,
RyzenAI 1.8.0, retrieved 2026-08-04).

**VERIFIED-DOC.** `cache_key` defaults to the **MD5 hash of the ONNX model** —
a *model* fingerprint, not a host/driver/firmware fingerprint. `cache_dir` defaults
to `C:\temp\%USERNAME%\vaip\.cache` on **Windows**; AMD's 1.8.0 docs give **no
Linux default**. The `/tmp/{user}/vaip/.cache/` value circulating in the wild comes
only from the unversioned generic ORT page
(https://onnxruntime.ai/docs/execution-providers/Vitis-AI-ExecutionProvider.html,
retrieved 2026-08-04) — **SINGLE-SOURCE, and that page covers the broader Vitis AI
EP family, not the RyzenAI client stack.** Anyone who does adopt this path would
want `cache_dir` set explicitly rather than left to default — stated as a
property of the configuration, not as an instruction to run anything here.

### §7.2 Is it one-time? No.

**VERIFIED-DOC, verbatim (re-checked against `modelrun.rst` lines 392–394,
RyzenAI 1.8.0, retrieved 2026-08-04):**

> *"Cache directories generated by the Vitis AI Execution Provider should not be
> reused across different versions of the Vitis AI EP or across different version
> of the NPU drivers."*

and the application — not the stack — is told to enforce it: it *"should check the
version of the Vitis AI EP and of the NPU drivers. If the application detects a
version change, it should delete the cache, or create a new cache directory."*
**Nothing in the stack does this for you.** EP version change → recompile. NPU
driver version change → recompile.

Note the scope: both sentences are written about **cache directories**. AMD states
**no analogous invalidation rule for the EP-context model it recommends for
deployment**. The same compiled binary is inside both, so the generalisation is
reasonable — but it is not what the text says.

**On a rolling distro this is the operationally decisive fact.** Ubuntu bumps the
in-tree `amdxdna` with every kernel update, and AMD publishes **no rule that maps
EP versions to `amdxdna` kernel-module versions**.

**VERIFIED-DOC.** AMD's compatibility matrix is **Windows-only and has no 1.8
row**: EP 1.7 → min driver `32.0.203.280`; 1.6 → `32.0.203.280`; 1.5 →
`32.0.203.280`; 1.4.1 → `32.0.203.259`; 1.4 → `32.0.203.257`; 1.3.1 →
`32.0.203.242`; 1.3 → `32.0.203.237`; 1.2 → `32.0.201.204`. Those `32.0.x.y`
strings are **Windows** NPU driver versions. There is **no published equivalent
mapping EP versions to `amdxdna`, XRT, or NPU firmware versions on Linux**, and the
doc does not state what happens when the driver falls outside the range.
(https://ryzenai.docs.amd.com/en/latest/app_development.html + `app_development.rst`,
RyzenAI 1.8.0, retrieved 2026-08-04.)

**ABSENT.** NPU **firmware** version is not mentioned anywhere in AMD's
cache-invalidation policy. Neither is XRT version. The policy names exactly two
axes: EP version and driver version. Whether this host's `fw_version 1.1.2.64`
participates in artifact validity is **undefined by published documentation**.
*Adjacent fact that does exist:* AMD's `xdna-driver` README documents
firmware/driver coupling at the driver level — a stale `npu.sbin` not matching the
loaded `amdxdna.ko` is a known Linux failure cause
(https://github.com/amd/xdna-driver, retrieved 2026-08-04). Firmware participates
in **stack** validity; its role in **artifact** validity is unpublished.

### §7.3 The failure mode when a cache is stale

**Not "documented" — GitHub-evidenced and AMD-maintainer-confirmed (§9.17).** AMD's
documentation never describes this. What exists is RyzenAI-SW issue **#171**
(opened 2025-03-30, **closed 2025-04-10 as completed**, 3 comments): a cache built
under EP **1.3.1** and reused under EP **1.4.0** produced
`Cannot find or create target with fingerprint=0x08000205000e16c6`, flipped the
operator report from **NPU 398** to **CPU 400**, and AMD's own `quicktest.py`
printed **"Test Passed"** throughout. AMD collaborator `uday610` responded
2025-04-03: *"There should be a cache directory from your previous run,
`modelcachekey_quicktest`. Can you remove this and run in 1.4?"* — i.e. **AMD
itself diagnosed the silent CPU fallback as a stale cache and prescribed deleting
it.** (https://api.github.com/repos/amd/RyzenAI-SW/issues/171, retrieved
2026-08-04.) *Axis correction: the primary trigger was the **EP version** change;
the driver had also been updated, so "after a driver update" misattributes it.*

Precisely how silent: there **is** a diagnostic log line and the operator report
would show it. What is silent is **the API surface** — no exception, session
creation succeeds, exit status is clean, and AMD's own smoke test reports success.
And since RyzenAI 1.5 the `vitisai_ep_report.json` — the main detection mechanism —
**is no longer generated by default**. The Linux shape is the same: issue #341
reports `[Vitis AI EP] No. of Operators : CPU 30`, EP initialised, zero operators
offloaded, no error (https://github.com/amd/RyzenAI-SW/issues/341, opened
2026-02-12, open, retrieved 2026-08-04 — cause there is missing `voe`/config
components, not a stale cache).

**ABSENT.** **No published case exists of a stale VAIML/Vitis AI EP cache producing
silently WRONG numerical output.** Searched specifically, twice, 2026-08-04
(queries: stale cache wrong results / cache not invalidated / driver update
segfault / "delete the cache" + wrong output; surfaced #170, #171, #222, #312,
#328, #333, #341, #350, xdna-driver #1017, onnxruntime #27097 — **none** alleging
numerical corruption). The published stale-cache failure modes are (i) silent CPU
fallback and (ii) a hard fingerprint error at session creation. **Note this is a
different question from §5.3's issue #369**, which reports wrong values from a
*fresh* multi-output compile, not a stale cache.

### §7.4 Compile cost, and ahead-of-time compilation

**SINGLE-SOURCE.** AMD gives **no absolute compile-time number** for a small BERT.
The only quantitative statement anywhere is *"The length of time required for
compilation varies, but may take a few minutes to complete"* — and it is on the
**unversioned generic ORT page**, not the RyzenAI docs
(https://onnxruntime.ai/docs/execution-providers/Vitis-AI-ExecutionProvider.html,
retrieved 2026-08-04). Everything else is relative: 1.5 "2x-8x faster model
compilation"; 1.6 "Average 3x compile time improvement" (BF16 compiler section);
1.7 "~40% Compile time improvement for transformer models"
(https://github.com/amd/RyzenAI-SW/releases, retrieved 2026-08-04). **Order of
magnitude: minutes, not seconds** — and AMD's sustained year-over-year investment
in reducing it is itself evidence that it was slow.

**VERIFIED-DOC.** AOT precompilation is the intended production flow, and for BF16
it is **mandatory at deploy time**: *"The deployment version of the VitisAI
Execution Provider (EP) does not support the on-the-fly compilation of BF16 models.
Applications utilizing BF16 models must include pre-compiled versions of these
models."* (https://ryzenai.docs.amd.com/en/latest/app_development.html +
`app_development.rst`, RyzenAI 1.8.0, retrieved 2026-08-04.)

**DOWNGRADED to UNVERIFIED (§9.18).** The often-quoted *"For compiling large BF16
models a machine with at least 32GB of memory is recommended. The machine does not
need to have an NPU"* is **RyzenAI 1.4 documentation**
(https://ryzenai.docs.amd.com/en/1.4/modelrun.html, retrieved 2026-08-04) and is
**absent from all 1.8.0 sources** (grep of the rendered `en/latest/modelrun.html`
and of raw `modelrun.rst`, `app_development.rst`, `linux.rst`, 2026-08-04: no
"32GB", no "does not need to have an NPU"). And "compile it on a Linux workstation"
is an inference, not a documented statement — the 1.4 quote names no OS, and the
1.8 Known Issues say **"Model generation is not supported on Linux in this
release"** (LLM-scoped; see §9.19). **Whether BF16 compilation is supported on a
Linux build machine in 1.8.0 is UNVERIFIED.**

**Portability, DOWNGRADED to inference (§9.20).** The cache key is the model MD5,
and AMD itself distributes prebuilt cache directories
(https://huggingface.co/amd/NPU-CLIP-Python, `clip_text_model_cache.zip` /
`clip_vision_model_cache.zip`, RAI 1.5, retrieved 2026-08-04), so portability
across hosts of the same silicon class is a reasonable inference — bounded by
silicon class (PHX/HPT vs STX/KRK), not host identity. **But AMD documents no
portability guarantee for VitisAI EP artifacts.** The frequently-cited "Models
generated on Windows are compatible with Linux" sentence is from the **1.8 Known
Issues → LLM** section and is scoped to the LLM model-generation flow, **not** to
VitisAI EP caches (`relnotes.rst` line 90, RyzenAI 1.8.0, retrieved 2026-08-04).
**Cross-OS portability of a VitisAI EP artifact: UNVERIFIED.**

**A device fingerprint check does exist and can fail** — `Cannot find or create
target with fingerprint=0x…` (issue #171). So while the cache *key* is model-only,
the runtime does validate the target against the compiled artifact, and mismatch
is what turns a stale cache into a fallback rather than a crash.

---

## §8 — Concurrency and daemon operation (Q6)

### §8.1 Is the NPU exclusively locked? No.

**VERIFIED-DOC (kernel source, Linux v7.0 tag).** `amdxdna_drm_open()` allocates a
per-file client, `xa_init_flags(&client->hwctx_xa, …)` and
`list_add_tail(&client->node, &xdna->client_list)`. There is **no `O_EXCL` check,
no "device busy" rejection, no single-owner field**. Multiple processes may hold
the device open simultaneously.
(https://raw.githubusercontent.com/torvalds/linux/v7.0/drivers/accel/amdxdna/amdxdna_pci_drv.c,
retrieved 2026-08-04.)

**VERIFIED-DOC.** XRT's shim opens with plain `open(dev_node.c_str(), O_RDWR)` —
no `O_EXCL`, no `flock`, no lockfile. The only lock is an in-process
`std::mutex m_open_close_lock` plus an `m_dev_users` refcount, which has **no
cross-process effect**.
(https://raw.githubusercontent.com/amd/xdna-driver/main/src/shim/platform.cpp,
.../pcidev.cpp, repo pushed 2026-08-04, retrieved 2026-08-04.)

**VERIFIED-DOC.** This device (`1022:17f0` rev `0x10`, sysfs `vbnv RyzenAI-npu4`)
maps to `npu4_dev_priv`, which declares `.hwctx_limit = 16` and
`.fw_path = "amdnpu/17f0_10/"` — **the exact firmware directory present on this
host (§2)**. Corroborated by the mainline kernel doc: *"AMD Strix Point can support
16 concurrent workload contexts."*
(https://raw.githubusercontent.com/torvalds/linux/v7.0/drivers/accel/amdxdna/npu4_regs.c;
https://docs.kernel.org/accel/amdxdna/amdnpu.html; retrieved 2026-08-04.)

### §8.2 But you cannot give a small model fewer columns

**VERIFIED-DOC — and this is the finding that matters.** The premise of "4 columns"
is wrong: Strix Point's array is *"4 rows of compute tiles arranged into 8
columns"*. More importantly, on Linux v7.0 the npu4 path **never uses spatial
partitioning**. `aie2_alloc_resource()` begins:

```
if (AIE2_FEATURE_ON(xdna->dev_handle, AIE2_TEMPORAL_ONLY)) {
        hwctx->num_unused_col = total_col - num_col;
        hwctx->num_col = total_col;
        return aie2_create_context(...);
}
```

— it **bypasses the Resource Solver entirely and inflates every context to the full
8-column array**. `AIE2_TEMPORAL_ONLY` is set for **every firmware protocol version
the driver will accept**: `aie2_check_protocol()` **ORs** features from *all*
matching table entries (`ndev->feature_mask |= feature->features`), and the
`{BIT_U64(AIE2_TEMPORAL_ONLY), .major=6, .min_minor=12}` and
`{GENMASK_ULL(AIE2_TEMPORAL_ONLY, AIE2_NPU_COMMAND), .major=7}` entries between
them cover the entire accepted range; anything unmatched returns `-EOPNOTSUPP` and
is fatal to probe. **This host's driver is bound with a populated sysfs
`fw_version` (§2), so an entry matched.**

**Consequence: a small BERT encoder CANNOT be confined to 1–2 columns to leave
room for a second client. All concurrency is firmware-arbitrated time-slicing, not
spatial partitioning. The kernel doc's spatial-partition narrative describes a
hardware capability this code path does not use.**
(https://raw.githubusercontent.com/torvalds/linux/v7.0/drivers/accel/amdxdna/aie2_ctx.c
lines 442–452; `.../npu4_regs.c` lines 91–97; `.../aie2_pci.c` lines 62–82, 152;
`.../aie2_pci.h` lines 232–246; Linux v7.0, retrieved 2026-08-04. *Mechanism
corrected per §9.21.*)

**VERIFIED-DOC (kernel side), firmware behaviour inferred.** On context exhaustion
you get **`-EINVAL`, not `-EBUSY`, and nothing queues.** `aie2_send_mgmt_msg_wait()`
maps any non-`AIE2_STATUS_SUCCESS` reply to `ret = -EINVAL` after logging
`command opcode 0x%x failed, status 0x%x`; a mailbox timeout gives `-ETIME`. There
is **no kernel-side pre-check against `hwctx_limit`** (it is used only for
post-create `fw_ctx_id` validation, `GET_INFO` `npu_task_max` reporting, and
telemetry sizing), **no wait queue, no retry, no preemption**. Enforcement of the
16-context ceiling lives in **firmware**, which is closed — that firmware actually
rejects the 17th create is inferred from `hwctx_limit=16`, not observed.

**Memory note.** *"Every workload context uses a host resident 64 MB buffer"*
(https://docs.kernel.org/accel/amdxdna/amdnpu.html, retrieved 2026-08-04) —
sixteen contexts is ~1 GB of host-resident instruction buffer, against this
machine's **22.58 GiB** total. The memory figure is **VERIFIED-HERE**
(`awk '/MemTotal/' /proc/meminfo` → `22.58 GiB`, 2026-08-04); an earlier draft
carried it in from prior session notes and presented it as a host fact without
having measured it in this audit, which is the same defect as the boost claim in
§6.2, caught by the same critic pass. It happens to be correct. It was measured
rather than left inherited.

### §8.3 Two ONNX Runtime processes?

**UNVERIFIED, with a sharpened caveat (§9.22).** Nothing in the kernel driver or
the XRT shim prevents two ORT processes each holding a Vitis AI EP session; each
would consume one of the 16 contexts. But **no AMD document states this works on
Linux**, and **AMD's own current 1.8.0 release notes list multi-process operation
as hang-prone**: *"Launching multiple processes on 4 1x4 binaries can cause hangs,
especially when models have many sub-graphs"* and *"Context creation might appear
to be limited when some application do not close contexts quickly"*
(https://ryzenai.docs.amd.com/en/latest/relnotes.html, RyzenAI 1.8.0, retrieved
2026-08-04). Every published concurrency figure ("up to eight applications", "up
to 16 hardware contexts", "8 simultaneous inference sessions") sits in
Windows-scoped notes.

**SINGLE-SOURCE, corrected (§9.23).** FastFlowLM publicly presents itself as taking
exclusive NPU access — "exclusive, low-overhead access to the NPU", printing
`[NPU Locked!]` (https://jorgep.com/blog/unlocking-the-npu-fastflowlm/,
Windows-focused, retrieved 2026-08-04) — while Lemonade wraps it in a **userspace**
`SlotPolicy::CoexistByType` that "allows an LLM instance, an Audio instance, and an
Embedding instance to share the NPU concurrently"
(https://deepwiki.com/lemonade-sdk/lemonade/7.4-npu-support, **auto-generated wiki,
treat with suspicion**, retrieved 2026-08-04). **On Linux v7.0 no kernel- or
shim-level exclusive lock exists to take**, so any Linux exclusivity would be
self-imposed userspace behaviour. Whether the Windows "lock" maps to anything on
Linux is unestablished.

### §8.4 Suspend, resume, and runtime PM — relevant on a handheld

**VERIFIED-DOC.** `amdxdna` registers `SYSTEM_SLEEP_PM_OPS(amdxdna_pm_suspend,
amdxdna_pm_resume)`. On suspend, contexts are stopped (firmware context and
mailbox destroyed, in-flight commands aborted); on resume they are transparently
rebuilt (`aie2_create_context` + `aie2_map_host_buf` + `aie2_config_cu` per
context). The fd and userspace hwctx handle stay valid; **userspace does not have
to reload the model.** The driver's own comment names the risk:

> *"The resume path cannot guarantee that mailbox channel can be regenerated. If
> this happen, when submit message to this mailbox channel, error will return."*

**That failure surfaces on the NEXT SUBMIT, not at resume time.** A handheld that
suspends constantly will eventually hit it, so a long-running memory server must
treat submit errors as "tear down and rebuild the XRT context", not as fatal.
(https://raw.githubusercontent.com/torvalds/linux/v7.0/drivers/accel/amdxdna/{amdxdna_pci_drv.c,aie2_pci.c,aie2_ctx.c,amdxdna_pm.c},
Linux v7.0, retrieved 2026-08-04.)

**VERIFIED-DOC.** Runtime PM uses the **same** handlers with
`#define AMDXDNA_AUTOSUSPEND_DELAY 5000 /* milliseconds */`, and
`aie2_hwctx_init()` **releases** its PM reference on the success path — so **merely
having a model loaded does not pin the NPU awake**. For an on-demand embedding
server idle more than 5 s between requests, **every request pays a full NPU restart
plus firmware context recreate.** The wall-clock cost of that is **unmeasured by
anyone** and is the single biggest unknown for p99 latency.

### §8.5 Decision: can a non-seat daemon open `/dev/accel/accel0`?

**No. It will fail with `EACCES` at `open(2)`, today, as configured.**
(VERIFIED-DOC mechanism + VERIFIED-HERE facts.)

The mechanism, from systemd's primary source: the `uaccess` udev builtin grants
the ACL to exactly one uid —
`r = sd_seat_get_active(seat, /* ret_session= */ NULL, &uid);` — and if that
returns `-ENXIO`/`-ENODATA` ("No active session on this seat"), **no uid is added**
and `devnode_acl()` is called with an empty set
(https://raw.githubusercontent.com/systemd/systemd/main/src/udev/udev-builtin-uaccess.c,
systemd main; host runs systemd 259, retrieved 2026-08-04). A systemd **system**
service has no logind session, therefore no active seat session, therefore **no
ACL**. Base perms are `root:render 0660` and `aurascoper` is not in `render` (§2).
XRT will report it as `Open /dev/accel/accel0 failed`.

*(Precision, §9.24: systemd main has a second path — devices tagged `xaccess-*` get
ACLs for non-seat sessions. It is irrelevant here because `/dev/accel/accel0`
carries only `TAG+="uaccess"`, per §2.)*

**Option (ii), `loginctl enable-linger`: dead end.** Linger keeps
`user@UID.service` alive **without any logind session on seat0**; the uaccess
builtin only ever consults `sd_seat_get_active()`. Corroborated independently
(https://discussion.fedoraproject.org/t/uaccess-tagged-udev-rules-in-usermode-systemd-services-and-ssh-access-for-sdrs/149482;
https://bbs.archlinux.org/viewtopic.php?id=306211, retrieved 2026-08-04):
"the uaccess acls don't get applied to users logged in via ssh or to user services
started via ssh or on boot with enable-linger".

**`DeviceAllow=` will not fix it either.** `DeviceAllow=`/`DevicePolicy=` control
systemd's cgroup device filter and can only further **restrict**; they never grant
DAC permission. If `DevicePolicy=closed`/`strict` is in play you additionally need
`DeviceAllow=/dev/accel/accel0 rw`, but on its own it cannot turn an `EACCES` into
a successful open. It is not an alternative to the group.

**The minimum change that would work — described, not recommended for
execution:** `SupplementaryGroups=render` on the systemd **system unit**. Base
perms are `root:render 0660`, so a process carrying `render` in its supplementary
groups opens the node read-write with **no ACL, no seat, and no root**. What it
does: grants the group **to the service process only**. What it does not do: it does
**not** add the user to `render`. What it risks: any process under that unit gains
read-write access to every `render`-group device on the machine, including the GPU
render nodes; and it requires editing a unit file as root.

**Why the standing rule "do not add myself to `render` as a fix" does not
transfer.** That rule is about the **interactive** case, where adding the group is
genuinely a no-op because the uaccess ACL already grants `aurascoper` rw on the
live seat session. **For a non-seat daemon there is no ACL at all**, so group
membership is not redundant — it is the entire access mechanism. The two cases are
not the same.

**Option (iv), a system-wide udev rule** (`SUBSYSTEM=="accel", KERNEL=="accel*",
GROUP="<group>", MODE="0660"` in `/etc/udev/rules.d/`) requires root and persists
across reboots for **every** accel device. Note that AMD's own `xrt_plugin` deb
does exactly this but at **world read-write** (`MODE="0666"`, §6.2) — and the same
postinst displaces the driver. **Do not adopt AMD's deb as a permissions fix.**

**A trap worth naming.** The uaccess ACL is **revoked on seat active-session
change**: logind's `seat_set_active()` calls `seat_trigger_devices()`, re-running
udev and hence the uaccess builtin, re-applying the ACL to the new active session's
uid — or clearing it if there is none
(https://raw.githubusercontent.com/systemd/systemd/main/src/login/logind-seat.c,
retrieved 2026-08-04). An already-open fd keeps working (POSIX checks at `open(2)`
only), so a daemon **started from the interactive session** survives VT switches
and even logout — **but dies permanently the first time it restarts while no
session is active.** That is a fragile foundation for a memory server, and it fails
at 3 a.m. rather than at deploy time.

**Finally, note what installing Ubuntu's XRT does to any of this: nothing.** None
of `libxrt2`, `libxrt-npu2`, `libxrt-utils`, `libxrt-utils-npu` ships a udev rule
or a systemd unit (§6.1). **The daemon permission problem survives that install
completely untouched.**

---

## §9 — Corrected during the audit

Every refutation is recorded here. Nothing was silently dropped.

### §9.A — Corrections to the PRIOR audit's conclusions

**§9.A.1 — "Blocked on acquisition; no AMD `.deb` anywhere on this filesystem."
SUPERSEDED.** True about the filesystem, wrong as a conclusion about
availability. Ubuntu 26.04's own archive ships the XRT NPU runtime, Debian-packaged
from AMD's own `xdna-driver` userspace, **built userspace-only with no amdxdna
DKMS**, resolving cleanly on this host. See §6.1. The prior audit likely also
propagated AMD's `libboost-filesystem1.74.0` prerequisite as if it applied to the
Ubuntu path; it does not — Ubuntu's XRT is built against boost 1.90 and resolves
natively.

**§9.A.2 — "model_generate (the LLM / OGA flow) is Windows-only in RyzenAI 1.8.0."
PARTIALLY WRONG, and the correction is subtle.** `llm_linux.rst` (RyzenAI 1.8.0,
retrieved 2026-08-04) is an entire page titled "Running LLM on Linux" walking
through an OGA **NPU-only** LLM run on Linux (Phi-3.5-mini `rai_1.8.0_npu_4K`), and
`linux.rst` line 14 lists "LLMs (NPU-only flow)" among Linux-supported formats.
What **is** Windows-only is (a) the **Hybrid** flow ("Note — Linux does not support
Hybrid flow", `llm_linux.rst`) and (b) **model generation**: "Model generation is
not supported on Linux in this release. Models generated on Windows are compatible
with Linux" (`relnotes.rst` line 90, 1.8 Known Issues → LLM). So: *running* an OGA
NPU-only LLM on Linux is documented; *building* the OGA model on Linux is not.
**This does not change any conclusion about BERT encoders**, which go through the
Vitis AI EP / VAIML path, not OGA.

**§9.A.3 — "Installing AMD's `xrt_plugin` .deb displaces the in-tree amdxdna
driver." CONFIRMED AND STRENGTHENED.** AMD's own `postinst` verbatim writes
`KERNEL=="accel*",DRIVERS=="amdxdna",MODE="0666"`, writes
`omit_drivers+=" amdxdna "` to `dracut.conf.d`, and `rmmod`/`modprobe`s its DKMS
build; the README recommends its staging module "rather than an older in-tree
module". It also **overwrites `/usr/lib/firmware/amdnpu`**. §6.2.

**§9.A.4 — the prior audit's ABI finding remains the load-bearing assumption for
Ubuntu's shim, and nobody has confirmed it for the packaged build.** See §11.

### §9.B — FATAL refutations (these claims do NOT appear as findings)

**§9.9.1 — REMOVED: "The BF16 operator-table gaps are a BLOCKER; a bge-small BF16
compile will end up mostly on CPU."** The table reading is correct and survives
(§3.1). The **causal conclusion does not**. It is directly contradicted by AMD
shipping `openai-whisper-medium-encoder` as a BF16 NPU model in the 1.8.0 Known
Issues → BF16 Models subsection, and adding `embeddinggemma-300m` embedding support
in the same release — both transformer encoders built from LayerNorm + Softmax +
GELU. The table's preamble scopes it only as "broad coverage … for CNN and NLP
models", never claims to be exhaustive, never mentions the VAIML BF16 compiler
flow, and **never defines what a blank cell means**. Reading a coverage table as a
per-model verdict is exactly the failure this audit was meant to avoid. **Corrected
status: table VERIFIED-DOC, consequence UNVERIFIED (§3.2).**

**§9.9.2 — REMOVED: the framing that the FP32→BF16 VitisAI EP path is a live option
on this machine.** It is not reachable on this host as configured. Ubuntu's
`libxrt-utils-npu`/`libxrt-npu2` is a different product from AMD's RyzenAI Linux
stack and **contains no execution provider** — the VitisAI EP lives in AMD's
package (`${RYZEN_AI_INSTALLATION_PATH}/onnxruntime/lib/`), not in XRT. AMD's stack
additionally mismatches this host on four axes (Ubuntu 24.04 vs 26.04; DKMS
amdxdna vs in-tree; Python 3.12 vs 3.14.4; boost 1.74 vs 1.90). **Every precision
claim in §4 is therefore explicitly conditional (§4 scope warning, §6.2).**

**§9.9.3 — REMOVED: "NOBODY has published a text embedding or BERT-family encoder
producing vectors on AMD XDNA under Linux."** Refuted by
`jyatesdotdev/npu-embeddings` (re-verified by me 2026-08-04 via the GitHub API:
exists, created 2026-06-30, pushed 2026-07-01, 1 star), which reports a complete
`all-MiniLM-L6-v2` forward pass on XDNA2 under Ubuntu 25.10 at 0.95 cosine vs the
float model and 50 ms/embedding, via MLIR-AIE/IRON + raw `pyxrt`, **bypassing the
Vitis AI EP**. The search design was the flaw: every query presupposed the vendor
path, and the mlir-aie check regexed the **upstream** tree ("does AMD ship a BERT
example") rather than searching for third parties who **built** one. Queries that
surface it in seconds: `all-MiniLM-L6-v2 npu xdna` (56 results), `pyxrt embedding
minilm` (7), `MiniLM xclbin NPU` (22). **Corrected status: the absence survives
only in the narrower form — nobody has done it via AMD's supported path (§5.1).**

### §9.C — MATERIAL refutations (claims appear DOWNGRADED)

**§9.1 — see §9.9.1.** (BF16 blocker → UNVERIFIED.)
**§9.2 — "At default settings you are told nothing at all."** Literally false. ORT
core emits `[W:onnxruntime:, session_state.cc VerifyEachNodeIsAssignedToAnEp] Some
nodes were not assigned to the preferred execution providers…`, which **is** at the
default WARNING threshold, and issue #318 shows it on Linux during total fallback.
**Corrected form (§3.4): you are told one generic line that names no ops, gives no
counts, and fires in healthy sessions too — so total CPU fallback is
indistinguishable from normal operation at defaults.** The EP itself emits no
error/exception/warning; its informative banner is INFO and suppressed.
**§9.3 — see §9.9.2.** (Scope of §4.)
**§9.4 — fabricated citation, STRUCK.** The quoted fragment "recommended … for
better control over accuracy" is **not** on
`https://ryzenai.docs.amd.com/en/latest/model_quantization.html`. That page's
Quark statement is "AMD Quark is the recommended quantization tool to convert FP32
models to **INT8**" — a different claim. The FP32-auto-convert quote is genuine and
retained. **That AMD anywhere describes BF16 pre-quantisation as optional-but-
recommended-for-accuracy is UNVERIFIED.**
**§9.5 — "Quark runs on Linux, so quantisation is not a Windows blocker."** The
Quark facts are correct but the conclusion clears the wrong obstacle — nobody
suspected Quark. The Linux risk is downstream in the EP, and the field evidence
there is adverse: RyzenAI-SW #341 (Linux, amdxdna, `/dev/accel/accel0` accessible —
this host's topology) reports "the `voe` Python wheel is entirely missing from
Linux x86_64 distributions", a config-parser crash, and `No. of Operators : CPU 30`
with zero NPU offload; #313 and #178 corroborate total CPU mapping on Linux. **All
three predate 1.8.0, and #341 used a custom-compiled ORT rather than AMD's
installer** — a commenter (2026-04-10) reports the official `install_ryzen_ai.sh`
for 1.7.1 *does* ship `voe-1.7.1-py3-none-linux_x86_64.whl` and "does allow to use
the NPU alright". Not proof 1.8.0 is broken; the only field reports that exist, and
they point one way.
**§9.6 — accumulator evidence, DOWNGRADED VERIFIED-DOC → SINGLE-SOURCE
(cross-generation).** The `accfloat` page is the **AIE2 / Vitis 2024.1** intrinsics
guide and never mentions AIE2P; the only architecture-matched corroboration is
third-party. Also: an **AIE-MLv2 / AM020** manual covering `aie2p` exists and was
not consulted (wrong search terms were used), and **XDNA2's marquee MAC format is
BFP16, not plain BF16** — unaddressed. (§4.3.) *(AMD's page literally reads "1b
sign, 8b exponent, 23b exponent" — a typo in the source, silently corrected to
"mantissa" in the original claim.)*
**§9.10 — issue #369 Round 4 was missed, and it cuts against the reassurance.** The
original read stopped at Round 3 ("AMD's 2-output Nemotron OCR CNN is correct, so
it is not universally broken"). Round 4 shows AMD's own YOLOv8s-WorldV2 Conv-CNN,
converted with AMD's own helper, returning **cls cos 0.000004**, and the issue
states this "invalidates the prior framing of Conv-CNN multi-output works in
general". **The corruption is not confined to transformer-shaped graphs.** (§5.3.)
**§9.11 — the #369 → BGE linkage, DOWNGRADED to ANALYST INFERENCE.** The artifact
facts are VERIFIED-DOC (BGE example uses two outputs and reads `outputs[1]`; #369
reports multi-output corruption). **The connection is inference.** Nobody in #369
tested BGE or any real embedding checkpoint; the reporter is a single unreplicated
author; the issue is open with no AMD confirmation; and the reporter discloses an
earlier environment defect (a user-site onnxruntime shadowing AMD's bundled
`onnxruntime-vitisai`) that invalidated an earlier round. **Risk hypothesis, not
established defect.**
**§9.12 — FastFlowLM "non-functional embeddings stub", CORRECTED.** FastFlowLM's
own docs describe a working NPU2 embedding path with real xclbins, Linux-eligible
on this host's silicon. The word "bare" in the third-party source was load-bearing
and was dropped: it means the endpoint **without `--embed`**, not the feature.
**Corrected: DOCUMENTED-BUT-UNMEASURED** — which counts as neither prior art nor as
a stub. The Lemonade half (embeddings via llamacpp → iGPU, not NPU) survives.
(§5.3.)
**§9.13 — "Ubuntu's 2.21.75 is the identical upstream tree."** False by
construction: Debian's tarball is `+dfsg`, repacked with files stripped. **Corrected:
same upstream version number (tag Feb 2025), DFSG-repacked, not byte-identical**;
and "exactly contemporaneous with RyzenAI 1.7.x" means same version *number*, not
same era of development. (§6.1.)
**§9.14 — "the compiler is not separable from the AMD installer."** Over-general.
The ORT-side Vitis AI EP glue is open source and self-buildable (`--use_vitisai`).
**Corrected: the compiler *backend* (VAIP/VAIML/xcompiler) is EULA-gated and the
open glue is inert without it** (§6.2).
**§9.15 — "nothing will tell you the NPU was never involved."** Holds only for
default-provider usage. Explicitly requesting `VitisAIExecutionProvider` on an ORT
build that lacks it produces an unavailable-provider error/warning. **Exact
behaviour of Ubuntu's ORT 1.23 build: FROM MEMORY, UNVERIFIED.** (§6.3.)
**§9.16 — cache tree internals, DOWNGRADED to UNVERIFIED.** `vaiml_par_0/` and
`vaiml_partition_fe.flexml/` are **not** in `modelrun.rst`; the HF cache zips were
never opened. Also the two "quotes" about dev-vs-production were paraphrases; the
verbatim text is now used (§7.1).
**§9.17 — issue #171 metadata was wrong, and the correction STRENGTHENS the
finding.** It is **closed 2025-04-10 as completed**, not open-and-unanswered, and
AMD collaborator `uday610` diagnosed it as a stale cache and prescribed deletion.
Also the trigger axis was the **EP version** change (1.3.1 → 1.4.0), not the driver
update alone. **Upgraded from anecdote to AMD-maintainer-confirmed failure mode.**
Separately, "the documented failure mode" → **"GitHub-evidenced, AMD-confirmed"**:
AMD's docs never describe it. (§7.3.)
**§9.18 — "32GB, no NPU needed, compile on a Linux workstation", DOWNGRADED to
UNVERIFIED for 1.8.0.** The quote is RyzenAI **1.4** documentation and is absent
from every 1.8.0 source (grep, 2026-08-04); the OS was never named. (§7.4.)
**§9.19 — "Models generated on Windows are compatible with Linux" mis-scoped.** It
is a **1.8 Known Issues → LLM** note about model generation, not a statement about
VitisAI EP artifacts. (§7.4.)
**§9.20 — artifact portability, DOWNGRADED to inference.** Model-MD5 keying and
AMD's published prebuilt caches make same-silicon portability reasonable; AMD
documents **no** guarantee, and cross-OS portability of a VitisAI EP artifact is
**UNVERIFIED**. (§7.4.)
**§9.21 — the TEMPORAL_ONLY mechanism was described wrongly; the conclusion
survives.** It is **not** true that every `npu4_fw_feature_table` entry sets
`AIE2_TEMPORAL_ONLY` — three of five do not. The conclusion is rescued by
`aie2_check_protocol()` **OR-ing** features across all matching entries, which the
original never stated. Same conclusion, corrected mechanism (§8.2).
**§9.22 — "nothing prevents two ORT processes", CAVEAT ADDED.** AMD's own current
1.8.0 release notes document multi-process **hangs** as a known issue. The
kernel-mechanism fact is correct; the practical reliability claim is not. (§8.3.)
**§9.23 — "no published statement that FastFlowLM takes an exclusive lock",
REFUTED.** Such material exists (`[NPU Locked!]`, "exclusive, low-overhead
access"), Windows-focused. It does not contradict the driver analysis — no Linux
lock exists to take — but the absence claim was wrong. (§8.3.)
**§9.24 — uaccess "grants to exactly one uid", slightly overstated for systemd
main.** A second mechanism exists for `xaccess-*`-tagged devices. **Irrelevant
here** — this device carries only `TAG+="uaccess"` — so the conclusion is
untouched. Also: source read was systemd `main`; host is systemd 259. (§8.5.)
**§9.25 — firmware-floor ABSENT, REFRAMED.** Firmware floors *are* published in
`amd/xdna-driver`'s per-NPU regs constants (readable at tag 2.21.75 in the Debian
tarball), and mismatch field reports exist (`xdna-driver` #1074, #1219 — both
`17f0_11`/Strix Halo, **not** this host's `17f0_10`). **Corrected: no firmware
constraint is stated for XRT 2.21.75 *userspace*; enforcement lives in the kernel
driver; and on this host firmware acceptance is already VERIFIED-HERE — the in-tree
driver is running fw 1.1.2.64. The live risk is the shim ioctl ABI, not
firmware.** (§6, §11.)
**§9.26 — "nobody has published anything on Ubuntu-XRT-vs-in-tree-amdxdna
compatibility", SCOPED DOWN.** The Debian BTS is genuinely empty for `src:xrt`
("No reports found!", retrieved 2026-08-04) and there is no `README.Debian`
(HTTP 404). But adjacent published material exists: AMD's README FAQ warns to use
its own module "which implements the matching ioctls, rather than an older in-tree
module", and `xdna-driver` #1074/#1219 are Ubuntu field reports of in-tree-vs-newer-
stack mismatch. **Corrected: ABSENT holds only for the Debian-packaged XRT against
in-tree amdxdna specifically.** Note this host's direction (older 2.21.75 shim
against a newer kernel-7.0 in-tree driver) is the direction mainline uAPI stability
rules should protect.

### §9.D — NIT corrections (recorded for completeness)

- **§9.27** `microsoft/onnxruntime#26755` was **de-staled and assigned to maintainer
  `justinchuby`/`hariharans29` on 2026-01-12**, four days after the bot staled it.
  Citing only the stale event made it look abandoned.
- **§9.28** `inst.rst` (source of the INFO-severity banner evidence) is the
  **Windows** install page. Generalisation to Linux is plausible — the banner text
  appears on Linux in #318 — but the Linux default-suppression is inferred.
- **§9.29** Release dates: the GitHub releases API (re-fetched 2026-08-04) gives
  `v1.8.0` 2026-07-23, `v1.7.1` 2026-03-27, `v1.7.0` 2026-01-23, `v1.6.0`
  2025-10-08, `v1.5.0` 2025-07-02. Earlier drafts mixed **PDF cover dates**
  (e.g. "1.6 Nov 7 2025", "1.5 Oct 8 2025") with **tag dates**. Tag dates are used
  here; the divergence between doc-cover dates and tag dates is unexplained and
  minor.
- **§9.30** `pir0c0pter0/NPUamd` "proves" → **"reports"** (0 stars, single author,
  no third-party reproduction).
- **§9.31** ABSENT on partition-boundary cost stands, but TileFuse
  (arXiv 2606.11357) publishes millisecond-range AMD-NPU driver/reconfiguration
  overhead per invocation in an LLM setting — the nearest adjacent number, and it
  was not surfaced initially.
- **§9.32** Headline package attribution was loose: `libxrt-utils-npu` itself ships
  only `aiebu-asm`, `aiebu-dump`, `xrt-runner`; `xrt-smi` comes from `libxrt-utils`
  and the shim from `libxrt-npu2`. The *install set* delivers all three; no single
  package does.
- **§9.33** ReproRAG source-quality caveats added: unrefereed preprint, placeholder
  venue metadata, typos in the cited tables, and the paper never says which model
  the 6.31e-03 heatmap belongs to.
- **§9.34** Context-exhaustion errno: `-EINVAL` is not the *only* possible value —
  a mailbox timeout gives `-ETIME`; and firmware actually rejecting the 17th create
  is inferred, not observed.
- **§9.35** Three third-party repos that surfaced near the §9.9.3 finding are **not**
  additional prior art and were each checked: `leerobber/GH05T3` is Windows with
  conditional CPU fallback; `wishfine/localdoc-agent-amd-ai` is an aspirational
  design doc containing a fictional `apt install amd-npu-driver`;
  `tiqa13/nomic-embed-text-v1.5-amd-npu` is a bare ONNX export with no xclbin and
  0 downloads.

---

## §10 — Single-source claims

Every load-bearing fact below rests on **exactly one** citation. All retrieved
2026-08-04.

| # | Claim | Its one source |
|---|---|---|
| S1 | Changing one Resize op flipped a Linux model from `NPU 2356` to **zero NPU operators**, with only ORT's generic warning emitted; AI Analyzer produced no logs | https://github.com/amd/RyzenAI-SW/issues/318 (RyzenAI 1.6.1, Ubuntu 22.04, open) |
| S2 | BERT-base at opset 17 hangs indefinitely at `InferenceSession` creation in **both** INT8 and BF16, no logs, no timeout, while ResNet50 succeeds | https://github.com/amd/RyzenAI-SW/issues/312 (+ twin https://github.com/microsoft/onnxruntime/issues/26755) — hardware is Ryzen 7 8700G (Phoenix), **not** this host |
| S3 | BF16-vs-FP32 encoder embedding drift = **L2 6.31e-03** (vs FP16 5.74e-04, TF32 4.09e-04); and all 8 precision×determinism configs perfectly reproducible over 5 runs | https://arxiv.org/pdf/2509.18869 (unrefereed preprint; bge-**base**, NVIDIA/cuDNN) |
| S4 | BF16 MAC accumulates in FP32 on **this** silicon generation | https://tnzr.org/xdna/isa.html (third-party; the AMD `accfloat` doc is AIE2/Vitis 2024.1, wrong generation) |
| S5 | A transformer encoder compiled through the Linux VitisAI EP returned **numerically wrong values** across four sizes; single-output controls fine; Round 4 hits a Conv-CNN too | https://github.com/amd/RyzenAI-SW/issues/369 (SDK 1.7.1, Strix Halo, open, unreplicated, no AMD confirmation) |
| S6 | A Whisper `tiny.en` **encoder** ran at 77.3% NPU offload on Linux/Strix Halo, slower than CPU, accuracy unresolved; and `libvaip-pass_vaiml_partition.so` was absent from that tree | https://github.com/pir0c0pter0/NPUamd/blob/main/STATUS.md (0 stars, one author) |
| S7 | A complete `all-MiniLM-L6-v2` forward pass on XDNA2 under Linux at 0.95 cos / 50 ms per embedding, via MLIR-AIE + `pyxrt` | https://github.com/jyatesdotdev/npu-embeddings (1 star, 2 commits, no committed logs; **repo existence and metadata re-verified by me 2026-08-04**) |
| S8 | AMD's local-inference ecosystem has **no** runtime that runs encoder models; `vitisai` is Phase 2, `/embed` Phase 3 | https://github.com/lemonade-sdk/lemonade/issues/2592 (closed) |
| S9 | Lemonade serves embeddings via `llamacpp` → the **iGPU**, not the NPU | https://github.com/boxwrench/xdna-top/blob/main/docs/npu/runtime-landscape.md |
| S10 | FastFlowLM presents itself as taking **exclusive** NPU access (`[NPU Locked!]`) | https://jorgep.com/blog/unlocking-the-npu-fastflowlm/ (Windows-focused) |
| S11 | Lemonade wraps FastFlowLM in a userspace `SlotPolicy::CoexistByType` | https://deepwiki.com/lemonade-sdk/lemonade/7.4-npu-support (**auto-generated wiki — treat with suspicion**) |
| S12 | The AMD Ryzen AI Linux download is EULA-gated with **no AMD apt repository** | https://ryzenai.docs.amd.com/en/latest/linux.html (RyzenAI 1.8.0) |
| S13 | Ubuntu ships XRT `-4` and therefore lacks Debian revisions `-5`…`-9`, including the `-7` **xdna mmap patch** and the `-9` **xclbin parsing security fix** | https://sources.debian.org/src/xrt/1%3A2.21.75+dfsg-9/debian/changelog/ |
| S14 | `xrt-smi` needs a device "archive" the Debian package does not ship; archive-dependent subcommands will prompt for a download | https://sources.debian.org/src/xrt/1%3A2.21.75+dfsg-9/debian/patches/xrt-smi.patch/ |
| S15 | The Linux default `cache_dir` is `/tmp/{user}/vaip/.cache/` | https://onnxruntime.ai/docs/execution-providers/Vitis-AI-ExecutionProvider.html (**unversioned page covering the whole Vitis AI EP family, not the RyzenAI client stack**) |
| S16 | Compilation "may take a few minutes to complete" — the only absolute figure published by anyone | same unversioned ORT page as S15 |
| S17 | A stale cache produced `Cannot find or create target with fingerprint=…`, `CPU 400`, and "Test Passed"; AMD prescribed deleting the cache | https://github.com/amd/RyzenAI-SW/issues/171 (closed 2025-04-10 as completed) |
| S18 | `No. of Operators : CPU 30` on Linux with `voe` missing entirely from Linux x86_64 distributions | https://github.com/amd/RyzenAI-SW/issues/341 (open) |
| S19 | FastFlowLM `--embed 1` returns **non-deterministic** 768-d vectors on a Ryzen AI 9 HX 370 (8 distinct vectors across 60 identical requests) | https://github.com/FastFlowLM/FastFlowLM/issues/647 (Windows 11 Pro, open) |
| S20 | An `xdna-npu-toolkit` Linux embeddings walkthrough gets EP-initialises-on-NPU=True but "at-runtime compile available: False (deployment-only build)" | https://raw.githubusercontent.com/tibrezus/xdna-npu-toolkit/main/docs/EMBEDDINGS-WALKTHROUGH.md (XDNA1/Phoenix — likely does not gate this host's path) |

---

## §11 — Gaps and UNVERIFIED

Each item names the specific document, command, or experiment that would close it.
None of these is a recommendation to run anything on this host.

**G1 — THE CENTRAL GAP: what a blank cell means in the 1.8.0 operator table.**
If blank = unsupported, LayerNorm + Softmax + Gelu all fall back to CPU in the BF16
path — roughly 24–37 partition boundaries for a 12-layer bge-small, which would
almost certainly erase any benefit. If blank = merely untested/undocumented, the
encoder may compile whole. **Closer:** compile `bge-small-en-v1.5`
(`torch.onnx.export`, opset 17, `dynamic_axes=None`, static `(1,512)`) through the
VAIML config with `enable_cache_file_io_in_mem=0` and
`XLNX_ONNX_EP_REPORT_FILE=vitisai_ep_report.json`, then read `deviceStat[].nodeNum`
for CPU vs NPU. **That single JSON answers §3.1, §3.2 and §3.5 definitively.
Nothing short of running it will.** Blocked upstream by G6.

**G2 — whether the VAIML compiler accepts *decomposed* forms in lieu of fused ops.**
In the BF16 column, `Exp`/`ReduceSum`/`Sub`/`Div` are all `Y`, so a decomposed
Softmax is expressible; `ReduceMean`/`Sub`/`Pow`/`Sqrt`/`Div`/`Add` likewise for
LayerNorm; `Erf`/`Mul`/`Add`/`Div` for an Erf-form GELU. **But** AMD nowhere states
the compiler accepts decomposed forms, and opset-17 `torch.onnx.export` *emits* the
fused `LayerNormalization` node — so you would have to force a pre-17 export or do
graph surgery, contradicting the opset-17 recommendation. **Closer:** compare the
report JSON from an opset-17 export against one from an opset-16 export.

**G3 — whether `Gather` (the ~30k×384 token-embedding lookup) lands on NPU.**
Mode-dependent with no clean answer: `Gather` is `Y` in BF16 only; LayerNorm/
Softmax/Gelu are `Y` in XINT8 but `Gather` is not. **Closer:** read
`supportedOpType` per device in the report JSON for both a BF16 and an XINT8
compile.

**G4 — the semantic difference between `CPU` and `VITIS_EP_CPU` in the banner.**
Undocumented. Issue #318 shows both with different values, implying `VITIS_EP_CPU`
= nodes the EP claims but runs on CPU internally (cheap) vs `CPU` = nodes handed
back to the ORT CPU EP (real boundary, real tensor copy). **This matters enormously
for G1's arithmetic** and is not asserted here. **Closer:** ask AMD on the
RyzenAI-SW tracker, or infer from AI Analyzer profiling output — noting that #318
reports AI Analyzer producing no logs on Linux at all, so the one tool that could
quantify this may itself be non-functional on the platform in question.

**G5 — whether issue #369's silent multi-output corruption still reproduces on
1.8.0.** Filed against 1.7.1 / XRT 2.23.0 / FW 1.1.2.65; **no retest published**;
AMD has not responded in-thread. **This is the single highest-value unknown for an
embedding workload**, because AMD's own BGE example reads `outputs[1]` of a
two-output graph. **Closer:** a 1.8.0 retest of the 7-node
`LayerNorm → Linear → 2 outputs` minimal repro, or an AMD reply on #369. Related:
whether a **single**-output encoder export (last_hidden_state only, or pooling
folded into the graph) escapes it — #369's single-output controls were all cos ≥
0.99, but never on a real checkpoint.

**G6 — whether the compiler is obtainable and functional on this OS at all.**
`ryzen_ai-1.8.0.tgz` is EULA-gated and targets Ubuntu 24.04 / Python 3.12 / boost
1.74. **Closer:** accept the EULA on a *different* machine, read
`install_ryzen_ai.sh` and the bundled wheel metadata, and determine whether the
`voe`/VAIP wheels are pure-Python-version-tagged or hard-linked to 3.12 and boost
1.74. That is a read-only inspection of a downloaded tarball and does not require
installing anything here.

**G7 — whether the VAIML compiler additionally requires a Vitis/Vivado install or
a node-locked AI Engine license.** A search summary asserted a `xilinx.com/getlicense`
requirement; **no primary AMD 1.7.x/1.8.0 page corroborates it** —
`licenses.html` names only three EULAs and says nothing about a license file or
server. **Treat the getlicense claim as unconfirmed. Closer:** read the Linux Third
Party EULA PDF linked from `licenses.html`, or `install_ryzen_ai.sh` (see G6).

**G8 — which `amdxdna_accel.h` Ubuntu's shim compiles against.** `debian/copyright`
shows a **bundled** copy under `xdna/xdna-driver/` at tag 2.21.75, and
`debian/rules` sets `-DXRT_NPU=1`, but no include path or CMake line confirming it
was found. It presumably works with the in-tree driver only because that header and
the in-tree uAPI agree — which the prior audit established, but which **Debian
nowhere asserts or tests** (no `README.Debian`, HTTP 404; `debian/TODO` discusses
only x86 support, edge DKMS and rpath). **Closer:** fetch the orig tarball and grep
`xdna/xdna-driver/src/shim/` for the `#include` of `amdxdna_accel.h` plus the
`include_directories()` in its CMakeLists. Read-only, off-host.

**G9 — whether Ubuntu's XRT 2.21.75 shim actually works against this in-tree
`amdxdna`.** No published source addresses it. Debian's BTS is empty for `src:xrt`.
**Every** published Linux success in this audit used AMD's own XRT (2.23.0 or
2.25.37) plus the driver-displacing plugin. **Treat the Ubuntu-XRT path as entirely
unattested for the VitisAI EP. Closer:** `xrt-smi examine` prints a
firmware/driver compatibility verdict — but that requires the package installed
first, which is outside this audit's read-only scope.

**G10 — no absolute compile-time figure exists for a small BERT on XDNA2.**
**Closer:** one timed AOT compile on a 32 GB build machine. Note this depends on
G6 (does the 1.4-era "no NPU needed" statement still hold in 1.8.0 — it is absent
from 1.8.0 docs).

**G11 — runtime-PM wake latency after the 5 s autosuspend.** Full hardware restart
+ `create_context` + `map_host_buf` + `config_cu` per context, per request, for an
idle-between-requests server. **Unmeasured by anyone; the single biggest unknown
for p99 latency. Closer:** timed back-to-back vs 6-s-spaced inferences once a stack
exists.

**G12 — whether two concurrent ORT+VitisAI-EP processes coexist on Linux.**
**Closer:** run two and read `npu_task_curr` from `DRM_IOCTL_AMDXDNA_GET_INFO`
`resource_info`. Also unmeasured: whether the 16-context ceiling is reachable in
practice and what firmware status code the 17th create returns.

**G13 — the audit read Linux **v7.0 tag** source from `torvalds/linux`, not the
module this host runs** (`7.0.0-28-generic`, Ubuntu-patched,
`srcversion E4FC5AFFF53D21BC6AFDAE3`). Ubuntu could carry deltas to
`drivers/accel/amdxdna`. **Closer:** diff Ubuntu's `linux-source-7.0` (or
`git.launchpad.net/~ubuntu-kernel/ubuntu/+source/linux`) `drivers/accel/amdxdna`
against the v7.0 tag — off-host, read-only.

**G14 — the `AIE2_TEMPORAL_ONLY` conclusion rests on an inference.**
`aie2_check_protocol()` succeeded (the driver is bound), and every matchable npu4
entry contributes the bit — but the **live `feature_mask` and the mailbox protocol
major/minor were not read**, and they are not exposed in sysfs (`fw_version
1.1.2.64` is a *different* version namespace from the `prot_major`/`prot_minor` the
feature table matches on). **Closer:** driver debugfs, or a `dyndbg` trace of
`aie2_check_protocol` — neither run, per the read-only constraint.

**G15 — AIE2P accumulator width and the BFP16-vs-BF16 datapath question.**
**Closer:** the **AIE-MLv2 / AM020** architecture manual, or `aie_api` docs built
for the `aie2p` target.

**G16 — whether Ubuntu will SRU Debian revisions `-5`…`-9` (including the `-9`
xclbin security fix) into resolute.** **Closer:**
`launchpad.net/ubuntu/+source/xrt` publishing history and any open SRU/CVE tracker
entry.

**G17 — Reddit and community.amd.com were never actually reached.** Two
Reddit-targeted queries returned zero reddit.com results; a forum-targeted query
returned GitHub issues instead. **Closer:** query `old.reddit.com/r/LocalLLaMA/search`
and `/r/Amd/search` directly for "NPU embedding", "XDNA bert", "VitisAI linux" with
a 2025-01…2026-08 date filter; and search community.amd.com's Ryzen AI board
directly.

**G18 — whether ORT 1.23 as built by Ubuntu errors or warns on an explicitly
requested unavailable provider.** **Closer:** read
`onnxruntime_inference_collection.py` in the `python3-onnxruntime` source package.

---

## §12 — What this audit does NOT establish

1. **It does not establish that a BERT encoder compiles to this NPU.** The central
   documentary question — what a blank BF16 cell means — is UNVERIFIED, and AMD's
   own documents contradict each other on it. §3.2, G1.
2. **It does not establish that the compiler is obtainable and functional on
   Ubuntu 26.04.** AMD's Linux path targets 24.04/Python 3.12/boost 1.74 and no
   published report describes anyone running it on 26.04 with an in-tree driver.
   §6.2, G6.
3. **It does not establish that Ubuntu's XRT 2.21.75 shim works against this
   in-tree `amdxdna`.** That rests entirely on the prior audit's ioctl-ABI finding
   plus mainline uAPI stability convention — **no vendor, maintainer, or field
   report covers the pairing**. §6.1, G8, G9.
4. **It does not measure anything.** No compile was attempted, no inference was
   run, no package was installed, no module was loaded, no group was changed. The
   only host evidence is §2's read-only observations, all of which predate this
   document.
5. **It does not establish the numerical cost of BF16 for this specific model.**
   The one published encoder-drift number is for a 768-dim BGE on NVIDIA hardware
   in an unrefereed preprint, measured with a different metric than the one
   previously used for the Vulkan comparison. §4.4.
6. **It does not establish that the NPU would be faster.** The only published
   Linux encoder offload result on this silicon family is **slower than CPU**
   (S6), and no partition-boundary cost data exists for a transformer encoder from
   anyone (§3.5).
7. **It does not establish that NPU output is stable run to run.** AMD publishes no
   reproducibility guarantee, and the absence of a guarantee is the finding, not a
   proxy for one. §4.5.
8. **It does not clear the search space.** Reddit and AMD's own community forum
   were never successfully reached (G17), and the §5 absence was already refuted
   once by a repository an obvious query would have surfaced (§9.9.3). Treat the
   remaining absences as "not found by these searches", auditable via the query
   lists in §3.3, §3.5, §4.4, §4.5, §5.2, §7.3.
9. **It does not evaluate the security or maintenance consequences of the
   EULA-gated route**, beyond noting that AMD's plugin displaces the in-tree
   driver, overwrites `/lib/firmware/amdnpu`, and sets the accel device to
   world-read-write `0666`. §6.2.
10. **It does not recommend a course of action.** It establishes that the runtime
    is free and safe to acquire from the distro archive, that the compiler is not,
    and that the technical question the compiler would answer remains open.

---

## §13 — The critic pass, and what it changed

This document was attacked by a dedicated consistency critic after synthesis and
before publication. Recording what it caught is more useful than a document that
reads as if it had always been right — the same principle
`docs/hardware/README.md` states about strike-throughs.

### §13.1 Contradictions found *within* the draft

| # | Conflict | Resolution |
|---|---|---|
| 1 | §0 called the compiler the **"single blocker"**, but §5.1 documents a non-EULA MLIR-AIE path and §8.5 concludes a daemon gets `EACCES` today. | §0 rewritten: **two independent present-tense blockers**, plus explicit acknowledgement that "single" held only under a vendor-path premise §5.1 refuted. |
| 2 | §0 said the runtime half was **"genuinely solved"**; §11 G9 and §12(3) say the Ubuntu-XRT/in-tree pairing is **entirely unattested**. | §0 now says **resolvable, not solved**, and names the ABI finding as a load-bearing assumption rather than a result. |
| 3 | §6.3 said Ubuntu's XRT is enough to "run a pre-compiled artifact"; §7.1 + §9.9.2 say the artifact class §7 describes is an EP-context model that **cannot load without the EP**. | §6.3 now splits the claim by artifact class in a table. True for raw `.xclbin`/`pyxrt`; false for the Vitis AI EP artifact. |
| 4 | §1 promised every finding carries exactly one of five status classes; the body uses five more. | Recorded rather than tidied — see §1. The extra labels are more informative than the classes they refine. |

### §13.2 Host-state claims that were not actually measured

The critic's most serious category: three assertions presented as host facts that
§2 never established. All three are now **VERIFIED-HERE**, measured after the
critic flagged them.

1. **boost — the important one.** §6.2 listed "boost **1.90**" among this host's
   properties. It is the **archive** version, not installed state. Measured: this
   host has **no `libboost-filesystem` at any version**. Corrected in place in
   §6.2 with the withdrawal noted, because this is the **third** time a libboost
   figure has been asserted without measurement in this project — after the
   `1.74.0` doc-prose figure and the `1.83.0` "first-hand" attribution. The
   pattern is not coincidence: libboost versions appear in vendor prose, in deb
   control fields, and in archive metadata, and all three read like host facts
   at a glance. **A libboost version must always name which of the three it is.**
2. **RAM.** §8.2's `22.58 GiB` was inherited from prior session notes. Measured;
   it is correct.
3. **systemd version.** §2 listed `259 (259.5-0ubuntu3)` as VERIFIED-HERE without
   it having been run in this audit. Measured; it is correct.

Two of three were right. That is exactly why the category matters — a rule that
only fires on wrong claims cannot be trusted to fire at all, and the one that was
wrong was wrong in the direction of an argument the document was making.

### §13.3 Questions the audit did not fully answer

Recorded as gaps rather than quietly dropped:

- **Q1 — the target graph is never inventoried.** §3.1 asserts which operators "a
  BERT encoder needs" without enumerating `bge-small-en-v1.5`'s actual ONNX
  graph. At minimum `Mul`, `Pow`, `ReduceMean`, `Cast`, `Unsqueeze`, and the
  `Expand`/`Where`/`Equal` attention-mask construction go unchecked against the
  1.8.0 table, as does the `Tanh` pooler that §5.3 shows AMD's own example reads
  via `outputs[1]`. **The audit's central question is about one specific graph
  and never enumerates that graph.** Closing it needs only an ONNX export and
  `onnx.checker` — and there is **no ONNX file anywhere on this host**
  (VERIFIED-HERE, `find ~ -name '*.onnx'` → empty; `~/models` holds four GGUFs
  only). The export is an unbuilt prerequisite, not an assumed starting point.
- **Q1 — "24–37 partition boundaries"** (§3.5) carries no status class and no
  derivation, in a section that declares the quantity ABSENT. Treat it as an
  order-of-magnitude guess, not a finding.
- **Q2 — no decision criterion.** §4.4 concludes BF16 vectors are "not drop-in
  comparable with an f32-built index" but never states what cosine or NDCG
  deficit would be *acceptable*, nor the cost of rebuilding the index on a new
  backend. Given that this project already pre-registers numeric bands before
  measuring (`docs/acceptance/composed-error.md`), **that band should be
  registered before any NPU number exists**, not after. This is the same
  discipline, and the same reason.
- **Q6 — power and thermal.** §8.4 flags wake latency but nothing addresses the
  cost of NPU wake-per-request on a battery-powered handheld that suspends
  constantly.

### §13.4 Read-only discipline

The critic found the document essentially clean: `SupplementaryGroups=render`
(§8.5) is described and explicitly not recommended, and §11's closers are
disclaimed. One bare imperative — "Set `cache_dir` explicitly" (§7.1) — has been
conditionalised. No command in this document installs, builds, loads a module, or
changes group membership.
