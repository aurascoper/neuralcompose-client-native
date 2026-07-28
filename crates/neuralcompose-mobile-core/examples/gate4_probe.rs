//! Live Gate 4 replay: connects to the reference stub, feeds raw WS frames and
//! monotonic timestamps into `StreamMonitor`, and prints every phase
//! transition. The shell role (socket + timers) is played by this binary; all
//! meaning comes from the core.
//!
//! Run:  node contracts/stub-server/server.mjs   (in another terminal)
//!       cargo run --example gate4_probe
//! Then: curl -X POST 127.0.0.1:8787/control/pause   → Live → Stale{growing}
//!       curl -X POST 127.0.0.1:8787/control/resume  → Live
//!       curl -X POST '127.0.0.1:8787/control/drop?ms=2500'
//!                                → Closed → Connecting → Live (backoff 1s, 2s)

use std::net::TcpStream;
use std::time::{Duration, Instant};

use neuralcompose_mobile_core::{
    format_banner_en, format_label_en, ReconnectDecision, SocketEvent, StreamMonitor,
};
use tungstenite::{connect, Message, WebSocket};

const WS_URL: &str = "ws://127.0.0.1:8787/api/eeg/stream";

fn now_ms(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

fn main() {
    let epoch = Instant::now();
    let monitor = StreamMonitor::with_defaults();
    let mut last_printed = String::new();

    'lifecycle: loop {
        monitor.on_socket_event(SocketEvent::Connecting, now_ms(epoch));
        report(&monitor, epoch, &mut last_printed);

        let mut socket = match connect(WS_URL) {
            Ok((socket, _)) => {
                monitor.on_socket_event(SocketEvent::Opened, now_ms(epoch));
                report(&monitor, epoch, &mut last_printed);
                socket
            }
            Err(e) => {
                eprintln!("dial failed: {e}");
                monitor.on_socket_event(SocketEvent::Errored, now_ms(epoch));
                report(&monitor, epoch, &mut last_printed);
                if !backoff_or_exit(&monitor) {
                    break 'lifecycle;
                }
                continue;
            }
        };
        set_read_timeout(&mut socket, Duration::from_millis(200));

        loop {
            match socket.read() {
                Ok(Message::Text(text)) => {
                    monitor.on_frame(text.to_string(), now_ms(epoch));
                }
                Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => {
                    monitor.on_socket_event(SocketEvent::Closed, now_ms(epoch));
                    report(&monitor, epoch, &mut last_printed);
                    if !backoff_or_exit(&monitor) {
                        break 'lifecycle;
                    }
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    // No frame within the poll window — phase() decides staleness.
                }
                Err(e) => {
                    eprintln!("socket error: {e}");
                    monitor.on_socket_event(SocketEvent::Closed, now_ms(epoch));
                    report(&monitor, epoch, &mut last_printed);
                    if !backoff_or_exit(&monitor) {
                        break 'lifecycle;
                    }
                    break;
                }
            }
            report(&monitor, epoch, &mut last_printed);
        }
    }
    println!("probe: gave up (Error latched) — exiting");
}

fn report(monitor: &StreamMonitor, epoch: Instant, last_printed: &mut String) {
    let p = monitor.presentation(now_ms(epoch));
    let mut line = format!("{:?} | {}", p.phase, format_label_en(p));
    if let Some(banner) = format_banner_en(p) {
        line = format!("{line} | {banner}");
    }
    // Collapse Stale{age} churn to one line per second.
    let key = line.split('|').next().unwrap().trim().to_string();
    let printable = if key.starts_with("Stale") {
        format!("Stale({}s) | {}", p.age_s.unwrap_or(0), format_label_en(p))
    } else {
        line.clone()
    };
    if printable != *last_printed {
        println!("[{:>8}ms] {printable}", now_ms(epoch));
        *last_printed = printable;
    }
}

fn backoff_or_exit(monitor: &StreamMonitor) -> bool {
    match monitor.reconnect_decision() {
        ReconnectDecision::RetryAfterMs { delay_ms } => {
            println!("           retry in {delay_ms}ms");
            std::thread::sleep(Duration::from_millis(delay_ms));
            true
        }
        ReconnectDecision::GiveUp => false,
    }
}

fn set_read_timeout(
    socket: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    d: Duration,
) {
    if let tungstenite::stream::MaybeTlsStream::Plain(s) = socket.get_mut() {
        let _ = s.set_read_timeout(Some(d));
    }
}
