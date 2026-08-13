//! The four hypnagogic modes, and the dialectic tuning behind three of them.
//!
//! Port of `Sources/BCICore/Dialectic/ContextProfile.swift` (108 lines) and the
//! `HypnagogicMode` enum at `Sources/NeuralComposeApp/AppViewModel.swift:50`.
//!
//! The numeric constants here **are** the modes — a drifted constant is a
//! changed product, not a style difference — so they are kept byte-identical to
//! the Swift and pinned by tests below.
//!
//! The names deliberately avoid sleep-stage and physiological language: they
//! describe how the *dialogue behaves*, not a state the Muse is claimed to
//! detect or induce.

use crate::dynamics::{DialecticalWeights, Tuning};
use std::time::Duration;

/// The single hypnagogic interaction mode — one axis, not an
/// `InteractionStyle × ContextProfile` grid.
///
/// `Mirror` runs the plain reply loop (ONE generate call per turn). The other
/// three run the dialectic competition (TWO calls per turn, or three under
/// Reflective) at the matching [`ContextProfile`] tuning. "Dialectical" is not a
/// separate toggle: choosing `Reflective` — the canonical dialectical behaviour
/// — *is* choosing dialectical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HypnagogicMode {
    Mirror,
    Focused,
    Reflective,
    Contemplative,
}

impl HypnagogicMode {
    pub const ALL: [HypnagogicMode; 4] = [
        HypnagogicMode::Mirror,
        HypnagogicMode::Focused,
        HypnagogicMode::Reflective,
        HypnagogicMode::Contemplative,
    ];

    /// Wire/CLI identifier. Matches the Swift `rawValue`, which the
    /// `NEURALCOMPOSE_HYPNAGOGIC_AUTOSTART` env var also uses.
    pub fn id(self) -> &'static str {
        match self {
            HypnagogicMode::Mirror => "mirror",
            HypnagogicMode::Focused => "focused",
            HypnagogicMode::Reflective => "reflective",
            HypnagogicMode::Contemplative => "contemplative",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HypnagogicMode::Mirror => "Mirror",
            HypnagogicMode::Focused => "Focused",
            HypnagogicMode::Reflective => "Reflective",
            HypnagogicMode::Contemplative => "Contemplative",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == s)
    }

    /// The dialectic tuning profile for this mode, or `None` for `Mirror` (the
    /// non-competing reply loop). The non-mirror ids line up with
    /// [`ContextProfile`], so this is a direct lift.
    pub fn profile(self) -> Option<ContextProfile> {
        match self {
            HypnagogicMode::Mirror => None,
            HypnagogicMode::Focused => Some(ContextProfile::Focused),
            HypnagogicMode::Reflective => Some(ContextProfile::Reflective),
            HypnagogicMode::Contemplative => Some(ContextProfile::Contemplative),
        }
    }

    /// Whether this mode runs the dialectic loop.
    pub fn is_dialectical(self) -> bool {
        self.profile().is_some()
    }
}

/// A named location in the dialectic's behavioral space. A profile is almost
/// entirely a **preset over knobs the engine already has** — with one exception
/// that is not a knob: see [`ContextProfile::witness_enabled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextProfile {
    /// Coherent, grounded, conversational: resists drift, resolves readily,
    /// rarely falls silent.
    Focused,
    /// The default dialectical behaviour: gentle semantic exploration with
    /// tension that persists across turns.
    Reflective,
    /// *Less, not more* (the point is non-elaboration, à la zazen): low novelty
    /// pressure, synthesis suppressed, and a high tolerance for unresolved
    /// tension and silence, at a slower cadence.
    Contemplative,
}

impl ContextProfile {
    pub const ALL: [ContextProfile; 3] = [
        ContextProfile::Focused,
        ContextProfile::Reflective,
        ContextProfile::Contemplative,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ContextProfile::Focused => "focused",
            ContextProfile::Reflective => "reflective",
            ContextProfile::Contemplative => "contemplative",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ContextProfile::Focused => "Focused",
            ContextProfile::Reflective => "Reflective",
            ContextProfile::Contemplative => "Contemplative",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            ContextProfile::Focused => {
                "Coherent, grounded, conversational — resists drift, resolves readily, rarely silent."
            }
            ContextProfile::Reflective => {
                "Gentle semantic exploration with tension that persists across turns (the default)."
            }
            ContextProfile::Contemplative => {
                "Slower and quieter — low novelty pressure, synthesis suppressed, high tolerance for unresolved tension and silence."
            }
        }
    }

    /// Competition + field + synthesis knobs. `Reflective` is exactly the
    /// shipped defaults; the others move only existing parameters.
    pub fn tuning(self) -> Tuning {
        let default = Tuning::default();
        match self {
            ContextProfile::Focused => Tuning {
                // stalemate→silence is rare
                high_tension: 0.75,
                // ground, don't wander
                weights: DialecticalWeights {
                    coherence: 1.2,
                    resonance: 0.8,
                    novelty: 0.4,
                },
                // resolve readily
                synthesis_high_bar: 0.5,
                synthesis_low_bar: 0.4,
                synthesis_sustain_k: 3,
                // grounded; EEG barely nudges
                gloss_wind: 0.2,
                ..default
            },
            ContextProfile::Reflective => default,
            ContextProfile::Contemplative => Tuning {
                // more turns count as undecided…
                stalemate_margin: 0.12,
                // …and fall silent more readily
                high_tension: 0.45,
                // reduced novelty pressure
                weights: DialecticalWeights {
                    coherence: 0.9,
                    resonance: 0.6,
                    novelty: 0.5,
                },
                // reluctant to resolve
                synthesis_high_bar: 0.8,
                synthesis_low_bar: 0.65,
                synthesis_sustain_k: 6,
                gloss_wind: 0.2,
                ..default
            },
        }
    }

    /// How long a run of dialectical silences may persist before a soft cue —
    /// contemplative tolerates the most. Bounds the loop so it can never stall
    /// into permanent silence.
    pub fn max_consecutive_silence(self) -> u32 {
        match self {
            ContextProfile::Focused => 2,
            ContextProfile::Reflective => 3,
            ContextProfile::Contemplative => 6,
        }
    }

    /// Pause between turns — contemplative breathes slowest.
    pub fn inter_turn_delay(self) -> Duration {
        match self {
            ContextProfile::Focused => Duration::from_secs(2),
            ContextProfile::Reflective => Duration::from_secs(3),
            ContextProfile::Contemplative => Duration::from_secs(6),
        }
    }

    /// The introspective Witness runs ONLY for Reflective.
    ///
    /// **This is the one field that is not a preset.** A Witness turn makes a
    /// THIRD generate call, so it is control flow in the loop, not a knob on
    /// the dynamics — which is why Reflective's [`Self::tuning`] is exactly the
    /// default and the differentiation lives here instead. Focused and
    /// Contemplative leave it off (no third call).
    pub fn witness_enabled(self) -> bool {
        matches!(self, ContextProfile::Reflective)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_round_trips_through_its_id() {
        for m in HypnagogicMode::ALL {
            assert_eq!(HypnagogicMode::parse(m.id()), Some(m));
        }
        assert_eq!(HypnagogicMode::parse("dialectical"), None);
        assert_eq!(HypnagogicMode::parse(""), None);
    }

    /// Mirror is the only non-dialectical mode. If this flips, the loop
    /// selection in `loops.rs` silently changes which engine runs.
    #[test]
    fn only_mirror_lacks_a_profile() {
        assert!(!HypnagogicMode::Mirror.is_dialectical());
        assert_eq!(HypnagogicMode::Mirror.profile(), None);
        for m in [
            HypnagogicMode::Focused,
            HypnagogicMode::Reflective,
            HypnagogicMode::Contemplative,
        ] {
            assert!(m.is_dialectical());
            // The ids line up — that lift is what keeps the two enums in step.
            assert_eq!(m.profile().unwrap().id(), m.id());
        }
    }

    /// Reflective must remain byte-identical to the shipped defaults: the whole
    /// claim that it is "the canonical behaviour" rests on this.
    #[test]
    fn reflective_tuning_is_exactly_the_default() {
        assert_eq!(ContextProfile::Reflective.tuning(), Tuning::default());
    }

    /// Pins the constants that ARE the modes, against the Swift source.
    #[test]
    fn profile_constants_match_the_swift() {
        let f = ContextProfile::Focused.tuning();
        assert_eq!(f.high_tension, 0.75);
        assert_eq!(f.weights.coherence, 1.2);
        assert_eq!(f.weights.resonance, 0.8);
        assert_eq!(f.weights.novelty, 0.4);
        assert_eq!(f.synthesis_high_bar, 0.5);
        assert_eq!(f.synthesis_low_bar, 0.4);
        assert_eq!(f.synthesis_sustain_k, 3);
        assert_eq!(f.gloss_wind, 0.2);

        let c = ContextProfile::Contemplative.tuning();
        assert_eq!(c.stalemate_margin, 0.12);
        assert_eq!(c.high_tension, 0.45);
        assert_eq!(c.weights.coherence, 0.9);
        assert_eq!(c.weights.resonance, 0.6);
        assert_eq!(c.weights.novelty, 0.5);
        assert_eq!(c.synthesis_high_bar, 0.8);
        assert_eq!(c.synthesis_low_bar, 0.65);
        assert_eq!(c.synthesis_sustain_k, 6);
        assert_eq!(c.gloss_wind, 0.2);

        // Fields the profiles must NOT move — a preset that quietly retuned the
        // selection temperature would change every mode at once.
        for p in ContextProfile::ALL {
            let t = p.tuning();
            assert_eq!(t.tau_base, 0.5);
            assert_eq!(t.tau_tension_slope, 0.35);
            assert_eq!(t.tau_min, 0.12);
            assert_eq!(t.decisive_gap, 0.25);
            assert_eq!(t.field_inertia, 0.12);
            assert_eq!(t.gloss_ema_alpha, 0.6);
        }

        // `stalemate_margin` cannot go in that loop — contemplative moves it on
        // purpose — so it is pinned per profile instead. Added because a
        // mutation of the default from 0.05 to 0.02 survived the ENTIRE suite,
        // conformance fixture included: no fixture case has a top-two margin
        // landing between those two values, so nothing observed the change.
        // Pinning the constant is the proportionate fix; a behavioural case at
        // the boundary would need injected potentials in the harness.
        assert_eq!(Tuning::default().stalemate_margin, 0.05);
        assert_eq!(ContextProfile::Focused.tuning().stalemate_margin, 0.05);
        assert_eq!(ContextProfile::Reflective.tuning().stalemate_margin, 0.05);
        assert_eq!(
            ContextProfile::Contemplative.tuning().stalemate_margin,
            0.12
        );
    }

    /// The behavioural ordering the modes exist to express. Asserted as an
    /// ordering rather than three magic numbers so a coordinated retune stays
    /// legal and an accidental inversion does not.
    #[test]
    fn profiles_are_ordered_from_decisive_to_tolerant() {
        use ContextProfile::*;
        // Silence tolerance: focused resists it, contemplative invites it.
        assert!(
            Focused.tuning().high_tension > Reflective.tuning().high_tension
                && Reflective.tuning().high_tension > Contemplative.tuning().high_tension
        );
        assert!(
            Focused.max_consecutive_silence() < Reflective.max_consecutive_silence()
                && Reflective.max_consecutive_silence() < Contemplative.max_consecutive_silence()
        );
        // Cadence: contemplative breathes slowest.
        assert!(
            Focused.inter_turn_delay() < Reflective.inter_turn_delay()
                && Reflective.inter_turn_delay() < Contemplative.inter_turn_delay()
        );
        // Novelty pressure: focused grounds hardest.
        assert!(Focused.tuning().weights.novelty < Contemplative.tuning().weights.novelty);
        // Resolving: contemplative is the most reluctant to synthesize.
        assert!(Focused.tuning().synthesis_high_bar < Contemplative.tuning().synthesis_high_bar);
    }

    #[test]
    fn the_witness_is_reflective_only() {
        assert!(ContextProfile::Reflective.witness_enabled());
        assert!(!ContextProfile::Focused.witness_enabled());
        assert!(!ContextProfile::Contemplative.witness_enabled());
        assert_eq!(
            ContextProfile::ALL
                .iter()
                .filter(|p| p.witness_enabled())
                .count(),
            1
        );
    }
}
