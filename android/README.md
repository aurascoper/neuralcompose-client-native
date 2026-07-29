# Android

## Compose shell (`app/`) — M5-B

Self-contained Gradle project (own wrapper pinned to **8.14.3** — AGP 8.13
does not run on Gradle 9; never build the app with the system gradle). It
includes the generated UniFFI bindings via
`sourceSets["main"].kotlin.srcDir("../core/src/main/kotlin")` and depends on
`jna:5.17.0@aar` (the plain jar lacks Android `libjnidispatch.so`).

```sh
# One-time SDK bootstrap (headless, no Android Studio):
brew install --cask android-commandlinetools
export ANDROID_HOME=/opt/homebrew/share/android-commandlinetools
yes | sdkmanager --licenses
sdkmanager "platform-tools" "platforms;android-35" "build-tools;35.0.0" "ndk;28.2.13676358"

# Native lib (NOTE: jniLibs lives under app/, not core/):
cargo install cargo-ndk --locked
rustup target add --toolchain 1.97.1 aarch64-linux-android
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs \
  build --release --features uniffi -p neuralcompose-mobile-core

# Build + install on a connected device:
cd android/app
echo "sdk.dir=$ANDROID_HOME" > local.properties
./gradlew installDebug
adb reverse tcp:8787 tcp:8787     # device 127.0.0.1:8787 -> host stub
```

Device-testing gotchas (all observed on a Pixel 8a):

- **`adb reverse` does not survive USB re-enumeration** (lock/unlock cycles
  can drop it). If the app shows ERROR with 0 samples, check
  `adb reverse --list` first.
- **OkHttp `pingInterval` is load-bearing**: without it, a socket that dies
  while the process is frozen (screen off) becomes a believed-open zombie —
  the core reports STALE honestly but recovery never starts.
- The M5-A give-up latch is visible on-device: three handshake-without-frame
  failures → red ERROR that only a subsequently accepted frame (or app
  restart) clears. That is contract behavior, not a bug.

## Kotlin bindings, host-JVM tested (`core/`)

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
