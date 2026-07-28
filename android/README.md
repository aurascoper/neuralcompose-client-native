# Android

## Now: Kotlin bindings, host-JVM tested

`core/` is a host-JVM harness proving the UniFFI Kotlin bindings without any
Android SDK: JNA loads the mac `cdylib` from `../../target/debug` and
`Gate4Test.kt` asserts the same Gate 4 sequence as the Rust and Swift suites.

```sh
cargo build --features uniffi -p neuralcompose-mobile-core   # host dylib
./scripts/gen-bindings.sh                                     # regenerate bindings
cd android/core && gradle test
```

## Later: Compose shell (blocked on Android SDK install)

The Jetpack Compose shell reuses the exact same generated bindings; only the
native library packaging changes:

```sh
cargo install cargo-ndk
rustup target add aarch64-linux-android x86_64-linux-android
cargo ndk -t arm64-v8a -t x86_64 \
  -o android/core/src/main/jniLibs \
  build --release --features uniffi -p neuralcompose-mobile-core
```

Shell responsibilities (per the architecture): microphone permission,
recording/playback, foreground/background lifecycle, OkHttp/ktor WebSocket
creation + cancellation, persistent files, notifications, Compose rendering.
Feed raw WS text frames + `SystemClock.elapsedRealtime()` into
`StreamMonitor`; render purely from `Presentation`/`ChannelSnapshot`.
Never derive stream health from socket state alone.
