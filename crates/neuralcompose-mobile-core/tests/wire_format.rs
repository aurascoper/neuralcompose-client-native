// Port of the Expo client's src/api/__tests__/wireFormat.test.ts, driven by
// the shared reject/accept table in contracts/fixtures/eeg-frame-rejects.json.

use neuralcompose_mobile_core::decode_eeg_frame;
use serde_json::Value;

fn rejects_table() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/fixtures/eeg-frame-rejects.json"
    ))
    .expect("rejects fixture");
    serde_json::from_str(&text).expect("rejects fixture JSON")
}

#[test]
fn shared_reject_accept_table() {
    let table = rejects_table();
    for case in table["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().unwrap();
        let payload = case["payload"].as_str().unwrap();
        let expect_valid = case["expectValid"].as_u64().unwrap() as usize;
        let out = decode_eeg_frame(payload);
        assert_eq!(out.len(), expect_valid, "case: {name}");
        if let Some(expected) = case.get("expectChannels").and_then(|v| v.as_array()) {
            let expected: Vec<f64> = expected.iter().map(|v| v.as_f64().unwrap()).collect();
            assert_eq!(out[0].channels.to_vec(), expected, "channel order: {name}");
        }
    }
}

#[test]
fn decodes_single_valid_sample() {
    let out = decode_eeg_frame(r#"{"timestamp":1.5,"channels":[10,20,30,40]}"#);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].timestamp, 1.5);
    assert_eq!(out[0].channels, [10.0, 20.0, 30.0, 40.0]);
}

#[test]
fn batch_drops_only_invalid_entries() {
    let out = decode_eeg_frame(
        r#"[{"timestamp":1.5,"channels":[10,20,30,40]},{"timestamp":2,"channels":[1,2,3]},{"timestamp":3,"channels":[10,20,30,40]}]"#,
    );
    let timestamps: Vec<f64> = out.iter().map(|s| s.timestamp).collect();
    assert_eq!(timestamps, vec![1.5, 3.0]);
}

#[test]
fn preserves_fixed_channel_order() {
    let out = decode_eeg_frame(r#"{"timestamp":0,"channels":[4,3,2,1]}"#);
    assert_eq!(out[0].channels, [4.0, 3.0, 2.0, 1.0]);
}
