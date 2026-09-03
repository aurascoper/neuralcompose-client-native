//! Differential against `tools/spoken-loop/converse.py:404` — the guard this
//! port dropped.
//!
//! `is_non_speech` restores a line the Python loop has and the Rust port lost.
//! The ground truth is therefore the Python, and the fixture is labelled by
//! **its own regex** (`tools/nonspeech-fixture/generate.py`), never by a reading
//! of the Rust. A fixture labelled from a second implementation of the same
//! prose would agree with this port by construction and prove nothing — the
//! argument `env_conformance.rs` already makes.
//!
//! The cases are every `heard` value from a real session,
//! `session-1788413094.turns.jsonl`, in which the loop spent forty consecutive
//! turns replying to a fan. **Retyping those strings into this file would defeat
//! the point**: a pasted list stays green no matter what the log later holds,
//! and the link back to the thing that actually happened is what makes this a
//! regression test rather than a restatement of the implementation.
//!
//! **A missing fixture FAILS. It does not skip.** Same rule as the other two
//! fixture suites: a conformance test that passes quietly when its ground truth
//! is absent reports green for exactly the state it exists to detect.
//!
//! Regenerate with (this emits `repetition_v1.json` too, from the same single
//! read, so both fixtures pin one digest of one file by construction rather
//! than by anyone remembering to run two scripts against a moving target):
//!
//!   python3 tools/session-fixtures/generate.py \
//!       ~/Documents/NeuralCompose/InteractionLogs/session-1788413094.turns.jsonl \
//!       crates/neuralcompose-hypnagogic/tests/fixtures

use neuralcompose_hypnagogic::loops::is_non_speech;
use serde::Deserialize;

const FIXTURE_PATH: &str = "tests/fixtures/nonspeech_v1.json";
const SCHEMA: &str = "neuralcompose.hypnagogic.nonspeech-guard.v1";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_id: String,
    coverage: Coverage,
    cases: Vec<Case>,
}

/// Asserted, not merely reported. A fixture that quietly lost its non-speech
/// cases — or its speech cases — would still pass every per-case check below
/// while testing nothing, so the shape of the corpus is pinned too.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Coverage {
    turns: usize,
    non_speech: usize,
    speech: usize,
    longest_non_speech_run: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    index: u64,
    heard: String,
    non_speech: bool,
}

fn fixture() -> Fixture {
    let raw = std::fs::read_to_string(FIXTURE_PATH).unwrap_or_else(|e| {
        panic!(
            "{FIXTURE_PATH} is missing or unreadable ({e}). This test does not \
             skip without its ground truth — regenerate it with \
             tools/nonspeech-fixture/generate.py (see the module header)."
        )
    });
    let f: Fixture = serde_json::from_str(&raw).expect("fixture is valid JSON");
    assert_eq!(f.schema_id, SCHEMA, "fixture schema id changed");
    f
}

#[test]
fn every_recorded_utterance_is_classified_as_the_python_reference_classifies_it() {
    let f = fixture();
    let mut disagreements = Vec::new();
    for c in &f.cases {
        if is_non_speech(&c.heard) != c.non_speech {
            disagreements.push(format!(
                "turn {}: converse.py says nonSpeech={}, is_non_speech says {} — {:?}",
                c.index,
                c.non_speech,
                !c.non_speech,
                c.heard
            ));
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} of {} cases disagree with the Python reference:\n{}",
        disagreements.len(),
        f.cases.len(),
        disagreements.join("\n")
    );
}

/// The corpus must keep containing both kinds. A guard is only interesting if
/// it is exercised in both directions, and a fixture of pure noise would let a
/// `fn is_non_speech(_) -> bool { true }` pass the test above.
#[test]
fn the_corpus_holds_both_speech_and_noise() {
    let f = fixture();
    assert_eq!(f.cases.len(), f.coverage.turns, "coverage.turns is stale");
    assert_eq!(
        f.cases.iter().filter(|c| c.non_speech).count(),
        f.coverage.non_speech
    );
    assert_eq!(
        f.cases.iter().filter(|c| !c.non_speech).count(),
        f.coverage.speech
    );
    assert!(
        f.coverage.speech > 0 && f.coverage.non_speech > 0,
        "a one-sided corpus cannot fail a constant implementation"
    );
    // The finding this whole change exists for: an unbroken run of turns spent
    // answering room noise. If a future regeneration ever drops this below a
    // handful, the fixture no longer holds the case that motivated the guard.
    assert!(
        f.coverage.longest_non_speech_run >= 10,
        "the long noise run is gone from the corpus ({}); this fixture no \
         longer covers what it was collected for",
        f.coverage.longest_non_speech_run
    );
}

/// The guard must not eat a real utterance that merely contains an aside, and
/// must not eat one with an unterminated bracket — Python's regex needs a
/// closing delimiter, so the leftover survives there and must survive here.
#[test]
fn parenthetical_asides_and_unterminated_brackets_are_still_speech() {
    assert!(!is_non_speech("I said (loudly) yes"));
    assert!(!is_non_speech("(unterminated"));
    assert!(!is_non_speech("radiotropic biofilms [see chapter 3] switch states"));
    assert!(is_non_speech("[BLANK_AUDIO]"));
    assert!(is_non_speech("(buzzing) (buzzing)"));
    assert!(is_non_speech("   "));
    assert!(is_non_speech(""));
}
