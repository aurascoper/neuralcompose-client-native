# Contracts

Frozen HTTP + WebSocket contract between the NeuralCompose M4 server and all
clients.

**Provenance:** frozen 2026-07-28 from the Expo client at
`NeuralCompose-ios-client` worktree, branch `feat/ios-client`, commit
`886b25f9277d6c63ee5ef44c429cbdd2c6488561` (the commit that passed Gate 4).
Fixture JSON is verbatim from `src/mock/fixtures.ts`; the stub server is the
exact artifact Gate 4 was executed against. Do not "improve" these in place —
contract changes are new versions, coordinated with the server.

- `api-schema/` — JSON Schema (draft 2020-12) describing the shapes. Schemas
  are the *description*; the golden fixtures are the *contract*. Behavioral
  semantics (drop-invalid-entries, stale thresholds, reconnect policy) live in
  the Rust contract tests, not in schema.
- `fixtures/` — golden JSON. Every fixture must validate against its schema
  (enforced by `fixtures_golden.rs`). `eeg-frame-batch-8.json` is generated
  deterministically by `cargo run --example gen_fixtures` (drift-checked in
  CI by `scripts/check-fixtures.sh`).
- `generation-eval/` — the frozen M7-B generation-benchmark corpus.
  `m7b-corpus-v1.json` is 18 sanitized, synthetic prompts: exactly two per
  allowed `BenchmarkTaskKind` × `PromptContextProfile` pair, over the nine-pair
  matrix the protocol permits. **The corpus is owned by Rust**, which
  `include_bytes!`-es this file and derives its identity from its content;
  there is no API anywhere that accepts a corpus digest, so the digest cannot
  be asserted independently of the bytes. `scripts/check-fixtures.sh`
  re-serializes the parsed value and diffs it against this file, which is what
  proves the committed artifact and the compiled value are the same thing.
  Editing a prompt, reordering the list, or changing either taxonomy axis
  changes the corpus identity and therefore the declaration identity — that is
  the intent, not a hazard. The quota is exact, not a minimum: quota shape is
  the macro weighting, so a "small" edit here reweights the study.
- `constants.json` — shared numeric contract (channel order, buffer size,
  staleness thresholds, reconnect policy, WS path). The Rust core's compiled
  defaults are test-asserted equal to this file.
- `stub-server/` — the Gate 4 reference stub: the four HTTP fixtures + the
  `/api/eeg/stream` WebSocket (8-sample batches every 31.25 ms = 256 Hz) with
  outage controls (`/control/pause`, `/control/resume`, `/control/drop?ms=N`,
  `/control/state`).

## Endpoints

| Path | Meaning |
|---|---|
| `GET /api/diagnostics` | `diagnostics.schema.json` |
| `GET /api/health` | `health.schema.json` (4 channels, fixed order) |
| `GET /api/classifier` | `classifier.schema.json` |
| `GET /api/pipeline-mode` | `pipeline-mode.schema.json` |
| `WS /api/eeg/stream` | text frames of `eeg-frame.schema.json` (single sample or batch array) |
