//! Replay of session `1788413094` against the repetition floor.
//!
//! **This replays a session the non-speech guard now prevents, on purpose.**
//! Non-speech tags are not the only route to a fixed point: a genuinely stuck
//! conversation with real input looks the same to this detector, and the tag
//! filter would not help there. The noise run is simply the only recorded
//! example of the shape.
//!
//! Unlike `nonspeech_guard.rs` there is no upstream reference implementation to
//! label against — nothing classifies a turn as "repetitive" but this code. So
//! the fixture pins the replies and the two **regions**, the region boundary is
//! a human judgement recorded in the fixture with its reason, and the
//! assertions are about behaviour across those regions. **The floor itself is
//! never asserted**: a test that pinned 0.30 would pass by restating the
//! constant and would go red on a legitimate re-fit.
//!
//! **A missing fixture FAILS. It does not skip.** Same rule as the other three
//! fixture suites.
//!
//! Regenerate (emits `nonspeech_v1.json` too, from one read, so both pin one
//! digest):
//!
//!   python3 tools/session-fixtures/generate.py \
//!       ~/Documents/NeuralCompose/InteractionLogs/session-1788413094.turns.jsonl \
//!       crates/neuralcompose-hypnagogic/tests/fixtures

use neuralcompose_hypnagogic::dialectic::{DialecticConfig, DialecticalMemory};
use neuralcompose_hypnagogic::loops::similarity;
use serde::Deserialize;

const FIXTURE_PATH: &str = "tests/fixtures/repetition_v1.json";
const SCHEMA: &str = "neuralcompose.hypnagogic.repetition-floor.v1";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_id: String,
    regions: Regions,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Regions {
    healthy: Vec<u64>,
    stuck: Vec<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    index: u64,
    spoken_text: Option<String>,
    self_similarity: Option<f32>,
}

fn fixture() -> Fixture {
    let raw = std::fs::read_to_string(FIXTURE_PATH).unwrap_or_else(|e| {
        panic!(
            "{FIXTURE_PATH} is missing or unreadable ({e}). This test does not \
             skip without its ground truth — regenerate it with \
             tools/session-fixtures/generate.py (see the module header)."
        )
    });
    let f: Fixture = serde_json::from_str(&raw).expect("fixture is valid JSON");
    assert_eq!(f.schema_id, SCHEMA, "fixture schema id changed");
    f
}

/// Feed the recorded replies through the real ring and report the first turn at
/// which the guard would have fired, using the shipped configuration.
fn first_fire(f: &Fixture, cfg: &DialecticConfig) -> Option<u64> {
    let mut memory = DialecticalMemory::new(16, 0.5);
    for c in &f.cases {
        let Some(text) = c.spoken_text.as_deref() else {
            continue;
        };
        if memory.repetition_hits(cfg.repetition_floor) >= cfg.repetition_hits {
            return Some(c.index);
        }
        memory.record_reply_text(text.to_string(), cfg.repetition_window);
    }
    None
}

#[test]
fn the_guard_fires_inside_the_stuck_region_and_never_in_the_healthy_one() {
    let f = fixture();
    let cfg = DialecticConfig::default();
    let fired = first_fire(&f, &cfg).expect(
        "the guard never fired on a session that spent fifty-four consecutive \
         turns replying to a fan — this corpus is the reason the check exists",
    );
    assert!(
        f.regions.stuck.contains(&fired),
        "fired at turn {fired}, which is not in the stuck region"
    );
    assert!(
        !f.regions.healthy.contains(&fired),
        "fired at turn {fired}, a healthy turn — a false positive on the only \
         reference data there is"
    );
}

/// The healthy region must survive the whole way through, not merely avoid
/// being the *first* fire. Replaying only the healthy replies must never trip.
#[test]
fn the_healthy_region_alone_never_trips_the_floor() {
    let f = fixture();
    let cfg = DialecticConfig::default();
    let mut memory = DialecticalMemory::new(16, 0.5);
    for c in f
        .cases
        .iter()
        .filter(|c| f.regions.healthy.contains(&c.index))
    {
        let Some(text) = c.spoken_text.as_deref() else {
            continue;
        };
        assert!(
            memory.repetition_hits(cfg.repetition_floor) < cfg.repetition_hits,
            "the floor tripped at healthy turn {}",
            c.index
        );
        memory.record_reply_text(text.to_string(), cfg.repetition_window);
    }
}

/// The guard is a RATE and needs more than one near-repeat.
///
/// Without this, lowering `repetition_hits` to 1 survived every test: firing
/// earlier still fired inside the stuck region, so nothing noticed that the
/// guard had become a hair trigger. This asserts the property — two near-repeats
/// are not enough — rather than the constant, so a legitimate re-fit from 3 to 4
/// keeps passing while a collapse to 1 does not.
#[test]
fn two_near_repeats_are_not_enough_to_fire() {
    let cfg = DialecticConfig::default();
    let mut memory = DialecticalMemory::new(16, 0.5);
    let repeated = "the machine whirring is a sound that suggests a mechanical process";
    // Three identical replies yield two hits: the second and third each match
    // something earlier, the first matches nothing.
    for _ in 0..3 {
        memory.record_reply_text(repeated.to_string(), cfg.repetition_window);
    }
    assert_eq!(memory.repetition_hits(cfg.repetition_floor), 2);
    assert!(
        memory.repetition_hits(cfg.repetition_floor) < cfg.repetition_hits,
        "the guard fires on two near-repeats — it is a trigger, not a rate"
    );
}

/// The rejection of `self_similarity`, kept checkable rather than quoted.
///
/// This is the claim the whole design rests on: the metric that was already
/// computed and already logged cannot separate these two regions, and the
/// lexical one can. If a future embedder ever makes the cosine separable, this
/// test goes red and the design should be revisited — which is the point of
/// asserting it rather than writing it in a comment.
#[test]
fn self_similarity_does_not_separate_the_regions_but_token_overlap_does() {
    let f = fixture();
    let val = |idx: &[u64]| -> Vec<f32> {
        f.cases
            .iter()
            .filter(|c| idx.contains(&c.index))
            .filter_map(|c| c.self_similarity)
            .collect()
    };
    let healthy = val(&f.regions.healthy);
    let stuck = val(&f.regions.stuck);
    let max = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max);
    let min = |v: &[f32]| v.iter().cloned().fold(f32::MAX, f32::min);
    assert!(
        min(&stuck) < max(&healthy),
        "self_similarity now separates the regions (stuck min {} >= healthy max \
         {}). The comment in loops::similarity says it does not — one of them \
         is out of date.",
        min(&stuck),
        max(&healthy)
    );

    // The lexical measure, same two regions, must separate.
    let overlap = |idx: &[u64]| -> Vec<f32> {
        let texts: Vec<&str> = f
            .cases
            .iter()
            .filter(|c| idx.contains(&c.index))
            .filter_map(|c| c.spoken_text.as_deref())
            .collect();
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                texts[..i]
                    .iter()
                    .rev()
                    .take(5)
                    .map(|p| similarity(p, t))
                    .fold(0.0f32, f32::max)
            })
            .collect()
    };
    assert!(
        max(&overlap(&f.regions.healthy)) < max(&overlap(&f.regions.stuck)),
        "token overlap no longer separates the regions"
    );
}
