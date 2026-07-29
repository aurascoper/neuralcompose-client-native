# Muse golden-capture gate — acceptance record

Branch: `gate/muse-golden-capture`
Base: `593373f3e8be6a345ecc01e704836285ce8f63b7` (M7-A2 merge, tag
`m7-a2-runtime-contract`)
Date: 2026-07-29

**Raw EEG is not in this repository and never enters CI.** Everything below
is metadata: hashes, counts, identities, and observations. The recordings
themselves live only in app-private storage on the device.

## Outcome

| Half | State |
| --- | --- |
| **Android (Pixel 8a)** | **ACHIEVED — with real Muse data** |
| **iOS (physical iPhone)** | **BLOCKED_OPERATOR** — no device, no Apple Development signing identity |

The gate as a whole is therefore **BLOCKED_OPERATOR**: it requires *both*
platforms, and the iPhone half cannot be attempted on this machine.

## What the envelope is

One WebSocket message becomes one JSONL line with the payload preserved
**verbatim**; EEG parsing never enters Swift or Kotlin. `CaptureRecorder`
decodes through the same `decode_eeg_frame` the live path uses, so a replay
verifies against the identical decoder. Malformed frames are preserved and
counted as rejected rather than dropped — a capture that hid junk would
misrepresent the stream. `verify_capture` re-derives every manifest claim
from the persisted bytes: size and digest first, then sequence order,
receive-time monotonicity, per-line accepted counts, four finite channels,
source-timestamp monotonicity, and the three totals.

## Android device evidence — REAL MUSE

Pixel 8a, Android 17, installed over Wi-Fi debugging (paired by code; the
`adb pair` QR path also works). Bridge: `muse-ble-bridge`, BrainFlow
`MUSE_S_BOARD` over **native macOS Bluetooth — no Mind Monitor, no phone in
the path**, 256 Hz, 0 dropped samples across ~95k.

| Field | Value |
| --- | --- |
| recordingId | `rec-1785318906605` |
| duration | 65 302 ms |
| messages / accepted samples / rejected | 2090 / 16 720 / 0 |
| effective rate | 16 720 / 65.302 s = **256.0 Hz** (Muse S native) |
| first → last source timestamp | 302.956834 → 368.244091875 s since stream start |
| payload bytes | 1 837 809 |
| payload SHA-256 | `dde2e964e22f5d3923cbe5acf739e126fe1f50baa0d8938e0aecfe51c1ef2fc7` |
| channel order | TP9, AF7, AF8, TP10 |
| bridge locality | localNetwork |
| **replay after app restart** | **VERIFIED — 16 720 samples replayed** |

Protocol steps observed: app launched wirelessly; endpoint typed into the UI
and connected (`ws://192.168.40.86:8788/api/eeg/stream`); `OpenNoData → Live`
after the first accepted frame; ~60 s recorded; Stop published atomically;
app force-stopped and relaunched; recording rediscovered; Verify replayed it
through the Rust decoder; a separate recording deleted with both files
confirmed gone from the filesystem. **No `.partial` files remained** after any
publication.

Signal note: at connection the montage was still settling (railed TP9/TP10
segments, visible in the live view). By the time of this recording all four
channels showed physiological activity. Signal *quality* is not what this
gate measures, and no claim is made about it.

## Android device evidence — synthetic stub (labelled separately)

Same device and protocol against the Gate 4 synthetic stub
(`ws://192.168.40.86:8787`), run first to prove the plumbing:
`rec-1785318180613`, 65 411 ms, 2048 messages, 16 384 samples, 0 rejected,
2 146 275 bytes, SHA `23043904ac07464e6cca6f9f9c4ae5fd0a0d2d03cca971b630fdfd5bc66f5bca`,
**VERIFIED — 16 384 samples replayed** after restart.

**This is stub evidence, not Muse evidence**, and does not substitute for the
row above.

## Host / simulator evidence (binding-level only — NOT device acceptance)

- Rust: 136 tests green on default features and with `--features uniffi`;
  `clippy -D warnings` and `fmt --check` clean on both; fixture, binding
  drift, secret scan and `git diff --check` clean.
- Capture envelope: 9 regressions covering verbatim preservation, malformed
  frame accounting, digest and size tamper detection, same-size line
  reordering, manifest inflation across every count, and byte-identical
  output across platforms.
- Android `CaptureStore` was exercised against the real core dylib on the
  host (20 checks: publish/discovery/round-trip/counts/tamper/delete).
- iOS: builds clean for the iPhone 17 simulator; the app installs and
  launches. **The UI was never driven** — `simctl` cannot tap — so layout and
  control wiring are compiled, not exercised.

## Blockers

1. **No physical iPhone.** None paired or attached; `devicectl` lists none.
2. **No iOS signing identity for device installs.** The only codesigning
   identity present is a self-signed local-dev certificate for macOS; there
   is no Apple Development team certificate, and the target sets
   `CODE_SIGNING_ALLOWED: NO`. A device install needs a real team, which is
   an operator/account action.

Both are operator actions. Nothing about iOS device behaviour is claimed.

## Decisions worth review

- **ATS on iOS**: the nested `NSAppTransportSecurity` key cannot be expressed
  by `INFOPLIST_KEY_*`, so `project.yml` now generates an explicit
  `Info.plist` and the usage descriptions moved with it. It declares
  `NSAllowsLocalNetworking` — RFC1918, `.local` and link-local only — **not**
  `NSAllowsArbitraryLoads`. A host-scoped exception was rejected because the
  endpoint is configurable by design and pinning one IP breaks the moment the
  LAN changes.
- **Cleartext on Android**: the release config in `src/main` still names each
  cleartext host explicitly. A **debug-only** override in `src/debug` permits
  cleartext generally, so the endpoint field means what it says in the
  developer build while the shipped posture stays strict.
- **Two bridges were added** under `tools/` because the topology this gate
  assumes had no implementation here. Direct BLE is preferred; the Mind
  Monitor OSC bridge is a fallback. Both are development tools, excluded from
  every build graph.

## Non-claims

- **No iOS device evidence of any kind.** Simulator results are binding-level
  contract tests.
- **No claim about EEG signal quality, electrode contact, or physiological
  content** of the recording.
- **No claim that the stub recording is Muse evidence.**
- **No claim about background/foreground capture behaviour**, sustained
  multi-hour capture, or capture under reconnect churn — none were exercised.
- **No claim about simultaneous dual-device capture**; only one client
  connected at a time.
- The directory entry is not fsynced after rename on Android (Java exposes no
  API), so file contents are durable but an OS-level crash immediately after
  publication could in principle lose the rename.
