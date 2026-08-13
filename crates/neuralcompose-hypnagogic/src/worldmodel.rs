//! The world-model demo: a point mass in a box, and three ways to steer it.
//!
//! Two different kinds of work share this module, and conflating them is the
//! main way to misread it.
//!
//! - [`step`] is a **port**, of `WorldModel/env.py`'s `ParticleNavigatorEnv`.
//!   It has ground truth and `tests/env_conformance.rs` holds it to bit-exact
//!   agreement with a fixture generated from the Python itself.
//! - The **planners are not ports.** MPPI over *true analytic dynamics* has no
//!   existing implementation anywhere: Python's `mpc.py::plan_step` takes a
//!   `JEPAModule` and a `goal_latent` as required parameters and dereferences
//!   both unconditionally, and the Swift `WorldModelMPCEngine` plans through
//!   three Core ML models. Neither has a true-dynamics branch. So a result here
//!   is a fresh measurement, registered in `docs/acceptance/worldmodel-demo.md`
//!   before it was run.
//!
//! ## Three things a number from here cannot be compared to
//!
//! 1. **`WorldModel/EXPERIMENT_LEDGER.md`, every node of it.** Nodes 0–25 are
//!    all **`synthetic_1f` + `forward_eval.py`** — a different environment,
//!    encoder, task and planner (CEM, not MPPI). Naming those two files is the
//!    point: written as "not comparable to the ledger", a careless reader takes
//!    the ParticleNavigator-based work as off-limits and the `synthetic_1f`
//!    nodes as fair game, which is precisely inverted.
//! 2. **`WorldModel/README.md`'s own numbers**, same environment though they
//!    are. Its 0/35 hard-case result and 0.13–0.21 aggregate success are MPPI
//!    over a *learned JEPA latent* whose stated ceiling is r≈0.63 to true
//!    position. This plans over exact dynamics. Doing better here says nothing
//!    about that.
//! 3. **The Swift demo**, which has no episode measurements at all.
//!
//! ## EEG is not involved
//!
//! This is a point mass in a box. It carries no physiology and models no
//! cognitive process. Nothing here reads a `SpectralState` or touches the
//! dialectic.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────── the env ──

/// `WorldModel/env.py:28-37`, field for field. Also matches
/// `WorldModelMPCConfig`'s sibling `ParticleNavigatorEnv.Config` in Swift.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvConfig {
    pub arena_half_extent: f32,
    pub dt: f32,
    pub max_accel: f32,
    pub max_speed: f32,
    /// Velocity retained on wall contact. Keeps a collision informative rather
    /// than an absorbing "stick to the wall" state.
    pub restitution: f32,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            arena_half_extent: 1.0,
            dt: 0.1,
            max_accel: 1.0,
            max_speed: 2.0,
            restitution: 0.6,
        }
    }
}

/// `[x, y, vx, vy]`.
pub type State = [f32; 4];
/// `[ax, ay]`.
pub type Action = [f32; 2];

/// One environment step, or `None` if the inputs are not finite.
///
/// **The `None` is a deliberate divergence from both references, and it is the
/// third such divergence this port carries.** NumPy's `np.clip` propagates a
/// NaN action into the state. Swift's `max(-a, min(a, x))` is worse: `NaN < a`
/// is false, so a NaN action silently becomes a full-throttle `maxAccel` one —
/// a real bug, recorded at `WorldModelMPCEngineIntegrationTests.swift:51-55`,
/// where the guard was added to the *engine* rather than to `step`. Laundering
/// an absent value into a plausible one is the defect this codebase keeps
/// finding, so here it is refused at the source.
///
/// Everything else is `env.py:52-72` term for term, and the float semantics are
/// NumPy's, not Rust's idiomatic ones — see [`speed_of`] and the `f64` division
/// below. Both sides are `float32`, so the conformance test asserts bit
/// equality rather than a tolerance.
pub fn step(config: &EnvConfig, state: State, action: Action) -> Option<State> {
    if !state.iter().all(|v| v.is_finite()) || !action.iter().all(|v| v.is_finite()) {
        return None;
    }

    let mut pos = [state[0], state[1]];
    let mut vel = [state[2], state[3]];

    // `np.clip(action, -max_accel, max_accel)`, then `vel + accel * dt`.
    for i in 0..2 {
        let accel = action[i].clamp(-config.max_accel, config.max_accel);
        vel[i] += accel * config.dt;
    }

    // `speed = float(np.linalg.norm(vel))` — computed in f32, then widened.
    // The comparison and the reciprocal happen in f64 because `max_speed` and
    // `speed` are both Python floats at `env.py:60-61`, but the product with
    // `vel` lands back in f32 under NumPy's weak scalar promotion (NEP 50).
    let speed = speed_of(vel) as f64;
    if speed > config.max_speed as f64 {
        let scale = (config.max_speed as f64 / speed) as f32;
        vel[0] *= scale;
        vel[1] *= scale;
    }

    // Position integrates with the ALREADY-CLAMPED velocity (`env.py:63`).
    for i in 0..2 {
        pos[i] += vel[i] * config.dt;
    }

    // Per-axis walls. Position is hard-clamped to the wall — not reflected by
    // penetration depth — and only the velocity carries the restitution. A
    // corner hit therefore bounces both axes independently.
    for i in 0..2 {
        if pos[i] > config.arena_half_extent {
            pos[i] = config.arena_half_extent;
            vel[i] = -vel[i] * config.restitution;
        } else if pos[i] < -config.arena_half_extent {
            pos[i] = -config.arena_half_extent;
            vel[i] = -vel[i] * config.restitution;
        }
    }

    Some([pos[0], pos[1], vel[0], vel[1]])
}

/// `np.linalg.norm` over a length-2 `float32` array: an `f32` dot product and an
/// `f32` square root. Kept as its own function because it is the one place the
/// port's float width is load-bearing, and the fixture records the value so a
/// mismatch names this operation instead of only the resulting state.
pub fn speed_of(vel: [f32; 2]) -> f32 {
    (vel[0] * vel[0] + vel[1] * vel[1]).sqrt()
}

/// Euclidean distance between two states' positions.
pub fn distance(a: State, b: State) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_python_and_swift_values() {
        let c = EnvConfig::default();
        assert_eq!(c.arena_half_extent, 1.0);
        assert_eq!(c.dt, 0.1);
        assert_eq!(c.max_accel, 1.0);
        assert_eq!(c.max_speed, 2.0);
        assert_eq!(c.restitution, 0.6);
    }

    /// The divergence that has to be deliberate. Swift turns a NaN action into
    /// full throttle; NumPy propagates it into the state. Both hide it.
    #[test]
    fn a_non_finite_input_is_refused_rather_than_laundered() {
        let c = EnvConfig::default();
        let good: State = [0.0, 0.0, 0.0, 0.0];
        assert!(step(&c, good, [0.5, 0.5]).is_some());

        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(step(&c, good, [bad, 0.0]), None, "action {bad}");
            assert_eq!(step(&c, good, [0.0, bad]), None, "action {bad}");
            assert_eq!(step(&c, [bad, 0.0, 0.0, 0.0], [0.0, 0.0]), None);
            assert_eq!(step(&c, [0.0, 0.0, bad, 0.0], [0.0, 0.0]), None);
        }
    }

    /// Restitution reverses AND scales, and the position lands exactly on the
    /// wall rather than being reflected past it. Mirrors the Swift unit test
    /// `testRestitutionBounceReversesAndScalesVelocity`.
    #[test]
    fn a_wall_reverses_the_velocity_and_pins_the_position() {
        let c = EnvConfig::default();
        let next = step(&c, [1.0, 0.0, 1.0, 0.0], [0.0, 0.0]).unwrap();
        assert_eq!(next[0], 1.0, "position must sit on the wall");
        assert_eq!(next[2], -0.6, "velocity reverses and scales by restitution");
    }

    /// The clamp is on the L2 NORM, not per axis — a diagonal at the per-axis
    /// limit is over the speed limit and must be scaled.
    #[test]
    fn the_speed_clamp_is_isotropic_not_per_axis() {
        let c = EnvConfig::default();
        // vx = vy = 1.6 -> speed 2.263, over the 2.0 limit.
        let next = step(&c, [0.0, 0.0, 1.6, 1.6], [0.0, 0.0]).unwrap();
        let speed = speed_of([next[2], next[3]]);
        assert!(
            (speed - 2.0).abs() < 1e-5,
            "expected the norm scaled to max_speed, got {speed}"
        );
        assert!(
            (next[2] - next[3]).abs() < 1e-6,
            "an isotropic scale preserves the direction"
        );
    }

    /// Strict `>` at `env.py:60`: a velocity exactly at the limit is untouched.
    #[test]
    fn a_speed_exactly_at_the_limit_is_not_scaled() {
        let c = EnvConfig::default();
        let next = step(&c, [0.0, 0.0, 2.0, 0.0], [0.0, 0.0]).unwrap();
        assert_eq!(next[2], 2.0);
    }

    /// The action clip happens before anything else, so an absurd action is
    /// bounded rather than rejected — this is the one place the port does NOT
    /// refuse a bad input, because the reference defines the behaviour.
    #[test]
    fn an_out_of_range_action_is_clipped_not_refused() {
        let c = EnvConfig::default();
        let next = step(&c, [0.0, 0.0, 0.0, 0.0], [50.0, -50.0]).unwrap();
        // 1.0 * dt = 0.1
        assert_eq!(next[2], 0.1);
        assert_eq!(next[3], -0.1);
    }
}

// ────────────────────────────────────────────────────────────── planning ──

/// `WorldModel/mpc.py:128-174` and `WorldModelMPCConfig.swift`, which already
/// agree 13/13. Ported for field parity; a `field_count` test pins that.
///
/// **The values mean something different here.** They were tuned against a
/// *latent* cost whose ceiling is r≈0.63 to true position. This module's cost is
/// exact position, so `temperature` in particular is calibrated for a
/// distribution that no longer exists. They are carried unchanged anyway,
/// because the alternative — retuning — would make this arm incomparable to the
/// only configuration anyone has written down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpcConfig {
    pub horizon: usize,
    pub num_candidates: usize,
    /// Dimensionless when `adaptive_temperature` is set: it multiplies the
    /// population std of the cost. With adaptation off it is an absolute cost
    /// scale and 0.45 means something entirely different.
    pub temperature: f32,
    pub state_cost_weight: f32,
    pub smoothness_cost_weight: f32,
    pub terminal_cost_weight: f32,
    pub stall_velocity_threshold: f32,
    pub stall_distance_threshold: f32,
    /// 1.0 is a deliberate no-op: a stall is still detected and reported, it
    /// just widens nothing. `mpc.py:154-160` says so explicitly.
    pub stall_variance_multiplier: f32,
    pub stall_widen_fraction: f32,
    pub adaptive_temperature: bool,
    pub min_cost_scale: f32,
    pub normalize_running_cost_by_horizon: bool,
}

impl Default for MpcConfig {
    fn default() -> Self {
        Self {
            horizon: 10,
            num_candidates: 512,
            temperature: 0.45,
            state_cost_weight: 1.0,
            smoothness_cost_weight: 0.1,
            terminal_cost_weight: 2.0,
            stall_velocity_threshold: 0.1,
            stall_distance_threshold: 0.5,
            stall_variance_multiplier: 1.0,
            stall_widen_fraction: 0.25,
            adaptive_temperature: true,
            min_cost_scale: 1e-3,
            normalize_running_cost_by_horizon: false,
        }
    }
}

/// Goal tolerance. `mpc.py:125` and `dataset.py:38` already agree on it.
pub const GOAL_TOLERANCE: f32 = 0.1;

/// splitmix64. Small, deterministic, and seeded on purpose.
///
/// **Seeding is right here and was forbidden for the dialectic**, which is worth
/// being explicit about because the rules look contradictory. [`crate::seams::SelectionDraws`]
/// injects draws rather than seeding because Swift's and Rust's generators
/// diverge from the same seed, so a seeded fixture would fail a *correct* port.
/// That argument is about **cross-language conformance**, which this arm does
/// not have and cannot have — no other implementation of it exists. What matters
/// instead is that a reported measurement can be re-run, so the seed is explicit
/// and travels in the result.
///
/// Not [`crate::seams::SystemDraws`]: it re-reads `subsec_nanos()` per call, and
/// consecutive calls inside a rollout land in the same nanosecond. Not
/// `ScriptedDraws`: it repeats its last draw once exhausted, which across
/// thousands of rollout draws degenerates to a constant action, silently.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn uniform(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit() as f32
    }
}

/// A start and a goal. Positions only; both are lifted to zero velocity, as
/// `sweep.py:81-82` does.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub name: &'static str,
    pub start: [f32; 2],
    pub goal: [f32; 2],
}

impl Scenario {
    pub fn start_state(&self) -> State {
        [self.start[0], self.start[1], 0.0, 0.0]
    }
    pub fn goal_state(&self) -> State {
        [self.goal[0], self.goal[1], 0.0, 0.0]
    }
}

/// `sweep.py:49-57`, verbatim. All four corner diagonals, not just the one the
/// Swift demo happens to use.
///
/// `corner_pn_to_np` is the direction `WorldModel/README.md` records as failing
/// completely under *every* latent configuration tested, with two hypotheses
/// ruled out and the cause unknown. Planning over true dynamics takes the
/// predictor out of the loop entirely, so whatever happens to it here is **not**
/// an explanation of that failure.
pub const HARD_CASES: [Scenario; 7] = [
    Scenario {
        name: "corner_pp_to_nn",
        start: [0.8, 0.8],
        goal: [-0.8, -0.8],
    },
    Scenario {
        name: "corner_nn_to_pp",
        start: [-0.8, -0.8],
        goal: [0.8, 0.8],
    },
    Scenario {
        name: "corner_pn_to_np",
        start: [0.8, -0.8],
        goal: [-0.8, 0.8],
    },
    Scenario {
        name: "corner_np_to_pn",
        start: [-0.8, 0.8],
        goal: [0.8, -0.8],
    },
    Scenario {
        name: "large_a",
        start: [0.7, 0.6],
        goal: [-0.6, -0.6],
    },
    Scenario {
        name: "large_b",
        start: [-0.6, 0.7],
        goal: [0.6, -0.5],
    },
    Scenario {
        name: "large_c",
        start: [0.6, -0.7],
        goal: [-0.5, 0.6],
    },
];

/// The two easy scenarios the Swift demo rotates through alongside the corner
/// case (`WorldModelDemoScenarios.swift:27-33`).
pub const EASY_CASES: [Scenario; 2] = [
    Scenario {
        name: "easy_a",
        start: [0.2, 0.3],
        goal: [-0.3, -0.1],
    },
    Scenario {
        name: "easy_b",
        start: [-0.4, 0.5],
        goal: [0.5, -0.2],
    },
];

pub const DEFAULT_SEEDS: [u64; 5] = [0, 1, 2, 3, 4];

/// One control law. All three take the same inputs so the harness cannot
/// accidentally give one of them more information than another.
pub trait Planner {
    fn id(&self) -> &'static str;
    fn plan(
        &mut self,
        env: &EnvConfig,
        state: State,
        goal: State,
        prev_action: Option<Action>,
    ) -> Action;
    /// Stall detections so far. Zero for the controllers, which cannot stall by
    /// construction — they never stop commanding.
    fn stalls(&self) -> u32 {
        0
    }
}

/// `BaselineWorldModelDemoPlanner.swift:25-34`, ported exactly.
///
/// **Its docs call it proportional; it is not.** It normalizes the error to a
/// unit vector and multiplies by `max_accel`, so the commanded magnitude is
/// always exactly `max_accel` regardless of distance — bang-bang, and the
/// per-component clamp in the Swift is a no-op because `|dx/norm| <= 1`. It
/// reads no velocity, so it cannot brake.
///
/// Present for parity with what the Swift demo ships. **Never the bar**: beating
/// a controller that cannot brake is not evidence about planning.
#[derive(Clone, Debug, Default)]
pub struct BangBang;

impl Planner for BangBang {
    fn id(&self) -> &'static str {
        "bang-bang"
    }
    fn plan(&mut self, env: &EnvConfig, state: State, goal: State, _: Option<Action>) -> Action {
        let dx = goal[0] - state[0];
        let dy = goal[1] - state[1];
        let norm = (dx * dx + dy * dy).sqrt();
        if norm > 1e-6 {
            [dx / norm * env.max_accel, dy / norm * env.max_accel]
        } else {
            [0.0, 0.0]
        }
    }
}

/// A damped controller: `kp * (goal - pos) - kd * vel`, clipped.
///
/// The gains are `dataset.py:39-40`'s `WAYPOINT_KP = 3.0` / `WAYPOINT_KD = 1.0`.
/// **The target is not.** `dataset.py::_heuristic_action` steers at a randomly
/// resampled waypoint because its job is generating varied training data, not
/// reaching a goal; pointing it at the goal is a deliberate change and the
/// reason this is not called a port.
///
/// This is the honest bar. Unlike bang-bang it can brake, so beating it means
/// something — and if it clears the threshold too, the registered rule in
/// `docs/acceptance/worldmodel-demo.md` §4 says the task did not discriminate.
#[derive(Clone, Debug)]
pub struct PdController {
    pub kp: f32,
    pub kd: f32,
}

impl Default for PdController {
    fn default() -> Self {
        Self { kp: 3.0, kd: 1.0 }
    }
}

impl Planner for PdController {
    fn id(&self) -> &'static str {
        "pd"
    }
    fn plan(&mut self, env: &EnvConfig, state: State, goal: State, _: Option<Action>) -> Action {
        let mut out = [0.0f32; 2];
        for i in 0..2 {
            let a = self.kp * (goal[i] - state[i]) - self.kd * state[i + 2];
            out[i] = a.clamp(-env.max_accel, env.max_accel);
        }
        out
    }
}

/// MPPI over the true dynamics.
///
/// Cost structure is `mpc.py::score_candidates` with **position distance
/// replacing latent distance** and nothing else changed: a running cost-to-go
/// summed over the horizon, a separately-weighted *squared* terminal term (the
/// double appearance is deliberate there and kept here), and an action
/// smoothness penalty continuous across replans via `prev_action`.
///
/// Candidates are i.i.d. **uniform** within the actuation bounds, as
/// `mpc.py:177-184` samples them — not Gaussian around a shifted mean. Every
/// sampled action is already valid, so no mass is spent on values `step` would
/// clip anyway.
#[derive(Clone, Debug)]
pub struct Mppi {
    pub config: MpcConfig,
    rng: Rng,
    stalls: u32,
}

impl Mppi {
    pub fn new(config: MpcConfig, seed: u64) -> Self {
        Self {
            config,
            rng: Rng::new(seed),
            stalls: 0,
        }
    }
}

impl Planner for Mppi {
    fn id(&self) -> &'static str {
        "mppi"
    }
    fn stalls(&self) -> u32 {
        self.stalls
    }

    fn plan(
        &mut self,
        env: &EnvConfig,
        state: State,
        goal: State,
        prev_action: Option<Action>,
    ) -> Action {
        let c = self.config;
        let n = c.num_candidates.max(1);
        let h = c.horizon.max(1);

        // Stall detection, reported whether or not it widens anything.
        let speed = speed_of([state[2], state[3]]);
        let dist = distance(state, goal);
        let stalled = speed < c.stall_velocity_threshold && dist > c.stall_distance_threshold;
        if stalled {
            self.stalls += 1;
        }
        let widen_from = if stalled && c.stall_variance_multiplier > 1.0 {
            let frac = c.stall_widen_fraction.clamp(0.0, 1.0);
            n - ((n as f32 * frac) as usize).min(n)
        } else {
            n
        };

        let mut actions = vec![[0.0f32; 2]; n * h];
        let mut costs = vec![0.0f32; n];

        for i in 0..n {
            // Widened candidates are drawn last, matching the concatenation
            // order in `mpc.py:279-285`.
            let bound = if i >= widen_from {
                env.max_accel * c.stall_variance_multiplier
            } else {
                env.max_accel
            };

            let mut s = state;
            let mut running = 0.0f32;
            let mut smoothness = 0.0f32;
            let mut last = prev_action;
            let mut escaped = false;

            for t in 0..h {
                let a = [
                    self.rng.uniform(-bound, bound),
                    self.rng.uniform(-bound, bound),
                ];
                actions[i * h + t] = a;
                if let Some(p) = last {
                    let dx = a[0] - p[0];
                    let dy = a[1] - p[1];
                    smoothness += (dx * dx + dy * dy).sqrt();
                }
                last = Some(a);

                match step(env, s, a) {
                    Some(next) => s = next,
                    // Cannot happen from finite inputs, but a rollout that fell
                    // over must not silently score as a good one.
                    None => {
                        escaped = true;
                        break;
                    }
                }
                running += distance(s, goal);
            }

            if escaped {
                costs[i] = f32::MAX;
                continue;
            }
            let terminal = distance(s, goal).powi(2);
            let running = if c.normalize_running_cost_by_horizon {
                running / h as f32
            } else {
                running
            };
            costs[i] = c.state_cost_weight * running
                + c.smoothness_cost_weight * smoothness
                + c.terminal_cost_weight * terminal;
        }

        // softmax(-(cost - min) / temperature_effective), with the population
        // std (divisor N, matching torch's `unbiased=False`) when adaptive.
        let min = costs.iter().copied().fold(f32::INFINITY, f32::min);
        let temp = if c.adaptive_temperature {
            let mean = costs.iter().sum::<f32>() / n as f32;
            let var = costs.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n as f32;
            c.temperature * var.sqrt().max(c.min_cost_scale)
        } else {
            c.temperature
        }
        .max(f32::MIN_POSITIVE);

        let mut weights = vec![0.0f32; n];
        let mut total = 0.0f32;
        for i in 0..n {
            let w = (-(costs[i] - min) / temp).exp();
            weights[i] = w;
            total += w;
        }
        if !(total.is_finite() && total > 0.0) {
            // Degenerate weighting: fall back to the single best candidate
            // rather than emitting a NaN action.
            let best = costs
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0);
            return actions[best * h];
        }

        let mut blended = [0.0f32; 2];
        for i in 0..n {
            let w = weights[i] / total;
            blended[0] += w * actions[i * h][0];
            blended[1] += w * actions[i * h][1];
        }
        [
            blended[0].clamp(-env.max_accel, env.max_accel),
            blended[1].clamp(-env.max_accel, env.max_accel),
        ]
    }
}

/// What one episode did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodeResult {
    pub scenario: String,
    pub planner: String,
    pub seed: u64,
    pub steps: usize,
    pub final_distance: f32,
    pub reached: bool,
    pub stalls: u32,
}

/// Drive one planner through one scenario.
///
/// Goal tolerance is checked **before** stepping and again after the loop, as
/// `mpc.py::run_episode` does — so an episode that starts inside tolerance
/// records zero steps rather than one.
pub fn run_episode(
    env: &EnvConfig,
    planner: &mut dyn Planner,
    scenario: &Scenario,
    seed: u64,
    max_steps: usize,
    goal_tolerance: f32,
) -> EpisodeResult {
    let goal = scenario.goal_state();
    let mut state = scenario.start_state();
    let mut prev: Option<Action> = None;
    let mut steps = 0;

    for _ in 0..max_steps {
        if distance(state, goal) < goal_tolerance {
            break;
        }
        let action = planner.plan(env, state, goal, prev);
        state = match step(env, state, action) {
            Some(next) => next,
            None => break,
        };
        prev = Some(action);
        steps += 1;
    }

    let final_distance = distance(state, goal);
    EpisodeResult {
        scenario: scenario.name.to_string(),
        planner: planner.id().to_string(),
        seed,
        steps,
        final_distance,
        reached: final_distance < goal_tolerance,
        stalls: planner.stalls(),
    }
}

// ──────────────────────────────────────────────────────────── provenance ──

pub const WORLDMODEL_METHOD_ID: &str = "neuralcompose.hypnagogic.worldmodel-demo.v1";

/// The build and the frozen parameters a demo run happened under.
///
/// Same recipe as [`crate::turn_log::dialectic_method_identity`]: a local
/// `Params` struct over the real config types so no field list is maintained by
/// hand, then `bit_exact_numbers` so the digest cannot depend on how a float was
/// rendered to decimal, then sha256.
pub fn worldmodel_method_identity(
    env: &EnvConfig,
    mpc: &MpcConfig,
    software_version: impl Into<String>,
    git_commit: Option<String>,
) -> neuralcompose_mobile_core::provenance::MethodIdentity {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Params<'a> {
        domain: &'static str,
        env: &'a EnvConfig,
        mpc: &'a MpcConfig,
        goal_tolerance: f32,
    }
    let mut doc = serde_json::to_value(Params {
        domain: WORLDMODEL_METHOD_ID,
        env,
        mpc,
        goal_tolerance: GOAL_TOLERANCE,
    })
    .expect("configs are always serializable");
    crate::turn_log::bit_exact_numbers(&mut doc);
    neuralcompose_mobile_core::provenance::MethodIdentity {
        method_id: WORLDMODEL_METHOD_ID.to_string(),
        software_id: crate::turn_log::SOFTWARE_ID.to_string(),
        software_version: software_version.into(),
        git_commit,
        parameters_digest: neuralcompose_mobile_core::audio::sha256_hex(
            serde_json::to_vec(&doc).expect("a rewritten document serializes"),
        ),
    }
}

/// The envelope a demo result carries.
///
/// **`AgentInference`, and the reasoning matters more than the choice.** Not
/// `Observed`: upstream reserves that for an artifact ingested through a channel
/// no agent tool can reach, and this binary is exactly such a tool. Not
/// `DerivedDeterministically`: that class's whole meaning is that the evidence
/// store re-executes the transform and rejects a mismatch, and no store can
/// re-execute this planner — claiming it would be borrowing a guarantee nobody
/// provides.
///
/// So the honest class is the advisory one, and promoting a result to `Observed`
/// means an operator ingesting the emitted document out of band. **The taxonomy
/// has no slot for "measured exactly by this software on a synthetic task"**,
/// which is a real gap rather than a modelling preference; ADR-004 records it.
pub fn demo_envelope(
    method: neuralcompose_mobile_core::provenance::MethodIdentity,
    inputs: Vec<neuralcompose_mobile_core::provenance::ResourceRef>,
) -> neuralcompose_mobile_core::provenance::ProvenanceEnvelope {
    use neuralcompose_mobile_core::provenance::*;
    ProvenanceEnvelope {
        schema_id: PROVENANCE_ENVELOPE_SCHEMA.to_string(),
        assertion_kind: AssertionKind::AgentInference,
        method: Some(method),
        inputs,
        // A count of successes out of 35 is exact, not scored. A confidence here
        // would be inventing uncertainty the measurement does not have.
        confidence: None,
        comparison_embedding_space: None,
    }
}

#[cfg(test)]
mod planner_tests {
    use super::*;

    /// Field parity with `mpc.py::MPCConfig` and `WorldModelMPCConfig.swift`,
    /// which already agree 13/13. Counted through serde so adding a field to the
    /// struct without deciding whether Python has it breaks the build's tests.
    #[test]
    fn the_mpc_config_has_the_same_thirteen_fields_as_python_and_swift() {
        let v = serde_json::to_value(MpcConfig::default()).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 13, "field count drifted: {:?}", obj.keys());
        for name in [
            "horizon",
            "numCandidates",
            "temperature",
            "stateCostWeight",
            "smoothnessCostWeight",
            "terminalCostWeight",
            "stallVelocityThreshold",
            "stallDistanceThreshold",
            "stallVarianceMultiplier",
            "stallWidenFraction",
            "adaptiveTemperature",
            "minCostScale",
            "normalizeRunningCostByHorizon",
        ] {
            assert!(obj.contains_key(name), "missing {name}");
        }
    }

    #[test]
    fn the_mpc_defaults_are_the_python_values() {
        let c = MpcConfig::default();
        assert_eq!(c.horizon, 10);
        assert_eq!(c.num_candidates, 512);
        assert_eq!(c.temperature, 0.45);
        assert_eq!(c.terminal_cost_weight, 2.0);
        assert_eq!(
            c.stall_variance_multiplier, 1.0,
            "1.0 is a no-op ON PURPOSE"
        );
        assert!(c.adaptive_temperature);
        assert!(!c.normalize_running_cost_by_horizon);
    }

    #[test]
    fn the_hard_cases_are_the_python_seven_including_all_four_diagonals() {
        assert_eq!(HARD_CASES.len(), 7);
        let corners: Vec<&str> = HARD_CASES
            .iter()
            .map(|s| s.name)
            .filter(|n| n.starts_with("corner_"))
            .collect();
        assert_eq!(corners.len(), 4, "all four corner diagonals, not just one");
        let pp = HARD_CASES[0];
        assert_eq!(pp.start, [0.8, 0.8]);
        assert_eq!(pp.goal, [-0.8, -0.8]);
    }

    /// The seed is what makes a reported measurement re-runnable. Same seed,
    /// same actions; different seed, different actions.
    #[test]
    fn the_planner_is_reproducible_from_its_seed_and_varies_without_it() {
        let env = EnvConfig::default();
        let s = HARD_CASES[0];
        let run = |seed| {
            let mut p = Mppi::new(MpcConfig::default(), seed);
            (0..3)
                .map(|_| p.plan(&env, s.start_state(), s.goal_state(), None))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(7), run(7), "same seed must replay");
        assert_ne!(run(7), run(8), "different seeds must diverge");
    }

    /// Bang-bang commands full thrust regardless of distance — that is what
    /// makes it bang-bang rather than the "proportional" its docs claim.
    #[test]
    fn bang_bang_is_saturated_at_every_distance() {
        let env = EnvConfig::default();
        let mut p = BangBang;
        for far in [0.2f32, 1.0, 1.9] {
            let a = p.plan(&env, [0.0, 0.0, 0.0, 0.0], [far, 0.0, 0.0, 0.0], None);
            assert!(
                (a[0] - env.max_accel).abs() < 1e-6,
                "distance {far} gave {a:?}, not full thrust"
            );
        }
    }

    /// The PD arm brakes; bang-bang cannot. This is the difference that makes
    /// PD the honest bar.
    #[test]
    fn pd_brakes_when_moving_at_the_goal_and_bang_bang_does_not() {
        let env = EnvConfig::default();
        // Sitting on the goal, moving fast: the only correct action is to slow.
        let state = [0.0, 0.0, 1.5, 0.0];
        let goal = [0.0, 0.0, 0.0, 0.0];
        let pd = PdController::default().plan(&env, state, goal, None);
        assert!(pd[0] < 0.0, "pd must decelerate, got {pd:?}");
        let bb = BangBang.plan(&env, state, goal, None);
        assert_eq!(
            bb,
            [0.0, 0.0],
            "bang-bang has no velocity term to brake with"
        );
    }

    /// The one constant the entire registered measurement rests on.
    ///
    /// `GOAL_TOLERANCE` is the definition of "reached", so §3's 33/35 bar means
    /// nothing without it. Loosening it to 1.0 survived the whole suite until
    /// this test existed: every arm would then "reach" from anywhere inside the
    /// arena and the result would read 35/35 exactly as it does now. A shipped
    /// constant that nothing pins is the same defect `stalemate_margin` had.
    #[test]
    fn the_goal_tolerance_is_pinned_and_actually_discriminates() {
        assert_eq!(
            GOAL_TOLERANCE, 0.1,
            "mpc.py:125 and dataset.py:38 both say 0.1; changing it invalidates \
             docs/acceptance/worldmodel-demo.md §3"
        );

        // It must separate near from far. At 1.0 this assertion fails, which is
        // the point: half the arena would count as arrived.
        let near: State = [0.05, 0.0, 0.0, 0.0];
        let far: State = [0.5, 0.0, 0.0, 0.0];
        let goal: State = [0.0, 0.0, 0.0, 0.0];
        assert!(distance(near, goal) < GOAL_TOLERANCE);
        assert!(
            distance(far, goal) > GOAL_TOLERANCE,
            "a point half the arena away must not count as reached"
        );

        // And an episode must be able to FAIL, or "reached" is not a measurement.
        let env = EnvConfig::default();
        let unreachable = Scenario {
            name: "far",
            start: [0.9, 0.9],
            goal: [-0.9, -0.9],
        };
        let r = run_episode(&env, &mut Stationary, &unreachable, 0, 50, GOAL_TOLERANCE);
        assert!(
            !r.reached,
            "a planner that never moves must not reach the goal"
        );
    }

    /// Commands nothing. Exists only so the test above can show that an episode
    /// is capable of failing.
    struct Stationary;
    impl Planner for Stationary {
        fn id(&self) -> &'static str {
            "stationary"
        }
        fn plan(&mut self, _: &EnvConfig, _: State, _: State, _: Option<Action>) -> Action {
            [0.0, 0.0]
        }
    }

    /// An episode that starts inside tolerance takes zero steps — the check is
    /// before the step, as `mpc.py::run_episode` does it.
    #[test]
    fn an_episode_already_at_the_goal_takes_no_steps() {
        let env = EnvConfig::default();
        let s = Scenario {
            name: "here",
            start: [0.0, 0.0],
            goal: [0.0, 0.0],
        };
        let r = run_episode(&env, &mut BangBang, &s, 0, 50, GOAL_TOLERANCE);
        assert_eq!(r.steps, 0);
        assert!(r.reached);
    }
}
