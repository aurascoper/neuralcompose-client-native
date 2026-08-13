//! Per-mode behaviour, asserted from Rust alone with scripted spies.
//!
//! This automates what verification step 3 of the plan describes as reading the
//! JSON logs of a live run by eye — one generate call per turn for mirror, two
//! for the dialectical profiles, three for Reflective, and a strict
//! listen → generate… → speak ordering. An eyeball check on log output is
//! exactly the kind of verification this project keeps finding does not
//! actually happen, so it is a test.
//!
//! It needs no Mac, no model, no microphone and no fixture: every seam is a
//! spy, and the whole point of the pure-lib split is that this is possible.
//!
//! What it deliberately does NOT cover: the *content* of the prompts beyond the
//! witness being distinguishable, and the inter-turn delay (a lib constant the
//! shell consumes — see `loops.rs`'s known-gap note). A Swift-side per-mode call
//! trace would catch a witness prompt that is structurally right and textually
//! wrong; that is the expensive tier and is not this.

use neuralcompose_hypnagogic::dialectic::{DialecticConfig, DialecticLoop, WITNESS_SYSTEM};
use neuralcompose_hypnagogic::embedding::Embedding;
use neuralcompose_hypnagogic::loops::{MirrorConfig, MirrorLoop};
use neuralcompose_hypnagogic::profile::{ContextProfile, HypnagogicMode};
use neuralcompose_hypnagogic::role::waking_roles;
use neuralcompose_hypnagogic::seams::{
    GenerationParams, Listening, Prosody, ScriptedDraws, SeamResult, SentenceEmbedding, Speaking,
    TextGenerating,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Listen,
    Generate { witness: bool },
    Embed,
    Speak,
}

type Trace = Rc<RefCell<Vec<Event>>>;

struct SpyListener {
    trace: Trace,
    script: Vec<Option<String>>,
    index: usize,
}
impl Listening for SpyListener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        self.trace.borrow_mut().push(Event::Listen);
        let v = self.script.get(self.index).cloned().unwrap_or(None);
        self.index += 1;
        Ok(v)
    }
}

struct SpyGenerator {
    trace: Trace,
    n: usize,
}
impl TextGenerating for SpyGenerator {
    fn generate(
        &mut self,
        system: &str,
        _prompt: &str,
        _params: GenerationParams,
    ) -> SeamResult<String> {
        let witness = system == WITNESS_SYSTEM;
        self.trace.borrow_mut().push(Event::Generate { witness });
        self.n += 1;
        Ok(format!("candidate text number {}", self.n))
    }
}

/// Deterministic distinct vectors, so candidates are neither identical (zero
/// tension) nor incomparable.
struct SpyEmbedder {
    trace: Trace,
    n: u32,
}
impl SentenceEmbedding for SpyEmbedder {
    fn embed(&mut self, _text: &str) -> SeamResult<Embedding> {
        self.trace.borrow_mut().push(Event::Embed);
        self.n += 1;
        let a = (self.n as f32 * 0.7).sin();
        let b = (self.n as f32 * 1.3).cos();
        Ok(Embedding::new(vec![a, b, 0.25], "spy"))
    }
}

struct SpySpeaker {
    trace: Trace,
}
impl Speaking for SpySpeaker {
    fn speak(&mut self, _text: &str, _prosody: Prosody) -> SeamResult<()> {
        self.trace.borrow_mut().push(Event::Speak);
        Ok(())
    }
}

fn heard_script(n: usize) -> Vec<Option<String>> {
    (0..n)
        .map(|i| Some(format!("utterance number {i}")))
        .collect()
}

/// Runs `turns` turns of a mode and returns the event trace.
fn run_mode(mode: HypnagogicMode, turns: usize) -> Vec<Event> {
    let trace: Trace = Rc::new(RefCell::new(Vec::new()));
    let listener = SpyListener {
        trace: Rc::clone(&trace),
        script: heard_script(turns),
        index: 0,
    };
    let generator = SpyGenerator {
        trace: Rc::clone(&trace),
        n: 0,
    };
    let speaker = SpySpeaker {
        trace: Rc::clone(&trace),
    };

    match mode.profile() {
        None => {
            let mut l = MirrorLoop::new(listener, generator, speaker, MirrorConfig::default());
            for _ in 0..turns {
                l.turn().expect("mirror turn");
            }
        }
        Some(profile) => {
            let embedder = SpyEmbedder {
                trace: Rc::clone(&trace),
                n: 0,
            };
            // A mid-range draw: not so extreme that it always picks one basin.
            let draws = ScriptedDraws::new(vec![0.4, 0.6, 0.3, 0.7, 0.5]);
            let mut l = DialecticLoop::new(
                listener,
                generator,
                speaker,
                embedder,
                draws,
                waking_roles().to_vec(),
                profile,
                DialecticConfig::default(),
            );
            for _ in 0..turns {
                l.turn().expect("dialectic turn");
            }
        }
    }
    let out = trace.borrow().clone();
    out
}

/// Splits a trace into per-turn slices at each `Listen`.
fn turns_of(trace: &[Event]) -> Vec<Vec<Event>> {
    let mut turns: Vec<Vec<Event>> = Vec::new();
    for e in trace {
        if *e == Event::Listen {
            turns.push(Vec::new());
        }
        if let Some(last) = turns.last_mut() {
            last.push(e.clone());
        }
    }
    turns
}

fn generate_calls(turn: &[Event]) -> usize {
    turn.iter()
        .filter(|e| matches!(e, Event::Generate { .. }))
        .count()
}

fn witness_calls(turn: &[Event]) -> usize {
    turn.iter()
        .filter(|e| matches!(e, Event::Generate { witness: true }))
        .count()
}

/// The headline claim of the whole feature: the four modes cost different
/// numbers of model calls per turn, and Reflective's third call is the Witness.
#[test]
fn each_mode_makes_its_documented_number_of_generate_calls() {
    let expected = [
        (HypnagogicMode::Mirror, 1usize, 0usize),
        (HypnagogicMode::Focused, 2, 0),
        (HypnagogicMode::Reflective, 3, 1),
        (HypnagogicMode::Contemplative, 2, 0),
    ];
    for (mode, want_generates, want_witness) in expected {
        let trace = run_mode(mode, 3);
        for (i, turn) in turns_of(&trace).iter().enumerate() {
            assert_eq!(
                generate_calls(turn),
                want_generates,
                "{} turn {i}: expected {want_generates} generate call(s), trace {turn:?}",
                mode.id()
            );
            assert_eq!(
                witness_calls(turn),
                want_witness,
                "{} turn {i}: expected {want_witness} witness call(s)",
                mode.id()
            );
        }
    }
}

/// Only Reflective runs the Witness. This is the one difference between the
/// dialectical profiles that is control flow rather than a tuning preset, so it
/// is the one most likely to be lost in a refactor that treats them uniformly.
#[test]
fn the_witness_is_reflective_only_at_runtime_not_just_in_the_profile_table() {
    for mode in HypnagogicMode::ALL {
        let trace = run_mode(mode, 2);
        let total: usize = turns_of(&trace).iter().map(|t| witness_calls(t)).sum();
        if mode == HypnagogicMode::Reflective {
            assert!(total > 0, "reflective never called the witness");
        } else {
            assert_eq!(total, 0, "{} called the witness", mode.id());
        }
    }
}

/// The mic must never be open while anything is being spoken. Asserted as
/// ordering: within a turn every `Listen` precedes every `Speak`, and no
/// `Listen` appears after a `Speak`.
///
/// This is the acoustic-feedback guarantee the Swift gets from strict
/// alternation, and it is a property of the loop rather than of the shell.
#[test]
fn the_microphone_is_never_open_during_playback() {
    for mode in HypnagogicMode::ALL {
        for turn in turns_of(&run_mode(mode, 3)) {
            let first_speak = turn.iter().position(|e| *e == Event::Speak);
            let last_listen = turn.iter().rposition(|e| *e == Event::Listen);
            if let (Some(s), Some(l)) = (first_speak, last_listen) {
                assert!(
                    l < s,
                    "{}: a listen followed a speak within one turn: {turn:?}",
                    mode.id()
                );
            }
            assert_eq!(
                turn.iter().filter(|e| **e == Event::Listen).count(),
                1,
                "{}: a turn listened more than once: {turn:?}",
                mode.id()
            );
        }
    }
}

/// Ordering within a turn: listen, then every generate, then speak. A speak
/// interleaved between the two poles' generate calls would mean the loop voiced
/// a candidate before the competition had resolved.
#[test]
fn nothing_is_spoken_before_the_competition_has_resolved() {
    for mode in HypnagogicMode::ALL {
        for turn in turns_of(&run_mode(mode, 3)) {
            let last_generate = turn
                .iter()
                .rposition(|e| matches!(e, Event::Generate { .. }));
            let first_speak = turn.iter().position(|e| *e == Event::Speak);
            if let (Some(g), Some(s)) = (last_generate, first_speak) {
                assert!(
                    g < s,
                    "{}: spoke before the last generate returned: {turn:?}",
                    mode.id()
                );
            }
        }
    }
}

/// Mirror does no semantic scoring at all, so it must never embed. If it starts
/// embedding, it has quietly become a dialectic with one pole — and would pay
/// for an embedder the mode does not need.
#[test]
fn mirror_never_embeds_and_the_dialectical_modes_always_do() {
    let mirror = run_mode(HypnagogicMode::Mirror, 3);
    assert_eq!(mirror.iter().filter(|e| **e == Event::Embed).count(), 0);

    for mode in [
        HypnagogicMode::Focused,
        HypnagogicMode::Reflective,
        HypnagogicMode::Contemplative,
    ] {
        let trace = run_mode(mode, 3);
        assert!(
            trace.contains(&Event::Embed),
            "{} never embedded",
            mode.id()
        );
    }
}

/// Reflective embeds the witness finding too, to measure how far it sits from
/// what was voiced. So it embeds strictly more than Focused over the same input
/// — a cheap proxy for "the witness result is actually being used" rather than
/// generated and dropped.
#[test]
fn reflective_embeds_the_witness_finding() {
    let focused = run_mode(HypnagogicMode::Focused, 3);
    let reflective = run_mode(HypnagogicMode::Reflective, 3);
    let count = |t: &[Event]| t.iter().filter(|e| **e == Event::Embed).count();
    assert!(
        count(&reflective) > count(&focused),
        "reflective embedded {} vs focused {} — the witness finding is not being measured",
        count(&reflective),
        count(&focused)
    );
}

/// Silence must not reach the model. A turn where nothing was heard speaks a
/// cue and makes no generate call at all — otherwise every silent turn in a
/// long session bills a cloud round trip per pole.
#[test]
fn a_silent_turn_costs_no_model_call_in_any_mode() {
    for mode in HypnagogicMode::ALL {
        let trace: Trace = Rc::new(RefCell::new(Vec::new()));
        let listener = SpyListener {
            trace: Rc::clone(&trace),
            script: vec![None, None],
            index: 0,
        };
        let generator = SpyGenerator {
            trace: Rc::clone(&trace),
            n: 0,
        };
        let speaker = SpySpeaker {
            trace: Rc::clone(&trace),
        };
        match mode.profile() {
            None => {
                let mut l = MirrorLoop::new(listener, generator, speaker, MirrorConfig::default());
                for _ in 0..2 {
                    l.turn().unwrap();
                }
            }
            Some(profile) => {
                let embedder = SpyEmbedder {
                    trace: Rc::clone(&trace),
                    n: 0,
                };
                let mut l = DialecticLoop::new(
                    listener,
                    generator,
                    speaker,
                    embedder,
                    ScriptedDraws::new(vec![0.5]),
                    waking_roles().to_vec(),
                    profile,
                    DialecticConfig::default(),
                );
                for _ in 0..2 {
                    l.turn().unwrap();
                }
            }
        }
        let t = trace.borrow();
        assert_eq!(
            t.iter()
                .filter(|e| matches!(e, Event::Generate { .. }))
                .count(),
            0,
            "{} called the model on a silent turn: {t:?}",
            mode.id()
        );
    }
}

/// The profiles are presets over one engine, so their *shape* must be identical
/// — same call pattern, differing only in tuning. If Focused and Contemplative
/// ever diverge structurally, one of them has grown its own control flow.
#[test]
fn focused_and_contemplative_are_structurally_identical() {
    let strip = |m| {
        turns_of(&run_mode(m, 3))
            .iter()
            .map(|t| {
                (
                    generate_calls(t),
                    t.iter().filter(|e| **e == Event::Embed).count(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        strip(HypnagogicMode::Focused),
        strip(HypnagogicMode::Contemplative),
        "the two non-witness dialectical profiles differ in call structure"
    );
    // …and both differ from Reflective, which has the extra branch.
    assert_ne!(
        strip(HypnagogicMode::Focused),
        strip(HypnagogicMode::Reflective)
    );
}

/// A witness failure must not break the turn, and must not be silent either —
/// a persistently-failing Reflective run would otherwise be indistinguishable
/// from a Focused one.
#[test]
fn a_failing_witness_degrades_visibly_rather_than_silently() {
    struct FailingWitness {
        n: usize,
    }
    impl TextGenerating for FailingWitness {
        fn generate(
            &mut self,
            system: &str,
            _prompt: &str,
            _p: GenerationParams,
        ) -> SeamResult<String> {
            if system == WITNESS_SYSTEM {
                return Err(neuralcompose_hypnagogic::seams::SeamError::Failed(
                    "witness backend down".into(),
                ));
            }
            self.n += 1;
            Ok(format!("candidate {}", self.n))
        }
    }
    let trace: Trace = Rc::new(RefCell::new(Vec::new()));
    let mut l = DialecticLoop::new(
        SpyListener {
            trace: Rc::clone(&trace),
            script: heard_script(1),
            index: 0,
        },
        FailingWitness { n: 0 },
        SpySpeaker {
            trace: Rc::clone(&trace),
        },
        SpyEmbedder {
            trace: Rc::clone(&trace),
            n: 0,
        },
        ScriptedDraws::new(vec![0.5]),
        waking_roles().to_vec(),
        ContextProfile::Reflective,
        DialecticConfig::default(),
    );
    let turn = l
        .turn()
        .expect("a witness failure must not fail the turn")
        .unwrap();
    assert!(turn.witness_attempted, "the attempt must still be recorded");
    assert!(turn.witness_finding.is_none());
    assert!(
        turn.witness_error.is_some(),
        "a failing witness left no trace — Reflective now looks like Focused"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// The following cover mutants that survived the first mutation run of
// `dialectic.rs`. Every one of them is behaviour the module header claims and
// nothing above observed: the trajectory feeding the centroids, tension
// carrying across turns, the prosody blend, and the per-profile silence bound.
// ─────────────────────────────────────────────────────────────────────────

/// An embedder returning fixed vectors, keyed on the TEXT rather than on call
/// order. Order-keying desynchronises under Reflective, which embeds a fourth
/// string per turn (the witness finding) — the first draft of these tests did
/// that and produced a spurious failure two turns in.
struct FixedEmbedder {
    vectors: Vec<Vec<f32>>,
    candidate_index: usize,
}
impl SentenceEmbedding for FixedEmbedder {
    fn embed(&mut self, text: &str) -> SeamResult<Embedding> {
        let v = if text.starts_with("heard") {
            self.vectors[0].clone()
        } else if text.starts_with("witness") {
            // Off both candidate axes, so it perturbs nothing being asserted.
            vec![0.0, 0.0, 1.0]
        } else {
            let i = self.candidate_index % 2;
            self.candidate_index += 1;
            self.vectors[1 + i].clone()
        };
        Ok(Embedding::new(v, "fixed"))
    }
}

struct QuietSpeaker;
impl Speaking for QuietSpeaker {
    fn speak(&mut self, _t: &str, _p: Prosody) -> SeamResult<()> {
        Ok(())
    }
}
struct PlainGenerator {
    n: usize,
}
impl TextGenerating for PlainGenerator {
    fn generate(&mut self, system: &str, _p: &str, _g: GenerationParams) -> SeamResult<String> {
        if system == WITNESS_SYSTEM {
            return Ok("witness observation".to_string());
        }
        self.n += 1;
        Ok(format!("candidate {}", self.n))
    }
}
struct PlainListener {
    n: usize,
}
impl Listening for PlainListener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        self.n += 1;
        Ok(Some(format!("heard {}", self.n)))
    }
}

fn loop_with_vectors(
    profile: ContextProfile,
    vectors: Vec<Vec<f32>>,
) -> DialecticLoop<PlainListener, PlainGenerator, QuietSpeaker, FixedEmbedder, ScriptedDraws> {
    DialecticLoop::new(
        PlainListener { n: 0 },
        PlainGenerator { n: 0 },
        QuietSpeaker,
        FixedEmbedder {
            vectors,
            candidate_index: 0,
        },
        ScriptedDraws::new(vec![0.5]),
        waking_roles().to_vec(),
        profile,
        DialecticConfig::default(),
    )
}

/// Cycle: heard, candidate A, candidate B — three embeddings per turn.
fn spread_vectors() -> Vec<Vec<f32>> {
    vec![
        vec![0.0, 1.0, 0.0],
        vec![1.0, 0.0, 0.0],
        vec![0.3, 0.2, 0.9],
    ]
}

/// The trajectory must actually accumulate. Turn 0 has no history, so resonance
/// and novelty are the neutral 0.5; by turn 2 both centroids exist and the
/// scores must have moved off neutral.
///
/// Without this, a loop that never recorded anything would score every turn as
/// an early turn forever — resonance and novelty pinned at 0.5 — and the
/// profiles would differ only in their silence thresholds. Two mutants
/// (`record_heard` and `record_reply` removed) survived until this existed.
#[test]
fn the_trajectory_accumulates_and_moves_the_scores_off_neutral() {
    let mut l = loop_with_vectors(ContextProfile::Focused, spread_vectors());

    let first = l.turn().unwrap().unwrap();
    for s in &first.scored {
        assert_eq!(s.energy.resonance, 0.5, "turn 0 must have no history");
        assert_eq!(s.energy.novelty, 0.5, "turn 0 must have no replies");
    }

    l.turn().unwrap();
    let third = l.turn().unwrap().unwrap();
    assert!(
        third.scored.iter().any(|s| s.energy.resonance != 0.5),
        "resonance never left neutral — the heard trajectory is not accumulating"
    );
    assert!(
        third.scored.iter().any(|s| s.energy.novelty != 0.5),
        "novelty never left neutral — the reply trajectory is not accumulating"
    );
}

/// Standing tension is the loop's headline property: contradiction persists
/// across turns rather than being resolved every cycle. So after each turn the
/// loop's standing tension must equal that turn's measured tension.
#[test]
fn standing_tension_carries_across_turns() {
    let mut l = loop_with_vectors(ContextProfile::Reflective, spread_vectors());
    for _ in 0..3 {
        let t = l.turn().unwrap().unwrap();
        assert_eq!(
            l.standing_tension(),
            t.resolution.tension,
            "standing tension did not carry the turn's measured tension"
        );
    }
    assert!(
        l.standing_tension() > 0.0,
        "tension stayed at zero across the whole run"
    );
}

/// A spoken turn is voiced with the role voices blended by the competition's
/// own probabilities — that blend is what makes tension audible. The waking
/// roles' voices are nothing like the base hypnagogic prosody, so a loop that
/// skipped blending would speak in the base voice and be caught here.
#[test]
fn a_spoken_turn_is_voiced_with_the_blended_role_prosody() {
    let mut l = loop_with_vectors(ContextProfile::Focused, spread_vectors());
    let turn = l.turn().unwrap().unwrap();
    assert!(turn.spoken.is_some(), "expected a spoken turn");
    assert_ne!(
        turn.prosody,
        DialecticConfig::default().prosody,
        "the turn was voiced in the base prosody — the role blend was skipped"
    );
    // The blend of two waking voices must land between them, not on either.
    let rate = turn.prosody.rate.expect("blended rate");
    assert!(
        rate > Prosody::WAKING_COHERENT.rate.unwrap() - 0.01
            && rate < Prosody::WAKING_DIVERGENT.rate.unwrap() + 0.01,
        "blended rate {rate} is outside the two role voices"
    );
}

/// Vectors that force a stalemate: `heard` orthogonal to both candidates, and
/// the candidates diametrically opposed. Both score identically (margin 0) at
/// maximum tension (1.0), which is above every profile's `high_tension`.
fn stalemate_vectors() -> Vec<Vec<f32>> {
    vec![
        vec![0.0, 1.0, 0.0],  // heard
        vec![1.0, 0.0, 0.0],  // candidate A
        vec![-1.0, 0.0, 0.0], // candidate B, opposed
    ]
}

/// Each profile tolerates its own length of silence run before breaking it with
/// a cue — Focused 2, Reflective 3, Contemplative 6. A loop that hardcoded the
/// bound would still fall silent correctly and still eventually speak, so only
/// counting the turns distinguishes it.
#[test]
fn the_silence_run_bound_comes_from_the_profile() {
    for (profile, expected) in [
        (ContextProfile::Focused, 2u32),
        (ContextProfile::Reflective, 3),
        (ContextProfile::Contemplative, 6),
    ] {
        let mut l = loop_with_vectors(profile, stalemate_vectors());
        let mut spoke_on = None;
        for turn_number in 1..=8u32 {
            let t = l.turn().unwrap().unwrap();
            assert!(
                matches!(
                    t.resolution.outcome,
                    neuralcompose_hypnagogic::dynamics::DialecticalOutcome::Silent
                ),
                "{}: expected a stalemate, got {:?}",
                profile.id(),
                t.resolution.outcome
            );
            if t.spoken.is_some() {
                spoke_on = Some(turn_number);
                break;
            }
        }
        assert_eq!(
            spoke_on,
            Some(expected),
            "{} broke its silence run on the wrong turn",
            profile.id()
        );
    }
}
