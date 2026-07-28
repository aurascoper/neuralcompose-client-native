// The Gate 4 contract: the exact sequence the Expo client was validated
// against on the iOS Simulator (2026-07-28), as a pure state-machine test.
// Also pins the no-op-flush bug (retained data must never refresh sample age)
// and the reconnect give-up path with backoff exactly [1000, 2000, 4000] ms.

use neuralcompose_mobile_core::{
    MonitorConfig, ReconnectDecision, SocketEvent, StreamMonitor, StreamPhase,
};

const FRAME: &str = r#"{"timestamp":0.1,"channels":[1,2,3,4]}"#;
const BATCH: &str =
    r#"[{"timestamp":0.1,"channels":[1,2,3,4]},{"timestamp":0.2,"channels":[5,6,7,8]}]"#;

fn monitor() -> StreamMonitor {
    StreamMonitor::new(MonitorConfig::default())
}

#[test]
fn operator_specified_gate4_sequence() {
    let m = monitor();

    // socket opens, no samples yet → OpenNoData
    m.on_socket_event(SocketEvent::Connecting, 0);
    assert_eq!(m.phase(0), StreamPhase::Connecting);
    m.on_socket_event(SocketEvent::Opened, 10);
    assert_eq!(m.phase(10), StreamPhase::OpenNoData);

    // sample arrives → Live
    assert_eq!(m.on_frame(BATCH.to_string(), 100), 2);
    assert_eq!(m.phase(100), StreamPhase::Live);

    // socket remains open, age > 2s → Stale(age)
    assert_eq!(m.phase(2101), StreamPhase::Stale { age_ms: 2001 });

    // new sample arrives → Live
    assert_eq!(m.on_frame(FRAME.to_string(), 2200), 1);
    assert_eq!(m.phase(2200), StreamPhase::Live);

    // socket closes → Closed
    m.on_socket_event(SocketEvent::Closed, 2300);
    assert_eq!(m.phase(2300), StreamPhase::Closed);

    // retry begins → Connecting (delay for the 1st failed attempt is 1000ms)
    assert_eq!(
        m.reconnect_decision(),
        ReconnectDecision::RetryAfterMs { delay_ms: 1000 }
    );
    m.on_socket_event(SocketEvent::Connecting, 3300);
    assert_eq!(m.phase(3300), StreamPhase::Connecting);

    // new socket receives sample → Live
    m.on_socket_event(SocketEvent::Opened, 3400);
    assert_eq!(m.on_frame(FRAME.to_string(), 3450), 1);
    assert_eq!(m.phase(3450), StreamPhase::Live);
}

#[test]
fn no_op_flush_pin_retained_data_never_refreshes_age() {
    let m = monitor();
    m.on_socket_event(SocketEvent::Opened, 0);
    m.on_frame(BATCH.to_string(), 1000);
    let frozen = m.snapshot().last_received_at_ms;
    assert_eq!(frozen, Some(1000));

    // Poll phase/snapshot 50 times with growing now and NO new frames:
    // last_received_at_ms stays frozen and age grows monotonically.
    let mut prev_age = 0u64;
    for i in 1..=50u64 {
        let now = 1000 + i * 200;
        let snap = m.snapshot();
        assert_eq!(
            snap.last_received_at_ms, frozen,
            "snapshot() must not mutate receive state"
        );
        match m.phase(now) {
            StreamPhase::Live => assert!(now - 1000 <= 2000),
            StreamPhase::Stale { age_ms } => {
                assert_eq!(age_ms, now - 1000);
                assert!(age_ms >= prev_age, "age must grow monotonically");
                prev_age = age_ms;
            }
            other => panic!("unexpected phase {other:?}"),
        }
    }

    // An all-invalid batch is a no-op: accepted == 0 and age is NOT refreshed.
    let accepted = m.on_frame(
        r#"[{"timestamp":9,"channels":[1,2,3]}]"#.to_string(),
        99_000,
    );
    assert_eq!(accepted, 0);
    assert_eq!(m.snapshot().last_received_at_ms, frozen);
    // Empty/garbage frames likewise.
    assert_eq!(m.on_frame("not json{".to_string(), 99_500), 0);
    assert_eq!(m.snapshot().last_received_at_ms, frozen);
}

#[test]
fn give_up_after_three_failed_attempts_with_pinned_backoff() {
    let m = monitor();

    let mut delays = Vec::new();
    for attempt in 1..=3u64 {
        m.on_socket_event(SocketEvent::Connecting, attempt * 100);
        m.on_socket_event(SocketEvent::Closed, attempt * 100 + 50);
        match m.reconnect_decision() {
            ReconnectDecision::RetryAfterMs { delay_ms } => delays.push(delay_ms),
            ReconnectDecision::GiveUp => panic!("gave up too early at attempt {attempt}"),
        }
    }
    assert_eq!(delays, vec![1000, 2000, 4000]);

    // 4th failure → GiveUp, and the monitor latches Error (Expo parity:
    // onStatus('error') after exhausting retries).
    m.on_socket_event(SocketEvent::Closed, 1000);
    assert_eq!(m.reconnect_decision(), ReconnectDecision::GiveUp);
    assert_eq!(m.phase(1001), StreamPhase::Error);

    // A successful open resets everything.
    m.on_socket_event(SocketEvent::Opened, 2000);
    assert_eq!(m.phase(2000), StreamPhase::OpenNoData);
    m.on_socket_event(SocketEvent::Closed, 2100);
    assert_eq!(
        m.reconnect_decision(),
        ReconnectDecision::RetryAfterMs { delay_ms: 1000 }
    );
}

#[test]
fn non_monotonic_now_is_defended_with_saturation() {
    let m = monitor();
    m.on_socket_event(SocketEvent::Opened, 0);
    m.on_frame(FRAME.to_string(), 5000);
    // now earlier than last receive → age saturates to 0 → Live, no panic.
    assert_eq!(m.phase(4000), StreamPhase::Live);
}

#[test]
fn reset_mirrors_hook_remount() {
    let m = monitor();
    m.on_socket_event(SocketEvent::Opened, 0);
    m.on_frame(BATCH.to_string(), 10);
    m.on_socket_event(SocketEvent::Closed, 20);
    m.reset();
    assert_eq!(m.phase(30), StreamPhase::Connecting);
    let snap = m.snapshot();
    assert_eq!(snap.received, 0);
    assert_eq!(snap.last_received_at_ms, None);
    assert_eq!(snap.latest_index, -1);
    assert!(snap.channels.iter().all(|c| c.is_empty()));
}
