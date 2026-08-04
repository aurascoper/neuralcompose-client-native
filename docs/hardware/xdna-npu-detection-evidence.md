# XDNA 2 NPU — detection evidence, ok-cyberdeck

**Date:** 2026-07-30 · **Host:** GPD G1617-02, Ryzen AI 9 HX 370
**OS:** Ubuntu 26.04 LTS (resolute), kernel 7.0.0-28-generic

This records what is verifiable **read-only, today, on the development installation**. Nothing was
installed, no module was loaded or built, no group membership was changed, no partition was touched.

> **This document claims no support-matrix row — not even `Contracted`.** See §5.

---

## 1. What is verified

| Fact | Evidence |
|---|---|
| NPU present | `c5:00.1 Signal processing controller [1180]: AMD Strix/Krackan/Strix Halo Neural Processing Unit [1022:17f0] (rev 10)` |
| sysfs identity | `vendor=0x1022 device=0x17f0 revision=0x10` |
| Driver bound | `/sys/bus/pci/devices/0000:c5:00.1/driver → /sys/bus/pci/drivers/amdxdna` |
| Driver is **in-tree** | `filename: /lib/modules/7.0.0-28-generic/kernel/drivers/accel/amdxdna/amdxdna.ko.zst` · `vermagic: 7.0.0-28-generic` · `srcversion: E4FC5AFFF53D21BC6AFDAE3` |
| Module loaded | `amdxdna 172032 0`, with `gpu_sched` (shared with `amdgpu`) and `amd_pmf` dependent |
| Device node | `/dev/accel/accel0`, `crw-rw----+ root render`, char major 261 |
| **Accessible without a group change** | POSIX ACL `user:aurascoper:rw-` (see §3) |
| Firmware present and revision-matched | `/lib/firmware/amdnpu/17f0_10/` — directory name is `<device>_<revision>` in hex, and the device reports `0x17f0` rev `0x10` |
| IOMMU | group 24 |

Firmware digests, for later provenance:

```
8a48e929a02e8e6982a2fa4710826015322b3ba26d97f4c18a5a435eb3d59608  17f0_10/npu.sbin.1.1.2.64.zst
dd7a0320ff119ae883e0a120886f928708095752bd31cd38f3ba72e0b82513d8  17f0_10/npu.sbin.1.0.0.63.zst
```

Two firmware images are staged for this revision.

**RESOLVED 2026-08-03 — `npu.sbin.1.1.2.64` is the image in use.** The kernel logged the request
at boot, and the requested name resolves through a symlink:

```
kernel: amdxdna 0000:c5:00.1: [drm] Load firmware amdnpu/17f0_10/npu_7.sbin
npu_7.sbin.zst -> npu.sbin.1.1.2.64.zst      <- loaded
npu.sbin.zst   -> npu.sbin.1.0.0.63.zst      <- staged, not requested
```

`1.0.0.63` is the target of the *generic* `npu.sbin.zst` name, which this driver never asks for. It
is present but unused.

The original blocker was a bad inference, and the correction is the durable part: **`dmesg_restrict`
gates the `dmesg` syscall, not the journal.** `systemd-journald` ingests the kernel ring buffer
independently and grants read by group, and this user is in `adm`, so `journalctl -k -b` returns the
whole boot log. The question was answerable read-only the entire time, with no install and no group
change — it was filed as unanswerable because only one instrument was tried.

## 2. The BDF matches AMD's own documentation sample

AMD's Linux install page publishes this `xrt-smi examine` output:

```
|BDF             |Name                |Architecture  |Topology  |
|[0000:c5:00.1]  |NPU Strix           |aie2p         |6x8       |
```

This machine reports the NPU at **`0000:c5:00.1`** — the same BDF. Suggestive that the documented
configuration matches this hardware class, but it is a coincidence of enumeration order, not
evidence of anything working. `aie2p` and `6x8` remain **unverified** here; confirming them requires
`xrt-smi`, which is absent.

## 3. Access does not require a group change

```
$ ls -l /dev/accel/accel0
crw-rw----+ 1 root render 261, 0 /dev/accel/accel0
                          ^ the "+" is an ACL, not a mode bit

$ getfacl /dev/accel/accel0
user::rw-
user:aurascoper:rw-      <-- granted by systemd-logind to the active seat
group::rw-
mask::rw-
other::---

$ python3 -c "import os; os.close(os.open('/dev/accel/accel0', os.O_RDWR))"
(no error)
```

The user is **not** in `render` (`groups` = adm cdrom sudo dip plugdev users lpadmin lxd), yet the
device opens. logind grants the ACL to the active session. This matters because it means NPU work
needs no `usermod`, and the standing prohibition on group changes is not a blocker.

Caveat: a logind-granted ACL is **session-scoped**. It will not be present for a system service, a
cron job, or a non-seat login. Anything long-running must account for that rather than assume the
device is reachable.

**This is a constraint on deployment shape, not a permissions detail to fix later.** If any part of
the NeuralCompose architecture runs as a system service rather than inside a seat session,
`/dev/accel/accel0` is simply not there for it. Re-confirmed 2026-08-03: the ACL entry
`user:aurascoper:rw-` is the *only* thing granting access, and the user is still not in `render`.
The options — run inside the seat session, add a udev rule, or add the group — are architectural
choices to make deliberately, and the latter two are outside this machine's standing prohibitions.

## 4. What is missing

| | |
|---|---|
| `xrt-smi` | absent |
| `xbutil` | absent |
| `onnxruntime` (Python) | absent |

**All three are userspace.** The kernel side is complete and bound. This is the entire gap between
where the machine is now and enumerating the device.

## 5. What is *not* claimed

The 10-rung protocol ladder describes `Detected` and `KernelBound` as rungs. **They are hardware
facts, not support-ladder rungs**, and they do not entitle any row in
`neuralcompose-client-native/docs/support-matrix.md`.

That table's lowest rung, `Contracted`, means *"Schema and deterministic Rust behaviour exist."* For
XDNA, **neither exists**: there is no runtime target, no `backend_id`, no tests, and no ONNX-ABI
embedding pack fixture anywhere in the repo. `attained_support_status()` would return `None` for
this evidence, since `contracts_and_tests_pass` is false.

Specifically not claimed:

- that the NPU can execute anything;
- that `aie2p` / `6x8` is the topology (unverified — needs `xrt-smi`);
- that AMD's XRT userspace is compatible with this **in-tree** `amdxdna` rather than the DKMS module
  AMD's own installer builds;
- that a BF16 encoder can be *compiled* on Linux at all (see §6);
- that any embedding model has ever run on this NPU under Linux — no such first-party example
  exists.

## 6. Two open vendor questions

**Q1 — XRT userspace against the in-tree driver.** AMD's supported Linux path installs
`xrt_plugin….amdxdna.deb`, which DKMS-builds its own `amdxdna.ko`. This host already has the
mainline in-tree module (upstreamed in Linux 6.14; this kernel is 7.0). Whether AMD's XRT userspace
speaks the same ABI to the in-tree driver is **an assumption, not a fact**. It is the first thing to
test, and it must be recorded as an assumption in whatever it gates.

**Q2 — can a BF16 encoder be compiled on Linux?** AMD's own documentation contradicts itself:

- `relnotes.html`: *"Model generation is not supported on Linux in this release."* and *"Models
  generated on Windows are compatible with Linux."*
- `linux.html`: presents a compile-and-run flow, and lists *"NLP models (e.g., BERT, encoder-based)
  in BF16"* as supported.

The narrower `llm_linux.html` wording suggests the relnotes sentence concerns the OGA/LLM flow only,
but it is written unqualified. Unresolved. If compilation is Windows-only, the EP context must be
produced on the Windows 11 install (`nvme0n1p3`) and carried across, with the compiling host
recorded in the artifact provenance.

**RESOLVED 2026-08-03** by `~/outputs/xrt-userspace-intree-amdxdna-audit.md` §3.2, and it splits
cleanly rather than resolving one way:

- **`model_generate` (the LLM / OGA flow) is Windows-only** in 1.8.0. That is what the relnotes
  sentence covers.
- **The Vitis AI EP / VAIML compiler, which handles BF16 encoders, runs on Linux.**

**So the path NeuralCompose actually needs is Linux-native.** An embedding encoder is all
client-native has ever run. Windows remains a dependency only for the LLM/OGA flow — which is where
the 8–14B NPU evaluations live, so it is not a dependency of *this* product.

**A structural consequence stated here on 2026-08-03 is withdrawn.** I argued that if compilation
were Windows-only, the Windows 11 install on `nvme0n1p3` would be a build dependency rather than a
legacy partition, and that this settled the G7 survival audit. **The premise is false for the
encoder path, so the conclusion does not hold.** The 1.25 TB reclamation question stays open on its
own merits rather than being decided by toolchain necessity.

**Sequencing: Q1 before Q2** — retained, and now sharpened by §2.3. Q1 is answerable by installing
XRT userspace and seeing whether it enumerates against the in-tree module; Q2 required attempting a
compile. But **§2.3 finds that installing AMD's `.deb` displaces the in-tree driver**, so the ABI
question is only load-bearing if XRT userspace can be installed *without* the driver package. That
is answerable from the `.deb` contents without installing anything, and it should be settled first —
otherwise §1's compatibility result describes a configuration nobody will be in.

**Also note:** AMD's 1.8.0 Linux page no longer states an Ubuntu version at all — the requirements
table was removed. `24.04` survives only inside `.deb` filenames. ~~and the
`libboost-filesystem1.74.0` dependency~~ — **CORRECTED 2026-08-03: that is documentation prose only.
The shipped `.deb`s declare `1.83.0`.** The explicit requirement (Ubuntu 24.04 LTS, kernel ≥ 6.10,
32 GB RAM, Python 3.10.x) exists only in the archived 1.6.1 page; 1.8.0 names Python 3.12. Cite
1.6.1 for the version claim — 1.8.0 only implies it.

> The `1.74.0` error is worth keeping visible rather than deleting, because of *how* it survived.
> It was written here as a finding, then carried into the audit prompt as a **stated premise** —
> and a premise is background, not a claim, so the audit verified the host facts it was asked to
> verify and left the packaging facts alone. **A claim crossing from one artifact into the next
> stops being audited unless it is marked as still-unverified when it crosses.**

## 7. Reproducing this

```sh
lspci -nn -s c5:00.1
readlink -f /sys/bus/pci/devices/0000:c5:00.1/driver
modinfo amdxdna | grep -E '^(filename|srcversion|vermagic):'
getfacl -p /dev/accel/accel0
ls -l /lib/firmware/amdnpu/
python3 -c "import os; os.close(os.open('/dev/accel/accel0', os.O_RDWR)); print('open ok')"
```

Raw capture: `scratchpad/xdna-evidence-raw.txt`.
