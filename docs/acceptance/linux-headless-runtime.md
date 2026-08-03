# Linux headless runtime — acceptance record

Date: 2026-08-03
Branch: `feat/port-channel-health-classifier`
Base: `3b8be99` (PR #21 merge — the headless runtime landed from `main`)

The first execution of this repository's core on Linux, against a real Muse S
over the machine's own Bluetooth radio.

## Outcome

| Claim | State |
|---|---|
| Core executes on `linux/x86_64` in a live process | **ACHIEVED** |
| Real Muse S ingest end to end, zero drops | **ACHIEVED** |
| Channel-health classification available off Apple platforms | **ACHIEVED** |
| Physiological signal validated against a pre-registered threshold | **ACHIEVED — 1.98× on TP10, threshold 1.5×** |
| Any support-matrix row promoted | **NO — and deliberately so, see Non-claims** |

## Named hardware and software

Recorded because ADR-002's `DeviceValidated` rung requires *named* hardware, OS
and backend versions, and because an acceptance record without them is a claim
rather than evidence — even when, as here, no row is being promoted.

| Field | Value |
|---|---|
| Machine | GPD, AMD Ryzen AI 9 HX 370 w/ Radeon 890M (Strix Point) |
| OS | Ubuntu 26.04 LTS |
| Kernel | 7.0.0-28-generic |
| Bluetooth | `hci0`, BD `E0:D5:5D:91:71:A1`, BlueZ 5.85, USB bus |
| Rust | 1.97.1 |
| Headband | Muse S, BrainFlow `MUSE_S_BOARD` (id 39), native BLE — no BLED112 dongle |
| Bridge | `tools/muse-ble-bridge/bridge.py`, brainflow + websockets 17.0.1 in a venv |
| Runtime | `crates/neuralcompose-headless`, tungstenite 0.28 |

No `MUSE_MAC` was needed; discovery by name worked on Linux on the first
attempt. No group additions and no BlueZ permission changes were required — the
user is not in a `bluetooth` group and scanning worked regardless.

## Throughput

Three 30-second runs plus two 30-second recordings, all through the frozen
`/api/eeg/stream` contract.

| Run | Frames | Samples sent | Samples accepted | Drops |
|---|---|---|---|---|
| Fixture server (stdlib) | 126 | 1008 | 1008 | 0 |
| Live, before adjustment | 961 | 7688 | 7688 | 0 |
| Live, after adjustment | 961 | 7688 | 7688 | 0 |
| Live, with classifier | 960 | 7680 | 7680 | 0 |
| Bridge cumulative, whole session | — | 140 784 | — | **0** |

Effective rate 7688 / 30.0 s = **256.27 Hz** against a nominal 256. Every frame
carried 8 samples and every sample was accepted; `961 × 8 = 7688` exactly.
Phase was `live` throughout with a single connection generation and no
reconnects.

## Channel health

Classified by `ChannelHealthThresholds::default` — 2 µV dead, 200 µV saturated,
32-sample minimum — ported to Rust during this session (see below).

| Channel | Before adjustment | After | Status change |
|---|---|---|---|
| TP9 | 513 µV | 54 µV | saturated → healthy |
| AF7 | **881 µV** | **86 µV** | saturated → healthy |
| AF8 | 35 µV | 132 µV | healthy |
| TP10 | 69 µV | 62 µV | healthy |

AF7 fell **10×** on adjustment, which settles what the Swift enum's own
documentation raises as an ambiguity: this was contact, not an analog front-end
fault.

**A transient at t=15 s in the post-adjustment run is the more instructive
observation.** All four channels jumped to ~300–350 µV *simultaneously* and
recovered within five seconds. A failing electrode moves one channel; simultaneity
across all four is motion or muscle. The same 200 µV threshold fires for both
causes, and only the pattern across channels distinguishes them — which is why
the classifier is per-channel and never aggregates.

## Physiological validation — eyes open vs eyes closed

Protocol and arithmetic taken from `Scripts/validate-muse-physiology.py` in the
macOS repository: 30 s open, 30 s closed, alpha 8–13 Hz, threshold **1.5×**,
`bandpower` copied verbatim from lines 92–106. The threshold was pre-registered
long before this session.

| Channel | alpha ratio | beta ratio (control) |
|---|---|---|
| **TP10** | **1.98** | 0.61 |
| **TP9** | **1.90** | 0.98 |
| AF7 | 0.72 | 1.20 |
| AF8 | 0.41 | 0.41 |

**PASS** — best 1.98× on TP10 against a 1.5× threshold.

Three things make this coherent rather than merely above a line:

1. **The spatial pattern is right.** The two posterior channels rise; the two
   frontal ones do not. Occipital alpha is posterior, and this is the pattern
   that would be absent if the rise were an artifact.
2. **The frequency control separates.** TP10 alpha 1.98 against beta 0.61. A
   broadband change — movement, contact shift, gain drift — lifts both bands
   together. Alpha rising while beta falls is band-specific.
3. **RMS fell on every channel** (TP10 32.6 → 22.0 µV), so the ratio is not
   riding on a general amplitude increase.

**Weaker than the July reference, and recorded as such.** `docs/Validation.md`
in the macOS repository reports 3.21×–3.88× on TP10 across four sessions.
Tonight is 1.98× — same channel, same direction, roughly half the magnitude.
Shorter settling, different contact quality, and ordinary session variance at
n=4 prior sessions are all plausible. This clears the threshold; it does not
replicate the magnitude, and it is not presented as doing so.

## What was found, not built

Both of the session's defects were surfaced by running the thing, not by
reading it.

1. **The channel-health classifier existed only in Swift.** The core carried
   `ChannelHealthState`, a frozen `contracts/fixtures/health.json`, a schema
   whose `status` enum is `healthy | saturated | dead | unknown`, and a golden
   test asserting the fixture's AF8 reads `saturated` — but no rule deciding
   which of the four a measurement is. A live run reported 513 µV and 881 µV as
   bare numbers and they were read as merely "high" when the repo's own
   committed thresholds called both saturated. Ported in this branch, with the
   Swift tests carried over one-for-one and the thresholds' "NOT physiologically
   validated cutoffs" caveat carried with them.
2. **`bandpower` is in the same position.** The alpha analysis above ran in
   Python against the bridge, not through the core, because the core has no
   band-power analysis at all. That is the second capability this session found
   living outside the core, and it is recorded here rather than quietly worked
   around.

## Non-claims

- **No support-matrix row is promoted, and this record is not evidence for
  one.** All eleven rows are (OS, architecture, *backend*) triples —
  `llama-cpp-cpu`, `coreml`, `windows-ml-qnn`. Nothing here executed a backend,
  linked an accelerator library, or loaded a model. `RuntimeSmokeValidated`
  requires a deterministic fixture **model** to have run; a fixture EEG
  **stream** is not that. Treating this document as backend evidence would be
  promotion by implication, which ADR-002 exists to forbid.
- **The matrix has no row for what was actually validated.** It tracks model
  backends and says nothing about whether the ingest path runs anywhere. That is
  a gap in the register's shape, noted here and not fixed by editing a row.
- **The alpha result validates the path, not the thresholds.** 2 µV / 200 µV
  remain 2026-07-10 engineering heuristics on one subject; this session is the
  second they have ever been checked against.
- **One subject, one session, no sham condition.** Eyes-open/eyes-closed alpha
  is the oldest and most robust effect in the field, and this is still n=1
  without blinding.
- **The alpha analysis did not run through the core.** It is a Python consumer
  of the same wire contract.
- **`muse-ble-bridge` remains described in `tools/README.md` as "not a product
  surface"**, while being the sole ingest path used for every measurement above.
  That contradiction is unresolved and is not resolved by this record.
