# Native phase-space viewer

An opt-in Linux Rust/wgpu window for four-channel EEG and live dialectic events.
Geometry represents measured signal relationships. It does not identify focus,
intent, coherence of thought, or a cognitive state.

```sh
cargo run -p neuralcompose-viz -- --demo
cargo run -p neuralcompose-viz -- --demo --mapping delay
cargo run -p neuralcompose-viz -- --eeg-url ws://127.0.0.1:8788/api/eeg/stream
cargo run -p neuralcompose-viz -- --replay /path/session.eeg.jsonl
```

Replay requires the matching `session.eeg.manifest.json` and passes the existing
capture verifier before playback. It preserves receipt gaps. The UI labels demo,
replay, live, stale, reconnect, and paused states. Pause stops the display only.
No missing live input is replaced with synthetic data.

## Signal geometry

Cross-channel coordinates are X=AF7, Y=AF8, Z=TP9; TP10 controls color. Delay
coordinates use one selected channel at t, t+τ, t+2τ. The default is AF7 with τ=16
samples at 256 Hz (62.5 ms). The window holds 1,280 samples. A rolling mean is
removed; the user sets a common µV/unit scale (default 250). There is no per-axis
automatic rescaling. Out-of-range values are counted. Invalid vertices and time
gaps break strips; no segment crosses a connection generation.

Drag to orbit, scroll to zoom, and use Reset view to restore the camera. The
continuous strip uses additive HDR blending, age decay and a bloom composite.
Alpha/(alpha+theta) from the latest contiguous 512 samples controls the optional
particle atmosphere. This is a band-power ratio, not a confidence or health
score. Missing or zero band power has no atmosphere. Channel RMS remains visible.

## Live dialectic feed

Add `--visualization-socket /tmp/neuralcompose-visual.sock` to an otherwise valid
`neuralcompose-hypnagogic --mode dialectic ...` invocation. Start the viewer with
that same flag. The producer owns the 0600 Unix socket and refuses to replace an
existing path. A crashed process can leave a socket; remove it only after checking
that its owner is no longer running.

The wire protocol is newline-delimited `VisualPacket` JSON with schema
`neuralcompose.dialectic.visual.v1`, session and sequence. Each packet is a complete
snapshot, so skipped updates do not require reconstruction. A one-slot wake channel
and atomic latest snapshot keep socket I/O off the generation thread. A slow
consumer can skip intermediate stages. It cannot apply backpressure to generation.
The viewer bounds packets at 4 MiB and keeps sixteen turns, showing the latest four.
Embeddings remain ephemeral. Existing turn-log and FFI formats are unchanged.

Position encodes turn order and role only. Edges carry computed cosine; hover shows
nine decimal places and candidate text. Missing, zero, nonfinite, or incompatible
embeddings have unavailable comparisons. Candidate weights describe the existing
selection policy, not confidence about human intent. The foreground displays the
actual final reply, including silence forced by the repetition guard.

`--context-log /path/session.turns.jsonl` verifies `session.manifest.json` and shows
the last sixteen entries as text and scores. Historical logs contain no embeddings:
they never enter the similarity graph. A disconnected live feed is marked as cached
context. No EEG-to-language causation or time alignment is implied by layering.

## Validation and limits

The recorded protocol and artifact commands are in [tools/phase-space](../../tools/phase-space/README.md).
Unit/integration tests exercise gaps, delay coordinates, absent similarity, stream
generations, and observer parity. The parity test pre-fills the bounded channel,
holds the consumer behind a barrier, requires generation to finish before release,
and asserts the full-channel branch ran. It also checks the landed drift prompt,
heard-first embedding, draws, speech, reply history, and serialized logs.

Linux Vulkan is the runtime target for this work. Metal, Windows, hardware EEG, and
end-to-end speech/model execution need separate runtime evidence. The observer tests
use deterministic seam implementations; renderer timing does not validate a model,
clinical claim, or EEG interpretation.
