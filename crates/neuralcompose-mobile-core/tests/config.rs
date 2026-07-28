// Port of the Expo client's src/__tests__/config.test.ts.

use neuralcompose_mobile_core::{derive_ws_url, parse_use_mock, resolve_client_mode, ClientMode};

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

#[test]
fn defaults_to_mock_when_unset_or_empty() {
    assert!(parse_use_mock(None));
    assert!(parse_use_mock(s("")));
}

#[test]
fn selects_live_only_on_explicit_falsey_string() {
    assert!(!parse_use_mock(s("false")));
    assert!(!parse_use_mock(s("FALSE")));
    assert!(!parse_use_mock(s("0")));
    assert!(!parse_use_mock(s("no")));
}

#[test]
fn affirmative_and_junk_values_stay_mock() {
    assert!(parse_use_mock(s("true")));
    assert!(parse_use_mock(s("1")));
    assert!(parse_use_mock(s("banana")));
}

#[test]
fn derive_maps_http_to_ws_and_appends_stream_path() {
    assert_eq!(
        derive_ws_url("http://host:8081".into()),
        "ws://host:8081/api/eeg/stream"
    );
    assert_eq!(
        derive_ws_url("https://host:8081".into()),
        "wss://host:8081/api/eeg/stream"
    );
}

#[test]
fn derive_returns_empty_for_non_http_inputs() {
    assert_eq!(derive_ws_url("".into()), "");
    assert_eq!(derive_ws_url("host:8081".into()), "");
}

#[test]
fn mock_mode_needs_no_endpoints_and_reports_no_error() {
    let c = resolve_client_mode(None, None, None);
    assert_eq!(c.mode, ClientMode::Mock);
    assert_eq!(c.config_error, None);
}

#[test]
fn live_mode_with_valid_server_url_resolves_endpoints() {
    let c = resolve_client_mode(s("false"), s("http://mac.example:8081/"), None);
    assert_eq!(c.mode, ClientMode::Live);
    assert_eq!(c.server_url, "http://mac.example:8081"); // trailing slash trimmed
    assert_eq!(c.eeg_ws_url, "ws://mac.example:8081/api/eeg/stream");
    assert_eq!(c.config_error, None);
}

#[test]
fn explicit_ws_url_wins_over_derivation() {
    let c = resolve_client_mode(
        s("false"),
        s("https://mac.example"),
        s("wss://mac.example/custom"),
    );
    assert_eq!(c.eeg_ws_url, "wss://mac.example/custom");
}

#[test]
fn live_with_empty_server_url_is_visible_config_error_not_mock() {
    let c = resolve_client_mode(s("false"), s(""), s(""));
    assert_eq!(c.mode, ClientMode::Live); // never silently substitutes mock
    assert_eq!(
        c.config_error.as_deref(),
        Some("EXPO_PUBLIC_SERVER_URL is not set")
    );
}

#[test]
fn live_with_malformed_server_url_is_visible_config_error_not_mock() {
    let c = resolve_client_mode(s("false"), s("not-a-url"), None);
    assert_eq!(c.mode, ClientMode::Live);
    assert_eq!(
        c.config_error.as_deref(),
        Some("invalid EXPO_PUBLIC_SERVER_URL: not-a-url")
    );
}
