//! The shell: every effect this crate has, and nothing else.
//!
//! The lib decides what a turn *means*; this file is the only place a socket is
//! opened, a process is spawned, a file is written or a clock is read. That
//! split is what lets CI exercise the whole dialectic with no model, no
//! microphone and no server — and it is also why **a green `cargo test` says
//! nothing about whether this file works.** The suite stays green whether or
//! not a single spoken turn completes. The parts of the shell that *can* be
//! wrong silently — the request body, the response key, the argv flags, the
//! text mangling — deliberately live in `http.rs`, `command.rs` and `loops.rs`
//! where they are pure and tested. What is left here is spawn, POST, read, write.
//!
//! Verification for this file is the end-to-end run, not the test suite.

use neuralcompose_hypnagogic::claude_cli;
use neuralcompose_hypnagogic::command;
use neuralcompose_hypnagogic::dialectic::{DialecticConfig, DialecticLoop};
use neuralcompose_hypnagogic::eeg::{
    eeg_reading_for_turn, EegRecordContext, EegRefusal, EegTurnReading, SAMPLE_RATE_HZ,
};
use neuralcompose_hypnagogic::eligibility::{evaluate, tally, Registration};
use neuralcompose_hypnagogic::embedding::Embedding;
use neuralcompose_hypnagogic::http;
use neuralcompose_hypnagogic::loops::{
    is_non_speech, is_stop_phrase, strip_for_speech, MirrorConfig, MirrorLoop, STOP_PHRASES,
};
use neuralcompose_hypnagogic::profile::HypnagogicMode;
use neuralcompose_hypnagogic::role::waking_roles;
use neuralcompose_hypnagogic::seams::{
    GenerationParams, Listening, Prosody, SeamError, SeamResult, SelectionDraws, SentenceEmbedding,
    Speaking, TextGenerating,
};
use neuralcompose_hypnagogic::session::{
    claim_envelope, host_envelope, ClaimedSource, HciAdapter, PowerState, SessionRecord,
    SESSION_RECORD_SCHEMA,
};
use neuralcompose_hypnagogic::turn_log::{
    eeg_method_identity, turn_log_manifest_filename, turn_log_payload_filename, TurnLine,
    TurnLogRecorder,
};
use neuralcompose_hypnagogic::vad;
use neuralcompose_mobile_core::capture::{
    verify_capture, BridgeLocality, CaptureBuildIdentity, CaptureManifest, CaptureRecorder,
    ReplayVerdict,
};
use neuralcompose_mobile_core::channel_health::ChannelHealthThresholds;
use neuralcompose_mobile_core::electrode_check::MainsThresholds;
use neuralcompose_mobile_core::provenance::MethodIdentity;
use neuralcompose_mobile_core::stream::{MonitorConfig, SocketEvent, StreamMonitor};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ─────────────────────────────────────────────────────────────────── args ──

struct Args {
    mode: HypnagogicMode,
    turns: u32,
    server: String,
    whisper: PathBuf,
    whisper_model: PathBuf,
    log: bool,
    log_dir: PathBuf,
    json: bool,
    verify_log: Option<PathBuf>,
    mic: bool,
    push_to_talk: bool,
    voice_both: bool,
    mic_gate: Option<f64>,
    /// Override `DialecticConfig::drift_ceiling`. `0.0` disables re-anchoring.
    drift_ceiling: Option<f32>,
    /// Override `DialecticConfig::repetition_floor`. `0.0` disables the guard.
    repetition_floor: Option<f32>,
    speak: bool,
    tts: String,
    eeg_url: Option<String>,
    /// Operator's claim about what is on the other end, e.g.
    /// `muse-s-board-39`. Recorded as an `externalClaim`, never as an
    /// observation — this process cannot see the board.
    eeg_source: Option<String>,
    eeg_preset: Option<String>,
    eligibility: Option<PathBuf>,
    verify_capture: Option<PathBuf>,
    world_model_demo: bool,
    heldout: bool,
    /// `llama` (default, local) or `claude` (opt-in, off-device). Not a bool,
    /// because a third generator is plausible and `--no-local` would be a worse
    /// name for the same choice.
    generator: String,
    claude_model: String,
}

const USAGE: &str = "\
neuralcompose-hypnagogic — the four hypnagogic loop modes on Linux

  --mode <mirror|focused|reflective|contemplative>   default: mirror
  --turns <n>                                        default: 1; 0 = until you say stop
  --server <url>                                     default: http://127.0.0.1:8080
  --generator <llama|claude>                         default: llama (local)
                                  claude sends the prompt OFF THIS MACHINE via
                                  the `claude` CLI. Opt-in, never a default.
  --claude-model <id>                                default: claude-sonnet-5
  --whisper <path>                --whisper-model <path>
  --log                           --log-dir <path>
  --mic                           hands-free microphone: it cuts on silence,
                                  so you never press a key
  --push-to-talk                  the old --mic: speak, then press Enter
  --voice-both                    speak BOTH poles each turn, in their own
                                  voices, so the dialectic is audible
  --mic-gate <n>                  skip calibration and use this speech gate
  --drift-ceiling <n>             re-anchor the poles' prompts past this
                                  distance from the opening utterance; 0
                                  disables. EMBEDDER-SPECIFIC: the default is
                                  measured against bge-small, so change it if
                                  you change NC_EMBED_MODEL
  --repetition-floor <n>          force a silent turn when the replies stop
                                  moving; 0 disables
  --speak                         synthesize audio instead of printing
  --tts <kokoro|espeak>           voice engine for --speak (default kokoro)
  --eeg-url <ws://…>              attach an EEG source (dialectical modes only)
  --eeg-source <id>               what you believe is attached, e.g. muse-s-board-39
  --eeg-preset <name>             BrainFlow preset, if you set one
                                  (both are recorded as YOUR claim, not a reading)
  --world-model-demo              run the planner comparison and exit
  --heldout                       with it: the §8 held-out set and seeds
  --json                          --verify-log <path>
  --eligibility <turns.jsonl>     query a recorded session against the sealed
                                  pre-registration in contracts/eeg/
  --verify-capture <eeg.jsonl>    replay a raw capture against its manifest

Standalone modes run BEFORE the loop and exit; when more than one is given the
precedence is --verify-log, --verify-capture, --eligibility, --world-model-demo.
None of them needs llama-server, a model, a microphone or a headband.
";

fn parse_args() -> Result<Args, String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mut a = Args {
        mode: HypnagogicMode::Mirror,
        turns: 1,
        server: "http://127.0.0.1:8080".into(),
        whisper: PathBuf::from(format!("{home}/src/whisper.cpp/build/bin/whisper-cli")),
        whisper_model: PathBuf::from(format!("{home}/src/whisper.cpp/models/ggml-base.en.bin")),
        log: false,
        log_dir: PathBuf::from(format!("{home}/Documents/NeuralCompose/InteractionLogs")),
        json: false,
        verify_log: None,
        mic: false,
        push_to_talk: false,
        voice_both: false,
        mic_gate: None,
        drift_ceiling: None,
        repetition_floor: None,
        speak: false,
        tts: "kokoro".into(),
        eeg_url: None,
        eeg_source: None,
        eeg_preset: None,
        eligibility: None,
        verify_capture: None,
        world_model_demo: false,
        heldout: false,
        generator: "llama".into(),
        claude_model: claude_cli::DEFAULT_MODEL.into(),
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let need = |i: usize| -> Result<String, String> {
            argv.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{} needs a value", argv[i]))
        };
        match argv[i].as_str() {
            "--mode" => {
                let v = need(i)?;
                a.mode = HypnagogicMode::parse(&v)
                    .ok_or_else(|| format!("unknown mode {v:?}; expected one of mirror, focused, reflective, contemplative"))?;
                i += 1;
            }
            "--turns" => {
                a.turns = need(i)?
                    .parse()
                    .map_err(|_| "--turns needs a number".to_string())?;
                i += 1;
            }
            "--server" => {
                a.server = need(i)?;
                i += 1;
            }
            "--whisper" => {
                a.whisper = PathBuf::from(need(i)?);
                i += 1;
            }
            "--whisper-model" => {
                a.whisper_model = PathBuf::from(need(i)?);
                i += 1;
            }
            "--log-dir" => {
                a.log_dir = PathBuf::from(need(i)?);
                i += 1;
            }
            "--verify-log" => {
                a.verify_log = Some(PathBuf::from(need(i)?));
                i += 1;
            }
            "--eeg-url" => {
                a.eeg_url = Some(need(i)?);
                i += 1;
            }
            "--eeg-source" => {
                a.eeg_source = Some(need(i)?);
                i += 1;
            }
            "--eeg-preset" => {
                a.eeg_preset = Some(need(i)?);
                i += 1;
            }
            "--eligibility" => {
                a.eligibility = Some(PathBuf::from(need(i)?));
                i += 1;
            }
            "--verify-capture" => {
                a.verify_capture = Some(PathBuf::from(need(i)?));
                i += 1;
            }
            "--world-model-demo" => a.world_model_demo = true,
            "--heldout" => a.heldout = true,
            // The ceiling is a property of the EMBEDDER, not of the loop — the
            // default is measured against bge-small and is wrong for any other
            // model, and nothing here can derive one from the other. An
            // operator who changes NC_EMBED_MODEL has to change this with it.
            "--drift-ceiling" => {
                a.drift_ceiling = Some(
                    need(i)?
                        .parse()
                        .map_err(|_| "--drift-ceiling needs a number".to_string())?,
                );
                i += 1;
            }
            "--repetition-floor" => {
                a.repetition_floor = Some(
                    need(i)?
                        .parse()
                        .map_err(|_| "--repetition-floor needs a number".to_string())?,
                );
                i += 1;
            }
            "--mic" => a.mic = true,
            "--voice-both" => a.voice_both = true,
            "--push-to-talk" => {
                a.mic = true;
                a.push_to_talk = true;
            }
            "--mic-gate" => {
                let v = need(i)?;
                a.mic_gate = Some(
                    v.parse::<f64>()
                        .map_err(|_| format!("--mic-gate wants a number, got {v:?}"))?,
                );
                a.mic = true;
                i += 1;
            }
            "--speak" => a.speak = true,
            "--tts" => {
                let v = need(i)?;
                if !matches!(v.as_str(), "kokoro" | "espeak") {
                    return Err(format!("unknown --tts {v:?}; expected kokoro or espeak"));
                }
                a.tts = v;
                i += 1;
            }
            "--generator" => {
                let v = need(i)?;
                if !matches!(v.as_str(), "llama" | "claude") {
                    return Err(format!(
                        "unknown --generator {v:?}; expected llama (local) or claude (off-device)"
                    ));
                }
                a.generator = v;
                i += 1;
            }
            "--claude-model" => {
                a.claude_model = need(i)?;
                i += 1;
            }
            "--log" => a.log = true,
            "--json" => a.json = true,
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown flag {other:?}\n\n{USAGE}")),
        }
        i += 1;
    }
    Ok(a)
}

// ─────────────────────────────────────────────────────────────── the shell ──

/// Hands-free capture: the mic stays open and utterances are cut on silence.
///
/// **This is what `--mic` does now.** It used to print "speak now, then press
/// Enter" and block on stdin, which for a hypnagogic session — taken lying down
/// with your eyes closed — defeats the exercise. [`PushToTalkListener`] is still
/// there behind `--push-to-talk` for when a keypress is genuinely wanted.
///
/// `arecord -t raw` rather than `pw-record`, for the reason `converse.py` gives:
/// `pw-record` writes a WAV header to stdout and this needs a bare sample
/// stream. It still reaches the hardware through PipeWire's ALSA compatibility
/// layer, so it is the same audio path.
///
/// The decision of when speech starts and stops is in [`vad`], with the measured
/// rationale for every constant. This type is the microphone and the buffer.
struct VadListener {
    whisper: PathBuf,
    model: PathBuf,
    workdir: PathBuf,
    /// `None` until the first `listen()`, which calibrates against the room.
    /// Calibration happens once: the noise floor is a property of the room and
    /// re-measuring it between turns would recalibrate against the tail of
    /// whatever was just said.
    gate: Option<f64>,
    cfg: vad::VadConfig,
    /// Overrides calibration entirely. For when the room defeats it and the
    /// operator has a number that works — `converse.py` grew the same escape
    /// hatch for the same reason.
    forced_gate: Option<f64>,
}

impl VadListener {
    /// Reads exactly one frame, or `None` at end of stream.
    fn read_frame(stdout: &mut std::process::ChildStdout, buf: &mut [i16]) -> Option<()> {
        let mut bytes = vec![0u8; buf.len() * 2];
        let mut filled = 0;
        while filled < bytes.len() {
            match stdout.read(&mut bytes[filled..]) {
                Ok(0) => return None,
                Ok(n) => filled += n,
                Err(_) => return None,
            }
        }
        for (i, s) in buf.iter_mut().enumerate() {
            *s = i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
        }
        Some(())
    }
}

impl Listening for VadListener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        let argv = command::arecord_argv();
        let mut rec = Command::new(&argv[0])
            .args(&argv[1..])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| SeamError::Unavailable(format!("arecord: {e}")))?;
        let mut out = rec
            .stdout
            .take()
            .ok_or_else(|| SeamError::Unavailable("arecord gave no stdout".into()))?;

        let mut frame = vec![0i16; vad::FRAME_SAMPLES];

        // Calibrate once, on the first turn.
        if self.gate.is_none() {
            if let Some(g) = self.forced_gate {
                eprintln!("● mic: gate {g:.0} (set by hand, no calibration)");
                self.gate = Some(g);
            } else {
                eprintln!("● mic: calibrating the room, stay quiet…");
                let n = (3.0 * vad::RATE as f64 / vad::FRAME_SAMPLES as f64) as usize;
                let mut levels = Vec::with_capacity(n);
                for _ in 0..n {
                    if Self::read_frame(&mut out, &mut frame).is_none() {
                        break;
                    }
                    levels.push(vad::rms(&frame));
                }
                match vad::calibrate_gate(&mut levels, &self.cfg) {
                    Some(g) => {
                        eprintln!("● mic: gate {g:.0} — speak whenever you like");
                        self.gate = Some(g);
                    }
                    // Refused rather than defaulted: a gate nobody chose is how
                    // a loop ends up listening forever or triggering on nothing.
                    None => {
                        let _ = rec.kill();
                        return Err(SeamError::Unavailable(
                            "the microphone produced no audio to calibrate against. \
                             Check an input device exists and is not muted, or pass \
                             --mic-gate with a number."
                                .into(),
                        ));
                    }
                }
            }
        }

        let gate = self.gate.expect("set above");
        let mut v = vad::Vad::new(gate, self.cfg);
        let mut pcm: Vec<i16> = Vec::new();
        let mut ring: Vec<Vec<i16>> = Vec::new();

        loop {
            if Self::read_frame(&mut out, &mut frame).is_none() {
                let _ = rec.kill();
                let _ = rec.wait();
                return Ok(None);
            }
            match v.observe(vad::rms(&frame)) {
                vad::VadStep::Waiting => {
                    // Hold the last few frames so an utterance includes its own
                    // onset. Without this every turn loses its first syllable,
                    // which whisper then guesses at.
                    ring.push(frame.clone());
                    let keep = self.cfg.onset_frames as usize;
                    if ring.len() > keep {
                        ring.remove(0);
                    }
                }
                vad::VadStep::Speaking => {
                    if pcm.is_empty() {
                        for f in ring.drain(..) {
                            pcm.extend_from_slice(&f);
                        }
                    }
                    pcm.extend_from_slice(&frame);
                }
                vad::VadStep::Ended => {
                    pcm.extend_from_slice(&frame);
                    break;
                }
            }
        }

        let _ = rec.kill();
        let _ = rec.wait();

        let wav = self.workdir.join("turn.wav");
        write_wav(&wav, &pcm).map_err(|e| SeamError::Failed(format!("writing {wav:?}: {e}")))?;
        transcribe(&self.whisper, &self.model, &wav)
    }
}

/// Minimal 16-bit mono PCM WAV. Written by hand rather than adding a crate for
/// 44 bytes of header — the whole workspace has seven dependencies.
fn write_wav(path: &Path, pcm: &[i16]) -> std::io::Result<()> {
    let data_len = (pcm.len() * 2) as u32;
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // PCM header size
    f.write_all(&1u16.to_le_bytes())?; // format: PCM
    f.write_all(&1u16.to_le_bytes())?; // channels
    f.write_all(&vad::RATE.to_le_bytes())?;
    f.write_all(&(vad::RATE * 2).to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits per sample
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in pcm {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

/// Shared by both microphone listeners.
fn transcribe(whisper: &Path, model: &Path, wav: &Path) -> SeamResult<Option<String>> {
    let argv = command::whisper_argv(whisper, model, wav);
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .map_err(|e| SeamError::Unavailable(format!("whisper-cli: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let head: Vec<&str> = stderr
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect();
        return Err(SeamError::Failed(format!(
            "whisper-cli exited {}: {}",
            out.status,
            head.join(" / ")
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // Both listeners route through here, so the non-speech guard sits here too
    // rather than in each of them. `Ok(None)` is already the "only silence was
    // heard" contract, and both loops answer it with a silence cue and no model
    // call — which is exactly the right response to a fan.
    Ok(if text.is_empty() || is_non_speech(&text) {
        None
    } else {
        Some(text)
    })
}

/// Spawns `pw-record`, waits for Enter, then transcribes with whisper-cli.
///
/// Behind `--push-to-talk` since `--mic` became hands-free. Kept because a
/// keypress is deterministic and a voice gate is not, which matters when you are
/// demonstrating something rather than using it.
struct PushToTalkListener {
    whisper: PathBuf,
    model: PathBuf,
    workdir: PathBuf,
}

impl Listening for PushToTalkListener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        let wav = self.workdir.join("turn.wav");
        eprintln!("● speak now, then press Enter");
        let argv = command::record_argv(&wav);
        let mut rec = Command::new(&argv[0])
            .args(&argv[1..])
            .spawn()
            .map_err(|e| SeamError::Unavailable(format!("pw-record: {e}")))?;
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        let _ = rec.kill();
        let _ = rec.wait();

        // Check the recording exists BEFORE handing it to whisper.
        //
        // Without this, a capture that produced nothing — no audio source, a
        // denied device, `pw-record` dying on startup, or a turn ended before
        // the first buffer was flushed — reaches whisper-cli as a missing file
        // and comes back as sixty lines of usage text. That is a real failure
        // reported as a help screen, and it buries the one sentence that says
        // what went wrong.
        //
        // `tools/spoken-loop/shell.py` already refuses a zero-byte WAV at the
        // same point; this is the Rust path catching up to it.
        match std::fs::metadata(&wav) {
            Err(e) => {
                return Err(SeamError::Failed(format!(
                    "no recording at {}: {e}. pw-record produced nothing — check that \
                     an input device exists and is not muted.",
                    wav.display()
                )))
            }
            // A WAV header with no frames is ~44 bytes and transcribes to
            // silence, which is indistinguishable from a turn the user chose
            // not to speak. Treated as silence rather than an error: the mirror
            // loop already has a meaning for "nothing was heard".
            Ok(m) if m.len() <= 44 => {
                eprintln!("● nothing was recorded (empty capture)");
                return Ok(None);
            }
            Ok(_) => {}
        }

        // Shared with the VAD listener, including the stderr trimming: whisper
        // prints its whole usage screen on a bad invocation, and sixty lines of
        // option documentation hides the one line that says what failed.
        transcribe(&self.whisper, &self.model, &wav)
    }
}

/// A listener that reads typed lines instead of the microphone. The default,
/// because it makes every mode runnable with no audio stack at all.
struct StdinListener;

impl Listening for StdinListener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        eprint!("you: ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => {
                let t = line.trim().to_string();
                Ok(if t.is_empty() { None } else { Some(t) })
            }
            Err(e) => Err(SeamError::Failed(format!("stdin: {e}"))),
        }
    }
}

struct HttpGenerator {
    server: String,
}

impl TextGenerating for HttpGenerator {
    fn generate(
        &mut self,
        system: &str,
        prompt: &str,
        params: GenerationParams,
    ) -> SeamResult<String> {
        let body = http::chat_request_body(system, prompt, params);
        let resp = ureq::post(&http::chat_url(&self.server))
            .set("content-type", "application/json")
            .send_string(&body)
            .map_err(|e| SeamError::Unavailable(format!("llama-server: {e}")))?;
        let raw = resp
            .into_string()
            .map_err(|e| SeamError::Failed(format!("reading the chat response: {e}")))?;
        Ok(strip_for_speech(&http::parse_chat_content(&raw)?))
    }
}

/// Sonnet 5 (or any Claude model) through the local `claude` CLI — the port of
/// `BCICloudBridge/ClaudeCLIGenerator.swift`. **Opt-in only**, behind
/// `--generator claude`; the local path stays the default and is untouched.
///
/// ⚠️ This is the one seam in this binary that sends anything off this machine.
/// The argv it sends is built and asserted in [`claude_cli`], which is where the
/// egress boundary is stated; here there is only the spawn.
///
/// `GenerationParams` is accepted and dropped, because `claude -p` exposes
/// neither temperature nor `max_tokens`. Through this generator the two poles
/// differ **only** by their system prompts, not by sampling — the shell says so
/// once at startup rather than leaving it to be discovered from the turn log.
struct ClaudeCliGenerator {
    model: String,
    /// The subprocess runs here rather than in the invoking directory, so a
    /// `CLAUDE.md` that happens to be beside the session cannot be discovered
    /// and folded into a hypnagogic prompt.
    workdir: PathBuf,
}

impl TextGenerating for ClaudeCliGenerator {
    fn generate(
        &mut self,
        system: &str,
        prompt: &str,
        _params: GenerationParams,
    ) -> SeamResult<String> {
        let argv = claude_cli::argv(&self.model, system, prompt);
        let out = Command::new("claude")
            .args(&argv)
            .current_dir(&self.workdir)
            // Discarded for the same reason the Swift discards it: a chatty CLI
            // writing into an unread pipe can fill the buffer and stall exit.
            .stderr(std::process::Stdio::null())
            .output()
            .map_err(|e| {
                SeamError::Unavailable(format!(
                    "could not run the `claude` CLI ({e}) — is it on PATH and signed in?"
                ))
            })?;
        if !out.status.success() {
            return Err(SeamError::Failed(format!(
                "the claude CLI exited {} (run `claude -p hello` to check your login)",
                out.status
            )));
        }
        let raw = String::from_utf8_lossy(&out.stdout);
        Ok(strip_for_speech(&claude_cli::parse_result(&raw)?))
    }
}

struct EspeakSpeaker {
    workdir: PathBuf,
}

/// Honours `Prosody::pre_utterance_delay` — the silence *before* an utterance.
///
/// This was dropped on the floor by every speaker until now. It is not a
/// decoration: `loops.rs` records that the contemplative voices carry over a
/// second of it, and the profile's whole character is pacing rather than
/// timbre. On a fixed-voice engine, where two neural voices have no midpoint
/// and `Prosody::blend` can only pick the heavier one,
/// **pacing is the channel tension is still audible through** — so silently
/// discarding it removed the one prosodic dimension Kokoro had left.
///
/// Applied in the speakers rather than in `play()`, because the delay belongs
/// before the utterance as a whole, not between synthesis and playback.
/// `PrintSpeaker` deliberately does not call this: a dry run that pauses for
/// real seconds while printing text buys nothing.
fn pre_utterance_pause(prosody: &Prosody) {
    if let Some(seconds) = prosody.pre_utterance_delay {
        // Clamped: a blended delay is a weighted mean of the poles' values and
        // cannot legitimately be huge, but a stray one must not hang a session.
        if seconds.is_finite() && seconds > 0.0 {
            std::thread::sleep(std::time::Duration::from_secs_f64(seconds.min(5.0)));
        }
    }
}

impl Speaking for EspeakSpeaker {
    fn speak(&mut self, text: &str, prosody: Prosody) -> SeamResult<()> {
        pre_utterance_pause(&prosody);
        let wav = self.workdir.join("reply.wav");
        let argv = command::espeak_argv(
            text,
            &wav,
            prosody.rate,
            prosody.pitch_multiplier,
            prosody.volume,
        );
        let out = Command::new(&argv[0])
            .args(&argv[1..])
            .output()
            .map_err(|e| SeamError::Unavailable(format!("espeak-ng: {e}")))?;
        if !out.status.success() {
            return Err(SeamError::Failed(format!(
                "espeak-ng exited {}",
                out.status
            )));
        }
        play(&wav)?;
        // With --speak there is otherwise no trace of a run at all. Echoing the
        // spoken text keeps a session auditable by someone who was not
        // listening, and makes "spoke nothing" distinguishable from "spoke".
        eprintln!("  ♪ {text}");
        Ok(())
    }
}

/// Kokoro-82M via `tools/spoken-loop/speak.py`.
///
/// **Subprocess, not a link.** The script imports onnxruntime; this binary does
/// not. That distinction is the whole point: client-native's
/// single-inference-runtime rule governs what the shipped binary LINKS, and
/// spawning a Python script adds nothing to its link graph, its memory
/// footprint or its load time. Nothing here promotes a support-matrix row and
/// nothing here is a precedent for what the product links — the runtime
/// decision stays deferred and unmade, exactly as speak.py's own header says.
///
/// Prosody maps differently from espeak, and imperfectly. The full mapping,
/// stated so an unmapped dimension is a recorded loss rather than a silent one:
///   - `rate` -> `KOKORO_SPEED`, rescaled so 0.5 (the natural anchor) is 1.0
///   - `voice` -> `KOKORO_VOICE`, a fixed named voice per role
///   - `volume` -> `KOKORO_VOLUME`, applied by `speak.py` as a sample scale,
///     since Kokoro itself has no volume parameter
///   - `pre_utterance_delay` -> honoured by [`pre_utterance_pause`] before the
///     subprocess runs. It used to map to nothing, which quietly removed the
///     contemplative profile's pacing on Linux — and on a fixed-voice engine
///     pacing is the dimension carrying tension, because the voices cannot
///     blend.
///   - `pitch_multiplier` -> **nothing**. Kokoro cannot pitch-shift, so that
///     dimension of the Swift's prosody is genuinely unavailable here. This is
///     the one remaining gap, and it is a property of the engine rather than
///     of this wiring.
struct KokoroSpeaker {
    script: PathBuf,
    python: PathBuf,
    workdir: PathBuf,
}

impl Speaking for KokoroSpeaker {
    fn speak(&mut self, text: &str, prosody: Prosody) -> SeamResult<()> {
        pre_utterance_pause(&prosody);
        let wav = self.workdir.join("reply.wav");
        let mut cmd = Command::new(&self.python);
        cmd.arg(&self.script).arg(&wav).arg(text);
        if let Some(v) = prosody.voice {
            cmd.env("KOKORO_VOICE", v);
        }
        if let Some(vol) = prosody.volume {
            // Kokoro has no volume parameter; speak.py scales the samples.
            // Clamped there as well as here, because the script is also run by
            // hand.
            cmd.env("KOKORO_VOLUME", format!("{:.2}", vol.clamp(0.0, 2.0)));
        }
        if let Some(r) = prosody.rate {
            // The Swift/AVSpeech scale puts "natural" at 0.5; Kokoro's speed
            // multiplier puts it at 1.0. Clamped so a stray value cannot make
            // an utterance unintelligible.
            cmd.env("KOKORO_SPEED", format!("{:.2}", (r / 0.5).clamp(0.5, 2.0)));
        }
        let out = cmd
            .output()
            .map_err(|e| SeamError::Unavailable(format!("kokoro speak.py: {e}")))?;
        if !out.status.success() {
            return Err(SeamError::Failed(format!(
                "speak.py exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        play(&wav)?;
        eprintln!("  ♪ [{}] {text}", prosody.voice.unwrap_or("engine default"));
        Ok(())
    }
}

/// Plays a rendered wav, checking the exit code — `.status()` yields `Ok` for a
/// process that ran and failed, so a dead sink would otherwise be silent in
/// both senses.
fn play(wav: &std::path::Path) -> SeamResult<()> {
    let argv = command::play_argv(wav);
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|e| SeamError::Unavailable(format!("pw-play: {e}")))?;
    if !status.success() {
        return Err(SeamError::Failed(format!("pw-play exited {status}")));
    }
    Ok(())
}

/// Prints instead of speaking — the default, so a run needs no audio stack.
struct PrintSpeaker;

impl Speaking for PrintSpeaker {
    fn speak(&mut self, text: &str, _p: Prosody) -> SeamResult<()> {
        println!("  {text}");
        Ok(())
    }
}

/// Embeddings over the **CPU** backend, and it says so on startup.
///
/// Not a fallback: `BACKEND_ID_CPU` is the `RuntimeSmokeValidated` row on
/// `linux/x86_64` and no Vulkan-embedder row is. It is also what keeps this
/// process's Vulkan context count at zero, so `llama-server` remains the only
/// holder of a context on the one iGPU. The startup line makes that ceiling
/// self-reporting rather than something to remember to check with `fuser`.
struct CpuEmbedder {
    inner: neuralcompose_llama::Embedder,
    model_id: String,
}

impl SentenceEmbedding for CpuEmbedder {
    fn embed(&mut self, text: &str) -> SeamResult<Embedding> {
        let v = self
            .inner
            .embed(text)
            .map_err(|e| SeamError::Failed(format!("embedding: {e:?}")))?;
        Ok(Embedding::new(v, self.model_id.clone()))
    }
}

/// The only real randomness in the process.
struct SystemDraws;

impl SelectionDraws for SystemDraws {
    fn next_draw(&mut self) -> f64 {
        // Adequate for breaking a tie between two basins; this is not a
        // cryptographic decision, and keeping it dependency-free keeps the
        // binary's dependency set at exactly one crate.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0);
        let mut x = nanos.wrapping_mul(6364136223846793005).wrapping_add(1);
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51afd7ed558ccd);
        x ^= x >> 33;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ────────────────────────────────────────────────────────────────── main ──

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };

    if let Some(path) = &args.verify_log {
        std::process::exit(verify_log(path));
    }

    if let Some(path) = &args.verify_capture {
        std::process::exit(verify_capture_file(path));
    }

    // Like --verify-log: reads a recorded session and exits. Needs no server,
    // no model and no headband, so it runs before `preflight`.
    if let Some(path) = &args.eligibility {
        std::process::exit(eligibility_query(path, args.json));
    }

    // Before `run()`, which opens with an unconditional `preflight` against
    // llama-server. The demo needs no model, no server and no microphone.
    if args.world_model_demo {
        std::process::exit(world_model_demo(args.json, args.heldout));
    }

    if let Err(e) = run(args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Replays a raw capture against its manifest.
///
/// `verify_capture` has existed in `mobile-core` since M4 and had no Linux
/// caller — only the iOS and Android shells. Writing 88 MB an hour of corpus
/// that nothing on this platform can check is the same shape of gap this work
/// exists to close, so the shell that writes captures also verifies them.
fn verify_capture_file(payload: &Path) -> i32 {
    // `<id>.eeg.jsonl` -> `<id>.eeg.manifest.json`, NOT `<id>.manifest.json`.
    // The turn log's manifest sits in the same directory under that shorter
    // name, and `with_extension("").with_extension(...)` — the idiom
    // `--verify-log` uses — lands on it: it strips `.jsonl`, then replaces
    // `.eeg`. The first run of this check read the turn-log manifest and
    // reported "not a capture manifest", which is the correct complaint about
    // the wrong file.
    let name = payload
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some(stem) = name.strip_suffix(".eeg.jsonl") else {
        eprintln!(
            "{}: expected a capture payload named <id>.eeg.jsonl",
            payload.display()
        );
        return 1;
    };
    let manifest_path = payload.with_file_name(format!("{stem}.eeg.manifest.json"));
    let jsonl = match std::fs::read_to_string(payload) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", payload.display());
            return 1;
        }
    };
    let manifest_raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", manifest_path.display());
            return 1;
        }
    };
    let manifest: CaptureManifest = match serde_json::from_str(&manifest_raw) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} is not a capture manifest: {e}", manifest_path.display());
            return 1;
        }
    };
    let messages = manifest.messages_received;
    match verify_capture(jsonl, manifest) {
        ReplayVerdict::Verified {
            accepted_sample_count,
        } => {
            println!(
                "{}: VERIFIED ({accepted_sample_count} samples, {messages} messages)",
                payload.display()
            );
            0
        }
        ReplayVerdict::Failed { failure } => {
            println!("{}: FAILED {failure:?}", payload.display());
            3
        }
    }
}

/// Runs the sealed eligibility query over a recorded turn log.
///
/// Exit codes are distinct on purpose: `0` eligible, `3` ineligible, `1` could
/// not tell. A run that could not read the registration must not be
/// indistinguishable from a session that failed it.
fn eligibility_query(payload: &Path, json: bool) -> i32 {
    // Compiled in, so the binary that reads a session carries the same
    // registration bytes the tests verified against. Reading them from disk at
    // runtime would let a session be judged against a file edited after the
    // build.
    const SEALED: &[u8] = include_bytes!("../../../contracts/eeg/eligibility-v1.json");
    const SEAL: &str = include_str!("../../../contracts/eeg/eligibility-v1.json.sha256");

    let registration = match Registration::load_sealed(SEALED, SEAL) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("the eligibility pre-registration did not verify: {e:?}");
            return 1;
        }
    };

    let jsonl = match std::fs::read_to_string(payload) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", payload.display());
            return 1;
        }
    };
    let mut lines = Vec::new();
    for (i, raw) in jsonl.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<TurnLine>(raw) {
            Ok(l) => lines.push(l),
            Err(e) => {
                // Refused, not skipped. A log this build cannot fully parse
                // must not be scored on the subset it happens to understand.
                eprintln!("line {} of {} did not parse: {e}", i + 1, payload.display());
                return 1;
            }
        }
    }

    let verdict = evaluate(
        tally(&lines, &registration.required_channels),
        &registration.thresholds,
    );

    if json {
        println!("{}", serde_json::to_string_pretty(&verdict).unwrap());
    } else {
        println!(
            "{}: {}",
            payload.display(),
            if verdict.eligible {
                "ELIGIBLE"
            } else {
                "NOT ELIGIBLE"
            }
        );
        println!(
            "  registered {} ({})",
            registration.registered_on, registration.schema_id
        );
        for r in &verdict.reasons {
            println!("  - {r}");
        }
        println!(
            "  {} turns, {} with a reading, {} absent",
            verdict.tally.turns, verdict.tally.turns_with_reading, verdict.tally.turns_absent
        );
    }
    // A verdict either way is a successful query; ineligible is an answer, not
    // a failure to answer.
    if verdict.eligible {
        0
    } else {
        3
    }
}

fn verify_log(payload: &std::path::Path) -> i32 {
    let manifest_path = payload.with_extension("").with_extension("manifest.json");
    let jsonl = match std::fs::read_to_string(payload) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", payload.display());
            return 1;
        }
    };
    let manifest_raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "cannot read the manifest at {}: {e}",
                manifest_path.display()
            );
            return 1;
        }
    };
    let manifest = match serde_json::from_str(&manifest_raw) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("manifest is not valid: {e}");
            return 1;
        }
    };
    match neuralcompose_hypnagogic::turn_log::verify_turn_log(&jsonl, &manifest) {
        neuralcompose_hypnagogic::turn_log::TurnLogVerdict::Verified {
            turn_count,
            silent_turn_count,
        } => {
            println!("verified: {turn_count} turns, {silent_turn_count} silent");
            0
        }
        neuralcompose_hypnagogic::turn_log::TurnLogVerdict::Failed { failure } => {
            eprintln!("FAILED: {failure:?}");
            1
        }
    }
}

// ──────────────────────────────────────────────────────── world model ──

/// The registered planner comparison. Reads no clock beyond the build stamp,
/// opens no socket, loads no model.
///
/// The threshold and the reading rule were pinned in
/// `docs/acceptance/worldmodel-demo.md` and committed at `05f688c`, before this
/// function existed. It is not this function's job to decide whether the result
/// is good — only to report it and apply §4 as written.
fn world_model_demo(json: bool, heldout: bool) -> i32 {
    use neuralcompose_hypnagogic::worldmodel::{
        demo_envelope, heldout_cases, run_episode, worldmodel_method_identity, BangBang, EnvConfig,
        EpisodeResult, MpcConfig, Mppi, PdController, Planner, DEFAULT_SEEDS, GOAL_TOLERANCE,
        HARD_CASES, HELDOUT_SEEDS,
    };

    const MAX_STEPS: usize = 50;
    // §3, restated where the code applies it so the two cannot drift apart.
    const THRESHOLD: usize = 33;

    let env = EnvConfig::default();
    let mpc = MpcConfig::default();

    // §8.1: the held-out run swaps BOTH the scenarios and the seeds. Seeds alone
    // would refresh only MPPI, since the two controllers are deterministic.
    let scenarios: Vec<_> = if heldout {
        heldout_cases()
    } else {
        HARD_CASES.to_vec()
    };
    let seeds: &[u64] = if heldout {
        &HELDOUT_SEEDS
    } else {
        &DEFAULT_SEEDS
    };
    let trials = scenarios.len() * seeds.len();

    let mut results: Vec<EpisodeResult> = Vec::new();
    for scenario in scenarios.iter() {
        for &seed in seeds {
            let mut arms: Vec<Box<dyn Planner>> = vec![
                Box::new(Mppi::new(mpc, seed)),
                Box::new(PdController::default()),
                Box::new(BangBang),
            ];
            for planner in arms.iter_mut() {
                results.push(run_episode(
                    &env,
                    planner.as_mut(),
                    scenario,
                    seed,
                    MAX_STEPS,
                    GOAL_TOLERANCE,
                ));
            }
        }
    }

    let summarise = |id: &str| {
        let rows: Vec<&EpisodeResult> = results.iter().filter(|r| r.planner == id).collect();
        let reached = rows.iter().filter(|r| r.reached).count();
        let mean = rows.iter().map(|r| r.final_distance as f64).sum::<f64>() / rows.len() as f64;
        let stalls: u32 = rows.iter().map(|r| r.stalls).sum();
        let steps = rows.iter().map(|r| r.steps as f64).sum::<f64>() / rows.len() as f64;
        (reached, mean, stalls, steps)
    };

    let (mppi_reached, mppi_mean, mppi_stalls, mppi_steps) = summarise("mppi");
    let (pd_reached, pd_mean, _, pd_steps) = summarise("pd");
    let (bb_reached, bb_mean, _, bb_steps) = summarise("bang-bang");

    // §8.2, evaluated where the numbers are so the claims and the code cannot
    // drift apart. Percentages are excess over PD, the fastest arm in §6.
    let over_pd = |x: f64| (x - pd_steps) / pd_steps * 100.0;
    let c1 = pd_steps < bb_steps && bb_steps < mppi_steps;
    let c2 = over_pd(mppi_steps) >= 25.0;
    let c3 = over_pd(bb_steps) >= 5.0;

    // §4, applied as written. The order matters: non-discrimination is checked
    // BEFORE success, so a passing MPPI on an undiscriminating task cannot be
    // reported as a win.
    let verdict = if mppi_reached >= THRESHOLD && pd_reached >= THRESHOLD {
        "does-not-discriminate"
    } else if mppi_reached >= THRESHOLD {
        "threshold-met"
    } else {
        "threshold-missed"
    };

    let method = worldmodel_method_identity(
        &env,
        &mpc,
        env!("CARGO_PKG_VERSION"),
        option_env!("NC_HYPNAGOGIC_COMMIT").map(str::to_string),
    );
    let envelope = demo_envelope(method, Vec::new());

    if json {
        let doc = serde_json::json!({
            "schemaId": "neuralcompose.hypnagogic.worldmodel-demo.v1",
            "registration": "docs/acceptance/worldmodel-demo.md",
            "threshold": {"reached": THRESHOLD, "of": trials, "goalTolerance": GOAL_TOLERANCE},
            "verdict": verdict,
            "heldout": heldout,
            "stepClaims": {
                "registration": "§8.2",
                "c1_ordering_pd_lt_bb_lt_mppi": c1,
                "c2_mppi_over_pd_pct": over_pd(mppi_steps),
                "c2_met": c2,
                "c3_bangbang_over_pd_pct": over_pd(bb_steps),
                "c3_met": c3,
            },
            "arms": {
                "mppi": {"reached": mppi_reached, "meanFinalDistance": mppi_mean, "meanSteps": mppi_steps, "stalls": mppi_stalls},
                "pd": {"reached": pd_reached, "meanFinalDistance": pd_mean, "meanSteps": pd_steps},
                "bangBang": {"reached": bb_reached, "meanFinalDistance": bb_mean, "meanSteps": bb_steps},
            },
            "episodes": results,
            "provenance": envelope,
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
    } else {
        println!(
            "registration: docs/acceptance/worldmodel-demo.md ({} set, §3 bar {THRESHOLD}/{trials})",
            if heldout { "§8 HELD-OUT" } else { "§3 original" }
        );
        println!(
            "{:<10} {:>8} {:>20} {:>11} {:>8}",
            "arm", "reached", "mean final distance", "mean steps", "stalls"
        );
        for (name, reached, mean, steps, stalls) in [
            (
                "mppi",
                mppi_reached,
                mppi_mean,
                mppi_steps,
                mppi_stalls.to_string(),
            ),
            ("pd", pd_reached, pd_mean, pd_steps, "-".to_string()),
            ("bang-bang", bb_reached, bb_mean, bb_steps, "-".to_string()),
        ] {
            println!(
                "{name:<10} {:>8} {mean:>20.4} {steps:>11.1} {stalls:>8}",
                format!("{reached}/{trials}")
            );
        }
        println!();
        println!("§8.2 step claims (excess over pd):");
        println!(
            "  C1  ordering pd < bang-bang < mppi ....  {}",
            if c1 { "HOLDS" } else { "FAILS" }
        );
        println!(
            "  C2  mppi over pd >= 25% .............. {:>7.1}%  {}",
            over_pd(mppi_steps),
            if c2 { "HOLDS" } else { "FAILS" }
        );
        println!(
            "  C3  bang-bang over pd >= 5% .......... {:>7.1}%  {}",
            over_pd(bb_steps),
            if c3 { "HOLDS" } else { "FAILS" }
        );
        println!();
        match verdict {
            "does-not-discriminate" => println!(
                "verdict: THE TASK DOES NOT DISCRIMINATE over true dynamics.\n  \
                 Both MPPI and the damped controller clear the bar, so this is not an\n  \
                 MPPI success — §4 of the registration says so, and it said so first."
            ),
            "threshold-met" => println!(
                "verdict: threshold met. MPPI {mppi_reached}/{trials}, PD {pd_reached}/{trials}."
            ),
            _ => println!(
                "verdict: THRESHOLD MISSED. MPPI {mppi_reached}/{trials}, bar was {THRESHOLD}.\n  \
                 Reported as a miss. The bar does not move."
            ),
        }
        eprintln!(
            "\nnote: pd and bang-bang are deterministic, so their five seeds are five\n\
             identical trials, not five samples. Only mppi's seeds vary anything."
        );
    }

    0
}

// ──────────────────────────────────────────────────────────────────── eeg ──

/// A live EEG source: one WebSocket reader thread feeding a `StreamMonitor`.
///
/// Everything decided here is an effect — connect, read, clock. *Whether there
/// is anything to report* is [`eeg_reading_for_turn`]'s job, in the lib, where
/// it can be tested; this struct only supplies it a phase and a window.
///
/// The reader runs on its own thread because the dialectic blocks for seconds
/// at a time in `generate`, and samples arriving during that must still land.
/// `StreamMonitor` is `Sync` and takes `&self`, so no lock of ours is involved.
/// The effectful half of `CaptureRecorder`, which is deliberately effect-free:
/// it returns the exact bytes to append and the manifest to publish, and never
/// touches a file. This owns the file.
///
/// **Appends as frames arrive.** The turn log can afford to be assembled in
/// memory and written at the end — it is a few KB. A capture is ~24.5 KB/s, so
/// the same pattern would hold a whole session in RAM and lose all of it if the
/// process dies. It also means a killed run leaves a payload with no manifest,
/// which is exactly the `.partial` case `capture.rs` already defines.
struct CaptureWriter {
    recorder: CaptureRecorder,
    recording_id: String,
    /// `Mutex` because the reader thread appends and the main thread finishes.
    /// The lock is held only across one `write_all`.
    file: Mutex<std::fs::File>,
    /// Set once if a write ever fails, so the failure is reported at the end
    /// rather than once per frame — 32 identical lines a second would bury the
    /// conversation the run exists for.
    write_failed: AtomicBool,
    /// Counted so the manifest's byte size comes from what was actually
    /// written, not from what the recorder believes it handed over.
    bytes_written: AtomicU64,
    /// Digested as it is written. Re-reading the file at the end would work and
    /// would also mean reading ~29 MB back to describe bytes we just produced —
    /// and would digest whatever is on disk *now*, which is not necessarily
    /// what this process wrote.
    digest: Mutex<Sha256>,
}

impl CaptureWriter {
    fn new(dir: &Path, recording_id: String, build: CaptureBuildIdentity) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("capture dir: {e}"))?;
        let path = dir.join(format!("{recording_id}.eeg.jsonl"));
        let file = std::fs::File::create(&path)
            .map_err(|e| format!("capture payload {}: {e}", path.display()))?;
        Ok(Self {
            recorder: CaptureRecorder::new(recording_id.clone(), build, 0),
            recording_id,
            file: Mutex::new(file),
            write_failed: AtomicBool::new(false),
            bytes_written: AtomicU64::new(0),
            digest: Mutex::new(Sha256::new()),
        })
    }

    fn recording_id(&self) -> &str {
        &self.recording_id
    }

    /// One WebSocket frame, preserved verbatim. `CaptureRecorder` wraps it
    /// without reinterpreting the payload, so a replay re-decodes the same
    /// bytes the client saw.
    fn append(&self, payload: &str, now_ms: u64) {
        let line = self.recorder.on_message(payload.to_string(), now_ms);
        let mut file = match self.file.lock() {
            Ok(f) => f,
            // A poisoned lock means the writing thread panicked. Record the
            // failure; do not panic the reader thread as well.
            Err(_) => {
                self.write_failed.store(true, Ordering::Relaxed);
                return;
            }
        };
        let bytes = format!("{line}\n");
        match file.write_all(bytes.as_bytes()) {
            Ok(()) => {
                self.bytes_written
                    .fetch_add(bytes.len() as u64, Ordering::Relaxed);
                // Digested only on a successful write, so the manifest
                // describes the bytes that reached the file rather than the
                // ones we hoped to put there.
                if let Ok(mut d) = self.digest.lock() {
                    d.update(bytes.as_bytes());
                }
            }
            Err(_) => self.write_failed.store(true, Ordering::Relaxed),
        }
    }

    /// Publishes the manifest. Returns the path written, or the reason none was.
    ///
    /// **A failed write must not produce a manifest.** A manifest asserts that
    /// a payload of a given size and digest exists; publishing one over a
    /// truncated file would make the capture claim to verify and then fail
    /// `verify_capture` with a digest mismatch, which reads as corruption
    /// rather than as the write error it is.
    fn finish(&self, dir: &Path, now_ms: u64) -> Result<PathBuf, String> {
        if self.write_failed.load(Ordering::Relaxed) {
            return Err(format!(
                "capture {} had write failures; no manifest written. The payload \
                 is incomplete and is left as-is rather than described by a \
                 manifest that would not verify.",
                self.recording_id
            ));
        }
        if let Ok(mut f) = self.file.lock() {
            f.flush().map_err(|e| format!("capture flush: {e}"))?;
        }
        let size = self.bytes_written.load(Ordering::Relaxed);
        let digest = self
            .digest
            .lock()
            .map(|d| {
                d.clone()
                    .finalize()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            })
            .map_err(|_| "capture digest lock poisoned".to_string())?;
        let manifest = self.recorder.finish(now_ms, size, digest);
        let path = dir.join(format!("{}.eeg.manifest.json", self.recording_id));
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("capture manifest encode: {e}"))?;
        std::fs::write(&path, json).map_err(|e| format!("capture manifest write: {e}"))?;
        Ok(path)
    }
}

struct EegSource {
    monitor: Arc<StreamMonitor>,
    /// Monotonic origin. `StreamMonitor` never reads a clock — every `now_ms`
    /// it sees comes from here, and it must be monotonic or staleness is
    /// nonsense. `Instant`, never `SystemTime`.
    started: Instant,
    /// Seals the parameters behind every number in a channel record. Built once
    /// per run: the configuration does not change between turns, and rebuilding
    /// it per turn would digest the same document four times a minute.
    method: MethodIdentity,
    /// The raw-capture writer, when `--log` asked for one. `None` means no
    /// `.eeg.jsonl` is being written and the window ResourceRefs carry no
    /// locator.
    capture: Option<Arc<CaptureWriter>>,
}

impl EegSource {
    /// Connects synchronously so a bad URL fails *before* the loop starts.
    ///
    /// The alternative — connecting on the reader thread — produces a run that
    /// looks healthy and silently logs no channel health at all, which is
    /// indistinguishable from an EEG that is attached but stale.
    fn connect(
        url: &str,
        method: MethodIdentity,
        capture: Option<Arc<CaptureWriter>>,
    ) -> Result<Self, String> {
        let monitor = Arc::new(StreamMonitor::new(MonitorConfig::default()));
        let started = Instant::now();
        monitor.on_socket_event(SocketEvent::Connecting, 0);

        let (mut socket, _response) = tungstenite::connect(url).map_err(|e| {
            format!(
                "EEG source is not answering at {url} ({e}).\n\
                 Start one first, e.g.:\n  \
                 python3 tools/fixture-eeg-server/server.py --seconds 300\n\
                 or tools/muse-ble-bridge/bridge.py for a real headband.\n\
                 Note: only ws:// is supported (no TLS is linked in)."
            )
        })?;
        monitor.on_socket_event(SocketEvent::Opened, started.elapsed().as_millis() as u64);

        let reader = Arc::clone(&monitor);
        let writer = capture.clone();
        std::thread::spawn(move || {
            loop {
                let now = started.elapsed().as_millis() as u64;
                match socket.read() {
                    Ok(tungstenite::Message::Text(t)) => {
                        // Appended as it arrives, never buffered. At 256 Hz in
                        // batches of 8 this is ~24.5 KB/s (766 B/line x 32
                        // lines/s, measured against the fixture), so a
                        // 20-minute session is ~29 MB — fine on disk and not
                        // fine in memory for an unbounded run.
                        //
                        // A write failure is reported once and the stream
                        // continues: losing the corpus is bad, losing the
                        // conversation the corpus is of is worse.
                        if let Some(w) = writer.as_ref() {
                            w.append(t.as_str(), now);
                        }
                        reader.on_frame(t.as_str().to_string(), now);
                    }
                    // Ping/pong/binary are not the contract; ignore rather than
                    // treat as a close, which would misreport the phase.
                    Ok(_) => {}
                    Err(e) => {
                        // A closed stream must reach the monitor, or `phase()`
                        // keeps reporting Live off cached samples until the
                        // staleness window expires.
                        reader.on_socket_event(SocketEvent::Errored, now);
                        eprintln!("⚡ eeg: stream ended ({e})");
                        break;
                    }
                }
            }
        });

        Ok(EegSource {
            monitor,
            started,
            method,
            capture,
        })
    }

    /// Milliseconds since this source connected, on the same monotonic axis
    /// every `now_ms` uses.
    fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// This turn's reading, or the reason there is nothing current to report.
    fn reading(&self) -> Result<EegTurnReading, EegRefusal> {
        let now = self.started.elapsed().as_millis() as u64;
        // Taken BEFORE the snapshot so the timestamp can only be older than the
        // window, never newer. A window labelled with a timestamp from a sample
        // it does not contain would point a reader at the wrong bytes.
        let last_source_timestamp = self.monitor.newest_source_timestamp();
        eeg_reading_for_turn(
            self.monitor.phase(now),
            &self.monitor.snapshot().channels,
            SAMPLE_RATE_HZ,
            ChannelHealthThresholds::default(),
            MainsThresholds::default(),
            EegRecordContext {
                method: self.method.clone(),
                recording_id: self.capture.as_ref().map(|c| c.recording_id()),
                last_source_timestamp,
            },
        )
    }
}

/// Fails fast, before the microphone is ever opened.
///
/// `turn.sh` does the same thing with `curl /health`, and for the same reason: a
/// loop that discovers the server is down on its first generate has already
/// recorded an utterance it cannot answer.
/// Reports what the server actually loaded, when it says.
///
/// **The `fuser -v /dev/dri/renderD128` check alone cannot tell "the ceiling
/// holds" from "no GPU is in play at all".** Verified 2026-08-13: the installed
/// `/usr/local/bin/llama-server` is a Vulkan-built binary that cannot find its
/// `libggml-vulkan.so` at runtime, so it silently degrades to CPU — and `fuser`
/// then shows no inference process on the render node, which reads exactly like
/// a held ceiling. The check needs its companion: llama-server MUST appear on
/// the render node (proving Vulkan is genuinely in use) and this binary must
/// not. If neither appears, the check proved nothing.
fn preflight(server: &str) -> Result<(), String> {
    match ureq::get(&http::health_url(server))
        .timeout(std::time::Duration::from_secs(3))
        .call()
    {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "llama-server is not answering at {server} ({e}).\n\
             Start it first, e.g.:\n  \
             llama-server -m <model.gguf> --host 127.0.0.1 --port 8080\n\
             Or point elsewhere with --server."
        )),
    }
}

/// The `--generator claude` counterpart of [`preflight`]: prove the CLI is
/// there and signed in *before* the microphone opens, rather than discovering
/// it on the first generate — same rule the local path already follows.
///
/// `claude -p` with a trivial prompt is the cheapest honest check. `--version`
/// would prove the binary exists and nothing about the login, which is the
/// failure that actually happens.
fn preflight_claude(model: &str, workdir: &Path) -> Result<(), String> {
    eprintln!("● checking the claude CLI (one short round trip)…");
    let argv = claude_cli::argv(model, "Reply with the single word: ready", "ready?");
    let out = Command::new("claude")
        .args(&argv)
        .current_dir(workdir)
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| {
            format!(
                "the `claude` CLI is not runnable ({e}).\n\
                 Install it and sign in, or drop --generator claude to stay local."
            )
        })?;
    if !out.status.success() {
        return Err(format!(
            "the `claude` CLI exited {} — try `claude -p hello` to see why \
             (usually: not signed in).",
            out.status
        ));
    }
    claude_cli::parse_result(&String::from_utf8_lossy(&out.stdout))
        .map(|_| ())
        .map_err(|e| format!("the claude CLI answered, but not in the expected shape: {e}"))
}

fn run(args: Args) -> Result<(), String> {
    let cloud = args.generator == "claude";
    if !cloud {
        preflight(&args.server)?;
    }

    // Refused rather than warned: mirror mode writes no turn record at all, and
    // the turn log is the ONLY place EEG appears (nothing here biases the
    // dialectic). So `--mode mirror --eeg-url ...` would connect a headband,
    // read it every turn, and produce no observable whatsoever — a run that
    // looks like it worked and cannot be distinguished from one without the
    // flag. The plan's own stage-3 check specified exactly that combination.
    let workdir = std::env::temp_dir().join(format!("nc-hypnagogic-{}", std::process::id()));
    std::fs::create_dir_all(&workdir).map_err(|e| format!("workdir: {e}"))?;

    eprintln!("● mode: {} ({})", args.mode.label(), args.mode.id());
    // The generator identity as recorded in every turn's method identity. Built
    // here, once, so the banner and the record cannot disagree.
    let generator_id = if cloud {
        claude_cli::generator_id(&args.claude_model)
    } else {
        "llama-server".to_string()
    };
    if cloud {
        preflight_claude(&args.claude_model, &workdir)?;
        // Said at this volume on purpose. Everything else in this binary talks
        // to 127.0.0.1 or to a subprocess; this is the one line where that
        // stops being true, and a user who did not mean to opt in should be
        // able to see it and Ctrl-C.
        eprintln!("⚠ generation: {} via the `claude` CLI", args.claude_model);
        eprintln!("⚠   YOUR TRANSCRIPTS LEAVE THIS MACHINE. Audio and EEG do not:");
        eprintln!("⚠   whisper runs on-device and EEG never reaches a prompt.");
        eprintln!("⚠   `claude -p` exposes no temperature, so the two poles differ");
        eprintln!("⚠   by their system prompts ONLY — not by sampling.");
        eprintln!("⚠   Measured: ~5.5 s and $0.01–$0.24 per generate call, and a");
        eprintln!("⚠   reflective turn makes three. Ctrl-C now if that is a surprise.");
    } else {
        eprintln!("● generation: llama-server at {}", args.server);
    }
    if let Some(p) = args.mode.profile() {
        eprintln!(
            "● profile: {} · inter-turn {:?} · silence run ≤{}{}",
            p.label(),
            p.inter_turn_delay(),
            p.max_consecutive_silence(),
            if p.witness_enabled() {
                " · witness ON (3 calls/turn)"
            } else {
                ""
            }
        );
    }

    let session_id = format!(
        "session-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let mut recorder = args
        .log
        .then(|| TurnLogRecorder::new(session_id.clone(), args.mode.id()));
    // Opened up front so the payload exists from the first turn. `write_log`
    // now only publishes the manifest — the bytes it describes are already on
    // disk, which is the same ordering rule as before and a stronger version of
    // it: the manifest can never precede its payload because the payload is
    // written first by construction rather than by call order.
    let mut turn_file: Option<std::fs::File> = if args.log {
        std::fs::create_dir_all(&args.log_dir).map_err(|e| format!("log dir: {e}"))?;
        let p = args.log_dir.join(turn_log_payload_filename(&session_id));
        Some(std::fs::File::create(&p).map_err(|e| format!("turn log {}: {e}", p.display()))?)
    } else {
        None
    };
    // `None` in mirror mode, which loads no embedder at all — absent because
    // there was nothing to read back, not because the read failed.
    let mut embedder_backend: Option<&'static str> = None;

    // Refused rather than warned: mirror mode writes no turn record at all, and
    // the turn log is the ONLY place per-turn EEG appears (nothing here biases
    // the dialectic). So `--mode mirror --eeg-url ...` would connect a headband,
    // read it every turn, and produce no per-turn observable whatsoever — a run
    // that looks like it worked and cannot be distinguished from one without the
    // flag.
    if let (Some(url), None) = (&args.eeg_url, args.mode.profile()) {
        return Err(format!(
            "--eeg-url {url} has no observable in mirror mode: mirror writes \
             no turn record, and the turn log is the only place EEG appears.\n\
             Use --mode focused|reflective|contemplative, and --log to persist it."
        ));
    }

    // The raw capture. Written only when there is both an EEG to capture and a
    // `--log` asking for persistence: a corpus nobody asked to keep is 88 MB an
    // hour of surprise.
    let capture = match (&args.eeg_url, args.log) {
        (Some(_), true) => {
            let w = Arc::new(CaptureWriter::new(
                &args.log_dir,
                session_id.clone(),
                CaptureBuildIdentity {
                    platform: "linux".to_string(),
                    os_version: sysfs("/proc/sys/kernel/osrelease")
                        .unwrap_or_else(|| "unknown".to_string()),
                    app_version: env!("CARGO_PKG_VERSION").to_string(),
                    // Empty when the tree was dirty at build time — see build.rs.
                    // An unpinned build says so rather than naming a commit that
                    // does not describe it.
                    git_commit: option_env!("NC_HYPNAGOGIC_COMMIT")
                        .unwrap_or_default()
                        .to_string(),
                    // The bridge and the fixture server both listen on loopback
                    // or the LAN; no remote endpoint is reachable from here (no
                    // TLS is linked in, so `wss://` cannot even connect).
                    bridge_locality: BridgeLocality::LocalNetwork,
                },
            )?);
            eprintln!(
                "● capture: {}.eeg.jsonl (~24.5 KB/s while live)",
                session_id
            );
            Some(w)
        }
        (Some(_), false) => {
            eprintln!(
                "● capture: none (pass --log to write the raw EEG; \
                 channel records will carry a window digest but no locator)"
            );
            None
        }
        (None, _) => None,
    };

    // Built once per run: the configuration behind every channel record does
    // not change between turns, and rebuilding it per turn would digest the
    // same document once a second while claiming a fresh identity each time.
    let eeg_method = eeg_method_identity(
        SAMPLE_RATE_HZ,
        MonitorConfig::default().keep_samples,
        ChannelHealthThresholds::default(),
        MainsThresholds::default(),
        env!("CARGO_PKG_VERSION"),
        option_env!("NC_HYPNAGOGIC_COMMIT").map(str::to_string),
    );

    let eeg = match &args.eeg_url {
        Some(url) => {
            let src = EegSource::connect(url, eeg_method.clone(), capture.clone())?;
            eprintln!("● eeg: {url}");
            Some(src)
        }
        None => None,
    };

    let listener: Box<dyn Listening> = if args.mic && args.push_to_talk {
        eprintln!("● input: microphone, push-to-talk (speak, then press Enter)");
        Box::new(PushToTalkListener {
            whisper: args.whisper.clone(),
            model: args.whisper_model.clone(),
            workdir: workdir.clone(),
        })
    } else if args.mic {
        eprintln!("● input: microphone, hands-free (arecord + whisper-cli)");
        Box::new(VadListener {
            whisper: args.whisper.clone(),
            model: args.whisper_model.clone(),
            workdir: workdir.clone(),
            gate: None,
            cfg: vad::VadConfig::default(),
            forced_gate: args.mic_gate,
        })
    } else {
        eprintln!("● input: stdin (pass --mic for the microphone)");
        Box::new(StdinListener)
    };
    // Kokoro owns sentence-level prosody, so it must NOT be handed fragments:
    // chunking both denies it that context and pays a model load per fragment.
    let mut chunk_replies = true;
    let speaker: Box<dyn Speaking> = match (args.speak, args.tts.as_str()) {
        (true, "kokoro") => {
            let script = kokoro_script()?;
            let python = script.parent().unwrap().join(".venv/bin/python");
            if !python.exists() {
                return Err(format!(
                    "kokoro needs the venv at {} — see tools/spoken-loop/README.md, \
                     or pass --tts espeak",
                    python.display()
                ));
            }
            chunk_replies = false;
            eprintln!("● output: Kokoro-82M via speak.py + pw-play (whole utterances, not chunks)");
            eprintln!("●   onnxruntime is imported by that SCRIPT, not linked into this binary");
            Box::new(KokoroSpeaker {
                script,
                python,
                workdir: workdir.clone(),
            })
        }
        (true, _) => {
            eprintln!("● output: espeak-ng + pw-play (chunked micro-phrases)");
            Box::new(EspeakSpeaker {
                workdir: workdir.clone(),
            })
        }
        (false, _) => {
            eprintln!("● output: stdout (pass --speak to synthesize)");
            Box::new(PrintSpeaker)
        }
    };

    // One constructor for both loops: a `Box<dyn TextGenerating>` is itself
    // `TextGenerating` (the blanket impl in `seams.rs`), so the choice is made
    // once here rather than duplicated per mode.
    let make_generator = || -> Box<dyn TextGenerating> {
        if cloud {
            Box::new(ClaudeCliGenerator {
                model: args.claude_model.clone(),
                workdir: workdir.clone(),
            })
        } else {
            Box::new(HttpGenerator {
                server: args.server.clone(),
            })
        }
    };

    match args.mode.profile() {
        None => {
            let mut l = MirrorLoop::new(
                listener,
                make_generator(),
                speaker,
                MirrorConfig {
                    chunk_replies,
                    ..MirrorConfig::default()
                },
            );
            for _ in 0..args.turns {
                match l.turn() {
                    Ok(t) => {
                        if args.json {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "index": t.index, "mode": args.mode.id(),
                                    "heard": t.heard, "spoken": t.spoken, "wasCue": t.was_cue,
                                })
                            );
                        }
                    }
                    // A failed turn must not end the run: the Swift falls
                    // through to the delay rather than hot-spinning.
                    Err(e) => eprintln!("turn failed: {e}"),
                }
            }
        }
        Some(profile) => {
            let (embedder, backend) = build_embedder()?;
            embedder_backend = Some(backend);
            let mut l = DialecticLoop::new(
                listener,
                make_generator(),
                speaker,
                embedder,
                SystemDraws,
                waking_roles().to_vec(),
                profile,
                DialecticConfig {
                    chunk_replies,
                    voice_both: args.voice_both,
                    drift_ceiling: args
                        .drift_ceiling
                        .unwrap_or(DialecticConfig::default().drift_ceiling),
                    repetition_floor: args
                        .repetition_floor
                        .unwrap_or(DialecticConfig::default().repetition_floor),
                    // `None` whenever the tree was dirty at build time — see
                    // build.rs. An unpinned build says so rather than naming a
                    // commit that does not describe it.
                    git_commit: option_env!("NC_HYPNAGOGIC_COMMIT").map(str::to_string),
                    generator: generator_id.clone(),
                    ..DialecticConfig::default()
                },
            );
            let method = l.method_identity();
            // `--turns 0` runs until the session is ended by voice or by the
            // input stream closing. A hypnagogic session has no natural turn
            // count — you stop when you drift off — and a fixed one either cuts
            // it short or leaves the loop talking to an empty room.
            let open_ended = args.turns == 0;
            if open_ended {
                eprintln!(
                    "● open-ended: say {:?} to finish (or Ctrl-C — the log is written as it goes)",
                    STOP_PHRASES[0]
                );
            }
            let mut turn_number = 0u32;
            let mut stopped = false;
            while !stopped && (open_ended || turn_number < args.turns) {
                match l.turn() {
                    Ok(Some(t)) => {
                        // Checked before the record is written, so the turn that
                        // ends the session is still logged. It was a real turn.
                        if open_ended && is_stop_phrase(&t.heard) {
                            stopped = true;
                        }
                        // Built ONCE. Two calls would let the JSON echo and the
                        // persisted record disagree — and the echo is what a
                        // reader would trust while the record is what verifies.
                        let mut line =
                            t.to_turn_line(args.mode.id(), method.clone(), &generator_id);
                        if let Some(src) = eeg.as_ref() {
                            match src.reading() {
                                Ok(r) => {
                                    for note in &r.blocking {
                                        eprintln!("⚡ {note}");
                                    }
                                    if let Some(hint) = r.common_mode {
                                        eprintln!("⚡ {hint}");
                                    }
                                    line = line.with_channel_health(r.lines);
                                }
                                // Said out loud every turn on purpose: an
                                // attached-but-not-live EEG writes almost the
                                // same record as no EEG at all, so silence here
                                // would make the two hard to tell apart. The
                                // reason now goes on the record too, which is
                                // what lets eligibility be a query later
                                // instead of a memory.
                                Err(why) => {
                                    eprintln!("⚡ eeg: nothing to report this turn ({})", why.id());
                                    line = line.with_channel_health_absent(why.id());
                                }
                            }
                        }
                        if args.json {
                            println!("{}", serde_json::to_string(&line).unwrap());
                        }
                        // Appended and flushed per turn, not accumulated. An
                        // open-ended session can run for hours, and buffering
                        // the whole log in memory means a Ctrl-C — the ordinary
                        // way to end one — loses every turn of it. A payload
                        // with no manifest is the `.partial` case the capture
                        // side already defines, and is recoverable; a payload
                        // that never existed is not.
                        if let (Some(r), Some(f)) = (recorder.as_mut(), turn_file.as_mut()) {
                            let encoded = r.on_turn(&line);
                            if let Err(e) = writeln!(f, "{encoded}").and_then(|()| f.flush()) {
                                eprintln!("⚠ turn log: {e}");
                            }
                        }
                        if let Some(err) = &t.witness_error {
                            eprintln!("witness failed on turn {}: {err}", t.index);
                        }
                        // The loop's own drift and repetition state, on stderr
                        // beside the witness line. Before this, every one of
                        // these numbers was computed each turn and written only
                        // to the jsonl, where nobody looked until after a
                        // session had already gone wrong — `self_similarity`
                        // still is, and is printed here for exactly that
                        // reason.
                        if t.reanchored {
                            eprintln!(
                                "turn {}: drift {:.3} past the ceiling — prompts re-anchored \
                                 to the opening utterance (self-similarity {})",
                                t.index,
                                t.topic_drift.unwrap_or(f32::NAN),
                                t.self_similarity
                                    .map(|s| format!("{s:.3}"))
                                    .unwrap_or_else(|| "n/a".into()),
                            );
                        }
                        if t.repetition_forced_silence {
                            eprintln!(
                                "turn {}: {} of the last replies were near-repeats — turn \
                                 forced silent (the competition chose to speak; the log \
                                 keeps both)",
                                t.index, t.repetition_hits,
                            );
                        }
                    }
                    Ok(None) => eprintln!("turn {turn_number} skipped (nothing heard)"),
                    Err(e) => eprintln!("turn failed: {e}"),
                }
                turn_number += 1;
                if !stopped && (open_ended || turn_number < args.turns) {
                    std::thread::sleep(profile.inter_turn_delay());
                }
            }
            if stopped {
                eprintln!("● stopping, as asked");
            }
        }
    }

    // Shutdown, in dependency order: the capture manifest describes bytes that
    // must already be flushed, and the session record names the capture.
    let capture_ok = match (&capture, &eeg) {
        (Some(w), Some(src)) => {
            let now = src.elapsed_ms();
            match w.finish(&args.log_dir, now) {
                Ok(p) => {
                    eprintln!("● capture: {}", p.display());
                    true
                }
                Err(e) => {
                    // Reported, not swallowed, and not fatal: the turn log and
                    // the session record are still worth writing.
                    eprintln!("⚠ capture: {e}");
                    false
                }
            }
        }
        _ => false,
    };

    // Three states, so absence stays informative: a capture gets the command
    // that opens it; EEG configured without a capture says so (absent and
    // failed must not read the same); no EEG prints nothing at all. Print,
    // not spawn — a session's exit status must not depend on a display tool.
    match (&eeg, &capture, capture_ok) {
        (Some(_), Some(_), true) => eprintln!(
            "● workspace: uv run tools/workspace/workspace.py {} --dir {}  (from the repo root)",
            session_id,
            args.log_dir.display()
        ),
        (Some(_), Some(_), false) => {
            eprintln!("● workspace: none — the capture failed to finish (see ⚠ above)");
        }
        (Some(_), None, _) => {
            eprintln!("● workspace: none — EEG was attached but no capture was written (--log)");
        }
        (None, _, _) => {}
    }

    if let Some(r) = recorder {
        write_log(&args.log_dir, &session_id, turn_file.take(), &r)?;
        let session = SessionRecord {
            schema_id: SESSION_RECORD_SCHEMA.to_string(),
            session_id: session_id.clone(),
            mode: args.mode.id().to_string(),
            // Names the capture only if one was actually completed. A recording
            // id here for a manifest that was never written would point a later
            // reader at a file that does not verify.
            recording_id: capture_ok.then(|| session_id.clone()),
            eeg_url: args.eeg_url.clone(),
            embedder_backend_id: embedder_backend.map(str::to_string),
            power: read_power_state(),
            hci_adapters: read_hci_adapters(),
            host_provenance: host_envelope(),
            // `None` when the operator claimed nothing — which is honest, and
            // is why neither field defaults to a board this code merely expects
            // to be there.
            claimed_source: match (&args.eeg_source, &args.eeg_preset) {
                (None, None) => None,
                (board_id, preset) => Some(ClaimedSource {
                    board_id: board_id.clone(),
                    preset: preset.clone(),
                    provenance: claim_envelope(eeg_method.clone()),
                }),
            },
        };
        let path = args.log_dir.join(format!("{session_id}.session.json"));
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("writing the session record: {e}"))?;
        eprintln!("● session: {}", path.display());
    }
    Ok(())
}

/// Locates `tools/spoken-loop/speak.py` relative to this repo.
fn kokoro_script() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("NC_KOKORO_SPEAK") {
        let p = PathBuf::from(p);
        return if p.exists() {
            Ok(p)
        } else {
            Err(format!(
                "NC_KOKORO_SPEAK points at {} which does not exist",
                p.display()
            ))
        };
    }
    // CARGO_MANIFEST_DIR is crates/neuralcompose-hypnagogic; the tool lives two
    // levels up. Baked at build time so a run from any cwd finds it.
    let guess = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/spoken-loop/speak.py");
    if guess.exists() {
        return Ok(guess);
    }
    Err(format!(
        "cannot find tools/spoken-loop/speak.py (looked at {}); set NC_KOKORO_SPEAK \
         or pass --tts espeak",
        guess.display()
    ))
}

fn build_embedder() -> Result<(CpuEmbedder, &'static str), String> {
    let model = std::env::var("NC_EMBED_MODEL").map_err(|_| {
        "the dialectical modes need an embedder: set NC_EMBED_MODEL to a \
         bge-small-en-v1.5 GGUF. Mirror mode needs none."
            .to_string()
    })?;
    // `gpu_layers: 0` is the whole ceiling, expressed at the call site: with no
    // layers offloaded, ggml never initialises a Vulkan device and this process
    // creates no context. `n_ctx: 0` takes the model's own trained length.
    let inner = neuralcompose_llama::Embedder::load_with(std::path::Path::new(&model), 0, 0)
        .map_err(|e| {
            format!(
                "opening the CPU embedder from {model}: {e:?}\n\
                 (if this says Unavailable, neuralcompose-llama was built as a \
                 stub — set LLAMA_CPP_DIR and rebuild)"
            )
        })?;
    // ASSERTED, not announced. This line used to print `BACKEND_ID_CPU` as a
    // literal in the format string — a constant that said "CPU" whatever the
    // embedder had actually done, which is a claim about the code rather than
    // about the run.
    //
    // `Embedder::backend_id()` derives the answer from the OUTCOME: it reports
    // Vulkan only if an accelerator was found AND layers were requested. So
    // reading it back turns the ceiling from a comment into a startup gate, and
    // a build that starts offloading fails here instead of quietly taking the
    // render node out from under llama-server.
    let backend = inner.backend_id();
    if backend != neuralcompose_llama::BACKEND_ID_CPU {
        return Err(format!(
            "the embedder came up on {backend}, not {}.\n\
             Exactly one process in this system may hold a Vulkan context and it \
             is llama-server. Refusing to start rather than contend for the \
             render node.",
            neuralcompose_llama::BACKEND_ID_CPU
        ));
    }
    eprintln!("● embeddings: {backend} (read back from the loaded embedder, not assumed)");
    Ok((
        CpuEmbedder {
            inner,
            model_id: model,
        },
        backend,
    ))
}

// ───────────────────────────────────────────────────────────── host state ──

/// Trims a `/sys` file to its contents, or `None` if it cannot be read.
///
/// `None` covers "no such file", "no permission" and "empty" alike, because
/// none of them is a value. What must never happen is an unreadable governor
/// becoming `"unknown"` and later reading as a governor named unknown.
fn sysfs(path: impl AsRef<Path>) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Power, thermal and governor state. Ported from `power_state()` in
/// `tools/spoken-loop/dialectic-relay/relay.py`, which is the only structured
/// host-state capture already in this repository.
fn read_power_state() -> PowerState {
    let mut power = PowerState::default();
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        for e in entries.flatten() {
            let p = e.path();
            match sysfs(p.join("type")).as_deref() {
                Some("Mains") => {
                    // Only an explicit 1/0 is an answer. Anything else is a
                    // file we could not interpret, which is not "off mains".
                    power.on_ac = match sysfs(p.join("online")).as_deref() {
                        Some("1") => Some(true),
                        Some("0") => Some(false),
                        _ => power.on_ac,
                    };
                }
                Some("Battery") => {
                    power.battery_status = sysfs(p.join("status")).or(power.battery_status);
                    power.battery_capacity = sysfs(p.join("capacity"))
                        .and_then(|c| c.parse().ok())
                        .or(power.battery_capacity);
                }
                _ => {}
            }
        }
    }
    let cpufreq = Path::new("/sys/devices/system/cpu/cpu0/cpufreq");
    power.scaling_governor = sysfs(cpufreq.join("scaling_governor"));
    power.scaling_driver = sysfs(cpufreq.join("scaling_driver"));
    power.platform_profile = sysfs("/sys/firmware/acpi/platform_profile");
    power
}

/// Every Bluetooth adapter this machine has, and whether it is blocked.
///
/// All of them, not just the one the bridge probably used: this process cannot
/// know which adapter another process bound, and recording one as though it
/// were the one in play would be a guess wearing an observation's clothes.
fn read_hci_adapters() -> Vec<HciAdapter> {
    let Ok(entries) = std::fs::read_dir("/sys/class/bluetooth") else {
        return Vec::new();
    };
    let mut out: Vec<HciAdapter> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // `hci0`, not `hci1:256`. The colon-suffixed entries are open
            // connections on an adapter, not adapters — listing them made a
            // two-radio machine report three, with the phantom's rfkill state
            // null because a connection has no rfkill node.
            let is_adapter = name
                .strip_prefix("hci")
                .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()));
            if !is_adapter {
                return None;
            }
            let p = e.path();
            let rfkill = std::fs::read_dir(&p).ok().and_then(|inner| {
                inner.flatten().map(|d| d.path()).find(|d| {
                    d.file_name()
                        .map(|f| f.to_string_lossy().starts_with("rfkill"))
                        .unwrap_or(false)
                })
            });
            let flag = |file: &str| {
                rfkill
                    .as_ref()
                    .and_then(|r| match sysfs(r.join(file)).as_deref() {
                        Some("1") => Some(true),
                        Some("0") => Some(false),
                        _ => None,
                    })
            };
            Some(HciAdapter {
                device_path: std::fs::canonicalize(p.join("device"))
                    .ok()
                    .map(|d| d.display().to_string()),
                soft_blocked: flag("soft"),
                hard_blocked: flag("hard"),
                name,
            })
        })
        .collect();
    // Sorted so two runs on the same machine produce the same document, and a
    // diff between sessions shows a real change rather than readdir order.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Writes the payload first, then the manifest — never the reverse. A manifest
/// visible before the payload it describes would advertise a digest for bytes
/// that are not on disk yet.
fn write_log(
    dir: &std::path::Path,
    session_id: &str,
    turn_file: Option<std::fs::File>,
    recorder: &TurnLogRecorder,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("log dir: {e}"))?;
    let payload_path = dir.join(turn_log_payload_filename(session_id));
    // The payload was written turn by turn. Close it here, before the manifest
    // that describes it: a manifest visible over a payload with buffered bytes
    // still outstanding would advertise a digest for something not fully on
    // disk.
    if let Some(mut f) = turn_file {
        f.flush()
            .map_err(|e| format!("flushing the turn log: {e}"))?;
        drop(f);
    }
    let manifest = recorder.manifest();
    let manifest_path = dir.join(turn_log_manifest_filename(session_id));
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("writing the manifest: {e}"))?;
    eprintln!("● log: {}", payload_path.display());
    Ok(())
}
