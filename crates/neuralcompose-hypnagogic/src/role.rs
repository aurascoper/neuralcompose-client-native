//! The competitors in the dialectic.
//!
//! Port of `Sources/BCICore/Dialectic/DialecticalRole.swift` (164 lines).
//!
//! A role is defined by **an objective it pursues**, not a fixed
//! "Stabilizer"/"Dreamer" identity. Each turn every role generates one candidate
//! from its own tension-aware prompt at its own sampling temperature; the
//! candidates then compete on the *shared* semantic axes. Which candidate turns
//! out to be the stabilizing (or the divergent) move is therefore an **emergent**
//! property of that turn's energies — the label is read off the result, not baked
//! into the pipeline.
//!
//! This is the extension seam: adding a `symbolic` / `counterfactual` /
//! `emotional` role is *data* — append a [`DialecticalRole`] — not new control
//! flow, because the competition already iterates over the slice. The first
//! implementation ships exactly two per register, and the extra roles are
//! deliberately deferred.
//!
//! ## Two registers, and the dialectical modes use the waking one
//!
//! The Swift ships two role sets with identical objectives and sampling regimes,
//! differing only in register (prompt language + voice):
//!
//! - [`sleep_roles`] — the hypnagogic pair, soft and arousal-safe, reserved for
//!   the future wind-down / dream rungs.
//! - [`waking_roles`] — lucid, position-holding voices. **This is what Focused /
//!   Reflective / Contemplative run.** Picking the sleep pair for those profiles
//!   would quietly swap the whole register.
//!
//! The prompt shapers are explicitly called out in the Swift as "the user's knob
//! — the literal instructions behind the spoken turns", so they are kept verbatim
//! rather than paraphrased.

use crate::dynamics::DialecticalEnergy;
use crate::seams::Prosody;

/// Tension at or above which a displacement role is asked to reach *further*
/// from the literal. Shared by both registers, and the one number in this file
/// that changes behaviour rather than wording.
pub const HIGH_REACH_TENSION: f32 = 0.6;

/// A competitor in the dialectic.
///
/// `prompt_shaper` takes the standing tension so the generators themselves
/// evolve: a displacement-seeking role reaches farther from the literal when
/// tension is already high, and explores a nearby metaphor when it is low.
#[derive(Clone, Copy)]
pub struct DialecticalRole {
    /// Stable identifier, stamped onto the candidate as provenance. Note both
    /// registers share the same two ids — the register is a property of the
    /// *set*, not of the role's identity, exactly as in the Swift.
    pub id: &'static str,
    /// Sampling temperature for this role's generate call. Low for the coherence
    /// pole (faithful), high for the displacement pole (divergent). This drives
    /// real sampling: it flows into `TextGenerating::generate`.
    pub temperature: f64,
    /// Builds the prompt given what was heard and the standing tension.
    pub prompt_shaper: fn(heard: &str, tension: f32) -> String,
    /// The scalar this role is trying to maximize, read off a candidate's
    /// energy. Used to measure how well a candidate fulfilled its brief, and to
    /// *name* the emergent role of a candidate — never to **select** the spoken
    /// output.
    pub objective: fn(&DialecticalEnergy) -> f32,
    /// How this role sounds. The spoken turn blends the role voices in
    /// proportion to the competition's probabilities.
    pub voice: Prosody,
}

impl DialecticalRole {
    pub fn prompt(&self, heard: &str, tension: f32) -> String {
        (self.prompt_shaper)(heard, tension)
    }

    pub fn fulfillment(&self, energy: &DialecticalEnergy) -> f32 {
        (self.objective)(energy)
    }
}

impl std::fmt::Debug for DialecticalRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialecticalRole")
            .field("id", &self.id)
            .field("temperature", &self.temperature)
            .finish_non_exhaustive()
    }
}

pub const COHERENCE_SEEKING_ID: &str = "coherence-seeking";
pub const DISPLACEMENT_SEEKING_ID: &str = "displacement-seeking";

// ─────────────────────────────────────────────────────── waking register ──

/// The waking pair — what the Focused / Reflective / Contemplative profiles run.
/// Order is not significant; the competition is symmetric over the slice.
pub fn waking_roles() -> [DialecticalRole; 2] {
    [COHERENCE_SEEKING_WAKING, DISPLACEMENT_SEEKING_WAKING]
}

/// Waking faithful pole: find the clearest thread in what was heard and carry it
/// forward, sharpened and grounded. Low temperature.
pub const COHERENCE_SEEKING_WAKING: DialecticalRole = DialecticalRole {
    id: COHERENCE_SEEKING_ID,
    temperature: 0.45,
    prompt_shaper: |heard, _tension| {
        format!(
            "In a live dialogue, the other person just said: \"{heard}\"\n\n\
             Respond as the voice that seeks coherence: find the strongest, \
             clearest thread in what they said and carry it forward faithfully — \
             sharpen it, ground it, make it more precise. Do not drift away from \
             it or hedge. At most three sentences. Output only your reply."
        )
    },
    objective: |e| e.coherence,
    voice: Prosody::WAKING_COHERENT,
};

/// Waking divergent pole: refuse to restate; open the idea from a genuinely
/// different angle. How hard it pushes scales with the standing tension.
pub const DISPLACEMENT_SEEKING_WAKING: DialecticalRole = DialecticalRole {
    id: DISPLACEMENT_SEEKING_ID,
    temperature: 1.0,
    prompt_shaper: |heard, tension| {
        let reach = if tension >= HIGH_REACH_TENSION {
            "Push hard against it — surface the tension, the counter-position, or the angle it ignores."
        } else {
            "Introduce a genuinely different angle — a reframing, an analogy, or an overlooked possibility."
        };
        format!(
            "In a live dialogue, the other person just said: \"{heard}\"\n\n\
             Respond as the voice that seeks displacement: do not restate or \
             simply agree. {reach} Offer one distinct, substantive move that \
             opens the idea rather than closing it. At most three sentences. \
             Output only your reply."
        )
    },
    objective: |e| e.novelty,
    voice: Prosody::WAKING_DIVERGENT,
};

// ──────────────────────────────────────────────────────── sleep register ──

/// The hypnagogic pair. Reserved for the future wind-down / dream rungs — the
/// dialectical modes use [`waking_roles`].
pub fn sleep_roles() -> [DialecticalRole; 2] {
    [COHERENCE_SEEKING_SLEEP, DISPLACEMENT_SEEKING_SLEEP]
}

/// The faithful pole: a soft, resonant mirror that stays close to what was heard.
/// Prompts stay inside the arousal-safe hypnagogic envelope (calm, ≤2 sentences)
/// per `SLEEP_CYCLE_DESIGN.md`.
pub const COHERENCE_SEEKING_SLEEP: DialecticalRole = DialecticalRole {
    id: COHERENCE_SEEKING_ID,
    temperature: 0.45,
    prompt_shaper: |heard, _tension| {
        format!(
            "You are a calm, passive presence beside someone drifting toward \
             sleep. They just murmured: \"{heard}\"\n\n\
             Reflect it back gently — stay close to their words and their \
             feeling, add nothing new or alarming. Reply in at most two short, \
             soft sentences. Output only the reply."
        )
    },
    objective: |e| e.coherence,
    voice: Prosody::HYPNAGOGIC_STABILIZER,
};

/// The divergent pole: a genuinely different, associative reading. How far it is
/// asked to reach scales with the standing tension, but it must stay soft and
/// dreamlike — never jarring — so it does not spike arousal.
pub const DISPLACEMENT_SEEKING_SLEEP: DialecticalRole = DialecticalRole {
    id: DISPLACEMENT_SEEKING_ID,
    temperature: 1.0,
    prompt_shaper: |heard, tension| {
        let reach = if tension >= HIGH_REACH_TENSION {
            "Drift well away from the literal — let it become an unexpected image or memory."
        } else {
            "Let it drift into a nearby image or gentle metaphor."
        };
        format!(
            "You are the dreaming undercurrent beside someone falling asleep. \
             They just murmured: \"{heard}\"\n\n\
             {reach} Offer a single soft, associative continuation — a distinct \
             interpretation, not a restatement. Keep it calm and quiet, at most \
             two short sentences, nothing sudden or frightening. Output only the \
             reply."
        )
    },
    objective: |e| e.novelty,
    voice: Prosody::HYPNAGOGIC_DREAMER,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(coherence: f32, novelty: f32) -> DialecticalEnergy {
        DialecticalEnergy {
            coherence,
            resonance: 0.5,
            novelty,
        }
    }

    #[test]
    fn each_register_ships_the_two_poles_with_the_swift_temperatures() {
        for set in [waking_roles(), sleep_roles()] {
            assert_eq!(set[0].id, COHERENCE_SEEKING_ID);
            assert_eq!(set[0].temperature, 0.45);
            assert_eq!(set[1].id, DISPLACEMENT_SEEKING_ID);
            assert_eq!(set[1].temperature, 1.0);
            // The faithful pole must be the colder one, or the poles have
            // swapped and every turn's divergence collapses.
            assert!(set[0].temperature < set[1].temperature);
        }
    }

    /// Objectives are read off the energy, and each pole maximizes a different
    /// axis. If both returned the same scalar the competition would be
    /// one-sided while still looking like a competition.
    #[test]
    fn the_poles_pursue_different_axes() {
        let e = energy(0.9, 0.1);
        assert_eq!(COHERENCE_SEEKING_WAKING.fulfillment(&e), 0.9);
        assert_eq!(DISPLACEMENT_SEEKING_WAKING.fulfillment(&e), 0.1);
        assert_eq!(COHERENCE_SEEKING_SLEEP.fulfillment(&e), 0.9);
        assert_eq!(DISPLACEMENT_SEEKING_SLEEP.fulfillment(&e), 0.1);
    }

    #[test]
    fn every_prompt_carries_what_was_heard() {
        let heard = "I keep starting projects and never finishing them";
        for role in waking_roles().iter().chain(sleep_roles().iter()) {
            for tension in [0.0, 0.5, 1.0] {
                assert!(
                    role.prompt(heard, tension).contains(heard),
                    "{} dropped the utterance at tension {tension}",
                    role.id
                );
            }
        }
    }

    /// The reach branch is the one place tension changes the *words* sent to the
    /// model, and it is pinned either side of the threshold.
    #[test]
    fn displacement_reaches_further_at_high_tension() {
        for role in [DISPLACEMENT_SEEKING_WAKING, DISPLACEMENT_SEEKING_SLEEP] {
            let low = role.prompt("x", HIGH_REACH_TENSION - 0.01);
            let at = role.prompt("x", HIGH_REACH_TENSION);
            let high = role.prompt("x", 0.95);
            assert_ne!(low, at, "{}: the threshold is inclusive", role.id);
            assert_eq!(at, high, "{}: above the bar is one branch", role.id);
        }
    }

    /// Coherence ignores tension entirely — its prompt is the same every turn.
    #[test]
    fn coherence_prompts_do_not_vary_with_tension() {
        for role in [COHERENCE_SEEKING_WAKING, COHERENCE_SEEKING_SLEEP] {
            assert_eq!(role.prompt("x", 0.0), role.prompt("x", 1.0));
        }
    }

    /// The two registers must not be interchangeable: same ids and objectives,
    /// different words and different voices. A copy-paste that left the sleep
    /// prompts in the waking set would be invisible except here.
    #[test]
    fn the_registers_differ_in_wording_and_voice_but_not_identity() {
        for (waking, sleep) in waking_roles().iter().zip(sleep_roles().iter()) {
            assert_eq!(waking.id, sleep.id);
            assert_eq!(waking.temperature, sleep.temperature);
            assert_ne!(waking.prompt("x", 0.5), sleep.prompt("x", 0.5));
            assert_ne!(waking.voice, sleep.voice);
        }
        // Register-specific language, so a swapped set is caught by content.
        assert!(COHERENCE_SEEKING_SLEEP.prompt("x", 0.0).contains("sleep"));
        assert!(!COHERENCE_SEEKING_WAKING.prompt("x", 0.0).contains("sleep"));
        assert!(COHERENCE_SEEKING_WAKING
            .prompt("x", 0.0)
            .contains("live dialogue"));
    }

    /// Prompts instruct the model to emit only the reply. Losing this is not
    /// cosmetic: preamble would be spoken aloud and scored as content.
    #[test]
    fn every_prompt_asks_for_the_reply_alone() {
        for role in waking_roles().iter().chain(sleep_roles().iter()) {
            let p = role.prompt("x", 0.5).to_lowercase();
            assert!(
                p.contains("output only"),
                "{} lost the bare-reply instruction",
                role.id
            );
        }
    }
}
