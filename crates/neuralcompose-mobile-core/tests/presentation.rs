// Port of the Expo client's src/hooks/__tests__/streamPresentation.test.ts.
// The English formatters must reproduce the Jest oracle strings exactly.

use neuralcompose_mobile_core::{
    format_banner_en, format_label_en, MonitorConfig, SocketEvent, StreamMonitor, StreamPhase,
    StreamTone,
};

const THRESHOLD: u64 = 2000;

fn monitor() -> StreamMonitor {
    StreamMonitor::new(MonitorConfig {
        stale_after_ms: THRESHOLD,
        ..MonitorConfig::default()
    })
}

fn open_with_sample_at(m: &StreamMonitor, t: u64) {
    m.on_socket_event(SocketEvent::Opened, t);
    let n = m.on_frame(r#"{"timestamp":0.1,"channels":[1,2,3,4]}"#.to_string(), t);
    assert_eq!(n, 1);
}

#[test]
fn fresh_open_stream_is_open_ok_no_banner() {
    let m = monitor();
    open_with_sample_at(&m, 1000);
    let p = m.presentation(1500);
    assert_eq!(p.phase, StreamPhase::Live);
    assert_eq!(p.tone, StreamTone::Ok);
    assert_eq!(format_label_en(p), "OPEN");
    assert_eq!(format_banner_en(p), None);
}

#[test]
fn threshold_is_fresh_one_past_is_stale() {
    let m = monitor();
    open_with_sample_at(&m, 0);
    assert_eq!(m.presentation(THRESHOLD).tone, StreamTone::Ok);
    let p = m.presentation(THRESHOLD + 1);
    assert_eq!(p.tone, StreamTone::Stale);
    assert_eq!(
        p.phase,
        StreamPhase::Stale {
            age_ms: THRESHOLD + 1
        }
    );
}

#[test]
fn open_but_silent_is_stale_with_explanatory_banner() {
    let m = monitor();
    open_with_sample_at(&m, 0);
    let p = m.presentation(7000);
    assert_eq!(format_label_en(p), "STALE 7s");
    assert_eq!(p.tone, StreamTone::Stale);
    assert_eq!(p.age_s, Some(7));
    let banner = format_banner_en(p).expect("banner");
    assert!(banner.contains("no samples for 7s"), "{banner}");
    assert!(banner.contains("socket still open"), "{banner}");
    assert_eq!(
        banner,
        "Stream silent — no samples for 7s (socket still open)"
    );
}

#[test]
fn open_with_no_samples_ever_is_stale_no_data_not_healthy() {
    let m = monitor();
    m.on_socket_event(SocketEvent::Opened, 0);
    let p = m.presentation(10_000);
    assert_eq!(p.phase, StreamPhase::OpenNoData);
    assert_eq!(p.tone, StreamTone::Stale);
    assert_eq!(format_label_en(p), "OPEN · NO DATA");
}

#[test]
fn connecting_has_its_own_tone_without_disconnect_banner() {
    let m = monitor();
    m.on_socket_event(SocketEvent::Connecting, 0);
    let p = m.presentation(0);
    assert_eq!(p.phase, StreamPhase::Connecting);
    assert_eq!(p.tone, StreamTone::Connecting);
    assert_eq!(format_label_en(p), "CONNECTING");
    assert_eq!(format_banner_en(p), None);
}

#[test]
fn closed_and_errored_are_down_with_cached_data_banner() {
    for (event, label) in [
        (SocketEvent::Closed, "CLOSED"),
        (SocketEvent::Errored, "ERROR"),
    ] {
        let m = monitor();
        open_with_sample_at(&m, 0);
        m.on_socket_event(event, 1000);
        let p = m.presentation(1000);
        assert_eq!(p.tone, StreamTone::Down);
        assert_eq!(format_label_en(p), label);
        assert_eq!(
            format_banner_en(p).as_deref(),
            Some("Stream disconnected — showing last cached data")
        );
    }
}

#[test]
fn never_ok_for_a_non_open_socket_regardless_of_age() {
    let m = monitor();
    open_with_sample_at(&m, 0);
    m.on_socket_event(SocketEvent::Closed, 1);
    assert_eq!(m.presentation(1).tone, StreamTone::Down);
    let m2 = monitor();
    m2.on_socket_event(SocketEvent::Connecting, 0);
    assert_eq!(m2.presentation(0).tone, StreamTone::Connecting);
}
