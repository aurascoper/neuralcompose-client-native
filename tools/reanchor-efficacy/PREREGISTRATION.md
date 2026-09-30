# Does re-anchoring pull the conversation back?

Written before the runs. The criterion below decides; a number read afterwards does not.

## What is actually being tested

**Re-anchoring cannot pull `heard` back, and this does not claim to test that.** The loop is
`listen → generate`. `heard` comes from a human through whisper; the framing changes only what
the poles are shown. So in the session that motivated all of this — where the *transcription*
degraded — re-anchoring could never have fixed the input. At most it keeps the **replies** on
subject while the input degrades.

That narrower claim is what is measured here:

> Given the same drifted input, do the poles' replies stay closer to the opening subject with
> framing on than with it off?

## Design

- **Arms**, same input script, paired:
  - `framed`: `--drift-ceiling 0.18` (the shipped default; fires on every off-topic turn)
  - `unframed`: `--drift-ceiling 0` (mechanism disabled)
- **Input**: `tools/drift-calibration/off-topic.txt` — the seed, then ten unrelated utterances.
- **Runs**: 5 per arm. Generation is sampled, so one run is one draw.
- **Repetition guard**: disabled in both arms (`--repetition-floor 0`). It is a separate
  mechanism and a forced silent turn would remove a reply from the outcome.

## Outcome measure

Primary: **cosine similarity of each reply (`spokenText`) to the seed sentence**, embedded with
`nomic-embed-text-v1.5` on port 8082.

Deliberately a *different* embedder from the one gating the mechanism (bge-small). If the same
model both decided to fire and scored the result, a positive finding could be an artifact of
that model's geometry rather than a change in the text.

Secondary, and plainly interpretable: **does the reply contain any of `radiotropic`, `biofilm`,
`biofilms`, `microbial`, `radiation`?** A count of on-subject mentions per arm. This one needs
no model and cannot be argued with.

## Positive control

One run each of `on-topic.txt` under both arms. **If on-topic replies do not score clearly
above off-topic replies on the primary measure, the measure is dead and the whole comparison is
uninterpretable** — a null result would then say nothing about framing. This is the check whose
absence would repeat the mistake that produced `drift_ceiling: 0.45`.

## Criterion, fixed now

Let `d = mean(framed) − mean(unframed)` over per-run means, and `s` = pooled within-arm standard
deviation of those per-run means.

- **`d > s`** → framing measurably pulls the replies back. Report `d` in absolute units.
- **`|d| <= s`** → no detectable effect at this sample size. Report it as that, **not** as
  "framing does not work": five runs cannot rule out a small effect.
- **`d < −s`** → framing pushes replies *away* from the subject, which would be a reason to
  reconsider §2 entirely.

Absolute numbers get reported in every case. A margin is reported as a margin, never as a ratio
of one small number to another.

## Round 2 — the relocated framing

Written after round 1 was scored and **before any round-2 data exists**.

### What round 1 found, and the fix under test

Round 1's criterion (`d > s`) was **met** and was **too lenient**: `d = +0.0117` against a
control gap of `0.362` — 3.2% — with 1 reply of 50 mentioning the subject. The criterion tested
*detectability* and never tested *materiality*, and the control I did run supplies exactly the
yardstick the criterion failed to use. That omission is the finding, and it is not repeated
below.

Diagnosis: every prompt shaper interpolates `heard` **inside a quoted utterance**
(`role.rs:104`), so splicing the anchor into `heard` delivered it as something the person had
said — after which the coherence pole was told to find the strongest thread in that utterance
and "not drift away from it". The instruction fought the anchor.

Round 2 moves the anchor out of the quotes and appends it after the shaper's own text, as
direction rather than transcript (`DialecticalRole::anchored_prompt`).

### Criterion, fixed now — BOTH must hold

Same measures, same control, same 5 runs per arm, `strong` (`--drift-ceiling 0.18`) against
`unframed` (`--drift-ceiling 0`).

1. **Detectability**: `d > s`, as before. Necessary, and by itself worth nothing.
2. **Materiality**: framing must close **≥ 25% of the control gap** — that is,
   `mean(strong) ≥ mean(unframed) + 0.25 × (mean(control) − mean(unframed))`. On round 1's
   scale that bar is roughly `0.46`; it is recomputed from round 2's own control, not
   hardcoded.

Secondary, model-free and stated as a hard number: **≥ 10 of 50** drifted replies must mention
one of `radiotropic`, `biofilm(s)`, `microbial`, `radiation`. Round 1 scored 1/50 framed against
0/50 unframed.

Failing materiality while passing detectability is reported as **"a real but immaterial effect"**
and the recommendation becomes option 2 — demote the mechanism to observation only. Passing both
keeps it. There is no third reading available after the fact.

### Comparison to round 1 is context, not evidence

Round 1's `weak` arm was a separate batch with a separately started llama-server. Any
weak-versus-strong difference is reported as suggestive and never as the decision.

## Known limits, stated in advance

- One seed, one input script, one generation model, one embedder for scoring. Five runs an arm.
- The off-topic utterances are mundane household observations. A different flavour of drift —
  the ASR-corruption kind that actually happened — is not covered and cannot be, because that
  drift lives in `heard`, which framing does not touch.
- `spokenText` is the *resolved* turn, one of two candidates. The losing candidate is not
  scored, so this measures what the session would have heard, not everything generated.
