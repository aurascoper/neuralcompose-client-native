# ADR-006: Opt-in cloud generation — one seam leaves the machine, and the record says so

- Status: Proposed
- Date: 2026-08-14
- Supersedes: nothing. Extends ADR-004's vocabulary to the generation path.

## Context

Every seam in `neuralcompose-hypnagogic`'s shell talks to `127.0.0.1` or to a
subprocess on this machine: `llama-server` on the loopback, whisper as a
subprocess, Kokoro as a subprocess, the embedder in-process. `tools/spoken-loop/`
goes further and *hard-refuses* a non-loopback endpoint — "cloud endpoints are
forbidden" is a literal check in `generate.py`.

The Swift, however, has shipped a cloud generator since Stage 5:
`Sources/BCICloudBridge/ClaudeCLIGenerator.swift` drives a Claude model through
the local `claude` CLI, quarantined in its own module so the on-device boundary
contract could name what it excluded. This crate's own `dialectic.rs` header has
always said "the generator may be a cloud model; speech-to-text stays on-device,
so only text ever leaves the machine" — a sentence describing a capability that
did not exist on Linux.

This ADR ports it, and states why one seam is allowed to cross a line the rest
of the codebase holds.

## Decisions

### 1. The CLI is the transport, not an API key and an HTTP client

`claude -p` carries the user's own subscription auth. There is no key to store,
no key to leak from a config file, and no HTTP client added to this binary — the
process boundary is the credential boundary, the same reason the Swift chose it.

The precedent is already here: `dialectic-relay/codex_adapter.py` shells out to
`codex exec --sandbox read-only` rather than embedding a client.

**This does not weaken `generate.py`'s refusal.** That check governs the
llama-server endpoint: a *loopback* URL silently pointed at a cloud host would
be a session whose provenance says "local" and whose text was not. Naming a
different generator is the opposite — it is the crossing declared.

### 2. It is opt-in at every layer, and never a default

`--generator claude`, or `GENERATOR=claude` in the launcher. The default is
`llama` everywhere: in `parse_args`, in `DialecticConfig::default`, and in
`tools/launcher/neuralcompose-session`. Nothing in this change alters a single
byte of the local path's behaviour.

The binary prints what leaves before the first turn, at a volume that can be
read and cancelled — transcripts leave, audio and EEG do not.

### 3. `--tools ""` — a text-generation seam does not get a shell

`claude -p` is Claude Code. Without the flag it can read and write files and run
shell commands in whatever directory it was spawned in. The argv passes
`--tools ""` to disable the built-in set, and the subprocess runs in the
session's temp workdir rather than the invoking directory, so no `CLAUDE.md`
beside the session can be discovered and folded into a hypnagogic prompt.

This is the one place the port **diverges from the Swift**: the flag did not
exist when `ClaudeCLIGenerator` was written. `claude_cli::tests::
every_built_in_tool_is_disabled` asserts it separately from the full-argv test,
because a careless argv edit could drop it and every other test would still pass.

### 4. The turn record names the generator, readably and in the digest

`dialectic_method_identity` now takes the generator id and folds it into
`parameters_digest`, and `turn_envelope` names it as a `ResourceRef` of kind
`textGenerator` in `provenance.inputs`, with the id in `locator`.

Both, not either. The digest makes two sessions with identical profiles and
identical tuning *different methods* when one ran a 1.7B on this machine and the
other sent the transcript to Anthropic — which is what they are. The readable
locator is because **eligibility here is a query over recorded annotations**
(ADR-005 decision 9), and a query cannot recompute a hash it has no candidate
for.

No schema bump: `inputs` is already "what the method consumed" in
`neuralcompose.hypnagogic.turn.v3`, and it was empty on turn lines because the
candidate embeddings are deliberately not logged. The generator is the one input
a reader cannot recover from any other field.

## Ceilings — do not quietly cross them

**Per-role temperature is gone on this path.** `claude -p` exposes neither
`temperature` nor `max_tokens`. The dialectic's 0.45 coherence pole and 1.0
displacement pole are how the two poles are made to diverge; through this
generator they differ **only** by their system prompts. `GenerationParams` is
accepted and dropped, and the shell says so at startup. A future port that gains
sampling control should say so in a new decision rather than letting the caveat
rot.

**This is slower and costs money.** Measured on this machine, 2026-08-14:
~5.5 s wall per generate call against the local 1.7B's 1.39 s, and $0.01 on a
prompt-cache hit versus $0.20–$0.24 on a miss — the prefix is not stable across
invocations, so misses are not rare. A reflective turn makes three calls.
**Cloud generation is a prose-quality choice, not a latency or cost one.**

**Promotes no support-matrix row.** ADR-002 forbids promotion by implication.
`attained_support_status()` returns exactly what it returned before, and a
session generated off-device is not `RuntimeSmokeValidated` evidence for
anything: it exercises no llama.cpp path at all.

**Nothing about the EEG contract changes.** EEG never reaches a prompt — it is
recorded provenance, not model input (ADR-005 decision 1) — so there is no path
by which a headband's output could leave the machine through this seam. Audio
does not leave either: whisper runs on-device and only its transcript is passed.

**The dialectic is still two-sided only in form.** Both poles remain the same
model under different instructions; a cloud model does not make a turn hold a
disagreement rather than stage one.

## Verification

Each check states what a pass looks like. Run 2026-08-14 on this machine.

1. **The refusal fires before anything starts.** `--generator claude` with the
   CLI absent from PATH. PASS: one sentence naming the cause, non-zero exit, and
   no llama-server contacted. *Observed.*
2. **A real turn, end to end.** `--generator claude --mode mirror --turns 1`
   over stdin, default `claude-sonnet-5`. PASS: a reply in the constrained
   register. *Observed.*
3. **The provenance path.** A `--log` reflective session on the cloud path, then
   `--verify-log` on its output. PASS: every line carries a `textGenerator`
   input whose `locator` is `claude-cli:<model>`, and the log verifies clean.
   *Observed, at `claude-haiku-4-5` to keep the check cheap; the mechanism is
   model-independent.*
4. **The local path is unchanged.** The same session against `llama-server`.
   PASS: `locator` is `llama-server`, log verifies clean. *Observed.*
5. **The gate is proved failing, not only passing.**
   `the_generator_reaches_the_digest_and_separates_local_from_cloud` asserts the
   digest *differs* between generators as well as being stable within one.
   *Green.*
6. `cargo test --workspace` — 34 test binaries, all green.

**Not verified:** the cloud path with `--speak`, `--mic`, or an EEG source
attached. Those seams are orthogonal to the generator and unchanged by this
work, but no run has exercised them together.
