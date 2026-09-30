use neuralcompose_hypnagogic::{
    turn_log::{verify_turn_log, TurnLine, TurnLogManifest, TurnLogVerdict},
    visual::VisualPacket,
};
use neuralcompose_mobile_core::capture::{
    verify_capture, CaptureLine, CaptureManifest, ReplayVerdict,
};
use neuralcompose_mobile_core::{EEGSample, ReconnectDecision, SocketEvent, StreamMonitor};
use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub enum Source {
    Demo(String),
    Live(String),
    Replay(Vec<CaptureLine>),
}
pub fn capture(path: &Path) -> Result<Vec<CaptureLine>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let name = path.file_name().unwrap().to_string_lossy();
    let stem = name
        .strip_suffix(".eeg.jsonl")
        .ok_or("replay requires <id>.eeg.jsonl and its manifest")?;
    let manifest: CaptureManifest = serde_json::from_slice(
        &std::fs::read(path.with_file_name(format!("{stem}.eeg.manifest.json")))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if !matches!(
        verify_capture(raw.clone(), manifest),
        ReplayVerdict::Verified { .. }
    ) {
        return Err("capture verification failed".into());
    }
    raw.lines()
        .map(|line| serde_json::from_str(line).map_err(|e| e.to_string()))
        .collect()
}
pub fn context(path: &Path) -> Result<Vec<TurnLine>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let name = path.file_name().unwrap().to_string_lossy();
    let stem = name
        .strip_suffix(".turns.jsonl")
        .ok_or("context requires <id>.turns.jsonl and its manifest")?;
    let manifest: TurnLogManifest = serde_json::from_slice(
        &std::fs::read(path.with_file_name(format!("{stem}.manifest.json")))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if !matches!(
        verify_turn_log(&raw, &manifest),
        TurnLogVerdict::Verified { .. }
    ) {
        return Err("turn log verification failed".into());
    }
    let mut lines: Vec<TurnLine> = raw
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if lines.len() > 16 {
        lines.drain(..lines.len() - 16);
    }
    Ok(lines)
}
fn now(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}
pub fn run(source: Source, monitor: Arc<StreamMonitor>, epoch: Instant, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || match source {
        Source::Demo(kind) => {
            monitor.on_socket_event(SocketEvent::Opened, now(epoch));
            let mut index = 0;
            let mut next = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let samples: Vec<EEGSample> = (index..index + 8)
                    .map(|i| neuralcompose_viz::signal::fixture(i, &kind))
                    .collect();
                monitor.on_frame(serde_json::to_string(&samples).unwrap(), now(epoch));
                index += 8;
                next += Duration::from_micros(31250);
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
        }
        Source::Replay(lines) => {
            monitor.on_socket_event(SocketEvent::Opened, now(epoch));
            let first = lines.first().map_or(0, |l| l.received_at_monotonic_ms);
            let start = Instant::now();
            for line in lines {
                let deadline =
                    Duration::from_millis(line.received_at_monotonic_ms.saturating_sub(first));
                while start.elapsed() < deadline && !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                monitor.on_frame(line.payload, now(epoch));
            }
            monitor.on_socket_event(SocketEvent::Closed, now(epoch));
        }
        Source::Live(url) => {
            while !stop.load(Ordering::Relaxed) {
                monitor.on_socket_event(SocketEvent::Connecting, now(epoch));
                match tungstenite::connect(&url) {
                    Ok((mut ws, _)) => {
                        if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_mut() {
                            let _ = s.set_read_timeout(Some(Duration::from_millis(100)));
                        }
                        monitor.on_socket_event(SocketEvent::Opened, now(epoch));
                        while !stop.load(Ordering::Relaxed) {
                            match ws.read() {
                                Ok(tungstenite::Message::Text(t)) => {
                                    monitor.on_frame(t.to_string(), now(epoch));
                                }
                                Ok(tungstenite::Message::Close(_)) => break,
                                Ok(_) => {}
                                Err(tungstenite::Error::Io(e))
                                    if matches!(
                                        e.kind(),
                                        std::io::ErrorKind::WouldBlock
                                            | std::io::ErrorKind::TimedOut
                                    ) => {}
                                Err(_) => break,
                            }
                        }
                        monitor.on_socket_event(SocketEvent::Closed, now(epoch));
                    }
                    Err(_) => monitor.on_socket_event(SocketEvent::Errored, now(epoch)),
                }
                match monitor.reconnect_decision() {
                    ReconnectDecision::RetryAfterMs { delay_ms } => {
                        let start = Instant::now();
                        while start.elapsed().as_millis() < (delay_ms as u128)
                            && !stop.load(Ordering::Relaxed)
                        {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                    }
                    ReconnectDecision::GiveUp => break,
                }
            }
        }
    });
}

#[derive(Default)]
pub struct SemanticFeed {
    pub packet: Option<VisualPacket>,
    pub connected: bool,
    pub error: Option<String>,
}
#[cfg(unix)]
pub fn semantic(path: std::path::PathBuf, shared: Arc<Mutex<SemanticFeed>>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        use std::os::unix::net::UnixStream;
        while !stop.load(Ordering::Relaxed) {
            match UnixStream::connect(&path) {
                Ok(socket) => {
                    let _ = socket.set_read_timeout(Some(Duration::from_millis(200)));
                    shared.lock().unwrap().connected = true;
                    let mut reader = BufReader::new(socket);
                    let mut bytes = Vec::new();
                    while !stop.load(Ordering::Relaxed) {
                        match reader
                            .by_ref()
                            .take(4 * 1024 * 1024 + 1)
                            .read_until(b'\n', &mut bytes)
                        {
                            Ok(0) => break,
                            Ok(_) if bytes.len() > 4 * 1024 * 1024 => {
                                shared.lock().unwrap().error =
                                    Some("oversized visual snapshot".into());
                                break;
                            }
                            Ok(_) if bytes.last() == Some(&b'\n') => {
                                let mut feed = shared.lock().unwrap();
                                match serde_json::from_slice::<VisualPacket>(&bytes) {
                                    Ok(packet) if packet.state.validate() => {
                                        let newer = feed.packet.as_ref().is_none_or(|old| {
                                            old.session != packet.session
                                                || packet.sequence > old.sequence
                                        });
                                        if newer {
                                            feed.packet = Some(packet);
                                            feed.error = None;
                                        }
                                    }
                                    _ => feed.error = Some("invalid visual snapshot".into()),
                                }
                                bytes.clear();
                            }
                            Ok(_) => {}
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) => {}
                            Err(_) => break,
                        }
                    }
                }
                Err(e) => shared.lock().unwrap().error = Some(e.to_string()),
            }
            shared.lock().unwrap().connected = false;
            std::thread::sleep(Duration::from_millis(200));
        }
    });
}
