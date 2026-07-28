// Golden-fixture tests: every frozen fixture round-trips through the typed
// structs, validates against its JSON Schema, and the batch-8 frame decodes to
// the stub's sine formula. Also asserts the crate's compiled defaults equal
// contracts/constants.json (drift guard).

use neuralcompose_mobile_core::types::{
    ChannelHealthState, IntentPrediction, PipelineMode, StreamDiagnostics, CHANNEL_ORDER,
};
use neuralcompose_mobile_core::{
    decode_eeg_frame, ChannelSnapshot, MonitorConfig, SocketEvent, StreamMonitor,
};
use serde_json::Value;

fn contracts_path(rel: &str) -> String {
    format!("{}/../../contracts/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn read_json(rel: &str) -> Value {
    let text =
        std::fs::read_to_string(contracts_path(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn assert_valid(schema_rel: &str, instance: &Value) {
    let schema = read_json(schema_rel);
    let validator = jsonschema::validator_for(&schema).expect(schema_rel);
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{schema_rel}: {errors:?}");
}

#[test]
fn diagnostics_fixture_round_trips_and_validates() {
    let v = read_json("fixtures/diagnostics.json");
    assert_valid("api-schema/diagnostics.schema.json", &v);
    let typed: StreamDiagnostics = serde_json::from_value(v.clone()).expect("typed");
    assert_eq!(typed.sample_rate, 256.0);
    assert_eq!(typed.packet_loss_estimate, None);
    let back = serde_json::to_value(&typed).expect("serialize");
    assert_eq!(back, v);
}

#[test]
fn health_fixture_round_trips_validates_and_keeps_channel_order() {
    let v = read_json("fixtures/health.json");
    assert_valid("api-schema/health.schema.json", &v);
    let typed: Vec<ChannelHealthState> = serde_json::from_value(v).expect("typed");
    let order: Vec<&str> = typed.iter().map(|c| c.channel.as_str()).collect();
    assert_eq!(order, CHANNEL_ORDER.to_vec());
    assert_eq!(typed[2].status, "saturated"); // AF8 in the frozen fixture
}

#[test]
fn classifier_fixture_round_trips_and_validates() {
    let v = read_json("fixtures/classifier.json");
    assert_valid("api-schema/classifier.schema.json", &v);
    let typed: IntentPrediction = serde_json::from_value(v.clone()).expect("typed");
    assert_eq!(typed.intent, "rest");
    let back = serde_json::to_value(&typed).expect("serialize");
    assert_eq!(back, v);
}

#[test]
fn pipeline_mode_fixture_round_trips_and_validates() {
    let v = read_json("fixtures/pipeline-mode.json");
    assert_valid("api-schema/pipeline-mode.schema.json", &v);
    let typed: PipelineMode = serde_json::from_value(v.clone()).expect("typed");
    assert!(!typed.is_fully_live);
    let back = serde_json::to_value(&typed).expect("serialize");
    assert_eq!(back, v);
}

#[test]
fn single_frame_fixture_decodes_and_validates() {
    let v = read_json("fixtures/eeg-frame-single.json");
    assert_valid("api-schema/eeg-sample.schema.json", &v);
    let out = decode_eeg_frame(&v.to_string());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].channels, [10.0, 20.0, 30.0, 40.0]);
}

#[test]
fn batch8_fixture_matches_stub_sine_formula() {
    let text = std::fs::read_to_string(contracts_path("fixtures/eeg-frame-batch-8.json"))
        .expect("run `cargo run --example gen_fixtures` to generate eeg-frame-batch-8.json");
    let out = decode_eeg_frame(&text);
    assert_eq!(out.len(), 8);
    let tau = std::f64::consts::TAU;
    for (n, s) in out.iter().enumerate() {
        let t = n as f64 / 256.0;
        assert_eq!(s.timestamp, t);
        let expected = [
            45.0 * (tau * 8.0 * t).sin(),
            32.0 * (tau * 10.0 * t + 0.5).sin(),
            36.0 * (tau * 12.0 * t + 1.0).sin(),
            42.0 * (tau * 6.0 * t + 1.5).sin(),
        ];
        for (i, want) in expected.iter().enumerate() {
            assert!(
                (s.channels[i] - want).abs() < 1e-12,
                "sample {n} channel {i}: {} vs {want}",
                s.channels[i]
            );
        }
    }
    // Batch validates against the frame schema's array arm (validated per-sample
    // to avoid cross-file $ref resolution in the test harness).
    let v: Value = serde_json::from_str(&text).unwrap();
    for item in v.as_array().unwrap() {
        assert_valid("api-schema/eeg-sample.schema.json", item);
    }
}

#[test]
fn monitor_defaults_equal_constants_json() {
    let c = read_json("constants.json");
    let d = MonitorConfig::default();
    assert_eq!(
        u64::from(d.keep_samples),
        c["bufferSamples"].as_u64().unwrap()
    );
    assert_eq!(
        d.stale_after_ms,
        c["staleMs"]["channelSample"].as_u64().unwrap()
    );
    assert_eq!(
        u64::from(d.max_reconnect_attempts),
        c["reconnect"]["maxAttempts"].as_u64().unwrap()
    );
    assert_eq!(
        d.backoff_base_ms,
        c["reconnect"]["baseMs"].as_u64().unwrap()
    );
    assert_eq!(d.backoff_cap_ms, c["reconnect"]["capMs"].as_u64().unwrap());
    let order: Vec<&str> = c["channelOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(order, CHANNEL_ORDER.to_vec());
}

#[test]
fn snapshot_channels_always_four_in_order() {
    let m = StreamMonitor::with_defaults();
    m.on_socket_event(SocketEvent::Opened, 0);
    m.on_frame(r#"{"timestamp":0,"channels":[1,2,3,4]}"#.to_string(), 1);
    let snap: ChannelSnapshot = m.snapshot();
    assert_eq!(snap.channels.len(), 4);
    assert_eq!(
        snap.channels.iter().map(|c| c[0]).collect::<Vec<_>>(),
        vec![1.0, 2.0, 3.0, 4.0]
    );
}
