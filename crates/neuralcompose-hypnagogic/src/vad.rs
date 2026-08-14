//! Deciding when someone started and stopped speaking.
//!
//! **Pure**, like the rest of this lib. The microphone, the subprocess and the
//! audio buffer live in the binary; what is decided here is *when an utterance
//! begins and ends*, from a stream of frame levels. That decision is the whole
//! of the interesting logic and it is the half that fails silently — a gate set
//! wrong does not error, it produces a loop that listens forever and never
//! answers.
//!
//! ## Why this exists
//!
//! `--mic` was push-to-talk: it printed "speak now, then press Enter" and
//! blocked on stdin. For a hypnagogic session — which is the point of this
//! application, and which you take lying down with your eyes closed — pressing
//! Enter every turn defeats the exercise.
//!
//! ## Ported, not invented
//!
//! The constants and the shape come from `tools/spoken-loop/converse.py`, which
//! solved this on this machine already. Its notes are worth carrying over
//! verbatim, because each one is a measured failure rather than a preference:
//!
//! - This machine's mic emits frequent full-scale transients. Measured over 3 s:
//!   **median 1801, max 32768.** A 1 s *median* caught enough spikes to report a
//!   noise floor of 6262, which put the gate at 18785 — 57% of full scale, and
//!   unreachable by ordinary speech. The loop listened and never answered.
//!   Hence a **low percentile, not a median**.
//! - `GATE_MAX` is the backstop for when even the percentile is fooled.
//! - `FLOOR_MIN` is the other direction: a silent room calibrates to almost
//!   nothing and every rustle becomes an utterance.
//! - Onset needs several **consecutive** frames over the gate. One frame is not
//!   enough on this mic: a single transient would open an utterance made
//!   entirely of a click.
//!
//! The ALC245 in this machine runs about 40 dB hot, which is why a fixed
//! threshold cannot work here in either direction.

/// 16 kHz mono s16 — what whisper.cpp expects, and what `arecord -t raw` is
/// asked for. A mismatch does not fail, it transcribes noise.
pub const RATE: u32 = 16_000;
/// 100 ms of audio per frame.
pub const FRAME_SAMPLES: usize = 1600;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VadConfig {
    /// Speech is this many times the calibrated noise floor.
    pub speech_factor: f64,
    /// Never trust a floor below this: a silent room would give a hair trigger.
    pub floor_min: f64,
    /// A gate above this is unreachable by normal speech, whatever calibration
    /// said. The backstop for the spike problem in the module header.
    pub gate_max: f64,
    /// Consecutive over-gate frames before this counts as speech starting.
    pub onset_frames: u32,
    /// Silence this long after speech ends the utterance.
    pub silence_end_s: f64,
    /// Hard cap, so a noisy room cannot produce an unbounded recording.
    pub max_utterance_s: f64,
    /// Percentile of calibration levels taken as the floor. Low, not median.
    pub calibrate_percentile: f64,
}

impl Default for VadConfig {
    /// The values `converse.py` arrived at on this machine. They are calibrated
    /// against one microphone in one room and are not a general-purpose VAD.
    fn default() -> Self {
        Self {
            speech_factor: 2.0,
            floor_min: 120.0,
            gate_max: 6000.0,
            onset_frames: 3,
            silence_end_s: 1.2,
            max_utterance_s: 30.0,
            calibrate_percentile: 0.20,
        }
    }
}

/// Root-mean-square level of one frame.
///
/// `f64` accumulation because a frame of near-full-scale `i16` squares sums past
/// `i32` in 1600 samples, and a wrapped total would read as near-silence — the
/// loudest possible input reported as the quietest.
pub fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / samples.len() as f64).sqrt()
}

/// The speech gate, from a sample of ambient frame levels.
///
/// Takes `&mut` because it sorts in place — the caller's buffer is scratch.
///
/// Returns `None` from an empty sample rather than a default: a calibration
/// that observed nothing has not established a floor, and inventing one here is
/// how a loop ends up with a gate nobody chose.
pub fn calibrate_gate(levels: &mut [f64], cfg: &VadConfig) -> Option<f64> {
    if levels.is_empty() {
        return None;
    }
    levels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((levels.len() as f64) * cfg.calibrate_percentile) as usize;
    let floor = levels[idx.min(levels.len() - 1)].max(cfg.floor_min);
    Some((floor * cfg.speech_factor).min(cfg.gate_max))
}

/// What the caller should do with the frame it just measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VadStep {
    /// No speech yet. Discard the frame.
    Waiting,
    /// Speech is in progress, including the onset frames retroactively — the
    /// caller must have been buffering them. Keep the frame.
    Speaking,
    /// The utterance just ended. Keep this frame and hand the buffer over.
    Ended,
}

/// Tracks one utterance across a stream of frame levels.
#[derive(Clone, Debug)]
pub struct Vad {
    cfg: VadConfig,
    gate: f64,
    pending: u32,
    started: bool,
    frames: u32,
    silent_frames: u32,
}

impl Vad {
    pub fn new(gate: f64, cfg: VadConfig) -> Self {
        Self {
            cfg,
            gate,
            pending: 0,
            started: false,
            frames: 0,
            silent_frames: 0,
        }
    }

    pub fn gate(&self) -> f64 {
        self.gate
    }

    /// How many frames of onset are buffered but not yet part of an utterance.
    /// The caller keeps those frames so an utterance includes its own beginning.
    pub fn pending_frames(&self) -> u32 {
        self.pending
    }

    /// Feed one frame's level.
    pub fn observe(&mut self, level: f64) -> VadStep {
        let loud = level > self.gate;

        if !self.started {
            if loud {
                self.pending += 1;
                if self.pending >= self.cfg.onset_frames {
                    self.started = true;
                    self.frames = self.pending;
                    self.pending = 0;
                    self.silent_frames = 0;
                    return VadStep::Speaking;
                }
            } else {
                // Not a run, so it was a transient. Discard it — this is the
                // guard that stops a single click becoming an utterance.
                self.pending = 0;
            }
            return VadStep::Waiting;
        }

        self.frames += 1;
        self.silent_frames = if loud { 0 } else { self.silent_frames + 1 };

        let frame_s = FRAME_SAMPLES as f64 / RATE as f64;
        let silent_for = self.silent_frames as f64 * frame_s;
        let spoken_for = self.frames as f64 * frame_s;

        if silent_for >= self.cfg.silence_end_s || spoken_for >= self.cfg.max_utterance_s {
            self.reset();
            VadStep::Ended
        } else {
            VadStep::Speaking
        }
    }

    /// Ready for the next utterance. Called automatically on `Ended`; exposed so
    /// a caller that abandons an utterance can get back to a known state.
    pub fn reset(&mut self) {
        self.pending = 0;
        self.started = false;
        self.frames = 0;
        self.silent_frames = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> VadConfig {
        VadConfig::default()
    }

    /// One frame per 100 ms, so the durations below are in tenths of a second.
    fn frames_for(seconds: f64) -> usize {
        (seconds * RATE as f64 / FRAME_SAMPLES as f64).ceil() as usize
    }

    #[test]
    fn rms_of_silence_is_zero_and_of_a_constant_is_its_magnitude() {
        assert_eq!(rms(&[0; 100]), 0.0);
        assert!((rms(&[1000; 100]) - 1000.0).abs() < 1e-9);
        assert_eq!(rms(&[]), 0.0);
    }

    /// A frame of near-full-scale samples sums past i32 in 1600 samples. If the
    /// accumulator ever narrows, the loudest possible input reads as the
    /// quietest and the gate never opens.
    #[test]
    fn rms_does_not_overflow_on_a_full_scale_frame() {
        let loud = vec![i16::MAX; FRAME_SAMPLES];
        let r = rms(&loud);
        assert!(
            (r - 32767.0).abs() < 1.0,
            "full-scale frame measured {r}, not ~32767 — the accumulator wrapped"
        );
    }

    /// The measured failure this module exists around: spikes to full scale in
    /// an otherwise quiet room. A median catches them and puts the gate out of
    /// reach; a low percentile does not.
    #[test]
    fn calibration_ignores_transient_spikes_a_median_would_catch() {
        // Spike-dominated, which is the case converse.py hit over a SHORT
        // calibration window: at 1 s the spikes owned the median and the
        // reported floor was 6262. Three fifths spiky reproduces that here
        // without needing a real microphone.
        let quiet = 1801.0;
        let mut levels: Vec<f64> = (0..30)
            .map(|i| if i % 5 < 3 { 32768.0 } else { quiet })
            .collect();

        let mut sorted = levels.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            sorted[sorted.len() / 2],
            32768.0,
            "precondition: the spikes must own the median, or this proves nothing"
        );

        let gate = calibrate_gate(&mut levels, &cfg()).expect("calibrates");

        // The percentile took the quiet floor, not the spikes.
        assert_eq!(gate, quiet * cfg().speech_factor);
        // And the result stays reachable by ordinary speech. A median-derived
        // floor of 32768 would have doubled to 65536 and pinned the gate at the
        // backstop, which is the "listened and never answered" failure.
        assert!(
            gate < cfg().gate_max,
            "gate {gate} reached the backstop and would be unreachable"
        );
    }

    /// A room quiet enough to calibrate near zero must not produce a hair
    /// trigger where every rustle is an utterance.
    #[test]
    fn a_silent_room_is_floored_rather_than_trusted() {
        let mut levels = vec![0.5; 30];
        let gate = calibrate_gate(&mut levels, &cfg()).unwrap();
        assert_eq!(gate, cfg().floor_min * cfg().speech_factor);
    }

    /// A calibration that observed nothing has not established a floor. It says
    /// so rather than returning a number nobody chose.
    #[test]
    fn calibrating_on_nothing_refuses_rather_than_defaulting() {
        assert_eq!(calibrate_gate(&mut [], &cfg()), None);
    }

    /// The other measured failure: one full-scale transient must not open an
    /// utterance made entirely of a click.
    #[test]
    fn a_single_spike_does_not_start_an_utterance() {
        let mut v = Vad::new(1000.0, cfg());
        assert_eq!(v.observe(32768.0), VadStep::Waiting);
        assert_eq!(v.observe(10.0), VadStep::Waiting);
        // And the run counter reset, so spikes do not accumulate across silence.
        assert_eq!(v.pending_frames(), 0);
        assert_eq!(v.observe(32768.0), VadStep::Waiting);
        assert_eq!(v.observe(32768.0), VadStep::Waiting);
        assert_eq!(v.observe(10.0), VadStep::Waiting);
        assert_eq!(v.pending_frames(), 0, "two spikes short of onset persisted");
    }

    #[test]
    fn onset_needs_consecutive_frames_over_the_gate() {
        let mut v = Vad::new(1000.0, cfg());
        for _ in 0..cfg().onset_frames - 1 {
            assert_eq!(v.observe(5000.0), VadStep::Waiting);
        }
        assert_eq!(v.observe(5000.0), VadStep::Speaking);
    }

    #[test]
    fn silence_after_speech_ends_the_utterance() {
        let mut v = Vad::new(1000.0, cfg());
        for _ in 0..cfg().onset_frames {
            v.observe(5000.0);
        }
        for _ in 0..10 {
            assert_eq!(v.observe(5000.0), VadStep::Speaking);
        }
        // A pause shorter than the threshold does not end it — people breathe.
        let brief = frames_for(cfg().silence_end_s) - 1;
        for _ in 0..brief {
            assert_eq!(v.observe(10.0), VadStep::Speaking);
        }
        assert_eq!(v.observe(10.0), VadStep::Ended);
    }

    /// Ending must leave it ready for the next utterance, or a session gets
    /// exactly one turn.
    #[test]
    fn a_second_utterance_can_follow_the_first() {
        let mut v = Vad::new(1000.0, cfg());
        for _ in 0..cfg().onset_frames {
            v.observe(5000.0);
        }
        for _ in 0..frames_for(cfg().silence_end_s) {
            v.observe(10.0);
        }
        for _ in 0..cfg().onset_frames - 1 {
            assert_eq!(v.observe(5000.0), VadStep::Waiting);
        }
        assert_eq!(v.observe(5000.0), VadStep::Speaking);
    }

    /// A noisy room must not produce an unbounded recording.
    #[test]
    fn an_utterance_is_capped_even_if_the_speaker_never_stops() {
        let mut v = Vad::new(1000.0, cfg());
        let cap = frames_for(cfg().max_utterance_s) + cfg().onset_frames as usize + 10;
        let mut ended = false;
        for _ in 0..cap {
            if v.observe(5000.0) == VadStep::Ended {
                ended = true;
                break;
            }
        }
        assert!(ended, "a continuous speaker was never cut off");
    }

    /// Exactly at the gate is not over it. Pinned so flipping `>` to `>=` fails
    /// here rather than shifting every session's sensitivity silently.
    #[test]
    fn the_gate_is_strict() {
        let mut v = Vad::new(1000.0, cfg());
        for _ in 0..cfg().onset_frames * 2 {
            assert_eq!(v.observe(1000.0), VadStep::Waiting);
        }
    }
}
