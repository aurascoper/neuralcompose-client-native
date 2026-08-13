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
    pub fn to_turn_line(&self, mode: &str) -> TurnLine {
        let mut line = TurnLine::new(
            self.index,
            mode,
            &self.heard,
            &self.scored,
            &self.resolution,
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
            standing_tension: 0.0,
            consecutive_silence: 0,
            silence_index: 0,
            turn_index: 0,
        }
    }

    pub fn turns_taken(&self) -> u64 {
        self.turn_index
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
                for c in chunk(&cue) {
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
                chunks = chunk(&c.text);
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
                    chunks = chunk(&cue);
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
