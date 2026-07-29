//! Golden EEG capture envelope (Muse capture gate). Deterministic and
//! effect-free: this module opens no file, computes no clock, and performs
//! no I/O. Shells own the filesystem; the envelope lives here so an Android
//! and an iOS recording of the same stream are byte-identical documents.
//!
//! One WebSocket message becomes exactly one JSONL line, with the payload
//! **preserved verbatim** — EEG parsing is never reimplemented in Swift or
//! Kotlin. Parsing happens here, through the same `decode_eeg_frame` the
//! live path uses, so a replay verifies against the identical decoder.

use crate::types::{EEGSample, CHANNEL_ORDER};
use crate::wire::decode_eeg_frame;
use serde::{Deserialize, Serialize};

pub const CAPTURE_LINE_SCHEMA: &str = "neuralcompose.eeg-capture-line.v1";
pub const CAPTURE_MANIFEST_SCHEMA: &str = "neuralcompose.eeg-capture-manifest.v1";

/// Where the bridge sits relative to the device. Recorded so a replay can
/// never be mistaken for an on-device capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BridgeLocality {
    LocalNetwork,
    RemoteEndpoint,
}

/// Build identity the shell supplies once per recording.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CaptureBuildIdentity {
    pub platform: String,
    pub os_version: String,
    pub app_version: String,
    pub git_commit: String,
    pub bridge_locality: BridgeLocality,
}

/// One preserved WebSocket message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CaptureLine {
    pub sequence: u64,
    pub received_at_monotonic_ms: u64,
    pub accepted_sample_count: u32,
    /// The raw frame text, unmodified. Never a re-serialized reinterpretation.
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CaptureManifest {
    pub schema_id: String,
    pub line_schema_id: String,
    pub recording_id: String,
    pub platform: String,
    pub os_version: String,
    pub app_version: String,
    pub git_commit: String,
    pub bridge_locality: BridgeLocality,
    pub started_at_monotonic_ms: u64,
    pub ended_at_monotonic_ms: u64,
    pub duration_ms: u64,
    pub messages_received: u64,
    pub accepted_sample_count: u64,
    pub rejected_message_count: u64,
    /// Seconds since stream start — the wire axis, never wall clock.
    pub first_source_timestamp: Option<f64>,
    pub last_source_timestamp: Option<f64>,
    pub channel_order: Vec<String>,
    pub payload_byte_size: u64,
    pub payload_sha256_hex: String,
}

/// Encode one line exactly as it must appear in the `.eeg.jsonl` file:
/// compact JSON, no trailing newline (the shell appends the separator).
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn encode_capture_line(line: CaptureLine) -> String {
    serde_json::to_string(&line).expect("serialize")
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ReplayFailure {
    ManifestSchemaMismatch,
    PayloadDigestMismatch,
    PayloadSizeMismatch,
    MalformedLine { line_number: u64 },
    LineSchemaMismatch { line_number: u64 },
    SequenceOutOfOrder { line_number: u64 },
    ReceiveTimeWentBackwards { line_number: u64 },
    AcceptedCountMismatch { line_number: u64 },
    MessageCountMismatch,
    AcceptedSampleCountMismatch,
    RejectedMessageCountMismatch,
    SourceTimestampNotMonotonic { line_number: u64 },
    NonFiniteChannel { line_number: u64 },
    WrongChannelCount { line_number: u64 },
    FirstSourceTimestampMismatch,
    LastSourceTimestampMismatch,
    ChannelOrderMismatch,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ReplayVerdict {
    /// Every claim in the manifest is reproduced by re-decoding the payload.
    Verified {
        accepted_sample_count: u64,
    },
    Failed {
        failure: ReplayFailure,
    },
}

/// Accumulates a recording. Effect-free: it returns the exact bytes the
/// shell must append and the manifest the shell must publish, but never
/// touches a file. `now_ms` is the shell's monotonic receive time.
#[derive(Debug)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct CaptureRecorder {
    inner: std::sync::Mutex<RecorderInner>,
}

#[derive(Debug)]
struct RecorderInner {
    recording_id: String,
    build: CaptureBuildIdentity,
    started_at_ms: u64,
    last_received_at_ms: u64,
    sequence: u64,
    messages_received: u64,
    accepted_sample_count: u64,
    rejected_message_count: u64,
    first_source_timestamp: Option<f64>,
    last_source_timestamp: Option<f64>,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl CaptureRecorder {
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn new(recording_id: String, build: CaptureBuildIdentity, started_at_ms: u64) -> Self {
        Self {
            inner: std::sync::Mutex::new(RecorderInner {
                recording_id,
                build,
                started_at_ms,
                last_received_at_ms: started_at_ms,
                sequence: 0,
                messages_received: 0,
                accepted_sample_count: 0,
                rejected_message_count: 0,
                first_source_timestamp: None,
                last_source_timestamp: None,
            }),
        }
    }

    /// Record one received message and return the JSONL line to append.
    /// Decoding happens here — the shell never parses EEG. A message that
    /// decodes to nothing is still preserved verbatim and counted as
    /// rejected: a capture that silently dropped malformed frames would
    /// misrepresent the stream.
    pub fn on_message(&self, payload: String, now_ms: u64) -> String {
        let mut g = self.inner.lock().unwrap();
        let samples = decode_eeg_frame(&payload);
        let accepted = samples.len() as u32;
        g.sequence += 1;
        g.messages_received += 1;
        if accepted == 0 {
            g.rejected_message_count += 1;
        } else {
            g.accepted_sample_count += accepted as u64;
            if g.first_source_timestamp.is_none() {
                g.first_source_timestamp = samples.first().map(|s| s.timestamp);
            }
            g.last_source_timestamp = samples.last().map(|s| s.timestamp);
        }
        // Receive time is monotonic by contract; clamp rather than record a
        // backwards step the verifier would (correctly) reject.
        let received = now_ms.max(g.last_received_at_ms);
        g.last_received_at_ms = received;
        encode_capture_line(CaptureLine {
            sequence: g.sequence,
            received_at_monotonic_ms: received,
            accepted_sample_count: accepted,
            payload,
        })
    }

    /// Seal the recording. The shell supplies the persisted payload's byte
    /// size and digest — the two facts only the filesystem knows.
    pub fn finish(
        &self,
        ended_at_ms: u64,
        payload_byte_size: u64,
        payload_sha256_hex: String,
    ) -> CaptureManifest {
        let g = self.inner.lock().unwrap();
        let ended = ended_at_ms.max(g.started_at_ms);
        CaptureManifest {
            schema_id: CAPTURE_MANIFEST_SCHEMA.to_string(),
            line_schema_id: CAPTURE_LINE_SCHEMA.to_string(),
            recording_id: g.recording_id.clone(),
            platform: g.build.platform.clone(),
            os_version: g.build.os_version.clone(),
            app_version: g.build.app_version.clone(),
            git_commit: g.build.git_commit.clone(),
            bridge_locality: g.build.bridge_locality,
            started_at_monotonic_ms: g.started_at_ms,
            ended_at_monotonic_ms: ended,
            duration_ms: ended - g.started_at_ms,
            messages_received: g.messages_received,
            accepted_sample_count: g.accepted_sample_count,
            rejected_message_count: g.rejected_message_count,
            first_source_timestamp: g.first_source_timestamp,
            last_source_timestamp: g.last_source_timestamp,
            channel_order: CHANNEL_ORDER.iter().map(|c| c.to_string()).collect(),
            payload_byte_size,
            payload_sha256_hex,
        }
    }

    pub fn messages_received(&self) -> u64 {
        self.inner.lock().unwrap().messages_received
    }

    pub fn accepted_sample_count(&self) -> u64 {
        self.inner.lock().unwrap().accepted_sample_count
    }
}

fn four_channels(s: &EEGSample) -> bool {
    s.channels.len() == crate::types::CHANNEL_COUNT
}

fn finite_channels(s: &EEGSample) -> bool {
    s.channels.iter().all(|c| c.is_finite())
}

/// Replay a persisted recording through the same decoder that produced it
/// and check every claim the manifest makes. This is the whole point of the
/// gate: a file that cannot reproduce its own manifest is not evidence.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn verify_capture(jsonl: String, manifest: CaptureManifest) -> ReplayVerdict {
    let fail = |failure| ReplayVerdict::Failed { failure };

    if manifest.schema_id != CAPTURE_MANIFEST_SCHEMA
        || manifest.line_schema_id != CAPTURE_LINE_SCHEMA
    {
        return fail(ReplayFailure::ManifestSchemaMismatch);
    }
    let expected_order: Vec<String> = CHANNEL_ORDER.iter().map(|c| c.to_string()).collect();
    if manifest.channel_order != expected_order {
        return fail(ReplayFailure::ChannelOrderMismatch);
    }
    // The bytes must be the bytes: size and digest before any interpretation.
    let bytes = jsonl.as_bytes();
    if bytes.len() as u64 != manifest.payload_byte_size {
        return fail(ReplayFailure::PayloadSizeMismatch);
    }
    if crate::audio::sha256_hex(bytes.to_vec()) != manifest.payload_sha256_hex {
        return fail(ReplayFailure::PayloadDigestMismatch);
    }

    let mut messages = 0u64;
    let mut accepted_total = 0u64;
    let mut rejected = 0u64;
    let mut prev_sequence = 0u64;
    let mut prev_received = 0u64;
    let mut prev_source: Option<f64> = None;
    let mut first_source: Option<f64> = None;
    let mut last_source: Option<f64> = None;

    for (idx, raw) in jsonl.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let line_number = idx as u64 + 1;
        let line: CaptureLine = match serde_json::from_str(raw) {
            Ok(l) => l,
            Err(_) => return fail(ReplayFailure::MalformedLine { line_number }),
        };
        messages += 1;
        if line.sequence != prev_sequence + 1 {
            return fail(ReplayFailure::SequenceOutOfOrder { line_number });
        }
        prev_sequence = line.sequence;
        if line.received_at_monotonic_ms < prev_received {
            return fail(ReplayFailure::ReceiveTimeWentBackwards { line_number });
        }
        prev_received = line.received_at_monotonic_ms;

        // Re-decode the preserved payload with the production decoder.
        let samples = decode_eeg_frame(&line.payload);
        if samples.len() as u32 != line.accepted_sample_count {
            return fail(ReplayFailure::AcceptedCountMismatch { line_number });
        }
        if samples.is_empty() {
            rejected += 1;
            continue;
        }
        for s in &samples {
            // The decoder already rejects frames that are not four finite
            // channels, so these re-assert the invariant at replay time
            // rather than trusting it — cheap, and the whole point of a
            // golden gate is not taking the producer's word for it.
            if !four_channels(s) {
                return fail(ReplayFailure::WrongChannelCount { line_number });
            }
            if !finite_channels(s) {
                return fail(ReplayFailure::NonFiniteChannel { line_number });
            }
            if let Some(p) = prev_source {
                if s.timestamp < p {
                    return fail(ReplayFailure::SourceTimestampNotMonotonic { line_number });
                }
            }
            prev_source = Some(s.timestamp);
        }
        accepted_total += samples.len() as u64;
        if first_source.is_none() {
            first_source = samples.first().map(|s| s.timestamp);
        }
        last_source = samples.last().map(|s| s.timestamp);
    }

    if messages != manifest.messages_received {
        return fail(ReplayFailure::MessageCountMismatch);
    }
    if accepted_total != manifest.accepted_sample_count {
        return fail(ReplayFailure::AcceptedSampleCountMismatch);
    }
    if rejected != manifest.rejected_message_count {
        return fail(ReplayFailure::RejectedMessageCountMismatch);
    }
    if first_source != manifest.first_source_timestamp {
        return fail(ReplayFailure::FirstSourceTimestampMismatch);
    }
    if last_source != manifest.last_source_timestamp {
        return fail(ReplayFailure::LastSourceTimestampMismatch);
    }
    ReplayVerdict::Verified {
        accepted_sample_count: accepted_total,
    }
}

/// Canonical file names. Both shells must agree, or a recording written on
/// one platform is not discoverable on the other.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn capture_payload_filename(recording_id: String) -> String {
    format!("{recording_id}.eeg.jsonl")
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn capture_manifest_filename(recording_id: String) -> String {
    format!("{recording_id}.manifest.json")
}

/// In-progress files carry `.partial` and are never discoverable as
/// recordings; publication is the atomic rename of both.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn partial_suffix() -> String {
    ".partial".to_string()
}
