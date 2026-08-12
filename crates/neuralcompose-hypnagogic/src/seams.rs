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

/// How a spoken turn sounds. Port of the Swift `SpeechProsody`; the dialectic
/// blends role voices in proportion to the competition's probabilities, so
/// tension is audible even when only one basin is voiced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prosody {
    pub rate: f32,
    pub pitch: f32,
    pub volume: f32,
}

impl Prosody {
    /// Slow, low and quiet — the hypnagogic default.
    pub const HYPNAGOGIC: Self = Self {
        rate: 0.38,
        pitch: 0.85,
        volume: 0.7,
    };

    /// Weighted blend of two voices. `w` is the weight on `self`.
    pub fn blend(self, other: Prosody, w: f32) -> Prosody {
        let w = w.clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| a * w + b * (1.0 - w);
        Prosody {
            rate: mix(self.rate, other.rate),
            pitch: mix(self.pitch, other.pitch),
            volume: mix(self.volume, other.volume),
        }
    }
}

impl Default for Prosody {
    fn default() -> Self {
        Self::HYPNAGOGIC
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

    #[test]
    fn prosody_blend_is_weighted_and_clamped() {
        let a = Prosody {
            rate: 1.0,
            pitch: 1.0,
            volume: 1.0,
        };
        let b = Prosody {
            rate: 0.0,
            pitch: 0.0,
            volume: 0.0,
        };
        assert_eq!(a.blend(b, 1.0), a);
        assert_eq!(a.blend(b, 0.0), b);
        assert!((a.blend(b, 0.5).rate - 0.5).abs() < 1e-6);
        // Out-of-range weights clamp rather than extrapolating past either voice.
        assert_eq!(a.blend(b, 2.0), a);
        assert_eq!(a.blend(b, -1.0), b);
    }
}
