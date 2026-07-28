# neuralcompose-client (native)

Universal native client stack for the macOS NeuralCompose BCI pipeline:

- `crates/neuralcompose-mobile-core/` — **Rust behavioral core** (UniFFI →
  Kotlin + Swift). Owns everything deterministic: EEG frame validation and
  fixed TP9/AF7/AF8/TP10 ordering, batch/single WS decoding, bounded rolling
  buffers, receive-time tracking, stale-state calculation, reconnect policy,
  configuration validation, fixture generation, contract tests.
- `contracts/` — frozen HTTP + WebSocket contract (JSON Schema + golden
  fixtures + the Gate 4 reference stub server).
- `ios/` — Swift + SwiftUI shell (native I/O; renders from core semantics).
- `android/` — Kotlin + Jetpack Compose shell (deferred until the Android SDK
  is installed; Kotlin bindings are JVM-tested on the host meanwhile).
- `web/` — optional React diagnostics client (future).

Shells own all I/O (sockets, permissions, audio, lifecycle) and feed raw
payloads plus **monotonic** timestamps into the core; the core decides what
the I/O *means* (`StreamPhase`: Connecting / OpenNoData / Live / Stale{age} /
Closed / Error) and the shells render it. No Expo, no Metro, no WebView.

Origin: the Expo client (NeuralCompose PR #45, `feat/ios-client` @ `886b25f`)
is the executable specification and parity oracle for this repo. Its Gate 4
acceptance test caught an open-but-silent WebSocket being presented as
healthy — the class of bug this shared core exists to make impossible to
reimplement inconsistently.

## Quick start

```sh
cargo test                                  # core, no FFI
cargo test --features uniffi                # FFI attribute surface
node contracts/stub-server/server.mjs &     # Gate 4 reference stub
cargo run --example gate4_probe             # live phase-transition replay
```
