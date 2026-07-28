// M5-A contract: connection-generation freshness + retry-reset semantics.
// First deliberate divergence from the Expo oracle (which carried receive
// time across reconnects — the same category of error Gate 4 exposed).
// The seven operator-required regressions, in order.

use neuralcompose_mobile_core::{
    MonitorConfig, ReconnectDecision, SocketEvent, StreamMonitor, StreamPhase,
};

const FRAME: &str = r#"{"timestamp":0.1,"channels":[1,2,3,4]}"#;
const BATCH: &str =
    r#"[{"timestamp":0.1,"channels":[1,2,3,4]},{"timestamp":0.2,"channels":[5,6,7,8]}]"#;

fn monitor() -> StreamMonitor {
    StreamMonitor::new(MonitorConfig::default())
}

/// Open generation N, deliver a frame at `t`, close at `t + 50`.
fn live_then_close(m: &StreamMonitor, t: u64) {
    m.on_socket_event(SocketEvent::Opened, t);
    assert!(m.on_frame(BATCH.to_string(), t) > 0);
    assert_eq!(m.phase(t), StreamPhase::Live);
    m.on_socket_event(SocketEvent::Closed, t + 50);
}

// 1. Previous-generation freshness cannot authorize Live.
#[test]
fn previous_generation_freshness_cannot_authorize_live() {
    let m = monitor();
    live_then_close(&m, 1000);
    // Reopen 100ms later — the prior sample is only 150ms old, which under
    // v0 semantics authorized Live. It must not.
    m.on_socket_event(SocketEvent::Opened, 1150);
    assert_eq!(m.phase(1150), StreamPhase::OpenNoData);
    // And it stays OpenNoData regardless of how fresh the OLD sample was.
    assert_eq!(m.phase(1200), StreamPhase::OpenNoData);
}

// 2. Cached samples remain available after reconnect without changing phase.
#[test]
fn cached_samples_survive_reconnect_without_authorizing_live() {
    let m = monitor();
    live_then_close(&m, 1000);
    m.on_socket_event(SocketEvent::Opened, 1150);

    let chans = m.snapshot();
    assert_eq!(
        chans.channels[0].len(),
        2,
        "cached traces still displayable"
    );
    assert_eq!(chans.received, 2, "total across generations");

    let s = m.stream_snapshot(1150);
    assert_eq!(s.cached_sample_count, 2);
    assert_eq!(s.received_on_current_connection, 0);
    assert_eq!(s.last_received_at_current_ms, None);
    assert_eq!(s.last_received_at_any_ms, Some(1000));
    assert_eq!(s.phase, StreamPhase::OpenNoData);
}

// 3. Malformed and all-invalid batches do not establish current-generation freshness.
#[test]
fn invalid_frames_do_not_establish_freshness() {
    let m = monitor();
    live_then_close(&m, 1000);
    m.on_socket_event(SocketEvent::Opened, 1150);

    assert_eq!(m.on_frame("not json{".to_string(), 1200), 0);
    assert_eq!(
        m.on_frame(r#"[{"timestamp":9,"channels":[1,2,3]}]"#.to_string(), 1250),
        0
    );
    assert_eq!(m.on_frame("[]".to_string(), 1300), 0);

    assert_eq!(m.phase(1300), StreamPhase::OpenNoData);
    let s = m.stream_snapshot(1300);
    assert_eq!(s.last_received_at_current_ms, None);
    assert_eq!(s.received_on_current_connection, 0);

    // First VALID frame flips to Live.
    assert_eq!(m.on_frame(FRAME.to_string(), 1350), 1);
    assert_eq!(m.phase(1350), StreamPhase::Live);
}

// 4. An open-immediately-close server reaches GiveUp rather than retrying forever.
#[test]
fn open_immediately_close_server_exhausts_the_budget() {
    let m = monitor();
    let mut delays = Vec::new();
    loop {
        m.on_socket_event(SocketEvent::Connecting, 0);
        m.on_socket_event(SocketEvent::Opened, 1); // handshake succeeds...
        m.on_socket_event(SocketEvent::Closed, 2); // ...then immediate close
        match m.reconnect_decision() {
            ReconnectDecision::RetryAfterMs { delay_ms } => {
                delays.push(delay_ms);
                assert!(delays.len() <= 3, "must not retry forever: {delays:?}");
            }
            ReconnectDecision::GiveUp => break,
        }
    }
    assert_eq!(delays, vec![1000, 2000, 4000]);
    assert_eq!(m.phase(10), StreamPhase::Error, "give-up latches Error");
}

// 5. The first accepted sample resets the retry budget.
#[test]
fn first_accepted_sample_resets_the_retry_budget() {
    let m = monitor();
    // Two unsuccessful generations: budget at 2.
    for t in [0u64, 100] {
        m.on_socket_event(SocketEvent::Connecting, t);
        m.on_socket_event(SocketEvent::Opened, t + 1);
        m.on_socket_event(SocketEvent::Closed, t + 2);
    }
    assert_eq!(m.stream_snapshot(200).reconnect_attempts, 2);

    // Third generation opens: budget is NOT reset by the handshake.
    m.on_socket_event(SocketEvent::Opened, 300);
    assert_eq!(m.stream_snapshot(300).reconnect_attempts, 2);

    // First accepted frame is the real recovery: budget resets to zero.
    assert_eq!(m.on_frame(FRAME.to_string(), 310), 1);
    assert_eq!(m.stream_snapshot(310).reconnect_attempts, 0);

    // A later failure starts the backoff ladder from the bottom again.
    m.on_socket_event(SocketEvent::Closed, 400);
    assert_eq!(
        m.reconnect_decision(),
        ReconnectDecision::RetryAfterMs { delay_ms: 1000 }
    );
}

// 6. Generation numbers increase monotonically across reconnects.
#[test]
fn generation_numbers_increase_monotonically() {
    let m = monitor();
    assert_eq!(
        m.stream_snapshot(0).connection_generation,
        0,
        "never opened"
    );
    let mut last = 0;
    for t in [10u64, 100, 1000, 5000] {
        m.on_socket_event(SocketEvent::Opened, t);
        let gen = m.stream_snapshot(t).connection_generation;
        assert!(gen > last, "generation must strictly increase");
        last = gen;
        m.on_socket_event(SocketEvent::Closed, t + 1);
    }
    assert_eq!(last, 4);
    // reset() starts the lifecycle over.
    m.reset();
    assert_eq!(m.stream_snapshot(6000).connection_generation, 0);
}

// 7. Clock queries and presentation calls never mutate freshness.
#[test]
fn read_paths_never_mutate_freshness() {
    let m = monitor();
    live_then_close(&m, 1000);
    m.on_socket_event(SocketEvent::Opened, 1100);
    let baseline = m.stream_snapshot(1100);

    for i in 0..100u64 {
        let now = 1100 + i * 97;
        let _ = m.phase(now);
        let _ = m.presentation(now);
        let _ = m.snapshot();
        let s = m.stream_snapshot(now);
        assert_eq!(s.connection_generation, baseline.connection_generation);
        assert_eq!(s.received_on_current_connection, 0);
        assert_eq!(s.last_received_at_current_ms, None);
        assert_eq!(s.last_received_at_any_ms, baseline.last_received_at_any_ms);
        assert_eq!(s.reconnect_attempts, baseline.reconnect_attempts);
        assert_eq!(
            s.phase,
            StreamPhase::OpenNoData,
            "no read path may conjure Live"
        );
    }
}
