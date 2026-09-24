# Phase-space measurement and artifacts

The [registered protocol](PREREGISTRATION.md) fixes twelve cases and three runs per
case. It never replaces an aborted run or adds runs to resolve a mixed result.

From a clean committed checkout:

```sh
NC_VIZ_BUILD_STAMP=recorded cargo build --release --locked -p neuralcompose-viz
python3 tools/phase-space/benchmark.py \
  --binary target/release/neuralcompose-viz \
  --output /absolute/path/outside-checkout/new-run-directory
```

The recorded workload takes 39 minutes plus process setup. Keep the checkout,
executable, desktop configuration, and power policy unchanged during execution.
The runner saves order, source revision, executable digest, display/GPU details,
per-run frame intervals and sample ages, and failures. Only clean matching release
builds with a completed measurement span are quotable. `--recorded` disables scene
controls and refuses extra live feeds, screenshots, and output inside the checkout.

A diagnostic smoke is separate and always non-quotable:

```sh
target/release/neuralcompose-viz --demo --warmup 1 --seconds 6 \
  --metrics-out /tmp/phase-diagnostic.json --screenshot /tmp/phase-diagnostic.png
```

Generate copies of the two user-selected Office templates from retained runs:

```sh
python3 tools/phase-space/artifacts.py --runs /absolute/path/recorded-runs \
  --output /absolute/path/artifacts \
  --dashboard-template /path/analytics-dashboard/assets/reference.xlsx \
  --report-template /path/experiment-analysis/assets/reference.docx
```

Template paths default to the installed OpenAI templates plugin location on the
authoring host. Original files are not modified. The workbook retains the dashboard,
input sheet, hidden chart helpers, and four chart placements; it adds a Runs sheet.
The report retains the cover, sections, and table roles. All example values and
formula caches are replaced. No runs produces “Not measured,” never example results.
The JSON summary records template hashes and each case's two p95 spreads.

LibreOffice can render local PDF previews. SinglePageSheets preserves dashboard
layout without splitting charts across paper pages. Extract its first page for a
dashboard-only preview; SinglePageSheets also exports the hidden helper sheet:

```sh
libreoffice --headless --convert-to \
  'pdf:calc_pdf_Export:{"SinglePageSheets":{"type":"boolean","value":"true"}}' \
  --outdir /absolute/path/artifacts /absolute/path/artifacts/phase-space-dashboard.xlsx
pdfseparate -f 1 -l 1 /absolute/path/artifacts/phase-space-dashboard.pdf \
  /absolute/path/artifacts/dashboard-page-%d.pdf
mv /absolute/path/artifacts/dashboard-page-1.pdf \
  /absolute/path/artifacts/phase-space-dashboard.pdf
libreoffice --headless --convert-to pdf --outdir /absolute/path/artifacts \
  /absolute/path/artifacts/phase-space-experiment.docx
```

Inspect the rendered pages and retain the source run files with the documents.
The run revision identifies code that actually ran; a later artifact commit does
not replace that revision. Timing describes this host/build and the synthetic
workload only. Frame interval means scene-submission cadence, and sample age begins
at viewer receipt; neither measures physical sensor-to-screen latency.
