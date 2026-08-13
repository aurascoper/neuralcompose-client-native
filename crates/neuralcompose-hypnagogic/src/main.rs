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

use neuralcompose_hypnagogic::command;
use neuralcompose_hypnagogic::dialectic::{DialecticConfig, DialecticLoop};
use neuralcompose_hypnagogic::embedding::Embedding;
use neuralcompose_hypnagogic::http;
use neuralcompose_hypnagogic::loops::{strip_for_speech, MirrorConfig, MirrorLoop};
use neuralcompose_hypnagogic::profile::HypnagogicMode;
use neuralcompose_hypnagogic::role::waking_roles;
use neuralcompose_hypnagogic::seams::{
    GenerationParams, Listening, Prosody, SeamError, SeamResult, SelectionDraws, SentenceEmbedding,
    Speaking, TextGenerating,
};
use neuralcompose_hypnagogic::turn_log::{
    turn_log_manifest_filename, turn_log_payload_filename, TurnLogRecorder,
};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
    speak: bool,
}

const USAGE: &str = "\
neuralcompose-hypnagogic — the four hypnagogic loop modes on Linux

  --mode <mirror|focused|reflective|contemplative>   default: mirror
  --turns <n>                                        default: 1
  --server <url>                                     default: http://127.0.0.1:8080
  --whisper <path>                --whisper-model <path>
  --log                           --log-dir <path>
  --mic                           capture audio instead of reading stdin
  --speak                         synthesize with espeak-ng instead of printing
  --json                          --verify-log <path>
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
        speak: false,
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
            "--mic" => a.mic = true,
            "--speak" => a.speak = true,
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

/// Spawns `pw-record`, waits for Enter, then transcribes with whisper-cli.
struct MicListener {
    whisper: PathBuf,
    model: PathBuf,
    workdir: PathBuf,
}

impl Listening for MicListener {
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

        let argv = command::whisper_argv(&self.whisper, &self.model, &wav);
        let out = Command::new(&argv[0])
            .args(&argv[1..])
            .output()
            .map_err(|e| SeamError::Unavailable(format!("whisper-cli: {e}")))?;
        if !out.status.success() {
            return Err(SeamError::Failed(format!(
                "whisper-cli exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let text = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        Ok(if text.is_empty() { None } else { Some(text) })
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

struct EspeakSpeaker {
    workdir: PathBuf,
}

impl Speaking for EspeakSpeaker {
    fn speak(&mut self, text: &str, prosody: Prosody) -> SeamResult<()> {
        let wav = self.workdir.join("reply.wav");
        let argv = command::espeak_argv(text, &wav, prosody.rate, prosody.pitch_multiplier);
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
        let argv = command::play_argv(&wav);
        Command::new(&argv[0])
            .args(&argv[1..])
            .status()
            .map_err(|e| SeamError::Unavailable(format!("pw-play: {e}")))?;
        Ok(())
    }
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

    if let Err(e) = run(args) {
        eprintln!("error: {e}");
        std::process::exit(1);
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

fn run(args: Args) -> Result<(), String> {
    preflight(&args.server)?;

    let workdir = std::env::temp_dir().join(format!("nc-hypnagogic-{}", std::process::id()));
    std::fs::create_dir_all(&workdir).map_err(|e| format!("workdir: {e}"))?;

    eprintln!("● mode: {} ({})", args.mode.label(), args.mode.id());
    eprintln!("● generation: llama-server at {}", args.server);
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
    let mut payload = String::new();

    let listener: Box<dyn Listening> = if args.mic {
        eprintln!("● input: microphone via pw-record + whisper-cli");
        Box::new(MicListener {
            whisper: args.whisper.clone(),
            model: args.whisper_model.clone(),
            workdir: workdir.clone(),
        })
    } else {
        eprintln!("● input: stdin (pass --mic for the microphone)");
        Box::new(StdinListener)
    };
    let speaker: Box<dyn Speaking> = if args.speak {
        eprintln!("● output: espeak-ng + pw-play");
        Box::new(EspeakSpeaker {
            workdir: workdir.clone(),
        })
    } else {
        eprintln!("● output: stdout (pass --speak to synthesize)");
        Box::new(PrintSpeaker)
    };

    match args.mode.profile() {
        None => {
            let mut l = MirrorLoop::new(
                listener,
                HttpGenerator {
                    server: args.server.clone(),
                },
                speaker,
                MirrorConfig::default(),
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
            let embedder = build_embedder()?;
            let mut l = DialecticLoop::new(
                listener,
                HttpGenerator {
                    server: args.server.clone(),
                },
                speaker,
                embedder,
                SystemDraws,
                waking_roles().to_vec(),
                profile,
                DialecticConfig::default(),
            );
            for turn_number in 0..args.turns {
                match l.turn() {
                    Ok(Some(t)) => {
                        if args.json {
                            println!(
                                "{}",
                                serde_json::to_string(&t.to_turn_line(args.mode.id())).unwrap()
                            );
                        }
                        if let Some(r) = recorder.as_mut() {
                            payload.push_str(&r.on_turn(&t.to_turn_line(args.mode.id())));
                            payload.push('\n');
                        }
                        if let Some(err) = &t.witness_error {
                            eprintln!("witness failed on turn {}: {err}", t.index);
                        }
                    }
                    Ok(None) => eprintln!("turn {turn_number} skipped (nothing heard)"),
                    Err(e) => eprintln!("turn failed: {e}"),
                }
                if turn_number + 1 < args.turns {
                    std::thread::sleep(profile.inter_turn_delay());
                }
            }
        }
    }

    if let Some(r) = recorder {
        write_log(&args.log_dir, &session_id, &payload, &r)?;
    }
    Ok(())
}

fn build_embedder() -> Result<CpuEmbedder, String> {
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
    // Self-reporting ceiling: this line is the claim that no Vulkan context is
    // created by this process, made where it can be read rather than left to a
    // manual `fuser -v /dev/dri/renderD128`.
    eprintln!(
        "● embeddings: {} (CPU backend — this process creates NO Vulkan context)",
        neuralcompose_llama::BACKEND_ID_CPU
    );
    Ok(CpuEmbedder {
        inner,
        model_id: model,
    })
}

/// Writes the payload first, then the manifest — never the reverse. A manifest
/// visible before the payload it describes would advertise a digest for bytes
/// that are not on disk yet.
fn write_log(
    dir: &std::path::Path,
    session_id: &str,
    payload: &str,
    recorder: &TurnLogRecorder,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("log dir: {e}"))?;
    let payload_path = dir.join(turn_log_payload_filename(session_id));
    std::fs::write(&payload_path, payload).map_err(|e| format!("writing the turn log: {e}"))?;
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
