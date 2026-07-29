# Acceptance record — M5-B (Compose EEG slice) + M5-C (Pixel parity) — 2026-07-28

Evidence classes kept separate. All observations made on the M4 Mac with a
**physical Pixel 8a (akita, Android 16) over USB-C**, `adb reverse
tcp:8787 tcp:8787` to the checked-in Gate 4 stub. Base: `main @ e11971c`
(M5-A merged) — the Compose shell consumes the corrected generation-scoped
contract from day one.

## Build environment (headless — no Android Studio)

- Homebrew `android-commandlinetools`; SDK: platform-tools, platforms;android-35,
  build-tools;35.0.0, ndk;28.2.13676358
- `cargo-ndk` + Rust target `aarch64-linux-android` (toolchain-pinned 1.97.1);
  `libneuralcompose_mobile_core.so` (arm64-v8a, ~780 KB) via
  `cargo ndk … --features uniffi`
- `android/app`: AGP 8.13.0 + Gradle wrapper 8.14.3 (system Gradle 9 cannot
  run AGP 8; wrapper generated in an empty dir because even `gradle wrapper`
  evaluates the build) + Kotlin/Compose plugin 2.2.0, compileSdk/targetSdk 35,
  minSdk 26, JNA 5.17.0 **@aar**, OkHttp 4.12 with **`pingInterval(5s)`**
  (load-bearing — see findings), narrow cleartext config for 127.0.0.1/10.0.2.2
  only.

## Evidence

| Class | Result |
|---|---|
| Gradle build | `./gradlew assembleDebug` / `installDebug` **BUILD SUCCESSFUL**; installed on the Pixel 8a |
| Rust regression | 9 suites (46 tests) green; clippy `-D warnings`, fmt, fixture drift all clean |
| Kotlin host-JVM regression | `android/core` 2 tests, 0 failures (unaffected by the app module) |
| Pixel — Live | Green `OPEN` pill, four sine channels in fixed TP9/AF7/AF8/TP10 order, counter advancing (px7-01) |
| Pixel — Stale | Orange `STALE 6s` + "Stream silent — no samples for 6s (socket still open)", counter frozen, cached traces (px7-02) |
| Pixel — OpenNoData (M5-A) | Fresh connection to a paused server: orange **`OPEN · NO DATA`**, 0 samples, empty traces — a socket without frames is never presented as healthy (px8-03) |
| Pixel — Recovery | After resume: green `OPEN`, counter advancing (px8-04) — first accepted frame is the recovery proof |
| Pixel — Closed (bonus) | Red `CLOSED` + "Stream disconnected — showing last cached data" with cached traces mid-retry-ladder (px7-04) |
| Pixel — Error latch (bonus) | Three handshake-no-frame failures (connects landed in a drop/pause window) → red latched `ERROR`, exactly the M5-A give-up contract (px6-03) |
| Pixel — Honest zombie (bonus) | After ~42 min screen-off freeze, believed-open dead socket rendered as `STALE 2514s` with the silent-stream banner — the Gate 4 failure class, presented truthfully by the core (px2-02, pre-pingInterval build) |

## Findings folded back into the code/docs

1. **OkHttp `pingInterval` is required** (commit in this branch): without it a
   socket that dies while Android freezes the process (screen off) stays a
   believed-open zombie; the core reports STALE honestly but the shell never
   learns the socket is dead, so the reconnect ladder never starts.
2. **`adb reverse` does not survive USB re-enumeration** — documented in
   `android/README.md`; symptom is ERROR with 0 samples on a fresh launch.
3. Foreground UI interactions (home-gesture) recreate the Activity/ViewModel;
   a fresh monitor connecting into an outage window can legitimately exhaust
   its budget → latched ERROR. Contract behavior; a manual retry affordance
   is future UI work (M6+).

## Non-claims

- No Play packaging/signing, no emulator matrix (physical device only), no
  16 KB-page-size device verification, no mic/audio (M6), no androidTest
  suite (acceptance is screenshot-based like iOS), iOS shell untouched this
  milestone. The stub remains the only validated server; no Mac production
  server exists yet.
