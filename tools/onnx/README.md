# `tools/onnx/` — graph inventory for the XDNA 2 feasibility audit

These four scripts produce §14 of
`docs/hardware/xdna2-bert-encoder-feasibility.md`. They are committed so the
operator inventory is reproducible rather than asserted — the same reason
`composed-error.stdout.txt` is in the repo.

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
