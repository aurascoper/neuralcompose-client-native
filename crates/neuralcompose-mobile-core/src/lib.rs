//! NeuralCompose shared behavioral core.
//!
//! Shells (SwiftUI, Compose, web) own all I/O — sockets, timers, permissions,
//! audio, lifecycle — and feed raw payloads plus **monotonic** `now_ms`
//! timestamps in. This crate decides what the I/O *means*: validated samples,
//! bounded buffers, and a truthful `StreamPhase` (an open socket with no
//! recent samples is `Stale`, not healthy — the Gate 4 lesson).
//!
//! The FFI boundary is synchronous and pure-in/pure-out; the only stateful
//! object is [`stream::StreamMonitor`]. This crate never reads a clock.

pub mod audio;
pub mod band_power;
pub mod buffer;
pub mod capture;
pub mod channel_health;
pub mod config;
pub mod conformance;
pub mod electrode_check;
pub mod model_pack;
pub mod presentation;
pub mod property_law;
pub mod provider;
pub mod reconnect;
pub mod runtime_target;
pub mod stream;
pub mod types;
pub mod wire;

pub use audio::{
    sha256_hex, AudioLifecycle, AudioSnapshot, AudioTransition, RecordingManifest, RecordingPhase,
};
pub use band_power::band_power;
pub use buffer::SampleBuffer;
pub use channel_health::{ChannelHealthStatus, ChannelHealthThresholds};
pub use config::{
    derive_ws_url, parse_use_mock, resolve_client_mode, ClientMode, ResolvedClientConfig,
};
pub use electrode_check::{
    assess_channel, common_mode_hint, ElectrodeReport, ElectrodeVerdict, MainsThresholds,
};
pub use presentation::{
    format_banner_en, format_label_en, present, Presentation, StreamPhase, StreamTone,
};
pub use reconnect::{next_reconnect, ReconnectDecision};
pub use stream::{ChannelSnapshot, MonitorConfig, SocketEvent, StreamMonitor, StreamSnapshot};
pub use types::EEGSample;
pub use wire::decode_eeg_frame;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
