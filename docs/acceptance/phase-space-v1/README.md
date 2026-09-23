# Acceptance record — native phase-space viewer

Recorded on 2026-09-23. All twelve cases met both targets on all three repeats.
This is an engineering result for the synthetic workload on this host.

## Recorded build and conditions

- Source revision: `3745369692fdd9908588316bac212f6e3b171183`.
- Release executable SHA-256: `fcbd203c249400fbffb203ae99bbe5b384666732f91b4d24c21f16e715cbe029`.
- Every run checked a clean checkout and the matching clean release build.
- GPU/driver: AMD Radeon 890M Graphics (RADV STRIX1); Vulkan; radv; Mesa 26.0.8-1ubuntu0.3.
- Scene extent: 2346 × 1091 physical pixels.
- Initial display configuration: 2560 × 1440 at 120 Hz; initial power profile: power-saver.
- CPU and OS details: [CPU](runs/cpu.txt), [manifest](runs/manifest.json).
- Thirty-six fresh processes, each with 5 seconds of warmup and 60 seconds measured.
- Zero aborted, missing, replacement, or non-quotable runs.

The source commit includes the [registered protocol](../../../tools/phase-space/PREREGISTRATION.md).
Artifacts were generated afterward. Their commit does not replace the recorded
renderer revision. No renderer or observer source changed after recording.

## Measured outcomes

The largest run-level frame p95 was 10.43 ms
(target ≤20 ms). The largest run-level receive-to-submit p95 was
40.97 ms (target ≤100 ms). These are maxima of
separate run-level percentiles, not pooled percentiles.

| Case | Frame p95: min / median / max ms | Age p95: min / median / max ms | Verdict |
| --- | --- | --- | --- |
| rhythmic-channels-trace | 9.30 / 9.36 / 10.09 | 40.28 / 40.54 / 40.71 | accepted (3/3) |
| rhythmic-channels-full | 9.00 / 9.67 / 10.03 | 40.60 / 40.61 / 40.64 | accepted (3/3) |
| rhythmic-delay-trace | 9.09 / 9.25 / 9.33 | 40.27 / 40.44 / 40.46 | accepted (3/3) |
| rhythmic-delay-full | 9.07 / 9.31 / 9.63 | 40.48 / 40.58 / 40.67 | accepted (3/3) |
| noise-channels-trace | 9.43 / 9.49 / 9.86 | 40.55 / 40.58 / 40.61 | accepted (3/3) |
| noise-channels-full | 9.14 / 9.17 / 9.90 | 40.51 / 40.51 / 40.82 | accepted (3/3) |
| noise-delay-trace | 9.21 / 9.66 / 10.43 | 40.26 / 40.44 / 40.97 | accepted (3/3) |
| noise-delay-full | 9.09 / 9.35 / 9.38 | 40.36 / 40.62 / 40.67 | accepted (3/3) |
| impulse-channels-trace | 9.36 / 9.44 / 9.51 | 40.47 / 40.53 / 40.56 | accepted (3/3) |
| impulse-channels-full | 9.11 / 9.17 / 9.61 | 40.45 / 40.58 / 40.73 | accepted (3/3) |
| impulse-delay-trace | 9.14 / 9.16 / 9.81 | 40.47 / 40.48 / 40.52 | accepted (3/3) |
| impulse-delay-full | 9.39 / 9.56 / 9.65 | 40.60 / 40.60 / 40.64 | accepted (3/3) |

The audit recalculated every p95 from its retained array and checked run count,
repeat identities, durations, live status, source revision, executable digest,
and constant GPU/scene extent. [Audit and run hashes](runs/audit.json).
Numeric run files are byte-preserved. Archived text transcripts have trailing
whitespace removed.

## Artifacts and verification

- [Dashboard workbook](artifacts/phase-space-dashboard.xlsx) and [PDF preview](artifacts/phase-space-dashboard.pdf).
- [Experiment report](artifacts/phase-space-experiment.docx) and [PDF preview](artifacts/phase-space-experiment.pdf).
- [Machine-readable case summary and original template hashes](artifacts/summary.json).
- [Synthetic scene preview](artifacts/phase-space-viewer.png); this separate screenshot run is non-quotable.
- [Pre-recording verification](../../../tools/phase-space/VERIFICATION.md) and [workspace test transcript](verification/workspace-tests.txt).
- [Blocking-send mutation failure](verification/backpressure-mutation.txt) and [restored observer test](verification/observer-restored.txt).

The workspace/all-features Rust suite passed 453 tests; the artifact checks passed
3 tests. Formatting, Clippy, fixture, binding, environment-fixture, and secret checks
passed. The separate evidence-class drift guard failed only because the sibling
repository's checkout revision differs from its pinned record; the enum source is
unchanged. The provenance fixture was not advanced by this work.

## Evidence boundary

Frame timing is scene-submission cadence. Sample age begins at viewer receipt and
ends after scene queue submission. Neither measures sensor-to-screen latency,
feature-estimator response, or physical scan-out. The alpha/theta estimate uses a
two-second history window. Timing targets do not establish an embodiment threshold.

No hardware EEG, human steering, cognition, treatment effect, or end-to-end speech
and model execution was evaluated. Model seams used deterministic test fixtures;
the native model backend was explicitly unavailable without LLAMA_CPP_DIR. A
separate llvmpipe Vulkan smoke rendered successfully and remains non-quotable.

The [media-theory and steering review](../../phase-space-steering-review.md) records
the subsequent design discussion as proposed work. No mental-state mapping, assisted
attraction, or neurofeedback training was included in these recorded runs.
