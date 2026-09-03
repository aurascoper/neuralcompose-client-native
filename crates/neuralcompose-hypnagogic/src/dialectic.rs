//! The dialectic loop — a persistent dynamical competition, one turn at a time.
//!
//! Port of `Sources/BCICore/Composition/HypnagogicDialecticLoop.swift` (460
//! lines), **plus behaviour the Swift does not have** — the session anchor, the
//! prompt re-framing and the repetition floor. All three are listed under
//! *Additions beyond the Swift* in `lib.rs`; none is a port artifact.
//!
//! Where [`crate::loops::MirrorLoop`] collapses each turn into one
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
     sentence, what both voices avoided noticing — and if the exchange has \
     stopped being about its own subject, say that instead. Do not take a \
     side, do not summarize, and do not offer advice. Output only the \
     observation.";

/// Phrases a voice uses when it does not recognise what it was asked about.
///
/// Matched as substrings, unlike [`crate::loops::STOP_PHRASES`], because these
/// appear inside a reply rather than being the whole of one. That makes the
/// check brittle in the way a phrase list always is, and it is accepted here
/// because the flag is recorded rather than acted on — a false positive costs a
/// wrong field in a log line, not a wrong turn.
const CLARIFICATION_PHRASES: [&str; 6] = [
    "could you clarify",
    "can you clarify",
    "not a standard",
    "what do you mean by",
    "i'm not sure what",
    "unfamiliar with",
];

/// Whether a candidate is asking what it was just asked about.
///
/// **Recorded, never acted on, and nothing reads it yet.** Its value is
/// retrospective: grepping past sessions to ask whether a voice objected before
/// the drift alarm did. There is no such consumer today, and saying so here is
/// the point — a field written and never queried is precisely the
/// `witness_distance` situation this module's other changes exist to fix, so it
/// is labelled unread rather than left to look like coverage.
pub fn asks_for_clarification(text: &str) -> bool {
    let lower = text.to_lowercase();
    CLARIFICATION_PHRASES.iter().any(|p| lower.contains(p))
}

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
    reply_texts: Vec<String>,
    history_window: usize,
    tension_ceiling: f32,
    low_tension_streak: u32,
}

impl DialecticalMemory {
    pub fn new(history_window: usize, tension_ceiling: f32) -> Self {
        Self {
            heard: Vec::new(),
            replies: Vec::new(),
            reply_texts: Vec::new(),
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

    /// The reply TEXTS, kept alongside the embeddings for the repetition check.
    ///
    /// Separate from the embedding ring on purpose: that one exists to be
    /// averaged into a centroid, and this one must not be, because the centroid
    /// is exactly the measure that fails to separate a stuck run from a
    /// coherent one. See [`crate::loops::similarity`].
    pub fn record_reply_text(&mut self, text: String, window: usize) {
        self.reply_texts.push(text);
        if self.reply_texts.len() > window {
            let excess = self.reply_texts.len() - window;
            self.reply_texts.drain(0..excess);
        }
    }

    /// Drops the retained reply texts. Called when the repetition guard fires,
    /// so the guard tests the turns that come after the intervention rather
    /// than latching on the ones that triggered it.
    pub fn clear_reply_texts(&mut self) {
        self.reply_texts.clear();
    }

    /// How many of the retained replies sit at or above `floor` in overlap with
    /// some *earlier* retained reply.
    ///
    /// A rate over the window rather than a consecutive run: the fixed point
    /// this exists to catch alternates between two attractors, so a streak
    /// requirement never fires on it.
    pub fn repetition_hits(&self, floor: f32) -> usize {
        self.reply_texts
            .iter()
            .enumerate()
            .filter(|(i, t)| {
                self.reply_texts[..*i]
                    .iter()
                    .any(|prev| crate::loops::similarity(prev, t) >= floor)
            })
            .count()
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
    /// Speak **both** positions, each in its own role voice, before the turn
    /// resolves.
    ///
    /// Off by default, because it changes what a turn sounds like and the
    /// conformance story is about what a turn *decides*.
    ///
    /// With it off, the competition is inaudible: two candidates are generated,
    /// scored, and one is spoken — so a listener hears a reply, not a dialectic.
    /// The disagreement that the whole engine exists to stage happens silently
    /// and is only visible afterwards in the turn log.
    ///
    /// With it on, each pole says its piece in its own fixed voice, and only
    /// then does the turn resolve. A synthesis is spoken afterwards because its
    /// text is neither candidate's; a plain win is not repeated, because it was
    /// already said as one of the positions.
    pub voice_both: bool,
    /// Distance from the session's anchor past which the poles' prompts are
    /// re-framed with the opening utterance. `0.0` disables.
    ///
    /// Lives on the config rather than on [`Tuning`] because `Tuning` is pinned
    /// constant-for-constant against the Swift and this knob has no Swift
    /// counterpart — see the *Additions beyond the Swift* section in `lib.rs`.
    ///
    /// **Calibrated against `bge-small-en-v1.5-f32`, and only that.** Twenty
    /// live turns through the real embedder, anchored on "What do you know
    /// about radiotropic biofilms?":
    ///
    /// | | n | min | median | max |
    /// |---|---|---|---|---|
    /// | on-topic follow-ups | 10 | 0.036 | 0.064 | **0.074** |
    /// | unrelated utterances | 10 | **0.294** | 0.324 | 0.347 |
    ///
    /// The default sits at the midpoint of that gap. An earlier draft shipped
    /// **0.45**, chosen from the theoretical `0.0..=1.0` range and from a test
    /// embedder whose vectors were orthogonal — which real sentence embeddings
    /// never are. Nothing reached it: `0.45` fired on 0 of those 20 turns,
    /// including the ten about kettles and gulls. A bound nothing can reach
    /// looks exactly like a guard that is satisfied.
    ///
    /// ponytail: embedder-specific. Change `NC_EMBED_MODEL` and this number is
    /// wrong — a different model has a different cone. Twenty turns on one
    /// anchor is also a thin calibration. `tests/drift_calibration.rs` pins the
    /// measurement so a re-fit is a visible edit rather than a silent drift.
    pub drift_ceiling: f32,
    /// Token-overlap similarity at or above which a reply counts as a
    /// near-repeat of a recent one. `0.0` disables the repetition check.
    ///
    /// ponytail: floor calibrated on ONE session (1788413094), and its
    /// "healthy" reference turns were themselves drifting — 0.226 is the
    /// ceiling of a degrading region, not a clean one. 0.30 is deliberately
    /// permissive: it misses marginal repetition rather than firing on healthy
    /// variety. Re-fit against a second session before trusting it, and before
    /// promoting the response from a forced silent turn to ending the session.
    pub repetition_floor: f32,
    /// How many of the last [`Self::repetition_window`] replies must sit at or
    /// above the floor before the turn is forced silent.
    ///
    /// A **rate**, not a streak, and that is the whole design. The observed
    /// fixed point oscillates between two attractors (0.968 at turn 43, 0.840
    /// at turn 44 of session 1788413094), so any consecutive-run requirement
    /// never fires on the very transcript that motivated the check.
    pub repetition_hits: usize,
    pub repetition_window: usize,
    /// Stamped into every turn record's provenance envelope (ADR-004).
    ///
    /// Supplied by the shell rather than read here, because a pure library
    /// cannot know its own deployment — the same division `CaptureBuildIdentity`
    /// already uses. `git_commit` is `None` for a build that did not come from a
    /// checkout, and that is never treated as a pinned build.
    pub software_version: String,
    pub git_commit: Option<String>,
    /// Which sampler wrote the candidates, for the turn record's method
    /// identity — see [`crate::turn_log::dialectic_method_identity`]. Supplied
    /// by the shell for the same reason as `software_version`: the library
    /// holds a `TextGenerating`, and a trait object cannot say where its text
    /// came from.
    pub generator: String,
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
            voice_both: false,
            drift_ceiling: 0.18,
            repetition_floor: 0.30,
            repetition_hits: 3,
            repetition_window: 8,
            software_version: env!("CARGO_PKG_VERSION").to_string(),
            git_commit: None,
            // The local path is the default everywhere, including in tests, so
            // a record that says anything else was an explicit choice.
            generator: "llama-server".to_string(),
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
    /// Cosine distance of this turn's `heard` from the session's anchor.
    /// `None` on the anchoring turn itself, and on an incomparable pair.
    pub topic_drift: Option<f32>,
    /// Whether the poles' prompts carried the opening utterance this turn.
    pub reanchored: bool,
    /// How many retained replies were near-repeats when this turn resolved.
    pub repetition_hits: usize,
    /// The competition chose to speak and the repetition guard overrode it.
    /// `resolution` still carries what the competition decided.
    pub repetition_forced_silence: bool,
    /// A voice said it did not recognise the subject. Recorded only; see
    /// [`asks_for_clarification`].
    pub clarification_requested: bool,
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
    pub fn to_turn_line(&self, mode: &str, method: MethodIdentity, generator: &str) -> TurnLine {
        let mut line = TurnLine::new(
            self.index,
            mode,
            &self.heard,
            &self.scored,
            &self.resolution,
            method,
            generator,
        )
        .with_witness(
            self.witness_attempted,
            self.witness_finding.clone(),
            self.witness_distance,
        )
        .with_drift(
            self.topic_drift,
            self.reanchored,
            self.repetition_hits,
            self.repetition_forced_silence,
            self.clarification_requested,
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
    /// The session's opening utterance and its embedding, set on the first turn
    /// that actually proceeds and never updated afterwards.
    ///
    /// Never updated is the entire point. `DialecticalMemory`'s rings are
    /// bounded, so every centroid in this file drifts along with the
    /// conversation — measure against one of those and a slow walk away from
    /// the subject reads as no movement at all.
    anchor: Option<(String, Embedding)>,
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
            config.generator.clone(),
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
            anchor: None,
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

        // 0. Embed what was heard BEFORE generating, so the drift from the
        // session's anchor is known while the prompts are still being built.
        // The Swift embeds this in the batch at step 2; moving it earlier costs
        // nothing (the same single call, just sooner) and is what lets step 1
        // react to drift at all.
        let heard_emb = self.embedder.embed(&heard)?;

        // Distance from the anchor — the FIRST utterance of the session, set
        // once and never updated. Measuring against `history_centroid` instead
        // would be useless here: that ring is bounded, so it drifts along with
        // the conversation and a walk away from the subject never registers.
        //
        // `None` from `cosine_similarity` means the two are incomparable, and
        // it stays `None` rather than becoming a number. Same rule as
        // divergence #1 in `lib.rs`: an absent value must not impersonate a
        // real one, and here a `0.0` sentinel would read as "no drift at all".
        let drift: Option<f32> = self.anchor.as_ref().and_then(|(_, a)| {
            heard_emb
                .cosine_similarity(a)
                .map(|c| 1.0 - dynamics::normalized(c))
        });

        // When the exchange has wandered past the ceiling, the poles are shown
        // where it started. `heard` ITSELF is never rewritten — it still logs,
        // still embeds, still feeds `record_heard` exactly as transcribed,
        // because the drifted text is the evidence. Only the generation prompt
        // is framed, which also keeps `role.rs`'s prompt shapers verbatim from
        // the Swift, as their doc comment requires.
        let drifted = matches!((drift, self.config.drift_ceiling),
            (Some(d), ceiling) if ceiling > 0.0 && d > ceiling);
        // The seed handed to the roles as *instruction*, or `None`. It is
        // deliberately NOT spliced into `heard`: see
        // [`DialecticalRole::anchored_prompt`] for what that cost, measured.
        let frame: Option<String> = match (&self.anchor, drifted) {
            (Some((seed, _)), true) => Some(seed.clone()),
            _ => None,
        };

        // 1. One generator per role, each shaped by the standing tension.
        let mut candidates: Vec<(String, String)> = Vec::new();
        for role in &self.roles {
            let params = GenerationParams {
                temperature: role.temperature,
                max_tokens: self.config.max_tokens,
            };
            let prompt = match &frame {
                Some(seed) => role.anchored_prompt(&heard, self.standing_tension, seed),
                None => role.prompt(&heard, self.standing_tension),
            };
            let raw = self
                .generator
                .generate(role_system(role), &prompt, params)?;
            let text = crate::loops::strip_for_speech(&raw);
            if !text.is_empty() {
                candidates.push((role.id.to_string(), text));
            }
        }
        // Both generators empty — skip, and change no state.
        if candidates.is_empty() {
            return Ok(None);
        }

        // 2. Embed every candidate. `heard` was embedded at step 0.
        let mut candidate_embs: Vec<Embedding> = Vec::with_capacity(candidates.len());
        for (_, t) in &candidates {
            candidate_embs.push(self.embedder.embed(t)?);
        }

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
        //
        // The anchor is set HERE, not at step 0, because a turn that reaches
        // step 0 can still be skipped — every generator returning empty text
        // changes no state, and an anchor set on such a turn would be state
        // changed by a turn that officially did nothing.
        if self.anchor.is_none() {
            self.anchor = Some((heard.clone(), heard_emb.clone()));
        }
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

        // 5.5 Voice the positions, when asked.
        //
        // Deliberately AFTER the competition resolves, so
        // `nothing_is_spoken_before_the_competition_resolves` still holds: the
        // turn is decided before anything is heard, and what changes is only how
        // much of the decision is audible.
        //
        // Each pole gets its OWN voice, unblended. The blend below carries how
        // close the competition was, which is a property of the resolution; a
        // position is just itself, and blending it would smear the one signal
        // fixed voices exist to carry.
        let mut positions_voiced = false;
        if self.config.voice_both {
            for s in &scored {
                let voice = self.role_prosody(&s.candidate.role_id);
                for ch in self.split(&s.candidate.text) {
                    self.speaker.speak(&ch, voice)?;
                }
            }
            positions_voiced = true;
        }

        // 5.75 The repetition floor.
        //
        // The anchor at step 0 detects moving AWAY from the subject. It cannot
        // detect being stuck ON one: once the subject is `machine whirring`,
        // drift from the anchor is stable and small, which is the same
        // zero-by-construction blind spot a stably wrong seed has. A fixed
        // point needs its own check.
        //
        // Deliberately NOT `self_similarity`, which is right here, already
        // computed, already logged, and does not work — see
        // [`crate::loops::similarity`] for the two tables. Coherent and stuck
        // are not distinguishable by an embedding cosine against the recent
        // centroid, because both of them are coherent.
        let repetition_hits = if self.config.repetition_floor > 0.0 {
            self.memory.repetition_hits(self.config.repetition_floor)
        } else {
            0
        };
        let repetition_forced =
            self.config.repetition_floor > 0.0 && repetition_hits >= self.config.repetition_hits;

        // 6. Voice the outcome.
        let mut spoken = None;
        let mut chunks = Vec::new();
        let mut turn_prosody = self.config.prosody;
        match &resolution.outcome {
            // The competition's own decision is left INTACT in `resolution` and
            // in the turn record. It decided to speak; the guard overrode it,
            // and both of those are facts the log should keep. Rewriting the
            // outcome would hide the override behind a turn that merely looks
            // like an ordinary silence.
            DialecticalOutcome::Spoke(_) | DialecticalOutcome::Synthesized(_)
                if repetition_forced =>
            {
                self.consecutive_silence += 1;
                // Clear the window that fired. The intervention exists to break
                // the fixed point, and evidence kept across it would re-fire on
                // the next turn no matter what the loop did differently — the
                // guard would latch rather than test. A loop that is still stuck
                // refills the window with repeats and fires again, which is the
                // correct behaviour and is what the silence-run bound below is
                // there to terminate.
                self.memory.clear_reply_texts();
            }
            DialecticalOutcome::Spoke(c) | DialecticalOutcome::Synthesized(c) => {
                self.consecutive_silence += 0;
                self.memory.record_reply(c.embedding.clone());
                self.memory
                    .record_reply_text(c.text.clone(), self.config.repetition_window);
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
                // A plain win was already said, as one of the positions. Saying
                // it again would make every turn end in an echo. A SYNTHESIS is
                // different: its text is neither candidate's, so it has not been
                // heard yet and must be.
                let already_said =
                    positions_voiced && matches!(resolution.outcome, DialecticalOutcome::Spoke(_));
                if !already_said {
                    for ch in &chunks {
                        self.speaker.speak(ch, turn_prosody)?;
                    }
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
            topic_drift: drift,
            reanchored: drifted,
            repetition_hits,
            repetition_forced_silence: repetition_forced,
            clarification_requested: candidates.iter().any(|(_, t)| asks_for_clarification(t)),
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
mod clarification_tests {
    use super::asks_for_clarification;

    #[test]
    fn a_voice_objecting_to_the_term_is_flagged() {
        // Both from session 1788413094, verbatim.
        assert!(asks_for_clarification(
            "The key term here is \"radio traffic biofilms,\" which is not a \
             standard biological term."
        ));
        assert!(asks_for_clarification(
            "Could you clarify what you mean by \"radio trophic vial films\"?"
        ));
    }

    #[test]
    fn an_ordinary_reply_is_not_flagged() {
        assert!(!asks_for_clarification(
            "The transition between states is like a river shifting its course."
        ));
        // The phrase list is matched case-insensitively, so a capitalised
        // opening must still hit rather than slip past.
        assert!(asks_for_clarification("Can you clarify that?"));
    }
}

#[cfg(test)]
mod repetition_tests {
    use super::DialecticalMemory;

    #[test]
    fn a_varied_window_does_not_register_and_a_repeating_one_does() {
        let mut m = DialecticalMemory::new(16, 0.5);
        for t in [
            "the river shifts its course slowly",
            "biofilms rearrange their identity",
            "what if evolution runs on resonance",
        ] {
            m.record_reply_text(t.to_string(), 8);
        }
        assert_eq!(m.repetition_hits(0.30), 0, "varied replies registered");

        for _ in 0..3 {
            m.record_reply_text(
                "the machine whirring is a sound that suggests a mechanical process".to_string(),
                8,
            );
        }
        assert!(m.repetition_hits(0.30) >= 2, "near-verbatim repeats missed");
    }

    /// The window is bounded, so old repeats age out. Without this a session
    /// that repeated early would carry the hit forever and the guard would
    /// latch rather than measure.
    #[test]
    fn the_window_is_bounded_and_old_repeats_age_out() {
        let mut m = DialecticalMemory::new(16, 0.5);
        for _ in 0..2 {
            m.record_reply_text("the machine whirring is a mechanical process".into(), 4);
        }
        assert!(m.repetition_hits(0.30) >= 1);
        // Genuinely varied, sharing no vocabulary. An earlier draft of this
        // test used `format!("unrelated thought number {i}")`, which differs by
        // one token out of six and so registered as repetition itself — the
        // detector was right and the test data was wrong.
        for t in [
            "rivers carve stone slowly",
            "the kettle forgot its whistle",
            "eleven gulls argue about bread",
            "moss claims every north face",
        ] {
            m.record_reply_text(t.to_string(), 4);
        }
        assert_eq!(m.repetition_hits(0.30), 0, "the old repeats never aged out");
    }

    #[test]
    fn clearing_drops_the_evidence_that_fired() {
        let mut m = DialecticalMemory::new(16, 0.5);
        for _ in 0..3 {
            m.record_reply_text("the machine whirring is a mechanical process".into(), 8);
        }
        assert!(m.repetition_hits(0.30) > 0);
        m.clear_reply_texts();
        assert_eq!(m.repetition_hits(0.30), 0);
    }
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

    /// One utterance: what was said, and the voice it was said in.
    type Utterance = (String, Option<&'static str>);
    type Transcript = std::rc::Rc<std::cell::RefCell<Vec<Utterance>>>;

    /// Records what was said and in whose voice, so a test can assert on the
    /// audible shape of a turn rather than only on its decision.
    #[derive(Default)]
    struct Heard(Transcript);
    impl Speaking for Heard {
        fn speak(&mut self, t: &str, p: Prosody) -> SeamResult<()> {
            self.0.borrow_mut().push((t.to_string(), p.voice));
            Ok(())
        }
    }

    type SpyLoop = DialecticLoop<L, G, Heard, E, ScriptedDraws>;

    fn loop_with(voice_both: bool, heard: &Heard) -> SpyLoop {
        DialecticLoop::new(
            L,
            G(0),
            Heard(std::rc::Rc::clone(&heard.0)),
            E(0),
            ScriptedDraws::new(vec![0.999]),
            crate::role::waking_roles().to_vec(),
            ContextProfile::Focused,
            DialecticConfig {
                voice_both,
                chunk_replies: false,
                ..DialecticConfig::default()
            },
        )
    }

    /// The default: the competition is inaudible. One utterance per turn, and a
    /// listener hears a reply rather than a disagreement.
    #[test]
    fn without_voice_both_only_the_resolution_is_spoken() {
        let heard = Heard::default();
        let turn = loop_with(false, &heard).turn().unwrap().unwrap();
        let said = heard.0.borrow();
        assert_eq!(said.len(), 1, "expected one utterance, got {said:?}");
        assert_eq!(said[0].0, turn.spoken.clone().unwrap());
    }

    /// With it on, BOTH candidates are spoken, each in its own fixed voice —
    /// which is the whole point: a listener tracks which position is speaking by
    /// voice, and two positions in one voice is not a dialectic.
    #[test]
    fn voice_both_speaks_each_pole_in_its_own_voice() {
        let heard = Heard::default();
        let turn = loop_with(true, &heard).turn().unwrap().unwrap();
        let said = heard.0.borrow();

        assert_eq!(said.len(), 2, "expected both positions, got {said:?}");
        let texts: Vec<&str> = said.iter().map(|(t, _)| t.as_str()).collect();
        for s in &turn.scored {
            assert!(
                texts.contains(&s.candidate.text.as_str()),
                "{} was never spoken: {texts:?}",
                s.candidate.text
            );
        }
        let voices: Vec<Option<&'static str>> = said.iter().map(|(_, v)| *v).collect();
        assert_ne!(
            voices[0], voices[1],
            "both poles spoke in the same voice, so the dialectic is inaudible"
        );
    }

    /// A plain win was already said as one of the positions. Repeating it would
    /// end every turn in an echo.
    #[test]
    fn voice_both_does_not_repeat_a_winner_it_already_spoke() {
        let heard = Heard::default();
        let turn = loop_with(true, &heard).turn().unwrap().unwrap();
        assert!(
            matches!(turn.resolution.outcome, DialecticalOutcome::Spoke(_)),
            "this fixture must resolve to a plain win to discriminate"
        );
        let said = heard.0.borrow();
        let winner = turn.spoken.clone().unwrap();
        assert_eq!(
            said.iter().filter(|(t, _)| *t == winner).count(),
            1,
            "the winner was spoken twice: {said:?}"
        );
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
