// M6 contract: recording lifecycle semantics, mapped line-by-line to the
// Expo-oracle acceptance baseline observed on 2026-07-28.

use neuralcompose_mobile_core::{sha256_hex, AudioLifecycle, RecordingManifest, RecordingPhase};

fn granted() -> AudioLifecycle {
    let lc = AudioLifecycle::new();
    assert!(lc.on_permission(true, 10));
    lc
}

fn record_one(lc: &AudioLifecycle, t0: u64) {
    assert!(lc.on_record_start(t0));
    assert!(lc.on_record_stop(t0 + 4000));
    assert!(lc.on_persisted(
        "rec-1".into(),
        1_753_000_000_000,
        4000,
        "m4a".into(),
        102_400,
        sha256_hex(b"fake audio bytes".to_vec()),
        t0 + 4100,
    ));
}

// permission denied → disabled record, visible explanation, no file or entry
#[test]
fn denied_permission_makes_record_unreachable_with_no_entry() {
    let lc = AudioLifecycle::new();
    assert!(lc.on_permission(false, 5));
    assert_eq!(lc.phase(), RecordingPhase::PermissionDenied);

    assert!(!lc.on_record_start(10), "record must be rejected");
    let s = lc.snapshot();
    assert_eq!(s.phase, RecordingPhase::PermissionDenied);
    assert!(s.manifests.is_empty(), "no file, no entry");
    assert!(!s.has_unfinalized_recording);
}

// permission granted → record becomes reachable
#[test]
fn granted_permission_makes_record_reachable() {
    let lc = granted();
    assert_eq!(lc.phase(), RecordingPhase::Ready);
    assert!(lc.on_record_start(20));
    assert_eq!(lc.phase(), RecordingPhase::Recording);
}

// recording → visible active state; stop → atomically persisted audio+metadata
#[test]
fn stop_persists_atomically_manifest_only_with_success() {
    let lc = granted();
    assert!(lc.on_record_start(100));
    assert!(lc.on_record_stop(4100));
    assert_eq!(lc.phase(), RecordingPhase::Persisting);
    // No manifest while persisting — atomicity.
    assert!(lc.snapshot().manifests.is_empty());

    let hash = sha256_hex(b"bytes".to_vec());
    assert!(lc.on_persisted(
        "id-1".into(),
        1_000,
        4000,
        "m4a".into(),
        999,
        hash.clone(),
        4200
    ));
    let s = lc.snapshot();
    assert_eq!(s.phase, RecordingPhase::Recorded);
    assert_eq!(s.manifests.len(), 1);
    let m = &s.manifests[0];
    assert_eq!(
        (
            m.id.as_str(),
            m.duration_ms,
            m.format.as_str(),
            m.byte_size,
            m.sha256_hex.as_str()
        ),
        ("id-1", 4000, "m4a", 999, hash.as_str())
    );
}

#[test]
fn persist_failure_yields_failed_state_and_no_manifest() {
    let lc = granted();
    assert!(lc.on_record_start(100));
    assert!(lc.on_record_stop(4100));
    assert!(lc.on_persist_failed("disk full".into(), 4200));
    match lc.phase() {
        RecordingPhase::Failed { reason } => assert_eq!(reason, "disk full"),
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(
        lc.snapshot().manifests.is_empty(),
        "failed persist must not create an entry"
    );
    // Acknowledge → Ready again.
    assert!(lc.on_failure_acknowledged(5000));
    assert_eq!(lc.phase(), RecordingPhase::Ready);
}

// navigation → entry survives (read paths never mutate)
#[test]
fn read_paths_never_mutate_and_entries_survive() {
    let lc = granted();
    record_one(&lc, 100);
    let before = lc.snapshot();
    for _ in 0..50 {
        let _ = lc.snapshot();
        let _ = lc.phase();
    }
    let after = lc.snapshot();
    assert_eq!(before.manifests, after.manifests);
    assert_eq!(before.phase, after.phase);
    assert_eq!(before.transitions.len(), after.transitions.len());
}

// play → playback starts; second action stops it
#[test]
fn play_toggles_and_second_action_stops() {
    let lc = granted();
    record_one(&lc, 100);
    assert!(lc.on_play_start(5000));
    assert_eq!(lc.phase(), RecordingPhase::Playing);
    assert!(!lc.on_play_start(5001), "already playing");
    assert!(lc.on_play_stop(6000));
    assert_eq!(lc.phase(), RecordingPhase::Recorded);
}

// background → explicit interruption/recovery behavior
#[test]
fn interruption_during_recording_recovers_to_ready_without_entry() {
    let lc = granted();
    assert!(lc.on_record_start(100));
    assert!(lc.on_interruption(2000));
    assert_eq!(lc.phase(), RecordingPhase::Interrupted);
    assert!(lc.snapshot().has_unfinalized_recording);

    assert!(lc.on_interruption_ended(3000));
    assert_eq!(lc.phase(), RecordingPhase::Ready);
    assert!(
        lc.snapshot().manifests.is_empty(),
        "interrupted take was never persisted"
    );
}

#[test]
fn interruption_during_playback_recovers_to_recorded() {
    let lc = granted();
    record_one(&lc, 100);
    assert!(lc.on_play_start(5000));
    assert!(lc.on_interruption(5500));
    assert!(lc.on_interruption_ended(6000));
    assert_eq!(lc.phase(), RecordingPhase::Recorded);
    assert_eq!(lc.snapshot().manifests.len(), 1);
}

// restart → persisted entries reload without phantom recordings
#[test]
fn restart_reloads_manifests_without_phantom_recording() {
    let saved = vec![RecordingManifest {
        id: "old-1".into(),
        created_at_ms: 1_752_000_000_000,
        duration_ms: 2500,
        format: "m4a".into(),
        byte_size: 55_555,
        sha256_hex: sha256_hex(b"persisted earlier".to_vec()),
    }];
    let lc = AudioLifecycle::with_manifests(saved.clone());
    let s = lc.snapshot();
    assert_eq!(
        s.phase,
        RecordingPhase::Idle,
        "no phantom state after restart"
    );
    assert_eq!(s.manifests, saved);
    assert!(!s.has_unfinalized_recording);
    assert!(s.transitions.is_empty(), "history starts fresh per process");
    // Recording is unreachable until permission is re-reported.
    assert!(!lc.on_record_start(10));
    assert!(lc.on_permission(true, 20));
    assert!(lc.on_record_start(30));
}

#[test]
fn illegal_transitions_are_rejected_without_state_change() {
    let lc = AudioLifecycle::new();
    assert!(!lc.on_record_start(1), "Idle: permission unknown");
    assert!(!lc.on_record_stop(1));
    assert!(!lc.on_play_start(1));
    assert!(!lc.on_interruption(1));
    assert!(!lc.on_persisted("x".into(), 0, 0, "m4a".into(), 0, "h".into(), 1));
    assert_eq!(lc.phase(), RecordingPhase::Idle);
    assert!(lc.snapshot().transitions.is_empty());

    let lc2 = granted();
    // Cannot re-report permission mid-recording.
    assert!(lc2.on_record_start(10));
    assert!(!lc2.on_permission(true, 11));
    assert_eq!(lc2.phase(), RecordingPhase::Recording);
}

#[test]
fn transition_history_is_complete_and_ordered() {
    let lc = granted();
    record_one(&lc, 100);
    assert!(lc.on_play_start(5000));
    assert!(lc.on_play_stop(6000));
    let s = lc.snapshot();
    let events: Vec<&str> = s.transitions.iter().map(|t| t.event.as_str()).collect();
    assert_eq!(
        events,
        vec![
            "permission",
            "record_start",
            "record_stop",
            "persisted",
            "play_start",
            "play_stop"
        ]
    );
    let times: Vec<u64> = s.transitions.iter().map(|t| t.at_ms).collect();
    let mut sorted = times.clone();
    sorted.sort_unstable();
    assert_eq!(times, sorted, "history is monotonically ordered");
}

#[test]
fn sha256_hex_is_deterministic_and_matches_known_vector() {
    assert_eq!(
        sha256_hex(b"abc".to_vec()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(sha256_hex(Vec::new()), sha256_hex(Vec::new()));
}
