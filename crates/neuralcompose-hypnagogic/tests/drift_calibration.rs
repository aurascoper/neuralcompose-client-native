//! Does the shipped `drift_ceiling` separate real embeddings?
//!
//! `drift_anchor.rs` proves the *mechanism* with a synthetic embedder that puts
//! on- and off-topic text on orthogonal axes. That is the right tool for
//! mechanism and the wrong one for the constant: real sentence embeddings are
//! never orthogonal, they sit in a narrow cone, and a ceiling tuned against
//! orthogonal vectors is far too high for any of them.
//!
//! That is not hypothetical. The first version of this feature shipped
//! `drift_ceiling: 0.45`, chosen from the theoretical `0.0..=1.0` range and
//! validated only against the synthetic embedder. Measured against
//! `bge-small-en-v1.5`, ten utterances about kettles, bus timetables and gulls
//! drifted **0.294 to 0.347** from a biofilms anchor. The ceiling fired on none
//! of them. Every test passed, because none of them used the real model — a
//! bound nothing can reach is indistinguishable from a guard that is satisfied.
//!
//! So this file pins the measurement. It runs no model: the numbers were
//! captured from two live sessions and committed, and the assertions are about
//! whether the shipped constant sits in the gap those numbers describe.
//!
//! **A missing fixture FAILS. It does not skip.**
//!
//! Recapture and regenerate: see `tools/drift-calibration/generate.py`.

use neuralcompose_hypnagogic::dialectic::DialecticConfig;
use serde::Deserialize;

const FIXTURE_PATH: &str = "tests/fixtures/drift_calibration_v1.json";
const SCHEMA: &str = "neuralcompose.hypnagogic.drift-calibration.v1";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_id: String,
    separation: Separation,
    on_topic: Vec<Case>,
    off_topic: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Separation {
    on_topic_max: f32,
    off_topic_min: f32,
}

#[derive(Deserialize)]
struct Case {
    heard: String,
    drift: f32,
}

fn fixture() -> Fixture {
    let raw = std::fs::read_to_string(FIXTURE_PATH).unwrap_or_else(|e| {
        panic!(
            "{FIXTURE_PATH} is missing or unreadable ({e}). This test does not \
             skip without its ground truth — see tools/drift-calibration/generate.py."
        )
    });
    let f: Fixture = serde_json::from_str(&raw).expect("fixture is valid JSON");
    assert_eq!(f.schema_id, SCHEMA, "fixture schema id changed");
    f
}

/// The two classes must actually be separable by the metric. If they are not,
/// no ceiling exists and the whole mechanism is unsupported — that would be a
/// finding, not a threshold to tune.
#[test]
fn on_topic_and_off_topic_drift_do_not_overlap() {
    let f = fixture();
    assert!(
        f.separation.on_topic_max < f.separation.off_topic_min,
        "the classes overlap: on-topic reaches {} while off-topic starts at {}",
        f.separation.on_topic_max,
        f.separation.off_topic_min
    );
}

/// The shipped constant must sit inside that gap.
///
/// This is the assertion the 0.45 default would have failed, and the reason it
/// exists. Note it does NOT pin an exact value — any ceiling in the gap passes,
/// so a re-fit is free while a value outside the measured range is not.
#[test]
fn the_shipped_ceiling_sits_between_the_two_classes() {
    let f = fixture();
    let ceiling = DialecticConfig::default().drift_ceiling;
    assert!(
        ceiling > f.separation.on_topic_max,
        "ceiling {ceiling} is at or below the on-topic maximum {} — every \
         on-topic turn would be re-anchored",
        f.separation.on_topic_max
    );
    assert!(
        ceiling < f.separation.off_topic_min,
        "ceiling {ceiling} is at or above the off-topic minimum {} — nothing \
         reaches it, which is what shipping 0.45 did",
        f.separation.off_topic_min
    );
}

/// Every recorded turn must be classified the way it was labelled. The summary
/// in `separation` is derived, and a derived number can be stale; this walks the
/// cases themselves.
#[test]
fn the_shipped_ceiling_classifies_every_recorded_turn_correctly() {
    let f = fixture();
    let ceiling = DialecticConfig::default().drift_ceiling;
    let mut wrong = Vec::new();
    for c in &f.on_topic {
        if c.drift > ceiling {
            wrong.push(format!("on-topic re-anchored ({}): {:?}", c.drift, c.heard));
        }
    }
    for c in &f.off_topic {
        if c.drift <= ceiling {
            wrong.push(format!("off-topic missed ({}): {:?}", c.drift, c.heard));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} recorded turns misclassified at ceiling {ceiling}:\n{}",
        wrong.len(),
        f.on_topic.len() + f.off_topic.len(),
        wrong.join("\n")
    );
}
