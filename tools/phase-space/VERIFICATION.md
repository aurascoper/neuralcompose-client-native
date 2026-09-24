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

## Follow-up: what the evidence-class guard asserts

Inspected and rerun on 2026-09-23 after the recorded benchmark. The guard returned
exit status 1, solely for sibling checkout revision equality. It checks
`neural-memory-server`, defaulting to `~/src/neural-memory-server` with a
`NEURAL_MEMORY_SERVER_DIR` override. It does not check `neuralcompose-eeg-lejepa`
or require the client and server repositories to have the same HEAD.

| Assertion in `scripts/check-evidence-class-drift.sh` | Observed result |
| --- | --- |
| Hash the source file at the fixture's historical upstream commit and compare with `upstream.fileSha256` | Passed |
| Parse the current working file's `EvidenceClass` variant names and compare with `upstream.evidenceClasses` | Passed |
| Require the sibling's current HEAD to equal `upstream.commit` | Failed: `127c109` differs from `5da4a5c` |

An independent SHA-256 comparison found the same digest for the historical file,
the current HEAD file, the working file, and the fixture:

`6f4f06a0014cf8f3b16ecfc700018f88c0d2cb31f238f6fc4027b69cdde75255`

The sibling checkout was clean. The guard itself does **not** compare the current
whole-file hash with the historical hash; its current-file comparison extracts
variant names. This independent hash check establishes the stronger content
equality for this inspection.

The [provenance ADR](../../docs/architecture/decision-log/ADR-004-provenance-vocabulary.md)
describes preventing silent vocabulary drift and a local developer gate. The
script additionally requires the checkout to remain at the dated source revision.
That requirement is stronger than content agreement: an unrelated upstream commit
can fail it. Thus the failure is correct under the script's literal rule, but does
not establish enum drift. Its suggested `checkedAgainstCommit/checkedOn` fields
also do not exist in this fixture; the actual fields are `upstream.commit` and
`upstream.readOn`.

If the intended contract is vocabulary compatibility, a future change should
report historical evidence integrity, current vocabulary compatibility, and
checkout revision difference separately. Keep the original source pin, fail when
its source is unavailable or inconsistent, and do not rewrite it just to obtain a
passing check. No guard, mapping, or fixture change was made by this review.

Separately, local `git remote` for `neuralcompose-eeg-lejepa` returned `origin`.
The premise that this checkout has no configured remote is not current. This
inspection contacted no remote and read no EEG-derived data from that repository.

## Follow-up, 2026-09-24: guard fixed in `81449c3`

The entries above record the guard as it was on 2026-09-23. They are unchanged.
On 2026-09-24 the guard stopped failing on a moved commit alone:

- `c942e51` re-pinned `upstream.commit` to `127c10987ae9b0aef58f9e354dd040ab4432a358`,
  on `neural-memory-server` `main` and `origin/main`. This departs from the
  "keep the original source pin" suggestion above, by the owner's decision.
  `5da4a5c` stays recorded as the first read in `provenance.rs`.
- `81449c3` hashes each pinned file at the recorded commit and at HEAD.
  It adds `agentWritableClasses.fileSha256` for `write.rs`, which had no check.
  A moved commit with identical bytes passes with a NOTE.
- The next commit makes a hash failure name the line range and what to re-read
  there. It prints the checkout's branch next to the commit.

Controls, each on a scratch copy of the fixture or a scratch clone of the server:

| Control | Result |
| --- | --- |
| Record at `5da4a5c`, identical files | exit 0, NOTE: pinned files are byte-identical |
| `write.rs` edited at HEAD | exit 1, `write.rs changed since 127c109…` |
| Wrong `write.rs` hash in the record | exit 1, record digest mismatch |
| Pinned commit absent from the checkout | exit 1, `… is not in …/neural-memory-server` |
| `humanDecision` renamed in the record | exit 1, enum diff printed |

The real checkout passes: exit 0, `main` at `127c109`.
