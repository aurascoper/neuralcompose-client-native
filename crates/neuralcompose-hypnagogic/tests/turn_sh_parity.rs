//! Differential against `tools/spoken-loop/turn.sh` — the working Linux loop
//! this port is derived from.
//!
//! `turn.sh` is the only ground truth available for the shell stage: there is no
//! Swift equivalent (macOS uses AVFoundation and a Core ML embedder), so the
//! conformance fixture cannot reach any of it. What `turn.sh` *does* pin is the
//! text-mangling behaviour that has actually been run against this model family,
//! and that is the part most likely to be subtly wrong.
//!
//! `tests/fixtures/strip_reference.json` holds the output of its exact
//! `perl | sed | tr | sed` pipeline over a corpus of model-shaped replies.
//!
//! **The two deliberate differences are listed below and asserted**, not merely
//! tolerated. That is the point: any *new* divergence fails, while the two that
//! exist on purpose are documented at the place a reader would look.

use neuralcompose_hypnagogic::loops::strip_for_speech;
use serde::Deserialize;

#[derive(Deserialize)]
struct Reference {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    input: String,
    turn_sh: String,
}

/// Where this port deliberately differs from `turn.sh`, keyed by input.
///
/// Both are cases where `turn.sh`'s behaviour would be *heard*, which is the
/// only justification that counts here — the reply is spoken aloud, so a
/// mangling difference is an audible difference.
fn deliberate_difference(input: &str) -> Option<(&'static str, &'static str)> {
    match input {
        // turn.sh's perl regex requires a closing tag, so an unterminated
        // <think> passes through untouched — and espeak-ng would then read the
        // literal string "think" aloud. A model that emits an unterminated
        // think block has produced no usable reply, so the right output is
        // nothing at all: the loop stays quiet instead of speaking a tag.
        "<think>unterminated thinking" => Some((
            "",
            "turn.sh would speak the literal tag aloud; an unterminated think \
             block means there is no reply to speak",
        )),
        // turn.sh leaves the double spaces where think blocks were removed.
        // Harmless to a synthesizer but pointless, and collapsing keeps chunk()
        // from emitting whitespace-only fragments.
        "Mixed <think>a</think> middle <think>b</think> end." => Some((
            "Mixed middle end.",
            "internal whitespace is collapsed; turn.sh leaves the gaps the \
             removed blocks left behind",
        )),
        _ => None,
    }
}

fn reference() -> Reference {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/strip_reference.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "turn.sh reference fixture missing at {}: {e}",
            path.display()
        )
    });
    serde_json::from_str(&raw).expect("reference fixture is valid JSON")
}

/// Every case either matches `turn.sh` exactly or is one of the two documented
/// differences. A third difference — however reasonable — fails here, which is
/// what stops the two implementations drifting apart quietly.
#[test]
fn stripping_matches_turn_sh_except_where_documented() {
    let r = reference();
    assert!(
        r.cases.len() >= 10,
        "reference corpus is too small to discriminate"
    );

    let mut undocumented: Vec<String> = Vec::new();
    let mut matched = 0usize;
    let mut differed = 0usize;

    for case in &r.cases {
        let got = strip_for_speech(&case.input);
        match deliberate_difference(&case.input) {
            Some((expected, why)) => {
                assert_eq!(
                    got, expected,
                    "documented difference for {:?} no longer holds ({why})",
                    case.input
                );
                assert_ne!(
                    got, case.turn_sh,
                    "{:?} is listed as a deliberate difference but now agrees \
                     with turn.sh — remove it from the exception list",
                    case.input
                );
                differed += 1;
            }
            None if got == case.turn_sh => matched += 1,
            None => undocumented.push(format!(
                "  {:?}\n    turn.sh: {:?}\n    rust:    {:?}",
                case.input, case.turn_sh, got
            )),
        }
    }

    assert!(
        undocumented.is_empty(),
        "{} undocumented divergence(s) from turn.sh:\n{}",
        undocumented.len(),
        undocumented.join("\n")
    );
    assert_eq!(differed, 2, "the exception list is stale");
    assert!(matched >= 8, "only {matched} cases actually agreed");
}

/// The exception list must not be able to hide a wholesale rewrite: the bulk of
/// the corpus has to agree, or the differential has stopped being one.
#[test]
fn the_exception_list_stays_small() {
    let r = reference();
    let exceptions = r
        .cases
        .iter()
        .filter(|c| deliberate_difference(&c.input).is_some())
        .count();
    assert!(
        exceptions * 4 < r.cases.len(),
        "{exceptions} of {} cases are exceptions — this is no longer a \
         differential against turn.sh",
        r.cases.len()
    );
}

/// The properties the loop depends on, independent of the corpus: nothing
/// spoken may contain a think tag or markdown emphasis, whatever the input.
#[test]
fn no_markup_survives_stripping_in_any_reference_case() {
    for case in reference().cases {
        let got = strip_for_speech(&case.input);
        for forbidden in ["<think>", "</think>", "**"] {
            assert!(
                !got.contains(forbidden),
                "{forbidden:?} survived stripping of {:?} -> {got:?}",
                case.input
            );
        }
        assert_eq!(got, got.trim(), "output is not trimmed: {got:?}");
    }
}
