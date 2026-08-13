// `ParticleNavigatorEnv` port conformance — the check that makes "port" a claim
// anyone can test rather than a description.
//
// The fixture is generated from `WorldModel/env.py` ITSELF, not from a
// reimplementation. A fixture built from a second implementation of the same
// prose would agree with this port by construction and prove nothing.
//
// **A missing fixture FAILS. It does not skip.** A conformance test that quietly
// passes when its ground truth is absent reports green for exactly the state it
// exists to detect — the same argument `dialectic_conformance.rs` makes.
//
// Regenerate with (the interpreter matters: it is the one `band_power`'s
// 12-significant-figure reference numbers came from):
//
//   ~/src/NeuralCompose/.venv-muse/bin/python \
//       tools/worldmodel-fixture/generate.py \
//       > crates/neuralcompose-hypnagogic/tests/fixtures/env_v1.json
//
// Agreement is BIT-EXACT, not a tolerance. Both sides are float32 and
// deterministic; the fixture stores each value as a float32 widened to float64,
// which is lossless, so a tolerance here could only ever hide a real divergence.

use neuralcompose_hypnagogic::worldmodel::{speed_of, step, EnvConfig, State};
use serde::Deserialize;

const FIXTURE_PATH: &str = "tests/fixtures/env_v1.json";
const SCHEMA: &str = "neuralcompose.hypnagogic.env-conformance.v1";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_id: String,
    config: EnvConfig,
    initial_state: Vec<f64>,
    coverage: Coverage,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Coverage {
    steps: usize,
    bounced_x: usize,
    bounced_y: usize,
    corner_hits: usize,
    clamped_steps: usize,
    unclamped_steps: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    action: Vec<f64>,
    next: Vec<f64>,
    /// Diagnostic: the pre-clamp speed Python computed. Recorded so a mismatch
    /// names the operation that diverged rather than only the resulting state.
    speed_before_clamp: f64,
    clamped: bool,
}

fn fixture() -> Fixture {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_PATH);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "env conformance fixture missing at {} ({e}).\n\n\
             The Rust port of ParticleNavigatorEnv is UNVERIFIED without it — this\n\
             test does not skip, because a conformance check that passes when its\n\
             ground truth is absent is worse than no check.\n\n\
             Regenerate with:\n  \
             ~/src/NeuralCompose/.venv-muse/bin/python \\\n    \
             tools/worldmodel-fixture/generate.py \\\n    \
             > crates/neuralcompose-hypnagogic/{FIXTURE_PATH}\n\n\
             Set NEURALCOMPOSE_DIR if the sibling checkout is elsewhere. Verify the\n\
             result parses before trusting it (`python3 -m json.tool`): a generator\n\
             that writes a diagnostic to stdout produces a file of the right size\n\
             and shape that is not valid JSON.",
            path.display()
        )
    });
    serde_json::from_str(&raw).expect("fixture is valid JSON matching this schema")
}

fn as_state(v: &[f64]) -> State {
    assert_eq!(v.len(), 4, "a state is four values");
    [v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32]
}

/// Every step reproduces Python exactly. Divergences accumulate so one run
/// reports all of them, not just the first.
#[test]
fn every_step_reproduces_the_python_env_bit_for_bit() {
    let f = fixture();
    assert_eq!(f.schema_id, SCHEMA, "fixture schema changed");

    let mut state = as_state(&f.initial_state);
    let mut failures: Vec<String> = Vec::new();

    for (i, s) in f.steps.iter().enumerate() {
        let action = [s.action[0] as f32, s.action[1] as f32];
        let expected = as_state(&s.next);

        let got = match step(&f.config, state, action) {
            Some(next) => next,
            None => {
                failures.push(format!("step {i}: the port refused a finite input"));
                break;
            }
        };

        if got != expected {
            // Name the diverging operation, not just the state.
            let pre_clamp = {
                let mut v = [state[2], state[3]];
                for k in 0..2 {
                    v[k] += action[k].clamp(-f.config.max_accel, f.config.max_accel) * f.config.dt;
                }
                speed_of(v)
            };
            failures.push(format!(
                "step {i}: action {action:?}\n    \
                 expected {expected:?}\n    \
                 got      {got:?}\n    \
                 speed before clamp: python {} rust {pre_clamp} (python clamped: {})",
                s.speed_before_clamp, s.clamped
            ));
            if failures.len() >= 5 {
                failures.push("… stopping after 5".into());
                break;
            }
        }
        state = expected; // continue from Python's state so one error is not N
    }

    assert!(
        failures.is_empty(),
        "the Rust port diverges from WorldModel/env.py in {} place(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// The fixture must actually reach the states that discriminate. Without this,
/// a trajectory that never touches a wall would let a port with no wall handling
/// pass, and one that never saturates would let a port with no speed limit pass.
/// This is the meta-test against the always-passing shape.
#[test]
fn the_fixture_reaches_the_states_that_discriminate() {
    let f = fixture();
    let c = &f.coverage;

    assert!(c.steps >= 200, "only {} steps", c.steps);
    assert_eq!(
        c.steps,
        f.steps.len(),
        "coverage count disagrees with the data"
    );
    assert!(
        c.bounced_x > 0,
        "no x-wall bounce: wall handling is unexercised"
    );
    assert!(c.bounced_y > 0, "no y-wall bounce");
    assert!(
        c.corner_hits > 0,
        "no corner hit: both axes bouncing in one step"
    );
    assert!(c.clamped_steps > 0, "the speed clamp never fires");
    assert!(
        c.unclamped_steps > 0,
        "the clamp fires on EVERY step — a port that always scales would pass"
    );

    // And the recorded flags must match the data, or the coverage claim is prose.
    let clamped = f.steps.iter().filter(|s| s.clamped).count();
    assert_eq!(
        clamped, c.clamped_steps,
        "clamped count is not what the steps say"
    );
}

/// The port must stay inside the arena for the whole trajectory. Cheap, and it
/// catches a dropped wall clamp even if the fixture comparison were loosened.
#[test]
fn the_particle_never_leaves_the_arena() {
    let f = fixture();
    let mut state = as_state(&f.initial_state);
    for (i, s) in f.steps.iter().enumerate() {
        state = step(&f.config, state, [s.action[0] as f32, s.action[1] as f32])
            .unwrap_or_else(|| panic!("step {i} refused a finite input"));
        for (axis, v) in state.iter().take(2).enumerate() {
            assert!(
                v.abs() <= f.config.arena_half_extent,
                "step {i}: axis {axis} escaped to {v}"
            );
        }
        let speed = speed_of([state[2], state[3]]);
        assert!(
            speed <= f.config.max_speed + 1e-5,
            "step {i}: speed {speed} exceeds the limit"
        );
    }
}
