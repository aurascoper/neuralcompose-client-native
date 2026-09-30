# Does re-anchoring pull the conversation back? — measured

Criterion fixed in `PREREGISTRATION.md` before each round. Per-reply evidence in
`results/round1.json` and `results/round2.json`; those numbers are the primary record, since
generation is sampled and a re-run cannot reproduce them.

## Answer

**Yes, once the anchor is placed as instruction rather than transcript.** The same text in the
wrong slot did almost nothing.

| | round 1 — anchor spliced into `heard` | round 2 — anchor appended as instruction |
|---|---|---|
| `d` (framed − unframed) | +0.0117 | **+0.1831** |
| pooled within-arm SD `s` | 0.0082 | 0.0298 |
| share of control gap closed | 3.2% | **43.8%** — bar was 25% |
| replies mentioning the subject | 1/50 | **35/50** — bar was 10/50 |
| pairwise run comparisons won | 21/25 | **25/25** |
| Welch `t` | 2.27 (p = 0.057) | **9.71** |
| ranked runs | `U U U F U F F U F F` | `U U U U U F F F F F` |

Round 2 passes detectability, materiality and the model-free secondary. Round 1 passed only
detectability.

## The placement was the whole effect

Every prompt shaper interpolates `heard` **inside a quoted utterance** (`role.rs:104`). Round 1
spliced the anchor into `heard`, producing:

```text
In a live dialogue, the other person just said: "The kettle in the next room has
started whistling again.

(this exchange began with: What do you know about radiotropic biofilms?)"

Respond as the voice that seeks coherence: find the strongest, clearest thread in
what they said and carry it forward faithfully... Do not drift away from it.
```

The anchor arrived as something the person had said, and the pole was then instructed to find
the strongest thread in that utterance and not drift from it — in a sentence about a kettle,
that is the kettle. Round 2 appends the identical information after the shaper's own text, where
it reads as direction. Same seed, same inputs, same model, same ceiling; 3.2% became 43.8%.

**Round 1's near-null was not evidence the mechanism fails.** It was evidence it had been
plugged into the wrong slot, and only the measurement separated those. Shipping on "small but
real, good enough" would have shipped an inert feature behind a green suite — the same shape as
`drift_ceiling: 0.45`, and the same shape as the Witness that started all of this.

`role.rs`'s `the_anchor_is_direction_not_transcript` is the regression guard. Every prior test
asked whether the seed was *present*; none asked *where*. Presence is not placement.

## Scale, from the control

On-topic replies score 0.783; drifted replies with framing off score 0.365. The unframed arm sat
at 0.365 in both rounds, which is what makes the two batches comparable. Framing recovers a
little under half of that 0.418 distance.

The control also confirms the ceiling from the other side: on-topic input triggered re-anchoring
**0 times in 11 turns**, off-topic **10 of 10**, in every run of both rounds.

## What this does not show

- **It cannot fix the session that motivated it.** The loop is `listen → generate`; `heard`
  comes from whisper and framing only changes what the poles are shown. The drift in session
  1788413094 was in the transcription. The non-speech guard addresses that; this does not, and
  no re-anchoring scheme could.
- Five runs an arm, one seed, one input script, one generation model, one scoring embedder.
- The off-topic utterances are mundane household observations. Other flavours of drift are
  uncovered.
- `spokenText` is the resolved turn, one of two candidates; the losing candidate is unscored.
  This measures what a listener would have heard, not everything generated.

## Process notes worth keeping

- Round 1's criterion tested *detectability* and never *materiality*. The positive control it
  did include supplied exactly the yardstick the criterion omitted. `d > s` at n=5 is satisfied
  by an effect worth 3% of the available range.
- The first attempt at round 2 was destroyed from inside: running `cargo test` rebuilt the
  binary as a stub mid-batch, because `build.rs` reruns whenever `LLAMA_CPP_DIR` changes, and
  `run.sh` invokes the binary per iteration. `run.sh` now builds its own binary and aborts on a
  short run or an `Unavailable`, rather than collecting near-empty runs and averaging them.
- The first completion waiter counted output *files*, which exist from the shell redirect before
  a run has written anything. It declared a batch done mid-write. Wait on the process, not on
  its artifacts.
