# Phase-space renderer engineering comparison

This protocol measures rendering on one named host. It does not test physiology,
cognitive state, user comprehension, or treatment efficacy.

## Fixed execution

Twelve cases: rhythmic alpha/theta, seeded broadband noise, and constant signal
with impulses; each in cross-channel and delay coordinates; each trace-only and
full layered composition. Both variants receive identical synthetic semantic
updates with 384-dimensional fixture embeddings, two candidates per turn,
and a new turn every five seconds. Run three complete rounds in recorded shuffled order (seed 20260923 +
round). Each run starts a fresh process, warms for five seconds, then measures
60 seconds. No extra or replacement runs. Total timed execution: 39 minutes.

Use a clean committed checkout and a release build from that same checkout.
The executable embeds its build commit and clean-state observation. Both the
runner and executable check the working tree. Record binary SHA-256, Cargo.lock,
rustc/profile, display configuration, GPU/driver, order, and input/configuration.
Write all output outside the checkout. Dirty builds or runs are diagnostic and
non-quotable. Aborted, incomplete, missing, or non-quotable runs leave their case
unresolved; they are not replaced. Retain every failure and every valid result.

## Measurements and decisions

Frame intervals are elapsed monotonic time between actual phase-scene GPU queue
submissions. They are not GPU execution times or physical scan-out times.
Sample-to-submit age is the same submission time minus the viewer's monotonic
receive time of the newest accepted EEG frame. It is not sensor-to-screen latency.
Use nearest-rank p95 separately for each run. Do not pool the three runs' frames.
Report every run and min/median/max of each case's three run-level p95 values.

Both targets must hold in a run: p95 frame interval <= 20 ms and p95
sample-to-submit age <= 100 ms. A case with three qualifying completed runs is
accepted at 3/3, targets not met at 0/3, and unresolved at 1/3 or 2/3. All three
must be present and quotable for a verdict other than unresolved. A follow-up
requires a separately specified protocol; it does not erase this result.

Guardrails: bounded data/history buffers, zero ingestion loss caused by the
renderer, stale-source labeling, no false continuity across gaps/generations,
and no invented semantic distances. Selection of a display mode cannot change
model calls, prompts, embedding order, draw count, outcomes, or persisted logs.

These are descriptive host measurements. Human allocation, significance tests,
and confidence intervals are not applicable. Three repeats describe observed
spread; they do not establish a universal latency guarantee.
