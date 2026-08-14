//! Argument vectors for the external tools the shell spawns.
//!
//! **Pure on purpose**, for the same reason as [`crate::http`]: a wrong flag is
//! silent. `whisper-cli` without `-nt` emits `[00:00:00.000 --> …]` timestamps
//! that would be fed to the model as if they were speech, and without `-np` it
//! prints a progress banner into the same stream. Both produce a loop that
//! *runs* and transcribes garbage.
//!
//! Shapes taken from `tools/spoken-loop/config.example.json` and `turn.sh`,
//! which is the working reference on this machine. Only `Command::spawn` lives
//! in the binary.

use std::path::Path;

/// Capture one utterance to `out` as 16 kHz mono s16 — the format whisper.cpp
/// expects. A mismatch here does not fail; it transcribes noise.
pub fn record_argv(out: &Path) -> Vec<String> {
    vec![
        "pw-record".into(),
        "--rate".into(),
        "16000".into(),
        "--channels".into(),
        "1".into(),
        "--format".into(),
        "s16".into(),
        out.display().to_string(),
    ]
}

/// Transcribe `wav` with `whisper-cli`.
///
/// `-nt` (no timestamps) and `-np` (no progress) are load-bearing: without them
/// the stdout this loop parses as a transcript also contains timestamp brackets
/// and a progress banner, and both would be sent to the model as speech.
pub fn whisper_argv(binary: &Path, model: &Path, wav: &Path) -> Vec<String> {
    vec![
        binary.display().to_string(),
        "-m".into(),
        model.display().to_string(),
        "-f".into(),
        wav.display().to_string(),
        "-nt".into(),
        "-np".into(),
    ]
}

/// Synthesize `text` to `out` with espeak-ng.
///
/// Prosody is applied here rather than in the lib: `-s` is words per minute and
/// `-p` is a 0–99 pitch, so the normalized [`crate::seams::Prosody`] values have
/// to be mapped, and that mapping is engine-specific.
pub fn espeak_argv(
    text: &str,
    out: &Path,
    rate: Option<f32>,
    pitch: Option<f32>,
    volume: Option<f32>,
) -> Vec<String> {
    let mut argv = vec!["espeak-ng".to_string()];
    if let Some(r) = rate {
        argv.push("-s".into());
        argv.push(espeak_words_per_minute(r).to_string());
    }
    if let Some(p) = pitch {
        argv.push("-p".into());
        argv.push(espeak_pitch(p).to_string());
    }
    if let Some(v) = volume {
        argv.push("-a".into());
        argv.push(espeak_amplitude(v).to_string());
    }
    argv.push("-w".into());
    argv.push(out.display().to_string());
    argv.push(text.to_string());
    argv
}

/// Maps the Swift/AVSpeech-style rate (roughly 0.0–1.0, 0.5 ≈ natural) onto
/// espeak-ng's words per minute, clamped to a range that stays intelligible.
///
/// The hypnagogic voices sit at 0.35–0.42 and the waking ones at 0.50–0.54, so
/// the useful span is narrow; 175 wpm is espeak's own default and anchors 0.5.
pub fn espeak_words_per_minute(rate: f32) -> u32 {
    let wpm = 175.0 * (rate.clamp(0.0, 1.0) / 0.5);
    wpm.clamp(80.0, 400.0) as u32
}

/// Maps a pitch multiplier (1.0 = unchanged) onto espeak-ng's 0–99 scale, where
/// 50 is its default.
pub fn espeak_pitch(multiplier: f32) -> u32 {
    let p = 50.0 * multiplier.clamp(0.0, 2.0);
    p.clamp(0.0, 99.0) as u32
}

/// `Prosody::volume` (0.0-1.0, the Swift/AVSpeech scale) to espeak-ng's `-a`
/// amplitude, whose default is 100 and whose maximum is 200.
///
/// 1.0 maps to 100, not 200: the Swift's 1.0 means "full normal volume", and
/// sending 200 would make every utterance louder than the engine's own
/// default rather than matching it. The hypnagogic voices sit at 0.6-0.8, and
/// the whole point of the dimension is that a receding voice is quieter than a
/// present one — an offset that doubled everything would flatten that.
pub fn espeak_amplitude(volume: f32) -> u32 {
    let a = 100.0 * volume.clamp(0.0, 2.0);
    a.clamp(0.0, 200.0) as u32
}

/// Play a rendered wav.
pub fn play_argv(wav: &Path) -> Vec<String> {
    vec!["pw-play".into(), wav.display().to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// The two flags whose absence produces a loop that runs and transcribes
    /// garbage rather than one that fails.
    #[test]
    fn whisper_suppresses_timestamps_and_progress() {
        let argv = whisper_argv(&p("/w/whisper-cli"), &p("/w/base.en.bin"), &p("/tmp/t.wav"));
        assert!(
            argv.contains(&"-nt".to_string()),
            "timestamps would be transcribed as speech"
        );
        assert!(
            argv.contains(&"-np".to_string()),
            "the progress banner would be transcribed as speech"
        );
        assert_eq!(argv[0], "/w/whisper-cli");
        // -m and -f must precede their values, or whisper reads them as flags.
        let m = argv.iter().position(|a| a == "-m").unwrap();
        assert_eq!(argv[m + 1], "/w/base.en.bin");
        let f = argv.iter().position(|a| a == "-f").unwrap();
        assert_eq!(argv[f + 1], "/tmp/t.wav");
    }

    /// whisper.cpp expects 16 kHz mono. Any other rate transcribes noise
    /// without erroring.
    #[test]
    fn recording_is_16k_mono_s16() {
        let argv = record_argv(&p("/tmp/in.wav"));
        for (flag, value) in [
            ("--rate", "16000"),
            ("--channels", "1"),
            ("--format", "s16"),
        ] {
            let i = argv.iter().position(|a| a == flag).expect(flag);
            assert_eq!(argv[i + 1], value);
        }
        assert_eq!(argv.last().unwrap(), "/tmp/in.wav");
    }

    /// The text is the LAST argument and is never concatenated into a shell
    /// string — a reply containing a quote or a semicolon must not be able to
    /// become a command.
    #[test]
    fn spoken_text_is_a_single_argument_not_a_shell_fragment() {
        let nasty = "; rm -rf ~ && echo \"pwned\" `id`";
        let argv = espeak_argv(nasty, &p("/tmp/o.wav"), None, None, None);
        assert_eq!(argv.last().unwrap(), nasty);
        assert_eq!(argv.iter().filter(|a| a.contains("rm -rf")).count(), 1);
    }

    #[test]
    fn prosody_maps_onto_espeaks_own_scales() {
        // 0.5 is the natural anchor and must land on espeak's default 175 wpm.
        assert_eq!(espeak_words_per_minute(0.5), 175);
        // The hypnagogic envelope must come out audibly slower than waking.
        assert!(espeak_words_per_minute(0.35) < espeak_words_per_minute(0.5));
        assert!(espeak_words_per_minute(0.54) > espeak_words_per_minute(0.5));
        // A multiplier of 1.0 is espeak's default pitch.
        assert_eq!(espeak_pitch(1.0), 50);
        assert!(espeak_pitch(0.8) < 50);
    }

    /// Out-of-range values must clamp into an intelligible range rather than
    /// producing a silent or unusable utterance.
    #[test]
    fn absurd_prosody_clamps_rather_than_breaking_speech() {
        assert_eq!(espeak_words_per_minute(0.0), 80);
        assert_eq!(espeak_words_per_minute(5.0), 350);
        assert_eq!(espeak_pitch(0.0), 0);
        assert_eq!(espeak_pitch(9.0), 99);
    }

    /// Absent prosody must omit the flags entirely — `None` means "let the
    /// engine choose", and emitting a computed default would silently decide.
    #[test]
    fn absent_prosody_omits_the_flags() {
        let argv = espeak_argv("hi", &p("/tmp/o.wav"), None, None, None);
        assert!(!argv.contains(&"-s".to_string()));
        assert!(!argv.contains(&"-p".to_string()));
        assert!(!argv.contains(&"-a".to_string()));
    }

    /// Volume is a real dimension of these voices — a receding pole is quieter
    /// than a present one — and it reached no engine at all until now.
    #[test]
    fn volume_maps_onto_espeaks_amplitude_with_1_0_as_its_default() {
        assert_eq!(
            espeak_amplitude(1.0),
            100,
            "1.0 must be espeak's own default"
        );
        assert!(
            espeak_amplitude(0.6) < 100,
            "a receding voice must be quieter"
        );
        assert!(espeak_amplitude(0.8) > espeak_amplitude(0.6));
        // Clamped rather than clipped.
        assert_eq!(espeak_amplitude(0.0), 0);
        assert_eq!(espeak_amplitude(9.0), 200);
        assert_eq!(espeak_amplitude(-1.0), 0);
    }

    /// The flag has to actually reach the argv, not merely have a mapping
    /// function. The bug this catches is precisely the one that existed: a
    /// prosody dimension with a sound mapping that nothing ever called.
    #[test]
    fn a_supplied_volume_reaches_the_argv() {
        let argv = espeak_argv("hi", &p("/tmp/o.wav"), None, None, Some(0.6));
        let i = argv
            .iter()
            .position(|a| a == "-a")
            .expect("volume never reached the command line");
        assert_eq!(argv[i + 1], espeak_amplitude(0.6).to_string());
    }

    /// Every prosody dimension espeak can express must be expressible together
    /// — and the text must still be last, where the injection test needs it.
    #[test]
    fn all_three_espeak_dimensions_can_be_set_at_once() {
        let argv = espeak_argv(
            "drifting",
            &p("/tmp/o.wav"),
            Some(0.35),
            Some(0.8),
            Some(0.7),
        );
        for flag in ["-s", "-p", "-a", "-w"] {
            assert!(argv.contains(&flag.to_string()), "{flag} missing: {argv:?}");
        }
        assert_eq!(argv.last().unwrap(), "drifting");
    }
}
