//! The anchor, and what it does to the poles' prompts.
//!
//! Separate from `mode_behaviour.rs` because that file's header states it
//! deliberately does not cover prompt *content*. This one does nothing else, so
//! the two stay honest about their own scope rather than one quietly growing
//! past its stated boundary.
//!
//! The embedder here is keyed on the text — unlike `mode_behaviour.rs`'s spy,
//! which returns vectors by call count — because drift is a property of what
//! was said and cannot be exercised by an embedder that ignores it.

use neuralcompose_hypnagogic::dialectic::{DialecticConfig, DialecticLoop};
use neuralcompose_hypnagogic::embedding::Embedding;
use neuralcompose_hypnagogic::profile::ContextProfile;
use neuralcompose_hypnagogic::role::waking_roles;
use neuralcompose_hypnagogic::seams::{
    GenerationParams, Listening, Prosody, ScriptedDraws, SeamResult, SentenceEmbedding, Speaking,
    TextGenerating,
};
use std::cell::RefCell;
use std::rc::Rc;

const SEED: &str = "what do you know about radiotropic biofilms";
const ON_TOPIC: &str = "how do radiotropic biofilms change state";
const OFF_TOPIC: &str = "the machine whirring is a mechanical process";

struct Listener {
    script: Vec<String>,
    index: usize,
}
impl Listening for Listener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        let v = self.script.get(self.index).cloned();
        self.index += 1;
        Ok(v)
    }
}

/// Records every non-witness prompt, so the test can read what the poles were
/// actually shown.
struct PromptSpy {
    prompts: Rc<RefCell<Vec<String>>>,
    n: usize,
}
impl TextGenerating for PromptSpy {
    fn generate(
        &mut self,
        _system: &str,
        prompt: &str,
        _params: GenerationParams,
    ) -> SeamResult<String> {
        self.prompts.borrow_mut().push(prompt.to_string());
        self.n += 1;
        // Distinct every time: this file tests framing, and identical replies
        // would drag the repetition guard into it.
        Ok(format!("distinct reply {} about {}", self.n, self.n * 7))
    }
}

/// Two clusters, far apart, so drift is a step function the test controls.
/// Anything containing "biofilm" is on-topic; everything else is not.
struct ClusterEmbedder;
impl SentenceEmbedding for ClusterEmbedder {
    fn embed(&mut self, text: &str) -> SeamResult<Embedding> {
        let v = if text.to_lowercase().contains("biofilm") {
            vec![1.0, 0.0, 0.05]
        } else {
            vec![0.0, 1.0, 0.05]
        };
        Ok(Embedding::new(v, "cluster"))
    }
}

struct Mute;
impl Speaking for Mute {
    fn speak(&mut self, _text: &str, _prosody: Prosody) -> SeamResult<()> {
        Ok(())
    }
}

fn run(script: &[&str], drift_ceiling: f32) -> Vec<String> {
    let prompts = Rc::new(RefCell::new(Vec::new()));
    let config = DialecticConfig {
        drift_ceiling,
        // Off: this file is about framing. The repetition guard has its own
        // suite, and leaving it armed here would couple two independent checks.
        repetition_floor: 0.0,
        ..DialecticConfig::default()
    };
    let mut loop_ = DialecticLoop::new(
        Listener {
            script: script.iter().map(|s| s.to_string()).collect(),
            index: 0,
        },
        PromptSpy {
            prompts: Rc::clone(&prompts),
            n: 0,
        },
        Mute,
        ClusterEmbedder,
        ScriptedDraws::new(vec![0.5; 32]),
        waking_roles().to_vec(),
        ContextProfile::Focused,
        config,
    );
    for _ in 0..script.len() {
        loop_.turn().expect("turn");
    }
    let out = prompts.borrow().clone();
    out
}

#[test]
fn an_on_topic_turn_is_not_framed_and_a_drifted_one_is() {
    // Turn 0 sets the anchor. Turn 1 stays on topic. Turn 2 leaves it.
    let prompts = run(&[SEED, ON_TOPIC, OFF_TOPIC], 0.45);
    let framed: Vec<bool> = prompts
        .iter()
        .map(|p| p.contains("this exchange began with"))
        .collect();

    // Two poles per turn, so prompts come in pairs.
    assert!(
        framed[0..2].iter().all(|f| !f),
        "the anchoring turn framed itself"
    );
    assert!(
        framed[2..4].iter().all(|f| !f),
        "an on-topic turn was framed with the seed"
    );
    assert!(
        framed[4..6].iter().all(|f| *f),
        "a drifted turn was NOT framed with the seed"
    );
    assert!(
        prompts[4].contains(SEED),
        "the framing must carry the actual opening utterance, not a placeholder"
    );
}

/// The SHIPPED default must actually work.
///
/// Every other test here passes an explicit ceiling, so mutating
/// `DialecticConfig::default()`'s `drift_ceiling` to an unreachable value
/// survived the whole suite — the configuration users get was untested while
/// the mechanism it drives was covered. Caught by mutation, fixed here.
#[test]
fn the_shipped_default_ceiling_actually_frames_a_drifted_turn() {
    let default_ceiling = DialecticConfig::default().drift_ceiling;
    let prompts = run(&[SEED, OFF_TOPIC], default_ceiling);
    assert!(
        prompts[2..4]
            .iter()
            .all(|p| p.contains("this exchange began with")),
        "the default ceiling of {default_ceiling} never fires — the shipped \
         configuration does nothing"
    );
}

/// A ceiling of 0.0 disables the whole mechanism. Without this, a "framing is
/// off" configuration and a "framing never triggers" bug look identical.
#[test]
fn a_zero_ceiling_disables_framing_entirely() {
    let prompts = run(&[SEED, OFF_TOPIC, OFF_TOPIC], 0.0);
    assert!(
        prompts
            .iter()
            .all(|p| !p.contains("this exchange began with")),
        "framing happened with the ceiling disabled"
    );
}

/// The transcript itself is never rewritten — only the prompt is. The drifted
/// text is the evidence, and a loop that laundered it into the log would
/// destroy the only record of what went wrong.
#[test]
fn the_heard_transcript_is_never_rewritten_by_framing() {
    let prompts = Rc::new(RefCell::new(Vec::new()));
    let config = DialecticConfig {
        drift_ceiling: 0.45,
        repetition_floor: 0.0,
        ..DialecticConfig::default()
    };
    let mut loop_ = DialecticLoop::new(
        Listener {
            script: vec![SEED.into(), OFF_TOPIC.into()],
            index: 0,
        },
        PromptSpy {
            prompts: Rc::clone(&prompts),
            n: 0,
        },
        Mute,
        ClusterEmbedder,
        ScriptedDraws::new(vec![0.5; 32]),
        waking_roles().to_vec(),
        ContextProfile::Focused,
        config,
    );
    loop_.turn().expect("turn").expect("a turn happened");
    let second = loop_.turn().expect("turn").expect("a turn happened");
    assert_eq!(
        second.heard, OFF_TOPIC,
        "heard was rewritten; the framing must not touch the transcript"
    );
    assert!(second.reanchored, "the drifted turn should be flagged");
    assert!(
        second.topic_drift.is_some_and(|d| d > 0.45),
        "topic_drift should carry the measured distance, got {:?}",
        second.topic_drift
    );
}
