# `tools/onnx/` — graph inventory for the XDNA 2 feasibility audit

> **Archival, not exercised. These are one-shot scripts, run once by hand on
> 2026-08-04 in a throwaway venv, and they are not wired into CI and never will
> be.** They cannot be: none of `torch`, `transformers`, `onnx` or
> `onnxruntime` is installed on `ok-cyberdeck` or available to CI, and the
> inputs are two 133 MB files that are deliberately not committed.
>
> This notice exists because a committed script that nothing runs is one rename
> away from being mistaken for a check — and a check that never executes reports
> `ok` while verifying nothing. That failure mode has already appeared twice in
> this repository: `vulkan_agreement.rs` asserting correctly on inputs that never
> triggered it, and `composed_error.rs` reporting `ok` in CI because
> `NC_REQUIRE_QUANT` is never set there.
>
> **The evidence is `docs/hardware/xdna2-bert-encoder-feasibility.ops.txt`**, the
> committed raw output. These scripts are how it was produced, kept so the
> inventory is reproducible rather than asserted — the same reason
> `composed-error.stdout.txt` is in the repo. They are not a gate, and nothing
> should be built on them re-running.

These four scripts produce §14 of
`docs/hardware/xdna2-bert-encoder-feasibility.md`.

| script | what it does |
|---|---|
| `inspect_onnx.py` | opset, ir_version, I/O signature, initializer dtypes and element count, full operator histogram including subgraphs |
| `crosscheck_ryzenai_ops.py` | joins that histogram against the Ryzen AI 1.8.0 `ops_support` table transcribed in §3.1; separates *flagged*, *unlisted*, and structural probes; reports shape dynamism |
| `export_bge_opset17_static.py` | mirrors AMD's `export_bge_onnx.py` — legacy tracer, `opset_version=17`, static `(1,512)`, fp32 weights |
| `compare_exports.py` | mean-pools + L2-normalises both exports through `onnxruntime` CPU and reports cosine / `max_diff`, establishing that the operator differences are export form and not a different network |

**They need a venv** — none of `torch`, `transformers`, `onnx` or `onnxruntime`
is installed on `ok-cyberdeck`, and installing them system-wide is not required:

```sh
python3 -m venv /tmp/onnxenv
/tmp/onnxenv/bin/python -m pip install onnx onnxruntime transformers \
    torch --index-url https://download.pytorch.org/whl/cpu
/tmp/onnxenv/bin/python tools/onnx/inspect_onnx.py ~/models/onnx/bge-small-en-v1.5.onnx
```

**Nothing here touches the NPU, the driver, or the device node.** These are
graph-reading scripts; the audit's read-only discipline is unaffected.

The canonical artifact is `BAAI/bge-small-en-v1.5` file `onnx/model.onnx` at
revision `5c38ec7c405ec4b44b94cc5a9bb96e735b38267a`,
sha256 `828e1496d7fabb79cfa4dcd84fa38625c0d3d21da474a00f08db0f559940cf35`.
The `.onnx` files themselves are **not committed** — 133 MB each, and both are
reproducible from the revision above.
