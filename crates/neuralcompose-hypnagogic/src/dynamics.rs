//! The pure math of one dialectical turn: score candidates on the three
//! semantic axes, measure the tension between them, resolve the competition.
//!
//! Port of `Sources/BCICore/Dialectic/DialecticalDynamics.swift` (276 lines)
//! and the value types in `DialecticalCompetition.swift` (185). `f32`
//! throughout because the Swift is `Float`; widening to `f64` here would make
//! the conformance fixture unmatchable for no gain.
//!
//! Two design notes carried over from the Swift, because they are the reason
//! the code looks like this:
//!
//!  - **Selection is a single softmax sample, not a branch.** "Decisive" and
//!    "near-equilibrium" are the same mechanism at different margins: under a
//!    tension-sharpened temperature a large potential gap makes the sample
//!    near-deterministic, while a near-zero gap lets the draw tip the basin.
//!    `margin`/`decisive` are recorded for diagnostics and never hard-forked on.
//!  - **Tension never scores a candidate.** As an additive term it would be
//!    identical for every candidate and cancel in the softmax ratio, so it
//!    instead modulates the selection temperature, the silence gate, and (in
//!    the loop) the generator prompts.
//!
//! CONFORMANCE STATUS: this is a hand port and is **not yet asserted against
//! the Swift**. The fixture that makes "port" a checkable claim is generated on
//! the Mac (`swift` is not installed on the Linux box) and lands with
//! `tests/dialectic_conformance.rs`. Until then, treat agreement as intended,
//! not established.

use crate::embedding::Embedding;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────── weights ──

/// The three semantic axes a candidate is scored on, weighted into a single
/// dialectical potential `D`. Convention: all axis inputs are in `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DialecticalWeights {
    /// Fidelity to what was just heard.
    pub coherence: f32,
    /// Fit to the dialogue's accumulated trajectory.
    pub resonance: f32,
    /// Departure from what the machine has already said.
    pub novelty: f32,
}

impl DialecticalWeights {
    /// Neutral starting point. Coherence leads, novelty is a strong second so
    /// the displacement pole is a real contender, resonance is the gentlest pull.
    pub const BALANCED: Self = Self {
        coherence: 1.0,
        resonance: 0.6,
        novelty: 0.8,
    };
}

impl Default for DialecticalWeights {
    fn default() -> Self {
        Self::BALANCED
    }
}

// ──────────────────────────────────────────────────────────────── energy ──

/// One candidate's position on the three axes, each normalized to `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DialecticalEnergy {
    pub coherence: f32,
    pub resonance: f32,
    pub novelty: f32,
}

impl DialecticalEnergy {
    /// The weighted dialectical potential `D = ⟨weights, energy⟩`.
    pub fn potential(&self, w: &DialecticalWeights) -> f32 {
        w.coherence * self.coherence + w.resonance * self.resonance + w.novelty * self.novelty
    }
}

// ───────────────────────────────────────────────────────────── candidate ──

/// A generated continuation plus its embedding and the id of the role that
/// produced it. `role_id` is *provenance* — which objective it was generated to
/// pursue — not a fixed identity of the spoken output: which candidate turns
/// out to be the stabilizing move emerges from the energies each turn.
#[derive(Clone, Debug, PartialEq)]
pub struct DialecticalCandidate {
    pub text: String,
    pub embedding: Embedding,
    pub role_id: String,
}

/// A candidate paired with its scored energy and resulting potential. One
/// struct rather than parallel arrays so index alignment cannot drift.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoredCandidate {
    pub candidate: DialecticalCandidate,
    pub energy: DialecticalEnergy,
    pub potential: f32,
    /// How well this candidate satisfied the objective of the role that
    /// produced it, or `None` when the producing role could not be resolved.
    ///
    /// Diagnostic: it lets a later stage notice a role that failed its own brief
    /// (a displacement pass that produced something un-novel because the model
    /// refused to diverge). **That is exactly why this is an `Option` where the
    /// Swift writes `role?.objective(energy) ?? 0`** — `0` *is* the
    /// failed-its-brief signal, so a lookup miss would not hide among plausible
    /// values, it would impersonate the diagnostic and report a maximally-failed
    /// pole. See lib.rs's divergences section.
    pub role_fulfillment: Option<f32>,
}

// ─────────────────────────────────────────────────────────────── outcome ──

/// What a turn resolves to. `Silent` is a *legitimate* non-resolution: the loop
/// says nothing and carries the unresolved tension into the next turn.
/// Synthesis is a rare emergent third, not the default.
#[derive(Clone, Debug, PartialEq)]
pub enum DialecticalOutcome {
    Spoke(DialecticalCandidate),
    Silent,
    Synthesized(DialecticalCandidate),
}

// ──────────────────────────────────────────────────────────────── tuning ──

/// Small, well-commented knobs. Field-driven weights are milestone 5 and are
/// deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// Base selection temperature at zero tension (softer competition).
    pub tau_base: f32,
    /// How much rising tension sharpens the competition (lowers `τ`).
    pub tau_tension_slope: f32,
    /// Floor so `τ` never reaches zero, which would trap argmax and kill the
    /// bifurcation entirely.
    pub tau_min: f32,
    /// A turn is a stalemate when the top-two potential gap is below this.
    pub stalemate_margin: f32,
    /// …and only *falls silent* on a stalemate when tension is at least this
    /// high. A low-tension near-tie is genuine near-agreement, which should
    /// still speak — silence is reserved for unresolved opposition.
    pub high_tension: f32,
    /// Margin at/above which the turn is flagged decisive (diagnostic only).
    pub decisive_gap: f32,
    pub weights: DialecticalWeights,
    /// Bar a resurfaced idea must clear to interrupt as an emergent third when
    /// the dialogue is *not* converging.
    pub synthesis_high_bar: f32,
    /// The gentler bar once tension has stayed low for `synthesis_sustain_k`
    /// turns — convergence invites a synthesis.
    pub synthesis_low_bar: f32,
    pub synthesis_sustain_k: u32,
    /// Tension at/below which a turn counts as converging for the streak.
    pub synthesis_tension_ceiling: f32,
    /// Slow clock: fraction the weight field moves toward its target each turn.
    pub field_inertia: f32,
    /// Fast clock: EMA smoothing on the spectral gloss, per window.
    pub gloss_ema_alpha: f32,
    /// Maximum weight shift the gloss/history can induce. Modest on purpose so
    /// EEG biases, never dictates.
    pub gloss_wind: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            tau_base: 0.5,
            tau_tension_slope: 0.35,
            tau_min: 0.12,
            stalemate_margin: 0.05,
            high_tension: 0.6,
            decisive_gap: 0.25,
            weights: DialecticalWeights::BALANCED,
            synthesis_high_bar: 0.6,
            synthesis_low_bar: 0.45,
            synthesis_sustain_k: 4,
            synthesis_tension_ceiling: 0.35,
            field_inertia: 0.12,
            gloss_ema_alpha: 0.6,
            gloss_wind: 0.35,
        }
    }
}

/// The resolved competition plus the intermediate quantities the loop needs to
/// build a turn record.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolution {
    pub outcome: DialecticalOutcome,
    pub tension: f32,
    pub margin: f32,
    pub selection_temperature: f32,
    /// True when the potential gap alone would have decided the turn — the
    /// dynamics, not the draw, won.
    pub decisive: bool,
}

// ─────────────────────────────────────────────────────────────── scoring ──

/// Maps a raw cosine similarity `[-1, 1]` onto `[0, 1]` so every axis and the
/// tension share one non-negative scale.
#[inline]
pub fn normalized(cosine: f32) -> f32 {
    (cosine + 1.0) / 2.0
}

/// Scores one candidate against the turn context, or `None` if any reference
/// it must be scored against is **incomparable** with it.
///
/// Two absences are deliberately distinguished, and conflating them is what the
/// `Option` is for:
///
///  - A **missing** centroid (an early turn, before any history exists) is
///    legitimate and scores a neutral `0.5`, biasing neither pole. That is the
///    Swift's behaviour and the fixture pins it.
///  - An **incomparable** centroid or `heard` — a different embedding space or
///    dimension — is a defect, not a neutral turn. It yields `None` so the
///    caller must decide, rather than letting the candidate score as merely
///    unaligned and quietly lose the competition.
pub fn energy(
    candidate: &Embedding,
    heard: &Embedding,
    history_centroid: Option<&Embedding>,
    reply_centroid: Option<&Embedding>,
) -> Option<DialecticalEnergy> {
    let coherence = normalized(candidate.cosine_similarity(heard)?);
    let resonance = match history_centroid {
        Some(h) => normalized(candidate.cosine_similarity(h)?),
        None => 0.5,
    };
    // novelty = distance from what we've said; 1 − similarity on [0,1].
    let novelty = match reply_centroid {
        Some(r) => 1.0 - normalized(candidate.cosine_similarity(r)?),
        None => 0.5,
    };
    Some(DialecticalEnergy {
        coherence,
        resonance,
        novelty,
    })
}

/// Mean pairwise dissimilarity among the candidate embeddings, on `[0, 1]`.
/// Zero or one candidate ⇒ no tension. `None` if any pair is incomparable —
/// a tension averaged over a mismatched pair would be a fabricated number.
pub fn tension(embeddings: &[Embedding]) -> Option<f32> {
    if embeddings.len() < 2 {
        return Some(0.0);
    }
    let mut acc = 0.0f32;
    let mut pairs = 0u32;
    for i in 0..embeddings.len() {
        for j in (i + 1)..embeddings.len() {
            acc += 1.0 - normalized(embeddings[i].cosine_similarity(&embeddings[j])?);
            pairs += 1;
        }
    }
    Some(if pairs == 0 { 0.0 } else { acc / pairs as f32 })
}

/// `τ(T)`: higher tension → lower temperature → a sharper competition, which is
/// *also* more volatile near equilibrium, since a tiny margin under a low `τ`
/// flips on the smallest perturbation.
pub fn selection_temperature(tension: f32, tuning: &Tuning) -> f32 {
    (tuning.tau_base - tuning.tau_tension_slope * tension).max(tuning.tau_min)
}

// ───────────────────────────────────────────────────────────── selection ──

/// Numerically stable softmax over `potentials / τ`. Degenerate inputs (empty,
/// non-positive `τ`, or a zero-sum exponential) fall back to uniform.
pub fn probabilities(potentials: &[f32], tau: f32) -> Vec<f32> {
    let n = potentials.len();
    let uniform = || -> Vec<f32> {
        if n == 0 {
            Vec::new()
        } else {
            vec![1.0 / n as f32; n]
        }
    };
    let max_p = potentials.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if n == 0 || tau <= 0.0 || !max_p.is_finite() {
        return uniform();
    }
    let exps: Vec<f32> = potentials
        .iter()
        .map(|p| ((p - max_p) / tau).exp())
        .collect();
    let sum: f32 = exps.iter().sum();
    if sum <= 0.0 {
        return uniform();
    }
    exps.into_iter().map(|e| e / sum).collect()
}

/// Samples an index from `probabilities` using an injected uniform draw in
/// `[0, 1)` — the single point of non-determinism in the whole engine.
///
/// The draw is **injected, never seeded**. Swift's and Rust's PRNGs produce
/// different sequences from the same seed, so a seeded fixture would fail on a
/// correct port and the "selection differs while scores match" diagnostic would
/// fire every time and mean nothing. See `SelectionDraws` in `seams.rs`.
pub fn sample(probabilities: &[f32], draw: f64) -> usize {
    if probabilities.is_empty() {
        return 0;
    }
    let d = draw.clamp(0.0, 0.999_999) as f32;
    let mut cumulative = 0.0f32;
    for (i, p) in probabilities.iter().enumerate() {
        cumulative += p;
        if d < cumulative {
            return i;
        }
    }
    probabilities.len() - 1
}

// ─────────────────────────────────────────────────── synthesis primitive ──

/// How well a candidate *reconciles* the two poles: its similarity to whichever
/// pole it is farther from, on `[0, 1]`. A genuine synthesis must sit close to
/// both at once, so it is only as strong as its weaker connection.
///
/// Honest note on the geometry, carried from the Swift: "far from both poles
/// yet explaining both" is contradictory in an embedding metric — the point
/// nearest their midpoint is still ~45° from each, and moving farther from each
/// moves away from the midpoint too. So this measures *reconciliation*, not
/// distance. A copy of one pole scores only the poles' own cross-similarity
/// baseline, and a synthesis's transcendence comes instead from being a
/// recurring idea sourced from elsewhere in the graph.
pub fn synthesis_score(
    candidate: &Embedding,
    thesis: &Embedding,
    antithesis: &Embedding,
) -> Option<f32> {
    let to_thesis = normalized(candidate.cosine_similarity(thesis)?);
    let to_anti = normalized(candidate.cosine_similarity(antithesis)?);
    Some(to_thesis.min(to_anti))
}

// ──────────────────────────────────────────────────────────── resolution ──

/// Resolves a scored competition into an outcome, given the standing tension
/// and one draw. Precedence: an emergent **synthesis** resolves the standing
/// contradiction; a high-tension **stalemate** falls silent; otherwise the
/// basin is **sampled** from the tension-sharpened softmax.
///
/// `synthesis` stays `None` until memory is wired in (milestone 3); it is a
/// parameter rather than internal state so this function is total and
/// independently testable.
pub fn compete(
    scored: &[ScoredCandidate],
    tension: f32,
    draw: f64,
    tuning: &Tuning,
    synthesis: Option<&DialecticalCandidate>,
    force_synthesis: bool,
) -> Resolution {
    let tau = selection_temperature(tension, tuning);
    let potentials: Vec<f32> = scored.iter().map(|s| s.potential).collect();
    let mut sorted = potentials.clone();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let margin = if sorted.len() >= 2 {
        sorted[0] - sorted[1]
    } else {
        sorted.first().copied().unwrap_or(0.0)
    };
    let decisive = margin >= tuning.decisive_gap;

    let resolve = |outcome: DialecticalOutcome| Resolution {
        outcome,
        tension,
        margin,
        selection_temperature: tau,
        decisive,
    };

    // 1. Synthesis is an event — a genuinely new third idea, or a sustained
    //    convergence the caller has already detected. It resolves the turn.
    if force_synthesis {
        if let Some(s) = synthesis {
            return resolve(DialecticalOutcome::Synthesized(s.clone()));
        }
    }

    // Nothing to say.
    if scored.is_empty() {
        return resolve(DialecticalOutcome::Silent);
    }

    // 2. Metastable stalemate: opposed and undecided → hold the tension, say
    //    nothing this turn.
    if scored.len() >= 2 && margin < tuning.stalemate_margin && tension >= tuning.high_tension {
        return resolve(DialecticalOutcome::Silent);
    }

    // 3. Symmetry-breaking: sample a basin. Large margin under low τ ⇒
    //    near-deterministic; near-zero margin ⇒ the draw tips it.
    let probs = probabilities(&potentials, tau);
    let idx = sample(&probs, draw);
    resolve(DialecticalOutcome::Spoke(scored[idx].candidate.clone()))
}

/// L2-normalized mean of a set of embeddings — "the direction the dialogue has
/// been travelling."
///
/// `None` for an empty set, or if any member is incomparable with the first.
/// **Deliberate divergence from the Swift**, which takes provenance from the
/// first element and silently *skips* members of a different dimension: a
/// centroid that quietly averaged a subset would still be a plausible vector,
/// and every later similarity against it would be plausible too. On the happy
/// path — one embedder, one dimension — the two agree exactly, so this changes
/// nothing the conformance fixture measures.
pub fn centroid(embeddings: &[Embedding]) -> Option<Embedding> {
    let first = embeddings.first()?;
    let dim = first.values.len();
    if dim == 0 || !embeddings.iter().all(|e| first.is_comparable_with(e)) {
        return None;
    }
    let mut sum = vec![0.0f32; dim];
    for e in embeddings.iter() {
        for (i, v) in e.values.iter().enumerate() {
            sum[i] += v;
        }
    }
    let norm = sum.iter().map(|v| v * v).sum::<f32>().sqrt();
    let values = if norm > 1e-6 {
        sum.iter().map(|v| v / norm).collect()
    } else {
        sum
    };
    Some(Embedding::new(values, first.model_id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emb(values: &[f32]) -> Embedding {
        Embedding::new(values.to_vec(), "fixture")
    }

    fn scored(potential: f32, role: &str) -> ScoredCandidate {
        ScoredCandidate {
            candidate: DialecticalCandidate {
                text: format!("{role} says something"),
                embedding: emb(&[1.0, 0.0]),
                role_id: role.to_string(),
            },
            energy: DialecticalEnergy {
                coherence: 0.5,
                resonance: 0.5,
                novelty: 0.5,
            },
            potential,
            role_fulfillment: Some(0.0),
        }
    }

    #[test]
    fn normalized_maps_cosine_range_onto_unit_range() {
        assert_eq!(normalized(-1.0), 0.0);
        assert_eq!(normalized(0.0), 0.5);
        assert_eq!(normalized(1.0), 1.0);
    }

    /// Absent centroids must read neutral, not zero — a zero would bias every
    /// early turn toward the coherence pole.
    #[test]
    fn missing_centroids_score_neutral() {
        let e = energy(&emb(&[1.0, 0.0]), &emb(&[1.0, 0.0]), None, None).unwrap();
        assert_eq!(e.resonance, 0.5);
        assert_eq!(e.novelty, 0.5);
        assert!((e.coherence - 1.0).abs() < 1e-6);
    }

    /// MISSING and INCOMPARABLE must not resolve the same way. A missing
    /// centroid is an early turn; an incomparable one is a defect, and scoring
    /// it neutral would hide a mismatched embedding space forever.
    #[test]
    fn an_incomparable_reference_is_unscorable_not_neutral() {
        let candidate = emb(&[1.0, 0.0]);
        let heard = emb(&[1.0, 0.0]);
        let wrong_space = Embedding::new(vec![1.0, 0.0], "some-other-model");

        assert!(energy(&candidate, &heard, None, None).is_some());
        assert_eq!(energy(&candidate, &wrong_space, None, None), None);
        assert_eq!(energy(&candidate, &heard, Some(&wrong_space), None), None);
        assert_eq!(energy(&candidate, &heard, None, Some(&wrong_space)), None);
    }

    #[test]
    fn tension_needs_two_candidates_and_grows_with_opposition() {
        assert_eq!(tension(&[]), Some(0.0));
        assert_eq!(tension(&[emb(&[1.0, 0.0])]), Some(0.0));
        let agreeing = tension(&[emb(&[1.0, 0.0]), emb(&[1.0, 0.0])]).unwrap();
        let opposed = tension(&[emb(&[1.0, 0.0]), emb(&[-1.0, 0.0])]).unwrap();
        assert!(agreeing < 1e-6);
        assert!((opposed - 1.0).abs() < 1e-6);
        assert!(opposed > agreeing);
    }

    /// One mismatched candidate must not average into a plausible tension.
    #[test]
    fn tension_over_a_mismatched_pair_is_none() {
        let mixed = [
            emb(&[1.0, 0.0]),
            Embedding::new(vec![0.0, 1.0], "some-other-model"),
        ];
        assert_eq!(tension(&mixed), None);
    }

    #[test]
    fn temperature_falls_with_tension() {
        let t = Tuning::default();
        assert!((selection_temperature(0.0, &t) - 0.5).abs() < 1e-6);
        assert!(selection_temperature(0.5, &t) < selection_temperature(0.0, &t));
        assert!(selection_temperature(1.0, &t) < selection_temperature(0.5, &t));
    }

    /// `tau_min` is UNREACHABLE under the default tuning, and this pins that.
    ///
    /// Tension is bounded to `[0, 1]` by construction (it is a mean of
    /// `1 − normalized(cos)` terms), so the lowest temperature the default
    /// knobs can produce is `tau_base − tau_tension_slope = 0.15`, which is
    /// above the `0.12` floor. The floor only starts binding once the slope
    /// exceeds `tau_base − tau_min = 0.38`.
    ///
    /// That makes the clamp a guard that never meets its failing input under
    /// shipped settings — worth knowing rather than mistaking for tested
    /// behaviour, and worth keeping, because a profile is free to retune the
    /// slope and the floor is what stops `τ → 0` trapping argmax.
    #[test]
    fn the_temperature_floor_does_not_bind_under_default_tuning() {
        let t = Tuning::default();
        let lowest = selection_temperature(1.0, &t);
        assert!((lowest - 0.15).abs() < 1e-6);
        assert!(lowest > t.tau_min);

        // It does bind once a profile pushes the slope past the gap.
        let steep = Tuning {
            tau_tension_slope: 0.5,
            ..t
        };
        assert_eq!(selection_temperature(1.0, &steep), steep.tau_min);
    }

    #[test]
    fn probabilities_are_a_distribution_and_degenerate_inputs_are_uniform() {
        let p = probabilities(&[2.0, 1.0], 0.5);
        assert!((p.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!(p[0] > p[1]);
        assert!(probabilities(&[], 0.5).is_empty());
        assert_eq!(probabilities(&[1.0, 1.0], 0.0), vec![0.5, 0.5]);
    }

    /// The draw is a coordinate on the cumulative distribution, so these
    /// boundaries are the contract the fixture will pin.
    #[test]
    fn sample_walks_the_cumulative_distribution() {
        let p = vec![0.25, 0.75];
        assert_eq!(sample(&p, 0.0), 0);
        assert_eq!(sample(&p, 0.24), 0);
        assert_eq!(sample(&p, 0.25), 1);
        assert_eq!(sample(&p, 0.999), 1);
        // Out-of-range draws clamp rather than panic or wrap.
        assert_eq!(sample(&p, -1.0), 0);
        assert_eq!(sample(&p, 5.0), 1);
        assert_eq!(sample(&[], 0.5), 0);
    }

    #[test]
    fn synthesis_is_only_as_strong_as_its_weaker_connection() {
        let thesis = emb(&[1.0, 0.0]);
        let anti = emb(&[-1.0, 0.0]);
        // A copy of one pole is maximally far from the other, so it cannot
        // score as a synthesis however well it matches its own side.
        assert!(synthesis_score(&thesis, &thesis, &anti).unwrap() < 1e-6);
        // The midpoint direction reconciles both equally.
        let mid = emb(&[0.0, 1.0]);
        assert!((synthesis_score(&mid, &thesis, &anti).unwrap() - 0.5).abs() < 1e-6);
        // An incomparable pole is unscorable, not a weak connection.
        let wrong = Embedding::new(vec![0.0, 1.0], "some-other-model");
        assert_eq!(synthesis_score(&mid, &thesis, &wrong), None);
    }

    #[test]
    fn a_high_tension_near_tie_falls_silent() {
        let t = Tuning::default();
        let r = compete(
            &[scored(1.0, "a"), scored(1.01, "b")],
            0.9,
            0.5,
            &t,
            None,
            false,
        );
        assert_eq!(r.outcome, DialecticalOutcome::Silent);
        assert!(!r.decisive);
    }

    /// The same near-tie at LOW tension must speak — silence is reserved for
    /// unresolved opposition, not for agreement.
    #[test]
    fn a_low_tension_near_tie_still_speaks() {
        let t = Tuning::default();
        let r = compete(
            &[scored(1.0, "a"), scored(1.01, "b")],
            0.1,
            0.5,
            &t,
            None,
            false,
        );
        assert!(matches!(r.outcome, DialecticalOutcome::Spoke(_)));
    }

    #[test]
    fn a_wide_margin_is_decisive_and_speaks_even_under_high_tension() {
        let t = Tuning::default();
        let r = compete(
            &[scored(0.1, "a"), scored(3.0, "b")],
            0.9,
            0.5,
            &t,
            None,
            false,
        );
        assert!(r.decisive);
        match r.outcome {
            DialecticalOutcome::Spoke(c) => assert_eq!(c.role_id, "b"),
            other => panic!("expected the dominant basin to win, got {other:?}"),
        }
    }

    #[test]
    fn no_candidates_resolves_silent_rather_than_panicking() {
        let r = compete(&[], 0.0, 0.5, &Tuning::default(), None, false);
        assert_eq!(r.outcome, DialecticalOutcome::Silent);
        assert_eq!(r.margin, 0.0);
    }

    /// Synthesis outranks both the stalemate gate and sampling.
    #[test]
    fn forced_synthesis_takes_precedence() {
        let s = DialecticalCandidate {
            text: "a third thing".into(),
            embedding: emb(&[0.0, 1.0]),
            role_id: "synthesis".into(),
        };
        let r = compete(
            &[scored(1.0, "a"), scored(1.01, "b")],
            0.9,
            0.5,
            &Tuning::default(),
            Some(&s),
            true,
        );
        assert_eq!(r.outcome, DialecticalOutcome::Synthesized(s));
    }

    /// force_synthesis without a candidate must not silently synthesize.
    #[test]
    fn forced_synthesis_without_a_candidate_falls_through() {
        let r = compete(
            &[scored(3.0, "a")],
            0.0,
            0.5,
            &Tuning::default(),
            None,
            true,
        );
        assert!(matches!(r.outcome, DialecticalOutcome::Spoke(_)));
    }

    #[test]
    fn centroid_is_l2_normalized() {
        let c = centroid(&[emb(&[3.0, 0.0]), emb(&[0.0, 3.0])]).unwrap();
        let norm = c.values.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
        assert!((c.values[0] - c.values[1]).abs() < 1e-6);
        assert!(centroid(&[]).is_none());
    }

    /// A centroid over a mixed set would be a plausible vector that every later
    /// similarity is then measured against — so it refuses rather than skips.
    #[test]
    fn centroid_refuses_a_mixed_set_rather_than_averaging_a_subset() {
        assert_eq!(centroid(&[emb(&[3.0, 0.0]), emb(&[1.0])]), None);
        assert_eq!(
            centroid(&[
                emb(&[3.0, 0.0]),
                Embedding::new(vec![0.0, 3.0], "some-other-model")
            ]),
            None
        );
    }
}
