//! The mirror loop — one turn at a time.
//!
//! Port of `Sources/BCICore/Composition/HypnagogicDialogueLoop.swift` (184
//! lines). The dialectic loop lands beside it next.
//!
//! ## Why this is a step function and not a `run()`
//!
//! The Swift is an `actor` holding a `Task`, looping until cancelled and
//! `Task.sleep`-ing between turns. That shape needs a clock and a scheduler,
//! and this crate's lib has neither by design. So the port inverts it: one
//! [`MirrorLoop::turn`] call performs exactly one listen→reply→speak cycle and
//! returns, and the **binary** owns the `for` loop, the inter-turn sleep
//! (`ContextProfile::inter_turn_delay`) and the stop condition.
//!
//! Nothing is lost. The Swift's cancellation machinery exists because a GUI
//! starts and stops the loop from a button; a headless run bounded by `--turns`
//! has no such need, and a `stop()` that must interrupt an in-flight `listen`
//! is a property of the shell's listener, not of this logic.
//!
//! ## What is preserved exactly
//!
//! - **Strict listen→speak alternation.** The mic is closed before anything is
//!   spoken, so there is no acoustic feedback path and no mute is needed.
//! - **A silent turn costs no model call.** It speaks a rotating induction cue
//!   instead — which is also why the cue index advances only on silence.
//! - **Replies are spoken as micro-phrases**, split on sentence and clause
//!   punctuation, each with its own prosody.
//! - **A failed turn does not abort the loop.** The Swift swallows a
//!   listen/generate/speak error and falls through to the delay rather than
//!   hot-spinning; here the error is *returned* so the caller can log it and
//!   decide, which is strictly more information and no less robust.
//!
//! ## Honest scope, carried from the Swift
//!
//! MANUAL trigger only — this is not wired to any sleep-stage or N1 detector,
//! and firing it on an unvalidated N1 signal is out of scope. The generator may
//! be a cloud model; speech-to-text stays on-device, so only *text* ever leaves
//! the machine. Engineering scaffolding, **not** a validated intervention: any
//! efficacy claim requires the D8 pre-registration.

use crate::seams::{
    GenerationParams, Listening, Prosody, SeamError, SeamResult, Speaking, TextGenerating,
};

/// Rotating soft cues spoken on a silent turn, when no model is called.
pub const DEFAULT_SILENCE_CUES: [&str; 4] = [
    "Drifting, softly, deeper.",
    "Let the shapes dissolve and float away.",
    "Sinking, warm and slow.",
    "The quiet carries you down.",
];

#[derive(Clone, Debug, PartialEq)]
pub struct MirrorConfig {
    pub max_tokens: u32,
    pub temperature: f64,
    pub prosody: Prosody,
    pub silence_cues: Vec<String>,
    /// System instruction for the reply. Constrained to soft, short replies —
    /// the Swift carries this in the generator's own system prompt rather than
    /// the loop, but the Linux generator is a bare HTTP endpoint with no such
    /// prompt of its own, so it lives here.
    pub system: String,
}

pub const MIRROR_SYSTEM: &str = "You are a calm, passive presence beside someone drifting toward \
     sleep. Reflect back what they say gently — stay close to their words and \
     their feeling, add nothing new or alarming. Your reply is spoken aloud: \
     plain prose only, no markdown, no lists, no stage directions. At most two \
     short, soft sentences. Output only the reply.";

impl Default for MirrorConfig {
    fn default() -> Self {
        Self {
            max_tokens: 60,
            temperature: 0.6,
            prosody: Prosody::HYPNAGOGIC,
            silence_cues: DEFAULT_SILENCE_CUES.iter().map(|s| s.to_string()).collect(),
            system: MIRROR_SYSTEM.to_string(),
        }
    }
}

/// What one turn did. Returned rather than logged so the shell owns every effect.
#[derive(Clone, Debug, PartialEq)]
pub struct MirrorTurn {
    pub index: u64,
    /// The transcript, or `None` when the turn was silent.
    pub heard: Option<String>,
    /// What was spoken — a model reply, or an induction cue on a silent turn.
    pub spoken: String,
    /// True when `spoken` is a cue and no model was called.
    pub was_cue: bool,
    /// The micro-phrases actually handed to the speaker.
    pub chunks: Vec<String>,
}

pub struct MirrorLoop<L, G, S> {
    listener: L,
    generator: G,
    speaker: S,
    config: MirrorConfig,
    silence_index: usize,
    turn_index: u64,
}

impl<L: Listening, G: TextGenerating, S: Speaking> MirrorLoop<L, G, S> {
    pub fn new(listener: L, generator: G, speaker: S, config: MirrorConfig) -> Self {
        Self {
            listener,
            generator,
            speaker,
            config,
            silence_index: 0,
            turn_index: 0,
        }
    }

    pub fn turns_taken(&self) -> u64 {
        self.turn_index
    }

    /// One listen → reply → speak cycle.
    ///
    /// The turn index advances even on error, so a run's turn numbering matches
    /// the number of attempts rather than the number of successes — otherwise a
    /// log would silently renumber around failures.
    pub fn turn(&mut self) -> SeamResult<MirrorTurn> {
        let index = self.turn_index;
        self.turn_index += 1;

        let heard = self.listener.listen()?;
        let transcript = heard
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty());

        let (spoken, was_cue) = match &transcript {
            Some(t) => {
                let params = GenerationParams {
                    temperature: self.config.temperature,
                    max_tokens: self.config.max_tokens,
                };
                (
                    self.generator.generate(&self.config.system, t, params)?,
                    false,
                )
            }
            // Silent turn — no model call.
            None => (self.next_silence_cue(), true),
        };

        let chunks = chunk(&spoken);
        for c in &chunks {
            self.speaker.speak(c, self.config.prosody)?;
        }

        Ok(MirrorTurn {
            index,
            heard: transcript,
            spoken,
            was_cue,
            chunks,
        })
    }

    /// Advances only on a silent turn, so the cues rotate across silences rather
    /// than across all turns — a long spoken stretch does not burn through them.
    fn next_silence_cue(&mut self) -> String {
        if self.config.silence_cues.is_empty() {
            return String::new();
        }
        let cue =
            self.config.silence_cues[self.silence_index % self.config.silence_cues.len()].clone();
        self.silence_index += 1;
        cue
    }
}

/// Splits text into speakable micro-phrases on `.`, `,`, `;`, `…`, `!`, `?`.
/// Falls back to the whole trimmed text when there is no terminal punctuation,
/// and to nothing at all when there is no text.
pub fn chunk(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if matches!(ch, '.' | ',' | ';' | '…' | '!' | '?') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
            }
            current.clear();
        }
    }
    let tail = current.trim();
    if !tail.is_empty() {
        chunks.push(tail.to_string());
    }
    if chunks.is_empty() {
        let whole = text.trim();
        if whole.is_empty() {
            return Vec::new();
        }
        return vec![whole.to_string()];
    }
    chunks
}

/// Strips the things a text model emits that must never be spoken aloud:
/// `<think>` blocks, markdown emphasis, headings, blockquotes and list bullets.
///
/// Ported from the shell pipeline `tools/spoken-loop/turn.sh` already worked
/// out against this exact model family, rather than re-derived. It lives here,
/// pure and tested, instead of in the generator shell, because it is the same
/// cleanup for every backend.
pub fn strip_for_speech(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // Drop <think>…</think> spans, including an unterminated trailing one.
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);

    let cleaned: Vec<String> = out
        .lines()
        .map(|line| {
            let mut l = line.trim_start();
            l = l.trim_start_matches('#').trim_start();
            l = l.trim_start_matches('>').trim_start();
            if let Some(stripped) = l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")) {
                l = stripped;
            }
            l.replace(['*', '_'], "")
        })
        .collect();
    cleaned
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A listener that is never expected to be reached — for a mode that does not
/// listen. Returning an error rather than `Ok(None)` keeps "no listener wired"
/// distinguishable from "the user said nothing".
pub struct UnreachableListener;

impl Listening for UnreachableListener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        Err(SeamError::Unavailable("no listener is wired".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Spy {
        spoken: Vec<(String, Prosody)>,
        prompts: Vec<String>,
    }

    struct ScriptedListener {
        script: Vec<Option<String>>,
        index: usize,
    }
    impl Listening for ScriptedListener {
        fn listen(&mut self) -> SeamResult<Option<String>> {
            let v = self.script.get(self.index).cloned().unwrap_or(None);
            self.index += 1;
            Ok(v)
        }
    }

    struct ScriptedGenerator {
        reply: String,
        spy: Rc<RefCell<Spy>>,
        fail: bool,
    }
    impl TextGenerating for ScriptedGenerator {
        fn generate(
            &mut self,
            _system: &str,
            prompt: &str,
            _params: GenerationParams,
        ) -> SeamResult<String> {
            self.spy.borrow_mut().prompts.push(prompt.to_string());
            if self.fail {
                return Err(SeamError::Failed("model down".into()));
            }
            Ok(self.reply.clone())
        }
    }

    struct SpySpeaker {
        spy: Rc<RefCell<Spy>>,
    }
    impl Speaking for SpySpeaker {
        fn speak(&mut self, text: &str, prosody: Prosody) -> SeamResult<()> {
            self.spy
                .borrow_mut()
                .spoken
                .push((text.to_string(), prosody));
            Ok(())
        }
    }

    fn loop_with(
        script: Vec<Option<String>>,
        reply: &str,
        fail: bool,
    ) -> (
        MirrorLoop<ScriptedListener, ScriptedGenerator, SpySpeaker>,
        Rc<RefCell<Spy>>,
    ) {
        let spy = Rc::new(RefCell::new(Spy::default()));
        let l = MirrorLoop::new(
            ScriptedListener { script, index: 0 },
            ScriptedGenerator {
                reply: reply.to_string(),
                spy: Rc::clone(&spy),
                fail,
            },
            SpySpeaker {
                spy: Rc::clone(&spy),
            },
            MirrorConfig::default(),
        );
        (l, spy)
    }

    #[test]
    fn a_heard_turn_calls_the_model_and_speaks_the_reply() {
        let (mut l, spy) = loop_with(vec![Some("i can't sleep".into())], "Rest now.", false);
        let t = l.turn().unwrap();
        assert_eq!(t.heard.as_deref(), Some("i can't sleep"));
        assert_eq!(t.spoken, "Rest now.");
        assert!(!t.was_cue);
        assert_eq!(spy.borrow().prompts, vec!["i can't sleep"]);
        assert_eq!(spy.borrow().spoken.len(), 1);
    }

    /// A silent turn must not reach the model at all — that is the whole point
    /// of the cue path, and a regression here would bill a cloud call per
    /// silence.
    #[test]
    fn a_silent_turn_speaks_a_cue_without_calling_the_model() {
        let (mut l, spy) = loop_with(vec![None], "unused", false);
        let t = l.turn().unwrap();
        assert_eq!(t.heard, None);
        assert!(t.was_cue);
        assert_eq!(t.spoken, DEFAULT_SILENCE_CUES[0]);
        assert!(spy.borrow().prompts.is_empty(), "the model was called");
    }

    /// Whitespace-only transcripts are silence, not content.
    #[test]
    fn blank_transcripts_count_as_silence() {
        let (mut l, spy) = loop_with(vec![Some("   \n ".into())], "unused", false);
        let t = l.turn().unwrap();
        assert!(t.was_cue);
        assert!(spy.borrow().prompts.is_empty());
    }

    /// Cues advance on silences only, so a talkative stretch does not consume
    /// them and every silence sounds different from the last.
    #[test]
    fn cues_rotate_across_silences_not_across_turns() {
        let (mut l, _) = loop_with(
            vec![None, Some("hello".into()), None, None, None, None],
            "reply",
            false,
        );
        let spoken: Vec<String> = (0..6).map(|_| l.turn().unwrap().spoken).collect();
        assert_eq!(spoken[0], DEFAULT_SILENCE_CUES[0]);
        assert_eq!(spoken[1], "reply");
        assert_eq!(spoken[2], DEFAULT_SILENCE_CUES[1]);
        assert_eq!(spoken[3], DEFAULT_SILENCE_CUES[2]);
        assert_eq!(spoken[4], DEFAULT_SILENCE_CUES[3]);
        assert_eq!(spoken[5], DEFAULT_SILENCE_CUES[0], "cues wrap");
    }

    /// Turn numbering counts attempts, not successes — otherwise a log would
    /// silently renumber around a failure and two runs would not line up.
    #[test]
    fn the_turn_index_advances_even_when_the_turn_fails() {
        let (mut l, _) = loop_with(vec![Some("a".into()), Some("b".into())], "r", true);
        assert!(l.turn().is_err());
        assert_eq!(l.turns_taken(), 1);
        assert!(l.turn().is_err());
        assert_eq!(l.turns_taken(), 2);
    }

    #[test]
    fn replies_are_spoken_as_micro_phrases() {
        let (mut l, spy) = loop_with(vec![Some("x".into())], "Rest now. Let go, softly.", false);
        let t = l.turn().unwrap();
        assert_eq!(t.chunks, vec!["Rest now.", "Let go,", "softly."]);
        assert_eq!(spy.borrow().spoken.len(), 3);
        assert!(spy
            .borrow()
            .spoken
            .iter()
            .all(|(_, p)| *p == Prosody::HYPNAGOGIC));
    }

    #[test]
    fn chunking_handles_no_punctuation_and_empty_text() {
        assert_eq!(chunk("just a phrase"), vec!["just a phrase"]);
        assert_eq!(chunk("   "), Vec::<String>::new());
        assert_eq!(chunk(""), Vec::<String>::new());
        assert_eq!(chunk("Sink… deeper?"), vec!["Sink…", "deeper?"]);
    }

    /// A punctuation RUN emits lone-punctuation chunks, because every
    /// punctuation character closes the current chunk and only *empty* results
    /// are dropped — `"."` is not empty.
    ///
    /// This is faithful to `HypnagogicDialogueLoop.chunk` and is pinned rather
    /// than fixed. It is a real artifact: each stray `"."` becomes its own
    /// `speak()` call, which on the sleep voices adds a 0.4 s
    /// `pre_utterance_delay` apiece and, in the shell, a subprocess spawn. It is
    /// cosmetic rather than incorrect, so "improving" it here would put the port
    /// out of step with the Swift for no correctness gain — the kind of drift
    /// that makes a port unverifiable. Fix it in the Swift first if it matters.
    #[test]
    fn punctuation_runs_emit_lone_punctuation_chunks_as_the_swift_does() {
        assert_eq!(
            chunk("Wait... really?!"),
            vec!["Wait.", ".", ".", "really?", "!"]
        );
    }

    #[test]
    fn think_blocks_and_markdown_never_reach_the_speaker() {
        assert_eq!(
            strip_for_speech("<think>weighing it up</think>Rest now."),
            "Rest now."
        );
        // An unterminated think block would otherwise be read aloud in full.
        assert_eq!(strip_for_speech("ok <think>never closed"), "ok");
        assert_eq!(strip_for_speech("**bold** and _soft_"), "bold and soft");
        assert_eq!(
            strip_for_speech("# Heading\n> quoted\n- bullet"),
            "Heading quoted bullet"
        );
        assert_eq!(strip_for_speech("  spaced   out  "), "spaced out");
    }

    /// "No listener wired" must not masquerade as "the user said nothing",
    /// which would silently turn every turn into a cue.
    #[test]
    fn the_unreachable_listener_errors_rather_than_reporting_silence() {
        let mut l = UnreachableListener;
        assert!(matches!(l.listen(), Err(SeamError::Unavailable(_))));
    }
}
