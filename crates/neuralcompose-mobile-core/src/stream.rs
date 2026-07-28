//! The one stateful FFI object. Shells feed socket lifecycle events and raw
//! WS text frames IN (with a shell-supplied **monotonic** `now_ms`); phase,
//! presentation, and channel snapshots come OUT. Rust never reads a clock;
//! non-monotonic input is defended with saturating arithmetic.

use std::sync::Mutex;

use crate::buffer::SampleBuffer;
use crate::presentation::{present, Presentation, StreamPhase};
use crate::reconnect::{next_reconnect_with, ReconnectDecision};
use crate::wire::decode_eeg_frame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SocketEvent {
    Connecting,
    Opened,
    Closed,
    Errored,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MonitorConfig {
    pub keep_samples: u32,
    pub stale_after_ms: u64,
    pub max_reconnect_attempts: u32,
    pub backoff_base_ms: u64,
    pub backoff_cap_ms: u64,
}

impl Default for MonitorConfig {
    /// Defaults are test-asserted equal to `contracts/constants.json`.
    fn default() -> Self {
        Self {
            keep_samples: 1280,
            stale_after_ms: 2000,
            max_reconnect_attempts: 3,
            backoff_base_ms: 500,
            backoff_cap_ms: 30_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ChannelSnapshot {
    /// 4 vecs in fixed TP9, AF7, AF8, TP10 order; newest `keep` samples.
    pub channels: Vec<Vec<f64>>,
    /// Index of the newest sample within the snapshot window; -1 when empty
    /// (mirrors the Expo `EEGBuffer.latest`).
    pub latest_index: i64,
    /// Total samples accepted since the last `reset()`.
    pub received: u64,
    /// Monotonic ms of the newest ACCEPTED sample; `None` before the first.
    pub last_received_at_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SocketState {
    Connecting,
    Open,
    Closed,
    Errored,
}

struct Inner {
    socket: SocketState,
    buffer: SampleBuffer,
    received: u64,
    last_received_at_ms: Option<u64>,
    /// Failed connection attempts (incremented on Closed/Errored, reset on Opened).
    attempts: u32,
    gave_up: bool,
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct StreamMonitor {
    config: MonitorConfig,
    inner: Mutex<Inner>,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl StreamMonitor {
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            inner: Mutex::new(Inner {
                socket: SocketState::Connecting,
                buffer: SampleBuffer::new(config.keep_samples),
                received: 0,
                last_received_at_ms: None,
                attempts: 0,
                gave_up: false,
            }),
            config,
        }
    }

    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn with_defaults() -> Self {
        Self::new(MonitorConfig::default())
    }

    /// Shell reports socket lifecycle. `Opened` resets the attempt counter;
    /// `Closed`/`Errored` count one failed attempt each and never schedule
    /// anything themselves — the shell asks `reconnect_decision()` and owns
    /// the timer.
    pub fn on_socket_event(&self, event: SocketEvent, _now_ms: u64) {
        let mut inner = self.inner.lock().unwrap();
        match event {
            SocketEvent::Connecting => inner.socket = SocketState::Connecting,
            SocketEvent::Opened => {
                inner.socket = SocketState::Open;
                inner.attempts = 0;
                inner.gave_up = false;
            }
            SocketEvent::Closed => {
                inner.socket = SocketState::Closed;
                inner.attempts = inner.attempts.saturating_add(1);
            }
            SocketEvent::Errored => {
                inner.socket = SocketState::Errored;
                inner.attempts = inner.attempts.saturating_add(1);
            }
        }
    }

    /// Shell hands over one raw WS text frame. Returns the number of samples
    /// accepted. `last_received_at_ms` advances ONLY when accepted > 0 —
    /// this pins the Gate 4 no-op-flush bug: retained data and empty or
    /// invalid frames must never refresh sample age.
    pub fn on_frame(&self, text: String, now_ms: u64) -> u32 {
        let samples = decode_eeg_frame(&text);
        if samples.is_empty() {
            return 0;
        }
        let mut inner = self.inner.lock().unwrap();
        let accepted = samples.len() as u32;
        for s in samples {
            inner.buffer.push(s);
        }
        inner.received += u64::from(accepted);
        inner.last_received_at_ms = Some(now_ms);
        accepted
    }

    /// Read-only phase derivation. MUST NOT mutate receive state.
    pub fn phase(&self, now_ms: u64) -> StreamPhase {
        let inner = self.inner.lock().unwrap();
        if inner.gave_up || inner.socket == SocketState::Errored {
            return StreamPhase::Error;
        }
        match inner.socket {
            SocketState::Connecting => StreamPhase::Connecting,
            SocketState::Closed => StreamPhase::Closed,
            SocketState::Errored => StreamPhase::Error,
            SocketState::Open => match inner.last_received_at_ms {
                None => StreamPhase::OpenNoData,
                Some(last) => {
                    let age_ms = now_ms.saturating_sub(last);
                    if age_ms > self.config.stale_after_ms {
                        StreamPhase::Stale { age_ms }
                    } else {
                        StreamPhase::Live
                    }
                }
            },
        }
    }

    pub fn presentation(&self, now_ms: u64) -> Presentation {
        present(self.phase(now_ms))
    }

    pub fn snapshot(&self) -> ChannelSnapshot {
        let inner = self.inner.lock().unwrap();
        let arrays = inner.buffer.channel_arrays();
        let window_len = arrays[0].len() as i64;
        ChannelSnapshot {
            channels: arrays.into_iter().collect(),
            latest_index: window_len - 1,
            received: inner.received,
            last_received_at_ms: inner.last_received_at_ms,
        }
    }

    /// Pure decision from the current failed-attempt count. When it returns
    /// `GiveUp` the monitor latches `Error` (matching the Expo client, which
    /// reports `error` after exhausting its 3 attempts).
    pub fn reconnect_decision(&self) -> ReconnectDecision {
        let mut inner = self.inner.lock().unwrap();
        let decision = next_reconnect_with(
            inner.attempts,
            self.config.max_reconnect_attempts,
            self.config.backoff_base_ms,
            self.config.backoff_cap_ms,
        );
        if decision == ReconnectDecision::GiveUp {
            inner.gave_up = true;
        }
        decision
    }

    /// New subscription lifecycle (mirrors the Expo hook's remount reset).
    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.socket = SocketState::Connecting;
        inner.buffer.clear();
        inner.received = 0;
        inner.last_received_at_ms = None;
        inner.attempts = 0;
        inner.gave_up = false;
    }
}
