//! The one stateful FFI object. Shells feed socket lifecycle events and raw
//! WS text frames IN (with a shell-supplied **monotonic** `now_ms`); phase,
//! presentation, and snapshots come OUT. Rust never reads a clock;
//! non-monotonic input is defended with saturating arithmetic.
//!
//! ## M5-A contract revision (first deliberate divergence from the Expo oracle)
//!
//! Freshness is scoped to the CURRENT connection generation. The Expo client
//! carried `lastUpdate` across its internal reconnects, so a freshly opened
//! socket could briefly claim `Live` on the strength of a sample received on
//! the previous socket — the same category of error Gate 4 exposed (transport
//! state from one connection used as evidence about another). Here:
//!
//! ```text
//! socket N receives sample       → Live
//! socket N closes                → Closed
//! socket N+1 opens               → OpenNoData
//! cached traces remain visible   → still OpenNoData
//! invalid frame arrives          → still OpenNoData
//! first valid frame on N+1       → Live
//! ```
//!
//! Retry budget: a WebSocket handshake alone proves nothing — an
//! open-immediately-close server must exhaust the budget, not reset it.
//! Attempts reset on the FIRST ACCEPTED FRAME of a generation, never on open.

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

/// Channel display data. Cached samples survive reconnects on purpose (the
/// UI may keep showing the last traces) — but cached data never influences
/// `phase()`; that is what `StreamSnapshot.last_received_at_current_ms` is for.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ChannelSnapshot {
    /// 4 vecs in fixed TP9, AF7, AF8, TP10 order; newest `keep` samples.
    pub channels: Vec<Vec<f64>>,
    /// Index of the newest sample within the snapshot window; -1 when empty.
    pub latest_index: i64,
    /// Total samples accepted since the last `reset()` (all generations).
    pub received: u64,
    /// Monotonic ms of the newest accepted sample on ANY generation.
    pub last_received_at_ms: Option<u64>,
}

/// Stream metadata: everything freshness- and retry-related, per generation.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct StreamSnapshot {
    /// Increments on every completed WebSocket handshake (Opened). 0 = never opened.
    pub connection_generation: u64,
    pub received_on_current_connection: u64,
    pub total_received: u64,
    pub last_received_at_current_ms: Option<u64>,
    pub last_received_at_any_ms: Option<u64>,
    pub cached_sample_count: u32,
    pub reconnect_attempts: u8,
    pub phase: StreamPhase,
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
    generation: u64,
    received_current: u64,
    received_total: u64,
    last_received_at_current_ms: Option<u64>,
    last_received_at_any_ms: Option<u64>,
    /// Unsuccessful connection attempts (incremented on Closed/Errored; reset
    /// ONLY by the first accepted frame of a generation).
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
                generation: 0,
                received_current: 0,
                received_total: 0,
                last_received_at_current_ms: None,
                last_received_at_any_ms: None,
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

    /// Shell reports socket lifecycle. `Opened` starts a NEW generation with
    /// no freshness and does NOT touch the retry budget (a handshake proves
    /// nothing). `Closed`/`Errored` consume one attempt each and never
    /// schedule anything — the shell asks `reconnect_decision()` and owns the
    /// timer.
    pub fn on_socket_event(&self, event: SocketEvent, _now_ms: u64) {
        let mut inner = self.inner.lock().unwrap();
        match event {
            SocketEvent::Connecting => inner.socket = SocketState::Connecting,
            SocketEvent::Opened => {
                inner.socket = SocketState::Open;
                inner.generation = inner.generation.saturating_add(1);
                inner.received_current = 0;
                inner.last_received_at_current_ms = None;
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
    /// accepted. Freshness (current AND any) advances ONLY when accepted > 0
    /// — retained data, empty frames, and invalid frames never refresh age.
    /// The first accepted frame of a generation is the real proof of
    /// recovery: it resets the retry budget (and clears a latched give-up).
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
        inner.received_current += u64::from(accepted);
        inner.received_total += u64::from(accepted);
        inner.last_received_at_current_ms = Some(now_ms);
        inner.last_received_at_any_ms = Some(now_ms);
        inner.attempts = 0;
        inner.gave_up = false;
        accepted
    }

    /// Read-only phase derivation from CURRENT-generation freshness only.
    /// MUST NOT mutate any state.
    pub fn phase(&self, now_ms: u64) -> StreamPhase {
        let inner = self.inner.lock().unwrap();
        if inner.gave_up || inner.socket == SocketState::Errored {
            return StreamPhase::Error;
        }
        match inner.socket {
            SocketState::Connecting => StreamPhase::Connecting,
            SocketState::Closed => StreamPhase::Closed,
            SocketState::Errored => StreamPhase::Error,
            SocketState::Open => match inner.last_received_at_current_ms {
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

    /// Channel display data (cached across reconnects by design).
    pub fn snapshot(&self) -> ChannelSnapshot {
        let inner = self.inner.lock().unwrap();
        let arrays = inner.buffer.channel_arrays();
        let window_len = arrays[0].len() as i64;
        ChannelSnapshot {
            channels: arrays.into_iter().collect(),
            latest_index: window_len - 1,
            received: inner.received_total,
            last_received_at_ms: inner.last_received_at_any_ms,
        }
    }

    /// Stream metadata snapshot. Read-only; never mutates freshness.
    pub fn stream_snapshot(&self, now_ms: u64) -> StreamSnapshot {
        let phase = self.phase(now_ms);
        let inner = self.inner.lock().unwrap();
        StreamSnapshot {
            connection_generation: inner.generation,
            received_on_current_connection: inner.received_current,
            total_received: inner.received_total,
            last_received_at_current_ms: inner.last_received_at_current_ms,
            last_received_at_any_ms: inner.last_received_at_any_ms,
            cached_sample_count: inner.buffer.len() as u32,
            reconnect_attempts: inner.attempts.min(u32::from(u8::MAX)) as u8,
            phase,
        }
    }

    /// Pure decision from the current unsuccessful-attempt count. When it
    /// returns `GiveUp` the monitor latches `Error` until a frame is accepted
    /// or `reset()` is called.
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

    /// New subscription lifecycle (mirrors a screen remount). Clears cached
    /// data, generations, and the retry budget.
    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.socket = SocketState::Connecting;
        inner.buffer.clear();
        inner.generation = 0;
        inner.received_current = 0;
        inner.received_total = 0;
        inner.last_received_at_current_ms = None;
        inner.last_received_at_any_ms = None;
        inner.attempts = 0;
        inner.gave_up = false;
    }
}

/// Deliberately a **second, non-exported** impl block.
///
/// Everything here is for the Linux shell's provenance records and has no
/// mobile caller. Putting it in the `uniffi::export` block above would
/// regenerate the Swift and Kotlin bindings — and trip
/// `scripts/check-binding-drift.sh` — to add a method neither shell calls.
impl StreamMonitor {
    /// Source timestamp of the newest buffered sample: seconds since stream
    /// start, the wire axis, never wall clock. `None` when nothing is buffered.
    ///
    /// Paired with the sample count, this locates the window that
    /// [`Self::snapshot`] just returned inside a recorded `.eeg.jsonl`, which is
    /// what makes a derivation over that window reproducible rather than merely
    /// attributed.
    pub fn newest_source_timestamp(&self) -> Option<f64> {
        self.inner.lock().unwrap().buffer.newest_source_timestamp()
    }
}
