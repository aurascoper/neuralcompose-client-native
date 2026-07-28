//! WebSocket frame decoding — byte-for-byte port of the Expo client's
//! `src/api/wireFormat.ts` (parity oracle: `contracts/fixtures/eeg-frame-rejects.json`).

use crate::types::{EEGSample, CHANNEL_COUNT};
use serde_json::Value;

/// Decode one WS text frame into zero or more valid samples.
/// - single sample object → `[sample]`
/// - batch array → the valid samples, invalid entries dropped
/// - anything else (bad JSON, wrong shape, non-finite, wrong channel count) → `[]`
///
/// Known divergence from the JS oracle: `serde_json` rejects the *whole* frame
/// when any number literal overflows f64 (e.g. `1e999`), where `JSON.parse`
/// yields `Infinity` and the validator drops just that entry. For single-sample
/// frames the observable result is identical (0 samples); a batch mixing an
/// overflow literal with valid entries would drop the batch here. The explicit
/// `is_finite` checks stay regardless, as defense in depth.
pub fn decode_eeg_frame(text: &str) -> Vec<EEGSample> {
    let parsed: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    match parsed {
        Value::Array(items) => items.iter().filter_map(sample_from_value).collect(),
        other => sample_from_value(&other).into_iter().collect(),
    }
}

fn sample_from_value(v: &Value) -> Option<EEGSample> {
    let obj = v.as_object()?;
    let timestamp = obj.get("timestamp")?.as_f64()?;
    if !timestamp.is_finite() {
        return None;
    }
    let raw = obj.get("channels")?.as_array()?;
    if raw.len() != CHANNEL_COUNT {
        return None;
    }
    let mut channels = [0.0f64; CHANNEL_COUNT];
    for (i, c) in raw.iter().enumerate() {
        let f = c.as_f64()?;
        if !f.is_finite() {
            return None;
        }
        channels[i] = f;
    }
    Some(EEGSample {
        timestamp,
        channels,
    })
}
