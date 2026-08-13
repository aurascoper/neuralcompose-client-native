# The world-model demo — registration

Date: 2026-08-13
Status: **registered. Not yet measured. §6 is empty by design.**

This document is a **pre-registration**, not a result. It fixes the reading rule before
the numbers that rule adjudicates exist. Sections are labelled `§N` and cited by number.

Written and committed **before `crates/neuralcompose-hypnagogic/src/worldmodel.rs` has
ever been run.** That ordering is the only thing that makes §3 a threshold rather than a
description of whatever happened.

## §1 — What is being measured

An **MPPI planner over the true analytic dynamics** of `ParticleNavigatorEnv`, against two
baselines, on the seven `HARD_CASES` from `WorldModel/sweep.py`.

This has no existing implementation anywhere. Python's `mpc.py::plan_step` takes a
`JEPAModule` and a `goal_latent` as required positional parameters and dereferences both
unconditionally — there is no branch in it or in `score_candidates` that rolls out true
dynamics. The Swift `WorldModelMPCEngine` plans through three Core ML models. So this is a
**fresh measurement**, not a port check, and it is reported as one.

The *environment* half is a different matter: that is a port, and §5 gives it a
conformance target.

## §2 — Three things this cannot be compared to

1. **`WorldModel/EXPERIMENT_LEDGER.md`, all of it.** Every node 0–25 is
   `synthetic_1f` + `forward_eval.py` — a different environment, encoder, task and planner
   (CEM, not MPPI), reporting `mpc_success` against `random_baseline_success`. Its numbers
   share the words "MPC", "success rate" and "random baseline" with this work and nothing
   else. **Naming those two files specifically is the point**; "not comparable to the
   ledger" points at the wrong thing, and a careless reading inverts which nodes are
   off-limits.
2. **`WorldModel/README.md`'s own numbers**, even though they are the same environment.
   Its 0/35 hard-case result and 0.13–0.21 aggregate success are MPPI over a **learned
   JEPA latent**, whose stated ceiling is a latent-to-position correlation of r≈0.63. This
   work plans over **exact dynamics**. It should do far better, and that would say nothing
   whatever about the latent planner. Same env, different experiment.
3. **The Swift demo**, which has no episode measurements at all — only a skipped-by-default
   integration test and two unit tests on the baseline planner.

There is no mechanism preventing a result from being filed against the wrong harness. The
previous plan claimed `Scripts/check_ledger_prefix.py` enforced a `Harness:`/`Provenance:`
rule; **that script does not exist.** `Scripts/check_ledger_references.py` does, and is a
self-described advisory prototype not wired into CI that checks node-heading format and
reference resolution — nothing about harness attribution. The separation currently holds
only because Harness A results were never entered into the ledger.

## §3 — The threshold, pinned

> **MPPI over true dynamics reaches the goal on ≥ 33 of 35 trials.**
>
> Goal reached means final distance `< 0.1` — `GOAL_TOLERANCE` in `mpc.py:125`, and
> `WAYPOINT_REACHED_DIST` in `dataset.py:38`, which already agree.
> Trials are the seven `HARD_CASES` × seeds `[0, 1, 2, 3, 4]`, `max_steps = 50`.

## §4 — The non-discrimination rule, pinned

> **If the PD arm also clears 33/35, the verdict is "the task does not discriminate over
> true dynamics" — not an MPPI success.**

A planner that ties a damped controller on a task the controller already solves has
demonstrated nothing about planning. This rule exists because the failure mode here is not
MPPI losing; it is MPPI winning against a straw man on a task that was never hard once the
model is exact.

**Bang-bang is reported, never used as the bar.** `BaselineWorldModelDemoPlanner` is
`unit_vector(goal − pos) × maxAccel` — constant full thrust, no velocity term, no braking.
Its own doc comment and `ADR-007` call it "proportional"; that is wrong in the code's terms,
and bang-bang overshoots *more* than a P controller would. Beating it is not evidence.
It is included for parity with what the Swift demo actually ships, and for no other reason.

**No arm is tuned after seeing results.** If a threshold is missed, the miss is reported
and the reason investigated; the threshold is not moved. Any later re-run under changed
parameters is a new registration with its own date.

## §5 — The environment port, which does have a conformance target

`step()` is pure, deterministic and stateless, needs only numpy, and already has a
term-for-term Swift twin at `Sources/WorldModelDemo/ParticleNavigatorEnv.swift`. **Nothing
pins it against Python in any language today** — the Swift tests are four hand-computed
literals.

`tests/fixtures/env_v1.json` is generated from `WorldModel/env.py` itself. Required
coverage, asserted by a meta-test rather than assumed: ≥ 200 steps, a wall bounce on each
axis, a corner hit (both axes in one step), a speed clamp, and at least one step where the
clamp does not fire.

Agreement is **bit-exact `f32`**, not a tolerance. Both sides are `float32` and
deterministic, so a tolerance would only hide a real divergence. If bit-exactness proves
unreachable, the fallback is `1e-12` relative **and the header must name the exact
operation that forced it.**

## §6 — Results

Measured 2026-08-13, one run, byte-identical on repetition. Seven `HARD_CASES` × seeds
`[0,1,2,3,4]`, `max_steps = 50`, tolerance `0.1`. Parameters digest `28e15df058d157cd…`.

| arm | reached | mean final distance | mean steps to goal |
|---|---|---|---|
| mppi | **35/35** | 0.0490 | 26.5 |
| pd | **35/35** | 0.0341 | 17.1 |
| bang-bang | **35/35** | 0.0326 | 19.7 |

### Verdict: the task does not discriminate over true dynamics

§3's bar was met — and §4 governs, because the PD arm cleared it too. This is **not**
reported as an MPPI success. Every arm solved every trial.

That §4 fired on its first use is the only reason this section does not read as a win.

### Three things the numbers say that the plan assumed otherwise

1. **MPPI is the *worst* of the three arms**, on both mean final distance and steps to
   goal. Uniform candidate sampling over a 10-step horizon is a weak planner next to a
   tuned PD when the model is exact and the budget is four times the minimum
   (2.263 / 0.2 per step ≈ 12 steps needed, 50 allowed).
2. **"Beating bang-bang is nearly free" is refuted.** It was recorded as an untested
   hypothesis in the plan; measured, bang-bang has the *best* mean final distance of the
   three. It does overshoot — 19.7 steps against PD's 17.1 — but it reaches the goal every
   time, and the wall bounces that were supposed to defeat it do not.

   > **Corrected by §8.4.** The distance ranking is **set-specific and does not
   > generalize**: on the held-out scenarios bang-bang is *worst* of the three (0.0700
   > against PD's 0.0392), not best. What survives is only the narrower claim — bang-bang
   > reaches the goal on every trial of both sets, so it is not nearly-free to beat. The
   > "best mean final distance" half of this finding should not be cited.
3. **`corner_pn_to_np` is solved by all three arms**, in 18–28 steps. That is the direction
   `WorldModel/README.md` records as failing completely under *every* latent configuration
   tested (final distance 2.234–2.272 from a 2.263 start), with two hypotheses ruled out
   and the cause unknown.

   Per §7 this is **not** an explanation of that failure — the predictor is not in the loop
   here at all. What it does establish, narrowly, is that **the geometry alone is not what
   makes that direction hard.** An exact model traverses it as easily as the other three
   diagonals. Whatever is wrong lives in the learned predictor, which is where the README's
   own remaining hypothesis already pointed.

### What would be needed to discriminate

Nothing here is retuned to produce a better answer — §4 forbids it, and the bar does not
move after the fact. A discriminating experiment would need a **new registration** with its
own date and its own threshold: a shorter step budget, actuation noise, a partially
observed state, or obstacles — something that punishes greedy steering. This task, over
exact dynamics, does not.

## §7 — Non-claims

- **Promotes no support-matrix row.** A synthetic-task planner run is not runtime evidence
  for any backend, and `attained_support_status()` returns exactly what it returned before.
- **Says nothing about EEG, sleep, or the dialectic.** `ParticleNavigatorEnv` is a point
  mass in a box. It carries no physiology and models no cognitive process.
- **Says nothing about JEPA, latent planning, or the learned predictor**, which this
  deliberately does not use — see §2.2.
- **Does not resolve the `corner_pn_to_np` asymmetry.** `WorldModel/README.md` records that
  `(0.8,−0.8) → (−0.8,0.8)` fails completely under every latent configuration tested, with
  two hypotheses ruled out and the cause unknown. Planning over true dynamics removes the
  predictor from the loop entirely, so a success here is **not** an explanation of that
  failure — it only shows the geometry alone is not what makes it hard.

## §10 — What a Swift conformance failure would mean, decided in advance

The same `env_v1.json` pins `Sources/WorldModelDemo/ParticleNavigatorEnv.swift`, which
today has four hand-computed literals and no reference. Registered before that test is
run, because the interesting outcome is the one most easily explained away afterwards.

> **If the Swift does not reproduce the fixture, that is a defect in the macOS app's
> environment — the one `WorldModelDemo` actually runs on — not a problem with the fixture
> and not an artifact of the Rust port.**

The fixture is generated from `WorldModel/env.py` itself and the Rust matched it
bit-for-bit on its first run. Two independent implementations agreeing leaves the third as
the outlier; a fixture defect would have had to break Rust too.

**The one place a mismatch is expected, if there is one.** `env.py:59-61` computes the
speed norm and then divides in **float64** (`float(np.linalg.norm(vel))`, and `max_speed`
is a Python float), while `ParticleNavigatorEnv.swift:63-67` does the whole clamp in
`Float`. If mismatches land **only on the 11 speed-clamped steps**, that is this
difference, and it is a real numerical divergence in the Swift rather than a rounding
curiosity — the two implementations would disagree about the velocity after any saturating
manoeuvre. Mismatches **away from** clamped steps are something else and would need
separate diagnosis.

**The NaN divergence cannot be what fires.** Swift's clamp turns a NaN action into full
throttle, but the fixture's action script contains no non-finite value, so that path is
never entered. It stays a known, separately-recorded defect.

**Nothing here blocks anything.** A failure is written up in the NeuralCompose repository
as a review note, the way the `cosineSimilarity` and `centroid` findings were. It does not
gate this repository, and it does not change §6 or §8.

### §10.1 — Result: it matched, and the predicted divergence did not appear

Run 2026-08-13 in `NeuralCompose-dialectic-fixture`, committed there as `5705082`. The
Swift reproduces all 260 steps **bit-for-bit**, including all 11 speed-clamped ones. So the
float64-vs-Float division named above is not observable at these values — the two roundings
coincide. **§10's prediction was wrong in the harmless direction**: it said where a failure
would be, and there was none.

Three independent implementations now agree exactly on `step()`: the Python original, the
Rust port, and the Swift. Nothing pinned it in any language before today.

**The test was verified capable of failing.** Scaling `restitution` by 1.0001 in the Swift
turns it red in 6 places; restoring the file byte-identically turns it green. A conformance
test nobody has watched fail is a guess, and this one was watched.

## §8 — Second registration: step count, with the selection disclosed

Date: 2026-08-13, after §6. **Written and committed before the run it governs**, same as §3.

**The metric was chosen after seeing it separate.** §6's binary metric saturated at 35/35
for all three arms; mean steps to goal did not — 17.1 / 19.7 / 26.5, a 55% spread that
orders all three cleanly. Picking a metric because it worked is selection, and undisclosed
selection is how a null result becomes a finding. So it is disclosed here, and it is
re-run on data that did not inform the choice.

### §8.1 — Fresh data, and what "fresh" can and cannot mean here

**Seeds only refresh MPPI.** PD and bang-bang are deterministic: given a scenario they
produce one trajectory, and running them under five more seeds yields five identical
copies, not five samples. A seed-based replication would therefore test one third of the
claim while looking like it tested all of it.

So the held-out set is defined two ways, both by rule rather than by inspection:

- **Fresh seeds `[5, 6, 7, 8, 9]`** — disjoint from §6's `[0..4]`. Refreshes MPPI.
- **Fresh scenarios, generated not chosen**: starts at radius `0.8` and angles
  `k · 45°` for `k = 0..7`, each with the antipodal goal `−start`. Eight scenarios,
  none of which is a `HARD_CASES` member (whose corner coordinates `(±0.8, ±0.8)` sit at
  radius 1.131). Refreshes all three arms.

Reported as 8 scenarios × 5 seeds = 40 trials.

### §8.2 — The claims, pinned

> **C1 — the ordering replicates.** Mean steps to goal satisfies `pd < bang-bang < mppi`.
>
> **C2 — the separation is real, not noise.** MPPI's mean exceeds PD's by **≥ 25%**.
> Observed in §6 was 55%; 25% is a deliberately conservative replication bar, because the
> question is whether the effect exists, not whether it reproduces to two figures.
>
> **C3 — the tighter gap survives too.** Bang-bang's mean exceeds PD's by **≥ 5%**.
> This is the one that can fail: §6 measured 15%, and it is the pair with the least room.

If a claim fails it is reported failed. The metric is not swapped again — a second
undisclosed selection would make the first disclosure worthless.

### §8.3 — Why the binary metric saturated, stated so it is not rediscovered

Mean final distances of 0.033–0.049 against a `0.1` bar means every arm clears by two to
three times. Against an arena diagonal of ≈2.263 the tolerance is loose enough that
`GOAL_TOLERANCE = 1.0` would still read 35/35 — which is exactly why that mutant survived
the whole suite until §6's run forced a test for it. **The bar was never near the achieved
values**, so the binary metric could only ever have come out saturated or catastrophic.

### §8.4 — Results

Measured 2026-08-13 on §8.1's held-out data. Reproducible byte-for-byte. 8 scenarios × 5
fresh seeds = 40 trials.

| arm | reached | mean final distance | mean steps |
|---|---|---|---|
| mppi | 40/40 | 0.0564 | 24.3 |
| pd | 40/40 | 0.0392 | 16.5 |
| bang-bang | 40/40 | **0.0700** | 17.0 |

| claim | original set | held-out | verdict |
|---|---|---|---|
| C1 — ordering `pd < bang-bang < mppi` | holds | holds | **HOLDS** |
| C2 — mppi over pd ≥ 25% | 54.5% | 47.1% | **HOLDS** |
| C3 — bang-bang over pd ≥ 5% | 15.0% | **3.0%** | **FAILS** |

**C3 failed, and it is the claim §8.2 named as the one that could.** The tight pair did not
replicate: 15.0% on the set the metric was chosen from, 3.0% on data that did not inform
the choice. That is what disclosed selection plus fresh data is for — had steps been
presented as the plan all along, a 15% gap would have entered the record as a finding.

**A second reversal, not registered and therefore reported as an observation only.**
Bang-bang's mean final distance went from *best of three* (0.0326) on `HARD_CASES` to
*worst of three* (0.0700) on the held-out set. So §6's finding #2 — "bang-bang has the best
mean final distance" — **is set-specific and does not generalize**; it is corrected in §6.
What survives from that finding is only the narrower part: bang-bang is not nearly-free to
beat, because it reaches the goal every time in both sets.

**What the metric actually discriminates.** Steps separates the *planner* from the
*controllers* robustly (C1 and C2 both hold, at 47–55%). It does **not** reliably separate
the two controllers from each other (C3, 3.0%). So the answer to §6's "what would be needed
to discriminate" is partial: steps is enough to say MPPI is slower than either controller,
and not enough to rank the controllers. Ranking those two would still need a new
registration and a task that punishes overshoot.

## §9 — What `corner_pn_to_np` does and does not localize

`WorldModel/README.md` records `(0.8,−0.8) → (−0.8,0.8)` failing completely under every
latent configuration tested — final distance 2.234–2.272 from a 2.263 start — with two
hypotheses ruled out (latent-vs-position monotonicity along the line; training-data
density) and the cause open.

§6 measured all three arms solving that direction over true dynamics in 18–28 steps. That
supports a **third ruled-out hypothesis**, and it is narrower than it may look:

> The environment is tractable in that direction **and** a controller family as simple as a
> two-gain PD is adequate for it. So the failure is not in the dynamics and not in the
> planner. It lives in the **representation, or in the cost computed from it.**

**Why this does not repeat the retracted comparison.** The retracted draft set
`ParticleNavigatorEnv` + `mpc.py` numbers against ledger node 25, which measured
`synthetic_1f` + `forward_eval.py` — a different env, encoder, task and planner. This claim
compares **no performance numbers across harnesses at all**. It is a statement about one
environment — `ParticleNavigatorEnv`, the same one the failing latent configurations run
in — that it is solvable and that steering it is easy. The latent results are cited only as
the thing being explained, never as a number to beat. That distinction is the whole
argument, so it is written here rather than left for a reader to reconstruct.

**Still not an explanation.** Which of representation-or-cost is at fault, and why that
direction specifically, is untouched. `README.md`'s own remaining hypothesis — multi-step
predictor rollout fidelity for this action direction — is neither supported nor refuted by
anything here.
