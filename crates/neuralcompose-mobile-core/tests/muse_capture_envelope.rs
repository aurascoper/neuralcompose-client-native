// Muse golden-capture envelope regressions. The envelope is frozen here
// before either shell writes a byte: an Android and an iOS recording of the
// same stream must be identical documents, and a recording that cannot
// reproduce its own manifest is not evidence.

use neuralcompose_mobile_core::capture::*;

fn build() -> CaptureBuildIdentity {
    CaptureBuildIdentity {
        platform: "android".into(),
        os_version: "17".into(),
        app_version: "0.1.0".into(),
        git_commit: "0123456789abcdef0123456789abcdef01234567".into(),
        bridge_locality: BridgeLocality::LocalNetwork,
    }
}

fn frame(t: f64) -> String {
    format!(r#"{{"timestamp":{t},"channels":[1.0,2.0,3.0,4.0]}}"#)
}

fn batch(start: f64, n: usize) -> String {
    let items: Vec<String> = (0..n).map(|i| frame(start + i as f64 * 0.004)).collect();
    format!("[{}]", items.join(","))
}

/// Record a short session and return (jsonl, manifest) exactly as a shell
/// would persist them.
fn record(payloads: &[(String, u64)]) -> (String, CaptureManifest) {
    let r = CaptureRecorder::new("rec-1".into(), build(), 1_000);
    let mut lines = Vec::new();
    for (p, now) in payloads {
        lines.push(r.on_message(p.clone(), *now));
    }
    // The shell's file: one line per message, newline-terminated.
    let jsonl = lines
        .iter()
        .map(|l| format!("{l}\n"))
        .collect::<Vec<_>>()
        .join("");
    let sha = sha_of(&jsonl);
    let manifest = r.finish(61_000, jsonl.len() as u64, sha);
    (jsonl, manifest)
}

fn sha_of(s: &str) -> String {
    // Same digest the shell computes over the persisted bytes.
    use neuralcompose_mobile_core::sha256_hex;
    sha256_hex(s.as_bytes().to_vec())
}

#[test]
fn a_recording_verifies_against_its_own_manifest() {
    let (jsonl, manifest) = record(&[
        (batch(0.0, 12), 1_100),
        (batch(0.048, 12), 1_200),
        (frame(0.096), 1_300),
    ]);
    assert_eq!(manifest.messages_received, 3);
    assert_eq!(manifest.accepted_sample_count, 25);
    assert_eq!(manifest.rejected_message_count, 0);
    assert_eq!(manifest.first_source_timestamp, Some(0.0));
    assert_eq!(manifest.channel_order, ["TP9", "AF7", "AF8", "TP10"]);
    assert_eq!(manifest.duration_ms, 60_000);
    assert_eq!(
        verify_capture(jsonl, manifest),
        ReplayVerdict::Verified {
            accepted_sample_count: 25
        }
    );
}

#[test]
fn the_payload_is_preserved_verbatim_not_reinterpreted() {
    // Odd-but-legal spacing and key order must survive byte-for-byte: the
    // shells store what arrived, they do not re-serialize an interpretation.
    let odd = r#"{ "channels":[1.5,2.5,3.5,4.5] ,  "timestamp": 7.25 }"#;
    let r = CaptureRecorder::new("rec".into(), build(), 0);
    let line = r.on_message(odd.to_string(), 10);
    let decoded: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        decoded["payload"].as_str().unwrap(),
        odd,
        "the payload must round-trip unchanged"
    );
    assert_eq!(decoded["acceptedSampleCount"], 1);
    assert_eq!(decoded["sequence"], 1);
}

#[test]
fn malformed_messages_are_preserved_and_counted_as_rejected() {
    // A capture that silently dropped junk would misrepresent the stream.
    let (jsonl, manifest) = record(&[
        (batch(0.0, 4), 1_100),
        ("not json at all".into(), 1_150),
        (r#"{"timestamp":1.0,"channels":[1.0,2.0]}"#.into(), 1_180), // wrong count
        (batch(0.016, 4), 1_200),
    ]);
    assert_eq!(manifest.messages_received, 4);
    assert_eq!(manifest.rejected_message_count, 2);
    assert_eq!(manifest.accepted_sample_count, 8);
    assert!(
        jsonl.contains("not json at all"),
        "rejected frames are still preserved"
    );
    assert!(matches!(
        verify_capture(jsonl, manifest),
        ReplayVerdict::Verified { .. }
    ));
}

#[test]
fn tampering_with_the_payload_fails_replay() {
    let (jsonl, manifest) = record(&[(batch(0.0, 8), 1_100), (batch(0.032, 8), 1_200)]);

    // A single flipped digit changes the digest.
    let flipped = jsonl.replacen("1.0", "9.0", 1);
    assert_ne!(flipped, jsonl);
    assert_eq!(
        verify_capture(flipped, manifest.clone()),
        ReplayVerdict::Failed {
            failure: ReplayFailure::PayloadDigestMismatch
        }
    );
    // Truncation is caught by size before anything is interpreted.
    let truncated: String = jsonl.chars().take(jsonl.len() / 2).collect();
    assert_eq!(
        verify_capture(truncated, manifest.clone()),
        ReplayVerdict::Failed {
            failure: ReplayFailure::PayloadSizeMismatch
        }
    );
    // Polarity: the untouched file still verifies.
    assert!(matches!(
        verify_capture(jsonl, manifest),
        ReplayVerdict::Verified { .. }
    ));
}

#[test]
fn a_manifest_that_overstates_its_recording_fails() {
    let (jsonl, base) = record(&[(batch(0.0, 8), 1_100), (batch(0.032, 8), 1_200)]);

    let inflate_counts = |mut m: CaptureManifest, f: fn(&mut CaptureManifest)| {
        f(&mut m);
        m
    };
    // Every count in the manifest is re-derived from the bytes, so none of
    // them can be inflated after the fact.
    type ManifestMutation = fn(&mut CaptureManifest);
    let cases: Vec<(&str, ManifestMutation, ReplayFailure)> = vec![
        (
            "messages",
            |m: &mut CaptureManifest| m.messages_received += 1,
            ReplayFailure::MessageCountMismatch,
        ),
        (
            "samples",
            |m: &mut CaptureManifest| m.accepted_sample_count += 1,
            ReplayFailure::AcceptedSampleCountMismatch,
        ),
        (
            "rejected",
            |m: &mut CaptureManifest| m.rejected_message_count += 1,
            ReplayFailure::RejectedMessageCountMismatch,
        ),
        (
            "first timestamp",
            |m: &mut CaptureManifest| m.first_source_timestamp = Some(99.0),
            ReplayFailure::FirstSourceTimestampMismatch,
        ),
        (
            "last timestamp",
            |m: &mut CaptureManifest| m.last_source_timestamp = Some(99.0),
            ReplayFailure::LastSourceTimestampMismatch,
        ),
        (
            "channel order",
            |m: &mut CaptureManifest| m.channel_order = vec!["AF7".into(), "TP9".into()],
            ReplayFailure::ChannelOrderMismatch,
        ),
        (
            "schema",
            |m: &mut CaptureManifest| m.schema_id = "something.else".into(),
            ReplayFailure::ManifestSchemaMismatch,
        ),
    ];
    for (name, mutate, expected) in cases {
        let m = inflate_counts(base.clone(), mutate);
        assert_eq!(
            verify_capture(jsonl.clone(), m),
            ReplayVerdict::Failed { failure: expected },
            "inflating {name} must fail replay"
        );
    }
}

#[test]
fn reordered_or_edited_lines_fail_replay() {
    // These mutations keep the byte count identical, so they must be caught
    // by the digest — proving size alone is not the guard.
    let (jsonl, manifest) = record(&[
        (batch(0.0, 4), 1_100),
        (batch(0.016, 4), 1_200),
        (batch(0.032, 4), 1_300),
    ]);
    let mut lines: Vec<&str> = jsonl.lines().collect();
    lines.swap(0, 2);
    let swapped = lines
        .iter()
        .map(|l| format!("{l}\n"))
        .collect::<Vec<_>>()
        .join("");
    assert_eq!(swapped.len(), jsonl.len(), "same size, different order");
    assert_eq!(
        verify_capture(swapped, manifest.clone()),
        ReplayVerdict::Failed {
            failure: ReplayFailure::PayloadDigestMismatch
        }
    );
    // And if a shell rewrote the file, sequence order is checked directly.
    let mut reordered: Vec<&str> = jsonl.lines().collect();
    reordered.swap(0, 1);
    let text = reordered
        .iter()
        .map(|l| format!("{l}\n"))
        .collect::<Vec<_>>()
        .join("");
    let sha = sha_of(&text);
    let restated = CaptureManifest {
        payload_sha256_hex: sha,
        ..manifest
    };
    assert_eq!(
        verify_capture(text, restated),
        ReplayVerdict::Failed {
            failure: ReplayFailure::SequenceOutOfOrder { line_number: 1 }
        }
    );
}

#[test]
fn both_platforms_produce_identical_documents_for_one_stream() {
    // The whole reason the envelope lives in Rust: only the build identity
    // may differ between an Android and an iOS capture of the same frames.
    let frames = [
        (batch(0.0, 6), 500u64),
        (batch(0.024, 6), 600),
        (batch(0.048, 6), 700),
    ];
    let render = |platform: &str| {
        let b = CaptureBuildIdentity {
            platform: platform.into(),
            os_version: "x".into(),
            ..build()
        };
        let r = CaptureRecorder::new("rec".into(), b, 100);
        frames
            .iter()
            .map(|(p, n)| format!("{}\n", r.on_message(p.clone(), *n)))
            .collect::<Vec<_>>()
            .join("")
    };
    assert_eq!(
        render("android"),
        render("ios"),
        "the payload document must not depend on the platform"
    );
}

#[test]
fn receive_time_never_goes_backwards_in_the_file() {
    // A shell that hands back a stale clock reading must not be able to
    // write a backwards timestamp — the verifier would reject the file.
    let r = CaptureRecorder::new("rec".into(), build(), 1_000);
    let a = r.on_message(frame(0.0), 5_000);
    let b = r.on_message(frame(0.004), 4_000); // clock went backwards
    let ta: serde_json::Value = serde_json::from_str(&a).unwrap();
    let tb: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(
        tb["receivedAtMonotonicMs"].as_u64().unwrap()
            >= ta["receivedAtMonotonicMs"].as_u64().unwrap()
    );
}

#[test]
fn filenames_and_partial_suffix_are_shared_by_both_shells() {
    assert_eq!(capture_payload_filename("abc".into()), "abc.eeg.jsonl");
    assert_eq!(capture_manifest_filename("abc".into()), "abc.manifest.json");
    assert_eq!(partial_suffix(), ".partial");
    // An in-progress file is never a published recording.
    assert_ne!(
        format!(
            "{}{}",
            capture_payload_filename("abc".into()),
            partial_suffix()
        ),
        capture_payload_filename("abc".into())
    );
}
