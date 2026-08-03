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
    next_reconnect, ChannelHealthThresholds, MonitorConfig, ReconnectDecision, SocketEvent,
    StreamMonitor,
};

const DEFAULT_URL: &str = "ws://127.0.0.1:8788/api/eeg/stream";

struct Args {
    url: String,
    seconds: u64,
    status_every_ms: u64,
    json: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        url: DEFAULT_URL.to_string(),
        seconds: 0,
        status_every_ms: 1000,
        json: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--url" => a.url = it.next().ok_or("--url needs a value")?,
            "--seconds" => {
                a.seconds = it
                    .next()
                    .ok_or("--seconds needs a value")?
                    .parse()
                    .map_err(|_| "--seconds must be an integer")?
            }
            "--status-every-ms" => {
                a.status_every_ms = it
                    .next()
                    .ok_or("--status-every-ms needs a value")?
                    .parse()
                    .map_err(|_| "--status-every-ms must be an integer")?
            }
            "--json" => a.json = true,
            "-h" | "--help" => {
                println!(
                    "neuralcompose-headless — drive the core against a live EEG stream\n\n\
                     --url <ws://…>          default {DEFAULT_URL}\n\
                     --seconds <n>           stop after n seconds (0 = run until interrupted)\n\
                     --status-every-ms <n>   status cadence, default 1000\n\
                     --json                  emit one JSON status object per tick\n"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unrecognised argument {other:?}")),
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
                emit_status(&monitor, t, frames_total, args.json);
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
    emit_status(&monitor, t, frames_total, args.json);
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
        let names = ["TP9", "AF7", "AF8", "TP10"];
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
