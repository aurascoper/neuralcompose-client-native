# Pre-recording verification

Source base: `b388329` (`origin/feat/hypnagogic-linux`). The drift anchor and reply
repetition guard are already present in this base. Work is isolated in
`codex/phase-space-viz`; the user's original checkout is not the test source.
The benchmark manifest will name the clean implementation commit that actually ran.

| Command/check | Observed result |
| --- | --- |
| `cargo test -p neuralcompose-mobile-core -p neuralcompose-hypnagogic -p neuralcompose-viz --lib --tests --quiet` | 430 passed, 0 failed, 0 ignored |
| `cargo test --workspace --all-features --lib --tests --quiet` | 453 passed, 0 failed, 0 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all -- --check` and `git diff --check` | Passed |
| `python3 -m unittest discover -s tools/phase-space -v` | 2 passed |
| `scripts/check-fixtures.sh` | No drift |
| `scripts/check-binding-drift.sh` | No drift |
| `scripts/check-env-fixture-drift.sh` | No drift |
| `scripts/check-evidence-class-drift.sh` | Failed on sibling checkout revision; see below |

Counts name their commands and feature scope. They do not reconcile the historical
202 figure. They are not a replacement attribution for the earlier 397 baseline.

The full-channel observer test passed with the consumer behind a barrier and an
assertion that the full branch ran. Replacing `try_send` temporarily with blocking
`send` made that test fail at “generation waited for the stalled consumer.” The
original source was restored, then all three observer tests passed again. The
fixture exercises heard-first embedding, drift framing, selection draws, repeated
reply suppression, speech calls, and byte-identical serialized turn logs.

A private Unix socket round trip passed, including permissions, refusal to replace
an existing socket, JSON decoding, and owned-path cleanup. Live model and speech
execution were not run: `LLAMA_CPP_DIR` was unset, so the native model crate used
its explicit unavailable stub. The deterministic observer tests use seam fixtures.

Two non-quotable rendering smokes completed with actual scene submissions:
AMD Radeon 890M / RADV Vulkan, and llvmpipe Vulkan under Xvfb. The hardware screenshot
was inspected for trace, band atmosphere, chronological role nodes, cosine edge
labels, and final text. These smokes are separate from recorded measurements.

The empty-template preview was rendered with LibreOffice and inspected. It exposed
stale original formula caches; the generator now removes all example values and
shared formulas before inserting engineering content. Missing results remain
“Not measured.” Template originals are unchanged. The generated report retains its
sections and table roles; the workbook retains its chart layout and hidden helpers.

The evidence-class guard failure predates this change. Its fixture is unchanged
from the source base and pins neural-memory-server `5da4a5c5ab509e22bc9eb72dbeb6d3fac835606b`.
The local sibling checkout is clean at `127c10987ae9b0aef58f9e354dd040ab4432a358`.
`crates/neural-memory-domain/src/terms.rs` is byte-identical between those revisions,
and the guard's enum comparison passed. Its revision equality check failed. This
work does not advance that independent provenance record or claim the guard passed.
