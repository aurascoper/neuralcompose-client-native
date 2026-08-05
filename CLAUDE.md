# neuralcompose-client-native

Rust core + platform shells for the NeuralCompose client. The core is
**deterministic and does no I/O**: it owns contracts, the conformance/support
ladder, model packs, and the recording state machine. Everything that touches a
socket, a microphone, an accelerator or a clock lives in a shell.

The macOS `NeuralCompose` repo is a separate codebase with its own registry.

## Layout

- `crates/neuralcompose-mobile-core/` — no-I/O core, UniFFI-exported to Swift (`ios/`) and Kotlin (`android/`)
- `crates/neuralcompose-llama/` — llama.cpp backend over a hand-written C shim. **Zero dependencies, by design.**
- `crates/neuralcompose-headless/` — Linux CLI shell (`--embed`, `--bench`); owns the socket, clock, retry timer and stdout, nothing else
- `contracts/` — JSON Schema + golden fixtures. Frozen: contract changes are new versions, never in-place edits.
- `docs/support-matrix.md` — the register of what is supported and on what evidence
- `docs/acceptance/` — "does this row earn a rung?" · `docs/hardware/` — "what is this machine?", promotes nothing
- `docs/architecture/decision-log/` — ADRs; **@docs/architecture/decision-log/ADR-002-runtime-targets-property-law-and-conformance.md** is the one to read
- `tools/` — dev tools and demos. Not a product surface, not on any target graph.

```sh
cargo test --workspace
bash scripts/check-fixtures.sh        # golden fixture drift
bash scripts/check-binding-drift.sh   # UniFFI bindings vs core
bash scripts/gen-bindings.sh          # regenerate Swift/Kotlin after a core signature change
```

## The rule everything else serves

**Promotion is never by implication, and compiling is not running.** Each rung of
`Contracted → BuildValidated → RuntimeSmokeValidated → DeviceValidated →
ReleaseSupported` needs its own evidence. `attained_support_status()`
(`crates/neuralcompose-mobile-core/src/runtime_target.rs`) is the machine-checkable
form of `docs/support-matrix.md`, and **a row may never claim more than that
function returns for its evidence.**

A hardware fact — a device enumerates, a driver binds, a firmware image loads — is
not a rung and does not imply one. Neither is a working demo. If you find yourself
arguing a row up from something that merely *sounds* like evidence, that is the
failure mode this repo is built to prevent.

## Conventions that are easy to violate by being helpful

- **Corrections are struck through in place, never deleted.** A withdrawn claim plus the reasoning that withdrew it is worth more than a document that reads as if it had always been right.
- **Records are never retro-edited from later conversations.** A superseded decision gets a new dated record and a pointer; the old text stays as written.
- **Evidence class is not yours to raise.** The MCP path writes `agentInference` only; `observed` and decisions are entered out of band, deliberately.
- **Never commit model artifacts.** `*.gguf`, `*.onnx`, `*.safetensors`, `models/` are gitignored. They are derived from a pinned revision, so they are not evidence and must never enter the ledger.
- **Pin what you cite.** Commit hashes, digests and byte counts, not "the latest".

## Gotchas

- **`git fetch` only tracks `main`.** `remote.origin.fetch` is narrowed to `+refs/heads/main:refs/remotes/origin/main`, so no other branch gets a remote-tracking ref: `git log origin/<branch>` fails and `git status` shows no ahead/behind. Pushes land fine. Widen it if that is not deliberate.
- **Changing a `SupportEvidence` / exported type breaks the UniFFI checksum.** Regenerate bindings; `check-binding-drift.sh` gates it.
- **`neuralcompose-llama` compiles to a stub with `LLAMA_CPP_DIR` unset**, and every entry point returns `Unavailable`. CI sees the stub.
- **A skipped Rust test prints `ok`.** A stub build produced the same "all passed" line as a real backend run. Any run producing acceptance evidence must set `NC_REQUIRE_BACKEND=1`, which turns skips into failures.
- **Models are not in the repo.** Paths are conventionally `~/models/` and `~/.cache/llama.cpp/`.
- **Gitignore rules ending in `/` match directories only**, and git treats a symlink-to-directory as a file. Symlinking a shared venv or model dir past a slashed rule is how both nearly got committed.
- **Never pipe a build or test into `tail`/`head`** — you read the pager's exit status, not the command's. A failed `cmake --build` reported success that way.
