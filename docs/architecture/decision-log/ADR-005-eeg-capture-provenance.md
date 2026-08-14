# ADR-005: Typed EEG capture — what was measured, what was interpreted, and what was refused

- Status: Proposed
- Date: 2026-08-14
- Supersedes: nothing. Extends ADR-004's vocabulary to the EEG path.

## Context

`neuralcompose-hypnagogic` was a complete Linux dialectic shell with no corpus.
EEG reached the turn log as three bare numbers per channel; no raw signal was
persisted on Linux at all; and ADR-004's `ProvenanceEnvelope` — which exists to
stop an indication being mistaken for a measurement — was applied nowhere in the
EEG path.

The goal is the app as corpus instrument: sessions worth sitting through, whose
EEG is **recorded provenance rather than model input**, typed at capture time so
a later reader can ask why a session was excluded instead of re-deriving it.

## Decisions

### 1. There is no JEPA in v1 and the seam stays empty

The `WorldModel` encoder is trained on `synthetic_1f`, a synthetic
continuous-control task. It predicts particle dynamics. No EEG encoder exists in
this project — EEGPT was ruled out on montage, Laya on scale.

`gloss_scalar` stays at `NEUTRAL_GLOSS` (0.5), logged as present-and-neutral
rather than absent, and `spectral_state` stays `None`. **No gloss is derived
from band ratios.** That mapping is unvalidated, and giving it the authority of
a measurement is the failure this ADR exists to prevent.

This makes the gloss ceiling correct rather than a limitation: EEG is recorded
provenance in v1, not input to the dialectic.

### 2. `band_power` returns `Option<f64>`, and the gate depends on it

`band_power` returned `0.0` for five distinct refusals — sub-second window,
unusable rate, non-finite sample, inverted band, band above Nyquist — and `0.0`
is also a value the arithmetic legitimately produces. "Refused" and "measured no
power" were the same value to every caller.

**This had to be fixed before the capture gate could be written, not after.**
A gate built on `== 0.0` would work until someone correctly refactored
`band_power` to return `Option`, at which point short windows would yield `None`,
the comparison would stop matching, and *fixing the defect would introduce the
fail-open the gate exists to prevent*. Nothing would have caught it.

Contained change: no uniffi export, one production caller
(`electrode_check.rs`), everything else its own tests.

`ElectrodeReport::mains_power` became `Option<f64>` for the same reason, found
in the same pass: it was `0.0` when no band was measurable, which reads as "no
line noise here" — the cleanest possible result — for the case where nothing was
looked at.

### 3. The capture gate refuses, with distinct reasons

`eeg_reading_for_turn` returns `Result<_, EegRefusal>` rather than `Option`.
Seven refusal ids, each a stable kebab-case string on the record.

The two band refusals are deliberately **not** merged:

- `band-not-measurable` — `band_power` returned `None`. The instrument could not
  look.
- `band-exactly-zero` — `band_power` returned exactly `Some(0.0)`. It looked at a
  channel that is not alive.

Only the second is evidence about the electrode.

**Correcting the reasoning in the design note.** "An exact 0.0 from a float
computation over real EEG is a measure-zero event" is true of a computation over
signal and *false of a function that short-circuits to 0.0 by design*. It became
true only after decision 2. And the reachable case is not a coincidence at all:
an exact zero means every detrended sample was exactly zero, i.e. a constant
window — a dead or railed channel. That is deterministic, not probabilistic, and
`a_constant_window_is_exactly_zero_not_merely_small` pins it.

The lag precondition (`window >= fs / centre_hz / 2`, 57 samples for delta at
256 Hz) is asserted even though `band_power`'s one-second floor of 256 samples
subsumes it, because that floor lives in another crate's function.

This is the trap the Swift path has: `JEPATransition` guards only `isFinite`, and
`FeatureExtractor:117` reports zero when a window is shorter than the lag. That
hazard is not ported here.

### 4. Three provenance tiers, because two collapse a derivation into an observation

| Tier | Kind | Contents |
| --- | --- | --- |
| Raw frames (`<id>.eeg.jsonl`) | `observed` | the WebSocket payloads, verbatim |
| `ChannelDerived` | `derivedDeterministically` | `rmsMicrovolts`, `mainsPower` |
| `ChannelAnnotation` | `heuristicAnnotation` → `NeverIngestible` | `status`, `verdict`, `mainsLineHz` |

An earlier two-way split typed `rms` as `observed`. It is not: it is a named
transform over samples. The only observed thing in this pipeline is the raw
frame — and it is persisted for the first time here, so an input reference
finally exists.

`mainsPower` sits on the **derived** tier, not with the verdict. Bundling a power
figure with an interpretation under one envelope collapses two epistemic classes
into the looser of them, which is the failure the taxonomy exists to prevent.

The raw-frame envelope carries `method: None`. `Observed` is not in
`requires_method` because an observation is a reading, not a computation; build
identity and digest already live in the `CaptureManifest`.

**The naming rule decided the annotation tier.** `status` reads `healthy`,
`saturated`, `dead` — state words, claims about an electrode — and
`channel_health.rs:24-32` says the thresholds behind them are not
physiologically validated. A field whose name implies a state cannot be typed as
a measurement. `alpha_tp10_power` typed `observed` would be honest; a field named
`relaxation` typed `observed` is laundering.

`verify_turn_log` enforces the kinds. Convention would not survive a refactor.

### 5. The input reference names the window, not the file

`ResourceRef` digests **the samples actually consumed**, with the capture file as
`locator`. The capture's `payloadSha256Hex` only exists at `finish()`, at end of
session, while these envelopes are written during it — so a whole-file digest was
not available. Digesting the window is both computable at turn time and the
stronger claim: it makes the derivation reproducible rather than merely
attributed. `sampleCount` and `lastSourceTimestamp` locate it in the capture.

Without a capture the locator is `None` — the window is still digested, but no
file is named, because naming one that was never written is worse than admitting
there is none.

### 6. Absence carries its reason

`channelHealth: null` and `channelHealthAbsentReason: "<id>"` are written as a
pair, mutually exclusive by construction and checked by `verify_turn_log`.

Before this, "no reading this turn" and "a dead electrode this turn" produced
identical records. That is what forced session eligibility to be a human verdict
reconstructed afterwards.

### 7. Logging the electrode verdict does not overturn the decision that kept it out

`eeg.rs` deliberately kept the electrode verdict out of the record: *advice is
not a measurement, and a turn record should not carry a sentence telling a future
reader to reseat an electrode that was reseated months ago.*

**That reason is about the advice sentence, and it still holds.** The record
carries `ElectrodeVerdict`'s stable id — a classification eligibility can query —
and never `ElectrodeVerdict::advice()`. The original decision is preserved, not
overridden. Typing it `heuristicAnnotation` is what keeps the classification
un-ingestible; it is not the reason it is safe to log.

### 8. Session metadata is a sidecar, not a `CaptureManifest` field

`<id>.session.json`: power supply, battery, cpufreq governor and driver,
platform profile, and every hci adapter with its rfkill state — all `observed`
from `/sys`, reusing the field shape of `relay.py`'s `power_state()`.

Not in `CaptureManifest`, which is a `uniffi::Record` consumed by the iOS and
Android shells. Linux host facts do not exist on those platforms, and adding them
would force binding regeneration to model something cross-platform that isn't.

**Board id and BrainFlow preset are `externalClaim`, from `--eeg-source` /
`--eeg-preset`, absent by default.** This binary cannot see either: the bridge
sends `{timestamp, channels}` and never announces the board. Defaulting to Muse S
because that is what is usually attached would be inventing a measurement.

There is deliberately **no `address` field**. The BD address is not in sysfs. The
first version of this struct had one and it was `null` in every run — which reads
as "the adapter had no address" rather than "this process cannot see one".

### 9. The embedder backend is asserted, not announced

Startup compared a hardcoded `BACKEND_ID_CPU` in a format string — a claim about
the code, not about the run. It now reads `Embedder::backend_id()`, which derives
the answer from the outcome, and refuses to start on anything but CPU.

Exactly one process in this system may hold a Vulkan context, and it is
`llama-server`.

### 10. Eligibility is a sealed pre-registration and a query

`contracts/eeg/eligibility-v1.json` + `.sha256`. The predicate refuses a
registration whose digest does not match, and returns a **verdict with its
reasons** — never a bare bool.

**The seal proves the file was not edited. It does not make the numbers right.**
The 0-of-4 prior comes from 83-second block-design runs where the subject sat
still on a timer; a conversational session is multi-minute and involves speech,
which moves the jaw and the electrodes. The registration says so in its own text
and lists four non-discriminating outcomes, so the first surprise is a recorded
outcome rather than an argument about whether the number was ever reasonable.

Revision means `eligibility-v2.json` with its own seal and date. This file is
never edited to fit data.

### 11. Prosody dimensions that reached no engine now do

Found while verifying the spoken loop: `Prosody::volume` and
`Prosody::pre_utterance_delay` were consumed by **nothing**. Both speakers
dropped them silently.

`pre_utterance_delay` is the cadence dimension — `loops.rs` records the
contemplative voices carrying over a second of it. On a fixed-voice engine, where
two neural voices have no midpoint and `Prosody::blend` can only take the heavier
one, **pacing is the channel tension stays audible through**. Discarding it
removed the one prosodic dimension Kokoro had left.

Now: `volume` maps to espeak's `-a` and to a new `KOKORO_VOLUME` sample scale in
`speak.py`; `pre_utterance_delay` is honoured by both speakers.
`pitch_multiplier` remains unmapped under Kokoro — that is a property of the
engine, which cannot pitch-shift, and is recorded as a real loss.

## Ceilings — not to be quietly crossed

- **The montage is the ceiling.** TP10 is the only channel with a replicated
  validated alpha contrast. TP9 is a replicated null with healthy contact, AF7
  has failed three sessions running, AF8 is amplitude-confounded. Four channels
  are captured; **the corpus is effectively one derivation and must never be
  described as multichannel.**
- **The pad question is unresolved.** The mains check's PASS path is evidenced
  only by the synthetic fixture; it has never run on a correctly fitted headband,
  and 50 Hz detection is untested on hardware.
- **A feature's name must agree with its class.**
- **Hand-engineered band features and a JEPA are alternatives, not
  complements.** Feeding engineered band ratios to a JEPA defeats its premise and
  makes it an expensive MLP. Band energies here are a **gate only** — computed to
  refuse unusable windows and discarded. No `alpha_tp10_power` field exists, so
  it cannot be laundered. Provenance typing applies either way, which is why it
  was worth doing before that fork is taken.

## Non-claims

Promotes no support-matrix row. `attained_support_status()` returns exactly what
it returned before; per ADR-002 there is no promotion by implication. A live
microphone and a live headband are the opposite of deterministic fixtures.

No EEG has been recorded from a human under this code. Every measurement here
comes from `tools/fixture-eeg-server`, which carries no physiology and makes no
claim about EEG.
