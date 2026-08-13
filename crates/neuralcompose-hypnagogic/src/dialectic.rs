//! The dialectic loop — a persistent dynamical competition, one turn at a time.
//!
//! Port of `Sources/BCICore/Composition/HypnagogicDialecticLoop.swift` (460
//! lines). Where [`crate::loops::MirrorLoop`] collapses each turn into one
//! reply, this runs every [`DialecticalRole`] against the same utterance, scores
//! the candidates on shared semantic axes, and resolves the turn by a
//! tension-sharpened sample into *speaking* a basin or falling *silent* on a
//! metastable stalemate. Standing tension is carried across turns, so
//! contradiction can persist rather than being resolved every cycle.
//!
//! Step function for the same reason as `MirrorLoop`: the lib owns no clock, so
//! [`DialecticLoop::turn`] does one cycle and the binary owns the loop and the
//! per-profile `inter_turn_delay`.
//!
//! ## Scope: milestone 2, deliberately
//!
//! [`DialecticalMemory`] here is **the temporal trajectory only** — the bounded
//! heard/reply rings that feed the centroids. The Swift's type of that name also
//! wraps a `SemanticGraph` and derives `entropy`/`drift` for a weight field;
//! those are milestones 3 and 5 and are absent, so:
//!
//! - `synthesis` is always `None` and `force_synthesis` always false, which
//!   `compete` already models as an `Option` parameter;
//! - weights are the profile's fixed [`Tuning::weights`] rather than a field
//!   advanced by gloss/entropy/drift;
//! - `gloss_scalar` stays at the neutral 0.5 (no `SpectralState` on Linux).
//!
//! The trajectory half is *not* optional: without it every turn scores as an
//! early turn, resonance and novelty sit at the neutral 0.5 forever, and the
//! three profiles would differ only in their silence thresholds.
//!
//! ## Honest scope, carried from the Swift
//!
//! MANUAL trigger only; not wired to any sleep-stage detector. The generator may
//! be a cloud model — and note this loop makes **two** generate calls per turn,
//! or **three** under Reflective. Speech-to-text stays on-device, so only text
//! ever leaves the machine. The dialectic is genuinely two-sided only in form:
//! both poles are the same model under different instructions, so a turn stages
//! a disagreement rather than holding one.

use crate::dynamics::{
    self, DialecticalCandidate, DialecticalOutcome, Resolution, ScoredCandidate, Tuning,
};
use crate::embedding::Embedding;
use crate::loops::chunk;
use crate::profile::ContextProfile;
use crate::role::DialecticalRole;
use crate::seams::{
    GenerationParams, Listening, Prosody, SeamError, SeamResult, SelectionDraws, SentenceEmbedding,
    Speaking, TextGenerating,
};
use crate::turn_log::{TurnLine, NEUTRAL_GLOSS};
use neuralcompose_mobile_core::provenance::MethodIdentity;

/// The Witness's stance. The *observing* posture lives in a system prompt so the
/// user prompt can stay a bare question, exactly as the Swift splits it between
/// `witnessPrompt` and `ClaudeCLIGenerator.witnessSystemPrompt`.
pub const WITNESS_SYSTEM: &str =
    "You are a silent observer of a conversation between two voices. You are \
     never quoted and never speak to the participants. Name, in one short \
     sentence, what both voices avoided noticing. Do not take a side, do not \
     summarize, and do not offer advice. Output only the observation.";

/// The Witness's user prompt: what was heard plus both poles' candidates, so it
/// can name what the pair avoided.
pub fn witness_prompt(heard: &str, candidates: &[String]) -> String {
    let voices: Vec<String> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| format!("Voice {}: {}", i + 1, c))
        .collect();
    format!(
        "They heard: {heard}\n\n{}\n\nWhat did both voices avoid noticing?",
        voices.join("\n")
    )
}

/// The bounded temporal trajectory the competition scores against.
///
/// **The Swift type of this name is larger.** This is its milestone-2 half only
/// — the heard/reply rings and the low-tension streak. The `SemanticGraph`, the
/// `entropy`/`drift` derivations and `synthesisCandidate` are milestones 3 and 5
/// and are deliberately absent; see the module header.
#[derive(Clone, Debug)]
pub struct DialecticalMemory {
    heard: Vec<Embedding>,
    replies: Vec<Embedding>,
    history_window: usize,
    tension_ceiling: f32,
    low_tension_streak: u32,
}

impl DialecticalMemory {
    pub fn new(history_window: usize, tension_ceiling: f32) -> Self {
        Self {
            heard: Vec::new(),
            replies: Vec::new(),
            history_window: history_window.max(1),
            tension_ceiling,
            low_tension_streak: 0,
        }
    }

    /// L2-normalized mean of recent *heard* utterances — the human's trajectory.
    pub fn history_centroid(&self) -> Option<Embedding> {
        dynamics::centroid(&self.heard)
    }

    /// L2-normalized mean of recent *replies* — the machine's trajectory.
    pub fn reply_centroid(&self) -> Option<Embedding> {
        dynamics::centroid(&self.replies)
    }

    pub fn low_tension_streak(&self) -> u32 {
        self.low_tension_streak
    }

    pub fn record_heard(&mut self, embedding: Embedding) {
        push_bounded(&mut self.heard, embedding, self.history_window);
    }

    pub fn record_reply(&mut self, embedding: Embedding) {
        push_bounded(&mut self.replies, embedding, self.history_window);
    }

    /// Advances the convergence streak. A turn at or below the ceiling extends
    /// it; anything above resets it to zero.
    pub fn observe(&mut self, tension: f32) {
        if tension <= self.tension_ceiling {
            self.low_tension_streak += 1;
        } else {
            self.low_tension_streak = 0;
        }
    }
}

fn push_bounded(ring: &mut Vec<Embedding>, value: Embedding, window: usize) {
    ring.push(value);
    if ring.len() > window {
        let excess = ring.len() - window;
        ring.drain(0..excess);
    }
}

#[derive(Clone, Debug)]
pub struct DialecticConfig {
    pub max_tokens: u32,
    /// Base prosody, and the fallback voice for a candidate whose role id is not
    /// in the role set (e.g. a resurfaced synthesis).
    pub prosody: Prosody,
    pub silence_cues: Vec<String>,
    pub history_window: usize,
    /// See [`crate::loops::MirrorConfig::chunk_replies`]. Set false for a
    /// neural voice, which owns its own sentence-level prosody.
    pub chunk_replies: bool,
    /// Stamped into every turn record's provenance envelope (ADR-004).
    ///
    /// Supplied by the shell rather than read here, because a pure library
    /// cannot know its own deployment — the same division `CaptureBuildIdentity`
    /// already uses. `git_commit` is `None` for a build that did not come from a
    /// checkout, and that is never treated as a pinned build.
    pub software_version: String,
    pub git_commit: Option<String>,
}

impl Default for DialecticConfig {
    fn default() -> Self {
        Self {
            max_tokens: 60,
            prosody: Prosody::HYPNAGOGIC,
            silence_cues: crate::loops::DEFAULT_SILENCE_CUES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            history_window: 16,
            chunk_replies: true,
            software_version: env!("CARGO_PKG_VERSION").to_string(),
            git_commit: None,
        }
    }
}

/// What one dialectical turn did.
#[derive(Clone, Debug, PartialEq)]
pub struct DialecticTurn {
    pub index: u64,
    pub heard: String,
    pub scored: Vec<ScoredCandidate>,
    pub resolution: Resolution,
    pub spoken: Option<String>,
    pub chunks: Vec<String>,
    /// Prosody actually used, blended across the roles by the competition's own
    /// probabilities — this is what makes tension audible.
    pub prosody: Prosody,
    pub witness_attempted: bool,
    pub witness_finding: Option<String>,
    pub witness_distance: Option<f32>,
    pub self_similarity: Option<f32>,
    pub consecutive_silence: u32,
    /// Set when the Witness ran and failed. The turn still succeeds — a witness
    /// failure must not break it — but it must not be silent either, or a
    /// persistently-failing Reflective run looks byte-identical to Focused.
    pub witness_error: Option<String>,
}

impl DialecticTurn {
    /// The persisted record for this turn.
    ///
    /// `method` is passed in rather than derived here because a turn does not
    /// know which build ran it — [`DialecticLoop::method_identity`] is where the
    /// profile's tuning and the shell's version meet.
    pub fn to_turn_line(&self, mode: &str, method: MethodIdentity) -> TurnLine {
        let mut line = TurnLine::new(
            self.index,
            mode,
            &self.heard,
            &self.scored,
            &self.resolution,
            method,
        )
        .with_witness(
            self.witness_attempted,
            self.witness_finding.clone(),
            self.witness_distance,
        );
        if let Some(s) = self.self_similarity {
            line = line.with_self_similarity(s);
        }
        line.gloss_scalar = NEUTRAL_GLOSS;
        line
    }
}

pub struct DialecticLoop<L, G, S, E, R> {
    listener: L,
    generator: G,
    speaker: S,
    embedder: E,
    draws: R,
    roles: Vec<DialecticalRole>,
    profile: ContextProfile,
    tuning: Tuning,
    config: DialecticConfig,
    memory: DialecticalMemory,
    method: MethodIdentity,
    standing_tension: f32,
    consecutive_silence: u32,
    silence_index: usize,
    turn_index: u64,
}

impl<L, G, S, E, R> DialecticLoop<L, G, S, E, R>
where
    L: Listening,
    G: TextGenerating,
    S: Speaking,
    E: SentenceEmbedding,
    R: SelectionDraws,
{
    // Eight injected collaborators. Every one is a distinct type, so a
    // transposed argument is a compile error rather than a runtime surprise —
    // which is the failure clippy's threshold exists to catch. Grouping them
    // into a struct would only move the same eight names one level down.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        listener: L,
        generator: G,
        speaker: S,
        embedder: E,
        draws: R,
        roles: Vec<DialecticalRole>,
        profile: ContextProfile,
        config: DialecticConfig,
    ) -> Self {
        let tuning = profile.tuning();
        let memory =
            DialecticalMemory::new(config.history_window, tuning.synthesis_tension_ceiling);
        // Computed once: the profile and the shell's build identity are both
        // fixed for the life of the loop, so the digest cannot drift mid-session.
        let method = crate::turn_log::dialectic_method_identity(
            profile,
            config.software_version.clone(),
            config.git_commit.clone(),
        );
        Self {
            listener,
            generator,
            speaker,
            embedder,
            draws,
            roles,
            profile,
            tuning,
            config,
            memory,
            method,
            standing_tension: 0.0,
            consecutive_silence: 0,
            silence_index: 0,
            turn_index: 0,
        }
    }

    pub fn turns_taken(&self) -> u64 {
        self.turn_index
    }

    /// Which build, under which frozen tuning, is producing these turns.
    /// Hand this to [`DialecticTurn::to_turn_line`].
    pub fn method_identity(&self) -> MethodIdentity {
        self.method.clone()
    }

    pub fn standing_tension(&self) -> f32 {
        self.standing_tension
    }

    /// One dialectical turn. `Ok(None)` means the turn was skipped without any
    /// state change — silence heard, or every generator returned empty text.
    pub fn turn(&mut self) -> SeamResult<Option<DialecticTurn>> {
        let heard = match self.listener.listen()? {
            Some(h) if !h.trim().is_empty() => h.trim().to_string(),
            // Nothing heard: speak a soft cue rather than competing over silence.
            _ => {
                let cue = self.next_silence_cue();
                for c in self.split(&cue) {
                    self.speaker.speak(&c, self.config.prosody)?;
                }
                return Ok(None);
            }
        };

        // 1. One generator per role, each shaped by the standing tension.
        let mut candidates: Vec<(String, String)> = Vec::new();
        for role in &self.roles {
            let params = GenerationParams {
                temperature: role.temperature,
                max_tokens: self.config.max_tokens,
            };
            let raw = self.generator.generate(
                role_system(role),
                &role.prompt(&heard, self.standing_tension),
                params,
            )?;
            let text = crate::loops::strip_for_speech(&raw);
            if !text.is_empty() {
                candidates.push((role.id.to_string(), text));
            }
        }
        // Both generators empty — skip, and change no state.
        if candidates.is_empty() {
            return Ok(None);
        }

        // 2. Embed heard + every candidate.
        let mut texts: Vec<&str> = vec![heard.as_str()];
        texts.extend(candidates.iter().map(|(_, t)| t.as_str()));
        let mut embeddings = Vec::with_capacity(texts.len());
        for t in &texts {
            embeddings.push(self.embedder.embed(t)?);
        }
        let heard_emb = embeddings[0].clone();
        let candidate_embs: Vec<Embedding> = embeddings[1..].to_vec();

        // 3. Score against the accumulated trajectory.
        let history_centroid = self.memory.history_centroid();
        let reply_centroid = self.memory.reply_centroid();
        let mut scored = Vec::with_capacity(candidates.len());
        for ((role_id, text), emb) in candidates.iter().zip(candidate_embs.iter()) {
            let energy = dynamics::energy(
                emb,
                &heard_emb,
                history_centroid.as_ref(),
                reply_centroid.as_ref(),
            )
            .ok_or_else(|| {
                SeamError::Failed(format!(
                    "candidate from role {role_id} is not comparable with the turn context"
                ))
            })?;
            // DELIBERATE DIVERGENCE: the Swift writes `role?.objective(energy) ?? 0`.
            // 0 is exactly the "this role failed its brief" signal this field
            // exists to carry, so a lookup miss would impersonate the diagnostic.
            // See lib.rs's divergences section.
            let role_fulfillment = self
                .roles
                .iter()
                .find(|r| r.id == role_id)
                .map(|r| r.fulfillment(&energy));
            scored.push(ScoredCandidate {
                candidate: DialecticalCandidate {
                    text: text.clone(),
                    embedding: emb.clone(),
                    role_id: role_id.clone(),
                },
                energy,
                potential: energy.potential(&self.tuning.weights),
                role_fulfillment,
            });
        }

        let tension = dynamics::tension(&candidate_embs).ok_or_else(|| {
            SeamError::Failed("candidates are not mutually comparable".to_string())
        })?;

        let draw = self.draws.next_draw();
        let resolution = dynamics::compete(&scored, tension, draw, &self.tuning, None, false);

        // 4. Record and act.
        self.memory.record_heard(heard_emb);
        self.memory.observe(tension);
        self.standing_tension = tension;

        // The embedding whose colour this turn carried. On a silent turn the
        // strongest candidate stands in, so the reflexive metric still has a
        // subject — silence is a non-resolution, not an absence of position.
        let spoken_emb: Option<Embedding> = match &resolution.outcome {
            DialecticalOutcome::Spoke(c) | DialecticalOutcome::Synthesized(c) => {
                Some(c.embedding.clone())
            }
            DialecticalOutcome::Silent => scored
                .iter()
                .max_by(|a, b| a.potential.total_cmp(&b.potential))
                .map(|s| s.candidate.embedding.clone()),
        };
        let self_similarity = spoken_emb.as_ref().and_then(|emb| {
            reply_centroid
                .as_ref()
                .and_then(|rc| emb.cosine_similarity(rc))
                .map(dynamics::normalized)
        });

        // 5. The Witness — Reflective only, a THIRD generate call, never voiced.
        let witness_attempted = self.profile.witness_enabled();
        let mut witness_finding = None;
        let mut witness_distance = None;
        let mut witness_error = None;
        if witness_attempted {
            let candidate_texts: Vec<String> = candidates.iter().map(|(_, t)| t.clone()).collect();
            let params = GenerationParams {
                temperature: 1.0,
                max_tokens: self.config.max_tokens,
            };
            match self.generator.generate(
                WITNESS_SYSTEM,
                &witness_prompt(&heard, &candidate_texts),
                params,
            ) {
                Ok(raw) => {
                    let finding = crate::loops::strip_for_speech(&raw);
                    if !finding.is_empty() {
                        if let (Some(se), Ok(fe)) =
                            (spoken_emb.as_ref(), self.embedder.embed(&finding))
                        {
                            witness_distance = fe
                                .cosine_similarity(se)
                                .map(|c| 1.0 - dynamics::normalized(c));
                        }
                        witness_finding = Some(finding);
                    }
                }
                // A witness failure must NOT break the turn — but it must not be
                // silent either, or a persistently-failing Reflective run is
                // indistinguishable from a Focused one.
                Err(e) => witness_error = Some(e.to_string()),
            }
        }

        // 6. Voice the outcome.
        let mut spoken = None;
        let mut chunks = Vec::new();
        let mut turn_prosody = self.config.prosody;
        match &resolution.outcome {
            DialecticalOutcome::Spoke(c) | DialecticalOutcome::Synthesized(c) => {
                self.consecutive_silence += 0;
                self.memory.record_reply(c.embedding.clone());
                // Blend the role voices by how the competition actually went, so
                // a close call carries the losing pole's colour.
                let potentials: Vec<f32> = scored.iter().map(|s| s.potential).collect();
                let probs = dynamics::probabilities(&potentials, resolution.selection_temperature);
                let weighted: Vec<(Prosody, f32)> = scored
                    .iter()
                    .zip(probs.iter())
                    .map(|(s, p)| (self.role_prosody(&s.candidate.role_id), *p))
                    .collect();
                turn_prosody = Prosody::blend(&weighted);
                // The blend's NUMERIC fields carry the competition, but the
                // voice must identify the SPEAKER. `Prosody::blend` returns the
                // heaviest contributor's voice, and the heaviest contributor is
                // not always the winner: selection is a tension-sharpened
                // SAMPLE, so a lower-potential basin can take the turn — observed
                // live, a turn spoke the pole scoring 1.518 over the one scoring
                // 1.565. Leaving it would voice one pole's words in the other's
                // voice, defeating the single requirement fixed voices exist to
                // meet: a listener tracks which position speaks BY VOICE.
                turn_prosody.voice = self.role_prosody(&c.role_id).voice;
                chunks = self.split(&c.text);
                for ch in &chunks {
                    self.speaker.speak(ch, turn_prosody)?;
                }
                spoken = Some(c.text.clone());
            }
            DialecticalOutcome::Silent => {
                self.consecutive_silence += 1;
                // A silence run is bounded so the loop can never stall into
                // permanent quiet.
                if self.consecutive_silence >= self.profile.max_consecutive_silence() {
                    self.consecutive_silence += 0;
                    let cue = self.next_silence_cue();
                    chunks = self.split(&cue);
                    for ch in &chunks {
                        self.speaker.speak(ch, self.config.prosody)?;
                    }
                    spoken = Some(cue);
                }
            }
        }

        let index = self.turn_index;
        self.turn_index += 1;
        Ok(Some(DialecticTurn {
            index,
            heard,
            scored,
            resolution,
            spoken,
            chunks,
            prosody: turn_prosody,
            witness_attempted,
            witness_finding,
            witness_distance,
            self_similarity,
            consecutive_silence: self.consecutive_silence,
            witness_error,
        }))
    }

    /// Micro-phrases, or the whole utterance when a neural voice should own the
    /// prosody.
    fn split(&self, text: &str) -> Vec<String> {
        if self.config.chunk_replies {
            chunk(text)
        } else {
            let whole = text.trim();
            if whole.is_empty() {
                Vec::new()
            } else {
                vec![whole.to_string()]
            }
        }
    }

    fn role_prosody(&self, id: &str) -> Prosody {
        self.roles
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.voice)
            .unwrap_or(self.config.prosody)
    }

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

/// Each pole speaks under its own instruction; the register lives in the role's
/// prompt, so the system message only has to enforce speakability.
fn role_system(_role: &DialecticalRole) -> &'static str {
    "Your reply is spoken aloud by a text-to-speech engine. Plain conversational \
     prose only: no markdown, no asterisks, no headings, no lists, no stage \
     directions, no emoji."
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seams::{
        GenerationParams, Listening, ScriptedDraws, SeamResult, SentenceEmbedding, Speaking,
        TextGenerating,
    };

    fn e(v: &[f32]) -> Embedding {
        Embedding::new(v.to_vec(), "t")
    }

    const RING_INPUT: [[f32; 2]; 5] = [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [1.0, 0.0], [1.0, 0.0]];

    /// The trajectory rings are BOUNDED. Unbounded they grow for the life of a
    /// session — a slow leak, and worse, a centroid drifting toward the whole
    /// history instead of the recent window, so the dialogue stops responding to
    /// where it actually is.
    #[test]
    fn the_trajectory_rings_are_bounded_to_the_history_window() {
        let mut m = DialecticalMemory::new(2, 0.35);
        for v in RING_INPUT {
            m.record_heard(e(&v));
        }
        let bounded = m.history_centroid().expect("a centroid");
        // The last two are both (1, 0), so the windowed centroid is exactly that.
        assert!((bounded.values[0] - 1.0).abs() < 1e-6);
        assert!(bounded.values[1].abs() < 1e-6);

        // Companion: over all five the centroid is NOT (1, 0), so the assertion
        // above genuinely discriminates windowed from unwindowed.
        let mut unbounded = DialecticalMemory::new(99, 0.35);
        for v in RING_INPUT {
            unbounded.record_heard(e(&v));
        }
        assert!(
            unbounded.history_centroid().unwrap().values[1].abs() > 1e-3,
            "the two centroids are indistinguishable; this test cannot discriminate"
        );
    }

    #[test]
    fn the_reply_ring_is_bounded_too() {
        let mut m = DialecticalMemory::new(1, 0.35);
        m.record_reply(e(&[0.0, 1.0]));
        m.record_reply(e(&[1.0, 0.0]));
        assert!(
            (m.reply_centroid().unwrap().values[0] - 1.0).abs() < 1e-6,
            "the ring kept a stale reply"
        );
    }

    #[test]
    fn the_low_tension_streak_extends_and_resets() {
        let mut m = DialecticalMemory::new(4, 0.35);
        m.observe(0.1);
        m.observe(0.35);
        assert_eq!(m.low_tension_streak(), 2, "the ceiling is inclusive");
        m.observe(0.36);
        assert_eq!(
            m.low_tension_streak(),
            0,
            "a tense turn must reset the streak"
        );
    }

    #[test]
    fn the_witness_prompt_carries_both_voices_and_the_question() {
        let p = witness_prompt("i keep starting things", &["one".into(), "two".into()]);
        assert!(p.contains("i keep starting things"));
        assert!(p.contains("Voice 1: one"));
        assert!(p.contains("Voice 2: two"));
        assert!(p.contains("What did both voices avoid noticing?"));
    }

    struct L;
    impl Listening for L {
        fn listen(&mut self) -> SeamResult<Option<String>> {
            Ok(Some("something".into()))
        }
    }
    struct G(usize);
    impl TextGenerating for G {
        fn generate(&mut self, _s: &str, _p: &str, _g: GenerationParams) -> SeamResult<String> {
            self.0 += 1;
            Ok(format!("candidate {}", self.0))
        }
    }
    struct Sp;
    impl Speaking for Sp {
        fn speak(&mut self, _t: &str, _p: Prosody) -> SeamResult<()> {
            Ok(())
        }
    }
    struct E(usize);
    impl SentenceEmbedding for E {
        fn embed(&mut self, text: &str) -> SeamResult<Embedding> {
            let v = if text.starts_with("something") {
                vec![1.0, 0.0]
            } else {
                self.0 += 1;
                if self.0 == 1 {
                    vec![0.99, 0.1]
                } else {
                    vec![0.1, 0.99]
                }
            };
            Ok(Embedding::new(v, "t"))
        }
    }

    /// The spoken turn must carry the voice of the pole that ACTUALLY SPOKE,
    /// even when the other pole was the heavier contributor to the blend.
    ///
    /// The precondition is asserted, not assumed: unless the favourite and the
    /// speaker differ, this case cannot tell "voice of the speaker" from "voice
    /// of the favourite" and passes against either. A first version omitted
    /// that and duly survived a mutation reverting the fix.
    #[test]
    fn the_spoken_voice_follows_the_speaker_not_the_favourite() {
        // A draw at the top of the cumulative distribution lands in the LAST
        // bucket — the least likely candidate.
        let mut l = DialecticLoop::new(
            L,
            G(0),
            Sp,
            E(0),
            ScriptedDraws::new(vec![0.999]),
            crate::role::waking_roles().to_vec(),
            ContextProfile::Focused,
            DialecticConfig::default(),
        );
        let turn = l.turn().unwrap().unwrap();
        let spoke = match &turn.resolution.outcome {
            DialecticalOutcome::Spoke(c) => c.role_id.clone(),
            other => panic!("expected a spoken turn, got {other:?}"),
        };
        let voice_of = |id: &str| {
            crate::role::waking_roles()
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .voice
                .voice
        };

        let potentials: Vec<f32> = turn.scored.iter().map(|s| s.potential).collect();
        let probs = dynamics::probabilities(&potentials, turn.resolution.selection_temperature);
        let heaviest = turn
            .scored
            .iter()
            .zip(probs.iter())
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(s, _)| s.candidate.role_id.clone())
            .unwrap();
        assert_ne!(
            heaviest, spoke,
            "degenerate setup: the favourite IS the speaker, so this cannot discriminate"
        );
        assert_ne!(voice_of(&heaviest), voice_of(&spoke));
        assert_eq!(
            turn.prosody.voice,
            voice_of(&spoke),
            "spoke as {spoke} but voiced as {:?} (favourite was {heaviest})",
            turn.prosody.voice
        );
        assert!(
            turn.prosody.rate.is_some(),
            "numeric fields must still blend"
        );
    }

    /// A neural voice owns its own sentence prosody, so it must be handed whole
    /// utterances. Chunking also costs a model load per fragment.
    #[test]
    fn disabling_chunking_speaks_one_whole_utterance() {
        #[derive(Default)]
        struct Counting(std::rc::Rc<std::cell::RefCell<Vec<String>>>);
        impl Speaking for Counting {
            fn speak(&mut self, t: &str, _p: Prosody) -> SeamResult<()> {
                self.0.borrow_mut().push(t.to_string());
                Ok(())
            }
        }
        struct MultiSentence;
        impl TextGenerating for MultiSentence {
            fn generate(&mut self, _s: &str, _p: &str, _g: GenerationParams) -> SeamResult<String> {
                Ok("One thing. Then another, softly.".into())
            }
        }
        for (chunk_replies, want) in [(true, 3usize), (false, 1)] {
            let said = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let mut l = DialecticLoop::new(
                L,
                MultiSentence,
                Counting(std::rc::Rc::clone(&said)),
                E(0),
                ScriptedDraws::new(vec![0.5]),
                crate::role::waking_roles().to_vec(),
                ContextProfile::Focused,
                DialecticConfig {
                    chunk_replies,
                    ..DialecticConfig::default()
                },
            );
            l.turn().unwrap().unwrap();
            assert_eq!(
                said.borrow().len(),
                want,
                "chunk_replies={chunk_replies} produced {:?}",
                said.borrow()
            );
        }
    }
}
