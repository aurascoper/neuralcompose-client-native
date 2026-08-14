//! Headless Linux runtime: the third shell.
//!
//! Connects to the frozen `/api/eeg/stream` wire contract, feeds every frame to
//! the real `StreamMonitor`, and prints what the core reports. No app, no GUI,
//! no fourth UI surface to keep in parity.
//!
//! WHAT THIS IS FOR
//!
//! `docs/support-matrix.md` records every row as `Contracted` — schema and
//! deterministic Rust behaviour exist, and nothing has ever executed. Making the
//! core executable on a third platform is a prerequisite for ever leaving that
//! state, and this is the first process that does it.
//!
//! THE DIVISION OF LABOUR IS THE POINT
//!
//! `lib.rs` states that shells own all I/O and feed the core raw payloads plus
//! **monotonic** `now_ms`. So this binary owns exactly four things — the socket,
//! the clock, the retry timer, and stdout — and the core owns everything else:
//! decoding, per-channel buffering, freshness, phase, and the reconnect
//! decision. Every number printed below is computed by the core, not here. If
//! this file started deciding when a stream is stale, the shell boundary would
//! be broken and the evidence would be worthless.
//!
//! It is a separate crate for the same reason: the core's contract is that it
//! has no I/O dependencies, and a socket client in its Cargo.toml would end that
//! whether or not any core module imported it.
//!
//! NOT CLAIMED, AND THIS MATTERS MORE THAN WHAT IS
//!
//! Running this **promotes no support-matrix row**. Every one of the eleven rows
//! is an (OS, architecture, *backend*) triple — `llama-cpp-cpu`, `coreml`,
//! `windows-ml-qnn` and so on — and this binary executes no backend, links no
//! accelerator library and loads no model. `RuntimeSmokeValidated` requires a
//! deterministic fixture MODEL to have run; a fixture EEG stream is not that.
//!
//! What it does prove is narrower and real: the core's streaming path executes
//! in a live process on linux/x86_64, against a socket, with an injected
//! monotonic clock. The matrix has no row for that, which is itself worth
//! noticing — the register tracks model backends and says nothing about whether
//! the ingest path runs anywhere.
//!
//! `BuildValidated` for the linux rows is a separate and smaller claim, already
//! substantiated by CI compiling on named runners. Reading this binary's
//! existence as evidence for a backend row would be promotion by implication,
//! which ADR-002 exists to forbid.

use std::io::Write;
use std::time::{Duration, Instant};

use neuralcompose_mobile_core::presentation::StreamPhase;
use neuralcompose_mobile_core::{
    assess_channel, common_mode_hint, next_reconnect, ChannelHealthThresholds, ElectrodeReport,
    MainsThresholds, MonitorConfig, ReconnectDecision, SocketEvent, StreamMonitor,
};

const DEFAULT_URL: &str = "ws://127.0.0.1:8788/api/eeg/stream";

/// The Muse S rate, and the rate the frozen wire contract is served at by both
/// `tools/muse-ble-bridge` and `tools/fixture-eeg-server`. The contract carries
/// timestamps but not a declared rate, so this is an assumption of the shell
/// rather than something the core is told — and it matters only for the mains
/// check, which needs Hz to locate a band. A wrong value here would move the
/// 50/60 Hz window and silently mis-locate the line.
const SAMPLE_RATE_HZ: f64 = 256.0;

struct Args {
    url: String,
    /// Text to embed. `Some` puts the binary in embed mode: no socket, no
    /// headband — it exercises the model backend and exits.
    embed: Option<String>,
    model: Option<String>,
    gpu_layers: u32,
    devices: bool,
    bench: bool,
    bench_iters: u32,
    threads: u32,
    sweep: bool,
    seconds: u64,
    status_every_ms: u64,
    json: bool,
    check: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        url: DEFAULT_URL.to_string(),
        embed: None,
        model: None,
        gpu_layers: 0,
        devices: false,
        bench: false,
        bench_iters: 30,
        threads: 0,
        sweep: false,
        seconds: 0,
        status_every_ms: 1000,
        json: false,
        check: false,
    };
    let (mut saw_seconds, mut saw_cadence) = (false, false);
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--url" => a.url = it.next().ok_or("--url needs a value")?,
            "--seconds" => {
                saw_seconds = true;
                a.seconds = it
                    .next()
                    .ok_or("--seconds needs a value")?
                    .parse()
                    .map_err(|_| "--seconds must be an integer")?
            }
            "--status-every-ms" => {
                saw_cadence = true;
                a.status_every_ms = it
                    .next()
                    .ok_or("--status-every-ms needs a value")?
                    .parse()
                    .map_err(|_| "--status-every-ms must be an integer")?
            }
            "--json" => a.json = true,
            "--check" => a.check = true,
            "--embed" => a.embed = Some(it.next().ok_or("--embed needs text")?),
            "--model" => a.model = Some(it.next().ok_or("--model needs a path")?),
            "--gpu-layers" => {
                a.gpu_layers = it
                    .next()
                    .ok_or("--gpu-layers needs a value")?
                    .parse()
                    .map_err(|_| "--gpu-layers must be a non-negative integer")?
            }
            "--devices" => a.devices = true,
            "--bench" => a.bench = true,
            "--sweep" => a.sweep = true,
            "--threads" => {
                a.threads = it
                    .next()
                    .ok_or("--threads needs a value")?
                    .parse()
                    .map_err(|_| "--threads must be a non-negative integer")?
            }
            "--bench-iters" => {
                a.bench_iters = it
                    .next()
                    .ok_or("--bench-iters needs a value")?
                    .parse()
                    .map_err(|_| "--bench-iters must be a positive integer")?
            }
            "-h" | "--help" => {
                println!(
                    "neuralcompose-headless — drive the core against a live EEG stream\n\n\
                     --url <ws://…>          default {DEFAULT_URL}\n\
                     --seconds <n>           stop after n seconds (0 = run until interrupted)\n\
                     --status-every-ms <n>   status cadence, default 1000\n\
                     --json                  emit one JSON status object per tick\n\
                     --check                 electrode fitting check: per-channel cause and\n\
                     \x20                       what to do, updating live while you adjust the\n\
                     \x20                       band. Exits non-zero if a channel needs fixing.\n\
                     \x20                       Defaults to 20s at a 2s cadence.\n\
                     --embed <text>          embed text with the llama-cpp-cpu backend and exit\n\
                     --model <path>          GGUF model for --embed\n\
                     --gpu-layers <n>        offload n layers to an accelerator (0 = CPU)\n\
                     --devices               list the compute devices ggml can see, and exit\n\
                     --bench                 time CPU vs accelerator across input lengths\n\
                     --sweep                 fine TOKEN sweep x n_ubatch, to separate a dispatch\n\
                     \x20                       boundary from smooth bandwidth saturation\n\
                     --bench-iters <n>       measured iterations per cell, default 30\n\
                     --threads <n>           CPU threads (0 = all cores; llama.cpp's own\n\
                     \x20                       default is a hard-coded 4)\n"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unrecognised argument {other:?}")),
        }
    }
    // Check mode is a fitting aid, so it wants a bounded run and a cadence slow
    // enough to react to. Explicit flags still win — this only fills defaults.
    if a.check {
        if !saw_seconds {
            a.seconds = 20;
        }
        if !saw_cadence {
            a.status_every_ms = 2000;
        }
    }
    Ok(a)
}

/// The shell owns the clock. `Instant` is monotonic, which is what the core
/// requires — a wall clock would let an NTP step masquerade as stream staleness.
fn now_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

fn phase_str(p: &StreamPhase) -> String {
    match p {
        StreamPhase::Connecting => "connecting".into(),
        StreamPhase::OpenNoData => "open-no-data".into(),
        StreamPhase::Live => "live".into(),
        StreamPhase::Stale { age_ms } => format!("stale({age_ms}ms)"),
        StreamPhase::Closed => "closed".into(),
        StreamPhase::Error => "error".into(),
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\ntry --help");
            std::process::exit(2);
        }
    };

    if args.devices {
        std::process::exit(run_devices());
    }

    if args.sweep {
        std::process::exit(run_sweep(
            args.model.as_deref(),
            args.bench_iters,
            args.threads,
        ));
    }

    if args.bench {
        std::process::exit(run_bench(
            args.model.as_deref(),
            args.bench_iters,
            args.threads,
        ));
    }

    // Embed mode short-circuits everything below: it touches no socket and no
    // headband, because the model backend and the EEG ingest path are separate
    // claims and running them together would blur which one any evidence is
    // about.
    if let Some(text) = args.embed.clone() {
        std::process::exit(run_embed(&text, args.model.as_deref(), args.gpu_layers));
    }

    let start = Instant::now();
    let monitor = StreamMonitor::new(MonitorConfig::default());
    let deadline = (args.seconds > 0).then(|| Duration::from_secs(args.seconds));

    eprintln!("neuralcompose-headless → {}", args.url);
    eprintln!("core owns decode/buffer/phase; this shell owns socket, clock, retry, stdout");

    let mut attempts: u32 = 0;
    let mut last_status = 0u64;
    let mut frames_total: u64 = 0;

    'outer: loop {
        if let Some(d) = deadline {
            if start.elapsed() >= d {
                break;
            }
        }

        monitor.on_socket_event(SocketEvent::Connecting, now_ms(start));
        let connect = tungstenite::connect(&args.url);

        let mut socket = match connect {
            Ok((s, _resp)) => {
                monitor.on_socket_event(SocketEvent::Opened, now_ms(start));
                attempts = 0;
                eprintln!("connected");
                s
            }
            Err(e) => {
                monitor.on_socket_event(SocketEvent::Errored, now_ms(start));
                eprintln!("connect failed: {e}");
                // The CORE decides whether to retry and for how long; this shell
                // only sleeps. Putting the backoff policy here would duplicate
                // logic the core already owns and tests.
                attempts += 1;
                match next_reconnect(attempts) {
                    ReconnectDecision::RetryAfterMs { delay_ms } => {
                        eprintln!("retrying in {delay_ms}ms (attempt {attempts})");
                        std::thread::sleep(Duration::from_millis(delay_ms));
                        continue 'outer;
                    }
                    ReconnectDecision::GiveUp => {
                        eprintln!("giving up after {attempts} attempts");
                        break 'outer;
                    }
                }
            }
        };

        loop {
            if let Some(d) = deadline {
                if start.elapsed() >= d {
                    break 'outer;
                }
            }

            match socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    // The ONLY place a payload crosses into the core.
                    let accepted = monitor.on_frame(text.to_string(), now_ms(start));
                    frames_total += 1;
                    let _ = accepted;
                }
                Ok(tungstenite::Message::Close(_)) => {
                    monitor.on_socket_event(SocketEvent::Closed, now_ms(start));
                    eprintln!("socket closed by peer");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    monitor.on_socket_event(SocketEvent::Errored, now_ms(start));
                    eprintln!("read error: {e}");
                    break;
                }
            }

            let t = now_ms(start);
            if t.saturating_sub(last_status) >= args.status_every_ms {
                last_status = t;
                if args.check {
                    emit_check(&monitor, t);
                } else {
                    emit_status(&monitor, t, frames_total, args.json);
                }
            }
        }

        attempts += 1;
        match next_reconnect(attempts) {
            ReconnectDecision::RetryAfterMs { delay_ms } => {
                eprintln!("reconnecting in {delay_ms}ms (attempt {attempts})");
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
            ReconnectDecision::GiveUp => {
                eprintln!("giving up after {attempts} attempts");
                break;
            }
        }
    }

    let t = now_ms(start);
    if args.check {
        emit_check(&monitor, t);
    } else {
        emit_status(&monitor, t, frames_total, args.json);
    }
    let snap = monitor.stream_snapshot(t);
    eprintln!(
        "\nsummary: {} frames, {} samples accepted, {} connection generation(s), final phase {}",
        frames_total,
        snap.total_received,
        snap.connection_generation,
        phase_str(&snap.phase)
    );
    // Non-zero when nothing was ever accepted: a run that connects and receives
    // nothing must not look like a successful one in CI or in an acceptance doc.
    if snap.total_received == 0 {
        eprintln!("no samples were accepted — not evidence of a working runtime");
        std::process::exit(1);
    }

    if args.check {
        let reports = assess_all(&monitor);
        let bad: Vec<&str> = reports
            .iter()
            .zip(CHANNEL_NAMES)
            .filter(|(r, _)| r.verdict.is_blocking())
            .map(|(_, n)| n)
            .collect();
        // `unknown` is counted as not-ready separately from blocking: a channel
        // nothing could be measured on has not passed, and reporting it as
        // passing is the one failure mode that would make this check worse than
        // no check at all.
        let unmeasured: Vec<&str> = reports
            .iter()
            .zip(CHANNEL_NAMES)
            .filter(|(r, _)| r.verdict == neuralcompose_mobile_core::ElectrodeVerdict::Unknown)
            .map(|(_, n)| n)
            .collect();
        if bad.is_empty() && unmeasured.is_empty() {
            eprintln!("electrode check: PASS — all four channels usable");
        } else {
            if !bad.is_empty() {
                eprintln!("electrode check: FAIL — fix {}", bad.join(", "));
            }
            if !unmeasured.is_empty() {
                eprintln!(
                    "electrode check: {} could not be measured",
                    unmeasured.join(", ")
                );
            }
            std::process::exit(3);
        }
    }
}

const CHANNEL_NAMES: [&str; 4] = ["TP9", "AF7", "AF8", "TP10"];

/// Assess every channel from the core's own buffered samples.
///
/// The window is whatever the core is holding — roughly five seconds at 256 Hz
/// — which is deliberately the RECENT past, because the point of check mode is
/// to react while the user's hands are on the headband. `band_power` refuses
/// below one second, so an early tick reports `unknown` rather than guessing.
fn assess_all(monitor: &StreamMonitor) -> Vec<ElectrodeReport> {
    let ch = monitor.snapshot();
    ch.channels
        .iter()
        .map(|c| {
            assess_channel(
                c,
                SAMPLE_RATE_HZ,
                ChannelHealthThresholds::default(),
                MainsThresholds::default(),
            )
        })
        .collect()
}

fn emit_check(monitor: &StreamMonitor, now: u64) {
    let snap = monitor.stream_snapshot(now);
    println!(
        "\n[{:>6}ms] {} — {} samples",
        now,
        phase_str(&snap.phase),
        snap.total_received
    );
    let reports = assess_all(monitor);
    for (r, name) in reports.iter().zip(CHANNEL_NAMES) {
        // Matched as a pair: the power and the frequency are `Some` together,
        // and printing a power without the line it belongs to would show a
        // number for a band nothing established.
        let line = match (r.mains_power, r.line_hz) {
            (Some(p), Some(f)) => format!("{p:>9.1} @{f:.0}Hz"),
            _ => "        —".to_string(),
        };
        let mark = if r.verdict.is_blocking() { "✗" } else { " " };
        println!(
            "  {mark} {name:<5} {:7.1} µV  mains {line}  {:<17} {}",
            r.rms,
            r.verdict.as_str(),
            r.verdict.advice()
        );
    }
    // A cross-channel pattern no single verdict can express: every per-channel
    // line says "reseat this pad", which is the wrong advice when the fault is
    // shared by all of them.
    if let Some(hint) = common_mode_hint(&reports) {
        println!("  ! {hint}");
    }
    let _ = std::io::stdout().flush();
}

fn emit_status(monitor: &StreamMonitor, now: u64, frames: u64, json: bool) {
    let snap = monitor.stream_snapshot(now);
    let pres = monitor.presentation(now);
    let ch = monitor.snapshot();
    // Per-channel RMS is computed here from the core's own buffered arrays, in
    // fixed TP9/AF7/AF8/TP10 order — the channel identity the core treats as
    // anatomy rather than slots.
    let rms: Vec<f64> = ch
        .channels
        .iter()
        .map(|c| {
            if c.is_empty() {
                0.0
            } else {
                (c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64).sqrt()
            }
        })
        .collect();

    if json {
        println!(
            "{{\"t_ms\":{},\"frames\":{},\"samples\":{},\"generation\":{},\"cached\":{},\
             \"phase\":\"{}\",\"silent_banner\":{},\"disconnected_banner\":{},\
             \"rms\":[{}],\"health\":[{}]}}",
            now,
            frames,
            snap.total_received,
            snap.connection_generation,
            snap.cached_sample_count,
            phase_str(&snap.phase),
            pres.show_silent_banner,
            pres.show_disconnected_banner,
            rms.iter()
                .map(|v| format!("{v:.3}"))
                .collect::<Vec<_>>()
                .join(","),
            {
                let th = ChannelHealthThresholds::default();
                rms.iter()
                    .zip(ch.channels.iter())
                    .map(|(v, c)| format!("\"{}\"", th.status(*v, c.len() as u64).as_str()))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        );
    } else {
        let names = CHANNEL_NAMES;
        // The CORE classifies; this shell only prints. Before the thresholds
        // were ported, a run showing 513 and 881 uV read as "high" to a human
        // eye when the repo's own committed rule called both saturated.
        let th = ChannelHealthThresholds::default();
        let bars: Vec<String> = rms
            .iter()
            .zip(names)
            .zip(ch.channels.iter())
            .map(|((v, n), c)| {
                let st = th.status(*v, c.len() as u64);
                format!("{n} {v:7.2} {}", st.as_str())
            })
            .collect();
        println!(
            "[{:>7}ms] {:<14} samples {:>7} cached {:>5} | {}",
            now,
            phase_str(&snap.phase),
            snap.total_received,
            snap.cached_sample_count,
            bars.join("  ")
        );
    }
    let _ = std::io::stdout().flush();
}

/// Embed one string and print the result.
///
/// Prints the backend identifiers and the llama.cpp commit alongside the
/// vector, because ADR-002's runtime rungs require a NAMED backend version and
/// a number without its provenance is not evidence. The commit is read from the
/// checkout at build time, so it cannot drift from the binary the way a
/// hand-copied hash in a document can.
fn run_devices() -> i32 {
    if !neuralcompose_llama::is_available() {
        eprintln!("no model backend in this build (built without LLAMA_CPP_DIR)");
        return 4;
    }
    let devices = neuralcompose_llama::devices();
    if devices.is_empty() {
        println!("no compute devices enumerated");
        return 4;
    }
    for (i, d) in devices.iter().enumerate() {
        println!(
            "  [{i}] {:<12} {:?}{}  {}",
            d.name,
            d.kind,
            if d.kind.is_accelerator() { " *" } else { "  " },
            d.description
        );
    }
    0
}

fn run_embed(text: &str, model: Option<&str>, gpu_layers: u32) -> i32 {
    use neuralcompose_llama::{Embedder, BACKEND_COMMIT, RUNTIME_ABI};

    if !neuralcompose_llama::is_available() {
        eprintln!(
            "no model backend in this build (built without LLAMA_CPP_DIR).\n\
             rebuild with LLAMA_CPP_DIR=<llama.cpp checkout> to enable --embed."
        );
        return 4;
    }
    let path = match model {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join("models/bge-small-en-v1.5-f32.gguf")
        }
    };
    if !path.exists() {
        eprintln!("no model at {} — pass --model <path.gguf>", path.display());
        return 4;
    }

    eprintln!("runtime {RUNTIME_ABI} @ llama.cpp {BACKEND_COMMIT}");
    eprintln!("model {}", path.display());

    let mut embedder = match Embedder::load_with(&path, 512, gpu_layers) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("load failed: {e}");
            return 4;
        }
    };
    // Reported AFTER load, from the outcome rather than the request: asking for
    // offload does not prove an accelerator was found, and a silent CPU
    // fallback filed as Vulkan evidence would make a matrix row wrong.
    match embedder.accelerator() {
        Some(d) => eprintln!(
            "backend {} on {} ({})",
            embedder.backend_id(),
            d.name,
            d.description
        ),
        None if gpu_layers > 0 => eprintln!(
            "backend {} — {gpu_layers} layers requested but NO accelerator enumerated",
            embedder.backend_id()
        ),
        None => eprintln!("backend {}", embedder.backend_id()),
    }

    match embedder.embed(text) {
        Ok(v) => {
            eprintln!("dimensions {}", v.len());
            let head: Vec<String> = v.iter().take(8).map(|x| format!("{x:.7}")).collect();
            println!("[{}, ...]", head.join(", "));
            0
        }
        Err(e) => {
            eprintln!("embed failed: {e}");
            4
        }
    }
}

/// Resolve the fixture model path shared by `--embed` and `--bench`.
fn default_model(model: Option<&str>) -> std::path::PathBuf {
    match model {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join("models/bge-small-en-v1.5-f32.gguf")
        }
    }
}

/// Deterministic input of approximately `words` words.
///
/// Built from a fixed vocabulary with no randomness, so two runs of the
/// benchmark measure the same work. Input LENGTH is the variable that matters
/// here: an accelerator pays a fixed dispatch cost per call, so a short input
/// measures overhead and a long one measures throughput. Reporting only one
/// would let either backend look better than it is.
fn bench_text(words: usize) -> String {
    const VOCAB: [&str; 8] = [
        "signal",
        "electrode",
        "cortex",
        "rhythm",
        "threshold",
        "channel",
        "spectrum",
        "artifact",
    ];
    let mut s = String::new();
    for i in 0..words {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(VOCAB[i % VOCAB.len()]);
    }
    s
}

/// Time CPU against the accelerator across three input lengths.
fn run_bench(model: Option<&str>, iters: u32, threads: u32) -> i32 {
    use std::time::Instant;

    use neuralcompose_llama::{devices, Embedder};

    if !neuralcompose_llama::is_available() {
        eprintln!("no model backend in this build (built without LLAMA_CPP_DIR)");
        return 4;
    }
    let path = default_model(model);
    if !path.exists() {
        eprintln!("no model at {} — pass --model <path.gguf>", path.display());
        return 4;
    }
    if iters == 0 {
        eprintln!("--bench-iters must be at least 1");
        return 2;
    }

    let has_accel = devices().iter().any(|d| d.kind.is_accelerator());

    // bge-small's trained context is 512 tokens. The long case is sized to
    // approach that without exceeding it; going over would silently truncate
    // and make the two backends do different amounts of work.
    let cases: [(&str, usize); 3] = [("short", 3), ("medium", 60), ("long", 300)];

    // One measurement: median and p90 over `iters`, after warmup.
    //
    // Median rather than mean because a single scheduler preemption or page
    // fault skews a mean and says nothing about the backend. p90 is reported
    // alongside so a long tail is visible rather than hidden by the median.
    let measure = |e: &mut Embedder, text: &str| -> Option<(f64, f64)> {
        for _ in 0..3 {
            e.embed(text).ok()?;
        }
        let mut samples = Vec::with_capacity(iters as usize);
        for _ in 0..iters {
            let t0 = Instant::now();
            e.embed(text).ok()?;
            samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).expect("no NaN from a clock"));
        let median = samples[samples.len() / 2];
        let p90 = samples[(samples.len() * 9 / 10).min(samples.len() - 1)];
        Some((median, p90))
    };

    // A CPU baseline on llama.cpp's default 4 threads would measure a handicap
    // rather than a backend, so 0 means "every core this machine has".
    let threads = if threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4)
    } else {
        threads
    };

    let mut cpu = match Embedder::load_full(&path, 512, 0, threads) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cpu load failed: {e}");
            return 4;
        }
    };
    let mut gpu = if has_accel {
        match Embedder::load_full(&path, 512, 99, threads) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("accelerator load failed: {e}");
                return 4;
            }
        }
    } else {
        None
    };

    eprintln!(
        "model {} | {iters} iterations per cell, 3 warmup, median of sorted samples",
        path.display()
    );
    eprintln!(
        "cpu threads requested {threads}, llama.cpp reports {} active",
        cpu.active_threads()
    );
    match &gpu {
        Some(g) => eprintln!(
            "cpu = {} | accel = {} on {}",
            cpu.backend_id(),
            g.backend_id(),
            g.accelerator().map(|d| d.name.as_str()).unwrap_or("?")
        ),
        None => eprintln!("cpu = {} | no accelerator enumerated", cpu.backend_id()),
    }

    println!(
        "\n  {:<8} {:>6} {:>12} {:>9} {:>12} {:>9} {:>9}",
        "case", "words", "cpu ms", "p90", "accel ms", "p90", "speedup"
    );
    println!("  {}", "-".repeat(70));

    for (name, words) in cases {
        let text = bench_text(words);
        let Some((c_med, c_p90)) = measure(&mut cpu, &text) else {
            eprintln!("cpu embed failed on {name}");
            return 4;
        };
        match gpu.as_mut() {
            Some(g) => {
                let Some((g_med, g_p90)) = measure(g, &text) else {
                    eprintln!("accelerator embed failed on {name}");
                    return 4;
                };
                println!(
                    "  {name:<8} {words:>6} {c_med:>12.3} {c_p90:>9.3} {g_med:>12.3} {g_p90:>9.3} {:>8.2}x",
                    c_med / g_med
                );
            }
            None => println!(
                "  {name:<8} {words:>6} {c_med:>12.3} {c_p90:>9.3} {:>12} {:>9} {:>9}",
                "—", "—", "—"
            ),
        }
    }
    // Speedup above 1 means the accelerator is faster. Stated explicitly
    // because the column is a ratio and ratios invite being read backwards.
    println!("\n  speedup = cpu median / accel median; >1 means the accelerator wins");
    0
}

/// Build text whose TOKEN count is exactly `target`, verified against the model.
///
/// The benchmark's word-based cases carry a hidden variable: word-to-token
/// ratio depends on content, so a band stated in words smears any boundary that
/// exists in tokens. The kernel and the batching logic see tokens, so the sweep
/// is expressed in tokens and the count is confirmed with the model's own
/// tokenizer rather than estimated.
fn text_of_tokens(e: &neuralcompose_llama::Embedder, target: usize) -> Option<String> {
    const VOCAB: [&str; 8] = [
        "signal",
        "electrode",
        "cortex",
        "rhythm",
        "threshold",
        "channel",
        "spectrum",
        "artifact",
    ];
    // Grow one word at a time until the tokenizer reports `target`. Every word
    // above is a single WordPiece token for this vocab, but that is checked
    // rather than assumed — a multi-token word would overshoot and the loop
    // reports failure instead of silently measuring the wrong length.
    let mut words: Vec<&str> = Vec::new();
    for i in 0..target * 2 {
        words.push(VOCAB[i % VOCAB.len()]);
        let text = words.join(" ");
        match e.token_count(&text) {
            Some(n) if n == target => return Some(text),
            Some(n) if n > target => return None,
            Some(_) => {}
            None => return None,
        }
    }
    None
}

/// Fine token sweep crossed with `n_ubatch`.
///
/// THE QUESTION. `--bench` shows a speedup that peaks mid-range and falls at
/// the long case, reproducibly, even though attention is quadratic and the gap
/// should widen. Two hypotheses make opposite predictions under one
/// manipulation:
///
/// - **Dispatch boundary.** llama.cpp splits a batch into micro-batches of
///   `n_ubatch`; each submit costs a fixed round-trip that the CPU path does
///   not pay. Prediction: the drop is a STEP at a token count, and the step
///   MOVES when `n_ubatch` moves.
/// - **Bandwidth saturation.** The iGPU shares memory bandwidth with the CPU.
///   Prediction: the drop is SMOOTH and does NOT move with `n_ubatch`.
///
/// Neither outcome is ambiguous, which is why this is one manipulation rather
/// than a parameter hunt.
fn run_sweep(model: Option<&str>, iters: u32, threads: u32) -> i32 {
    use std::time::Instant;

    use neuralcompose_llama::{devices, Embedder};

    if !neuralcompose_llama::is_available() {
        eprintln!("no model backend in this build");
        return 4;
    }
    let path = default_model(model);
    if !path.exists() {
        eprintln!("no model at {}", path.display());
        return 4;
    }
    if !devices().iter().any(|d| d.kind.is_accelerator()) {
        eprintln!("no accelerator — this sweep compares CPU against one");
        return 4;
    }
    let threads = if threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4)
    } else {
        threads
    };

    // bge-small's trained context is 512 and its position embeddings are
    // learned, so beyond that the model is out of distribution and the timing
    // would not describe anything meaningful. The sweep stops there.
    const N_CTX: u32 = 512;
    // Dense at the low end: the peak is there, not mid-range. The word-based
    // benchmark's "short" case was ~5 tokens and its "medium" ~62, which left
    // the entire rise between them unmeasured.
    let token_targets: [usize; 14] = [4, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512];
    // n_ubatch CANNOT be swept for this model, and the reason is structural
    // rather than empirical: llama.cpp asserts
    //   GGML_ASSERT(cparams.n_ubatch >= n_tokens && "encoder requires n_ubatch >= n_tokens")
    // A bidirectional encoder cannot be micro-batched — every token attends to
    // every other — so there is no dispatch boundary to cross and no boundary
    // hypothesis to test. Requesting a smaller n_ubatch does not produce a
    // different dispatch pattern; it aborts the process.
    //
    // The three legal columns measured before that abort were 5.41 / 5.45 /
    // 5.47 at 32 tokens: identical, as they must be. Kept as a single column.
    let ubatches: [u32; 1] = [512];

    let probe = match Embedder::load_tuned(&path, N_CTX, 0, threads, 0) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("probe load failed: {e}");
            return 4;
        }
    };
    let texts: Vec<(usize, String)> = token_targets
        .iter()
        .filter_map(|t| text_of_tokens(&probe, *t).map(|s| (*t, s)))
        .collect();
    drop(probe);
    if texts.len() < token_targets.len() {
        eprintln!(
            "only built {} of {} exact token lengths",
            texts.len(),
            token_targets.len()
        );
    }

    let measure = |e: &mut Embedder, text: &str| -> Option<f64> {
        for _ in 0..3 {
            e.embed(text).ok()?;
        }
        let mut s = Vec::with_capacity(iters as usize);
        for _ in 0..iters {
            let t0 = Instant::now();
            e.embed(text).ok()?;
            s.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        s.sort_by(|a, b| a.partial_cmp(b).expect("no NaN from a clock"));
        Some(s[s.len() / 2])
    };

    let mut cpu = match Embedder::load_tuned(&path, N_CTX, 0, threads, 0) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cpu load failed: {e}");
            return 4;
        }
    };
    eprintln!(
        "model {} | {iters} iters/cell | cpu threads {} | n_ctx {N_CTX}",
        path.display(),
        cpu.active_threads()
    );

    print!("\n  {:>7}", "tokens");
    for u in ubatches {
        print!(" {:>16}", format!("ub={u} speedup"));
    }
    println!("  {:>10}", "cpu ms");
    println!("  {}", "-".repeat(7 + 17 * ubatches.len() + 12));

    for (tokens, text) in &texts {
        let Some(c) = measure(&mut cpu, text) else {
            eprintln!("cpu embed failed at {tokens} tokens");
            return 4;
        };
        print!("  {tokens:>7}");
        for u in ubatches {
            // Skip rather than abort: llama.cpp treats n_ubatch < n_tokens as a
            // fatal assertion for encoders, so an out-of-range cell would kill
            // the process mid-sweep and lose every row already measured.
            if (u as usize) < *tokens {
                print!(" {:>16}", "n/a");
                continue;
            }
            let mut g = match Embedder::load_tuned(&path, N_CTX, 99, threads, u) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("accel load failed at ubatch {u}: {e}");
                    return 4;
                }
            };
            // VERIFY the swept parameter took effect. A silently-ignored
            // n_ubatch would give three identical columns, which reads exactly
            // like the boundary hypothesis being refuted.
            let active = g.active_ubatch();
            if active != u as i32 {
                eprintln!("\nn_ubatch {u} requested but {active} active — sweep is invalid");
                return 4;
            }
            match measure(&mut g, text) {
                Some(gm) => print!(" {:>16.2}", c / gm),
                None => {
                    eprintln!("accel embed failed at {tokens} tokens, ubatch {u}");
                    return 4;
                }
            }
        }
        println!("  {c:>10.2}");
    }
    println!(
        "\n  n_ubatch is not a free variable for an encoder: llama.cpp asserts\n  \
         n_ubatch >= n_tokens, so the batch is never split and no dispatch\n  \
         boundary exists to explain a step."
    );
    0
}
