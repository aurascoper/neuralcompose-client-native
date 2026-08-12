//! The injected seams the loops orchestrate over.
//!
//! Mirrors the Swift protocols one-for-one (`HypnagogicListening`,
//! `TextGenerating`, `SpeechSynthesizing`, `SentenceEmbedder`) plus one the
//! Swift did not need. The loops are *pure orchestration*: the lib links no
//! subprocess, no socket and no model, exactly as the Swift loops link no
//! AVFoundation, MLX or CLI. That is what lets CI exercise the whole dialectic
//! with no llama.cpp, no whisper and no llama-server present.

use crate::embedding::Embedding;
use std::fmt;

/// Errors a seam can fail with. Deliberately small: the loops treat every
/// failure the same way — the turn degrades, the loop continues — so a richer
/// taxonomy would be structure nothing reads.
#[derive(Clone, Debug, PartialEq)]
pub enum SeamError {
    Unavailable(String),
    Failed(String),
}

impl fmt::Display for SeamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeamError::Unavailable(m) => write!(f, "unavailable: {m}"),
            SeamError::Failed(m) => write!(f, "failed: {m}"),
        }
    }
}

impl std::error::Error for SeamError {}

pub type SeamResult<T> = Result<T, SeamError>;

/// Turn-based, on-device speech capture.
///
/// Unlike a push-to-talk recognizer, [`Self::listen`] opens the mic, waits for
/// one utterance (or silence), returns the text, and closes the mic — so the
/// mic is only ever hot during an explicit listen turn, never during playback,
/// and there is no continuous background recording.
///
/// Implementors MUST transcribe on-device: raw audio never leaves the machine,
/// only the resulting text reaches the caller.
pub trait Listening {
    /// `Ok(None)` means only silence was heard before the timeout elapsed —
    /// a normal turn outcome, not an error. The mic is closed either way.
    fn listen(&mut self) -> SeamResult<Option<String>>;
}

/// Sampling parameters for one generate call. Per-role temperature is what
/// makes the coherence pole faithful and the displacement pole divergent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GenerationParams {
    pub temperature: f64,
    pub max_tokens: u32,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            temperature: 0.6,
            max_tokens: 60,
        }
    }
}

/// Text generation. The live implementation speaks HTTP to a long-lived
/// `llama-server`; an in-process `neuralcompose-llama` completion path can land
/// behind this same trait later without touching a loop.
pub trait TextGenerating {
    fn generate(
        &mut self,
        system: &str,
        prompt: &str,
        params: GenerationParams,
    ) -> SeamResult<String>;
}

/// Speech synthesis. Prosody is passed as a value rather than applied here so
/// the lib carries no audio dependency.
pub trait Speaking {
    fn speak(&mut self, text: &str, prosody: Prosody) -> SeamResult<()>;
}

/// How a spoken turn sounds. Port of `SpeechProsody`
/// (`Sources/BCICore/Protocols/SpeechSynthesizing.swift:59`).
///
/// Every field is optional, and `None` means "let the engine decide" rather
/// than zero — which is why [`Prosody::blend`] lets a `None` field *abstain*
/// instead of dragging the mean toward 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prosody {
    pub rate: Option<f32>,
    pub pitch_multiplier: Option<f32>,
    pub volume: Option<f32>,
    /// Seconds of silence before the utterance. `TimeInterval` in the Swift.
    pub pre_utterance_delay: Option<f64>,
}

impl Prosody {
    /// Slow, low-pitched, soft — hypnagogic cue playback that must not spike
    /// arousal. Deliberately conservative; the safety rationale (no harsh
    /// treble, no sudden onset) is in `SLEEP_CYCLE_DESIGN.md`.
    pub const HYPNAGOGIC: Self = Self {
        rate: Some(0.35),
        pitch_multiplier: Some(0.8),
        volume: Some(0.6),
        pre_utterance_delay: Some(0.4),
    };

    /// The coherence pole's sleep voice — identical to [`Self::HYPNAGOGIC`].
    pub const HYPNAGOGIC_STABILIZER: Self = Self::HYPNAGOGIC;

    /// The displacement pole's sleep voice: quicker and brighter so the
    /// dreaming undercurrent is *audibly* different, while staying inside the
    /// same arousal-safe envelope.
    pub const HYPNAGOGIC_DREAMER: Self = Self {
        rate: Some(0.42),
        pitch_multiplier: Some(0.98),
        volume: Some(0.6),
        pre_utterance_delay: Some(0.3),
    };

    /// The coherence pole's **waking** voice — present and natural-paced, NOT
    /// the slow arousal-safe envelope. Used by the waking role set, which is
    /// what the Focused / Reflective / Contemplative profiles run.
    pub const WAKING_COHERENT: Self = Self {
        rate: Some(0.5),
        pitch_multiplier: Some(1.0),
        volume: Some(0.9),
        pre_utterance_delay: Some(0.1),
    };

    /// The displacement pole's **waking** voice — quicker and brighter so the
    /// opposing voice is audibly distinct, without the sleepy slowness.
    pub const WAKING_DIVERGENT: Self = Self {
        rate: Some(0.54),
        pitch_multiplier: Some(1.06),
        volume: Some(0.9),
        pre_utterance_delay: Some(0.05),
    };

    /// Weighted mean of several prosodies — the mechanism that makes tension
    /// *audible*. A spoken turn is voiced by blending the role voices in
    /// proportion to the competition's probabilities, so a close call carries
    /// the losing pole's colour even though only the winner's words are said.
    ///
    /// Each field is averaged only over the contributors that specify it (a
    /// `None` field abstains); non-positive weights are ignored; an all-`None`
    /// prosody comes back when nothing contributes.
    pub fn blend(weighted: &[(Prosody, f32)]) -> Prosody {
        fn mean_f32(w: &[(Prosody, f32)], get: impl Fn(&Prosody) -> Option<f32>) -> Option<f32> {
            let (mut acc, mut wsum) = (0.0f32, 0.0f32);
            for (p, weight) in w.iter().filter(|(_, weight)| *weight > 0.0) {
                if let Some(v) = get(p) {
                    acc += v * weight;
                    wsum += weight;
                }
            }
            (wsum > 0.0).then(|| acc / wsum)
        }
        fn mean_f64(w: &[(Prosody, f32)], get: impl Fn(&Prosody) -> Option<f64>) -> Option<f64> {
            let (mut acc, mut wsum) = (0.0f64, 0.0f64);
            for (p, weight) in w.iter().filter(|(_, weight)| *weight > 0.0) {
                if let Some(v) = get(p) {
                    acc += v * *weight as f64;
                    wsum += *weight as f64;
                }
            }
            (wsum > 0.0).then(|| acc / wsum)
        }
        Prosody {
            rate: mean_f32(weighted, |p| p.rate),
            pitch_multiplier: mean_f32(weighted, |p| p.pitch_multiplier),
            volume: mean_f32(weighted, |p| p.volume),
            pre_utterance_delay: mean_f64(weighted, |p| p.pre_utterance_delay),
        }
    }
}

/// Sentence embedding — the input currency of the dialectic's scoring.
///
/// The live implementation wraps `neuralcompose_llama::Embedder` on the **CPU**
/// backend. That is not a fallback: `BACKEND_ID_CPU` is the row that is
/// `RuntimeSmokeValidated` on `linux/x86_64`, no Vulkan-embedder row is, and
/// embeddings sit on the critical path of every dialectic turn — so taking the
/// GPU here would put a second Vulkan context on the one iGPU beside
/// `llama-server`, which nothing in this workspace serialises across processes.
pub trait SentenceEmbedding {
    fn embed(&mut self, text: &str) -> SeamResult<Embedding>;
}

/// The single point of non-determinism in the engine, isolated behind a seam.
///
/// **Draws are injected, never seeded.** Swift's and Rust's PRNGs produce
/// different sequences from the same seed, so a fixture that recorded a seed
/// would fail against a *correct* port and the "selection differs while scores
/// match" diagnostic would fire every time and mean nothing. The conformance
/// fixture therefore records the actual uniform draws the Swift consumed, and
/// [`ScriptedDraws`] replays them.
pub trait SelectionDraws {
    /// Next uniform draw in `[0, 1)`.
    fn next_draw(&mut self) -> f64;
}

/// Replays a recorded sequence of draws, then repeats the last one.
///
/// Exhausting the script returns the final draw rather than panicking or
/// wrapping to the start: a wrap would make a too-short script look like a
/// working one and quietly change which basin won on later turns.
#[derive(Clone, Debug)]
pub struct ScriptedDraws {
    draws: Vec<f64>,
    index: usize,
}

impl ScriptedDraws {
    pub fn new(draws: Vec<f64>) -> Self {
        Self { draws, index: 0 }
    }

    /// How many draws were consumed — lets a conformance test assert the port
    /// made the same number of selection decisions as the reference.
    pub fn consumed(&self) -> usize {
        self.index
    }

    /// True once the script has been exhausted and draws are being repeated.
    pub fn is_exhausted(&self) -> bool {
        self.index >= self.draws.len()
    }
}

impl SelectionDraws for ScriptedDraws {
    fn next_draw(&mut self) -> f64 {
        if self.draws.is_empty() {
            return 0.0;
        }
        let i = self.index.min(self.draws.len() - 1);
        self.index += 1;
        self.draws[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_draws_replay_in_order() {
        let mut d = ScriptedDraws::new(vec![0.1, 0.9]);
        assert_eq!(d.next_draw(), 0.1);
        assert_eq!(d.next_draw(), 0.9);
        assert_eq!(d.consumed(), 2);
    }

    /// Exhaustion repeats the last draw and says so, rather than wrapping —
    /// a wrap would silently change later basins.
    #[test]
    fn scripted_draws_repeat_rather_than_wrap() {
        let mut d = ScriptedDraws::new(vec![0.1, 0.9]);
        d.next_draw();
        d.next_draw();
        assert!(!d.is_exhausted() || d.consumed() == 2);
        assert_eq!(d.next_draw(), 0.9);
        assert!(d.is_exhausted());
        assert_ne!(d.next_draw(), 0.1);
    }

    #[test]
    fn an_empty_script_is_total() {
        let mut d = ScriptedDraws::new(vec![]);
        assert_eq!(d.next_draw(), 0.0);
    }

    fn p(rate: Option<f32>, volume: Option<f32>) -> Prosody {
        Prosody {
            rate,
            pitch_multiplier: None,
            volume,
            pre_utterance_delay: None,
        }
    }

    #[test]
    fn prosody_blend_is_a_weighted_mean() {
        let b = Prosody::blend(&[(p(Some(1.0), None), 1.0), (p(Some(0.0), None), 1.0)]);
        assert_eq!(b.rate, Some(0.5));
        let skewed = Prosody::blend(&[(p(Some(1.0), None), 3.0), (p(Some(0.0), None), 1.0)]);
        assert_eq!(skewed.rate, Some(0.75));
    }

    /// A `None` field must ABSTAIN, not count as zero — otherwise a voice that
    /// simply declines to set `rate` would silently drag the blend toward
    /// silence.
    #[test]
    fn a_none_field_abstains_rather_than_counting_as_zero() {
        let b = Prosody::blend(&[(p(Some(1.0), None), 1.0), (p(None, Some(0.2)), 1.0)]);
        assert_eq!(b.rate, Some(1.0));
        assert_eq!(b.volume, Some(0.2));
        assert_eq!(b.pitch_multiplier, None);
    }

    #[test]
    fn non_positive_weights_are_ignored_and_an_empty_blend_is_all_none() {
        let b = Prosody::blend(&[(p(Some(1.0), None), 1.0), (p(Some(0.0), None), 0.0)]);
        assert_eq!(b.rate, Some(1.0));
        let neg = Prosody::blend(&[(p(Some(1.0), None), 1.0), (p(Some(0.0), None), -5.0)]);
        assert_eq!(neg.rate, Some(1.0));
        assert_eq!(Prosody::blend(&[]), Prosody::default());
        assert_eq!(
            Prosody::blend(&[(p(Some(1.0), None), 0.0)]),
            Prosody::default()
        );
    }

    /// The waking voices are what the three dialectical profiles actually use,
    /// and they must stay audibly distinct from each other and from the sleep
    /// envelope — that distinctness is the whole point of blending them.
    #[test]
    fn waking_voices_are_distinct_and_faster_than_the_sleep_envelope() {
        assert_ne!(Prosody::WAKING_COHERENT, Prosody::WAKING_DIVERGENT);
        assert!(Prosody::WAKING_DIVERGENT.rate > Prosody::WAKING_COHERENT.rate);
        assert!(Prosody::WAKING_COHERENT.rate > Prosody::HYPNAGOGIC.rate);
        assert_eq!(Prosody::HYPNAGOGIC_STABILIZER, Prosody::HYPNAGOGIC);
        assert!(Prosody::HYPNAGOGIC_DREAMER.rate > Prosody::HYPNAGOGIC.rate);
    }
}
