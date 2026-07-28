//! Wire types mirroring the frozen contract (`contracts/api-schema/`).

use serde::{Deserialize, Serialize};

/// Fixed channel order for every array in this crate: TP9, AF7, AF8, TP10.
/// Samples are rejected, never reordered.
pub const CHANNEL_ORDER: [&str; 4] = ["TP9", "AF7", "AF8", "TP10"];
pub const CHANNEL_COUNT: usize = 4;

/// One EEG sample. `timestamp` is seconds since *stream* start (wire data) —
/// a different axis from the monotonic receive-time `now_ms` used for
/// staleness; never mix them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EEGSample {
    pub timestamp: f64,
    pub channels: [f64; 4],
}

// --- HTTP endpoint types (serde round-trip targets for the golden fixtures) ---

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamDiagnostics {
    pub transport: String,
    pub sample_rate: f64,
    pub packets_received: u64,
    pub packets_dropped: u64,
    pub packet_loss_estimate: Option<f64>,
    pub packet_jitter_millis: f64,
    pub last_inter_arrival_millis: f64,
    pub last_heartbeat: String,
    pub bound_port: u32,
    pub local_interface_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelHealthState {
    pub channel: String,
    pub status: String,
    pub rms: f64,
    pub samples: u64,
    pub timestamp: f64,
    pub last_sample_wall_clock: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentDistribution {
    pub rest: f64,
    pub jaw_clench: f64,
    pub single_blink: f64,
    pub double_blink: f64,
    pub select: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentPrediction {
    pub intent: String,
    pub confidence: f64,
    pub distribution: IntentDistribution,
    pub window_sequence: u64,
    pub end_timestamp: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineMode {
    pub source: String,
    pub source_profile: String,
    pub classifier: String,
    pub predictor: String,
    pub transport_detail: String,
    pub is_fully_live: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substitution_summary: Option<String>,
}
