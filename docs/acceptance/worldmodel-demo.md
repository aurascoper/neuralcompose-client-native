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

*Empty. To be filled by a single measured run, reported whether or not §3 is met, with
§4 applied as written.*

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
